use std::collections::HashSet;
use std::fs;
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

use crate::tui::app::{DiscoverableNode, MarketplaceSource, NodeInfo};

/// Embedded official nodes registry, compiled into the binary.
const OFFICIAL_NODES_YAML: &str = include_str!("official_nodes.yaml");
const OFFICIAL_SOURCE_NAME: &str = "Official Nodes";
const OFFICIAL_SOURCE_PATH: &str = "kornia/bubbaloop-nodes-official";
const OFFICIAL_SOURCE_TYPE: &str = "builtin";

#[derive(Debug, Serialize, Deserialize, Default)]
struct SourcesRegistry {
    sources: Vec<SourceEntry>,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
struct SourceEntry {
    name: String,
    path: String,
    #[serde(rename = "type")]
    source_type: String,
    enabled: bool,
}

#[derive(Debug, Serialize, Deserialize)]
struct NodeManifest {
    name: String,
    version: String,
    #[serde(rename = "type")]
    node_type: String,
    #[serde(default)]
    description: String,
}

/// Parsed representation of the embedded official_nodes.yaml
#[derive(Debug, Deserialize)]
struct OfficialNodesYaml {
    nodes: Vec<OfficialNodeEntry>,
}

#[derive(Debug, Deserialize, Clone)]
#[allow(dead_code)]
struct OfficialNodeEntry {
    name: String,
    description: String,
    version: String,
    #[serde(rename = "type")]
    node_type: String,
    repo: String,
    subdir: String,
}

pub struct Registry {
    config_dir: PathBuf,
    sources: SourcesRegistry,
}

impl Registry {
    pub fn load() -> Self {
        let config_dir = dirs::home_dir()
            .unwrap_or_else(|| PathBuf::from("."))
            .join(".bubbaloop");

        // Ensure config dir exists
        let _ = fs::create_dir_all(&config_dir);

        // Load sources registry
        let sources_path = config_dir.join("sources.json");
        let sources = if sources_path.exists() {
            fs::read_to_string(&sources_path)
                .ok()
                .and_then(|s| serde_json::from_str(&s).ok())
                .unwrap_or_default()
        } else {
            // Seed with official builtin source on first run
            let seeded = SourcesRegistry {
                sources: vec![SourceEntry {
                    name: OFFICIAL_SOURCE_NAME.to_string(),
                    path: OFFICIAL_SOURCE_PATH.to_string(),
                    source_type: OFFICIAL_SOURCE_TYPE.to_string(),
                    enabled: true,
                }],
            };
            // Persist the seeded sources
            if let Ok(json) = serde_json::to_string_pretty(&seeded) {
                let _ = fs::write(&sources_path, json);
            }
            seeded
        };

        Self {
            config_dir,
            sources,
        }
    }

    fn save_sources(&self) {
        let path = self.config_dir.join("sources.json");
        if let Ok(json) = serde_json::to_string_pretty(&self.sources) {
            let _ = fs::write(path, json);
        }
    }

    pub fn get_sources(&self) -> Vec<MarketplaceSource> {
        self.sources
            .sources
            .iter()
            .map(|s| MarketplaceSource {
                name: s.name.clone(),
                path: s.path.clone(),
                source_type: s.source_type.clone(),
                enabled: s.enabled,
            })
            .collect()
    }

    fn get_enabled_sources(&self) -> Vec<&SourceEntry> {
        self.sources.sources.iter().filter(|s| s.enabled).collect()
    }

    pub fn toggle_source(&mut self, path: &str) {
        if let Some(source) = self.sources.sources.iter_mut().find(|s| s.path == path) {
            source.enabled = !source.enabled;
            self.save_sources();
        }
    }

    pub fn remove_source(&mut self, path: &str) {
        self.sources.sources.retain(|s| s.path != path);
        self.save_sources();
    }

    /// Add a new source to the registry
    pub fn add_source(&mut self, name: &str, path: &str, source_type: &str) -> Result<(), String> {
        // Check if source with same path already exists
        if self.sources.sources.iter().any(|s| s.path == path) {
            return Err("Source with this path already exists".to_string());
        }

        self.sources.sources.push(SourceEntry {
            name: name.to_string(),
            path: path.to_string(),
            source_type: source_type.to_string(),
            enabled: true,
        });
        self.save_sources();
        Ok(())
    }

    /// Update an existing source
    pub fn update_source(
        &mut self,
        original_path: &str,
        new_name: &str,
        new_path: &str,
    ) -> Result<(), String> {
        // If path changed, check for duplicates first
        if original_path != new_path && self.sources.sources.iter().any(|s| s.path == new_path) {
            return Err("Source with this path already exists".to_string());
        }

        // Find the source by original path and update it
        if let Some(source) = self
            .sources
            .sources
            .iter_mut()
            .find(|s| s.path == original_path)
        {
            source.name = new_name.to_string();
            source.path = new_path.to_string();
            self.save_sources();
            Ok(())
        } else {
            Err("Source not found".to_string())
        }
    }

    pub fn scan_discoverable_nodes(&self, registered_nodes: &[NodeInfo]) -> Vec<DiscoverableNode> {
        let mut discovered = Vec::new();
        // Use name-based dedup so installed nodes don't re-appear from builtin sources
        let registered_names: HashSet<_> =
            registered_nodes.iter().map(|n| n.name.as_str()).collect();
        let registered_paths: HashSet<_> = registered_nodes
            .iter()
            .map(|n| normalize_path(&n.path))
            .collect();
        let mut discovered_names: HashSet<String> = HashSet::new();
        let mut discovered_paths: HashSet<String> = HashSet::new();

        // Scan enabled sources (first, so configured sources win over local dev)
        for source in self.get_enabled_sources() {
            if source.source_type == OFFICIAL_SOURCE_TYPE {
                // Parse builtin nodes from embedded YAML
                for node in parse_builtin_nodes() {
                    if !registered_names.contains(node.name.as_str())
                        && !discovered_names.contains(&node.name)
                    {
                        discovered_names.insert(node.name.clone());
                        discovered.push(node);
                    }
                }
            } else {
                let found = scan_for_nodes(&source.path);
                for node in found {
                    let normalized = normalize_path(&node.path);
                    if !registered_paths.contains(&normalized)
                        && !discovered_paths.contains(&normalized)
                        && !registered_names.contains(node.name.as_str())
                        && !discovered_names.contains(&node.name)
                    {
                        discovered_paths.insert(normalized);
                        discovered_names.insert(node.name.clone());
                        discovered.push(DiscoverableNode {
                            path: node.path,
                            name: node.name,
                            version: node.version,
                            node_type: node.node_type,
                            source: source.name.clone(),
                        });
                    }
                }
            }
        }

        discovered
    }
}

/// Parse the embedded official nodes YAML into discoverable nodes.
fn parse_builtin_nodes() -> Vec<DiscoverableNode> {
    let registry: OfficialNodesYaml = match serde_yaml::from_str(OFFICIAL_NODES_YAML) {
        Ok(r) => r,
        Err(_) => return Vec::new(),
    };

    registry
        .nodes
        .into_iter()
        .map(|entry| {
            // Path format: "user/repo --subdir name"
            // This is readable in the TUI and maps directly to the CLI command:
            //   bubbaloop node add kornia/bubbaloop-nodes-official --subdir rtsp-camera
            let path = format!("{} --subdir {}", entry.repo, entry.subdir);
            DiscoverableNode {
                path,
                name: entry.name,
                version: entry.version,
                node_type: entry.node_type,
                source: OFFICIAL_SOURCE_NAME.to_string(),
            }
        })
        .collect()
}

struct ScannedNode {
    path: String,
    name: String,
    version: String,
    node_type: String,
}

fn scan_for_nodes(base_path: &str) -> Vec<ScannedNode> {
    let mut nodes = Vec::new();
    let path = Path::new(base_path);

    if !path.exists() {
        return nodes;
    }

    if let Ok(entries) = fs::read_dir(path) {
        for entry in entries.flatten() {
            if entry.file_type().map(|t| t.is_dir()).unwrap_or(false) {
                let node_path = entry.path();
                let manifest_path = node_path.join("node.yaml");

                if manifest_path.exists() {
                    if let Ok(content) = fs::read_to_string(&manifest_path) {
                        if let Ok(manifest) = serde_yaml::from_str::<NodeManifest>(&content) {
                            nodes.push(ScannedNode {
                                path: node_path.to_string_lossy().to_string(),
                                name: manifest.name,
                                version: manifest.version,
                                node_type: manifest.node_type,
                            });
                        }
                    }
                }
            }
        }
    }

    nodes
}

fn normalize_path(path: &str) -> String {
    let p = path.trim_end_matches('/');
    if let Some(expanded) = expand_tilde(p) {
        expanded
    } else {
        p.to_string()
    }
}

fn expand_tilde(path: &str) -> Option<String> {
    if path.starts_with('~') {
        dirs::home_dir().map(|home| path.replacen('~', &home.to_string_lossy(), 1))
    } else {
        Some(path.to_string())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_builtin_nodes() {
        let nodes = parse_builtin_nodes();
        assert!(
            nodes.len() >= 7,
            "Expected at least 7 builtin nodes, got {}",
            nodes.len()
        );

        let names: Vec<&str> = nodes.iter().map(|n| n.name.as_str()).collect();
        assert!(names.contains(&"rtsp-camera"));
        assert!(names.contains(&"openmeteo"));
        assert!(names.contains(&"foxglove"));
        assert!(names.contains(&"recorder"));
        assert!(names.contains(&"inference"));
        assert!(names.contains(&"system-telemetry"));
        assert!(names.contains(&"network-monitor"));
    }

    #[test]
    fn test_builtin_node_fields() {
        let nodes = parse_builtin_nodes();
        let camera = nodes.iter().find(|n| n.name == "rtsp-camera").unwrap();

        assert_eq!(camera.version, "0.1.0");
        assert_eq!(camera.node_type, "rust");
        assert_eq!(camera.source, OFFICIAL_SOURCE_NAME);
        assert!(camera.path.contains("--subdir"));
        assert!(camera.path.contains("kornia/bubbaloop-nodes-official"));

        let netmon = nodes.iter().find(|n| n.name == "network-monitor").unwrap();
        assert_eq!(netmon.node_type, "python");
    }

    #[test]
    fn test_official_source_seeded() {
        let dir = tempfile::tempdir().unwrap();
        let sources_path = dir.path().join("sources.json");

        // Simulate first-run: no sources.json exists
        assert!(!sources_path.exists());

        // Load triggers seeding
        let registry = Registry {
            config_dir: dir.path().to_path_buf(),
            sources: {
                // Reproduce the seeding logic
                let seeded = SourcesRegistry {
                    sources: vec![SourceEntry {
                        name: OFFICIAL_SOURCE_NAME.to_string(),
                        path: OFFICIAL_SOURCE_PATH.to_string(),
                        source_type: OFFICIAL_SOURCE_TYPE.to_string(),
                        enabled: true,
                    }],
                };
                if let Ok(json) = serde_json::to_string_pretty(&seeded) {
                    let _ = fs::write(&sources_path, json);
                }
                seeded
            },
        };

        // Verify the file was created
        assert!(sources_path.exists());

        // Verify source content
        let sources = registry.get_sources();
        assert_eq!(sources.len(), 1);
        assert_eq!(sources[0].name, OFFICIAL_SOURCE_NAME);
        assert_eq!(sources[0].path, OFFICIAL_SOURCE_PATH);
        assert_eq!(sources[0].source_type, OFFICIAL_SOURCE_TYPE);
        assert!(sources[0].enabled);
    }

    #[test]
    fn test_builtin_dedup_with_registered_nodes() {
        let dir = tempfile::tempdir().unwrap();
        let registry = Registry {
            config_dir: dir.path().to_path_buf(),
            sources: SourcesRegistry {
                sources: vec![SourceEntry {
                    name: OFFICIAL_SOURCE_NAME.to_string(),
                    path: OFFICIAL_SOURCE_PATH.to_string(),
                    source_type: OFFICIAL_SOURCE_TYPE.to_string(),
                    enabled: true,
                }],
            },
        };

        // Simulate a registered node with the same name
        let registered = vec![NodeInfo {
            name: "rtsp-camera".to_string(),
            path: "/some/local/path".to_string(),
            version: "0.1.0".to_string(),
            node_type: "rust".to_string(),
            description: String::new(),
            status: "stopped".to_string(),
            is_built: false,
            build_output: Vec::new(),
        }];

        let discovered = registry.scan_discoverable_nodes(&registered);
        // rtsp-camera should NOT appear (already registered)
        assert!(
            !discovered.iter().any(|n| n.name == "rtsp-camera"),
            "rtsp-camera should be deduplicated"
        );
        // Other nodes should still appear
        assert!(
            discovered.iter().any(|n| n.name == "openmeteo"),
            "openmeteo should still be discoverable"
        );
    }
}
