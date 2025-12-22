use crate::config::CameraConfig;
use crate::h264_capture::{H264Frame, H264StreamCapture};
use crate::h264_decode::{DecoderBackend, RawFrame, VideoH264Decoder};
use crate::protos::{CompressedImage, Header, RawImage};
use ros_z::{context::ZContext, msg::ProtobufSerdes, pubsub::ZPub, Builder, Result as ZResult};
use std::sync::Arc;
use tokio::task::JoinSet;

fn get_pub_time() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_nanos() as u64)
        .unwrap_or(0)
}

fn frame_to_compressed_image(frame: H264Frame, camera_name: &str) -> CompressedImage {
    CompressedImage {
        header: Some(Header {
            acq_time: frame.pts,
            pub_time: get_pub_time(),
            sequence: frame.sequence,
            frame_id: camera_name.to_string(),
        }),
        format: "h264".to_string(),
        data: frame.as_slice().into(),
    }
}

fn frame_to_raw_image(frame: RawFrame, camera_name: &str) -> RawImage {
    RawImage {
        header: Some(Header {
            acq_time: frame.pts,
            pub_time: get_pub_time(),
            sequence: frame.sequence,
            frame_id: camera_name.to_string(),
        }),
        width: frame.width,
        height: frame.height,
        encoding: frame.format,
        step: frame.step,
        data: frame.data,
    }
}

/// RTSP Camera node - captures H264 streams and publishes via ros-z
pub struct RtspCameraNode {
    ctx: Arc<ZContext>,
    camera_config: CameraConfig,
}

impl RtspCameraNode {
    pub fn new(ctx: Arc<ZContext>, camera_config: CameraConfig) -> ZResult<Self> {
        Ok(Self { ctx, camera_config })
    }

    /// Compressed task: feeds decoder, publishes compressed images
    async fn compressed_task(
        ctx: Arc<ZContext>,
        capture: Arc<H264StreamCapture>,
        decoder: Arc<VideoH264Decoder>,
        camera_name: String,
        shutdown_tx: tokio::sync::watch::Sender<()>,
    ) -> ZResult<()> {
        let mut shutdown_rx = shutdown_tx.subscribe();

        // Create compressed publisher
        let node = ctx
            .create_node(format!("camera_{}_compressed", camera_name))
            .build()?;

        let compressed_topic = format!("/camera/{}/compressed", camera_name);
        let compressed_pub: ZPub<CompressedImage, ProtobufSerdes<CompressedImage>> = node
            .create_pub::<CompressedImage>(&compressed_topic)
            .with_serdes::<ProtobufSerdes<CompressedImage>>()
            .build()?;

        log::info!(
            "[{}] Compressed task started → '{}'",
            camera_name,
            compressed_topic
        );

        let mut published: u64 = 0;
        let mut last_log = std::time::Instant::now();

        loop {
            tokio::select! {
                biased;

                _ = shutdown_rx.changed() => {
                    log::info!("[{}] Compressed task received shutdown", camera_name);
                    break;
                }

                result = capture.receiver().recv_async() => {
                    match result {
                        Ok(h264_frame) => {
                            // Feed decoder
                            if let Err(e) = decoder.push(h264_frame.as_slice(), h264_frame.pts, h264_frame.keyframe) {
                                log::warn!("[{}] Decoder push failed: {}", camera_name, e);
                            }

                            // Publish compressed
                            let sequence = h264_frame.sequence;
                            let msg = frame_to_compressed_image(h264_frame, &camera_name);
                            if compressed_pub.async_publish(&msg).await.is_ok() {
                                published += 1;
                            }

                            // Log stats every second
                            if last_log.elapsed().as_secs() >= 1 {
                                log::info!(
                                    "[{}] Compressed: frame {}, {} published",
                                    camera_name,
                                    sequence,
                                    published
                                );
                                last_log = std::time::Instant::now();
                            }
                        }
                        Err(_) => break, // Channel closed
                    }
                }
            }
        }

        log::info!(
            "[{}] Compressed task exiting (published: {})",
            camera_name,
            published
        );

        Ok(())
    }

    /// Raw image task: reads decoded frames, publishes via ros-z
    async fn raw_task(
        ctx: Arc<ZContext>,
        decoder: Arc<VideoH264Decoder>,
        camera_name: String,
        shutdown_tx: tokio::sync::watch::Sender<()>,
    ) -> ZResult<()> {
        let mut shutdown_rx = shutdown_tx.subscribe();

        // Create raw image publisher using ros-z
        let node = ctx
            .create_node(format!("camera_{}_raw", camera_name))
            .build()?;

        let raw_topic = format!("/camera/{}/raw", camera_name);
        let raw_pub: ZPub<RawImage, ProtobufSerdes<RawImage>> = node
            .create_pub::<RawImage>(&raw_topic)
            .with_serdes::<ProtobufSerdes<RawImage>>()
            .build()?;

        log::info!("[{}] Raw task started → '{}'", camera_name, raw_topic);

        let mut published: u64 = 0;
        let mut last_log = std::time::Instant::now();

        loop {
            tokio::select! {
                biased;

                _ = shutdown_rx.changed() => {
                    log::info!("[{}] Raw task received shutdown", camera_name);
                    break;
                }

                result = decoder.receiver().recv_async() => {
                    match result {
                        Ok(frame) => {
                            let sequence = frame.sequence;

                            // Build and publish raw image
                            let msg = frame_to_raw_image(frame, &camera_name);
                            if raw_pub.async_publish(&msg).await.is_ok() {
                                published += 1;
                            }

                            // Log stats every second
                            if last_log.elapsed().as_secs() >= 1 {
                                log::info!(
                                    "[{}] Raw: frame {}, {} published",
                                    camera_name,
                                    sequence,
                                    published
                                );
                                last_log = std::time::Instant::now();
                            }
                        }
                        Err(_) => break, // Channel closed
                    }
                }
            }
        }

        log::info!(
            "[{}] Raw task exiting (published: {})",
            camera_name,
            published
        );
        Ok(())
    }

    pub async fn run(self, shutdown_tx: tokio::sync::watch::Sender<()>) -> ZResult<()> {
        let camera_name = self.camera_config.name.clone();

        // Create H264 capture
        let capture = Arc::new(H264StreamCapture::new(
            &self.camera_config.url,
            self.camera_config.latency,
        )?);

        capture.start()?;

        // Create decoder (shared between tasks via its output channel)
        let decoder_backend: DecoderBackend = self.camera_config.decoder.into();
        let decoder = Arc::new(VideoH264Decoder::new(
            decoder_backend,
            self.camera_config.height,
            self.camera_config.width,
        )?);

        log::info!(
            "Camera '{}' decoder: {:?} {}x{}",
            camera_name,
            decoder_backend,
            self.camera_config.width,
            self.camera_config.height
        );

        // Spawn tasks
        let mut tasks: JoinSet<()> = JoinSet::new();

        // Compressed task
        tasks.spawn({
            let ctx = self.ctx.clone();
            let camera_name = camera_name.clone();
            let capture = capture.clone();
            let decoder = decoder.clone();
            let shutdown_tx = shutdown_tx.clone();
            async move {
                if let Err(e) =
                    Self::compressed_task(ctx, capture, decoder, camera_name.clone(), shutdown_tx)
                        .await
                {
                    log::error!("[{}] Compressed task failed: {}", camera_name, e);
                }
            }
        });

        // Raw image task (using ros-z instead of direct Zenoh SHM)
        tasks.spawn({
            let ctx = self.ctx.clone();
            let camera_name = camera_name.clone();
            let decoder = decoder.clone();
            let shutdown_tx = shutdown_tx.clone();
            async move {
                if let Err(e) = Self::raw_task(ctx, decoder, camera_name.clone(), shutdown_tx).await
                {
                    log::error!("[{}] Raw task failed: {}", camera_name, e);
                }
            }
        });

        // Wait for shutdown signal
        let mut shutdown_rx = shutdown_tx.subscribe();
        let _ = shutdown_rx.changed().await;

        log::info!("Shutting down camera '{}'...", camera_name);

        // Wait for all tasks to complete
        while tasks.join_next().await.is_some() {}

        // Cleanup resources
        if let Err(e) = capture.close() {
            log::error!("[{}] Failed to close capture: {}", camera_name, e);
        }
        if let Err(e) = decoder.close() {
            log::error!("[{}] Failed to close decoder: {}", camera_name, e);
        }

        log::info!("Camera '{}' shutdown complete", camera_name);
        Ok(())
    }
}
