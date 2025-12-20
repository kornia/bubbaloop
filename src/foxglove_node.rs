use crate::config::CameraConfig;
use crate::protos::CompressedImage;
use foxglove::{
    schemas::{CompressedVideo, Timestamp},
    WebSocketServer,
};
use ros_z::{
    context::ZContext, msg::ProtobufSerdes, node::ZNode, pubsub::ZSub, Builder, Result as ZResult,
};
use std::sync::Arc;
use zenoh::sample::Sample;

/// Foxglove bridge node that subscribes to all camera topics and bridges to WebSocket
pub struct FoxgloveNode {
    #[allow(dead_code)]
    node: ZNode,
    camera_names: Vec<String>,
    ctx: Arc<ZContext>,
}

impl FoxgloveNode {
    /// Create a new Foxglove bridge node for multiple cameras
    pub fn new(ctx: Arc<ZContext>, cameras: &[CameraConfig]) -> ZResult<Self> {
        // Create ROS-Z node
        let node = ctx.create_node("foxglove_node").build()?;

        let camera_names: Vec<String> = cameras.iter().map(|c| c.name.clone()).collect();

        log::info!(
            "Created Foxglove node for {} cameras: {:?}",
            cameras.len(),
            camera_names
        );

        Ok(Self {
            node,
            camera_names,
            ctx,
        })
    }

    /// Run the Foxglove bridge
    pub async fn run(self, shutdown_tx: tokio::sync::watch::Sender<()>) -> ZResult<()> {
        let mut shutdown_rx = shutdown_tx.subscribe();

        log::info!(
            "Foxglove node started, bridging {} cameras",
            self.camera_names.len()
        );

        // Start WebSocket server
        let server = match WebSocketServer::new().start().await {
            Ok(s) => s,
            Err(e) => {
                log::error!("Failed to start Foxglove WebSocket server: {}", e);
                return Ok(());
            }
        };

        log::info!("Foxglove WebSocket server started on port 8765");

        // Spawn a task for each camera subscriber
        let mut handles = Vec::new();

        for camera_name in &self.camera_names {
            let topic = format!("/camera/{}/compressed", camera_name);
            let camera_name_clone = camera_name.clone();
            let ctx = self.ctx.clone();
            let shutdown_tx_clone = shutdown_tx.clone();

            handles.push(tokio::spawn(async move {
                if let Err(e) = run_camera_subscriber(ctx, &topic, &camera_name_clone, shutdown_tx_clone).await {
                    log::error!("Camera subscriber '{}' error: {}", camera_name_clone, e);
                }
            }));
        }

        // Wait for shutdown signal
        let _ = shutdown_rx.changed().await;

        log::info!("Shutting down Foxglove node...");

        // Cancel all subscriber tasks
        for handle in handles {
            handle.abort();
        }

        server.stop().wait().await;

        Ok(())
    }
}

/// Run a subscriber for a single camera
async fn run_camera_subscriber(
    ctx: Arc<ZContext>,
    topic: &str,
    camera_name: &str,
    shutdown_tx: tokio::sync::watch::Sender<()>,
) -> ZResult<()> {
    let mut shutdown_rx = shutdown_tx.subscribe();

    // Create a temporary node for the subscriber
    let node = ctx.create_node(&format!("foxglove_sub_{}", camera_name)).build()?;

    // Create subscriber
    let subscriber: ZSub<CompressedImage, Sample, ProtobufSerdes<CompressedImage>> = node
        .create_sub::<CompressedImage>(topic)
        .with_serdes::<ProtobufSerdes<CompressedImage>>()
        .build()?;

    // Create Foxglove channel for CompressedVideo (better for H264 streams)
    let channel = foxglove::Channel::<CompressedVideo>::new(topic);

    log::info!("Foxglove subscriber for camera '{}' started on topic '{}'", camera_name, topic);

    loop {
        tokio::select! {
            _ = shutdown_rx.changed() => {
                break;
            }
            Ok(msg) = subscriber.async_recv() => {
                let foxglove_msg = convert_to_compressed_video(&msg);
                channel.log(&foxglove_msg);
            }
        }
    }

    log::info!("Foxglove subscriber for camera '{}' stopped", camera_name);

    Ok(())
}

/// Convert our CompressedImage to Foxglove's CompressedVideo format
fn convert_to_compressed_video(msg: &CompressedImage) -> CompressedVideo {
    CompressedVideo {
        timestamp: msg.header.as_ref().map(|h| {
            Timestamp::new(
                (h.pub_time / 1_000_000_000) as u32,
                (h.pub_time % 1_000_000_000) as u32,
            )
        }),
        frame_id: msg
            .header
            .as_ref()
            .map(|h| h.frame_id.clone())
            .unwrap_or_else(|| "camera".to_string()),
        format: msg.format.clone(),
        data: msg.data.clone().into(),
    }
}
