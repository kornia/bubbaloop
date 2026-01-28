//! CLI module for Bubbaloop commands

pub mod debug;
pub mod doctor;
pub mod node;
pub mod plugin;
pub mod status;

pub use debug::{DebugCommand, DebugError};
pub use node::{NodeCommand, NodeError};
pub use plugin::{PluginCommand, PluginError};
