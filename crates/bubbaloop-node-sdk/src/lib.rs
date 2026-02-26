//! Batteries-included SDK for writing bubbaloop nodes.
//!
//! Extracts ~300 lines of identical scaffolding (Zenoh session, health heartbeat,
//! schema queryable, config loading, signal handling, shutdown) into a single crate.
//! Node authors implement the `Node` trait and call `run_node::<MyNode>().await`.
//!
//! # Example
//!
//! ```ignore
//! use bubbaloop_node_sdk::{Node, NodeContext};
//!
//! struct MySensor { /* ... */ }
//!
//! #[async_trait::async_trait]
//! impl Node for MySensor {
//!     type Config = MyConfig;
//!     fn name() -> &'static str { "my-sensor" }
//!     fn descriptor() -> &'static [u8] { DESCRIPTOR }
//!     async fn init(ctx: &NodeContext, config: &MyConfig) -> anyhow::Result<Self> { /* ... */ }
//!     async fn run(self, ctx: NodeContext) -> anyhow::Result<()> { /* ... */ }
//! }
//!
//! #[tokio::main]
//! async fn main() -> anyhow::Result<()> {
//!     bubbaloop_node_sdk::run_node::<MySensor>().await
//! }
//! ```

mod config;
mod context;
mod health;
mod schema;
mod shutdown;
mod zenoh_session;

pub use context::NodeContext;

// Re-exports for convenience (so nodes don't need to add these deps)
pub use anyhow;
pub use async_trait;
pub use log;
pub use prost;
pub use tokio;
pub use zenoh;

use std::path::PathBuf;

/// Trait that node authors implement to define their node's behavior.
///
/// The SDK handles everything else: Zenoh session, health heartbeat,
/// schema registration, config loading, signal handling, logging.
#[async_trait::async_trait]
pub trait Node: Send + Sync + 'static {
    /// Node-specific configuration type (deserialized from YAML).
    type Config: serde::de::DeserializeOwned + Send + Sync + 'static;

    /// Human-readable node name used for topic construction and health reporting.
    /// Must match the `name` field in `node.yaml`.
    fn name() -> &'static str;

    /// Protobuf FileDescriptorSet bytes for schema registration.
    /// Typically: `include_bytes!(concat!(env!("OUT_DIR"), "/descriptor.bin"))`
    fn descriptor() -> &'static [u8];

    /// Called once after Zenoh session is established and config is loaded.
    /// Use this to create publishers, subscribers, and any node-specific state.
    async fn init(ctx: &NodeContext, config: &Self::Config) -> anyhow::Result<Self>
    where
        Self: Sized;

    /// Main loop. Called after init(). Must respect ctx.shutdown_rx for graceful exit.
    /// When the shutdown signal fires, this method should return Ok(()).
    async fn run(self, ctx: NodeContext) -> anyhow::Result<()>;
}

/// Built-in CLI arguments handled by the SDK.
#[derive(argh::FromArgs)]
#[argh(description = "Bubbaloop node")]
struct SdkArgs {
    /// path to configuration file
    #[argh(option, short = 'c', default = "default_config_path()")]
    config: PathBuf,

    /// zenoh endpoint to connect to
    #[argh(option, short = 'e')]
    endpoint: Option<String>,
}

fn default_config_path() -> PathBuf {
    PathBuf::from("config.yaml")
}

/// Run a node with the SDK runtime.
///
/// This is the single entry point that node authors call from `main()`.
/// It handles: logging init, CLI args, config load, scope/machine resolution,
/// shutdown channel, Zenoh session, schema queryable, health heartbeat.
pub async fn run_node<N: Node>() -> anyhow::Result<()> {
    // 1. Init logging
    env_logger::Builder::from_env(env_logger::Env::default().default_filter_or("info")).init();

    // 2. Parse CLI args
    let args: SdkArgs = argh::from_env();

    // 3. Load config
    let node_config: N::Config = config::load_config(&args.config)?;
    log::info!(
        "{}: config loaded from {}",
        N::name(),
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
    log::info!("Scope: {}, Machine ID: {}", scope, machine_id);

    // 5. Setup shutdown channel
    let (shutdown_tx, _shutdown_rx) = shutdown::setup_shutdown()?;

    // 6. Open Zenoh session (client mode, enforced)
    let session = zenoh_session::open_zenoh_session(&args.endpoint).await?;

    // 7. Declare schema queryable
    let _schema_queryable =
        schema::declare_schema_queryable(&session, &scope, &machine_id, N::name(), N::descriptor())
            .await?;

    // 8. Spawn health heartbeat task
    let _health_handle = health::spawn_health_heartbeat(
        session.clone(),
        &scope,
        &machine_id,
        N::name(),
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

    // 10. Init node
    let node = N::init(&ctx, &node_config).await?;
    log::info!("{} node initialized", N::name());

    // 11. Run node (blocks until shutdown)
    node.run(ctx).await?;

    log::info!("{} node shut down", N::name());
    Ok(())
}
