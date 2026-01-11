use argh::FromArgs;
use foxglove::WebSocketServer;
use foxglove_bridge::{FoxgloveNode, TopicsConfig};
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

    // Initialize ROS-Z context - connect to zenoh-bridge-remote-api
    // This allows the node to receive messages from cameras
    // Default to localhost, but allow override via ZENOH_ENDPOINT environment variable
    let zenoh_endpoint =
        std::env::var("ZENOH_ENDPOINT").unwrap_or_else(|_| "tcp/127.0.0.1:7448".to_string());

    log::info!("Connecting to Zenoh bridge at: {}", zenoh_endpoint);

    let ctx = Arc::new(
        ZContextBuilder::default()
            .with_json("connect/endpoints", json!([zenoh_endpoint]))
            .build()?,
    );

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
