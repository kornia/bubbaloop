use argh::FromArgs;
use mcap_recorder::recorder_node::RecorderNode;
use ros_z::{context::ZContextBuilder, Builder, Result as ZResult};
use serde_json::json;
use std::path::PathBuf;
use std::sync::Arc;

#[derive(FromArgs)]
/// MCAP recorder service for ROS-Z topics
struct Args {
    /// output directory for MCAP files (default: current directory)
    #[argh(option, short = 'o', default = "PathBuf::from(\".\")")]
    output_dir: PathBuf,
}

#[tokio::main]
async fn main() -> ZResult<()> {
    // Initialize logging
    let env = env_logger::Env::default().default_filter_or("info");
    env_logger::init_from_env(env);

    let args: Args = argh::from_env();

    log::info!("Output directory: {}", args.output_dir.display());

    // Create output directory if it doesn't exist
    if !args.output_dir.exists() {
        std::fs::create_dir_all(&args.output_dir)?;
        log::info!("Created output directory: {}", args.output_dir.display());
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

    // Initialize ROS-Z context
    let zenoh_endpoint =
        std::env::var("ZENOH_ENDPOINT").unwrap_or_else(|_| "tcp/127.0.0.1:7448".to_string());

    log::info!("Connecting to Zenoh bridge at: {}", zenoh_endpoint);

    let ctx = Arc::new(
        ZContextBuilder::default()
            .with_json("connect/endpoints", json!([zenoh_endpoint]))
            .build()?,
    );

    // Create ros-z node
    let node = Arc::new(ctx.create_node("mcap_recorder").build()?);

    // Create recorder node - waits for start/stop commands via services
    let recorder_node = RecorderNode::new(node, args.output_dir)?;

    log::info!("Recorder service started. Use 'bubbaloop record start/stop' to control recording.");

    recorder_node.run(shutdown_tx).await?;

    log::info!("Recorder service shut down, exiting");

    Ok(())
}
