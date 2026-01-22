//! Core trait for Bubbaloop nodes (Bubbles).

use async_trait::async_trait;
use ros_z::context::ZContext;
use serde::de::DeserializeOwned;
use std::sync::Arc;
use tokio::sync::watch;

use super::error::NodeError;

/// Metadata about a Bubble plugin.
///
/// This provides static information about the node for discovery,
/// documentation, and debugging purposes.
///
/// Use the `bubble_metadata!` macro to generate this from Cargo.toml:
/// ```rust,ignore
/// fn metadata() -> BubbleMetadata {
///     bubble_metadata!(
///         topics_published: &["/my/topic"],
///         topics_subscribed: &[],
///     )
/// }
/// ```
#[derive(Debug, Clone)]
pub struct BubbleMetadata {
    /// Short name identifier from CARGO_PKG_NAME
    pub name: &'static str,
    /// SemVer version string from CARGO_PKG_VERSION
    pub version: &'static str,
    /// Human-readable description from CARGO_PKG_DESCRIPTION
    pub description: &'static str,
    /// Topics this node publishes to
    pub topics_published: &'static [&'static str],
    /// Topics this node subscribes to
    pub topics_subscribed: &'static [&'static str],
}

/// Macro to generate BubbleMetadata from Cargo.toml manifest.
///
/// This macro extracts name, version, and description from the package's
/// Cargo.toml, so you only need to specify the topics.
///
/// # Example
/// ```rust,ignore
/// fn metadata() -> BubbleMetadata {
///     bubble_metadata!(
///         topics_published: &["/weather/current", "/weather/hourly"],
///         topics_subscribed: &["/weather/config/location"],
///     )
/// }
/// ```
#[macro_export]
macro_rules! bubble_metadata {
    (
        topics_published: $pub:expr,
        topics_subscribed: $sub:expr $(,)?
    ) => {
        $crate::plugin::BubbleMetadata {
            name: env!("CARGO_PKG_NAME"),
            version: env!("CARGO_PKG_VERSION"),
            description: env!("CARGO_PKG_DESCRIPTION"),
            topics_published: $pub,
            topics_subscribed: $sub,
        }
    };
}

/// The core trait that all Bubbaloop nodes must implement.
///
/// This trait defines the lifecycle of a Bubble:
/// 1. `metadata()` - Static information about the node
/// 2. `new()` - Construct the node with Zenoh context and configuration
/// 3. `run()` - Execute the node's main loop until shutdown signal
///
/// # Example
///
/// ```rust,ignore
/// use bubbaloop::prelude::*;
///
/// #[derive(Debug, Deserialize)]
/// struct MyConfig {
///     topic: String,
/// }
///
/// struct MyNode {
///     ctx: Arc<ZContext>,
///     config: MyConfig,
/// }
///
/// #[async_trait]
/// impl BubbleNode for MyNode {
///     type Config = MyConfig;
///
///     fn metadata() -> BubbleMetadata {
///         BubbleMetadata {
///             name: "my-node",
///             version: "0.1.0",
///             description: "My custom node",
///             topics_published: &["/my/topic"],
///             topics_subscribed: &[],
///         }
///     }
///
///     fn new(ctx: Arc<ZContext>, config: Self::Config) -> Result<Self, NodeError> {
///         Ok(Self { ctx, config })
///     }
///
///     async fn run(self, mut shutdown: watch::Receiver<()>) -> Result<(), NodeError> {
///         loop {
///             tokio::select! {
///                 _ = shutdown.changed() => {
///                     log::info!("Shutdown signal received");
///                     break;
///                 }
///                 // Your node logic here
///             }
///         }
///         Ok(())
///     }
/// }
/// ```
#[async_trait]
pub trait BubbleNode: Send + Sync + Sized {
    /// The configuration type for this node.
    /// Must be deserializable from YAML.
    type Config: DeserializeOwned + Send;

    /// Return metadata about this plugin.
    ///
    /// This is used for discovery, logging, and documentation.
    fn metadata() -> BubbleMetadata;

    /// Create a new instance of the node.
    ///
    /// # Arguments
    /// * `ctx` - The Zenoh context for pub/sub operations
    /// * `config` - The node configuration loaded from YAML
    ///
    /// # Returns
    /// The constructed node or an error if initialization fails
    fn new(ctx: Arc<ZContext>, config: Self::Config) -> Result<Self, NodeError>;

    /// Run the node's main loop.
    ///
    /// This method should run until the shutdown signal is received.
    /// Use `tokio::select!` to handle shutdown gracefully:
    ///
    /// ```rust,ignore
    /// loop {
    ///     tokio::select! {
    ///         _ = shutdown.changed() => break,
    ///         // other branches...
    ///     }
    /// }
    /// ```
    ///
    /// # Arguments
    /// * `shutdown` - Watch channel that signals when to stop
    ///
    /// # Returns
    /// Ok(()) on clean shutdown, or an error if the node failed
    async fn run(self, shutdown: watch::Receiver<()>) -> Result<(), NodeError>;
}
