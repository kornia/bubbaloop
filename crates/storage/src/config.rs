use serde::Deserialize;
use std::collections::HashMap;
use std::path::Path;

use crate::error::{Result, StorageError};

/// Storage service configuration.
#[derive(Debug, Clone, Deserialize)]
pub struct StorageConfig {
    /// LanceDB storage URI: local path or `gs://bucket/path` for GCS.
    pub storage_uri: String,

    /// Zenoh topic patterns to subscribe to (supports `**` wildcards).
    pub topics: Vec<String>,

    /// Optional mapping of topic patterns to fully qualified protobuf type names.
    /// Used for schema registration when ros-z type detection is unavailable.
    #[serde(default)]
    pub schema_hints: HashMap<String, String>,

    /// Number of messages to batch before flushing to LanceDB.
    #[serde(default = "default_batch_size")]
    pub batch_size: usize,

    /// Maximum seconds between flushes, even if batch is not full.
    #[serde(default = "default_flush_interval")]
    pub flush_interval_secs: u64,
}

fn default_batch_size() -> usize {
    30
}

fn default_flush_interval() -> u64 {
    5
}

impl StorageConfig {
    /// Load configuration from a YAML file.
    pub fn load(path: &Path) -> Result<Self> {
        let content = std::fs::read_to_string(path)
            .map_err(|e| StorageError::Config(format!("Failed to read config: {e}")))?;
        serde_yaml::from_str(&content)
            .map_err(|e| StorageError::Config(format!("Failed to parse config: {e}")))
    }

    /// Find the message type for a topic using schema_hints.
    /// Returns "unknown" if no hint matches.
    pub fn message_type_for_topic(&self, topic: &str) -> String {
        for (pattern, type_name) in &self.schema_hints {
            if topic_matches_pattern(topic, pattern) {
                return type_name.clone();
            }
        }
        "unknown".to_string()
    }
}

/// Simple topic pattern matching with `*` (single segment) and `**` (multi-segment).
fn topic_matches_pattern(topic: &str, pattern: &str) -> bool {
    let topic_parts: Vec<&str> = topic.split('/').filter(|s| !s.is_empty()).collect();
    let pattern_parts: Vec<&str> = pattern.split('/').filter(|s| !s.is_empty()).collect();
    matches_parts(&topic_parts, &pattern_parts)
}

fn matches_parts(topic: &[&str], pattern: &[&str]) -> bool {
    if pattern.is_empty() {
        return topic.is_empty();
    }
    if pattern[0] == "**" {
        // ** matches zero or more segments
        for i in 0..=topic.len() {
            if matches_parts(&topic[i..], &pattern[1..]) {
                return true;
            }
        }
        return false;
    }
    if topic.is_empty() {
        return false;
    }
    if pattern[0] == "*" || pattern[0] == topic[0] {
        return matches_parts(&topic[1..], &pattern[1..]);
    }
    false
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_topic_matches_exact() {
        assert!(topic_matches_pattern("/weather/current", "/weather/current"));
        assert!(!topic_matches_pattern("/weather/current", "/weather/hourly"));
    }

    #[test]
    fn test_topic_matches_single_wildcard() {
        assert!(topic_matches_pattern(
            "/camera/entrance/compressed",
            "/camera/*/compressed"
        ));
        assert!(!topic_matches_pattern(
            "/camera/entrance/raw",
            "/camera/*/compressed"
        ));
    }

    #[test]
    fn test_topic_matches_double_wildcard() {
        assert!(topic_matches_pattern("/camera/entrance/compressed", "/camera/**"));
        assert!(topic_matches_pattern("/camera/a/b/c", "/camera/**"));
        assert!(!topic_matches_pattern("/weather/current", "/camera/**"));
    }

    #[test]
    fn test_message_type_for_topic() {
        let config = StorageConfig {
            storage_uri: "./test".into(),
            topics: vec![],
            schema_hints: HashMap::from([
                (
                    "/camera/*/compressed".into(),
                    "bubbaloop.camera.v1.CompressedImage".into(),
                ),
                (
                    "/weather/current".into(),
                    "bubbaloop.weather.v1.CurrentWeather".into(),
                ),
            ]),
            batch_size: 30,
            flush_interval_secs: 5,
        };

        assert_eq!(
            config.message_type_for_topic("/camera/entrance/compressed"),
            "bubbaloop.camera.v1.CompressedImage"
        );
        assert_eq!(
            config.message_type_for_topic("/weather/current"),
            "bubbaloop.weather.v1.CurrentWeather"
        );
        assert_eq!(
            config.message_type_for_topic("/lidar/front/pointcloud"),
            "unknown"
        );
    }

    #[test]
    fn test_load_default_config() {
        let yaml = r#"
storage_uri: "./recordings"
topics:
  - "/camera/**"
  - "/weather/**"
"#;
        let config: StorageConfig = serde_yaml::from_str(yaml).unwrap();
        assert_eq!(config.storage_uri, "./recordings");
        assert_eq!(config.topics.len(), 2);
        assert_eq!(config.batch_size, 30);
        assert_eq!(config.flush_interval_secs, 5);
        assert!(config.schema_hints.is_empty());
    }

    #[test]
    fn test_load_full_config() {
        let yaml = r#"
storage_uri: "gs://my-bucket/recordings"
topics:
  - "/camera/**"
  - "/weather/**"
batch_size: 100
flush_interval_secs: 10
schema_hints:
  "/camera/*/compressed": "bubbaloop.camera.v1.CompressedImage"
  "/weather/current": "bubbaloop.weather.v1.CurrentWeather"
"#;
        let config: StorageConfig = serde_yaml::from_str(yaml).unwrap();
        assert_eq!(config.storage_uri, "gs://my-bucket/recordings");
        assert_eq!(config.batch_size, 100);
        assert_eq!(config.flush_interval_secs, 10);
        assert_eq!(config.schema_hints.len(), 2);
    }

    #[test]
    fn test_double_wildcard_matches_root() {
        assert!(topic_matches_pattern("/a/b/c", "/**"));
        assert!(topic_matches_pattern("/a", "/**"));
    }

    #[test]
    fn test_double_wildcard_matches_zero_segments() {
        // ** at the end matches zero additional segments
        assert!(topic_matches_pattern("/camera", "/camera/**"));
    }

    #[test]
    fn test_no_match_empty_topic() {
        assert!(!topic_matches_pattern("", "/camera/**"));
    }
}
