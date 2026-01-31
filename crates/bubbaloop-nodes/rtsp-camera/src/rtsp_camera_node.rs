use crate::config::CameraConfig;
use crate::h264_capture::{H264Frame, H264StreamCapture};
use crate::h264_decode::{DecoderBackend, RawFrame, VideoH264Decoder};
use bubbaloop_schemas::{CompressedImage, Header, RawImage};
use prost::Message;
use ros_z::{context::ZContext, msg::ProtobufSerdes, pubsub::ZPub, Builder, Result as ZResult};
use std::sync::Arc;
use tokio::task::JoinSet;
use zenoh::bytes::ZBytes;
use zenoh::shm::{BlockOn, GarbageCollect, ShmProviderBuilder};
use zenoh::Wait;

/// SHM pool size per camera (256MB = ~1300 frames at 200KB each)
const SHM_POOL_SIZE: usize = 256 * 1024 * 1024;

fn get_pub_time() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_nanos() as u64)
        .unwrap_or(0)
}

fn frame_to_compressed_image(
    frame: H264Frame,
    camera_name: &str,
    machine_id: &str,
) -> CompressedImage {
    CompressedImage {
        header: Some(Header {
            acq_time: frame.pts,
            pub_time: get_pub_time(),
            sequence: frame.sequence,
            frame_id: camera_name.to_string(),
            machine_id: machine_id.to_string(),
            ..Default::default()
        }),
        format: "h264".to_string(),
        data: frame.as_slice().into(),
    }
}

fn frame_to_raw_image(frame: RawFrame, camera_name: &str, machine_id: &str) -> RawImage {
    RawImage {
        header: Some(Header {
            acq_time: frame.pts,
            pub_time: get_pub_time(),
            sequence: frame.sequence,
            frame_id: camera_name.to_string(),
            machine_id: machine_id.to_string(),
            ..Default::default()
        }),
        width: frame.width,
        height: frame.height,
        encoding: frame.format,
        step: frame.step,
        data: frame.data,
    }
}

/// RTSP Camera node - captures H264 streams and publishes via compressed and SHM topics
pub struct RtspCameraNode {
    ctx: Arc<ZContext>,
    camera_config: CameraConfig,
    machine_id: String,
}

impl RtspCameraNode {
    pub fn new(
        ctx: Arc<ZContext>,
        camera_config: CameraConfig,
        machine_id: String,
    ) -> ZResult<Self> {
        Ok(Self {
            ctx,
            camera_config,
            machine_id,
        })
    }

    /// Compressed task: feeds decoder, publishes compressed images
    async fn compressed_task(
        ctx: Arc<ZContext>,
        capture: Arc<H264StreamCapture>,
        decoder: Arc<VideoH264Decoder>,
        camera_name: String,
        machine_id: String,
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
        let mut last_published_count: u64 = 0;

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
                            let msg = frame_to_compressed_image(h264_frame, &camera_name, &machine_id);
                            if compressed_pub.async_publish(&msg).await.is_ok() {
                                published += 1;
                            }

                            // Log stats with FPS every second
                            let elapsed = last_log.elapsed();
                            if elapsed.as_secs() >= 1 {
                                let frames_this_period = published - last_published_count;
                                let fps = frames_this_period as f64 / elapsed.as_secs_f64();
                                log::info!(
                                    "[{}] Compressed: seq={}, total={}, fps={:.1}",
                                    camera_name,
                                    sequence,
                                    published,
                                    fps
                                );
                                last_published_count = published;
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

    /// SHM task: reads decoded frames, publishes via SHM
    async fn shm_task(
        decoder: Arc<VideoH264Decoder>,
        camera_name: String,
        machine_id: String,
        shutdown_tx: tokio::sync::watch::Sender<()>,
    ) -> ZResult<()> {
        let mut shutdown_rx = shutdown_tx.subscribe();

        // Create SHM provider
        let shm_provider = Arc::new(ShmProviderBuilder::default_backend(SHM_POOL_SIZE).wait()?);

        // Zenoh session with SHM enabled
        let mut zenoh_config = zenoh::Config::default();
        zenoh_config.insert_json5("transport/shared_memory/enabled", "true")?;
        let shm_session = Arc::new(zenoh::open(zenoh_config).wait()?);

        let shm_raw_topic = format!("camera/{}/raw_shm", camera_name);
        let shm_raw_pub = shm_session.declare_publisher(&shm_raw_topic).wait()?;

        log::info!(
            "[{}] SHM task started → '{}' ({} MB pool, 1 Hz)",
            camera_name,
            shm_raw_topic,
            SHM_POOL_SIZE / (1024 * 1024)
        );

        let mut published: u64 = 0;
        let mut decoded_count: u64 = 0;
        let mut last_log = std::time::Instant::now();
        let mut last_published_count: u64 = 0;
        let mut last_decoded_count: u64 = 0;

        // Rate limiting: 1 Hz (publish once per second)
        let publish_interval = std::time::Duration::from_secs(1);
        let mut last_publish = std::time::Instant::now() - publish_interval;

        loop {
            tokio::select! {
                biased;

                _ = shutdown_rx.changed() => {
                    log::info!("[{}] SHM task received shutdown", camera_name);
                    break;
                }

                result = decoder.receiver().recv_async() => {
                    match result {
                        Ok(frame) => {
                            decoded_count += 1;
                            let sequence = frame.sequence;

                            // Rate limit: only publish at 1 Hz
                            let now = std::time::Instant::now();
                            let should_publish = now.duration_since(last_publish) >= publish_interval;

                            if should_publish {
                                last_publish = now;

                                // Build protobuf
                                let msg = frame_to_raw_image(frame, &camera_name, &machine_id);
                                let proto_bytes = msg.encode_to_vec();

                                // Allocate SHM buffer
                                let mut shm_buf = match shm_provider
                                    .alloc(proto_bytes.len())
                                    .with_policy::<BlockOn<GarbageCollect>>()
                                    .await
                                {
                                    Ok(buf) => buf,
                                    Err(e) => {
                                        log::error!("[{}] SHM alloc failed: {}", camera_name, e);
                                        continue;
                                    }
                                };

                                // Copy and publish
                                shm_buf.as_mut().copy_from_slice(&proto_bytes);
                                let zbytes: ZBytes = shm_buf.into();

                                if shm_raw_pub.put(zbytes).await.is_ok() {
                                    published += 1;
                                }
                            }

                            // Log stats with FPS every second
                            let elapsed = last_log.elapsed();
                            if elapsed.as_secs() >= 1 {
                                let decoded_this_period = decoded_count - last_decoded_count;
                                let decode_fps = decoded_this_period as f64 / elapsed.as_secs_f64();
                                let pub_this_period = published - last_published_count;
                                log::info!(
                                    "[{}] SHM: seq={}, decoded_fps={:.1}, pub={}",
                                    camera_name,
                                    sequence,
                                    decode_fps,
                                    pub_this_period
                                );
                                last_decoded_count = decoded_count;
                                last_published_count = published;
                                last_log = std::time::Instant::now();
                            }
                        }
                        Err(_) => break, // Channel closed
                    }
                }
            }
        }

        log::info!(
            "[{}] SHM task exiting (published: {})",
            camera_name,
            published
        );
        Ok(())
    }

    pub async fn run(
        self,
        shutdown_tx: tokio::sync::watch::Sender<()>,
        zenoh_session: std::sync::Arc<zenoh::Session>,
        scope: String,
        machine_id: String,
    ) -> ZResult<()> {
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

        // Create health heartbeat publisher
        let health_topic = format!("bubbaloop/{}/{}/health/rtsp-camera", scope, machine_id);
        let health_publisher = zenoh_session
            .declare_publisher(health_topic.clone())
            .await
            .map_err(|e| {
                Box::<dyn std::error::Error + Send + Sync>::from(format!(
                    "Health publisher error: {}",
                    e
                ))
            })?;
        log::info!("[{}] Health heartbeat topic: {}", camera_name, health_topic);

        // Spawn tasks with shutdown receivers
        let mut tasks: JoinSet<()> = JoinSet::new();

        tasks.spawn({
            let ctx = self.ctx.clone();
            let camera_name = camera_name.clone();
            let capture = capture.clone();
            let decoder = decoder.clone();
            let machine_id = self.machine_id.clone();
            let shutdown_tx = shutdown_tx.clone();
            async move {
                if let Err(e) = Self::compressed_task(
                    ctx,
                    capture,
                    decoder,
                    camera_name.clone(),
                    machine_id,
                    shutdown_tx,
                )
                .await
                {
                    log::error!("[{}] Compressed task failed: {}", camera_name, e);
                }
            }
        });

        tasks.spawn({
            let camera_name = camera_name.clone();
            let decoder = decoder.clone();
            let machine_id = self.machine_id.clone();
            let shutdown_tx = shutdown_tx.clone();
            async move {
                if let Err(e) =
                    Self::shm_task(decoder, camera_name.clone(), machine_id, shutdown_tx).await
                {
                    log::error!("[{}] SHM task failed: {}", camera_name, e);
                }
            }
        });

        // Health heartbeat + shutdown loop
        let mut shutdown_rx = shutdown_tx.subscribe();
        let mut health_interval = tokio::time::interval(std::time::Duration::from_secs(5));

        loop {
            tokio::select! {
                biased;

                _ = shutdown_rx.changed() => {
                    log::info!("Shutting down camera '{}'...", camera_name);
                    break;
                }

                _ = health_interval.tick() => {
                    if let Err(e) = health_publisher.put("ok").await {
                        log::warn!("[{}] Failed to publish health heartbeat: {}", camera_name, e);
                    }
                }
            }
        }

        // Tasks will exit via their select! loops when they see shutdown
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
