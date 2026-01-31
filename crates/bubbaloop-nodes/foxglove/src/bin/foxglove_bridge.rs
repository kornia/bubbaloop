use argh::FromArgs;
use bubbaloop_schemas::config::TopicsConfig;
use foxglove::WebSocketServer;
use foxglove_bridge::foxglove_node::FoxgloveNode;
use ros_z::{context::ZContextBuilder, Builder, Result as ZResult};
use serde_json::json;
use std::sync::Arc;

#[derive(FromArgs)]
/// Foxglove bridge for camera visualization
struct Args {
    /// path to the topics configuration file
    #[argh(
        option,
        short = 'c',
        default = "String::from(\"crates/foxglove_bridge/configs/topics.yaml\")"
    )]
    config: String,
}

#[tokio::main]
async fn main() -> ZResult<()> {
    // Initialize logging
    let env = env_logger::Env::default().default_filter_or("info");
    env_logger::init_from_env(env);

    let args: Args = argh::from_env();

    // Load topics configuration
    let config = match TopicsConfig::from_file(&args.config) {
        Ok(c) => c,
        Err(e) => {
            log::error!("Failed to load config from '{}': {}", args.config, e);
            std::process::exit(1);
        }
    };

    log::info!(
        "Loaded configuration with {} topics for Foxglove bridge",
        config.topics.len()
    );

    if config.topics.is_empty() {
        log::error!("No topics found in configuration");
        std::process::exit(1);
    }

    // Create shutdown channel
    let shutdown_tx = tokio::sync::watch::Sender::new(());

    // Set up Ctrl+C handler
    ctrlc::set_handler({
        let shutdown_tx = shutdown_tx.clone();
        move || {
            log::info!("Received Ctrl+C, shutting down gracefully...");
            if let Err(e) = shutdown_tx.send(()) {
                log::warn!(
                    "Failed to send shutdown signal: {}. Receiver may have been dropped.",
                    e
                );
            }
        }
    })?;

    // Read scope/machine env vars for health heartbeat
    let scope = std::env::var("BUBBALOOP_SCOPE").unwrap_or_else(|_| "local".to_string());
    let machine_id = std::env::var("BUBBALOOP_MACHINE_ID").unwrap_or_else(|_| {
        hostname::get()
            .map(|h| h.to_string_lossy().to_string())
            .unwrap_or_else(|_| "unknown".to_string())
    });
    log::info!("Scope: {}, Machine ID: {}", scope, machine_id);

    // Initialize ROS-Z context
    // Use ZENOH_ENDPOINT env var if set, otherwise use multicast scouting
    let endpoint = std::env::var("ZENOH_ENDPOINT").ok();
    let ctx = if let Some(ref ep) = endpoint {
        log::info!("Connecting to Zenoh at: {}", ep);
        Arc::new(
            ZContextBuilder::default()
                .with_json("connect/endpoints", json!([ep]))
                .build()?,
        )
    } else {
        log::info!("Using Zenoh multicast scouting for discovery");
        Arc::new(ZContextBuilder::default().build()?)
    };

    // Create vanilla zenoh session for health heartbeat
    let zenoh_session = {
        let mut c = zenoh::Config::default();
        if let Some(ref ep) = endpoint {
            c.insert_json5("connect/endpoints", &format!(r#"["{}"]"#, ep))
                .unwrap();
        }
        zenoh::open(c).await.map_err(|e| {
            Box::<dyn std::error::Error + Send + Sync>::from(format!("Zenoh session error: {}", e))
        })?
    };

    // Start health heartbeat task
    let health_topic = format!("bubbaloop/{}/{}/health/foxglove-bridge", scope, machine_id);
    let health_publisher = zenoh_session
        .declare_publisher(health_topic.clone())
        .await
        .map_err(|e| {
            Box::<dyn std::error::Error + Send + Sync>::from(format!(
                "Health publisher error: {}",
                e
            ))
        })?;
    log::info!("Health heartbeat topic: {}", health_topic);

    let mut health_shutdown_rx = shutdown_tx.subscribe();
    tokio::spawn(async move {
        let mut health_interval = tokio::time::interval(std::time::Duration::from_secs(5));
        loop {
            tokio::select! {
                biased;
                _ = health_shutdown_rx.changed() => break,
                _ = health_interval.tick() => {
                    if let Err(e) = health_publisher.put("ok").await {
                        log::warn!("Failed to publish health heartbeat: {}", e);
                    }
                }
            }
        }
    });

    // Start Foxglove WebSocket server
    log::info!("Starting Foxglove WebSocket server on port 8765...");
    let server = WebSocketServer::new().start().await?;
    log::info!("Foxglove WebSocket server started. Connect Foxglove Studio to ws://localhost:8765");

    log::info!(
        "Creating Foxglove bridge with {} topics",
        config.topics.len()
    );

    // Create a single ros-z node for the entire application
    let node = Arc::new(ctx.create_node("foxglove_bridge").build()?);

    // Create a single Foxglove bridge node that subscribes to all topics
    let foxglove_node = FoxgloveNode::new(node, &config.topics)?;

    // Run the bridge node
    foxglove_node.run(shutdown_tx).await?;

    log::info!("Shutting down Foxglove WebSocket server...");
    server.stop().wait().await;

    log::info!("All nodes shut down, exiting");

    Ok(())
}
