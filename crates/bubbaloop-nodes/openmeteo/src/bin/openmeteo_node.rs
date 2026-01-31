use argh::FromArgs;
use openmeteo::{config::Config, openmeteo_node::OpenMeteoNode, resolve_location};
use ros_z::{context::ZContextBuilder, Builder, Result as ZResult};
use serde_json::json;
use std::sync::Arc;

#[derive(FromArgs)]
/// Open-Meteo weather data publisher for Zenoh
struct Args {
    /// path to the configuration file (optional, uses defaults with auto-discovery)
    #[argh(option, short = 'c')]
    config: Option<String>,

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

    // Load configuration (or use defaults)
    let config = if let Some(config_path) = &args.config {
        match Config::from_file(config_path) {
            Ok(c) => c,
            Err(e) => {
                log::error!("Failed to load config from '{}': {}", config_path, e);
                std::process::exit(1);
            }
        }
    } else {
        log::info!("No config file specified, using defaults with auto-discovery");
        Config::default()
    };

    // Resolve location (auto-discover if needed)
    let resolved_location = match resolve_location(&config.location).await {
        Ok(loc) => loc,
        Err(e) => {
            log::error!("Failed to resolve location: {}", e);
            std::process::exit(1);
        }
    };

    log::info!(
        "Using location: ({:.4}, {:.4}){}",
        resolved_location.latitude,
        resolved_location.longitude,
        resolved_location
            .city
            .as_ref()
            .map(|c| format!(" - {}", c))
            .unwrap_or_default()
    );

    // Create shutdown channel
    let shutdown_tx = tokio::sync::watch::Sender::new(());

    // Set up Ctrl+C handler
    {
        let shutdown_tx = shutdown_tx.clone();
        ctrlc::set_handler(move || {
            log::info!("Received Ctrl+C, shutting down gracefully...");
            let _ = shutdown_tx.send(());
        })
        .expect("Error setting Ctrl+C handler");
    }

    // Read scope/machine env vars for health heartbeat
    let scope = std::env::var("BUBBALOOP_SCOPE").unwrap_or_else(|_| "local".to_string());
    let machine_id = std::env::var("BUBBALOOP_MACHINE_ID").unwrap_or_else(|_| {
        hostname::get()
            .map(|h| h.to_string_lossy().to_string())
            .unwrap_or_else(|_| "unknown".to_string())
    });
    log::info!("Scope: {}, Machine ID: {}", scope, machine_id);

    // Initialize ROS-Z context
    let endpoint = std::env::var("ZENOH_ENDPOINT").unwrap_or(args.zenoh_endpoint);
    log::info!("Connecting to Zenoh at: {}", endpoint);
    let ctx = Arc::new(
        ZContextBuilder::default()
            .with_json("connect/endpoints", json!([endpoint]))
            .build()?,
    );

    // Create vanilla zenoh session for health heartbeat
    let zenoh_session = {
        let mut c = zenoh::Config::default();
        c.insert_json5("connect/endpoints", &format!(r#"["{}"]"#, endpoint))
            .unwrap();
        zenoh::open(c).await.map_err(|e| {
            Box::<dyn std::error::Error + Send + Sync>::from(format!("Zenoh session error: {}", e))
        })?
    };

    // Create and run the weather node
    let node = OpenMeteoNode::new(ctx, resolved_location, config.fetch, machine_id.clone())?;
    node.run(shutdown_tx, zenoh_session, scope, machine_id)
        .await?;

    log::info!("Weather node shut down, exiting");

    Ok(())
}
