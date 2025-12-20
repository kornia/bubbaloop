use crate::config::CameraConfig;
use crate::h264_capture::{H264Frame, H264StreamCapture};
use crate::protos::{CompressedImage, Header};
use ros_z::{
    context::ZContext, msg::ProtobufSerdes, node::ZNode, pubsub::ZPub, Builder, Result as ZResult,
};
use std::sync::Arc;

/// Get current publication timestamp in nanoseconds
fn get_pub_time() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_nanos() as u64)
        .unwrap_or(0)
}

/// Convert an H264 frame to a CompressedImage message
fn frame_to_compressed_image(frame: &H264Frame, camera_name: &str) -> CompressedImage {
    CompressedImage {
        header: Some(Header {
            acq_time: frame.pts,
            pub_time: get_pub_time(),
            sequence: frame.sequence,
            frame_id: camera_name.to_string(),
        }),
        format: "h264".to_string(),
        // Copy from GstBuffer to Vec<u8> for protobuf serialization
        data: frame.as_slice().to_vec(),
    }
}

/// RTSP Camera node that captures H264 frames and publishes them via ros-z
pub struct RtspCameraNode {
    #[allow(dead_code)]
    node: ZNode,
    publisher: ZPub<CompressedImage, ProtobufSerdes<CompressedImage>>,
    camera_config: CameraConfig,
}

impl RtspCameraNode {
    /// Create a new RTSP camera node
    pub fn new(ctx: Arc<ZContext>, camera_config: CameraConfig) -> ZResult<Self> {
        // Create ROS-Z node
        let node_name = format!("camera_{}_node", camera_config.name);
        let node = ctx.create_node(&node_name).build()?;

        // Create publisher with protobuf serialization
        let topic = format!("/camera/{}/compressed", camera_config.name);
        let publisher = node
            .create_pub::<CompressedImage>(&topic)
            .with_serdes::<ProtobufSerdes<CompressedImage>>()
            .build()?;

        log::info!(
            "Created RTSP camera node '{}' publishing to '{}'",
            camera_config.name,
            topic
        );

        Ok(Self {
            node,
            publisher,
            camera_config,
        })
    }

    /// Run the camera capture and publishing loop
    pub async fn run(self, shutdown_tx: tokio::sync::watch::Sender<()>) -> ZResult<()> {
        let mut shutdown_rx = shutdown_tx.subscribe();

        // Initialize H264 capture
        let capture =
            match H264StreamCapture::new(&self.camera_config.url, self.camera_config.latency) {
                Ok(c) => c,
                Err(e) => {
                    log::error!(
                        "Failed to create H264 capture for camera '{}': {}",
                        self.camera_config.name,
                        e
                    );
                    return Ok(());
                }
            };

        // Start the capture pipeline
        if let Err(e) = capture.start() {
            log::error!(
                "Failed to start capture for camera '{}': {}",
                self.camera_config.name,
                e
            );
            return Ok(());
        }

        log::info!(
            "Camera '{}' started capturing from {}",
            self.camera_config.name,
            self.camera_config.url
        );

        // Main publishing loop - use receiver directly in tokio select
        loop {
            tokio::select! {
                _ = shutdown_rx.changed() => {
                    break;
                }
                Ok(frame) = capture.receiver().recv_async() => {
                    let msg = frame_to_compressed_image(&frame, &self.camera_config.name);

                    if let Err(e) = self.publisher.async_publish(&msg).await {
                        log::error!(
                            "Error publishing frame from camera '{}': {}",
                            self.camera_config.name,
                            e
                        );
                    }
                }
            }
        }

        log::info!("Shutting down camera node '{}'...", self.camera_config.name);

        // Close capture (GStreamer pipeline cleanup)
        if let Err(e) = capture.close() {
            log::error!(
                "Error closing capture for camera '{}': {}",
                self.camera_config.name,
                e
            );
        }

        Ok(())
    }
}
