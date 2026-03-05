//! Node add, remove, and instance management CLI commands.

use std::path::{Path, PathBuf};

use super::install;
use super::list::discover_nodes_in_subdirs;
use super::{send_command, NodeError, Result};

#[allow(clippy::too_many_arguments)]
pub(crate) async fn add_node(
    source: &str,
    output: Option<&str>,
    branch: &str,
    subdir: Option<&str>,
    name: Option<&str>,
    config: Option<&str>,
    build: bool,
    do_install: bool,
) -> Result<()> {
    // Normalize source URL
    let normalized = install::normalize_git_url(source);

    let base_path = if install::is_git_url(&normalized) {
        // Clone from GitHub
        install::clone_from_github(&normalized, output, branch)?
    } else {
        // Local path
        let path = Path::new(source);
        if !path.exists() {
            return Err(NodeError::NotFound(source.to_string()));
        }
        path.canonicalize()?.to_string_lossy().to_string()
    };

    // Resolve the actual node path (handles --subdir and multi-node discovery)
    let node_path = resolve_node_path(&base_path, subdir)?;

    // Add to daemon via Zenoh gateway
    let client = crate::cli::daemon_client::DaemonClient::connect().await?;
    let _resp = client.add_node(&node_path, name, config).await?;
    println!("Added node from: {}", node_path);

    let node_name = install::extract_node_name(&node_path).ok();

    // Optional: build
    if build {
        if let Some(ref name) = node_name {
            println!("Building node...");
            send_command(name, "build").await?;
        }
    }

    // Optional: install
    if do_install {
        if let Some(ref name) = node_name {
            println!("Installing as service...");
            send_command(name, "install").await?;
        }
    }

    Ok(())
}

pub(crate) async fn remove_node(name: &str, delete_files: bool) -> Result<()> {
    let client = crate::cli::daemon_client::DaemonClient::connect().await?;
    client.remove_node(name).await?;
    println!("Removed node: {}", name);

    if delete_files {
        eprintln!("Note: File deletion not implemented yet. Remove files manually.");
    }

    Ok(())
}

/// Create an instance of a multi-instance node (like rtsp-camera)
///
/// This command creates a named instance from an already-registered base node.
/// The instance will use the base node's binary but with its own config file.
///
/// # Arguments
/// * `base_node` - Name of the registered base node (e.g., "rtsp-camera")
/// * `suffix` - Instance suffix (e.g., "terrace" creates "rtsp-camera-terrace")
/// * `config` - Path to config file for this instance
/// * `copy_config` - Copy example config from base node's configs/ directory
/// * `do_install` - Install as systemd service after creating
/// * `start` - Start the instance after creating (implies --install)
///
/// # Example
/// ```bash
/// bubbaloop node instance rtsp-camera terrace --config ~/.bubbaloop/configs/rtsp-camera-terrace.yaml
/// ```
pub(crate) async fn create_instance(
    base_node: &str,
    suffix: &str,
    config: Option<&str>,
    copy_config: bool,
    do_install: bool,
    start: bool,
) -> Result<()> {
    // Validate suffix (same rules as node names: alphanumeric, hyphens, underscores)
    if suffix.is_empty() || suffix.len() > 64 {
        return Err(NodeError::InvalidArgs(
            "Instance suffix must be 1-64 characters".into(),
        ));
    }
    if !suffix
        .chars()
        .all(|c| c.is_alphanumeric() || c == '-' || c == '_')
    {
        return Err(NodeError::InvalidArgs(
            "Instance suffix can only contain alphanumeric characters, hyphens, and underscores"
                .into(),
        ));
    }
    if suffix.starts_with('-') || suffix.starts_with('_') {
        return Err(NodeError::InvalidArgs(
            "Instance suffix cannot start with hyphen or underscore".into(),
        ));
    }

    // Build the full instance name: base-suffix
    let instance_name = format!("{}-{}", base_node, suffix);

    // Query daemon for base node via Zenoh gateway
    let client = crate::cli::daemon_client::DaemonClient::connect().await?;
    let nodes_json = client.list_nodes().await?;
    let nodes: Vec<crate::mcp::platform::NodeInfo> = serde_json::from_str(&nodes_json)
        .map_err(|e| NodeError::CommandFailed(format!("Invalid daemon response: {}", e)))?;

    // Find the base node - check it exists
    let base_exists = nodes.iter().any(|n| n.name == base_node);
    if !base_exists {
        return Err(NodeError::NotFound(format!(
            "Base node '{}' not found. Add it first with: bubbaloop node add <path>",
            base_node
        )));
    }

    // Handle config file
    let config_path = if copy_config {
        // For copy_config we need the base node's path which the API doesn't expose.
        // Fall back to searching in ~/.bubbaloop/nodes/
        let home =
            dirs::home_dir().ok_or_else(|| NodeError::Io(std::io::Error::other("HOME not set")))?;
        let nodes_dir = home.join(".bubbaloop").join("nodes");

        // Try to find the base node directory
        let mut found_config: Option<String> = None;
        if let Ok(entries) = std::fs::read_dir(&nodes_dir) {
            for entry in entries.flatten() {
                let node_yaml = entry.path().join("node.yaml");
                if node_yaml.exists() {
                    if let Ok(content) = std::fs::read_to_string(&node_yaml) {
                        if let Ok(manifest) = serde_yaml::from_str::<serde_yaml::Value>(&content) {
                            if manifest.get("name").and_then(|v| v.as_str()) == Some(base_node) {
                                let configs_dir = entry.path().join("configs");
                                if configs_dir.exists() {
                                    let example_config = find_example_config(&configs_dir)?;
                                    let dest_dir = home.join(".bubbaloop").join("configs");
                                    std::fs::create_dir_all(&dest_dir)?;
                                    let dest_path =
                                        dest_dir.join(format!("{}.yaml", instance_name));
                                    std::fs::copy(&example_config, &dest_path)?;
                                    println!("Copied example config to: {}", dest_path.display());
                                    println!(
                                        "Edit this file to configure your instance before starting."
                                    );
                                    found_config = Some(dest_path.to_string_lossy().to_string());
                                }
                                break;
                            }
                        }
                    }
                }
            }
        }

        if found_config.is_none() {
            return Err(NodeError::NotFound(format!(
                "Could not find configs/ directory for base node '{}'",
                base_node
            )));
        }
        found_config
    } else if let Some(config) = config {
        // Validate config path exists
        let config_path = Path::new(config);
        if !config_path.exists() {
            return Err(NodeError::NotFound(format!(
                "Config file not found: {}",
                config
            )));
        }
        Some(config_path.canonicalize()?.to_string_lossy().to_string())
    } else {
        None
    };

    // Register the instance with the daemon via Zenoh gateway
    client
        .add_node(base_node, Some(&instance_name), config_path.as_deref())
        .await?;
    println!(
        "Created instance '{}' from base node '{}'",
        instance_name, base_node
    );

    // Optional: install and/or start
    if start || do_install {
        println!("Installing instance as systemd service...");
        send_command(&instance_name, "install").await?;
    }

    if start {
        println!("Starting instance...");
        send_command(&instance_name, "start").await?;
    }

    Ok(())
}

/// Find an example config file in the configs/ directory
pub(crate) fn find_example_config(configs_dir: &Path) -> Result<PathBuf> {
    let entries: Vec<_> = std::fs::read_dir(configs_dir)?
        .filter_map(|e| e.ok())
        .filter(|e| {
            e.path()
                .extension()
                .map(|ext| ext == "yaml" || ext == "yml")
                .unwrap_or(false)
        })
        .collect();

    if entries.is_empty() {
        return Err(NodeError::NotFound(format!(
            "No .yaml config files found in {}",
            configs_dir.display()
        )));
    }

    // Return the first one found
    Ok(entries[0].path())
}

/// Resolve the node path, applying --subdir if set, or discovering nodes if needed.
pub(crate) fn resolve_node_path(base_path: &str, subdir: Option<&str>) -> Result<String> {
    let base = Path::new(base_path);

    if let Some(sub) = subdir {
        // Validate subdir to prevent path traversal
        if sub.is_empty()
            || sub.contains("..")
            || sub.contains('/')
            || sub.contains('\\')
            || sub.starts_with('.')
        {
            return Err(NodeError::InvalidArgs(
                "subdir must be a simple directory name (no paths, no '..')".into(),
            ));
        }
        let node_path = base.join(sub);
        let manifest = node_path.join("node.yaml");
        if !manifest.exists() {
            return Err(NodeError::NotFound(format!(
                "No node.yaml found at {}/{}",
                base_path, sub
            )));
        }
        return Ok(node_path.to_string_lossy().to_string());
    }

    // Check for node.yaml at root
    let manifest = base.join("node.yaml");
    if manifest.exists() {
        return Ok(base_path.to_string());
    }

    // No node.yaml at root -- discover subdirectories
    let discovered = discover_nodes_in_subdirs(base);
    if discovered.is_empty() {
        return Err(NodeError::NotFound(format!(
            "No node.yaml found at {} or in any subdirectory",
            base_path
        )));
    }

    let mut msg = format!(
        "No node.yaml found at repository root.\n\nFound {} node(s) in subdirectories:\n",
        discovered.len()
    );
    for (name, subdir, node_type) in &discovered {
        msg.push_str(&format!(
            "  {:<20} (type: {:<6}) -- use: bubbaloop node add <source> --subdir {}\n",
            name, node_type, subdir
        ));
    }
    msg.push_str("\nHint: Use --subdir <name> to add a specific node.");

    Err(NodeError::NotFound(msg))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_resolve_node_path_single_node_at_root() {
        let dir = tempfile::tempdir().unwrap();
        std::fs::write(
            dir.path().join("node.yaml"),
            "name: test-node\nversion: \"0.1.0\"\ntype: rust",
        )
        .unwrap();

        let result = resolve_node_path(dir.path().to_str().unwrap(), None);
        assert!(result.is_ok());
        assert_eq!(result.unwrap(), dir.path().to_str().unwrap());
    }

    #[test]
    fn test_resolve_node_path_with_subdir() {
        let dir = tempfile::tempdir().unwrap();
        let subdir = dir.path().join("my-node");
        std::fs::create_dir(&subdir).unwrap();
        std::fs::write(
            subdir.join("node.yaml"),
            "name: my-node\nversion: \"0.1.0\"\ntype: rust",
        )
        .unwrap();

        let result = resolve_node_path(dir.path().to_str().unwrap(), Some("my-node"));
        assert!(result.is_ok());
        assert!(result.unwrap().ends_with("my-node"));
    }

    #[test]
    fn test_resolve_node_path_subdir_missing_manifest() {
        let dir = tempfile::tempdir().unwrap();
        let subdir = dir.path().join("empty-dir");
        std::fs::create_dir(&subdir).unwrap();

        let result = resolve_node_path(dir.path().to_str().unwrap(), Some("empty-dir"));
        assert!(result.is_err());
        let err = result.unwrap_err().to_string();
        assert!(err.contains("No node.yaml found"));
    }

    #[test]
    fn test_resolve_node_path_multi_node_discovery() {
        let dir = tempfile::tempdir().unwrap();

        // Create two node subdirectories
        for name in &["camera", "weather"] {
            let subdir = dir.path().join(name);
            std::fs::create_dir(&subdir).unwrap();
            std::fs::write(
                subdir.join("node.yaml"),
                format!("name: {}\nversion: \"0.1.0\"\ntype: rust", name),
            )
            .unwrap();
        }

        // No node.yaml at root, no --subdir -> should discover and error
        let result = resolve_node_path(dir.path().to_str().unwrap(), None);
        assert!(result.is_err());
        let err = result.unwrap_err().to_string();
        assert!(err.contains("Found 2 node(s)"));
        assert!(err.contains("camera"));
        assert!(err.contains("weather"));
        assert!(err.contains("--subdir"));
    }

    #[test]
    fn test_resolve_node_path_no_nodes_found() {
        let dir = tempfile::tempdir().unwrap();
        // Empty directory, no node.yaml anywhere
        let result = resolve_node_path(dir.path().to_str().unwrap(), None);
        assert!(result.is_err());
        let err = result.unwrap_err().to_string();
        assert!(err.contains("No node.yaml found"));
    }

    #[test]
    fn test_resolve_node_path_rejects_path_traversal() {
        let dir = tempfile::tempdir().unwrap();
        // ".." traversal
        let result = resolve_node_path(dir.path().to_str().unwrap(), Some("../etc"));
        assert!(result.is_err());
        assert!(result
            .unwrap_err()
            .to_string()
            .contains("simple directory name"));

        // Slash in subdir
        let result = resolve_node_path(dir.path().to_str().unwrap(), Some("foo/bar"));
        assert!(result.is_err());

        // Hidden directory
        let result = resolve_node_path(dir.path().to_str().unwrap(), Some(".hidden"));
        assert!(result.is_err());

        // Empty string
        let result = resolve_node_path(dir.path().to_str().unwrap(), Some(""));
        assert!(result.is_err());
    }

    #[test]
    fn test_find_example_config_finds_yaml() {
        let dir = tempfile::tempdir().unwrap();
        let configs_dir = dir.path().join("configs");
        std::fs::create_dir(&configs_dir).unwrap();
        std::fs::write(configs_dir.join("example.yaml"), "name: test").unwrap();

        let result = find_example_config(&configs_dir);
        assert!(result.is_ok());
        assert!(result.unwrap().to_string_lossy().contains("example.yaml"));
    }

    #[test]
    fn test_find_example_config_finds_yml() {
        let dir = tempfile::tempdir().unwrap();
        let configs_dir = dir.path().join("configs");
        std::fs::create_dir(&configs_dir).unwrap();
        std::fs::write(configs_dir.join("example.yml"), "name: test").unwrap();

        let result = find_example_config(&configs_dir);
        assert!(result.is_ok());
    }

    #[test]
    fn test_find_example_config_empty_dir() {
        let dir = tempfile::tempdir().unwrap();
        let configs_dir = dir.path().join("configs");
        std::fs::create_dir(&configs_dir).unwrap();

        let result = find_example_config(&configs_dir);
        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains("No .yaml config"));
    }

    #[test]
    fn test_find_example_config_ignores_non_yaml() {
        let dir = tempfile::tempdir().unwrap();
        let configs_dir = dir.path().join("configs");
        std::fs::create_dir(&configs_dir).unwrap();
        std::fs::write(configs_dir.join("readme.txt"), "not a config").unwrap();
        std::fs::write(configs_dir.join("config.json"), "{}").unwrap();

        let result = find_example_config(&configs_dir);
        assert!(result.is_err()); // Only .yaml/.yml files
    }

    /// Test that instance name is constructed correctly: base-suffix
    #[test]
    fn test_instance_name_construction() {
        let base = "rtsp-camera";
        let suffix = "terrace";
        let instance_name = format!("{}-{}", base, suffix);
        assert_eq!(instance_name, "rtsp-camera-terrace");

        let base2 = "weather-node";
        let suffix2 = "station_1";
        let instance_name2 = format!("{}-{}", base2, suffix2);
        assert_eq!(instance_name2, "weather-node-station_1");
    }

    #[test]
    fn test_instance_suffix_validation_valid() {
        // Valid suffixes that should work
        let valid_suffixes = ["terrace", "entrance", "cam01", "garden_1", "my-camera"];
        for suffix in valid_suffixes {
            assert!(
                suffix
                    .chars()
                    .all(|c| c.is_alphanumeric() || c == '-' || c == '_'),
                "Expected '{}' to be valid",
                suffix
            );
            assert!(
                !suffix.starts_with('-') && !suffix.starts_with('_'),
                "Expected '{}' to not start with - or _",
                suffix
            );
        }
    }

    #[test]
    fn test_instance_suffix_validation_invalid() {
        // Invalid suffixes that should be rejected
        let invalid_suffixes = [
            "",              // empty
            "-terrace",      // starts with dash
            "_terrace",      // starts with underscore
            "terrace space", // contains space
            "terrace/path",  // contains slash
            "../traversal",  // path traversal
            "terrace;cmd",   // shell metacharacter
        ];
        for suffix in invalid_suffixes {
            let is_valid = !suffix.is_empty()
                && suffix.len() <= 64
                && suffix
                    .chars()
                    .all(|c| c.is_alphanumeric() || c == '-' || c == '_')
                && !suffix.starts_with('-')
                && !suffix.starts_with('_');
            assert!(!is_valid, "Expected '{}' to be invalid", suffix);
        }
    }
}
