//! YAML Skill Loader — configure sensors with 5-line YAML files.
//!
//! Skills map a human-readable driver name to a marketplace node,
//! letting users express sensor configuration without writing Rust.
//!
//! Example skill file (`~/.bubbaloop/skills/my-camera.yaml`):
//! ```yaml
//! name: my-camera
//! driver: rtsp
//! config:
//!   url: rtsp://192.168.1.10/stream
//! ```

use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::path::Path;

/// Errors from skill loading and validation operations.
#[derive(Debug, thiserror::Error)]
pub enum SkillError {
    #[error("Skill name is invalid: {0}")]
    InvalidName(String),
    #[error("Unknown driver '{0}' — run `bubbaloop skill drivers` to see available drivers")]
    UnknownDriver(String),
    #[error("Skill file '{path}' failed to parse: {source}")]
    ParseError {
        path: String,
        source: serde_yaml::Error,
    },
    #[error("IO error reading skills directory: {0}")]
    Io(#[from] std::io::Error),
}

pub type Result<T> = std::result::Result<T, SkillError>;

/// A skill configuration parsed from a YAML file.
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct SkillConfig {
    /// Node name — must satisfy `[a-zA-Z0-9_-]{1,64}`.
    pub name: String,
    /// Driver identifier (e.g. `rtsp`, `v4l2`, `serial`).
    pub driver: String,
    /// Driver-specific key/value parameters.
    #[serde(default)]
    pub config: HashMap<String, serde_yaml::Value>,
    /// Optional cron-style schedule (Phase 4 placeholder).
    #[serde(default)]
    pub schedule: Option<String>,
    /// Declarative actions list (Phase 4 placeholder).
    #[serde(default)]
    pub actions: Vec<serde_yaml::Value>,
}

/// Maps a driver name to the corresponding marketplace node.
#[derive(Debug, Clone, Copy)]
pub struct DriverEntry {
    pub driver_name: &'static str,
    pub marketplace_node: &'static str,
    pub description: &'static str,
}

/// Built-in driver catalog.
pub static DRIVER_CATALOG: &[DriverEntry] = &[
    DriverEntry {
        driver_name: "rtsp",
        marketplace_node: "rtsp-camera",
        description: "IP cameras, NVRs",
    },
    DriverEntry {
        driver_name: "v4l2",
        marketplace_node: "v4l2-camera",
        description: "USB webcams, CSI cameras",
    },
    DriverEntry {
        driver_name: "serial",
        marketplace_node: "serial-bridge",
        description: "Arduino, UART, RS-485",
    },
    DriverEntry {
        driver_name: "gpio",
        marketplace_node: "gpio-controller",
        description: "Buttons, LEDs, relays",
    },
    DriverEntry {
        driver_name: "http-poll",
        marketplace_node: "http-sensor",
        description: "REST APIs, weather services",
    },
    DriverEntry {
        driver_name: "mqtt",
        marketplace_node: "mqtt-bridge",
        description: "Home automation, industrial",
    },
    DriverEntry {
        driver_name: "modbus",
        marketplace_node: "modbus-bridge",
        description: "Industrial IoT, PLCs",
    },
    DriverEntry {
        driver_name: "system",
        marketplace_node: "system-telemetry",
        description: "CPU, RAM, disk, temperature",
    },
];

/// Look up a driver by name in the built-in catalog.
pub fn resolve_driver(name: &str) -> Option<&'static DriverEntry> {
    DRIVER_CATALOG
        .iter()
        .find(|e| e.driver_name == name)
}

/// Validate a skill config against naming rules and the driver catalog.
pub fn validate_skill(skill: &SkillConfig) -> Result<()> {
    crate::validation::validate_node_name(&skill.name)
        .map_err(SkillError::InvalidName)?;

    if resolve_driver(&skill.driver).is_none() {
        return Err(SkillError::UnknownDriver(skill.driver.clone()));
    }

    Ok(())
}

/// Load all `*.yaml` skill files from the given directory.
///
/// Files that fail to parse are logged and skipped. An empty or missing
/// directory returns an empty `Vec` rather than an error.
pub fn load_skills(skills_dir: &Path) -> Result<Vec<SkillConfig>> {
    if !skills_dir.exists() {
        log::debug!(
            "Skills directory '{}' does not exist — no skills loaded",
            skills_dir.display()
        );
        return Ok(Vec::new());
    }

    let mut skills = Vec::new();

    for entry in std::fs::read_dir(skills_dir)? {
        let entry = entry?;
        let path = entry.path();

        if path.extension().and_then(|e| e.to_str()) != Some("yaml") {
            continue;
        }

        let raw = match std::fs::read_to_string(&path) {
            Ok(s) => s,
            Err(err) => {
                log::warn!("Failed to read skill file '{}': {}", path.display(), err);
                continue;
            }
        };

        let skill: SkillConfig = match serde_yaml::from_str(&raw) {
            Ok(s) => s,
            Err(source) => {
                let path_str = path.display().to_string();
                log::warn!("Failed to parse skill file '{}': {}", path_str, source);
                return Err(SkillError::ParseError {
                    path: path_str,
                    source,
                });
            }
        };

        match validate_skill(&skill) {
            Ok(()) => {
                log::debug!(
                    "Loaded skill '{}' (driver: {})",
                    skill.name,
                    skill.driver
                );
                skills.push(skill);
            }
            Err(err) => {
                log::warn!(
                    "Skill file '{}' failed validation: {}",
                    path.display(),
                    err
                );
            }
        }
    }

    Ok(skills)
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::tempdir;

    // ── SkillConfig YAML parsing ─────────────────────────────────────────────

    #[test]
    fn parse_full_yaml() {
        let yaml = r#"
name: my-camera
driver: rtsp
config:
  url: rtsp://192.168.1.10/stream
  fps: 30
schedule: "0 * * * *"
actions:
  - record
"#;
        let skill: SkillConfig = serde_yaml::from_str(yaml).unwrap();
        assert_eq!(skill.name, "my-camera");
        assert_eq!(skill.driver, "rtsp");
        assert_eq!(
            skill.config["url"],
            serde_yaml::Value::String("rtsp://192.168.1.10/stream".into())
        );
        assert_eq!(skill.schedule.as_deref(), Some("0 * * * *"));
        assert_eq!(skill.actions.len(), 1);
    }

    #[test]
    fn parse_minimal_yaml() {
        let yaml = "name: webcam\ndriver: v4l2\n";
        let skill: SkillConfig = serde_yaml::from_str(yaml).unwrap();
        assert_eq!(skill.name, "webcam");
        assert_eq!(skill.driver, "v4l2");
        assert!(skill.config.is_empty());
        assert!(skill.schedule.is_none());
        assert!(skill.actions.is_empty());
    }

    #[test]
    fn parse_missing_name_fails() {
        let yaml = "driver: rtsp\n";
        let result: std::result::Result<SkillConfig, _> = serde_yaml::from_str(yaml);
        assert!(result.is_err(), "expected parse error for missing name");
    }

    #[test]
    fn parse_missing_driver_fails() {
        let yaml = "name: my-camera\n";
        let result: std::result::Result<SkillConfig, _> = serde_yaml::from_str(yaml);
        assert!(result.is_err(), "expected parse error for missing driver");
    }

    // ── resolve_driver ───────────────────────────────────────────────────────

    #[test]
    fn resolve_all_builtin_drivers() {
        let cases = [
            ("rtsp", "rtsp-camera"),
            ("v4l2", "v4l2-camera"),
            ("serial", "serial-bridge"),
            ("gpio", "gpio-controller"),
            ("http-poll", "http-sensor"),
            ("mqtt", "mqtt-bridge"),
            ("modbus", "modbus-bridge"),
            ("system", "system-telemetry"),
        ];
        for (driver, node) in cases {
            let entry = resolve_driver(driver)
                .unwrap_or_else(|| panic!("driver '{}' not found in catalog", driver));
            assert_eq!(entry.marketplace_node, node);
        }
    }

    #[test]
    fn resolve_unknown_driver_returns_none() {
        assert!(resolve_driver("not-a-driver").is_none());
        assert!(resolve_driver("").is_none());
        assert!(resolve_driver("RTSP").is_none()); // case-sensitive
    }

    // ── validate_skill ───────────────────────────────────────────────────────

    #[test]
    fn validate_valid_skill() {
        let skill = SkillConfig {
            name: "my-camera".into(),
            driver: "rtsp".into(),
            config: HashMap::new(),
            schedule: None,
            actions: Vec::new(),
        };
        assert!(validate_skill(&skill).is_ok());
    }

    #[test]
    fn validate_rejects_invalid_name() {
        let skill = SkillConfig {
            name: "bad name!".into(),
            driver: "rtsp".into(),
            config: HashMap::new(),
            schedule: None,
            actions: Vec::new(),
        };
        assert!(matches!(validate_skill(&skill), Err(SkillError::InvalidName(_))));
    }

    #[test]
    fn validate_rejects_empty_name() {
        let skill = SkillConfig {
            name: "".into(),
            driver: "rtsp".into(),
            config: HashMap::new(),
            schedule: None,
            actions: Vec::new(),
        };
        assert!(matches!(validate_skill(&skill), Err(SkillError::InvalidName(_))));
    }

    #[test]
    fn validate_rejects_unknown_driver() {
        let skill = SkillConfig {
            name: "my-sensor".into(),
            driver: "unknown-driver".into(),
            config: HashMap::new(),
            schedule: None,
            actions: Vec::new(),
        };
        assert!(matches!(
            validate_skill(&skill),
            Err(SkillError::UnknownDriver(_))
        ));
    }

    // ── load_skills ──────────────────────────────────────────────────────────

    #[test]
    fn load_skills_missing_dir_returns_empty() {
        let dir = tempdir().unwrap();
        let missing = dir.path().join("nonexistent");
        let skills = load_skills(&missing).unwrap();
        assert!(skills.is_empty());
    }

    #[test]
    fn load_skills_empty_dir_returns_empty() {
        let dir = tempdir().unwrap();
        let skills = load_skills(dir.path()).unwrap();
        assert!(skills.is_empty());
    }

    #[test]
    fn load_skills_ignores_non_yaml_files() {
        let dir = tempdir().unwrap();
        std::fs::write(dir.path().join("readme.txt"), "not yaml").unwrap();
        std::fs::write(dir.path().join("config.toml"), "[section]").unwrap();
        let skills = load_skills(dir.path()).unwrap();
        assert!(skills.is_empty());
    }

    #[test]
    fn load_skills_loads_valid_files() {
        let dir = tempdir().unwrap();
        let yaml = "name: cam1\ndriver: rtsp\n";
        std::fs::write(dir.path().join("cam1.yaml"), yaml).unwrap();
        let skills = load_skills(dir.path()).unwrap();
        assert_eq!(skills.len(), 1);
        assert_eq!(skills[0].name, "cam1");
        assert_eq!(skills[0].driver, "rtsp");
    }

    #[test]
    fn load_skills_skips_invalid_validation() {
        let dir = tempdir().unwrap();
        // Invalid name (spaces not allowed) but valid YAML
        let bad = "name: \"bad name\"\ndriver: rtsp\n";
        std::fs::write(dir.path().join("bad.yaml"), bad).unwrap();
        // Valid skill
        let good = "name: good-cam\ndriver: v4l2\n";
        std::fs::write(dir.path().join("good.yaml"), good).unwrap();

        let skills = load_skills(dir.path()).unwrap();
        assert_eq!(skills.len(), 1);
        assert_eq!(skills[0].name, "good-cam");
    }

    #[test]
    fn load_skills_errors_on_malformed_yaml() {
        let dir = tempdir().unwrap();
        std::fs::write(dir.path().join("broken.yaml"), ": : : invalid").unwrap();
        let result = load_skills(dir.path());
        assert!(matches!(result, Err(SkillError::ParseError { .. })));
    }

    #[test]
    fn load_skills_loads_multiple_files() {
        let dir = tempdir().unwrap();
        for (i, driver) in ["rtsp", "v4l2", "system"].iter().enumerate() {
            let yaml = format!("name: sensor-{i}\ndriver: {driver}\n");
            std::fs::write(dir.path().join(format!("s{i}.yaml")), yaml).unwrap();
        }
        let skills = load_skills(dir.path()).unwrap();
        assert_eq!(skills.len(), 3);
    }
}
