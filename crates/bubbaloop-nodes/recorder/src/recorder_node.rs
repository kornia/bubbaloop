use bubbaloop_schemas::{get_descriptor_for_message, CompressedImage};
use prost::Message;
use ros_z::{node::ZNode, Builder, Result as ZResult};
use std::collections::BTreeMap;
use std::path::PathBuf;
use std::sync::Arc;
use zenoh::sample::Sample;

/// Recorder node that subscribes to topics and writes to MCAP
pub struct RecorderNode {
    node: Arc<ZNode>,
    topics: Vec<String>,
    output_path: PathBuf,
}

impl RecorderNode {
    pub fn new(node: Arc<ZNode>, topics: &[String], output_path: PathBuf) -> ZResult<Self> {
        log::info!(
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
