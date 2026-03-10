//! gpio-sensor node — Single GPIO pin sensor with Zenoh publishing.
//!
//! Each process handles one pin/sensor. For multiple sensors, register
//! multiple instances with different configs via the bubbaloop daemon.
//!
//! # Quick start
//!
//! ```bash
//! cargo build --release
//! BUBBALOOP_SCOPE=lab BUBBALOOP_MACHINE_ID=rpi4 \
//!   ./target/release/gpio_sensor_node -c configs/temperature.yaml
//! ```

mod config;
mod gpio_sensor_node;

use argh::FromArgs;
use config::Config;
use gpio_sensor_node::GpioSensorNode;
use std::path::PathBuf;
use std::sync::Arc;

/// GPIO pin sensor — reads pin state and publishes readings over Zenoh
#[derive(FromArgs)]
struct Args {
    /// path to configuration file
    #[argh(option, short = 'c', default = "default_config_path()")]
    config: PathBuf,

    /// zenoh endpoint to connect to (overridden by BUBBALOOP_ZENOH_ENDPOINT env var)
    #[argh(option, short = 'e', default = "default_endpoint()")]
    endpoint: String,
}

fn default_config_path() -> PathBuf {
    PathBuf::from("config.yaml")
}

fn default_endpoint() -> String {
    String::from("tcp/127.0.0.1:7447")
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    // Initialize logging — all output goes to stderr (never pollute stdout).
    env_logger::Builder::from_env(env_logger::Env::default().default_filter_or("info")).init();

    let args: Args = argh::from_env();

    // Load and validate configuration from YAML.
    let config = Config::from_file(&args.config).map_err(|e| {
        anyhow::anyhow!(
            "Failed to load config from '{}': {}",
            args.config.display(),
            e
        )
    })?;
    log::info!(
        "Loaded config: name='{}' pin={} type={} interval={}s",
        config.name,
        config.pin,
        config.sensor_type,
        config.interval_secs
    );

    // Create shutdown channel — tokio watch lets us fan-out the signal
    // to both the sensor loop and any future background tasks.
    let (shutdown_tx, _) = tokio::sync::watch::channel(());

    // Set up Ctrl+C / SIGTERM handler.
    {
        let shutdown_tx = shutdown_tx.clone();
        ctrlc::set_handler(move || {
            log::info!("Shutdown signal received");
            let _ = shutdown_tx.send(());
        })?;
    }

    // Resolve Zenoh endpoint — env var overrides CLI flag.
    let endpoint = std::env::var("BUBBALOOP_ZENOH_ENDPOINT").unwrap_or(args.endpoint);
    log::info!("Connecting to Zenoh at: {}", endpoint);

    // Open Zenoh session in CLIENT mode.
    // IMPORTANT: always "client" — "peer" mode bypasses the zenohd router
    // and messages from other clients would be invisible.
    let mut zenoh_config = zenoh::Config::default();
    zenoh_config
        .insert_json5("mode", r#""client""#)
        .map_err(|e| anyhow::anyhow!("Zenoh config error: {}", e))?;
    zenoh_config
        .insert_json5("connect/endpoints", &format!(r#"["{}"]"#, endpoint))
        .map_err(|e| anyhow::anyhow!("Zenoh config error: {}", e))?;

    let session = Arc::new(
        zenoh::open(zenoh_config)
            .await
            .map_err(|e| anyhow::anyhow!("Failed to open Zenoh session: {}", e))?,
    );

    // Resolve scope and machine identity from environment.
    // BUBBALOOP_SCOPE   — deployment scope (e.g. "lab", "prod", "local")
    // BUBBALOOP_MACHINE_ID — unique machine identifier (default: sanitized hostname)
    let scope =
        std::env::var("BUBBALOOP_SCOPE").unwrap_or_else(|_| "local".to_string());
    let machine_id = std::env::var("BUBBALOOP_MACHINE_ID")
        .unwrap_or_else(|_| {
            hostname::get()
                .map(|h| h.to_string_lossy().to_string())
                .unwrap_or_else(|_| "unknown".to_string())
        })
        // Hyphens in hostnames break Zenoh topic routing — replace with underscores.
        .replace('-', "_");
    log::info!("Scope: {}, Machine ID: {}", scope, machine_id);

    // Create and run the sensor node.
    let node = GpioSensorNode::new(session.clone(), config, &scope, &machine_id)
        .map_err(|e| anyhow::anyhow!("Failed to create GPIO sensor node: {}", e))?;

    log::info!("gpio-sensor node started");

    node.run(shutdown_tx.subscribe())
        .await
        .map_err(|e| anyhow::anyhow!("GPIO sensor node failed: {}", e))?;

    log::info!("gpio-sensor node stopped");
    Ok(())
}

// ---------------------------------------------------------------------------
// SDK migration note
// ---------------------------------------------------------------------------
//
// This example is self-contained (uses zenoh directly) so it builds without
// network access. To migrate to the bubbaloop-node-sdk, which handles Zenoh
// session, health heartbeat, schema queryable, and shutdown automatically:
//
// 1. Add to Cargo.toml:
//    bubbaloop-node-sdk = { git = "https://github.com/kornia/bubbaloop.git", branch = "main" }
//
// 2. Implement the `Node` trait in gpio_sensor_node.rs:
//    ```rust
//    use bubbaloop_node_sdk::{Node, NodeContext};
//
//    #[async_trait::async_trait]
//    impl Node for GpioSensorNode {
//        type Config = Config;
//        fn name() -> &'static str { "gpio-sensor" }
//        fn descriptor() -> &'static [u8] { &[] }  // no proto schema — JSON only
//        async fn init(ctx: &NodeContext, config: &Config) -> anyhow::Result<Self> {
//            GpioSensorNode::new(ctx.session.clone(), config.clone(), &ctx.scope, &ctx.machine_id)
//        }
//        async fn run(self, ctx: NodeContext) -> anyhow::Result<()> {
//            self.run(ctx.shutdown_rx).await
//        }
//    }
//    ```
//
// 3. Simplify main() to:
//    ```rust
//    #[tokio::main]
//    async fn main() -> anyhow::Result<()> {
//        bubbaloop_node_sdk::run_node::<GpioSensorNode>().await
//    }
//    ```
