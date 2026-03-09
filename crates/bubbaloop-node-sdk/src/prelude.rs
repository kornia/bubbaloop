//! Convenience re-exports for node authors.
//!
//! ```ignore
//! use bubbaloop_node_sdk::prelude::*;
//! ```

pub use crate::context::NodeContext;
pub use crate::Node;

// Category traits + runners
pub use crate::processor::{run_processor, Processor};
pub use crate::sink::{run_sink, Sink};
pub use crate::source::{run_source, Source};

// Common dependencies (so nodes don't need extra deps)
pub use anyhow::Result;
pub use async_trait::async_trait;
pub use log::{error, info, warn};
pub use serde::Deserialize;
pub use serde_json::{json, Value};
