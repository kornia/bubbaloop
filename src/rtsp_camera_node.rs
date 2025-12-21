use crate::config::CameraConfig;
use crate::h264_capture::{H264Frame, H264StreamCapture};
use crate::h264_decode::{RawFrame, VideoH264Decoder};
use crate::protos::{CompressedImage, Header, RawImage};
use ros_z::{
    context::ZContext, msg::ProtobufSerdes, node::ZNode, pubsub::ZPub, Builder, Result as ZResult,
};
use std::sync::Arc;

fn get_pub_time() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_nanos() as u64)
        .unwrap_or(0)
}

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

fn frame_to_raw_image(frame: &RawFrame, camera_name: &str, sequence: u32) -> RawImage {
    RawImage {
        header: Some(Header {
            acq_time: frame.pts,
            pub_time: get_pub_time(),
            sequence,
            frame_id: camera_name.to_string(),
        }),
        width: frame.width,
        height: frame.height,
        encoding: frame.format.clone(),
        step: frame.step,
        data: frame.data.clone(),
    }
}

/// RTSP Camera node - captures and publishes H264 + optional raw frames
pub struct RtspCameraNode {
    #[allow(dead_code)]
    node: ZNode,
    compressed_pub: ZPub<CompressedImage, ProtobufSerdes<CompressedImage>>,
    raw_pub: ZPub<RawImage, ProtobufSerdes<RawImage>>,
    camera_config: CameraConfig,
}

impl RtspCameraNode {
    pub fn new(ctx: Arc<ZContext>, camera_config: CameraConfig) -> ZResult<Self> {
        let node_name = format!("camera_{}_node", camera_config.name);
        let node = ctx.create_node(&node_name).build()?;

        let compressed_topic = format!("/camera/{}/compressed", camera_config.name);
        let compressed_pub = node
            .create_pub::<CompressedImage>(&compressed_topic)
            .with_serdes::<ProtobufSerdes<CompressedImage>>()
            .build()?;

        log::info!(
            "Camera '{}' publishing compressed to '{}'",
            camera_config.name,
            compressed_topic
        );

        let raw_topic = format!("/camera/{}/raw", camera_config.name);
        let raw_pub = node
            .create_pub::<RawImage>(&raw_topic)
            .with_serdes::<ProtobufSerdes<RawImage>>()
            .build()?;
        log::info!("Camera '{}' raw topic '{}'", camera_config.name, raw_topic);

        Ok(Self {
            node,
            compressed_pub,
            raw_pub,
            camera_config,
        })
    }

    pub async fn run(self, shutdown_tx: tokio::sync::watch::Sender<()>) -> ZResult<()> {
        let mut shutdown_rx = shutdown_tx.subscribe();

        // Create H264 capture
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

        // Create decoder
        let decoder = match VideoH264Decoder::new(
            self.camera_config.decoder.into(),
            self.camera_config.height,
            self.camera_config.width,
        ) {
            Ok(d) => {
                log::info!(
                    "Camera '{}' decoder created (backend: {:?}, {}x{})",
                    self.camera_config.name,
                    self.camera_config.decoder,
                    self.camera_config.width,
                    self.camera_config.height
                );
                d
            }
            Err(e) => {
                log::error!(
                    "Failed to create decoder for '{}': {}",
                    self.camera_config.name,
                    e
                );
                return Ok(());
            }
        };

        log::info!(
            "Camera '{}' capturing from {}",
            self.camera_config.name,
            self.camera_config.url
        );

        let h264_rx = capture.receiver();

        loop {
            tokio::select! {
                biased;

                _ = shutdown_rx.changed() => break,

                // H264 compressed frames
                Ok(compressed_frame) = h264_rx.recv_async() => {
                    // Send to decoder
                    let _ = decoder.push(compressed_frame.as_slice(), compressed_frame.pts, compressed_frame.keyframe);

                    // Publish compressed
                    let msg = frame_to_compressed_image(&compressed_frame, &self.camera_config.name);
                    let _ = self.compressed_pub.async_publish(&msg).await;
                }

                // Decoded raw frames
                Ok(raw_frame) = decoder.receiver().recv_async() => {
                    let sequence = raw_frame.sequence;

                    let msg = frame_to_raw_image(&raw_frame, &self.camera_config.name, sequence);
                    let _ = self.raw_pub.async_publish(&msg).await;
                }
            }
        }

        log::info!("Shutting down camera '{}'...", self.camera_config.name);
        let _ = capture.close();
        let _ = decoder.close();
        Ok(())
    }
}
