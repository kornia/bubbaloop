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

/// Input for the decoder thread
struct DecoderInput {
    data: Vec<u8>,
    pts: u64,
    keyframe: bool,
}

/// RTSP Camera node that captures H264 frames and publishes them via ros-z
pub struct RtspCameraNode {
    #[allow(dead_code)]
    node: ZNode,
    compressed_publisher: ZPub<CompressedImage, ProtobufSerdes<CompressedImage>>,
    raw_publisher: Option<ZPub<RawImage, ProtobufSerdes<RawImage>>>,
    /// Send H264 frames to decoder thread (non-blocking)
    decoder_tx: Option<flume::Sender<DecoderInput>>,
    /// Receive decoded frames from decoder thread
    decoded_rx: Option<flume::Receiver<RawFrame>>,
    camera_config: CameraConfig,
}

impl RtspCameraNode {
    /// Create a new RTSP camera node
    pub fn new(ctx: Arc<ZContext>, camera_config: CameraConfig) -> ZResult<Self> {
        let node_name = format!("camera_{}_node", camera_config.name);
        let node = ctx.create_node(&node_name).build()?;

        // Create compressed publisher
        let compressed_topic = format!("/camera/{}/compressed", camera_config.name);
        let compressed_publisher = node
            .create_pub::<CompressedImage>(&compressed_topic)
            .with_serdes::<ProtobufSerdes<CompressedImage>>()
            .build()?;

        log::info!(
            "Created RTSP camera node '{}' publishing to '{}'",
            camera_config.name,
            compressed_topic
        );

        // Create raw publisher and decoder thread if enabled
        let (raw_publisher, decoder_tx, decoded_rx) = if camera_config.raw.enabled {
            let raw_topic = format!("/camera/{}/raw", camera_config.name);
            let raw_pub = node
                .create_pub::<RawImage>(&raw_topic)
                .with_serdes::<ProtobufSerdes<RawImage>>()
                .build()?;

            let backend = camera_config.raw.decoder.into();
            match VideoH264Decoder::new(backend) {
                Ok(decoder) => {
                    log::info!(
                        "Camera '{}' raw publishing to '{}' with {:?} decoder",
                        camera_config.name,
                        raw_topic,
                        camera_config.raw.decoder
                    );

                    // Channels for decoder thread
                    let (h264_tx, h264_rx) = flume::bounded::<DecoderInput>(4);
                    let (decoded_tx, decoded_rx) = flume::unbounded::<RawFrame>();

                    // Spawn decoder in separate thread (GStreamer has its own threading)
                    let camera_name = camera_config.name.clone();
                    std::thread::spawn(move || {
                        log::debug!("Decoder thread started for '{}'", camera_name);

                        while let Ok(input) = h264_rx.recv() {
                            // Push to GStreamer (may block)
                            if let Err(e) = decoder.push(&input.data, input.pts, input.keyframe) {
                                log::error!("Decoder push error for '{}': {}", camera_name, e);
                                continue;
                            }

                            // Drain all decoded frames
                            while let Ok(mut frame) = decoder.receiver().try_recv() {
                                frame.sequence = decoder.next_sequence();
                                let _ = decoded_tx.send(frame);
                            }
                        }

                        log::debug!("Decoder thread ended for '{}'", camera_name);
                    });

                    (Some(raw_pub), Some(h264_tx), Some(decoded_rx))
                }
                Err(e) => {
                    log::error!(
                        "Failed to create decoder for '{}': {}",
                        camera_config.name,
                        e
                    );
                    (None, None, None)
                }
            }
        } else {
            (None, None, None)
        };

        Ok(Self {
            node,
            compressed_publisher,
            raw_publisher,
            decoder_tx,
            decoded_rx,
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
                        "Failed to create capture for '{}': {}",
                        self.camera_config.name,
                        e
                    );
                    return Ok(());
                }
            };

        if let Err(e) = capture.start() {
            log::error!(
                "Failed to start capture for '{}': {}",
                self.camera_config.name,
                e
            );
            return Ok(());
        }

        log::info!(
            "Camera '{}' capturing from {}",
            self.camera_config.name,
            self.camera_config.url
        );

        let capture_rx = capture.receiver();

        loop {
            tokio::select! {
                biased;

                _ = shutdown_rx.changed() => break,

                // H264 frames from capture
                Ok(frame) = capture_rx.recv_async() => {
                    // Publish compressed (fast)
                    let msg = frame_to_compressed_image(&frame, &self.camera_config.name);
                    let _ = self.compressed_publisher.async_publish(&msg).await;

                    // Send to decoder thread (non-blocking, drop if full)
                    if let Some(ref tx) = self.decoder_tx {
                        let _ = tx.try_send(DecoderInput {
                            data: frame.as_slice().to_vec(),
                            pts: frame.pts,
                            keyframe: frame.keyframe,
                        });
                    }
                }

                // Decoded frames from decoder thread
                Ok(frame) = async {
                    match &self.decoded_rx {
                        Some(rx) => rx.recv_async().await,
                        None => std::future::pending().await,
                    }
                } => {
                    if let Some(ref raw_pub) = self.raw_publisher {
                        let msg = frame_to_raw_image(&frame, &self.camera_config.name);
                        let _ = raw_pub.async_publish(&msg).await;
                    }
                }
            }
        }

        log::info!("Shutting down camera '{}'...", self.camera_config.name);
        let _ = capture.close();
        Ok(())
    }
}
