use argh::FromArgs;
use ros_z::{context::ZContextBuilder, Builder, Result as ZResult};
use rtsp_camera::{config::Config, rtsp_camera_node::RtspCameraNode};
use serde_json::json;
use std::sync::Arc;

#[derive(FromArgs)]
/// Multi-camera RTSP streaming with ROS-Z and Foxglove
struct Args {
    /// path to the camera configuration file
    #[argh(
        option,
        short = 'c',
        default = "String::from(\"configs/config_cpu.yaml\")"
    )]
    config: String,

    /// zenoh router endpoint to connect to
    /// Default: tcp/127.0.0.1:7447 (local zenohd router)
    #[argh(option, short = 'z', default = "String::from(\"tcp/127.0.0.1:7447\")")]
    zenoh_endpoint: String,
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

    // Initialize ROS-Z context - connects to local zenohd by default
    let endpoint = std::env::var("ZENOH_ENDPOINT").unwrap_or(args.zenoh_endpoint);
    log::info!("Connecting to Zenoh at: {}", endpoint);
    let ctx = Arc::new(
        ZContextBuilder::default()
            .with_json("connect/endpoints", json!([endpoint]))
            .build()?,
    );

    // Read scope/machine env vars for health heartbeat
    let scope = std::env::var("BUBBALOOP_SCOPE").unwrap_or_else(|_| "local".to_string());
    let machine_id = std::env::var("BUBBALOOP_MACHINE_ID").unwrap_or_else(|_| {
        hostname::get()
            .map(|h| h.to_string_lossy().to_string())
            .unwrap_or_else(|_| "unknown".to_string())
    });
    log::info!("Scope: {}, Machine ID: {}", scope, machine_id);

    // Create vanilla zenoh session for health heartbeat
    let zenoh_session = {
        let mut c = zenoh::Config::default();
        c.insert_json5("connect/endpoints", &format!(r#"["{}"]"#, endpoint))
            .unwrap();
        std::sync::Arc::new(zenoh::open(c).await.map_err(|e| {
            Box::<dyn std::error::Error + Send + Sync>::from(format!("Zenoh session error: {}", e))
        })?)
    };

    // Spawn camera nodes
    let mut tasks = Vec::new();

    for camera_config in config.cameras.iter() {
        log::info!(
            "Starting camera '{}' from {}",
            camera_config.name,
            camera_config.url
        );

        match RtspCameraNode::new(ctx.clone(), camera_config.clone(), machine_id.clone()) {
            Ok(node) => {
                tasks.push(tokio::spawn(node.run(
                    shutdown_tx.clone(),
                    zenoh_session.clone(),
                    scope.clone(),
                    machine_id.clone(),
                )));
            }
            Err(e) => {
                log::error!(
                    "Failed to create camera node '{}': {}",
                    camera_config.name,
                    e
                );
                continue;
            }
        };
    }

    // Wait for all tasks to complete
    for task in tasks {
        task.await??;
    }

    log::info!("All nodes shut down, exiting");

    Ok(())
}
