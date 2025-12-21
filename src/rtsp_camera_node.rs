use crate::config::CameraConfig;
use crate::h264_capture::{H264Frame, H264StreamCapture};
use crate::h264_decode::{RawFrame, VideoH264Decoder};
use crate::protos::{CompressedImage, Header, RawImage};
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

/// Convert a decoded raw frame to a RawImage message
fn frame_to_raw_image(frame: &RawFrame, camera_name: &str) -> RawImage {
    RawImage {
        header: Some(Header {
            acq_time: frame.pts,
            pub_time: get_pub_time(),
            sequence: frame.sequence,
            frame_id: camera_name.to_string(),
        }),
        width: frame.width,
        height: frame.height,
        encoding: frame.format.clone(),
        step: frame.step,
        data: frame.data.clone(),
    }
}

/// RTSP Camera node that captures H264 frames and publishes them via ros-z
///
/// Optionally decodes H264 to raw RGB frames and publishes to a separate topic.
pub struct RtspCameraNode {
    #[allow(dead_code)]
    node: ZNode,
    compressed_publisher: ZPub<CompressedImage, ProtobufSerdes<CompressedImage>>,
    raw_publisher: Option<ZPub<RawImage, ProtobufSerdes<RawImage>>>,
    decoder: Option<VideoH264Decoder>,
    camera_config: CameraConfig,
}

impl RtspCameraNode {
    /// Create a new RTSP camera node
    pub fn new(ctx: Arc<ZContext>, camera_config: CameraConfig) -> ZResult<Self> {
        // Create ROS-Z node
        let node_name = format!("camera_{}_node", camera_config.name);
        let node = ctx.create_node(&node_name).build()?;

        // Create compressed publisher
        let compressed_topic = format!("/camera/{}/compressed", camera_config.name);
        let compressed_publisher = node
            .create_pub::<CompressedImage>(&compressed_topic)
            .with_serdes::<ProtobufSerdes<CompressedImage>>()
            .build()?;

        log::info!(
            "Created RTSP camera node '{}' publishing compressed to '{}'",
            camera_config.name,
            compressed_topic
        );

        // Create raw publisher and decoder if enabled
        let (raw_publisher, decoder) = if camera_config.raw.enabled {
            let raw_topic = format!("/camera/{}/raw", camera_config.name);
            let raw_pub = node
                .create_pub::<RawImage>(&raw_topic)
                .with_serdes::<ProtobufSerdes<RawImage>>()
                .build()?;

            let backend = camera_config.raw.decoder.into();
            let dec = match VideoH264Decoder::new(backend) {
                Ok(d) => d,
                Err(e) => {
                    log::error!(
                        "Failed to create H264 decoder for camera '{}': {}",
                        camera_config.name,
                        e
                    );
                    // Return Ok but without decoder - raw publishing will be disabled
                    return Ok(Self {
                        node,
                        compressed_publisher,
                        raw_publisher: None,
                        decoder: None,
                        camera_config,
                    });
                }
            };

            log::info!(
                "Camera '{}' raw publishing enabled to '{}' with {:?} decoder",
                camera_config.name,
                raw_topic,
                camera_config.raw.decoder
            );

            (Some(raw_pub), Some(dec))
        } else {
            (None, None)
        };

        Ok(Self {
            node,
            compressed_publisher,
            raw_publisher,
            decoder,
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

        // Main publishing loop
        loop {
            tokio::select! {
                _ = shutdown_rx.changed() => {
                    break;
                }
                // Handle incoming H264 frames from capture
                Ok(frame) = capture.receiver().recv_async() => {
                    // Always publish compressed
                    let msg = frame_to_compressed_image(&frame, &self.camera_config.name);
                    if let Err(e) = self.compressed_publisher.async_publish(&msg).await {
                        log::error!(
                            "Error publishing compressed frame from camera '{}': {}",
                            self.camera_config.name,
                            e
                        );
                    }

                    // Push to decoder if enabled
                    if let Some(ref decoder) = self.decoder {
                        if let Err(e) = decoder.push(frame.as_slice(), frame.pts, frame.keyframe) {
                            log::error!(
                                "Error pushing frame to decoder for camera '{}': {}",
                                self.camera_config.name,
                                e
                            );
                        }
                    }
                }
                // Handle decoded raw frames
                Ok(mut raw_frame) = async {
                    match &self.decoder {
                        Some(decoder) => decoder.receiver().recv_async().await,
                        None => std::future::pending().await,
                    }
                } => {
                    if let Some(ref raw_publisher) = self.raw_publisher {
                        if let Some(ref decoder) = self.decoder {
                            raw_frame.sequence = decoder.next_sequence();
                        }
                        let msg = frame_to_raw_image(&raw_frame, &self.camera_config.name);
                        if let Err(e) = raw_publisher.async_publish(&msg).await {
                            log::error!(
                                "Error publishing raw frame from camera '{}': {}",
                                self.camera_config.name,
                                e
                            );
                        }
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
