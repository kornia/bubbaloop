use crate::config::CameraConfig;
use crate::h264_capture::{H264Frame, H264StreamCapture};
use crate::h264_decode::{RawFrame, VideoH264Decoder};
use crate::protos::{CompressedImage, Header, RawImage};
use prost::Message;
use ros_z::{
    context::ZContext, msg::ProtobufSerdes, node::ZNode, pubsub::ZPub, Builder, Result as ZResult,
};
use std::sync::Arc;
use zenoh::bytes::ZBytes;
use zenoh::pubsub::Publisher;
use zenoh::shm::{GarbageCollect, ShmProviderBuilder};
use zenoh::Wait;

/// SHM pool size per camera (256MB = ~1300 frames at 200KB each)
const SHM_POOL_SIZE: usize = 256 * 1024 * 1024;

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

/// RTSP Camera node - captures H264 streams and publishes raw frames via SHM
pub struct RtspCameraNode {
    #[allow(dead_code)]
    node: ZNode,
    compressed_pub: ZPub<CompressedImage, ProtobufSerdes<CompressedImage>>,
    camera_config: CameraConfig,
    // SHM components for zero-copy raw image transfer
    shm_provider: Arc<zenoh::shm::ShmProvider<zenoh::shm::PosixShmProviderBackend>>,
    #[allow(dead_code)]
    shm_session: Arc<zenoh::Session>,
    shm_raw_pub: Publisher<'static>,
    // Stats
    shm_published: u64,
    shm_skipped: u64,
    shm_last_log: std::time::Instant,
}

impl RtspCameraNode {
    pub fn new(ctx: Arc<ZContext>, camera_config: CameraConfig) -> ZResult<Self> {
        let node_name = format!("camera_{}_node", camera_config.name);
        let node = ctx.create_node(&node_name).build()?;

        // Compressed H264 publisher (ros-z)
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

        // SHM provider for zero-copy raw image transfer
        let shm_provider = Arc::new(
            ShmProviderBuilder::default_backend(SHM_POOL_SIZE)
                .wait()
                .map_err(|e| {
                    Box::new(std::io::Error::other(format!("SHM backend error: {}", e)))
                        as Box<dyn std::error::Error + Send + Sync>
                })?,
        );

        log::info!(
            "Camera '{}' SHM pool: {} MB",
            camera_config.name,
            SHM_POOL_SIZE / (1024 * 1024)
        );

        // Zenoh session with SHM enabled
        let mut zenoh_config = zenoh::Config::default();
        zenoh_config
            .insert_json5("transport/shared_memory/enabled", "true")
            .map_err(|e| {
                Box::new(std::io::Error::other(format!("Zenoh config error: {}", e)))
                    as Box<dyn std::error::Error + Send + Sync>
            })?;

        let shm_session = Arc::new(zenoh::open(zenoh_config).wait().map_err(|e| {
            Box::new(std::io::Error::other(format!("Zenoh session error: {}", e)))
                as Box<dyn std::error::Error + Send + Sync>
        })?);

        // SHM publisher for raw images
        let shm_raw_topic = format!("camera/{}/raw_shm", camera_config.name);
        log::info!(
            "Camera '{}' SHM topic: '{}'",
            camera_config.name,
            shm_raw_topic
        );
        let shm_raw_pub = shm_session
            .declare_publisher(shm_raw_topic)
            .wait()
            .map_err(|e| {
                Box::new(std::io::Error::other(format!("SHM publisher error: {}", e)))
                    as Box<dyn std::error::Error + Send + Sync>
            })?;

        Ok(Self {
            node,
            compressed_pub,
            camera_config,
            shm_provider,
            shm_session,
            shm_raw_pub,
            shm_published: 0,
            shm_skipped: 0,
            shm_last_log: std::time::Instant::now(),
        })
    }

    /// Publish a raw frame via SHM using async allocation
    async fn publish_shm(&mut self, frame: RawFrame) {
        // Build protobuf message
        let msg = RawImage {
            header: Some(Header {
                acq_time: frame.pts,
                pub_time: get_pub_time(),
                sequence: frame.sequence,
                frame_id: self.camera_config.name.clone(),
            }),
            width: frame.width,
            height: frame.height,
            encoding: frame.format,
            step: frame.step,
            data: frame.data,
        };
        let proto_bytes = msg.encode_to_vec();

        // Allocate SHM buffer with BlockOn<GarbageCollect> policy (async)
        // - GarbageCollect: reclaims unused buffers on OOM
        // - BlockOn: enables async .await, yields with 1ms sleep between retries
        let mut shm_buf = match self
            .shm_provider
            .alloc(proto_bytes.len())
            .with_policy::<GarbageCollect>()
            .wait()
        {
            Ok(buf) => buf,
            Err(e) => {
                self.shm_skipped += 1;
                log::error!(
                    "[{}] Failed to allocate SHM buffer: {}",
                    self.camera_config.name,
                    e
                );
                return;
            }
        };

        // Copy data and publish
        shm_buf.as_mut().copy_from_slice(&proto_bytes);
        let zbytes: ZBytes = shm_buf.into();

        if self.shm_raw_pub.put(zbytes).await.is_ok() {
            self.shm_published += 1;
        } else {
            self.shm_skipped += 1;
        }

        // Log stats every second
        if self.shm_last_log.elapsed().as_secs() >= 1 {
            log::info!(
                "[{}] SHM: frame {}, {} published, {} skipped ({}x{}, {} bytes)",
                self.camera_config.name,
                msg.header.as_ref().map(|h| h.sequence).unwrap_or(0),
                self.shm_published,
                self.shm_skipped,
                msg.width,
                msg.height,
                proto_bytes.len()
            );
            self.shm_last_log = std::time::Instant::now();
        }
    }

    pub async fn run(mut self, shutdown_tx: tokio::sync::watch::Sender<()>) -> ZResult<()> {
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
                    "Camera '{}' decoder: {:?} {}x{}",
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
        let camera_name = self.camera_config.name.clone();

        loop {
            tokio::select! {
                biased;

                _ = shutdown_rx.changed() => break,

                // H264 compressed frames -> decode and publish compressed
                Ok(h264_frame) = h264_rx.recv_async() => {
                    // Push to decoder
                    if let Err(e) = decoder.push(h264_frame.as_slice(), h264_frame.pts, h264_frame.keyframe) {
                        log::warn!("[{}] Decoder push failed: {}", camera_name, e);
                    }

                    // Publish compressed (fire-and-forget)
                    let msg = frame_to_compressed_image(h264_frame, &camera_name);
                    let _ = self.compressed_pub.async_publish(&msg).await;
                }

                // Decoded raw frames -> publish via SHM
                Ok(raw_frame) = decoder.receiver().recv_async() => {
                    self.publish_shm(raw_frame).await;
                }
            }
        }

        log::info!("Shutting down camera '{}'...", self.camera_config.name);
        let _ = capture.close();
        let _ = decoder.close();

        Ok(())
    }
}
