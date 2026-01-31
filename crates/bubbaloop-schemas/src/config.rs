use serde::{Deserialize, Serialize};
use std::path::Path;

/// Root configuration structure for topics
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TopicsConfig {
    /// List of topics to subscribe to
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_valid_yaml() {
        let yaml = "topics:\n  - /camera/cam0/compressed\n  - /weather/current\n";
        let config = TopicsConfig::parse(yaml).unwrap();
        assert_eq!(config.topics.len(), 2);
        assert_eq!(config.topics[0], "/camera/cam0/compressed");
        assert_eq!(config.topics[1], "/weather/current");
    }

    #[test]
    fn test_parse_empty_topics() {
        let yaml = "topics: []\n";
        let config = TopicsConfig::parse(yaml).unwrap();
        assert!(config.topics.is_empty());
    }

    #[test]
    fn test_parse_invalid_yaml() {
        let result = TopicsConfig::parse("not: valid: yaml: [");
        assert!(result.is_err());
        let err = result.unwrap_err();
        assert!(matches!(err, ConfigError::ParseError(_)));
    }

    #[test]
    fn test_parse_missing_topics_field() {
        let result = TopicsConfig::parse("other_field: true\n");
        assert!(result.is_err());
    }

    #[test]
    fn test_from_file_nonexistent() {
        let result = TopicsConfig::from_file("/nonexistent/path.yaml");
        assert!(result.is_err());
        let err = result.unwrap_err();
        assert!(matches!(err, ConfigError::IoError(_)));
    }
}
