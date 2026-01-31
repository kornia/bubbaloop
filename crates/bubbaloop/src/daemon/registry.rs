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

/// Node manifest from node.yaml
#[derive(Debug, Clone, Serialize, Deserialize)]
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

/// Register a new node
pub fn register_node(node_path: &str) -> Result<NodeManifest> {
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

    // Load registry
    let mut registry = load_registry()?;

    // Check if already registered
    let canonical = path
        .canonicalize()
        .unwrap_or_else(|_| path.to_path_buf())
        .to_string_lossy()
        .to_string();

    if registry.nodes.iter().any(|n| {
        let n_path = Path::new(&n.path);
        let n_canonical = n_path
            .canonicalize()
            .unwrap_or_else(|_| n_path.to_path_buf());
        n_canonical.to_string_lossy() == canonical
    }) {
        return Err(RegistryError::NodeAlreadyRegistered(node_path.to_string()));
    }

    // Add to registry
    registry.nodes.push(NodeEntry {
        path: canonical,
        added_at: chrono_now(),
    });

    save_registry(&registry)?;
    Ok(manifest)
}

/// Unregister a node
pub fn unregister_node(node_path: &str) -> Result<()> {
    let mut registry = load_registry()?;

    let path = Path::new(node_path);
    let canonical = path
        .canonicalize()
        .unwrap_or_else(|_| path.to_path_buf())
        .to_string_lossy()
        .to_string();

    let initial_len = registry.nodes.len();
    registry.nodes.retain(|n| {
        let n_path = Path::new(&n.path);
        let n_canonical = n_path
            .canonicalize()
            .unwrap_or_else(|_| n_path.to_path_buf());
        n_canonical.to_string_lossy() != canonical
    });

    if registry.nodes.len() == initial_len {
        return Err(RegistryError::NodeNotFound(node_path.to_string()));
    }

    save_registry(&registry)?;
    Ok(())
}

/// List all registered nodes with their manifests
pub fn list_nodes() -> Result<Vec<(String, Option<NodeManifest>)>> {
    let registry = load_registry()?;

    let nodes = registry
        .nodes
        .iter()
        .map(|entry| {
            let path = Path::new(&entry.path);
            let manifest = read_manifest(path).ok();
            (entry.path.clone(), manifest)
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
            depends_on: vec![],
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
            author: None,
            build: None,
            command: None,
            depends_on: vec![],
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
            author: None,
            build: None,
            command: None,
            depends_on: vec![],
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
            author: None,
            build: None,
            command: None,
            depends_on: vec![],
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
            author: None,
            build: None,
            command: None,
            depends_on: vec![],
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
            author: None,
            build: None,
            command: None,
            depends_on: vec![],
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
            author: None,
            build: None,
            command: None,
            depends_on: vec![],
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
            author: None,
            build: None,
            command: None,
            depends_on: vec![],
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
            author: None,
            build: None,
            command: None,
            depends_on: vec![],
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
            author: None,
            build: None,
            command: None,
            depends_on: vec![],
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
            author: None,
            build: None,
            command: None,
            depends_on: vec![],
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
            author: None,
            build: None,
            command: Some("./target\0/release/test".to_string()),
            depends_on: vec![],
        };
        assert!(manifest.validate().is_err());
    }
}
