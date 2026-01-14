use bubbaloop::{
    get_descriptor_for_message,
    schemas::CompressedImage,
    services::recording::{
        schemas::{
            StartRecordingRequest, StartRecordingResponse, StopRecordingRequest,
            StopRecordingResponse,
        },
        StartRecording, StopRecording,
    },
};
use prost::Message;
use ros_z::{msg::ZMessage, node::ZNode, Builder, Result as ZResult};
use std::collections::BTreeMap;
use std::path::PathBuf;
use std::sync::Arc;
use std::time::{SystemTime, UNIX_EPOCH};
use tokio::task::JoinHandle;
use zenoh::{bytes::ZBytes, query::Query, sample::Sample};

/// Recording state - owns the writer when recording
enum RecorderState {
    Idle,
    Recording {
        writer: mcap::Writer<std::fs::File>,
        file_path: String,
    },
}

/// Commands sent from service tasks to the main loop
enum RecorderCommand {
    Start {
        topics: Vec<String>,
        reply: flume::Sender<StartRecordingResponse>,
    },
    Stop {
        reply: flume::Sender<StopRecordingResponse>,
    },
}

/// Recorder node that subscribes to topics and writes to MCAP
pub struct RecorderNode {
    node: Arc<ZNode>,
    output_dir: PathBuf,
}

impl RecorderNode {
    pub fn new(node: Arc<ZNode>, output_dir: PathBuf) -> ZResult<Self> {
        log::info!(
            "Recorder node initialized, output dir: {}",
            output_dir.display()
        );

        Ok(Self { node, output_dir })
    }

    fn generate_output_path(&self) -> String {
        let timestamp = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_secs();
        self.output_dir
            .join(format!("{}.mcap", timestamp))
            .to_string_lossy()
            .into()
    }

    pub async fn run(self, shutdown_tx: tokio::sync::watch::Sender<()>) -> ZResult<()> {
        let mut shutdown_rx = shutdown_tx.subscribe();

        // State owned locally - no locking needed
        let mut state = RecorderState::Idle;

        // Command channel - services send commands here
        let (cmd_tx, cmd_rx) = flume::unbounded::<RecorderCommand>();

        // Sample channel - subscriptions send samples here
        let (sample_tx, sample_rx) = flume::unbounded::<(String, Sample)>();

        // Track active subscription tasks (created on Start, aborted on Stop)
        let mut subscription_handles: Vec<JoinHandle<()>> = Vec::new();
        let mut messages_recorded: u64 = 0;

        // Spawn service tasks
        let start_service_handle = Self::spawn_start_service_task(
            self.node.clone(),
            cmd_tx.clone(),
            shutdown_tx.subscribe(),
        );
        let stop_service_handle =
            Self::spawn_stop_service_task(self.node.clone(), cmd_tx, shutdown_tx.subscribe());

        log::info!("Recorder node started, waiting for commands...");

        loop {
            tokio::select! {
                // Handle service commands
                Ok(cmd) = cmd_rx.recv_async() => {
                    match cmd {
                        RecorderCommand::Start { topics, reply } => {
                            // Check not already recording
                            if matches!(state, RecorderState::Recording { .. }) {
                                let _ = reply.send(StartRecordingResponse {
                                    success: false,
                                    message: "Already recording".into(),
                                    file_path: String::new(),
                                });
                                continue;
                            }

                            // Generate output path from server's configured directory
                            let path = self.generate_output_path();

                            // Create writer
                            let file = match std::fs::File::create(&path) {
                                Ok(f) => f,
                                Err(e) => {
                                    let _ = reply.send(StartRecordingResponse {
                                        success: false,
                                        message: format!("Failed to create file: {}", e),
                                        file_path: String::new(),
                                    });
                                    continue;
                                }
                            };

                            let writer = match mcap::Writer::new(file) {
                                Ok(w) => w,
                                Err(e) => {
                                    let _ = reply.send(StartRecordingResponse {
                                        success: false,
                                        message: format!("Failed to create MCAP writer: {}", e),
                                        file_path: String::new(),
                                    });
                                    continue;
                                }
                            };

                            // Spawn subscriptions for requested topics
                            for topic in &topics {
                                let handle = Self::spawn_subscription_task(
                                    self.node.clone(),
                                    topic.clone(),
                                    sample_tx.clone(),
                                    shutdown_tx.subscribe(),
                                );
                                subscription_handles.push(handle);
                            }

                            log::info!("Recording started: {} topics -> {}", topics.len(), path);

                            messages_recorded = 0;
                            state = RecorderState::Recording {
                                writer,
                                file_path: path.clone(),
                            };
                            let _ = reply.send(StartRecordingResponse {
                                success: true,
                                message: format!("Recording {} topics", topics.len()),
                                file_path: path,
                            });
                        }

                        RecorderCommand::Stop { reply } => {
                            // Take ownership of writer via std::mem::replace
                            let old_state = std::mem::replace(&mut state, RecorderState::Idle);

                            if let RecorderState::Recording { mut writer, file_path } = old_state {
                                // Abort subscription tasks
                                for handle in subscription_handles.drain(..) {
                                    handle.abort();
                                }
                                // Drain remaining samples
                                while sample_rx.try_recv().is_ok() {}

                                // Finish writer
                                if let Err(e) = writer.finish() {
                                    log::error!("Failed to finish MCAP writer: {}", e);
                                }

                                log::info!("Recording stopped: {} messages -> {}", messages_recorded, file_path);

                                let _ = reply.send(StopRecordingResponse {
                                    success: true,
                                    message: "Recording stopped".into(),
                                    file_path,
                                    messages_recorded,
                                });
                            } else {
                                let _ = reply.send(StopRecordingResponse {
                                    success: false,
                                    message: "Not recording".into(),
                                    file_path: String::new(),
                                    messages_recorded: 0,
                                });
                            }
                        }
                    }
                }

                // Handle topic samples (only write if Recording)
                Ok((topic, sample)) = sample_rx.recv_async() => {
                    if let RecorderState::Recording { ref mut writer, .. } = state {
                        // Get protobuf descriptor and schema name
                        match Self::write_sample(writer, &topic, &sample) {
                            Ok(_) => {
                                messages_recorded += 1;
                                if messages_recorded % 100 == 0 {
                                    log::debug!("Recorded {} messages", messages_recorded);
                                }
                            }
                            Err(e) => {
                                log::error!("Failed to write sample for '{}': {}", topic, e);
                            }
                        }
                    }
                    // If Idle, samples are simply dropped (shouldn't happen - no subs active)
                }

                _ = shutdown_rx.changed() => {
                    log::info!("Recorder node received shutdown signal");
                    break;
                }
            }
        }

        // Cleanup on shutdown
        if let RecorderState::Recording {
            mut writer,
            file_path,
        } = state
        {
            log::info!("Finishing recording on shutdown: {}", file_path);
            for handle in subscription_handles {
                handle.abort();
            }
            if let Err(e) = writer.finish() {
                log::error!("Failed to finish MCAP writer on shutdown: {}", e);
            }
        }

        // Wait for service tasks to complete
        start_service_handle.abort();
        stop_service_handle.abort();

        Ok(())
    }

    fn write_sample(
        writer: &mut mcap::Writer<std::fs::File>,
        topic: &str,
        sample: &Sample,
    ) -> ZResult<()> {
        // Get protobuf descriptor and schema name from bubbaloop crate
        let descriptor = get_descriptor_for_message::<CompressedImage>()?;
        let schema_id = writer.add_schema(
            &descriptor.schema_name,
            "protobuf",
            &descriptor.descriptor_bytes,
        )?;
        let channel_id =
            writer.add_channel(schema_id as u16, topic, "protobuf", &BTreeMap::new())?;

        // Decode the sample to get the header
        let msg = CompressedImage::decode(sample.payload().to_bytes().as_ref())?;
        let msg_header = mcap::records::MessageHeader {
            channel_id,
            sequence: msg.header.as_ref().unwrap().sequence,
            log_time: msg.header.as_ref().unwrap().acq_time,
            publish_time: msg.header.as_ref().unwrap().pub_time,
        };

        writer.write_to_known_channel(&msg_header, sample.payload().to_bytes().as_ref())?;

        Ok(())
    }

    fn spawn_subscription_task(
        node: Arc<ZNode>,
        topic: String,
        sample_tx: flume::Sender<(String, Sample)>,
        mut shutdown_rx: tokio::sync::watch::Receiver<()>,
    ) -> JoinHandle<()> {
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

            log::info!("Subscribed to topic '{}'", topic);

            loop {
                tokio::select! {
                    _ = shutdown_rx.changed() => break,
                    Ok(sample) = subscriber.async_recv_serialized() => {
                        if let Err(e) = sample_tx.send((topic.clone(), sample)) {
                            log::error!("Failed to send message to channel: {}", e);
                            break;
                        }
                    }
                }
            }

            log::info!("Subscription task for topic '{}' shutting down", topic);
        })
    }

    fn spawn_start_service_task(
        node: Arc<ZNode>,
        cmd_tx: flume::Sender<RecorderCommand>,
        mut shutdown_rx: tokio::sync::watch::Receiver<()>,
    ) -> JoinHandle<()> {
        tokio::spawn(async move {
            let cmd_tx_clone = cmd_tx.clone();
            let _server = match node
                .create_service::<StartRecording>("recorder/start")
                .build_with_callback(move |query: Query| {
                    let cmd_tx = cmd_tx_clone.clone();
                    // Spawn async task to handle request-response cycle
                    tokio::spawn(async move {
                        if let Err(e) = Self::handle_start_request(query, cmd_tx).await {
                            log::error!("Failed to handle start request: {}", e);
                        }
                    });
                }) {
                Ok(s) => s,
                Err(e) => {
                    log::error!("Failed to create start service: {}", e);
                    return;
                }
            };

            log::info!("Start recording service ready on 'recorder/start'");

            // Keep the task alive until shutdown (server stays alive via _server)
            let _ = shutdown_rx.changed().await;

            log::info!("Start service task shutting down");
        })
    }

    async fn handle_start_request(
        query: Query,
        cmd_tx: flume::Sender<RecorderCommand>,
    ) -> ZResult<()> {
        // Deserialize request
        let payload = query
            .payload()
            .ok_or_else(|| zenoh::Error::from("No payload in query"))?;
        let request = <StartRecordingRequest as ZMessage>::deserialize(&payload.to_bytes())
            .map_err(|e| zenoh::Error::from(e.to_string()))?;

        log::info!("Received start request: {} topics", request.topics.len());

        // Send command and wait for response
        let (reply_tx, reply_rx) = flume::bounded(1);
        cmd_tx
            .send_async(RecorderCommand::Start {
                topics: request.topics,
                reply: reply_tx,
            })
            .await
            .map_err(|e| zenoh::Error::from(e.to_string()))?;

        let response = reply_rx
            .recv_async()
            .await
            .map_err(|e| zenoh::Error::from(e.to_string()))?;

        // Send response
        query
            .reply(query.key_expr(), ZBytes::from(response.serialize()))
            .await?;

        Ok(())
    }

    fn spawn_stop_service_task(
        node: Arc<ZNode>,
        cmd_tx: flume::Sender<RecorderCommand>,
        mut shutdown_rx: tokio::sync::watch::Receiver<()>,
    ) -> JoinHandle<()> {
        tokio::spawn(async move {
            let cmd_tx_clone = cmd_tx.clone();
            let _server = match node
                .create_service::<StopRecording>("recorder/stop")
                .build_with_callback(move |query: Query| {
                    let cmd_tx = cmd_tx_clone.clone();
                    // Spawn async task to handle request-response cycle
                    tokio::spawn(async move {
                        if let Err(e) = Self::handle_stop_request(query, cmd_tx).await {
                            log::error!("Failed to handle stop request: {}", e);
                        }
                    });
                }) {
                Ok(s) => s,
                Err(e) => {
                    log::error!("Failed to create stop service: {}", e);
                    return;
                }
            };

            log::info!("Stop recording service ready on 'recorder/stop'");

            // Keep the task alive until shutdown (server stays alive via _server)
            let _ = shutdown_rx.changed().await;

            log::info!("Stop service task shutting down");
        })
    }

    async fn handle_stop_request(
        query: Query,
        cmd_tx: flume::Sender<RecorderCommand>,
    ) -> ZResult<()> {
        // StopRecordingRequest has no fields, but we still deserialize for consistency
        if let Some(payload) = query.payload() {
            let _request = <StopRecordingRequest as ZMessage>::deserialize(&payload.to_bytes())
                .map_err(|e| zenoh::Error::from(e.to_string()))?;
        }

        log::info!("Received stop request");

        // Send command and wait for response
        let (reply_tx, reply_rx) = flume::bounded(1);
        cmd_tx
            .send_async(RecorderCommand::Stop { reply: reply_tx })
            .await
            .map_err(|e| zenoh::Error::from(e.to_string()))?;

        let response = reply_rx
            .recv_async()
            .await
            .map_err(|e| zenoh::Error::from(e.to_string()))?;

        // Send response
        query
            .reply(query.key_expr(), ZBytes::from(response.serialize()))
            .await?;

        Ok(())
    }
}
