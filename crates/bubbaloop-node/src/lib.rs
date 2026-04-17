//! Batteries-included SDK for writing bubbaloop nodes.
//!
//! Extracts scaffolding (Zenoh session, health heartbeat, config loading,
//! signal handling, shutdown) into a single crate. Node authors implement
//! the `Node` trait and call `run_node::<MyNode>().await`.
//!
//! # Example
//!
//! ```ignore
//! use bubbaloop_node::{Node, NodeContext};
//!
//! struct MySensor { /* ... */ }
//!
//! #[async_trait::async_trait]
//! impl Node for MySensor {
//!     type Config = MyConfig;
//!     fn name() -> &'static str { "my-sensor" }
//!     async fn init(ctx: &NodeContext, config: &MyConfig) -> anyhow::Result<Self> { /* ... */ }
//!     async fn run(self, ctx: NodeContext) -> anyhow::Result<()> { /* ... */ }
//! }
//!
//! #[tokio::main]
//! async fn main() -> anyhow::Result<()> {
//!     bubbaloop_node::run_node::<MySensor>().await
//! }
//! ```

mod config;
mod context;
pub mod discover;
pub mod envelope;
pub mod error;
pub mod get_sample;
mod health;
pub mod manifest;
pub mod publisher;
mod shutdown;
pub mod subscriber;
mod zenoh_session;

pub use context::NodeContext;
pub use discover::{discover_nodes, NodeInfo};
pub use envelope::{Envelope, Header};
pub use error::NodeError;
pub use get_sample::get_sample;
pub use manifest::{Manifest, Role, MANIFEST_SCHEMA_VERSION};
pub use publisher::{CborPublisher, CborPublisherShm, JsonPublisher, RawPublisher};
pub use subscriber::{decode_envelope_bytes, CborSubscriber, RawSubscriber};

// Re-exports so nodes don't need to add these deps directly.
pub use anyhow;
pub use async_trait;
pub use log;
pub use serde_json;
pub use tokio;
pub use zenoh;

use std::path::PathBuf;

/// Trait that node authors implement to define their node's behavior.
#[async_trait::async_trait]
pub trait Node: Send + Sync + 'static {
    /// Node-specific configuration type (deserialized from YAML).
    type Config: serde::de::DeserializeOwned + Send + Sync + 'static;

    /// Node name used for topic construction. Must match the `name` field in `node.yaml`.
    fn name() -> &'static str;

    /// Called once after Zenoh session and config are ready.
    async fn init(ctx: &NodeContext, config: &Self::Config) -> anyhow::Result<Self>
    where
        Self: Sized;

    /// Main loop. Must select on `ctx.shutdown_rx` for graceful exit.
    async fn run(self, ctx: NodeContext) -> anyhow::Result<()>;
}

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
/// Entry point for `main()`. Handles logging, CLI args, config, Zenoh session,
/// health heartbeat, and shutdown.
pub async fn run_node<N: Node>() -> anyhow::Result<()> {
    env_logger::Builder::from_env(env_logger::Env::default().default_filter_or("info")).init();

    let args: SdkArgs = argh::from_env();

    let instance_name = config::extract_name(&args.config).unwrap_or_else(|| N::name().to_string());
    let role = config::extract_role(&args.config)
        .map(|s| manifest::Role::from_str_lossy(&s))
        .unwrap_or(manifest::Role::Unknown);
    let node_config: N::Config = config::load_config(&args.config)?;
    log::info!(
        "{} (instance={}): config loaded from {}",
        N::name(),
        instance_name,
        args.config.display()
    );

    let machine_id = std::env::var("BUBBALOOP_MACHINE_ID")
        .unwrap_or_else(|_| {
            hostname::get()
                .map(|h| h.to_string_lossy().to_string())
                .unwrap_or_else(|_| "unknown".to_string())
        })
        .replace('-', "_");
    log::info!("Machine ID: {}", machine_id);

    let (shutdown_tx, _) = shutdown::setup_shutdown()?;
    let session = zenoh_session::open_zenoh_session(&args.endpoint).await?;

    let _health_handle = health::spawn_health_heartbeat(
        session.clone(),
        &machine_id,
        &instance_name,
        shutdown_tx.subscribe(),
    )
    .await?;

    let inputs = std::sync::Arc::new(std::sync::Mutex::new(
        std::collections::BTreeMap::<String, manifest::Liveness>::new(),
    ));
    let outputs = std::sync::Arc::new(std::sync::Mutex::new(
        std::collections::BTreeMap::<String, manifest::Liveness>::new(),
    ));
    let started_at_ns = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_nanos() as u64)
        .unwrap_or(0);

    let _manifest_handle = manifest::spawn_manifest_queryable(
        session.clone(),
        machine_id.clone(),
        instance_name.clone(),
        role,
        started_at_ns,
        "rust",
        inputs.clone(),
        outputs.clone(),
        shutdown_tx.subscribe(),
    )
    .await?;

    let ctx = NodeContext {
        session: session.clone(),
        machine_id,
        instance_name,
        shutdown_rx: shutdown_tx.subscribe(),
        outputs,
        inputs,
    };

    let node = N::init(&ctx, &node_config).await?;
    log::info!("{} node initialized", N::name());

    node.run(ctx).await?;

    log::info!("{} node shut down", N::name());
    Ok(())
}
