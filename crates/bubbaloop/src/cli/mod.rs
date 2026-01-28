//! CLI module for Bubbaloop commands

pub mod debug;
pub mod doctor;
pub mod node;
pub mod status;

pub use debug::{DebugCommand, DebugError};
pub use node::{NodeCommand, NodeError};
