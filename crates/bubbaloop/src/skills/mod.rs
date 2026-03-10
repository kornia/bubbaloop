//! YAML Skill Loader — configure sensors with 5-line YAML files.
//!
//! Skills map a human-readable driver name to a marketplace node,
//! letting users express sensor configuration without writing Rust.
//!
//! Example v2 skill file (`~/.bubbaloop/skills/my-camera.yaml`):
//! ```yaml
//! name: my-camera
//! driver: rtsp
//! config:
//!   url: rtsp://192.168.1.10/stream
//! ```
//!
//! Example v3 pipeline skill file:
//! ```yaml
//! name: weather-pipeline
//! operators:
//!   - id: fetch
//!     driver: http-poll
//!     config:
//!       url: https://api.open-meteo.com/v1/forecast
//!     outputs:
//!       data:
//!         topic: weather/raw
//!         rate: 1fps
//! links:
//!   - from: fetch/data
//!     to: dashboard
//! on:
//!   - trigger: fetch/error
//!     action: log
//!     message: "Fetch failed"
//!     cooldown: 5m
//! ```

pub mod builtin;
pub mod resolve;
pub mod runtime;

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
    #[error("Skill format error: {0}")]
    InvalidFormat(String),
    #[error("Skill file '{path}' failed to parse: {source}")]
    ParseError {
        path: String,
        source: serde_yaml::Error,
    },
    #[error("IO error reading skills directory: {0}")]
    Io(#[from] std::io::Error),
}

pub type Result<T> = std::result::Result<T, SkillError>;

/// Output port definition on an operator.
#[derive(Debug, Clone, Default, Deserialize, Serialize)]
pub struct OutputDef {
    /// Override topic name. Defaults to `bubbaloop/{scope}/{machine_id}/{skill_name}/{output_id}`.
    #[serde(default)]
    pub topic: Option<String>,
    /// Rate limit: "30fps", "1fps", "5s", etc.
    #[serde(default)]
    pub rate: Option<String>,
}

/// A stage in a v3 pipeline.
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct OperatorDef {
    /// Unique ID within this pipeline (used in links: `from: op_id/output`).
    pub id: String,
    /// Driver identifier (e.g. `http-poll`, `rtsp`).
    pub driver: String,
    /// Driver-specific key/value parameters.
    #[serde(default)]
    pub config: HashMap<String, serde_yaml::Value>,
    /// Named output ports with optional topic/rate overrides.
    #[serde(default)]
    pub outputs: HashMap<String, OutputDef>,
}

/// A link connecting operator outputs in a pipeline.
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct LinkDef {
    /// Source: `"op_id/output"` or `"op_id"` (implicit single output).
    pub from: String,
    /// Sink: `"op_id"` or well-known sinks `"dashboard"`, `"log"`, `"null"`.
    pub to: String,
}

/// Declarative event handler (replaces Vec<serde_yaml::Value>).
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct EventHandler {
    /// Event source: `"{op_id}/{event_name}"` or legacy trigger name.
    pub trigger: String,
    /// Optional simple predicate: `"value > 80"`.
    #[serde(default)]
    pub condition: Option<String>,
    /// Action: `"notify"` | `"log"` | `"agent.wake"` | `"zenoh.publish"`.
    pub action: String,
    /// Human-readable message template.
    #[serde(default)]
    pub message: Option<String>,
    /// Minimum time between firings: `"5m"`, `"30s"`.
    #[serde(default)]
    pub cooldown: Option<String>,
    /// Only fire during this time window: `"23:00-06:00"`.
    #[serde(default)]
    pub between: Option<String>,
}

/// A skill configuration parsed from a YAML file.
///
/// Supports two formats:
/// - **v2** (single-driver): `driver:` is set, `operators:` is empty.
/// - **v3** (pipeline): `operators:` is non-empty, `driver:` is absent.
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct SkillConfig {
    /// Node name — must satisfy `[a-zA-Z0-9_-]{1,64}`.
    pub name: String,
    /// Driver identifier (v2 format only, e.g. `rtsp`, `v4l2`, `serial`).
    /// Absent in v3 pipeline format.
    #[serde(default)]
    pub driver: Option<String>,
    /// Whether this skill is active. Defaults to true; set false to skip on `bubbaloop up`.
    #[serde(default = "default_enabled")]
    pub enabled: bool,
    /// Driver-specific key/value parameters (v2 only; v3 puts config per-operator).
    #[serde(default)]
    pub config: HashMap<String, serde_yaml::Value>,
    /// Free-form agent-readable intent declaration (what should the agent do with this data).
    #[serde(default)]
    pub intent: String,
    /// Declarative event handlers (agent-executed).
    #[serde(default)]
    pub on: Vec<EventHandler>,
    /// Optional cron-style schedule (Phase 4 placeholder).
    #[serde(default)]
    pub schedule: Option<String>,
    /// Declarative actions list (Phase 4 placeholder).
    #[serde(default)]
    pub actions: Vec<serde_yaml::Value>,
    // ── v3 pipeline fields ────────────────────────────────────────────────────
    /// Pipeline operators (v3 format only).
    #[serde(default)]
    pub operators: Vec<OperatorDef>,
    /// Data-flow links between operators (v3 format only).
    #[serde(default)]
    pub links: Vec<LinkDef>,
    /// Shared variables available to all operators in this pipeline.
    #[serde(default)]
    pub vars: HashMap<String, String>,
}

impl SkillConfig {
    /// Returns the operators list.
    ///
    /// For v3 skills this is the `operators` field directly.
    /// For v2 skills (single `driver:`), this auto-wraps into a single-operator list
    /// so callers can treat both formats uniformly.
    pub fn to_operators(&self) -> Vec<OperatorDef> {
        if !self.operators.is_empty() {
            self.operators.clone()
        } else if let Some(ref driver) = self.driver {
            vec![OperatorDef {
                id: self.name.clone(),
                driver: driver.clone(),
                config: self.config.clone(),
                outputs: HashMap::new(),
            }]
        } else {
            vec![]
        }
    }
}

fn default_enabled() -> bool {
    true
}

/// Whether a driver runs built-in inside the daemon or requires a marketplace binary.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DriverKind {
    /// Runs as a tokio task inside the daemon — no binary download needed.
    BuiltIn,
    /// Requires downloading a precompiled binary from the marketplace.
    Marketplace,
}

/// Maps a driver name to the corresponding marketplace node.
#[derive(Debug, Clone, Copy)]
pub struct DriverEntry {
    pub driver_name: &'static str,
    pub kind: DriverKind,
    /// Marketplace node name. None for BuiltIn drivers.
    pub marketplace_node: Option<&'static str>,
    pub description: &'static str,
}

impl DriverEntry {
    pub fn is_builtin(&self) -> bool {
        self.kind == DriverKind::BuiltIn
    }
}

/// Built-in driver catalog.
pub static DRIVER_CATALOG: &[DriverEntry] = &[
    // ── BuiltIn drivers (run as tokio tasks inside the daemon) ──────────────
    DriverEntry {
        driver_name: "http-poll",
        kind: DriverKind::BuiltIn,
        marketplace_node: None,
        description: "REST APIs, weather services, HTTP sensors",
    },
    DriverEntry {
        driver_name: "system",
        kind: DriverKind::BuiltIn,
        marketplace_node: None,
        description: "CPU, RAM, disk, temperature",
    },
    DriverEntry {
        driver_name: "exec",
        kind: DriverKind::BuiltIn,
        marketplace_node: None,
        description: "Run a shell command on interval, publish stdout",
    },
    DriverEntry {
        driver_name: "webhook",
        kind: DriverKind::BuiltIn,
        marketplace_node: None,
        description: "Receive HTTP POST webhooks, publish body",
    },
    DriverEntry {
        driver_name: "tcp-listen",
        kind: DriverKind::BuiltIn,
        marketplace_node: None,
        description: "Raw TCP listener, publish received data",
    },
    // ── Marketplace drivers (require precompiled binary download) ────────────
    DriverEntry {
        driver_name: "rtsp",
        kind: DriverKind::Marketplace,
        marketplace_node: Some("rtsp-camera"),
        description: "IP cameras, NVRs, ONVIF",
    },
    DriverEntry {
        driver_name: "v4l2",
        kind: DriverKind::Marketplace,
        marketplace_node: Some("v4l2-camera"),
        description: "USB webcams, CSI cameras",
    },
    DriverEntry {
        driver_name: "serial",
        kind: DriverKind::Marketplace,
        marketplace_node: Some("serial-bridge"),
        description: "Arduino, UART, RS-485",
    },
    DriverEntry {
        driver_name: "gpio",
        kind: DriverKind::Marketplace,
        marketplace_node: Some("gpio-controller"),
        description: "Buttons, LEDs, relays",
    },
    DriverEntry {
        driver_name: "mqtt",
        kind: DriverKind::Marketplace,
        marketplace_node: Some("mqtt-bridge"),
        description: "Home automation, industrial MQTT",
    },
    DriverEntry {
        driver_name: "modbus",
        kind: DriverKind::Marketplace,
        marketplace_node: Some("modbus-bridge"),
        description: "Industrial IoT, PLCs",
    },
];

/// Look up a driver by name in the built-in catalog.
pub fn resolve_driver(name: &str) -> Option<&'static DriverEntry> {
    DRIVER_CATALOG.iter().find(|e| e.driver_name == name)
}

/// Validate a skill config against naming rules and the driver catalog.
///
/// Enforces:
/// - Valid node name.
/// - Exactly one of `driver` (v2) or `operators` (v3) is set — not both, not neither.
/// - For v2: the driver must exist in the catalog.
/// - For v3: each operator's driver must exist in the catalog.
pub fn validate_skill(skill: &SkillConfig) -> Result<()> {
    crate::validation::validate_node_name(&skill.name).map_err(SkillError::InvalidName)?;

    match (&skill.driver, skill.operators.is_empty()) {
        (Some(driver), true) => {
            // v2 format — validate the single driver
            if resolve_driver(driver).is_none() {
                return Err(SkillError::UnknownDriver(driver.clone()));
            }
        }
        (None, false) => {
            // v3 format — validate each operator's driver
            for op in &skill.operators {
                if resolve_driver(&op.driver).is_none() {
                    return Err(SkillError::UnknownDriver(op.driver.clone()));
                }
            }
        }
        (Some(_), false) => {
            return Err(SkillError::InvalidFormat(
                "cannot have both 'driver' and 'operators'".into(),
            ));
        }
        (None, true) => {
            return Err(SkillError::InvalidFormat(
                "must have either 'driver' or 'operators'".into(),
            ));
        }
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
                    skill.driver.as_deref().unwrap_or("<pipeline>")
                );
                skills.push(skill);
            }
            Err(err) => {
                log::warn!("Skill file '{}' failed validation: {}", path.display(), err);
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
        assert_eq!(skill.driver.as_deref(), Some("rtsp"));
        assert!(skill.enabled); // defaults to true
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
        assert_eq!(skill.driver.as_deref(), Some("v4l2"));
        assert!(skill.enabled); // defaults to true
        assert!(skill.config.is_empty());
        assert!(skill.schedule.is_none());
        assert!(skill.actions.is_empty());
    }

    #[test]
    fn parse_disabled_skill() {
        let yaml = "name: webcam\ndriver: v4l2\nenabled: false\n";
        let skill: SkillConfig = serde_yaml::from_str(yaml).unwrap();
        assert!(!skill.enabled);
    }

    #[test]
    fn parse_missing_name_fails() {
        let yaml = "driver: rtsp\n";
        let result: std::result::Result<SkillConfig, _> = serde_yaml::from_str(yaml);
        assert!(result.is_err(), "expected parse error for missing name");
    }

    #[test]
    fn parse_missing_driver_succeeds_as_v3_candidate() {
        // With driver optional, missing driver alone parses fine — validation catches
        // the "must have either driver or operators" constraint.
        let yaml = "name: my-camera\n";
        let result: std::result::Result<SkillConfig, _> = serde_yaml::from_str(yaml);
        assert!(
            result.is_ok(),
            "name-only YAML should parse (driver is optional now)"
        );
        let skill = result.unwrap();
        assert!(skill.driver.is_none());
        // validate_skill should reject it (neither driver nor operators)
        assert!(matches!(
            validate_skill(&skill),
            Err(SkillError::InvalidFormat(_))
        ));
    }

    // ── v3 pipeline YAML parsing ─────────────────────────────────────────────

    #[test]
    fn parse_v3_pipeline_yaml() {
        let yaml = r#"
name: weather-pipeline
operators:
  - id: fetch
    driver: http-poll
    config:
      url: https://api.open-meteo.com/v1/forecast
    outputs:
      data:
        topic: weather/raw
        rate: 1fps
links:
  - from: fetch/data
    to: dashboard
on:
  - trigger: fetch/error
    action: log
    message: "Fetch failed"
    cooldown: 5m
vars:
  api_key: "abc123"
"#;
        let skill: SkillConfig = serde_yaml::from_str(yaml).unwrap();
        assert_eq!(skill.name, "weather-pipeline");
        assert!(skill.driver.is_none());
        assert_eq!(skill.operators.len(), 1);
        assert_eq!(skill.operators[0].id, "fetch");
        assert_eq!(skill.operators[0].driver, "http-poll");
        assert_eq!(
            skill.operators[0].outputs["data"].topic.as_deref(),
            Some("weather/raw")
        );
        assert_eq!(
            skill.operators[0].outputs["data"].rate.as_deref(),
            Some("1fps")
        );
        assert_eq!(skill.links.len(), 1);
        assert_eq!(skill.links[0].from, "fetch/data");
        assert_eq!(skill.links[0].to, "dashboard");
        assert_eq!(skill.on.len(), 1);
        assert_eq!(skill.on[0].trigger, "fetch/error");
        assert_eq!(skill.on[0].action, "log");
        assert_eq!(skill.on[0].cooldown.as_deref(), Some("5m"));
        assert_eq!(skill.vars["api_key"], "abc123");
    }

    #[test]
    fn validate_v3_pipeline_valid() {
        let yaml = r#"
name: my-pipeline
operators:
  - id: src
    driver: http-poll
    config:
      url: https://example.com/api
"#;
        let skill: SkillConfig = serde_yaml::from_str(yaml).unwrap();
        assert!(validate_skill(&skill).is_ok());
    }

    #[test]
    fn validate_v3_rejects_unknown_operator_driver() {
        let yaml = r#"
name: bad-pipeline
operators:
  - id: src
    driver: not-a-driver
"#;
        let skill: SkillConfig = serde_yaml::from_str(yaml).unwrap();
        assert!(matches!(
            validate_skill(&skill),
            Err(SkillError::UnknownDriver(_))
        ));
    }

    #[test]
    fn validate_rejects_both_driver_and_operators() {
        let yaml = r#"
name: conflict
driver: rtsp
operators:
  - id: src
    driver: http-poll
"#;
        let skill: SkillConfig = serde_yaml::from_str(yaml).unwrap();
        assert!(matches!(
            validate_skill(&skill),
            Err(SkillError::InvalidFormat(_))
        ));
    }

    #[test]
    fn validate_rejects_neither_driver_nor_operators() {
        let yaml = "name: empty\n";
        let skill: SkillConfig = serde_yaml::from_str(yaml).unwrap();
        assert!(matches!(
            validate_skill(&skill),
            Err(SkillError::InvalidFormat(_))
        ));
    }

    // ── to_operators ─────────────────────────────────────────────────────────

    #[test]
    fn to_operators_v2_wraps_single_driver() {
        let yaml = "name: cam\ndriver: rtsp\nconfig:\n  url: rtsp://x\n";
        let skill: SkillConfig = serde_yaml::from_str(yaml).unwrap();
        let ops = skill.to_operators();
        assert_eq!(ops.len(), 1);
        assert_eq!(ops[0].id, "cam");
        assert_eq!(ops[0].driver, "rtsp");
        assert!(ops[0].config.contains_key("url"));
    }

    #[test]
    fn to_operators_v3_returns_operators_as_is() {
        let yaml = r#"
name: pipeline
operators:
  - id: a
    driver: http-poll
  - id: b
    driver: system
"#;
        let skill: SkillConfig = serde_yaml::from_str(yaml).unwrap();
        let ops = skill.to_operators();
        assert_eq!(ops.len(), 2);
        assert_eq!(ops[0].id, "a");
        assert_eq!(ops[1].id, "b");
    }

    #[test]
    fn to_operators_empty_skill_returns_empty() {
        let skill = SkillConfig {
            name: "x".into(),
            driver: None,
            enabled: true,
            config: HashMap::new(),
            intent: String::new(),
            on: vec![],
            schedule: None,
            actions: vec![],
            operators: vec![],
            links: vec![],
            vars: HashMap::new(),
        };
        assert!(skill.to_operators().is_empty());
    }

    // ── resolve_driver ───────────────────────────────────────────────────────

    #[test]
    fn resolve_known_marketplace_drivers() {
        let cases = [
            ("rtsp", "rtsp-camera"),
            ("v4l2", "v4l2-camera"),
            ("serial", "serial-bridge"),
            ("gpio", "gpio-controller"),
            ("mqtt", "mqtt-bridge"),
            ("modbus", "modbus-bridge"),
        ];
        for (driver, node) in cases {
            let entry =
                resolve_driver(driver).unwrap_or_else(|| panic!("driver '{}' not found", driver));
            assert_eq!(entry.kind, DriverKind::Marketplace);
            assert_eq!(entry.marketplace_node, Some(node));
        }
    }

    #[test]
    fn resolve_builtin_drivers() {
        for name in &["http-poll", "system", "exec", "webhook", "tcp-listen"] {
            let entry =
                resolve_driver(name).unwrap_or_else(|| panic!("driver '{}' not found", name));
            assert_eq!(entry.kind, DriverKind::BuiltIn);
            assert_eq!(entry.marketplace_node, None);
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
            driver: Some("rtsp".into()),
            enabled: true,
            config: HashMap::new(),
            intent: String::new(),
            on: vec![],
            schedule: None,
            actions: Vec::new(),
            operators: vec![],
            links: vec![],
            vars: HashMap::new(),
        };
        assert!(validate_skill(&skill).is_ok());
    }

    #[test]
    fn validate_rejects_invalid_name() {
        let skill = SkillConfig {
            name: "bad name!".into(),
            driver: Some("rtsp".into()),
            enabled: true,
            config: HashMap::new(),
            intent: String::new(),
            on: vec![],
            schedule: None,
            actions: Vec::new(),
            operators: vec![],
            links: vec![],
            vars: HashMap::new(),
        };
        assert!(matches!(
            validate_skill(&skill),
            Err(SkillError::InvalidName(_))
        ));
    }

    #[test]
    fn validate_rejects_empty_name() {
        let skill = SkillConfig {
            name: "".into(),
            driver: Some("rtsp".into()),
            enabled: true,
            config: HashMap::new(),
            intent: String::new(),
            on: vec![],
            schedule: None,
            actions: Vec::new(),
            operators: vec![],
            links: vec![],
            vars: HashMap::new(),
        };
        assert!(matches!(
            validate_skill(&skill),
            Err(SkillError::InvalidName(_))
        ));
    }

    #[test]
    fn validate_rejects_unknown_driver() {
        let skill = SkillConfig {
            name: "my-sensor".into(),
            driver: Some("unknown-driver".into()),
            enabled: true,
            config: HashMap::new(),
            intent: String::new(),
            on: vec![],
            schedule: None,
            actions: Vec::new(),
            operators: vec![],
            links: vec![],
            vars: HashMap::new(),
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
        assert_eq!(skills[0].driver.as_deref(), Some("rtsp"));
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

    // ── DriverEntry::is_builtin ──────────────────────────────────────────────

    #[test]
    fn is_builtin_helper() {
        let builtin = resolve_driver("http-poll").unwrap();
        assert!(builtin.is_builtin());
        let marketplace = resolve_driver("rtsp").unwrap();
        assert!(!marketplace.is_builtin());
    }
}
