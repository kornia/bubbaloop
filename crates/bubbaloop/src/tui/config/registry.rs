use std::collections::HashSet;
use std::fs;
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

use crate::registry as shared_registry;
use crate::tui::app::{DiscoverableNode, MarketplaceSource, NodeInfo};

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
        let mut sources: SourcesRegistry = if sources_path.exists() {
            fs::read_to_string(&sources_path)
                .ok()
                .and_then(|s| serde_json::from_str(&s).ok())
                .unwrap_or_default()
        } else {
            SourcesRegistry::default()
        };

        // Ensure the official builtin source exists (handles fresh installs and upgrades)
        let has_builtin = sources
            .sources
            .iter()
            .any(|s| s.source_type == OFFICIAL_SOURCE_TYPE);
        if !has_builtin {
            sources.sources.push(SourceEntry {
                name: OFFICIAL_SOURCE_NAME.to_string(),
                path: OFFICIAL_SOURCE_PATH.to_string(),
                source_type: OFFICIAL_SOURCE_TYPE.to_string(),
                enabled: true,
            });
            if let Ok(json) = serde_json::to_string_pretty(&sources) {
                let _ = fs::write(&sources_path, json);
            }
        }

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

    /// Trigger a background fetch of the official nodes registry from GitHub.
    /// The result is cached to disk; `scan_discoverable_nodes` reads from cache.
    pub fn refresh_builtin_cache(&self) {
        shared_registry::refresh_cache_async();
    }

    pub fn scan_discoverable_nodes(&self, registered_nodes: &[NodeInfo]) -> Vec<DiscoverableNode> {
        let mut discovered = Vec::new();
        let mut discovered_names: HashSet<String> = HashSet::new();
        let mut discovered_paths: HashSet<String> = HashSet::new();

        // Build lookup maps from registered nodes for status computation
        let registered_by_name: std::collections::HashMap<&str, &NodeInfo> = registered_nodes
            .iter()
            .filter(|n| n.base_node.is_empty()) // Only base nodes
            .map(|n| (n.name.as_str(), n))
            .collect();

        // Scan enabled sources — always return all marketplace nodes (no dedup filtering)
        for source in self.get_enabled_sources() {
            if source.source_type == OFFICIAL_SOURCE_TYPE {
                for mut node in self.load_builtin_nodes() {
                    if !discovered_names.contains(&node.name) {
                        // Compute status from registered nodes
                        if let Some(reg) = registered_by_name.get(node.name.as_str()) {
                            node.is_added = true;
                            node.is_built = reg.is_built;
                        }
                        node.instance_count = registered_nodes
                            .iter()
                            .filter(|n| n.base_node == node.name)
                            .count();
                        discovered_names.insert(node.name.clone());
                        discovered.push(node);
                    }
                }
            } else {
                let found = scan_for_nodes(&source.path);
                for node in found {
                    let normalized = normalize_path(&node.path);
                    if !discovered_paths.contains(&normalized)
                        && !discovered_names.contains(&node.name)
                    {
                        let is_added = registered_by_name.contains_key(node.name.as_str());
                        let is_built = registered_by_name
                            .get(node.name.as_str())
                            .map(|n| n.is_built)
                            .unwrap_or(false);
                        let instance_count = registered_nodes
                            .iter()
                            .filter(|n| n.base_node == node.name)
                            .count();
                        discovered_paths.insert(normalized);
                        discovered_names.insert(node.name.clone());
                        discovered.push(DiscoverableNode {
                            path: node.path,
                            name: node.name,
                            version: node.version,
                            node_type: node.node_type,
                            description: node.description,
                            source: source.name.clone(),
                            source_type: source.source_type.clone(),
                            is_added,
                            is_built,
                            instance_count,
                        });
                    }
                }
            }
        }

        discovered
    }

    /// Load official nodes from the local cache, delegating parsing to the shared registry.
    fn load_builtin_nodes(&self) -> Vec<DiscoverableNode> {
        let cache_path = self
            .config_dir
            .join("cache")
            .join(shared_registry::OFFICIAL_NODES_CACHE);
        let yaml = match fs::read_to_string(&cache_path) {
            Ok(s) => s,
            Err(_) => return Vec::new(),
        };

        shared_registry::parse_nodes_yaml(&yaml)
            .into_iter()
            .map(|entry| {
                let path = format!("{} --subdir {}", entry.repo, entry.subdir);
                DiscoverableNode {
                    path,
                    name: entry.name,
                    version: entry.version,
                    node_type: entry.node_type,
                    description: entry.description,
                    source: OFFICIAL_SOURCE_NAME.to_string(),
                    source_type: OFFICIAL_SOURCE_TYPE.to_string(),
                    is_added: false,
                    is_built: false,
                    instance_count: 0,
                }
            })
            .collect()
    }
}

struct ScannedNode {
    path: String,
    name: String,
    version: String,
    node_type: String,
    description: String,
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
                                description: manifest.description,
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
    expand_tilde(p)
}

fn expand_tilde(path: &str) -> String {
    if path.starts_with('~') {
        if let Some(home) = dirs::home_dir() {
            return path.replacen('~', &home.to_string_lossy(), 1);
        }
    }
    path.to_string()
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Sample YAML matching the nodes.yaml schema in bubbaloop-nodes-official.
    const TEST_NODES_YAML: &str = r#"
nodes:
  - name: rtsp-camera
    description: "RTSP camera capture"
    version: "0.1.0"
    type: rust
    category: camera
    tags: [video]
    repo: kornia/bubbaloop-nodes-official
    subdir: rtsp-camera
    binary: cameras_node

  - name: openmeteo
    description: "Weather data"
    version: "0.1.0"
    type: rust
    category: weather
    tags: [weather]
    repo: kornia/bubbaloop-nodes-official
    subdir: openmeteo
    binary: openmeteo_node

  - name: network-monitor
    description: "Network monitor"
    version: "0.1.0"
    type: python
    category: monitoring
    tags: [network]
    repo: kornia/bubbaloop-nodes-official
    subdir: network-monitor
"#;

    #[test]
    fn test_load_builtin_nodes_from_shared_registry() {
        let dir = tempfile::tempdir().unwrap();
        let cache_dir = dir.path().join("cache");
        let _ = fs::create_dir_all(&cache_dir);
        let _ = fs::write(
            cache_dir.join(shared_registry::OFFICIAL_NODES_CACHE),
            TEST_NODES_YAML,
        );

        let registry = Registry {
            config_dir: dir.path().to_path_buf(),
            sources: SourcesRegistry::default(),
        };

        let nodes = registry.load_builtin_nodes();
        assert_eq!(nodes.len(), 3);

        let camera = nodes.iter().find(|n| n.name == "rtsp-camera").unwrap();
        assert_eq!(camera.version, "0.1.0");
        assert_eq!(camera.node_type, "rust");
        assert_eq!(camera.source, OFFICIAL_SOURCE_NAME);
        assert_eq!(camera.source_type, OFFICIAL_SOURCE_TYPE);
        assert_eq!(camera.description, "RTSP camera capture");
        assert_eq!(
            camera.path,
            "kornia/bubbaloop-nodes-official --subdir rtsp-camera"
        );

        let netmon = nodes.iter().find(|n| n.name == "network-monitor").unwrap();
        assert_eq!(netmon.node_type, "python");
        assert_eq!(netmon.source_type, OFFICIAL_SOURCE_TYPE);
    }

    #[test]
    fn test_parse_nodes_yaml_invalid() {
        let nodes = shared_registry::parse_nodes_yaml("not valid yaml: [[[");
        assert!(nodes.is_empty());
    }

    #[test]
    fn test_official_source_seeded_on_fresh_install() {
        let dir = tempfile::tempdir().unwrap();
        let sources_path = dir.path().join("sources.json");

        assert!(!sources_path.exists());

        // Simulate Registry::load() logic for a fresh install (no sources.json)
        let mut sources = SourcesRegistry::default();
        let has_builtin = sources
            .sources
            .iter()
            .any(|s| s.source_type == OFFICIAL_SOURCE_TYPE);
        assert!(!has_builtin);

        sources.sources.push(SourceEntry {
            name: OFFICIAL_SOURCE_NAME.to_string(),
            path: OFFICIAL_SOURCE_PATH.to_string(),
            source_type: OFFICIAL_SOURCE_TYPE.to_string(),
            enabled: true,
        });
        let _ = fs::write(
            &sources_path,
            serde_json::to_string_pretty(&sources).unwrap(),
        );

        let registry = Registry {
            config_dir: dir.path().to_path_buf(),
            sources,
        };

        assert!(sources_path.exists());
        let result = registry.get_sources();
        assert_eq!(result.len(), 1);
        assert_eq!(result[0].name, OFFICIAL_SOURCE_NAME);
        assert_eq!(result[0].source_type, OFFICIAL_SOURCE_TYPE);
        assert!(result[0].enabled);
    }

    #[test]
    fn test_official_source_added_on_upgrade() {
        let dir = tempfile::tempdir().unwrap();
        let sources_path = dir.path().join("sources.json");

        // Simulate an existing sources.json without the builtin source (upgrade case)
        let old_sources = SourcesRegistry {
            sources: vec![SourceEntry {
                name: "My Local Nodes".to_string(),
                path: "/some/local/path".to_string(),
                source_type: "local".to_string(),
                enabled: true,
            }],
        };
        let _ = fs::write(
            &sources_path,
            serde_json::to_string_pretty(&old_sources).unwrap(),
        );

        // Simulate Registry::load() logic
        let mut sources: SourcesRegistry =
            serde_json::from_str(&fs::read_to_string(&sources_path).unwrap()).unwrap();
        let has_builtin = sources
            .sources
            .iter()
            .any(|s| s.source_type == OFFICIAL_SOURCE_TYPE);
        assert!(!has_builtin);

        sources.sources.push(SourceEntry {
            name: OFFICIAL_SOURCE_NAME.to_string(),
            path: OFFICIAL_SOURCE_PATH.to_string(),
            source_type: OFFICIAL_SOURCE_TYPE.to_string(),
            enabled: true,
        });

        let registry = Registry {
            config_dir: dir.path().to_path_buf(),
            sources,
        };

        let result = registry.get_sources();
        assert_eq!(result.len(), 2);
        assert_eq!(result[0].name, "My Local Nodes");
        assert_eq!(result[1].name, OFFICIAL_SOURCE_NAME);
        assert_eq!(result[1].source_type, OFFICIAL_SOURCE_TYPE);
    }

    #[test]
    fn test_scan_returns_all_nodes_with_status() {
        let dir = tempfile::tempdir().unwrap();

        // Write a cached nodes.yaml
        let cache_dir = dir.path().join("cache");
        let _ = fs::create_dir_all(&cache_dir);
        let _ = fs::write(
            cache_dir.join(shared_registry::OFFICIAL_NODES_CACHE),
            TEST_NODES_YAML,
        );

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

        // rtsp-camera is registered (base node, built)
        let registered = vec![
            NodeInfo {
                name: "rtsp-camera".to_string(),
                path: "/some/local/path".to_string(),
                version: "0.1.0".to_string(),
                node_type: "rust".to_string(),
                description: String::new(),
                status: "stopped".to_string(),
                is_built: true,
                build_output: Vec::new(),
                base_node: String::new(),
                config_override: String::new(),
            },
            // An instance of rtsp-camera
            NodeInfo {
                name: "rtsp-camera-terrace".to_string(),
                path: "/some/local/path".to_string(),
                version: "0.1.0".to_string(),
                node_type: "rust".to_string(),
                description: String::new(),
                status: "running".to_string(),
                is_built: true,
                build_output: Vec::new(),
                base_node: "rtsp-camera".to_string(),
                config_override: String::new(),
            },
        ];

        let discovered = registry.scan_discoverable_nodes(&registered);

        // All 3 marketplace nodes should be returned (no dedup filtering)
        assert_eq!(discovered.len(), 3);

        // rtsp-camera should be marked as added and built
        let camera = discovered.iter().find(|n| n.name == "rtsp-camera").unwrap();
        assert!(camera.is_added);
        assert!(camera.is_built);
        assert_eq!(camera.instance_count, 1); // one instance exists

        // openmeteo should NOT be marked as added
        let meteo = discovered.iter().find(|n| n.name == "openmeteo").unwrap();
        assert!(!meteo.is_added);
        assert!(!meteo.is_built);
        assert_eq!(meteo.instance_count, 0);

        // network-monitor should NOT be marked as added
        let netmon = discovered
            .iter()
            .find(|n| n.name == "network-monitor")
            .unwrap();
        assert!(!netmon.is_added);
        assert!(!netmon.is_built);
        assert_eq!(netmon.instance_count, 0);
    }

    #[test]
    fn test_scan_not_built_node() {
        let dir = tempfile::tempdir().unwrap();

        let cache_dir = dir.path().join("cache");
        let _ = fs::create_dir_all(&cache_dir);
        let _ = fs::write(
            cache_dir.join(shared_registry::OFFICIAL_NODES_CACHE),
            TEST_NODES_YAML,
        );

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

        // rtsp-camera is registered but NOT built
        let registered = vec![NodeInfo {
            name: "rtsp-camera".to_string(),
            path: "/some/local/path".to_string(),
            version: "0.1.0".to_string(),
            node_type: "rust".to_string(),
            description: String::new(),
            status: "stopped".to_string(),
            is_built: false,
            build_output: Vec::new(),
            base_node: String::new(),
            config_override: String::new(),
        }];

        let discovered = registry.scan_discoverable_nodes(&registered);
        let camera = discovered.iter().find(|n| n.name == "rtsp-camera").unwrap();
        assert!(camera.is_added);
        assert!(!camera.is_built); // Added but not built
        assert_eq!(camera.instance_count, 0);
    }

    #[test]
    fn test_scan_multiple_instances_counted() {
        let dir = tempfile::tempdir().unwrap();

        let cache_dir = dir.path().join("cache");
        let _ = fs::create_dir_all(&cache_dir);
        let _ = fs::write(
            cache_dir.join(shared_registry::OFFICIAL_NODES_CACHE),
            TEST_NODES_YAML,
        );

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

        let make_instance = |name: &str, base: &str| NodeInfo {
            name: name.to_string(),
            path: "/path".to_string(),
            version: "0.1.0".to_string(),
            node_type: "rust".to_string(),
            description: String::new(),
            status: "running".to_string(),
            is_built: true,
            build_output: Vec::new(),
            base_node: base.to_string(),
            config_override: String::new(),
        };

        let registered = vec![
            NodeInfo {
                name: "rtsp-camera".to_string(),
                path: "/path".to_string(),
                version: "0.1.0".to_string(),
                node_type: "rust".to_string(),
                description: String::new(),
                status: "stopped".to_string(),
                is_built: true,
                build_output: Vec::new(),
                base_node: String::new(),
                config_override: String::new(),
            },
            make_instance("rtsp-camera-terrace", "rtsp-camera"),
            make_instance("rtsp-camera-garage", "rtsp-camera"),
            make_instance("rtsp-camera-front", "rtsp-camera"),
        ];

        let discovered = registry.scan_discoverable_nodes(&registered);
        let camera = discovered.iter().find(|n| n.name == "rtsp-camera").unwrap();
        assert_eq!(camera.instance_count, 3);
    }

    #[test]
    fn test_load_builtin_nodes_no_cache() {
        let dir = tempfile::tempdir().unwrap();
        let registry = Registry {
            config_dir: dir.path().to_path_buf(),
            sources: SourcesRegistry::default(),
        };
        // No cache file => empty result
        assert!(registry.load_builtin_nodes().is_empty());
    }
}
