use serde::{Deserialize, Serialize};
use std::path::Path;

/// Location configuration for weather data
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LocationConfig {
    /// Latitude coordinate (optional if auto_discover is true)
    pub latitude: Option<f64>,
    /// Longitude coordinate (optional if auto_discover is true)
    pub longitude: Option<f64>,
    /// Timezone (e.g., "America/New_York", "Europe/Berlin")
    /// If not specified, auto-detected by API
    #[serde(default)]
    pub timezone: Option<String>,
    /// Auto-discover location from IP address (default: true)
    #[serde(default = "default_auto_discover")]
    pub auto_discover: bool,
}

fn default_auto_discover() -> bool {
    true
}

impl Default for LocationConfig {
    fn default() -> Self {
        Self {
            latitude: None,
            longitude: None,
            timezone: None,
            auto_discover: true,
        }
    }
}

/// Weather data fetch configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FetchConfig {
    /// Polling interval for current weather in seconds (default: 60)
    #[serde(default = "default_current_interval")]
    pub current_interval_secs: u64,
    /// Polling interval for hourly forecast in seconds (default: 3600)
    #[serde(default = "default_hourly_interval")]
    pub hourly_interval_secs: u64,
    /// Polling interval for daily forecast in seconds (default: 21600)
    #[serde(default = "default_daily_interval")]
    pub daily_interval_secs: u64,
    /// Number of hourly forecast hours (default: 48, max: 384)
    #[serde(default = "default_hourly_hours")]
    pub hourly_forecast_hours: u32,
    /// Number of daily forecast days (default: 7, max: 16)
    #[serde(default = "default_daily_days")]
    pub daily_forecast_days: u32,
}

fn default_current_interval() -> u64 {
    30
}
fn default_hourly_interval() -> u64 {
    1800
}
fn default_daily_interval() -> u64 {
    10800
} // 3 hours
fn default_hourly_hours() -> u32 {
    48
}
fn default_daily_days() -> u32 {
    7
}

impl Default for FetchConfig {
    fn default() -> Self {
        Self {
            current_interval_secs: default_current_interval(),
            hourly_interval_secs: default_hourly_interval(),
            daily_interval_secs: default_daily_interval(),
            hourly_forecast_hours: default_hourly_hours(),
            daily_forecast_days: default_daily_days(),
        }
    }
}

/// Root configuration structure
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct Config {
    /// Location configuration (optional, auto-discovers if not specified)
    #[serde(default)]
    pub location: LocationConfig,
    /// Fetch configuration (optional, uses defaults if not specified)
    #[serde(default)]
    pub fetch: FetchConfig,
}

impl Config {
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
