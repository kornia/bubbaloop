//! Bubbaloop Plugin SDK
//!
//! This module provides the foundational traits and utilities for building
//! Bubbaloop nodes (Bubbles). It standardizes the node lifecycle, configuration
//! loading, and Zenoh integration patterns.
//!
//! # Quick Start
//!
//! ```rust,ignore
//! use bubbaloop::prelude::*;
//!
//! #[derive(Debug, Deserialize)]
//! struct MyConfig {
//!     topic: String,
//!     interval_ms: u64,
//! }
//!
//! struct MyNode {
//!     ctx: Arc<ZContext>,
//!     config: MyConfig,
//! }
//!
//! #[async_trait]
//! impl BubbleNode for MyNode {
//!     type Config = MyConfig;
//!
//!     fn metadata() -> BubbleMetadata {
//!         BubbleMetadata {
//!             name: "my-node",
//!             version: "0.1.0",
//!             description: "Example node",
//!             topics_published: &["/my/topic"],
//!             topics_subscribed: &[],
//!         }
//!     }
//!
//!     fn new(ctx: Arc<ZContext>, config: Self::Config) -> Result<Self, NodeError> {
//!         Ok(Self { ctx, config })
//!     }
//!
//!     async fn run(self, mut shutdown: watch::Receiver<()>) -> Result<(), NodeError> {
//!         loop {
//!             tokio::select! {
//!                 _ = shutdown.changed() => break,
//!                 _ = tokio::time::sleep(Duration::from_millis(self.config.interval_ms)) => {
//!                     // Do work...
//!                 }
//!             }
//!         }
//!         Ok(())
//!     }
//! }
//!
//! #[tokio::main]
//! async fn main() -> Result<(), Box<dyn std::error::Error>> {
//!     run_node::<MyNode>().await
//! }
//! ```

mod config;
mod error;
mod runner;
mod traits;

pub use config::{load_config, load_config_or_default, parse_config};
pub use error::NodeError;
pub use runner::{run_node, run_node_with_args, setup_logging, NodeArgs};
pub use traits::{BubbleMetadata, BubbleNode};
