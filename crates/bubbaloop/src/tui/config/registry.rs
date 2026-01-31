use std::fs;
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

use crate::tui::app::{DiscoverableNode, MarketplaceSource, NodeInfo};

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
        let sources = if sources_path.exists() {
            fs::read_to_string(&sources_path)
                .ok()
                .and_then(|s| serde_json::from_str(&s).ok())
                .unwrap_or_default()
        } else {
            SourcesRegistry::default()
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
        let registered_paths: std::collections::HashSet<_> = registered_nodes
            .iter()
            .map(|n| normalize_path(&n.path))
            .collect();
        let mut discovered_paths: std::collections::HashSet<String> =
            std::collections::HashSet::new();

        // Scan enabled sources (first, so configured sources win over local dev)
        for source in self.get_enabled_sources() {
            let found = scan_for_nodes(&source.path);
            for node in found {
                let normalized = normalize_path(&node.path);
                if !registered_paths.contains(&normalized)
                    && !discovered_paths.contains(&normalized)
                {
                    discovered_paths.insert(normalized);
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

        discovered
    }
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
