//! Configuration loading utilities.

use serde::de::DeserializeOwned;
use std::path::Path;

use super::error::NodeError;

/// Load configuration from a YAML file.
///
/// # Arguments
/// * `path` - Path to the YAML configuration file
///
/// # Returns
/// The parsed configuration or an error
///
/// # Example
///
/// ```rust,ignore
/// #[derive(Debug, Deserialize)]
/// struct MyConfig {
///     topic: String,
///     interval_ms: u64,
/// }
///
/// let config: MyConfig = load_config("config.yaml")?;
/// ```
pub fn load_config<T: DeserializeOwned>(path: impl AsRef<Path>) -> Result<T, NodeError> {
    let path = path.as_ref();
    let contents = std::fs::read_to_string(path).map_err(|e| {
        NodeError::Config(format!("Failed to read {}: {}", path.display(), e))
    })?;

    serde_yaml::from_str(&contents).map_err(|e| {
        NodeError::Parse(format!("Failed to parse {}: {}", path.display(), e))
    })
}

/// Load configuration from a file, or use default if file doesn't exist.
///
/// # Arguments
/// * `path` - Path to the YAML configuration file
///
/// # Returns
/// The parsed configuration, or Default::default() if file not found
///
/// # Example
///
/// ```rust,ignore
/// #[derive(Debug, Deserialize, Default)]
/// struct MyConfig {
///     #[serde(default = "default_topic")]
///     topic: String,
/// }
///
/// fn default_topic() -> String { "/default/topic".into() }
///
/// let config: MyConfig = load_config_or_default("config.yaml")?;
/// ```
pub fn load_config_or_default<T: DeserializeOwned + Default>(
    path: impl AsRef<Path>,
) -> Result<T, NodeError> {
    let path = path.as_ref();

    if !path.exists() {
        log::info!("Config file not found, using defaults: {}", path.display());
        return Ok(T::default());
    }

    load_config(path)
}

/// Parse configuration from a YAML string.
///
/// Useful for testing or inline configuration.
pub fn parse_config<T: DeserializeOwned>(yaml: &str) -> Result<T, NodeError> {
    serde_yaml::from_str(yaml).map_err(|e| NodeError::Parse(e.to_string()))
}
