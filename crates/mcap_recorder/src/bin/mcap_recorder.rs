use argh::FromArgs;
use bubbaloop::config::TopicsConfig;
use mcap_recorder::recorder_node::RecorderNode;
use ros_z::{context::ZContextBuilder, Builder, Result as ZResult};
use serde_json::json;
use std::path::PathBuf;
use std::sync::Arc;
use std::time::{SystemTime, UNIX_EPOCH};

#[derive(FromArgs)]
/// MCAP recorder for ROS-Z topics
struct Args {
    /// path to the topics configuration file
    #[argh(
        option,
        short = 'c',
        default = "PathBuf::from(\"crates/mcap_recorder/configs/topics.yaml\")"
    )]
    config: PathBuf,

    /// output MCAP file path (default: timestamp-based)
    #[argh(option, short = 'o')]
    output: Option<PathBuf>,
}

#[tokio::main]
async fn main() -> ZResult<()> {
    // Initialize logging
    let env = env_logger::Env::default().default_filter_or("info");
    env_logger::init_from_env(env);

    let args: Args = argh::from_env();

    // Load topics configuration
    let config = TopicsConfig::from_file(&args.config)?;
    log::info!(
        "Loaded configuration with {} topics for MCAP recorder",
        config.topics.len()
    );

    // Determine output path
    let output_path = if let Some(path) = args.output {
        path
    } else {
        // Generate timestamp-based filename
        let timestamp = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_secs();
        PathBuf::from(format!("{}.mcap", timestamp))
    };

    log::info!("Output MCAP file: {}", output_path.display());

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

    // Initialize ROS-Z context
    // Use ZENOH_ENDPOINT env var if set, otherwise use multicast scouting
    let ctx = if let Ok(endpoint) = std::env::var("ZENOH_ENDPOINT") {
        log::info!("Connecting to Zenoh at: {}", endpoint);
        Arc::new(
            ZContextBuilder::default()
                .with_json("connect/endpoints", json!([endpoint]))
                .build()?,
        )
    } else {
        log::info!("Using Zenoh multicast scouting for discovery");
        Arc::new(ZContextBuilder::default().build()?)
    };

    // Create ros-z node
    let node = Arc::new(ctx.create_node("mcap_recorder").build()?);

    // Create recorder node (returns recorder handle, actor handle, and node)
    // Recording starts automatically when the node starts
    let recorder_node = RecorderNode::new(node, &config.topics, output_path)?;

    recorder_node.run(shutdown_tx).await?;

    log::info!("All nodes shut down, exiting");

    Ok(())
}
