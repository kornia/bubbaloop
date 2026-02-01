//! Shared node registry for marketplace operations.
//!
//! Provides lookup, search, and caching of the official nodes registry
//! (fetched from GitHub). Used by both CLI and TUI.

use serde::Deserialize;
use std::fs;
use std::path::{Path, PathBuf};

/// Raw URL for the official nodes registry on GitHub.
pub const OFFICIAL_NODES_URL: &str =
    "https://raw.githubusercontent.com/kornia/bubbaloop-nodes-official/main/nodes.yaml";

/// Local cache filename inside ~/.bubbaloop/cache/
pub const OFFICIAL_NODES_CACHE: &str = "official_nodes.yaml";

/// A node entry from the official registry.
#[derive(Debug, Clone, Deserialize)]
pub struct RegistryNode {
    pub name: String,
    pub version: String,
    #[serde(rename = "type")]
    pub node_type: String,
    #[serde(default)]
    pub description: String,
    #[serde(default)]
    pub category: String,
    #[serde(default)]
    pub tags: Vec<String>,
    pub repo: String,
    pub subdir: String,
}

#[derive(Debug, Deserialize)]
struct OfficialNodesYaml {
    nodes: Vec<RegistryNode>,
}

/// Parse a nodes.yaml string into a list of `RegistryNode`.
pub fn parse_nodes_yaml(yaml: &str) -> Vec<RegistryNode> {
    let registry: OfficialNodesYaml = match serde_yaml::from_str(yaml) {
        Ok(r) => r,
        Err(_) => return Vec::new(),
    };
    registry.nodes
}

/// Return the cache directory path (~/.bubbaloop/cache).
fn cache_dir() -> PathBuf {
    dirs::home_dir()
        .unwrap_or_else(|| PathBuf::from("."))
        .join(".bubbaloop")
        .join("cache")
}

/// Load registry nodes from the local cache file.
/// Returns an empty list if the cache doesn't exist or can't be parsed.
pub fn load_cached_registry() -> Vec<RegistryNode> {
    let cache_path = cache_dir().join(OFFICIAL_NODES_CACHE);
    match fs::read_to_string(&cache_path) {
        Ok(yaml) => parse_nodes_yaml(&yaml),
        Err(_) => Vec::new(),
    }
}

/// Refresh the cache by fetching from GitHub (blocking, uses curl).
/// Returns Ok(()) on success, Err with message on failure.
pub fn refresh_cache() -> Result<(), String> {
    let dir = cache_dir();
    fs::create_dir_all(&dir).map_err(|e| format!("Cannot create cache dir: {}", e))?;

    let cache_path = dir.join(OFFICIAL_NODES_CACHE);

    // Use absolute path for curl to prevent PATH hijacking
    let curl = find_curl().ok_or("curl not found in standard paths")?;

    let output = std::process::Command::new(curl)
        .args([
            "-sSfL",
            "--connect-timeout",
            "5",
            "--max-time",
            "10",
            OFFICIAL_NODES_URL,
        ])
        .output()
        .map_err(|e| format!("Failed to run curl: {}", e))?;

    if output.status.success() {
        fs::write(&cache_path, &output.stdout)
            .map_err(|e| format!("Failed to write cache: {}", e))?;
        Ok(())
    } else {
        let stderr = String::from_utf8_lossy(&output.stderr);
        Err(format!("curl failed: {}", stderr))
    }
}

/// Find curl in standard system paths to avoid PATH hijacking.
fn find_curl() -> Option<PathBuf> {
    for dir in &["/usr/bin", "/usr/local/bin", "/bin"] {
        let path = Path::new(dir).join("curl");
        if path.exists() {
            return Some(path);
        }
    }
    None
}

/// Refresh cache in a background thread (non-blocking, for TUI).
pub fn refresh_cache_async() {
    std::thread::spawn(|| {
        let _ = refresh_cache();
    });
}

/// Search registry nodes by query string, category, and/or tag.
///
/// - `query`: matches against name, description, or tags (case-insensitive substring)
/// - `category`: exact match on category (case-insensitive)
/// - `tag`: exact match on any tag (case-insensitive)
///
/// An empty query with no category/tag returns all nodes.
pub fn search_registry(
    nodes: &[RegistryNode],
    query: &str,
    category: Option<&str>,
    tag: Option<&str>,
) -> Vec<RegistryNode> {
    let query_lower = query.to_lowercase();

    nodes
        .iter()
        .filter(|node| {
            // Text search: match name, description, or tags
            let matches_query = query.is_empty()
                || node.name.to_lowercase().contains(&query_lower)
                || node.description.to_lowercase().contains(&query_lower)
                || node
                    .tags
                    .iter()
                    .any(|t| t.to_lowercase().contains(&query_lower));

            // Category filter
            let matches_category =
                category.is_none_or(|cat| node.category.to_lowercase() == cat.to_lowercase());

            // Tag filter
            let matches_tag = tag.is_none_or(|t| {
                node.tags
                    .iter()
                    .any(|nt| nt.to_lowercase() == t.to_lowercase())
            });

            matches_query && matches_category && matches_tag
        })
        .cloned()
        .collect()
}

/// Look up a single node by exact name.
pub fn find_by_name(nodes: &[RegistryNode], name: &str) -> Option<RegistryNode> {
    nodes.iter().find(|n| n.name == name).cloned()
}

/// Validate that a repo string looks like a valid GitHub "owner/repo" shorthand.
/// Rejects path traversal, shell metacharacters, and malformed values.
pub fn validate_repo(repo: &str) -> Result<(), String> {
    if repo.is_empty() {
        return Err("repo is empty".into());
    }
    if repo.contains("..") {
        return Err(format!("repo contains '..': {}", repo));
    }
    if repo.starts_with('/') || repo.starts_with('-') || repo.starts_with('.') {
        return Err(format!("repo starts with invalid character: {}", repo));
    }
    if repo.matches('/').count() != 1 {
        return Err(format!("repo must be 'owner/repo' format: {}", repo));
    }
    let valid = repo
        .chars()
        .all(|c| c.is_alphanumeric() || c == '-' || c == '_' || c == '/' || c == '.');
    if !valid {
        return Err(format!("repo contains invalid characters: {}", repo));
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    const TEST_NODES_YAML: &str = r#"
nodes:
  - name: rtsp-camera
    description: "RTSP camera capture with hardware H264 decode"
    version: "0.1.0"
    type: rust
    category: camera
    tags: [video, gstreamer, h264]
    repo: kornia/bubbaloop-nodes-official
    subdir: rtsp-camera
    binary: cameras_node

  - name: openmeteo
    description: "Weather data from Open-Meteo API"
    version: "0.1.0"
    type: rust
    category: weather
    tags: [weather, forecast]
    repo: kornia/bubbaloop-nodes-official
    subdir: openmeteo
    binary: openmeteo_node

  - name: inference
    description: "ML inference node for model serving"
    version: "0.1.0"
    type: rust
    category: inference
    tags: [ml, inference]
    repo: kornia/bubbaloop-nodes-official
    subdir: inference
    binary: inference_node

  - name: system-telemetry
    description: "System metrics collection"
    version: "0.1.0"
    type: rust
    category: monitoring
    tags: [telemetry, metrics]
    repo: kornia/bubbaloop-nodes-official
    subdir: system-telemetry
    binary: system_telemetry_node

  - name: network-monitor
    description: "Network connectivity monitor"
    version: "0.1.0"
    type: python
    category: monitoring
    tags: [network, monitoring]
    repo: kornia/bubbaloop-nodes-official
    subdir: network-monitor
"#;

    #[test]
    fn test_parse_nodes_yaml() {
        let nodes = parse_nodes_yaml(TEST_NODES_YAML);
        assert_eq!(nodes.len(), 5);

        let names: Vec<&str> = nodes.iter().map(|n| n.name.as_str()).collect();
        assert!(names.contains(&"rtsp-camera"));
        assert!(names.contains(&"openmeteo"));
        assert!(names.contains(&"inference"));
        assert!(names.contains(&"system-telemetry"));
        assert!(names.contains(&"network-monitor"));
    }

    #[test]
    fn test_parse_nodes_yaml_fields() {
        let nodes = parse_nodes_yaml(TEST_NODES_YAML);
        let camera = nodes.iter().find(|n| n.name == "rtsp-camera").unwrap();

        assert_eq!(camera.version, "0.1.0");
        assert_eq!(camera.node_type, "rust");
        assert_eq!(
            camera.description,
            "RTSP camera capture with hardware H264 decode"
        );
        assert_eq!(camera.category, "camera");
        assert_eq!(camera.tags, vec!["video", "gstreamer", "h264"]);
        assert_eq!(camera.repo, "kornia/bubbaloop-nodes-official");
        assert_eq!(camera.subdir, "rtsp-camera");

        let netmon = nodes.iter().find(|n| n.name == "network-monitor").unwrap();
        assert_eq!(netmon.node_type, "python");
        assert_eq!(netmon.category, "monitoring");
    }

    #[test]
    fn test_parse_nodes_yaml_invalid() {
        let nodes = parse_nodes_yaml("not valid yaml: [[[");
        assert!(nodes.is_empty());
    }

    #[test]
    fn test_search_by_name() {
        let nodes = parse_nodes_yaml(TEST_NODES_YAML);
        let results = search_registry(&nodes, "camera", None, None);
        assert_eq!(results.len(), 1);
        assert_eq!(results[0].name, "rtsp-camera");
    }

    #[test]
    fn test_search_by_description() {
        let nodes = parse_nodes_yaml(TEST_NODES_YAML);
        let results = search_registry(&nodes, "weather", None, None);
        assert_eq!(results.len(), 1);
        assert_eq!(results[0].name, "openmeteo");
    }

    #[test]
    fn test_search_by_category() {
        let nodes = parse_nodes_yaml(TEST_NODES_YAML);
        let results = search_registry(&nodes, "", Some("monitoring"), None);
        assert_eq!(results.len(), 2);
        let names: Vec<&str> = results.iter().map(|n| n.name.as_str()).collect();
        assert!(names.contains(&"system-telemetry"));
        assert!(names.contains(&"network-monitor"));
    }

    #[test]
    fn test_search_by_tag() {
        let nodes = parse_nodes_yaml(TEST_NODES_YAML);
        let results = search_registry(&nodes, "", None, Some("gstreamer"));
        assert_eq!(results.len(), 1);
        assert_eq!(results[0].name, "rtsp-camera");
    }

    #[test]
    fn test_search_empty_returns_all() {
        let nodes = parse_nodes_yaml(TEST_NODES_YAML);
        let results = search_registry(&nodes, "", None, None);
        assert_eq!(results.len(), 5);
    }

    #[test]
    fn test_search_case_insensitive() {
        let nodes = parse_nodes_yaml(TEST_NODES_YAML);
        let results = search_registry(&nodes, "CAMERA", None, None);
        assert_eq!(results.len(), 1);
        assert_eq!(results[0].name, "rtsp-camera");
    }

    #[test]
    fn test_search_combined_query_and_category() {
        let nodes = parse_nodes_yaml(TEST_NODES_YAML);
        // "network" query + "monitoring" category should match network-monitor
        let results = search_registry(&nodes, "network", Some("monitoring"), None);
        assert_eq!(results.len(), 1);
        assert_eq!(results[0].name, "network-monitor");
    }

    #[test]
    fn test_search_no_match() {
        let nodes = parse_nodes_yaml(TEST_NODES_YAML);
        let results = search_registry(&nodes, "nonexistent", None, None);
        assert!(results.is_empty());
    }

    #[test]
    fn test_find_by_name() {
        let nodes = parse_nodes_yaml(TEST_NODES_YAML);
        let found = find_by_name(&nodes, "openmeteo");
        assert!(found.is_some());
        assert_eq!(found.unwrap().category, "weather");

        let not_found = find_by_name(&nodes, "nonexistent");
        assert!(not_found.is_none());
    }

    #[test]
    fn test_validate_repo_valid() {
        assert!(validate_repo("kornia/bubbaloop-nodes-official").is_ok());
        assert!(validate_repo("user/repo").is_ok());
        assert!(validate_repo("my-org/my_repo.rs").is_ok());
    }

    #[test]
    fn test_validate_repo_rejects_path_traversal() {
        assert!(validate_repo("../../malicious").is_err());
        assert!(validate_repo("user/../etc").is_err());
    }

    #[test]
    fn test_validate_repo_rejects_invalid_format() {
        assert!(validate_repo("").is_err());
        assert!(validate_repo("just-a-name").is_err());
        assert!(validate_repo("a/b/c").is_err());
        assert!(validate_repo("/leading-slash").is_err());
        assert!(validate_repo("-leading-dash/repo").is_err());
        assert!(validate_repo(".hidden/repo").is_err());
    }

    #[test]
    fn test_validate_repo_rejects_shell_metacharacters() {
        assert!(validate_repo("user/repo;evil").is_err());
        assert!(validate_repo("user/repo&cmd").is_err());
        assert!(validate_repo("user/repo$(cmd)").is_err());
        assert!(validate_repo("user/repo`cmd`").is_err());
    }
}
