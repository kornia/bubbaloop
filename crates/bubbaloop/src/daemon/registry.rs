//! Node registry management
//!
//! Manages the nodes.json file that tracks registered nodes.

use serde::{Deserialize, Serialize};
use std::fs;
use std::path::{Path, PathBuf};
use thiserror::Error;

#[derive(Error, Debug)]
pub enum RegistryError {
    #[error("IO error: {0}")]
    Io(#[from] std::io::Error),

    #[error("JSON error: {0}")]
    Json(#[from] serde_json::Error),

    #[error("YAML error: {0}")]
    Yaml(#[from] serde_yaml::Error),

    #[error("Node not found: {0}")]
    NodeNotFound(String),

    #[error("Node already registered: {0}")]
    NodeAlreadyRegistered(String),

    #[error("Invalid node: {0}")]
    InvalidNode(String),
}

pub type Result<T> = std::result::Result<T, RegistryError>;

/// Capability types a node can provide
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "snake_case")]
pub enum Capability {
    Sensor,
    Actuator,
    Processor,
    Gateway,
}

/// Declaration of a topic the node publishes or subscribes to
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct TopicSpec {
    /// Topic suffix (e.g., "output", "compressed", "metrics")
    pub suffix: String,
    /// Protobuf message type (e.g., "bubbaloop.header.v1.Header")
    #[serde(default)]
    pub schema_type: Option<String>,
    /// Publishing rate in Hz (0 = event-driven)
    #[serde(default)]
    pub rate_hz: Option<f64>,
    /// Human-readable description
    #[serde(default)]
    pub description: Option<String>,
}

/// Declaration of a command the node accepts
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CommandSpec {
    /// Command name (e.g., "capture", "set_resolution")
    pub name: String,
    /// Human-readable description
    #[serde(default)]
    pub description: Option<String>,
    /// JSON schema for parameters (optional)
    #[serde(default)]
    pub parameters: Option<serde_json::Value>,
    /// JSON schema for return value (optional)
    #[serde(default)]
    pub returns: Option<serde_json::Value>,
}

/// Hardware/software requirements
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct Requirements {
    /// Required hardware (e.g., ["camera", "gpu", "network"])
    #[serde(default)]
    pub hardware: Vec<String>,
    /// Required software/binaries (e.g., ["ffmpeg", "gstreamer"])
    #[serde(default)]
    pub software: Vec<String>,
    /// Required environment variables
    #[serde(default)]
    pub env_vars: Vec<String>,
}

/// Node manifest from node.yaml
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct NodeManifest {
    pub name: String,
    pub version: String,
    #[serde(rename = "type")]
    pub node_type: String,
    #[serde(default)]
    pub description: String,
    #[serde(default)]
    pub author: Option<String>,
    #[serde(default)]
    pub build: Option<String>,
    #[serde(default)]
    pub command: Option<String>,
    /// Other nodes that this node depends on (must be started first)
    #[serde(default)]
    pub depends_on: Vec<String>,
    /// Capabilities this node provides
    #[serde(default)]
    pub capabilities: Vec<Capability>,
    /// Topics this node publishes
    #[serde(default)]
    pub publishes: Vec<TopicSpec>,
    /// Topics this node subscribes to
    #[serde(default)]
    pub subscribes: Vec<TopicSpec>,
    /// Commands this node accepts
    #[serde(default)]
    pub commands: Vec<CommandSpec>,
    /// Hardware/software requirements
    #[serde(default)]
    pub requires: Option<Requirements>,
    /// Extensible metadata (for future use)
    #[serde(default)]
    pub metadata: std::collections::HashMap<String, serde_json::Value>,
}

impl NodeManifest {
    /// Validate the node manifest fields
    pub fn validate(&self) -> Result<()> {
        // Validate name: 1-64 chars, alphanumeric + hyphen + underscore
        if self.name.is_empty() || self.name.len() > 64 {
            return Err(RegistryError::InvalidNode(format!(
                "Node name must be 1-64 characters, got: {}",
                self.name.len()
            )));
        }
        if !self
            .name
            .chars()
            .all(|c| c.is_alphanumeric() || c == '-' || c == '_')
        {
            return Err(RegistryError::InvalidNode(format!(
                "Node name contains invalid characters: {}",
                self.name
            )));
        }

        // Validate version: basic semver check (contains digits and dots)
        if self.version.is_empty() {
            return Err(RegistryError::InvalidNode(
                "Node version cannot be empty".to_string(),
            ));
        }
        let has_digit = self.version.chars().any(|c| c.is_ascii_digit());
        if !has_digit {
            return Err(RegistryError::InvalidNode(format!(
                "Node version must contain at least one digit: {}",
                self.version
            )));
        }

        // Validate type: must be 'rust' or 'python'
        let valid_types = ["rust", "python"];
        if !valid_types.contains(&self.node_type.to_lowercase().as_str()) {
            return Err(RegistryError::InvalidNode(format!(
                "Node type must be 'rust' or 'python', got: {}",
                self.node_type
            )));
        }

        // Validate description length: max 500 chars
        if self.description.len() > 500 {
            return Err(RegistryError::InvalidNode(format!(
                "Node description exceeds 500 characters: {}",
                self.description.len()
            )));
        }

        // Check for null bytes in string fields
        for field in [&self.name, &self.description, &self.node_type] {
            if field.contains('\0') {
                return Err(RegistryError::InvalidNode(
                    "Manifest fields cannot contain null bytes".to_string(),
                ));
            }
        }

        // Check optional string fields for null bytes
        if let Some(ref author) = self.author {
            if author.contains('\0') {
                return Err(RegistryError::InvalidNode(
                    "Author field cannot contain null bytes".to_string(),
                ));
            }
        }
        if let Some(ref build) = self.build {
            if build.contains('\0') {
                return Err(RegistryError::InvalidNode(
                    "Build field cannot contain null bytes".to_string(),
                ));
            }
        }
        if let Some(ref command) = self.command {
            if command.contains('\0') {
                return Err(RegistryError::InvalidNode(
                    "Command field cannot contain null bytes".to_string(),
                ));
            }
        }

        Ok(())
    }
}

/// Entry in the nodes registry (nodes.json)
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NodeEntry {
    pub path: String,
    #[serde(rename = "addedAt")]
    pub added_at: String,
    /// Instance name override (for multi-instance nodes)
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub name_override: Option<String>,
    /// Config file path override (passed to binary via -c)
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub config_override: Option<String>,
}

/// The nodes registry
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct NodesRegistry {
    pub nodes: Vec<NodeEntry>,
}

/// Get the bubbaloop home directory
pub fn get_bubbaloop_home() -> PathBuf {
    dirs::home_dir()
        .unwrap_or_else(|| PathBuf::from("/tmp"))
        .join(".bubbaloop")
}

/// Get the nodes.json file path
pub fn get_nodes_file() -> PathBuf {
    get_bubbaloop_home().join("nodes.json")
}

/// Load the nodes registry
pub fn load_registry() -> Result<NodesRegistry> {
    let path = get_nodes_file();
    if !path.exists() {
        return Ok(NodesRegistry::default());
    }

    let content = fs::read_to_string(&path)?;
    let registry: NodesRegistry = serde_json::from_str(&content)?;
    Ok(registry)
}

/// Save the nodes registry
pub fn save_registry(registry: &NodesRegistry) -> Result<()> {
    let home = get_bubbaloop_home();
    fs::create_dir_all(&home)?;

    let path = get_nodes_file();
    let content = serde_json::to_string_pretty(registry)?;
    fs::write(path, content)?;
    Ok(())
}

/// Read the node manifest from a node directory
pub fn read_manifest(node_path: &Path) -> Result<NodeManifest> {
    let manifest_path = node_path.join("node.yaml");
    if !manifest_path.exists() {
        return Err(RegistryError::InvalidNode(format!(
            "No node.yaml found in {}",
            node_path.display()
        )));
    }

    let content = fs::read_to_string(&manifest_path)?;
    let manifest: NodeManifest = serde_yaml::from_str(&content)?;

    // Validate the manifest fields
    manifest.validate()?;

    Ok(manifest)
}

/// Get the effective name for a registry entry given its manifest.
/// Returns name_override if set, otherwise the manifest name.
pub fn effective_name(entry: &NodeEntry, manifest: &NodeManifest) -> String {
    entry
        .name_override
        .as_deref()
        .unwrap_or(&manifest.name)
        .to_string()
}

/// Register a new node, optionally with an instance name and config override.
///
/// Returns `(manifest, effective_name)` where effective_name is `name_override`
/// if provided, otherwise `manifest.name`.
pub fn register_node(
    node_path: &str,
    name_override: Option<&str>,
    config_override: Option<&str>,
) -> Result<(NodeManifest, String)> {
    let path = Path::new(node_path);

    // Check directory exists
    if !path.exists() {
        return Err(RegistryError::InvalidNode(format!(
            "Directory not found: {}",
            node_path
        )));
    }

    // Read and validate manifest
    let manifest = read_manifest(path)?;

    // Determine effective name
    let eff_name = name_override.unwrap_or(&manifest.name).to_string();

    // Validate override name if provided (same rules as manifest name)
    if let Some(override_name) = name_override {
        if override_name.is_empty() || override_name.len() > 64 {
            return Err(RegistryError::InvalidNode(format!(
                "Instance name must be 1-64 characters, got: {}",
                override_name.len()
            )));
        }
        if !override_name
            .chars()
            .all(|c| c.is_alphanumeric() || c == '-' || c == '_')
        {
            return Err(RegistryError::InvalidNode(format!(
                "Instance name contains invalid characters: {}",
                override_name
            )));
        }
    }

    // Load registry
    let mut registry = load_registry()?;

    // Check if an entry with the same effective name already exists
    for entry in &registry.nodes {
        let entry_path = Path::new(&entry.path);
        if let Ok(entry_manifest) = read_manifest(entry_path) {
            let existing_name = effective_name(entry, &entry_manifest);
            if existing_name == eff_name {
                return Err(RegistryError::NodeAlreadyRegistered(eff_name));
            }
        }
    }

    let canonical = path
        .canonicalize()
        .unwrap_or_else(|_| path.to_path_buf())
        .to_string_lossy()
        .to_string();

    // Add to registry
    registry.nodes.push(NodeEntry {
        path: canonical,
        added_at: chrono_now(),
        name_override: name_override.map(String::from),
        config_override: config_override.map(String::from),
    });

    save_registry(&registry)?;
    Ok((manifest, eff_name))
}

/// Unregister a node by effective name or path.
///
/// First tries to match by effective name (name_override or manifest name),
/// then falls back to path matching for backward compatibility.
pub fn unregister_node(name_or_path: &str) -> Result<()> {
    let mut registry = load_registry()?;

    let initial_len = registry.nodes.len();

    // Try matching by effective name first
    registry.nodes.retain(|entry| {
        let entry_path = Path::new(&entry.path);

        // Try to match by effective name (manifest-based or name_override)
        if let Ok(manifest) = read_manifest(entry_path) {
            let eff = effective_name(entry, &manifest);
            if eff == name_or_path {
                return false; // remove this entry
            }
        } else {
            // Directory deleted - still try to match by name_override or directory name
            if let Some(ref name_ov) = entry.name_override {
                if name_ov == name_or_path {
                    log::warn!(
                        "Node directory '{}' was already deleted, removing registry entry",
                        entry.path
                    );
                    return false; // remove this entry
                }
            }
            // As a fallback, try matching against the directory name itself
            if let Some(dir_name) = entry_path.file_name().and_then(|n| n.to_str()) {
                if dir_name == name_or_path {
                    log::warn!(
                        "Node directory '{}' was already deleted, removing registry entry",
                        entry.path
                    );
                    return false; // remove this entry
                }
            }
        }

        // Fallback: path matching
        let n_canonical = entry_path
            .canonicalize()
            .unwrap_or_else(|_| entry_path.to_path_buf());
        let query_path = Path::new(name_or_path);
        let query_canonical = query_path
            .canonicalize()
            .unwrap_or_else(|_| query_path.to_path_buf());
        n_canonical != query_canonical
    });

    if registry.nodes.len() == initial_len {
        return Err(RegistryError::NodeNotFound(name_or_path.to_string()));
    }

    save_registry(&registry)?;
    Ok(())
}

/// List all registered nodes with their entries and manifests.
///
/// Returns `(NodeEntry, Option<NodeManifest>)` so callers can access
/// name_override and config_override for multi-instance support.
pub fn list_nodes() -> Result<Vec<(NodeEntry, Option<NodeManifest>)>> {
    let registry = load_registry()?;

    let nodes = registry
        .nodes
        .iter()
        .map(|entry| {
            let path = Path::new(&entry.path);
            let manifest = read_manifest(path).ok();
            (entry.clone(), manifest)
        })
        .collect();

    Ok(nodes)
}

/// Check if a node's binary/script is built
pub fn check_is_built(node_path: &str, manifest: &NodeManifest) -> bool {
    let path = Path::new(node_path);

    if let Some(ref command) = manifest.command {
        let binary_path = path.join(command);
        binary_path.exists()
    } else if manifest.node_type == "rust" {
        // Check for target/release or target/debug binary
        let release_path = path.join("target/release").join(&manifest.name);
        let debug_path = path.join("target/debug").join(&manifest.name);
        release_path.exists() || debug_path.exists()
    } else {
        // Python - check for main.py and venv
        let main_py = path.join("main.py");
        main_py.exists()
    }
}

/// Get current timestamp as ISO string (without chrono dependency)
fn chrono_now() -> String {
    use std::time::{SystemTime, UNIX_EPOCH};
    let duration = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default();
    let secs = duration.as_secs();

    // Simple ISO format without external dependencies
    // This is good enough for our purposes
    format!("{}000", secs)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_get_bubbaloop_home() {
        let home = get_bubbaloop_home();
        assert!(home.to_string_lossy().contains(".bubbaloop"));
    }

    #[test]
    fn test_manifest_validation_valid() {
        let manifest = NodeManifest {
            name: "test-node".to_string(),
            version: "1.0.0".to_string(),
            node_type: "rust".to_string(),
            description: "A test node".to_string(),
            author: Some("Test Author".to_string()),
            build: Some("cargo build --release".to_string()),
            command: Some("./target/release/test-node".to_string()),
            ..Default::default()
        };
        assert!(manifest.validate().is_ok());
    }

    #[test]
    fn test_manifest_validation_empty_name() {
        let manifest = NodeManifest {
            name: "".to_string(),
            version: "1.0.0".to_string(),
            node_type: "rust".to_string(),
            description: "Test".to_string(),
            ..Default::default()
        };
        assert!(manifest.validate().is_err());
    }

    #[test]
    fn test_manifest_validation_name_too_long() {
        let manifest = NodeManifest {
            name: "a".repeat(65),
            version: "1.0.0".to_string(),
            node_type: "rust".to_string(),
            description: "Test".to_string(),
            ..Default::default()
        };
        assert!(manifest.validate().is_err());
    }

    #[test]
    fn test_manifest_validation_invalid_name_chars() {
        let manifest = NodeManifest {
            name: "test node!".to_string(),
            version: "1.0.0".to_string(),
            node_type: "rust".to_string(),
            description: "Test".to_string(),
            ..Default::default()
        };
        assert!(manifest.validate().is_err());
    }

    #[test]
    fn test_manifest_validation_valid_name_with_underscores_hyphens() {
        let manifest = NodeManifest {
            name: "test_node-123".to_string(),
            version: "1.0.0".to_string(),
            node_type: "rust".to_string(),
            description: "Test".to_string(),
            ..Default::default()
        };
        assert!(manifest.validate().is_ok());
    }

    #[test]
    fn test_manifest_validation_empty_version() {
        let manifest = NodeManifest {
            name: "test-node".to_string(),
            version: "".to_string(),
            node_type: "rust".to_string(),
            description: "Test".to_string(),
            ..Default::default()
        };
        assert!(manifest.validate().is_err());
    }

    #[test]
    fn test_manifest_validation_version_no_digits() {
        let manifest = NodeManifest {
            name: "test-node".to_string(),
            version: "alpha".to_string(),
            node_type: "rust".to_string(),
            description: "Test".to_string(),
            ..Default::default()
        };
        assert!(manifest.validate().is_err());
    }

    #[test]
    fn test_manifest_validation_invalid_node_type() {
        let manifest = NodeManifest {
            name: "test-node".to_string(),
            version: "1.0.0".to_string(),
            node_type: "javascript".to_string(),
            description: "Test".to_string(),
            ..Default::default()
        };
        assert!(manifest.validate().is_err());
    }

    #[test]
    fn test_manifest_validation_python_type() {
        let manifest = NodeManifest {
            name: "test-node".to_string(),
            version: "1.0.0".to_string(),
            node_type: "python".to_string(),
            description: "Test".to_string(),
            ..Default::default()
        };
        assert!(manifest.validate().is_ok());
    }

    #[test]
    fn test_manifest_validation_description_too_long() {
        let manifest = NodeManifest {
            name: "test-node".to_string(),
            version: "1.0.0".to_string(),
            node_type: "rust".to_string(),
            description: "a".repeat(501),
            ..Default::default()
        };
        assert!(manifest.validate().is_err());
    }

    #[test]
    fn test_manifest_validation_null_bytes_in_name() {
        let manifest = NodeManifest {
            name: "test\0node".to_string(),
            version: "1.0.0".to_string(),
            node_type: "rust".to_string(),
            description: "Test".to_string(),
            ..Default::default()
        };
        assert!(manifest.validate().is_err());
    }

    #[test]
    fn test_manifest_validation_null_bytes_in_command() {
        let manifest = NodeManifest {
            name: "test-node".to_string(),
            version: "1.0.0".to_string(),
            node_type: "rust".to_string(),
            description: "Test".to_string(),
            command: Some("./target\0/release/test".to_string()),
            ..Default::default()
        };
        assert!(manifest.validate().is_err());
    }

    #[test]
    fn test_effective_name_with_override() {
        let entry = NodeEntry {
            path: "/opt/nodes/rtsp-camera".to_string(),
            added_at: "1700000000000".to_string(),
            name_override: Some("rtsp-camera-terrace".to_string()),
            config_override: Some("/etc/bubbaloop/terrace.yaml".to_string()),
        };
        let manifest = NodeManifest {
            name: "rtsp-camera".to_string(),
            version: "0.1.0".to_string(),
            node_type: "rust".to_string(),
            description: "RTSP camera".to_string(),
            ..Default::default()
        };
        assert_eq!(effective_name(&entry, &manifest), "rtsp-camera-terrace");
    }

    #[test]
    fn test_effective_name_without_override() {
        let entry = NodeEntry {
            path: "/opt/nodes/rtsp-camera".to_string(),
            added_at: "1700000000000".to_string(),
            name_override: None,
            config_override: None,
        };
        let manifest = NodeManifest {
            name: "rtsp-camera".to_string(),
            version: "0.1.0".to_string(),
            node_type: "rust".to_string(),
            description: "RTSP camera".to_string(),
            ..Default::default()
        };
        assert_eq!(effective_name(&entry, &manifest), "rtsp-camera");
    }

    /// Verify that multiple registry entries can share the same path but with
    /// different name_overrides, simulating multi-instance camera deployment.
    #[test]
    fn test_multi_instance_registry_entries() {
        let manifest = NodeManifest {
            name: "rtsp-camera".to_string(),
            version: "0.2.0".to_string(),
            node_type: "rust".to_string(),
            description: "RTSP camera node".to_string(),
            ..Default::default()
        };

        let entries = vec![
            NodeEntry {
                path: "/opt/nodes/rtsp-camera".to_string(),
                added_at: "1700000000000".to_string(),
                name_override: Some("rtsp-camera-terrace".to_string()),
                config_override: Some("/etc/bubbaloop/terrace.yaml".to_string()),
            },
            NodeEntry {
                path: "/opt/nodes/rtsp-camera".to_string(),
                added_at: "1700000001000".to_string(),
                name_override: Some("rtsp-camera-garage".to_string()),
                config_override: Some("/etc/bubbaloop/garage.yaml".to_string()),
            },
            NodeEntry {
                path: "/opt/nodes/rtsp-camera".to_string(),
                added_at: "1700000002000".to_string(),
                name_override: Some("rtsp-camera-entrance".to_string()),
                config_override: None,
            },
        ];

        // All share the same path but have distinct effective names
        let names: Vec<String> = entries
            .iter()
            .map(|e| effective_name(e, &manifest))
            .collect();
        assert_eq!(names.len(), 3);
        assert_eq!(names[0], "rtsp-camera-terrace");
        assert_eq!(names[1], "rtsp-camera-garage");
        assert_eq!(names[2], "rtsp-camera-entrance");

        // All share the same path
        assert!(entries.iter().all(|e| e.path == "/opt/nodes/rtsp-camera"));

        // Verify they serialize/deserialize correctly in a registry
        let registry = NodesRegistry {
            nodes: entries.clone(),
        };
        let json = serde_json::to_string_pretty(&registry).unwrap();
        let restored: NodesRegistry = serde_json::from_str(&json).unwrap();

        assert_eq!(restored.nodes.len(), 3);
        for (i, entry) in restored.nodes.iter().enumerate() {
            assert_eq!(
                effective_name(entry, &manifest),
                effective_name(&entries[i], &manifest)
            );
        }
    }
}
