//! Error types for Bubbaloop nodes.

use thiserror::Error;

/// Errors that can occur during node operation.
#[derive(Debug, Error)]
pub enum NodeError {
    /// Configuration file not found or unreadable
    #[error("Config error: {0}")]
    Config(String),

    /// Failed to parse configuration YAML
    #[error("Parse error: {0}")]
    Parse(String),

    /// Zenoh/ROS-Z communication error
    #[error("Zenoh error: {0}")]
    Zenoh(String),

    /// Node initialization failed
    #[error("Init error: {0}")]
    Init(String),

    /// Runtime error during node execution
    #[error("Runtime error: {0}")]
    Runtime(String),

    /// Generic I/O error
    #[error("IO error: {0}")]
    Io(#[from] std::io::Error),
}

impl From<serde_yaml::Error> for NodeError {
    fn from(err: serde_yaml::Error) -> Self {
        NodeError::Parse(err.to_string())
    }
}

impl From<zenoh::Error> for NodeError {
    fn from(err: zenoh::Error) -> Self {
        NodeError::Zenoh(err.to_string())
    }
}
