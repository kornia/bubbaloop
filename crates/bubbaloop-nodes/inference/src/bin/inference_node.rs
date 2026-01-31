//! Inference node - subscribes to raw images via SHM and computes statistics
//!
//! This is a validation node to test raw image transfer via Zenoh SHM.
//! It subscribes to `camera/*/raw_shm` topics and computes mean/std for each frame.
//!
//! Based on: https://github.com/eclipse-zenoh/zenoh/blob/main/examples/examples/z_sub_shm.rs

use bubbaloop_schemas::RawImage;
use prost::Message;
use zenoh::Wait;

/// Compute mean and standard deviation of pixel values
fn compute_image_stats(data: &[u8]) -> (f64, f64) {
    if data.is_empty() {
        return (0.0, 0.0);
    }

    let n = data.len() as f64;
    let sum: f64 = data.iter().map(|&x| x as f64).sum();
    let mean = sum / n;

    let variance: f64 = data.iter().map(|&x| (x as f64 - mean).powi(2)).sum::<f64>() / n;
    let std = variance.sqrt();

    (mean, std)
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    zenoh::init_log_from_env_or("error");
    env_logger::init();

    log::info!("Starting inference node (SHM subscriber)...");

    // Read scope/machine env vars for health heartbeat
    let scope = std::env::var("BUBBALOOP_SCOPE").unwrap_or_else(|_| "local".to_string());
    let machine_id = std::env::var("BUBBALOOP_MACHINE_ID").unwrap_or_else(|_| {
        hostname::get()
            .map(|h| h.to_string_lossy().to_string())
            .unwrap_or_else(|_| "unknown".to_string())
    });
    log::info!("Scope: {}, Machine ID: {}", scope, machine_id);

    // Create shutdown channel
    let shutdown_tx = tokio::sync::watch::Sender::new(());

    // Set up Ctrl+C handler
    {
        let shutdown_tx = shutdown_tx.clone();
        ctrlc::set_handler(move || {
            log::info!("Received Ctrl+C, shutting down gracefully...");
            let _ = shutdown_tx.send(());
        })?;
    }

    // Create Zenoh session with SHM enabled
    let mut config = zenoh::Config::default();
    config.insert_json5("transport/shared_memory/enabled", "true")?;
    if let Ok(endpoint) = std::env::var("ZENOH_ENDPOINT") {
        config.insert_json5("connect/endpoints", &format!(r#"["{}"]"#, endpoint))?;
    }

    let session = zenoh::open(config).wait()?;

    // Create health heartbeat publisher
    let health_topic = format!("bubbaloop/{}/{}/health/inference", scope, machine_id);
    let health_publisher = session.declare_publisher(health_topic.clone()).await?;
    log::info!("Health heartbeat topic: {}", health_topic);

    // Subscribe to all camera raw_shm topics using wildcard
    let topic = "camera/*/raw_shm";
    let subscriber = session.declare_subscriber(topic).wait()?;

    log::info!(
        "Inference node subscribed to '{}', waiting for SHM images...",
        topic
    );

    let mut frame_count = 0u64;
    let mut total_dropped = 0u64;
    let mut last_log_time = std::time::Instant::now();
    let mut shutdown_rx = shutdown_tx.subscribe();
    let mut health_interval = tokio::time::interval(std::time::Duration::from_secs(5));

    // SHM-agnostic receive loop - handles both SHM and RAW data transparently
    // Uses "latest only" strategy: drain queue and process only the most recent sample
    loop {
        tokio::select! {
            biased;

            _ = shutdown_rx.changed() => {
                log::info!("Inference node received shutdown signal");
                break;
            }

            _ = health_interval.tick() => {
                if let Err(e) = health_publisher.put("ok").await {
                    log::warn!("Failed to publish health heartbeat: {}", e);
                }
            }

            result = subscriber.recv_async() => {
                let mut sample = match result {
                    Ok(s) => s,
                    Err(_) => break,
                };

                // Drain any queued samples, keep only the latest
                let mut batch_dropped = 0u64;
                while let Ok(Some(newer)) = subscriber.try_recv() {
                    sample = newer;
                    batch_dropped += 1;
                }
                total_dropped += batch_dropped;

                frame_count += 1;

                let key_str = sample.key_expr().as_str();

                // Get payload bytes - works transparently for both SHM and RAW
                let payload = sample.payload();
                let bytes = payload.to_bytes();

                // Decode protobuf
                let msg = match RawImage::decode(bytes.as_ref()) {
                    Ok(m) => m,
                    Err(e) => {
                        log::warn!("Failed to decode RawImage proto from '{}': {}", key_str, e);
                        continue;
                    }
                };

                // Compute stats
                let (mean, std) = compute_image_stats(&msg.data);

                // Get header info
                let (seq, frame_id) = msg
                    .header
                    .as_ref()
                    .map(|h| (h.sequence, h.frame_id.as_str()))
                    .unwrap_or((0, "unknown"));

                // Log every frame initially, then throttle to 1/sec
                let elapsed = last_log_time.elapsed();
                if elapsed.as_secs_f32() >= 1.0 || frame_count <= 10 {
                    log::info!(
                        "[{}] Frame {} ({}x{} {}): mean={:.2}, std={:.2}, size={} bytes (skipped {} this batch)",
                        frame_id,
                        seq,
                        msg.width,
                        msg.height,
                        msg.encoding,
                        mean,
                        std,
                        msg.data.len(),
                        batch_dropped
                    );
                    last_log_time = std::time::Instant::now();
                }
            }
        }
    }

    log::info!(
        "Inference node stopped after {} frames ({} total skipped)",
        frame_count,
        total_dropped
    );

    Ok(())
}
