//! Launch a single node instance from a launch YAML file
//!
//! Usage:
//!   bubbaloop launch rtsp-camera entrance.yaml
//!   bubbaloop launch rtsp-camera entrance.yaml --build --start
//!   bubbaloop launch rtsp-camera entrance.yaml --dry-run
//!
//! launch file format:
//!
//! ```yaml
//! name: rtsp-camera-entrance
//! config:
//!   name: entrance
//!   publish_topic: camera/entrance/compressed
//!   url: "rtsp://user:pass@192.168.1.141:554/stream2"
//! ```

use std::path::PathBuf;

use argh::FromArgs;
use serde::Deserialize;
use thiserror::Error;

use crate::cli::node;

#[derive(Debug, Error)]
pub enum LaunchError {
    #[error("IO error: {0}")]
    Io(#[from] std::io::Error),
    #[error("YAML parse error: {0}")]
    Yaml(#[from] serde_yaml::Error),
    #[error("JSON error: {0}")]
    Json(#[from] serde_json::Error),
    #[error("Launch file not found: {0}")]
    FileNotFound(String),
    #[error("Instance error: {0}")]
    Instance(String),
    #[error("Node error: {0}")]
    Node(#[from] node::NodeError),
}

pub type Result<T> = std::result::Result<T, LaunchError>;

/// Launch a node instance from a YAML file
#[derive(FromArgs, PartialEq, Debug)]
#[argh(subcommand, name = "launch")]
pub struct LaunchCommand {
    /// registered node name (e.g. rtsp-camera)
    #[argh(positional)]
    pub node: String,

    /// path to launch YAML file
    #[argh(positional)]
    pub file: String,

    /// build the instance before starting
    #[argh(switch)]
    pub build: bool,

    /// install as systemd service
    #[argh(switch)]
    pub install: bool,

    /// start the instance after registering
    #[argh(switch)]
    pub start: bool,

    /// show what would be done without executing
    #[argh(switch)]
    pub dry_run: bool,
}

/// Single instance definition
#[derive(Debug, Deserialize)]
pub struct LaunchFile {
    /// Instance name (e.g. rtsp-camera-entrance)
    pub name: String,
    /// Inline config for this instance
    pub config: Option<serde_yaml::Value>,
}

/// Parse a launch YAML file
fn parse_launch_file(content: &str) -> Result<LaunchFile> {
    let launch: LaunchFile = serde_yaml::from_str(content)?;
    if launch.name.is_empty() {
        return Err(LaunchError::Instance(
            "launch file must have a non-empty 'name' field".to_string(),
        ));
    }
    Ok(launch)
}

/// Write an inline config block to ~/.bubbaloop/configs/{name}.yaml
fn write_config(
    configs_dir: &std::path::Path,
    name: &str,
    config: &serde_yaml::Value,
) -> Result<PathBuf> {
    std::fs::create_dir_all(configs_dir)?;
    let dest = configs_dir.join(format!("{}.yaml", name));
    let yaml = serde_yaml::to_string(config)?;
    std::fs::write(&dest, &yaml)?;
    Ok(dest)
}

fn default_configs_dir() -> PathBuf {
    dirs::home_dir()
        .unwrap_or_else(|| PathBuf::from("."))
        .join(".bubbaloop")
        .join("configs")
}

/// Return the directory path as a `String` if `dir/node.yaml` exists and its
/// `name` field matches `node_name`, otherwise `None`.
fn node_name_matches(dir: &std::path::Path, node_name: &str) -> Option<String> {
    let node_yaml = dir.join("node.yaml");
    if !node_yaml.exists() {
        return None;
    }
    let content = std::fs::read_to_string(&node_yaml).ok()?;
    let manifest: serde_yaml::Value = serde_yaml::from_str(&content).ok()?;
    if manifest.get("name").and_then(|v| v.as_str()) == Some(node_name) {
        Some(dir.to_string_lossy().to_string())
    } else {
        None
    }
}

impl LaunchCommand {
    pub async fn run(self) -> Result<()> {
        // 1. Read and parse the launch file
        let file_path = std::path::Path::new(&self.file);
        if !file_path.exists() {
            return Err(LaunchError::FileNotFound(self.file.clone()));
        }

        let content = std::fs::read_to_string(file_path)?;
        let launch = parse_launch_file(&content)?;

        if self.dry_run {
            println!("[DRY RUN] Instance: {}", launch.name);
            println!("  Base node: {}", self.node);
            println!("  Launch file: {}", self.file);
            if launch.config.is_some() {
                println!(
                    "  Config: would write to ~/.bubbaloop/configs/{}.yaml",
                    launch.name
                );
            }
            if self.build {
                println!("  Would: build");
            }
            if self.install {
                println!("  Would: install as systemd service");
            }
            if self.start {
                println!("  Would: start");
            }
            if !self.build && !self.install && !self.start {
                println!("  Would: register with daemon");
            }
            return Ok(());
        }

        // 2. Connect to daemon once and resolve the base node's path
        let client = crate::cli::daemon_client::DaemonClient::connect()
            .await
            .map_err(node::NodeError::from)?;
        let node_path = self.resolve_node_path(&client, &self.node).await?;

        // 3. Write config if present
        let config_path = if let Some(ref config) = launch.config {
            let dest = write_config(&default_configs_dir(), &launch.name, config)?;
            println!("Config written to {}", dest.display());
            Some(dest.to_string_lossy().to_string())
        } else {
            None
        };

        // 4. name_override: only if instance name differs from base node
        let name_override = if launch.name != self.node {
            Some(launch.name.as_str())
        } else {
            None
        };

        // 5. Register instance via REST API (reuse the same client)
        self.register_instance(&client, &node_path, name_override, config_path.as_deref())
            .await?;
        println!("Registered: {}", launch.name);

        // 6. Optional build / install / start
        if self.build {
            println!("Building...");
            node::send_command(&launch.name, "build").await?;
        }

        if self.install {
            println!("Installing as systemd service...");
            node::send_command(&launch.name, "install").await?;
        }

        if self.start {
            println!("Starting...");
            node::send_command(&launch.name, "start").await?;
        }

        println!("\nLaunched {} successfully!", launch.name);
        Ok(())
    }

    async fn resolve_node_path(
        &self,
        client: &crate::cli::daemon_client::DaemonClient,
        node_name: &str,
    ) -> Result<String> {
        // Verify the node is registered with the daemon.
        let nodes_json = client.list_nodes().await.map_err(node::NodeError::from)?;
        let nodes: Vec<crate::mcp::platform::NodeInfo> =
            serde_json::from_str(&nodes_json).unwrap_or_default();
        if !nodes.iter().any(|n| n.name == node_name) {
            return Err(LaunchError::Node(node::NodeError::NotFound(format!(
                "Node '{}' not registered. Register it first with: bubbaloop node add <path>",
                node_name
            ))));
        }

        // Search ~/.bubbaloop/nodes/ for a matching node.yaml (top-level and one level deep).
        let home =
            dirs::home_dir().ok_or_else(|| LaunchError::Instance("HOME not set".to_string()))?;
        let nodes_dir = home.join(".bubbaloop").join("nodes");
        let entries = match std::fs::read_dir(&nodes_dir) {
            Ok(e) => e,
            Err(_) => {
                return Err(LaunchError::Node(node::NodeError::NotFound(format!(
                    "Node '{}' is registered but its path could not be resolved from ~/.bubbaloop/nodes/",
                    node_name
                ))))
            }
        };

        for entry in entries.flatten() {
            // Check top-level node.yaml
            if let Some(path) = node_name_matches(&entry.path(), node_name) {
                return Ok(path);
            }
            // Check one level of subdirectories (multi-node repos)
            if !entry.path().is_dir() {
                continue;
            }
            if let Ok(sub_entries) = std::fs::read_dir(entry.path()) {
                for sub_entry in sub_entries.flatten() {
                    if let Some(path) = node_name_matches(&sub_entry.path(), node_name) {
                        return Ok(path);
                    }
                }
            }
        }

        Err(LaunchError::Node(node::NodeError::NotFound(format!(
            "Node '{}' is registered but its path could not be resolved from ~/.bubbaloop/nodes/",
            node_name
        ))))
    }

    async fn register_instance(
        &self,
        client: &crate::cli::daemon_client::DaemonClient,
        node_path: &str,
        name_override: Option<&str>,
        config_path: Option<&str>,
    ) -> Result<()> {
        client
            .add_node(node_path, name_override, config_path)
            .await
            .map_err(node::NodeError::from)?;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_full_launch_file() {
        let yaml = r#"
name: rtsp-camera-entrance
config:
  name: entrance
  publish_topic: camera/entrance/compressed
  url: "rtsp://user:pass@192.168.1.141:554/stream2"
  latency: 200
"#;
        let launch = parse_launch_file(yaml).unwrap();
        assert_eq!(launch.name, "rtsp-camera-entrance");
        assert!(launch.config.is_some());
    }

    #[test]
    fn test_parse_minimal_launch_file() {
        let yaml = "name: my-instance\n";
        let launch = parse_launch_file(yaml).unwrap();
        assert_eq!(launch.name, "my-instance");
        assert!(launch.config.is_none());
    }

    #[test]
    fn test_parse_empty_name_rejected() {
        let yaml = "name: \"\"\n";
        assert!(parse_launch_file(yaml).is_err());
    }

    #[test]
    fn test_parse_missing_name_rejected() {
        let yaml = "config:\n  key: value\n";
        assert!(parse_launch_file(yaml).is_err());
    }

    #[test]
    fn test_parse_invalid_yaml() {
        assert!(parse_launch_file("not valid yaml: [[[").is_err());
    }

    #[test]
    fn test_write_config_to_tempdir() {
        let dir = tempfile::tempdir().unwrap();
        let config: serde_yaml::Value =
            serde_yaml::from_str("name: terrace\npublish_topic: camera/terrace/compressed\n")
                .unwrap();

        let dest = write_config(dir.path(), "rtsp-camera-terrace", &config).unwrap();
        assert!(dest.exists());
        assert!(dest.to_string_lossy().ends_with("rtsp-camera-terrace.yaml"));

        let content = std::fs::read_to_string(&dest).unwrap();
        assert!(content.contains("terrace"));
        assert!(content.contains("publish_topic"));
    }

    #[test]
    fn test_write_config_nested_values() {
        let dir = tempfile::tempdir().unwrap();
        let config: serde_yaml::Value =
            serde_yaml::from_str("location:\n  latitude: 41.39\n  longitude: 2.17\n").unwrap();

        let dest = write_config(dir.path(), "openmeteo", &config).unwrap();
        let content = std::fs::read_to_string(&dest).unwrap();
        assert!(content.contains("latitude"));
        assert!(content.contains("41.39"));
    }
}
