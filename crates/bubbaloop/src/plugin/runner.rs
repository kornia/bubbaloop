//! Node runner utilities for standard main() implementations.

use argh::FromArgs;
use ros_z::context::ZContextBuilder;
use ros_z::Builder; // Required for .build() method
use serde_json::json;
use std::sync::Arc;
use tokio::sync::watch;

use super::config::load_config;
use super::error::NodeError;
use super::traits::BubbleNode;

/// Standard CLI arguments for Bubbaloop nodes.
#[derive(FromArgs, Debug)]
#[argh(description = "Bubbaloop node")]
pub struct NodeArgs {
    /// path to YAML configuration file
    #[argh(option, short = 'c', default = "String::from(\"config.yaml\")")]
    pub config: String,

    /// zenoh endpoint to connect to
    #[argh(option, short = 'e', default = "String::from(\"tcp/localhost:7447\")")]
    pub endpoint: String,
}

/// Initialize logging with env_logger.
///
/// Respects RUST_LOG environment variable. Defaults to "info" level.
///
/// # Example
///
/// ```rust,ignore
/// setup_logging();
/// log::info!("Node starting...");
/// ```
pub fn setup_logging() {
    env_logger::Builder::from_env(env_logger::Env::default().default_filter_or("info")).init();
}

/// Run a BubbleNode with standard CLI handling and lifecycle management.
///
/// This is the recommended way to create a node's main() function.
/// It handles:
/// - CLI argument parsing
/// - Logging setup
/// - Configuration loading
/// - Zenoh context creation
/// - Graceful shutdown on Ctrl+C
///
/// # Example
///
/// ```rust,ignore
/// use bubbaloop::prelude::*;
///
/// struct MyNode { /* ... */ }
///
/// #[async_trait]
/// impl BubbleNode for MyNode {
///     // ...
/// }
///
/// #[tokio::main]
/// async fn main() -> Result<(), Box<dyn std::error::Error>> {
///     run_node::<MyNode>().await
/// }
/// ```
pub async fn run_node<N: BubbleNode>() -> Result<(), Box<dyn std::error::Error>> {
    setup_logging();

    let args: NodeArgs = argh::from_env();
    let metadata = N::metadata();

    log::info!("Starting {} v{}", metadata.name, metadata.version);
    log::info!("  {}", metadata.description);
    log::info!("Loading config from: {}", args.config);

    let config: N::Config = load_config(&args.config)?;

    log::info!("Connecting to Zenoh at: {}", args.endpoint);
    let ctx = Arc::new(
        ZContextBuilder::default()
            .with_json("connect/endpoints", json!([args.endpoint]))
            .build()
            .map_err(|e: zenoh::Error| NodeError::Zenoh(e.to_string()))?,
    );

    // Setup shutdown signal
    let (shutdown_tx, shutdown_rx) = watch::channel(());
    ctrlc::set_handler(move || {
        log::info!("Received shutdown signal");
        let _ = shutdown_tx.send(());
    })?;

    log::info!("Node running. Press Ctrl+C to stop.");
    let node = N::new(ctx, config)?;
    node.run(shutdown_rx).await?;

    log::info!("Node stopped");
    Ok(())
}

/// Run a node with custom arguments (for advanced use cases).
///
/// Use this when you need custom CLI arguments beyond the standard set.
pub async fn run_node_with_args<N: BubbleNode>(
    config: N::Config,
    endpoint: &str,
) -> Result<(), NodeError> {
    let metadata = N::metadata();
    log::info!("Starting {} v{}", metadata.name, metadata.version);

    let ctx = Arc::new(
        ZContextBuilder::default()
            .with_json("connect/endpoints", json!([endpoint]))
            .build()
            .map_err(|e: zenoh::Error| NodeError::Zenoh(e.to_string()))?,
    );

    let (shutdown_tx, shutdown_rx) = watch::channel(());
    ctrlc::set_handler(move || {
        log::info!("Received shutdown signal");
        let _ = shutdown_tx.send(());
    })
    .map_err(|e| NodeError::Init(e.to_string()))?;

    let node = N::new(ctx, config)?;
    node.run(shutdown_rx).await
}
