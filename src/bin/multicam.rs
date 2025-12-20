use argh::FromArgs;
use bubbaloop::{config::Config, rtsp_camera_node::RtspCameraNode};
// use bubbaloop::foxglove_node::FoxgloveNode;  // Commented out for now
use ros_z::{context::ZContextBuilder, Builder, Result as ZResult};
use serde_json::json;
use std::sync::Arc;

#[derive(FromArgs)]
/// Multi-camera RTSP streaming with ROS-Z and Foxglove
struct Args {
    /// path to the configuration file
    #[argh(option, short = 'c', default = "String::from(\"config.yaml\")")]
    config: String,
}

#[tokio::main]
async fn main() -> ZResult<()> {
    // Initialize logging
    let env = env_logger::Env::default().default_filter_or("info");
    env_logger::init_from_env(env);

    let args: Args = argh::from_env();

    // Load configuration
    let config = match Config::from_file(&args.config) {
        Ok(c) => c,
        Err(e) => {
            log::error!("Failed to load config from '{}': {}", args.config, e);
            std::process::exit(1);
        }
    };

    log::info!("Loaded configuration with {} cameras", config.cameras.len());

    // Create shutdown channel
    let shutdown_tx = tokio::sync::watch::Sender::new(());

    // Set up Ctrl+C handler
    ctrlc::set_handler({
        let shutdown_tx = shutdown_tx.clone();
        move || {
            log::info!("Received Ctrl+C, shutting down gracefully...");
            shutdown_tx.send(()).ok();
        }
    })?;

    // Initialize ROS-Z context - connect to zenoh-bridge-remote-api on fixed port
    let ctx = Arc::new(
        ZContextBuilder::default()
            .with_json("connect/endpoints", json!(["tcp/127.0.0.1:7448"]))
            .build()?,
    );

    // Spawn camera nodes
    let mut tasks = Vec::new();

    for camera_config in config.cameras.iter() {
        log::info!(
            "Starting camera '{}' from {}",
            camera_config.name,
            camera_config.url
        );

        let node = match RtspCameraNode::new(ctx.clone(), camera_config.clone()) {
            Ok(n) => n,
            Err(e) => {
                log::error!(
                    "Failed to create camera node '{}': {}",
                    camera_config.name,
                    e
                );
                continue;
            }
        };

        tasks.push(tokio::spawn(node.run(shutdown_tx.clone())));
    }

    // Foxglove bridge node - commented out for now
    // log::info!("Starting Foxglove bridge node");
    // let foxglove_node = FoxgloveNode::new(ctx.clone(), &config.cameras)?;
    // tasks.push(tokio::spawn(foxglove_node.run(shutdown_tx.clone())));

    // Wait for all tasks to complete
    for task in tasks {
        if let Err(e) = task.await {
            log::error!("Task error: {}", e);
        }
    }

    log::info!("All nodes shut down, exiting");

    Ok(())
}
