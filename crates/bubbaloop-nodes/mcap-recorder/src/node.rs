use bubbaloop::prelude::*;
use bubbaloop::{get_descriptor_for_message, schemas::CompressedImage};
use prost::Message;
use ros_z::{node::ZNode, Builder, Result as ZResult};
use serde::Deserialize;
use std::collections::BTreeMap;
use std::path::PathBuf;
use std::sync::Arc;
use std::time::{SystemTime, UNIX_EPOCH};
use zenoh::sample::Sample;

// ============================================================================
// Configuration
// ============================================================================

/// Configuration for the MCAP recorder node
#[derive(Debug, Clone, Deserialize)]
pub struct Config {
    /// List of topics to record
    #[serde(default)]
    pub topics: Vec<String>,

    /// Output directory for MCAP files (default: current directory)
    #[serde(default = "default_output_dir")]
    pub output_dir: PathBuf,

    /// Output filename pattern (default: timestamp-based)
    /// If not specified, uses Unix timestamp
    #[serde(default)]
    pub output_filename: Option<String>,
}

fn default_output_dir() -> PathBuf {
    PathBuf::from(".")
}

impl Default for Config {
    fn default() -> Self {
        Self {
            topics: vec!["/camera/*/compressed".to_string()],
            output_dir: default_output_dir(),
            output_filename: None,
        }
    }
}

impl Config {
    /// Get the full output path for the MCAP file
    pub fn output_path(&self) -> PathBuf {
        let filename = self.output_filename.clone().unwrap_or_else(|| {
            let timestamp = SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .unwrap()
                .as_secs();
            format!("{}.mcap", timestamp)
        });
        self.output_dir.join(filename)
    }
}

// ============================================================================
// Legacy API (for backwards compatibility)
// ============================================================================

/// Recorder node that subscribes to topics and writes to MCAP
pub struct RecorderNode {
    node: Arc<ZNode>,
    topics: Vec<String>,
    output_path: PathBuf,
}

impl RecorderNode {
    pub fn new(node: Arc<ZNode>, topics: &[String], output_path: PathBuf) -> ZResult<Self> {
        info!(
            "Recorder node initialized with {} topics to subscribe",
            topics.len()
        );

        Ok(Self {
            node,
            topics: topics.to_vec(),
            output_path,
        })
    }

    pub async fn run(self, shutdown_tx: tokio::sync::watch::Sender<()>) -> ZResult<()> {
        let mut shutdown_rx = shutdown_tx.subscribe();

        // Create MCAP writer
        let file = std::fs::File::create(&self.output_path)?;

        let mut writer = mcap::Writer::new(file)?;

        log::info!(
            "Recorder node started, recording to: {}",
            self.output_path.display()
        );

        // Channel for (topic, sample) tuples
        let (tx, rx) = flume::unbounded::<(String, Sample)>();

        // Spawn subscription tasks for each topic (focus on compressed for now)
        let tasks: Vec<_> = self
            .topics
            .into_iter()
            .map(|topic| {
                let node = self.node.clone();
                let topic = topic.clone();
                let tx = tx.clone();

                let mut shutdown_rx_task = shutdown_tx.subscribe();

                tokio::spawn(async move {
                    // Subscribe to CompressedImage using ros-z
                    let subscriber = match node
                        .create_sub::<CompressedImage>(&topic)
                        .with_serdes::<ros_z::msg::ProtobufSerdes<CompressedImage>>()
                        .build()
                    {
                        Ok(s) => s,
                        Err(e) => {
                            log::error!("Failed to subscribe to topic '{}': {}", topic, e);
                            return;
                        }
                    };

                    loop {
                        tokio::select! {
                            _ = shutdown_rx_task.changed() => break,
                            Ok(sample) = subscriber.async_recv_serialized() => {
                                if let Err(e) = tx.send((topic.clone(), sample)) {
                                    log::error!("Failed to send message to channel: {}", e);
                                    break;
                                }
                            }
                        }
                    }

                    log::info!("Subscription task for topic '{}' shutting down", topic);
                })
            })
            .collect();

        // MCAP writing task
        loop {
            tokio::select! {
                Ok((topic, sample)) = rx.recv_async() => {
                    log::info!("Received sample for topic '{}'", topic);
                    // Get protobuf descriptor and schema name from bubbaloop crate
                    let descriptor = get_descriptor_for_message::<CompressedImage>()?;
                    let schema_id = writer.add_schema(&descriptor.schema_name, "protobuf", &descriptor.descriptor_bytes)?;
                    let channel_id = writer.add_channel(schema_id as u16, &topic, "protobuf", &BTreeMap::new())?;

                    // decode the sample to get the header
                    let msg = CompressedImage::decode(sample.payload().to_bytes().as_ref())?;
                    let msg_header = mcap::records::MessageHeader {
                        channel_id,
                        sequence: msg.header.as_ref().unwrap().sequence,
                        log_time: msg.header.as_ref().unwrap().acq_time,
                        publish_time: msg.header.as_ref().unwrap().pub_time,
                    };

                    writer.write_to_known_channel(&msg_header, sample.payload().to_bytes().as_ref())?;
                }
                _ = shutdown_rx.changed() => break,
            }
        }

        // Wait for all subscription tasks to complete
        futures::future::join_all(tasks).await;

        // Finish MCAP file
        writer.finish()?;

        Ok(())
    }
}

// ============================================================================
// BubbleNode Implementation
// ============================================================================

/// MCAP recorder node implementing the BubbleNode trait
///
/// Records topics to MCAP format files for later playback and analysis.
pub struct McapRecorderNode {
    ctx: Arc<ZContext>,
    config: Config,
}

#[async_trait]
impl BubbleNode for McapRecorderNode {
    type Config = Config;

    fn metadata() -> BubbleMetadata {
        bubble_metadata!(
            topics_published: &[],
            topics_subscribed: &["/camera/*/compressed"],
        )
    }

    fn new(ctx: Arc<ZContext>, config: Self::Config) -> Result<Self, NodeError> {
        info!(
            "Initializing MCAP recorder with {} topics",
            config.topics.len()
        );
        Ok(Self { ctx, config })
    }

    async fn run(self, mut shutdown: watch::Receiver<()>) -> Result<(), NodeError> {
        let output_path = self.config.output_path();

        // Create output directory if needed
        if let Some(parent) = output_path.parent() {
            std::fs::create_dir_all(parent)?;
        }

        // Create ros-z node
        let node = Arc::new(
            self.ctx
                .create_node("mcap_recorder")
                .build()
                .map_err(|e| NodeError::Zenoh(e.to_string()))?,
        );

        // Create shutdown channel for internal tasks
        let (internal_shutdown_tx, _) = tokio::sync::watch::channel(());

        // Create MCAP writer
        let file = std::fs::File::create(&output_path)?;
        let mut writer = mcap::Writer::new(file)
            .map_err(|e| NodeError::Runtime(e.to_string()))?;

        info!("Recording to: {}", output_path.display());

        // Channel for (topic, sample) tuples
        let (tx, rx) = flume::unbounded::<(String, Sample)>();

        // Spawn subscription tasks for each topic
        let tasks: Vec<_> = self
            .config
            .topics
            .iter()
            .cloned()
            .map(|topic| {
                let node = node.clone();
                let tx = tx.clone();
                let mut shutdown_rx_task = internal_shutdown_tx.subscribe();

                tokio::spawn(async move {
                    // Subscribe to CompressedImage using ros-z
                    let subscriber = match node
                        .create_sub::<CompressedImage>(&topic)
                        .with_serdes::<ros_z::msg::ProtobufSerdes<CompressedImage>>()
                        .build()
                    {
                        Ok(s) => s,
                        Err(e) => {
                            error!("Failed to subscribe to topic '{}': {}", topic, e);
                            return;
                        }
                    };

                    loop {
                        tokio::select! {
                            _ = shutdown_rx_task.changed() => break,
                            Ok(sample) = subscriber.async_recv_serialized() => {
                                if let Err(e) = tx.send((topic.clone(), sample)) {
                                    error!("Failed to send message to channel: {}", e);
                                    break;
                                }
                            }
                        }
                    }

                    info!("Subscription task for topic '{}' shutting down", topic);
                })
            })
            .collect();

        // MCAP writing loop
        loop {
            tokio::select! {
                biased;

                _ = shutdown.changed() => {
                    info!("Recorder received shutdown signal");
                    break;
                }

                Ok((topic, sample)) = rx.recv_async() => {
                    debug!("Received sample for topic '{}'", topic);

                    // Get protobuf descriptor and schema name
                    let descriptor = get_descriptor_for_message::<CompressedImage>()
                        .map_err(|e| NodeError::Runtime(e.to_string()))?;
                    let schema_id = writer
                        .add_schema(&descriptor.schema_name, "protobuf", &descriptor.descriptor_bytes)
                        .map_err(|e| NodeError::Runtime(e.to_string()))?;
                    let channel_id = writer
                        .add_channel(schema_id as u16, &topic, "protobuf", &BTreeMap::new())
                        .map_err(|e| NodeError::Runtime(e.to_string()))?;

                    // Decode the sample to get the header
                    let msg = CompressedImage::decode(sample.payload().to_bytes().as_ref())
                        .map_err(|e| NodeError::Runtime(e.to_string()))?;
                    let msg_header = mcap::records::MessageHeader {
                        channel_id,
                        sequence: msg.header.as_ref().unwrap().sequence,
                        log_time: msg.header.as_ref().unwrap().acq_time,
                        publish_time: msg.header.as_ref().unwrap().pub_time,
                    };

                    writer
                        .write_to_known_channel(&msg_header, sample.payload().to_bytes().as_ref())
                        .map_err(|e| NodeError::Runtime(e.to_string()))?;
                }
            }
        }

        // Signal internal tasks to shutdown
        let _ = internal_shutdown_tx.send(());

        // Wait for all subscription tasks to complete
        futures::future::join_all(tasks).await;

        // Finish MCAP file
        writer.finish().map_err(|e| NodeError::Runtime(e.to_string()))?;

        info!("Recording complete: {}", output_path.display());
        Ok(())
    }
}
