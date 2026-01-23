//! Launch file YAML schema definitions

use indexmap::IndexMap;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;

/// Root launch file configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LaunchFile {
    /// Launch file format version
    #[serde(default = "default_version")]
    pub version: String,

    /// Argument definitions with defaults
    #[serde(default)]
    pub args: IndexMap<String, ArgDefinition>,

    /// Environment variables (applied to all nodes)
    #[serde(default)]
    pub env: HashMap<String, String>,

    /// Node definitions (ordered map for deterministic launch order)
    pub nodes: IndexMap<String, NodeConfig>,
}

fn default_version() -> String {
    "1.0".to_string()
}

/// Argument definition with default value
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ArgDefinition {
    /// Default value for the argument
    pub default: ArgValue,
    /// Optional description
    #[serde(default)]
    pub description: Option<String>,
}

/// Argument values can be strings, booleans, or numbers
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(untagged)]
pub enum ArgValue {
    Bool(bool),
    Int(i64),
    Float(f64),
    String(String),
}

impl ArgValue {
    /// Convert to string representation
    pub fn as_str(&self) -> String {
        match self {
            ArgValue::Bool(b) => b.to_string(),
            ArgValue::Int(i) => i.to_string(),
            ArgValue::Float(f) => f.to_string(),
            ArgValue::String(s) => s.clone(),
        }
    }

    /// Parse from string
    pub fn from_str(s: &str) -> Self {
        // Try parsing as bool first
        if s.eq_ignore_ascii_case("true") {
            return ArgValue::Bool(true);
        }
        if s.eq_ignore_ascii_case("false") {
            return ArgValue::Bool(false);
        }
        // Try parsing as integer
        if let Ok(i) = s.parse::<i64>() {
            return ArgValue::Int(i);
        }
        // Try parsing as float
        if let Ok(f) = s.parse::<f64>() {
            return ArgValue::Float(f);
        }
        // Default to string
        ArgValue::String(s.to_string())
    }

    /// Check if value is truthy
    pub fn is_truthy(&self) -> bool {
        match self {
            ArgValue::Bool(b) => *b,
            ArgValue::Int(i) => *i != 0,
            ArgValue::Float(f) => *f != 0.0,
            ArgValue::String(s) => {
                !s.is_empty()
                    && !s.eq_ignore_ascii_case("false")
                    && !s.eq_ignore_ascii_case("0")
                    && !s.eq_ignore_ascii_case("no")
            }
        }
    }
}

/// Node configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NodeConfig {
    /// Package name (cargo workspace member)
    /// Mutually exclusive with `executable`
    #[serde(default)]
    pub package: Option<String>,

    /// Binary name within the package
    /// Required if `package` is set
    #[serde(default)]
    pub binary: Option<String>,

    /// Direct executable path (for non-cargo binaries)
    /// Mutually exclusive with `package`
    #[serde(default)]
    pub executable: Option<String>,

    /// Raw arguments passed directly to the executable
    #[serde(default)]
    pub raw_args: Vec<String>,

    /// Named arguments (converted to --key value or --key=value)
    #[serde(default)]
    pub args: HashMap<String, String>,

    /// Environment variables specific to this node
    #[serde(default)]
    pub env: HashMap<String, String>,

    /// Working directory for the process
    #[serde(default)]
    pub working_dir: Option<String>,

    /// Dependencies - nodes that must be started first
    #[serde(default)]
    pub depends_on: Vec<DependencySpec>,

    /// Group name for filtering
    #[serde(default)]
    pub group: Option<String>,

    /// Whether the node is enabled
    /// Can be a boolean or a string like "$(arg enable_recording)"
    #[serde(default = "default_enabled")]
    pub enabled: EnabledValue,

    /// Startup delay in milliseconds after dependencies are ready
    #[serde(default)]
    pub startup_delay_ms: Option<u64>,

    /// Restart policy
    #[serde(default)]
    pub restart: RestartPolicy,
}

fn default_enabled() -> EnabledValue {
    EnabledValue::Bool(true)
}

/// Enabled value can be a direct boolean or a substitution string
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(untagged)]
pub enum EnabledValue {
    Bool(bool),
    String(String),
}

impl EnabledValue {
    /// Resolve the enabled value after substitution
    pub fn is_enabled(&self, resolved: Option<&str>) -> bool {
        match self {
            EnabledValue::Bool(b) => *b,
            EnabledValue::String(s) => {
                let value = resolved.unwrap_or(s);
                value.eq_ignore_ascii_case("true")
                    || value == "1"
                    || value.eq_ignore_ascii_case("yes")
            }
        }
    }
}

/// Dependency specification
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(untagged)]
pub enum DependencySpec {
    /// Simple dependency: just the node name
    Simple(String),
    /// Conditional dependency: { node_name: condition }
    Conditional(HashMap<String, DependencyCondition>),
}

impl DependencySpec {
    /// Get the node name this dependency refers to
    pub fn node_name(&self) -> &str {
        match self {
            DependencySpec::Simple(name) => name,
            DependencySpec::Conditional(map) => map.keys().next().unwrap(),
        }
    }

    /// Get the condition (defaults to "started")
    pub fn condition(&self) -> DependencyCondition {
        match self {
            DependencySpec::Simple(_) => DependencyCondition::Started,
            DependencySpec::Conditional(map) => map.values().next().cloned().unwrap(),
        }
    }
}

/// Condition for dependency to be satisfied
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum DependencyCondition {
    /// Node process has started
    Started,
    /// Node is healthy (has been running for a while)
    Healthy,
}

/// Restart policy for a node
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
#[serde(rename_all = "lowercase")]
pub enum RestartPolicy {
    /// Never restart (default)
    #[default]
    Never,
    /// Always restart on exit
    Always,
    /// Only restart on failure (non-zero exit code)
    OnFailure,
}

impl LaunchFile {
    /// Load launch file from a YAML file
    pub fn from_file(path: &str) -> Result<Self, LaunchFileError> {
        let content = std::fs::read_to_string(path).map_err(|e| LaunchFileError::Io {
            path: path.to_string(),
            source: e,
        })?;
        Self::from_yaml(&content)
    }

    /// Parse launch file from YAML string
    pub fn from_yaml(content: &str) -> Result<Self, LaunchFileError> {
        let launch_file: LaunchFile =
            serde_yaml::from_str(content).map_err(LaunchFileError::Parse)?;
        launch_file.validate()?;
        Ok(launch_file)
    }

    /// Validate the launch file configuration
    pub fn validate(&self) -> Result<(), LaunchFileError> {
        for (name, node) in &self.nodes {
            // Check that either package+binary or executable is specified
            match (&node.package, &node.binary, &node.executable) {
                (Some(_), Some(_), None) => {} // package + binary: OK
                (None, None, Some(_)) => {}    // executable: OK
                (Some(_), None, None) => {
                    return Err(LaunchFileError::Validation(format!(
                        "Node '{}': 'package' requires 'binary' to be specified",
                        name
                    )));
                }
                (None, Some(_), None) => {
                    return Err(LaunchFileError::Validation(format!(
                        "Node '{}': 'binary' requires 'package' to be specified",
                        name
                    )));
                }
                (Some(_), _, Some(_)) | (_, Some(_), Some(_)) => {
                    return Err(LaunchFileError::Validation(format!(
                        "Node '{}': cannot specify both 'package'/'binary' and 'executable'",
                        name
                    )));
                }
                (None, None, None) => {
                    return Err(LaunchFileError::Validation(format!(
                        "Node '{}': must specify either 'package'+'binary' or 'executable'",
                        name
                    )));
                }
            }

            // Check that dependencies reference existing nodes
            for dep in &node.depends_on {
                let dep_name = dep.node_name();
                if !self.nodes.contains_key(dep_name) {
                    return Err(LaunchFileError::Validation(format!(
                        "Node '{}': depends on unknown node '{}'",
                        name, dep_name
                    )));
                }
            }
        }

        Ok(())
    }

    /// Get all unique group names
    pub fn groups(&self) -> Vec<String> {
        let mut groups: Vec<String> = self
            .nodes
            .values()
            .filter_map(|n| n.group.clone())
            .collect();
        groups.sort();
        groups.dedup();
        groups
    }
}

/// Errors that can occur when loading a launch file
#[derive(Debug, thiserror::Error)]
pub enum LaunchFileError {
    #[error("Failed to read launch file '{path}': {source}")]
    Io {
        path: String,
        #[source]
        source: std::io::Error,
    },

    #[error("Failed to parse launch file: {0}")]
    Parse(#[from] serde_yaml::Error),

    #[error("Validation error: {0}")]
    Validation(String),
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_simple_launch_file() {
        let yaml = r#"
version: "1.0"
nodes:
  bridge:
    executable: "target/release/bridge"
    group: core
  cameras:
    package: rtsp_camera
    binary: cameras_node
    depends_on:
      - bridge
"#;
        let launch_file = LaunchFile::from_yaml(yaml).unwrap();
        assert_eq!(launch_file.nodes.len(), 2);
        assert!(launch_file.nodes.contains_key("bridge"));
        assert!(launch_file.nodes.contains_key("cameras"));
    }

    #[test]
    fn test_arg_value_parsing() {
        assert!(matches!(ArgValue::from_str("true"), ArgValue::Bool(true)));
        assert!(matches!(ArgValue::from_str("false"), ArgValue::Bool(false)));
        assert!(matches!(ArgValue::from_str("42"), ArgValue::Int(42)));
        assert!(matches!(ArgValue::from_str("3.14"), ArgValue::Float(_)));
        assert!(matches!(ArgValue::from_str("hello"), ArgValue::String(_)));
    }

    #[test]
    fn test_validation_missing_binary() {
        let yaml = r#"
nodes:
  bad_node:
    package: some_package
"#;
        let result = LaunchFile::from_yaml(yaml);
        assert!(result.is_err());
    }

    #[test]
    fn test_dependency_spec() {
        let yaml = r#"
nodes:
  first:
    executable: "bin/first"
  second:
    executable: "bin/second"
    depends_on:
      - first
      - first: started
"#;
        let launch_file = LaunchFile::from_yaml(yaml).unwrap();
        let second = &launch_file.nodes["second"];
        assert_eq!(second.depends_on.len(), 2);
    }
}
