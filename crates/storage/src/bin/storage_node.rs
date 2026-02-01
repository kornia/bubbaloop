use argh::FromArgs;
use std::path::PathBuf;
use tokio::sync::watch;

use bubbaloop_storage::config::StorageConfig;

/// Bubbaloop storage service: records Zenoh messages to LanceDB.
#[derive(FromArgs)]
struct Args {
    /// path to storage configuration file
    #[argh(option, short = 'c', default = "default_config_path()")]
    config: PathBuf,

    /// zenoh endpoint to connect to
    #[argh(option, short = 'z')]
    zenoh_endpoint: Option<String>,
}

fn default_config_path() -> PathBuf {
    PathBuf::from("configs/storage.yaml")
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    env_logger::Builder::from_env(env_logger::Env::default().default_filter_or("info")).init();

    let args: Args = argh::from_env();

    log::info!("Loading config from: {}", args.config.display());
    let config = StorageConfig::load(&args.config)?;
    log::info!(
        "Storage URI: {}, Topics: {:?}",
        config.storage_uri,
        config.topics
    );

    // Zenoh session
    let mut zenoh_config = zenoh::Config::default();
    let endpoint = args.zenoh_endpoint.unwrap_or_else(|| {
        std::env::var("BUBBALOOP_ZENOH_ENDPOINT").unwrap_or_else(|_| "tcp/127.0.0.1:7447".into())
    });
    zenoh_config
        .insert_json5("connect/endpoints", &format!("[\"{endpoint}\"]"))
        .map_err(|e| anyhow::anyhow!("Failed to set zenoh endpoint: {e}"))?;
    zenoh_config
        .insert_json5("scouting/multicast/enabled", "false")
        .ok();

    log::info!("Connecting to Zenoh at {endpoint}...");
    let session = zenoh::open(zenoh_config)
        .await
        .map_err(|e| anyhow::anyhow!("Zenoh open failed: {e}"))?;
    log::info!("Zenoh session established");

    // Graceful shutdown
    let (shutdown_tx, shutdown_rx) = watch::channel(());
    ctrlc::set_handler(move || {
        log::info!("Shutdown signal received");
        let _ = shutdown_tx.send(());
    })?;

    // TODO: Phase 2+ will add LanceDB client, recording logic, and Zenoh queryable API here.
    log::info!("Storage node running. Waiting for shutdown...");

    let mut rx = shutdown_rx;
    let _ = rx.changed().await;

    log::info!("Storage node shutting down");
    session
        .close()
        .await
        .map_err(|e| anyhow::anyhow!("Zenoh close failed: {e}"))?;

    Ok(())
}
