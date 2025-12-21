use serde::{Deserialize, Serialize};
use std::path::Path;

/// Decoder backend selection for raw frame decoding
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum DecoderBackend {
    /// Software decoding using avdec_h264 (CPU, always available)
    #[default]
    Software,
    /// Hardware decoding using nvh264dec (NVIDIA desktop GPU)
    Nvidia,
    /// Hardware decoding using nvv4l2decoder (NVIDIA Jetson)
    Jetson,
}

impl From<DecoderBackend> for crate::h264_decode::DecoderBackend {
    fn from(config: DecoderBackend) -> Self {
        match config {
            DecoderBackend::Software => crate::h264_decode::DecoderBackend::Software,
            DecoderBackend::Nvidia => crate::h264_decode::DecoderBackend::Nvidia,
            DecoderBackend::Jetson => crate::h264_decode::DecoderBackend::Jetson,
        }
    }
}

/// Configuration for raw frame publishing
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct RawConfig {
    /// Enable raw frame publishing (decoded RGB frames)
    #[serde(default)]
    pub enabled: bool,
    /// Decoder backend to use
    #[serde(default)]
    pub decoder: DecoderBackend,
}

/// Configuration for a single RTSP camera
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CameraConfig {
    /// Unique name for the camera (used in topic names)
    pub name: String,
    /// RTSP URL (e.g., rtsp://user:pass@192.168.1.10:554/stream)
    pub url: String,
    /// Latency in milliseconds for the RTSP stream
    #[serde(default = "default_latency")]
    pub latency: u32,
    /// Raw frame publishing configuration
    #[serde(default)]
    pub raw: RawConfig,
}

fn default_latency() -> u32 {
    200
}

/// Root configuration structure
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Config {
    /// List of cameras to capture
    pub cameras: Vec<CameraConfig>,
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
    #[error("Validation error: {0}")]
    ValidationError(String),
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_config() {
        let yaml = r#"
cameras:
  - name: "front"
    url: "rtsp://192.168.1.10:554/stream"
    latency: 200
  - name: "rear"
    url: "rtsp://192.168.1.11:554/live"
"#;
        let config = Config::parse(yaml).unwrap();
        assert_eq!(config.cameras.len(), 2);
        assert_eq!(config.cameras[0].name, "front");
        assert_eq!(config.cameras[0].latency, 200);
        assert_eq!(config.cameras[1].latency, 200); // default
                                                    // Raw config defaults
        assert!(!config.cameras[0].raw.enabled);
        assert_eq!(config.cameras[0].raw.decoder, DecoderBackend::Software);
    }

    #[test]
    fn test_parse_config_with_raw() {
        let yaml = r#"
cameras:
  - name: "front"
    url: "rtsp://192.168.1.10:554/stream"
    raw:
      enabled: true
      decoder: nvidia
"#;
        let config = Config::parse(yaml).unwrap();
        assert!(config.cameras[0].raw.enabled);
        assert_eq!(config.cameras[0].raw.decoder, DecoderBackend::Nvidia);
    }

    #[test]
    fn test_parse_config_with_jetson() {
        let yaml = r#"
cameras:
  - name: "front"
    url: "rtsp://192.168.1.10:554/stream"
    raw:
      enabled: true
      decoder: jetson
"#;
        let config = Config::parse(yaml).unwrap();
        assert!(config.cameras[0].raw.enabled);
        assert_eq!(config.cameras[0].raw.decoder, DecoderBackend::Jetson);
    }
}
