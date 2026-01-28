//! CLI module for Bubbaloop commands

pub mod node;
pub mod plugin;
pub mod status;

pub use node::{NodeCommand, NodeError};
pub use plugin::{PluginCommand, PluginError};
