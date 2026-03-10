//! Configuration for the GPIO sensor node.

use serde::{Deserialize, Serialize};
use std::path::Path;

/// Sensor type — determines how raw pin readings are interpreted and labelled.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum SensorType {
    /// Raw HIGH/LOW state. Publishes `value: 0.0 | 1.0`, `unit: "bool"`.
    #[default]
    Digital,
    /// Voltage/ADC reading (0.0–1.0 normalized). Publishes `value: <float>`, `unit: "normalized"`.
    Analog,
    /// Temperature sensor (simulated: Gaussian noise around 22°C). Publishes `unit: "celsius"`.
    Temperature,
    /// PIR or edge-triggered motion detector. Publishes `value: 0.0 | 1.0`, `unit: "bool"`.
    Motion,
}

impl SensorType {
    pub fn unit(&self) -> &'static str {
        match self {
            SensorType::Digital => "bool",
            SensorType::Analog => "normalized",
            SensorType::Temperature => "celsius",
            SensorType::Motion => "bool",
        }
    }
}

impl std::fmt::Display for SensorType {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            SensorType::Digital => write!(f, "digital"),
            SensorType::Analog => write!(f, "analog"),
            SensorType::Temperature => write!(f, "temperature"),
            SensorType::Motion => write!(f, "motion"),
        }
    }
}

/// Configuration for a single GPIO sensor instance.
///
/// Load from YAML: `Config::from_file("configs/temperature.yaml")`
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Config {
    /// Unique name for this sensor instance (used in health topic and logs).
    pub name: String,

    /// Zenoh topic suffix for publishing readings.
    /// Full topic: `bubbaloop/{scope}/{machine_id}/{publish_topic}`
    pub publish_topic: String,

    /// GPIO pin number (BCM numbering on Raspberry Pi, 0–40).
    pub pin: u32,

    /// How to interpret the pin reading.
    #[serde(default)]
    pub sensor_type: SensorType,

    /// How often to sample the pin and publish (seconds).
    #[serde(default = "default_interval_secs")]
    pub interval_secs: f64,

    /// Logical polarity: if true, a HIGH pin means "active/detected".
    #[serde(default = "default_active_high")]
    pub active_high: bool,
}

fn default_interval_secs() -> f64 {
    1.0
}

fn default_active_high() -> bool {
    true
}

/// Errors that can occur when loading or validating the config.
#[derive(Debug, thiserror::Error)]
pub enum ConfigError {
    #[error("IO error: {0}")]
    IoError(String),
    #[error("Parse error: {0}")]
    ParseError(String),
    #[error("Validation error: {0}")]
    ValidationError(String),
}

impl Config {
    /// Load and validate configuration from a YAML file.
    pub fn from_file(path: impl AsRef<Path>) -> Result<Self, ConfigError> {
        let contents = std::fs::read_to_string(path.as_ref())
            .map_err(|e| ConfigError::IoError(e.to_string()))?;
        Self::parse(&contents)
    }

    /// Parse and validate configuration from a YAML string.
    pub fn parse(yaml: &str) -> Result<Self, ConfigError> {
        let config: Config =
            serde_yaml::from_str(yaml).map_err(|e| ConfigError::ParseError(e.to_string()))?;
        config.validate()?;
        Ok(config)
    }

    /// Validate configuration values.
    pub fn validate(&self) -> Result<(), ConfigError> {
        if self.name.is_empty() {
            return Err(ConfigError::ValidationError(
                "name must not be empty".to_string(),
            ));
        }
        if self.name.len() > 64 {
            return Err(ConfigError::ValidationError(format!(
                "name '{}' is too long (max 64 chars)",
                self.name
            )));
        }
        let name_ok = self
            .name
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || c == '_' || c == '-' || c == '.');
        if !name_ok {
            return Err(ConfigError::ValidationError(format!(
                "name '{}' contains invalid characters (must match [a-zA-Z0-9_\\-\\.]+)",
                self.name
            )));
        }

        if self.publish_topic.is_empty() {
            return Err(ConfigError::ValidationError(
                "publish_topic must not be empty".to_string(),
            ));
        }
        let topic_ok = self
            .publish_topic
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || c == '/' || c == '_' || c == '-' || c == '.');
        if !topic_ok {
            return Err(ConfigError::ValidationError(format!(
                "publish_topic '{}' contains invalid characters (must match [a-zA-Z0-9/_\\-\\.]+)",
                self.publish_topic
            )));
        }

        // RPi BCM pins go up to 27, but allow up to 40 for future/other boards.
        if self.pin > 40 {
            return Err(ConfigError::ValidationError(format!(
                "pin {} out of range (0–40 supported)",
                self.pin
            )));
        }

        // Guard against unreasonably fast or slow polling.
        if self.interval_secs < 0.01 || self.interval_secs > 3600.0 {
            return Err(ConfigError::ValidationError(format!(
                "interval_secs {} out of range (0.01–3600)",
                self.interval_secs
            )));
        }

        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_temperature_config() -> Result<(), ConfigError> {
        let yaml = r#"
name: temp-gpio-4
publish_topic: gpio/temperature/reading
pin: 4
sensor_type: temperature
interval_secs: 5.0
active_high: true
"#;
        let config = Config::parse(yaml)?;
        assert_eq!(config.name, "temp-gpio-4");
        assert_eq!(config.pin, 4);
        assert_eq!(config.sensor_type, SensorType::Temperature);
        assert!((config.interval_secs - 5.0).abs() < f64::EPSILON);
        assert!(config.active_high);
        Ok(())
    }

    #[test]
    fn test_parse_motion_config() -> Result<(), ConfigError> {
        let yaml = r#"
name: motion-pir-17
publish_topic: gpio/motion/reading
pin: 17
sensor_type: motion
interval_secs: 0.1
active_high: true
"#;
        let config = Config::parse(yaml)?;
        assert_eq!(config.sensor_type, SensorType::Motion);
        assert_eq!(config.pin, 17);
        Ok(())
    }

    #[test]
    fn test_defaults() -> Result<(), ConfigError> {
        let yaml = r#"
name: my-sensor
publish_topic: gpio/test/reading
pin: 5
"#;
        let config = Config::parse(yaml)?;
        assert_eq!(config.sensor_type, SensorType::Digital);
        assert!((config.interval_secs - 1.0).abs() < f64::EPSILON);
        assert!(config.active_high);
        Ok(())
    }

    #[test]
    fn test_invalid_pin() {
        let yaml = r#"
name: bad-pin
publish_topic: gpio/test/reading
pin: 99
"#;
        assert!(Config::parse(yaml).is_err());
    }

    #[test]
    fn test_invalid_interval_too_fast() {
        let yaml = r#"
name: too-fast
publish_topic: gpio/test/reading
pin: 4
interval_secs: 0.001
"#;
        assert!(Config::parse(yaml).is_err());
    }

    #[test]
    fn test_invalid_name_spaces() {
        let yaml = r#"
name: "bad name"
publish_topic: gpio/test/reading
pin: 4
"#;
        assert!(Config::parse(yaml).is_err());
    }

    #[test]
    fn test_invalid_topic_chars() {
        let yaml = r#"
name: sensor
publish_topic: "gpio/test/bad topic!"
pin: 4
"#;
        assert!(Config::parse(yaml).is_err());
    }

    #[test]
    fn test_sensor_type_unit() {
        assert_eq!(SensorType::Temperature.unit(), "celsius");
        assert_eq!(SensorType::Digital.unit(), "bool");
        assert_eq!(SensorType::Analog.unit(), "normalized");
        assert_eq!(SensorType::Motion.unit(), "bool");
    }
}
