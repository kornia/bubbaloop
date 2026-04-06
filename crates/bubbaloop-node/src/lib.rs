//! Batteries-included SDK for writing bubbaloop nodes.
//!
//! Extracts ~300 lines of identical scaffolding (Zenoh session, health heartbeat,
//! schema queryable, config loading, signal handling, shutdown) into a single crate.
//! Node authors implement the `Node` trait and call `run_node::<MyNode>().await`.
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
//!     fn descriptor() -> &'static [u8] { DESCRIPTOR }
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
pub mod error;
pub mod get_sample;
mod health;
pub mod proto_decoder;
pub mod publisher;
mod schema;
pub mod schemas;
mod shutdown;
pub mod subscriber;
mod zenoh_session;

pub use context::NodeContext;
pub use discover::{discover_nodes, NodeInfo};
pub use error::NodeError;
pub use get_sample::get_sample;
pub use proto_decoder::ProtoDecoder;
pub use publisher::{JsonPublisher, ProtoPublisher, ShmPublisher};
pub use subscriber::{RawSubscriber, TypedSubscriber};

pub use schemas::MessageTypeName;

// Re-exports so nodes don't need to add these deps directly.
pub use anyhow;
pub use async_trait;
pub use log;
pub use prost;
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

    /// Protobuf `FileDescriptorSet` bytes for schema registration.
    fn descriptor() -> &'static [u8];

    /// Called once after Zenoh session and config are ready.
    async fn init(ctx: &NodeContext, config: &Self::Config) -> anyhow::Result<Self>
    where
        Self: Sized;

    /// Main loop. Must select on `ctx.shutdown_rx` for graceful exit.
    async fn run(self, ctx: NodeContext) -> anyhow::Result<()>;

    /// Return true if this node requires a Zenoh SHM-enabled session.
    /// Default: false. Override to true for nodes that publish/subscribe over SHM.
    fn shm() -> bool {
        false
    }
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
/// schema queryable, health heartbeat, and shutdown.
pub async fn run_node<N: Node>() -> anyhow::Result<()> {
    env_logger::Builder::from_env(env_logger::Env::default().default_filter_or("info")).init();

    let args: SdkArgs = argh::from_env();

    let instance_name = config::extract_name(&args.config).unwrap_or_else(|| N::name().to_string());
    let node_config: N::Config = config::load_config(&args.config)?;
    log::info!(
        "{} (instance={}): config loaded from {}",
        N::name(),
        instance_name,
        args.config.display()
    );

    let scope = std::env::var("BUBBALOOP_SCOPE").unwrap_or_else(|_| "local".to_string());
    let machine_id = std::env::var("BUBBALOOP_MACHINE_ID")
        .unwrap_or_else(|_| {
            hostname::get()
                .map(|h| h.to_string_lossy().to_string())
                .unwrap_or_else(|_| "unknown".to_string())
        })
        .replace('-', "_");
    log::info!("Scope: {}, Machine ID: {}", scope, machine_id);

    let (shutdown_tx, _) = shutdown::setup_shutdown()?;
    let session = zenoh_session::open_zenoh_session(&args.endpoint, N::shm()).await?;

    let _schema_queryable = schema::declare_schema_queryable(
        &session,
        &scope,
        &machine_id,
        &instance_name,
        N::descriptor(),
    )
    .await?;

    let _health_handle = health::spawn_health_heartbeat(
        session.clone(),
        &scope,
        &machine_id,
        &instance_name,
        shutdown_tx.subscribe(),
    )
    .await?;

    let ctx = NodeContext {
        session: session.clone(),
        scope,
        machine_id,
        instance_name,
        shutdown_rx: shutdown_tx.subscribe(),
    };

    let node = N::init(&ctx, &node_config).await?;
    log::info!("{} node initialized", N::name());

    node.run(ctx).await?;

    log::info!("{} node shut down", N::name());
    Ok(())
}
