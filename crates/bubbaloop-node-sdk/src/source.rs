//! Source trait — data-producing nodes that read on an interval and publish to Zenoh.
//!
//! Implement `Source` for a polling data producer (e.g., REST API, sensor).
//! The SDK handles: session setup, health heartbeat, config loading,
//! schema registration, topic construction, and the read→publish loop.

use crate::NodeContext;
use serde::de::DeserializeOwned;
use std::time::Duration;

/// Trait for data-producing nodes.
///
/// A Source reads data on a fixed interval and publishes JSON to Zenoh.
/// The SDK runs the read loop automatically; the node author only implements
/// `init()`, `read()`, and optionally `interval()`.
#[async_trait::async_trait]
pub trait Source: Send + Sync + 'static {
    /// Node-specific configuration type (deserialized from YAML).
    type Config: DeserializeOwned + Send + Sync + 'static;

    /// Human-readable node name (must match `name` in `node.yaml`).
    fn name() -> &'static str;

    /// Protobuf FileDescriptorSet bytes for schema registration.
    fn descriptor() -> &'static [u8];

    /// Initialize the source after Zenoh session is established and config is loaded.
    async fn init(ctx: &NodeContext, config: &Self::Config) -> anyhow::Result<Self>
    where
        Self: Sized;

    /// Read a single value. Return `Some(json)` to publish, `None` to skip this tick.
    async fn read(&mut self, ctx: &NodeContext) -> anyhow::Result<Option<serde_json::Value>>;

    /// Interval between reads. Override to customize (default: 1 second).
    fn interval(&self) -> Duration {
        Duration::from_secs(1)
    }
}

/// Run a Source node with the full SDK runtime.
///
/// Handles: logging, CLI args, config, Zenoh session, schema queryable,
/// health heartbeat, and the `read → publish` loop with graceful shutdown.
pub async fn run_source<S: Source>() -> anyhow::Result<()> {
    // 1. Init logging
    env_logger::Builder::from_env(env_logger::Env::default().default_filter_or("info")).init();

    // 2. Parse CLI args
    let args: crate::SdkArgs = argh::from_env();

    // 3. Load config
    let node_config: S::Config = crate::config::load_config(&args.config)?;
    log::info!(
        "{}: config loaded from {}",
        S::name(),
        args.config.display()
    );

    // 4. Resolve scope + machine_id
    let scope = std::env::var("BUBBALOOP_SCOPE").unwrap_or_else(|_| "local".to_string());
    let machine_id = std::env::var("BUBBALOOP_MACHINE_ID")
        .unwrap_or_else(|_| {
            hostname::get()
                .map(|h| h.to_string_lossy().to_string())
                .unwrap_or_else(|_| "unknown".to_string())
        })
        .replace('-', "_");

    // 5. Setup shutdown channel
    let (shutdown_tx, _) = crate::shutdown::setup_shutdown()?;

    // 6. Open Zenoh session
    let session = crate::zenoh_session::open_zenoh_session(&args.endpoint).await?;

    // 7. Declare schema queryable
    let _schema_queryable = crate::schema::declare_schema_queryable(
        &session,
        &scope,
        &machine_id,
        S::name(),
        S::descriptor(),
    )
    .await?;

    // 8. Spawn health heartbeat
    let _health_handle = crate::health::spawn_health_heartbeat(
        session.clone(),
        &scope,
        &machine_id,
        S::name(),
        shutdown_tx.subscribe(),
    )
    .await?;

    // 9. Build context
    let ctx = NodeContext {
        session: session.clone(),
        scope,
        machine_id,
        shutdown_rx: shutdown_tx.subscribe(),
    };

    // 10. Init source
    let mut source = S::init(&ctx, &node_config).await?;
    log::info!("{} source initialized", S::name());

    // 11. Read loop
    let data_topic = ctx.topic(&format!("{}/data", S::name()));
    let mut interval = tokio::time::interval(source.interval());
    let mut shutdown_rx = ctx.shutdown_rx.clone();

    loop {
        tokio::select! {
            biased;
            _ = shutdown_rx.changed() => {
                log::info!("{} source shutting down", S::name());
                break;
            }
            _ = interval.tick() => {
                match source.read(&ctx).await {
                    Ok(Some(value)) => {
                        if let Err(e) = session.put(&data_topic, value.to_string()).await {
                            log::warn!("{}: publish failed: {}", S::name(), e);
                        }
                    }
                    Ok(None) => {} // skip
                    Err(e) => {
                        log::warn!("{}: read error: {}", S::name(), e);
                    }
                }
            }
        }
    }

    log::info!("{} source shut down", S::name());
    Ok(())
}
