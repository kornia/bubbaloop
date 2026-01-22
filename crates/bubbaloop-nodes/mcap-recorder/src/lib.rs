//! MCAP recorder node for Bubbaloop.
//!
//! This node subscribes to topics and records them to MCAP format files
//! for later playback and analysis.

mod node;

pub use node::{Config, McapRecorderNode, RecorderNode};
