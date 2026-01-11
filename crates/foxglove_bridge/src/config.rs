use serde::{Deserialize, Serialize};
use std::path::Path;

/// Root configuration structure for Foxglove bridge topics
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TopicsConfig {
    /// List of topics to subscribe to
    /// Message type is inferred from topic name:
    /// - Topics containing "compressed" -> CompressedImage
    /// - Topics containing "raw" -> RawImage
    pub topics: Vec<String>,
}

impl TopicsConfig {
    /// Load configuration from a YAML file
    pub fn from_file(path: impl AsRef<Path>) -> Result<Self, ConfigError> {
        let contents = std::fs::read_to_string(path.as_ref())
            .map_err(|e| ConfigError::IoError(e.to_string()))?;
        Self::parse(&contents)
    }

    /// Parse configuration from a YAML string
    pub fn parse(yaml: &str) -> Result<Self, ConfigError> {
        serde_yaml::from_str(yaml).map_err(|e| ConfigError::ParseError(e.to_string()))
    }
}

/// Configuration errors
#[derive(Debug, thiserror::Error)]
pub enum ConfigError {
    #[error("IO error: {0}")]
    IoError(String),
    #[error("Parse error: {0}")]
    ParseError(String),
}
