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
        // Validate name using the canonical shared validator
        crate::validation::validate_node_name(&self.name).map_err(RegistryError::InvalidNode)?;

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

/// Save the nodes registry.
///
/// NOTE: Registry mutations are serialized through the daemon's NodeManager.
/// File locking is not used here — all callers must go through the daemon
/// to avoid concurrent writes. Direct CLI calls (register_node, unregister_node)
/// are single-threaded by nature of the CLI process model.
pub fn save_registry(registry: &NodesRegistry) -> Result<()> {
    let home = get_bubbaloop_home();
    fs::create_dir_all(&home)?;

    let path = get_nodes_file();
    let content = serde_json::to_string_pretty(registry)?;
    fs::write(&path, content)?;

    // Set restrictive permissions on Unix (0600 — owner read/write only)
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let perms = std::fs::Permissions::from_mode(0o600);
        let _ = std::fs::set_permissions(&path, perms);
    }

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
        crate::validation::validate_node_name(override_name)
            .map_err(|e| RegistryError::InvalidNode(format!("Instance name invalid: {}", e)))?;
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

    // Validate config_override if provided
    if let Some(config) = config_override {
        validate_config_override(config)?;
    }

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

/// Check if a node's binary/script is built.
///
/// Detection order:
/// 1. Rust nodes: binary in `target/release/<name>` or `target/debug/<name>`.
///    Fallback: first relative token in `command` (for non-standard binary locations).
/// 2. No `command`: check for `main.py` in the node directory
/// 3. Command with `-m module.path`: check `module/path.py` in node dir
///    e.g. `pixi run python -m smartpower.nodes.runner` → `smartpower/nodes/runner.py`
/// 4. Command with a `*.py` token: check that file in node dir
///    e.g. `pixi run python sensor.py` → `sensor.py`
///    e.g. `python3 main.py` → `main.py`
/// 5. Anything else (external binary, pixi task, etc.): assume built (`true`)
pub fn check_is_built(node_path: &str, manifest: &NodeManifest) -> bool {
    let path = Path::new(node_path);

    if manifest.node_type == "rust" {
        // Standard cargo output locations
        let release_path = path.join("target/release").join(&manifest.name);
        let debug_path = path.join("target/debug").join(&manifest.name);
        if release_path.exists() || debug_path.exists() {
            return true;
        }
        // Fallback: command may point to a non-standard relative binary path
        // (e.g. `command: "target/aarch64-unknown-linux-gnu/release/my-node"`).
        // Absolute paths and flags are ignored — they don't live in the node dir.
        if let Some(ref command) = manifest.command {
            if let Some(token) = command
                .split_whitespace()
                .find(|t| !t.starts_with('-') && !std::path::Path::new(t).is_absolute())
            {
                return path.join(token).exists();
            }
        }
        return false;
    }

    let Some(ref command) = manifest.command else {
        // No command — default Python entrypoint
        return path.join("main.py").exists();
    };

    let tokens: Vec<&str> = command.split_whitespace().collect();

    // Empty or whitespace-only command cannot be resolved — not built
    if tokens.is_empty() {
        return false;
    }

    // Check `-m module.path` → module/path.py
    if let Some(pos) = tokens.iter().position(|t| *t == "-m") {
        let Some(module) = tokens.get(pos + 1) else {
            // `-m` with no following token is malformed — not built
            return false;
        };
        let module_file = module.replace('.', "/") + ".py";
        return path.join(&module_file).exists();
    }

    // Find first *.py token and check it exists in node dir
    if let Some(script) = tokens.iter().find(|t| t.ends_with(".py")) {
        return path.join(script).exists();
    }

    // External binary, pixi task, or other launcher — assume built
    true
}

/// Validate a config_override path to prevent systemd specifier injection and path traversal.
///
/// Rejects:
/// - `%` characters (systemd specifier expansion: `%n`, `%i`, `%h`, etc.)
/// - `..` path segments (path traversal)
/// - null bytes
/// - newlines and carriage returns
fn validate_config_override(config: &str) -> Result<()> {
    if config.is_empty() {
        return Err(RegistryError::InvalidNode(
            "Config override path cannot be empty".to_string(),
        ));
    }

    if config.contains('\0') {
        return Err(RegistryError::InvalidNode(
            "Config override path cannot contain null bytes".to_string(),
        ));
    }

    if config.contains('\n') || config.contains('\r') {
        return Err(RegistryError::InvalidNode(
            "Config override path cannot contain newlines".to_string(),
        ));
    }

    if config.contains('%') {
        return Err(RegistryError::InvalidNode(
            "Config override path cannot contain '%' (systemd specifier injection)".to_string(),
        ));
    }

    // Check for path traversal via ".." segments
    for segment in config.split('/') {
        if segment == ".." {
            return Err(RegistryError::InvalidNode(
                "Config override path cannot contain '..' segments (path traversal)".to_string(),
            ));
        }
    }
    // Also check Windows-style path separators
    for segment in config.split('\\') {
        if segment == ".." {
            return Err(RegistryError::InvalidNode(
                "Config override path cannot contain '..' segments (path traversal)".to_string(),
            ));
        }
    }

    Ok(())
}

/// Get current timestamp as ISO 8601 string (RFC 3339 format)
fn chrono_now() -> String {
    chrono::Utc::now().to_rfc3339()
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

    #[test]
    fn test_validate_config_override_valid() {
        assert!(validate_config_override("/etc/bubbaloop/terrace.yaml").is_ok());
        assert!(validate_config_override("/home/user/.bubbaloop/config.yaml").is_ok());
        assert!(validate_config_override("config.yaml").is_ok());
    }

    #[test]
    fn test_validate_config_override_rejects_systemd_specifiers() {
        assert!(validate_config_override("/etc/%n/config.yaml").is_err());
        assert!(validate_config_override("/home/%h/config.yaml").is_err());
        assert!(validate_config_override("%i.yaml").is_err());
    }

    #[test]
    fn test_validate_config_override_rejects_path_traversal() {
        assert!(validate_config_override("/etc/../passwd").is_err());
        assert!(validate_config_override("../secret.yaml").is_err());
        assert!(validate_config_override("/home/user/../../etc/shadow").is_err());
    }

    #[test]
    fn test_validate_config_override_rejects_null_bytes() {
        assert!(validate_config_override("/etc/config\0.yaml").is_err());
    }

    #[test]
    fn test_validate_config_override_rejects_newlines() {
        assert!(validate_config_override("/etc/config\n.yaml").is_err());
        assert!(validate_config_override("/etc/config\r.yaml").is_err());
    }

    #[test]
    fn test_validate_config_override_rejects_empty() {
        assert!(validate_config_override("").is_err());
    }

    // -----------------------------------------------------------------------
    // check_is_built tests
    // -----------------------------------------------------------------------

    fn manifest_with(node_type: &str, command: Option<&str>) -> NodeManifest {
        NodeManifest {
            name: "test-node".to_string(),
            version: "1.0.0".to_string(),
            node_type: node_type.to_string(),
            description: "Test".to_string(),
            command: command.map(String::from),
            ..Default::default()
        }
    }

    #[test]
    fn test_is_built_empty_command_returns_false() {
        let dir = tempfile::tempdir().unwrap();
        let m = manifest_with("python", Some(""));
        assert!(!check_is_built(dir.path().to_str().unwrap(), &m));
    }

    #[test]
    fn test_is_built_m_without_module_returns_false() {
        let dir = tempfile::tempdir().unwrap();
        let m = manifest_with("python", Some("pixi run python -m"));
        assert!(!check_is_built(dir.path().to_str().unwrap(), &m));
    }

    #[test]
    fn test_is_built_rust_release() {
        let dir = tempfile::tempdir().unwrap();
        let release_dir = dir.path().join("target/release");
        std::fs::create_dir_all(&release_dir).unwrap();
        std::fs::write(release_dir.join("test-node"), b"").unwrap();
        assert!(check_is_built(
            dir.path().to_str().unwrap(),
            &manifest_with("rust", None)
        ));
    }

    #[test]
    fn test_is_built_rust_debug() {
        let dir = tempfile::tempdir().unwrap();
        let debug_dir = dir.path().join("target/debug");
        std::fs::create_dir_all(&debug_dir).unwrap();
        std::fs::write(debug_dir.join("test-node"), b"").unwrap();
        assert!(check_is_built(
            dir.path().to_str().unwrap(),
            &manifest_with("rust", None)
        ));
    }

    #[test]
    fn test_is_built_rust_missing() {
        let dir = tempfile::tempdir().unwrap();
        assert!(!check_is_built(
            dir.path().to_str().unwrap(),
            &manifest_with("rust", None)
        ));
    }

    #[test]
    fn test_is_built_rust_non_standard_command() {
        let dir = tempfile::tempdir().unwrap();
        // Non-standard cross-compilation target path
        let cross_dir = dir.path().join("target/aarch64-unknown-linux-gnu/release");
        std::fs::create_dir_all(&cross_dir).unwrap();
        std::fs::write(cross_dir.join("test-node"), b"").unwrap();
        let m = manifest_with(
            "rust",
            Some("target/aarch64-unknown-linux-gnu/release/test-node"),
        );
        assert!(check_is_built(dir.path().to_str().unwrap(), &m));
    }

    #[test]
    fn test_is_built_no_command_finds_main_py() {
        let dir = tempfile::tempdir().unwrap();
        std::fs::write(dir.path().join("main.py"), b"").unwrap();
        assert!(check_is_built(
            dir.path().to_str().unwrap(),
            &manifest_with("python", None)
        ));
    }

    #[test]
    fn test_is_built_no_command_missing_main_py() {
        let dir = tempfile::tempdir().unwrap();
        assert!(!check_is_built(
            dir.path().to_str().unwrap(),
            &manifest_with("python", None)
        ));
    }

    #[test]
    fn test_is_built_module_flag() {
        let dir = tempfile::tempdir().unwrap();
        std::fs::create_dir_all(dir.path().join("smartpower/nodes")).unwrap();
        std::fs::write(dir.path().join("smartpower/nodes/runner.py"), b"").unwrap();
        let m = manifest_with(
            "python",
            Some("pixi run python -m smartpower.nodes.runner config.yaml"),
        );
        assert!(check_is_built(dir.path().to_str().unwrap(), &m));
    }

    #[test]
    fn test_is_built_absolute_interpreter_no_false_positive() {
        let dir = tempfile::tempdir().unwrap();
        // /usr/bin/python3 exists on the system but main.py is missing
        let m = manifest_with("python", Some("/usr/bin/python3 main.py"));
        assert!(!check_is_built(dir.path().to_str().unwrap(), &m));
    }

    #[test]
    fn test_is_built_absolute_interpreter_with_script() {
        let dir = tempfile::tempdir().unwrap();
        std::fs::write(dir.path().join("main.py"), b"").unwrap();
        let m = manifest_with("python", Some("/usr/bin/python3 main.py"));
        assert!(check_is_built(dir.path().to_str().unwrap(), &m));
    }

    #[test]
    fn test_is_built_non_default_script() {
        // "pixi run python sensor.py" — script is not main.py
        let dir = tempfile::tempdir().unwrap();
        std::fs::write(dir.path().join("sensor.py"), b"").unwrap();
        let m = manifest_with("python", Some("pixi run python sensor.py"));
        assert!(check_is_built(dir.path().to_str().unwrap(), &m));
    }

    #[test]
    fn test_is_built_pixi_task_no_py_returns_true() {
        // "pixi run run" — no .py token, external launcher, assume built
        let dir = tempfile::tempdir().unwrap();
        let m = manifest_with("python", Some("pixi run run"));
        assert!(check_is_built(dir.path().to_str().unwrap(), &m));
    }
}
