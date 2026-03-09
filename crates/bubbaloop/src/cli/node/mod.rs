//! Node management CLI commands
//!
//! Commands for managing bubbaloop nodes from the command line.
//! These interact with the daemon via HTTP REST API to manage systemd services.

pub mod build;
pub mod install;
pub mod lifecycle;
mod list;
mod manage;

// Re-export for use by sibling modules (e.g., install.rs uses super::resolve_node_path)
pub(crate) use manage::resolve_node_path;

use argh::FromArgs;
use std::path::PathBuf;
use thiserror::Error;

use crate::templates;

#[derive(Debug, Error)]
pub enum NodeError {
    #[error("Daemon error: {0}")]
    Daemon(#[from] crate::cli::daemon_client::DaemonClientError),
    #[error("Node not found: {0}")]
    NotFound(String),
    #[error("Command failed: {0}")]
    CommandFailed(String),
    #[error("IO error: {0}")]
    Io(#[from] std::io::Error),
    #[error("JSON error: {0}")]
    Json(#[from] serde_json::Error),
    #[error("Git clone failed: {0}")]
    GitClone(String),
    #[error("Invalid URL: {0}")]
    InvalidUrl(String),
    #[error("Invalid argument: {0}")]
    InvalidArgs(String),
}

pub type Result<T> = std::result::Result<T, NodeError>;

/// Node management commands
#[derive(FromArgs)]
#[argh(subcommand, name = "node")]
pub struct NodeCommand {
    #[argh(subcommand)]
    action: Option<NodeAction>,
}

#[derive(FromArgs)]
#[argh(subcommand)]
enum NodeAction {
    Init(InitArgs),
    Validate(ValidateArgs),
    List(ListArgs),
    Add(AddArgs),
    Remove(RemoveArgs),
    Instance(InstanceArgs),
    Install(InstallArgs),
    Uninstall(UninstallArgs),
    Start(StartArgs),
    Stop(StopArgs),
    Restart(RestartArgs),
    Logs(LogsArgs),
    Build(BuildArgs),
    Clean(CleanArgs),
    Enable(EnableArgs),
    Disable(DisableArgs),
    Search(SearchArgs),
    Discover(DiscoverArgs),
}

/// Initialize a new node from template
#[derive(FromArgs)]
#[argh(subcommand, name = "init")]
struct InitArgs {
    /// node name (e.g., "my-sensor")
    #[argh(positional)]
    name: String,

    /// node type: rust or python (default: rust)
    #[argh(option, short = 't', default = "String::from(\"rust\")")]
    node_type: String,

    /// node category: source, sink, or processor (default: source)
    #[argh(option, short = 'k', default = "String::from(\"source\")")]
    category: String,

    /// output directory (default: ./<name> in current directory)
    #[argh(option, short = 'o')]
    output: Option<String>,

    /// node description
    #[argh(option, short = 'd', default = "String::from(\"A Bubbaloop node\")")]
    description: String,

    /// author name
    #[argh(option, default = "String::from(\"Anonymous\")")]
    author: String,
}

/// Validate a node manifest and directory structure
#[derive(FromArgs)]
#[argh(subcommand, name = "validate")]
struct ValidateArgs {
    /// path to node directory (default: current directory)
    #[argh(positional, default = "String::from(\".\")")]
    path: String,
}

/// List all registered nodes
#[derive(FromArgs)]
#[argh(subcommand, name = "list")]
struct ListArgs {
    /// output format: table, json (default: table)
    #[argh(option, short = 'f', default = "String::from(\"table\")")]
    format: String,

    /// show only base nodes (excludes instances)
    #[argh(switch)]
    base: bool,

    /// show only instances (excludes base nodes)
    #[argh(switch)]
    instances: bool,
}

/// Add a node from local path or GitHub URL
#[derive(FromArgs)]
#[argh(subcommand, name = "add")]
struct AddArgs {
    /// source: local path, GitHub URL, or shorthand (user/repo)
    #[argh(positional)]
    source: String,

    /// target directory for Git clones (default: ~/.bubbaloop/nodes/<repo-name>)
    #[argh(option, short = 'o')]
    output: Option<String>,

    /// git branch to clone (default: main)
    #[argh(option, short = 'b', default = "String::from(\"main\")")]
    branch: String,

    /// build node after adding
    #[argh(switch)]
    build: bool,

    /// install as systemd service after adding
    #[argh(switch)]
    install: bool,

    /// subdirectory within repo containing node.yaml (for multi-node repos)
    #[argh(option, short = 's')]
    subdir: Option<String>,

    /// instance name (overrides node.yaml name for multi-instance nodes)
    #[argh(option, short = 'n')]
    name: Option<String>,

    /// config file path for this instance (passed to binary via -c)
    #[argh(option, short = 'c')]
    config: Option<String>,
}

/// Remove a node from the registry
#[derive(FromArgs)]
#[argh(subcommand, name = "remove")]
struct RemoveArgs {
    /// node name
    #[argh(positional)]
    name: String,

    /// also delete files
    #[argh(switch)]
    delete_files: bool,
}

/// Create an instance of a base node with specific config
///
/// Multi-instance nodes (like rtsp-camera) need different configs for each instance.
/// This command creates a named instance from an already-registered base node.
///
/// Example:
///   bubbaloop node instance rtsp-camera terrace --config ~/.bubbaloop/configs/rtsp-camera-terrace.yaml
///
/// This creates "rtsp-camera-terrace" instance that uses the rtsp-camera binary
/// but with its own config file.
#[derive(FromArgs)]
#[argh(subcommand, name = "instance")]
struct InstanceArgs {
    /// base node name (must be already registered, e.g., "rtsp-camera")
    #[argh(positional)]
    base_node: String,

    /// instance suffix (e.g., "terrace" creates "rtsp-camera-terrace")
    #[argh(positional)]
    suffix: String,

    /// config file path for this instance (required for most multi-instance nodes)
    #[argh(option, short = 'c')]
    config: Option<String>,

    /// copy example config from base node's configs/ directory
    #[argh(switch)]
    copy_config: bool,

    /// install as systemd service after creating
    #[argh(switch)]
    install: bool,

    /// start the instance after creating (implies --install)
    #[argh(switch)]
    start: bool,
}

/// Install a node as a systemd service (or from marketplace by name)
#[derive(FromArgs)]
#[argh(subcommand, name = "install")]
pub(crate) struct InstallArgs {
    /// node name (registered node or marketplace name)
    #[argh(positional)]
    pub(crate) name: String,

    /// git branch for marketplace install (default: main)
    #[argh(option, short = 'b', default = "String::from(\"main\")")]
    pub(crate) branch: String,

    /// also build the node (marketplace install only)
    #[argh(switch)]
    pub(crate) build: bool,
}

/// Uninstall a node's systemd service
#[derive(FromArgs)]
#[argh(subcommand, name = "uninstall")]
struct UninstallArgs {
    /// node name
    #[argh(positional)]
    name: String,
}

/// Start a node service
#[derive(FromArgs)]
#[argh(subcommand, name = "start")]
struct StartArgs {
    /// node name
    #[argh(positional)]
    name: String,
}

/// Stop a node service
#[derive(FromArgs)]
#[argh(subcommand, name = "stop")]
struct StopArgs {
    /// node name
    #[argh(positional)]
    name: String,
}

/// Restart a node service
#[derive(FromArgs)]
#[argh(subcommand, name = "restart")]
struct RestartArgs {
    /// node name
    #[argh(positional)]
    name: String,
}

/// View logs for a node
#[derive(FromArgs)]
#[argh(subcommand, name = "logs")]
pub(crate) struct LogsArgs {
    /// node name
    #[argh(positional)]
    pub(crate) name: String,

    /// number of lines to show (default: 50)
    #[allow(dead_code)]
    #[argh(option, short = 'n', default = "50")]
    pub(crate) lines: usize,

    /// follow log output
    #[argh(switch, short = 'f')]
    pub(crate) follow: bool,
}

/// Build a node
#[derive(FromArgs)]
#[argh(subcommand, name = "build")]
struct BuildArgs {
    /// node name
    #[argh(positional)]
    name: String,
}

/// Clean a node's build artifacts
#[derive(FromArgs)]
#[argh(subcommand, name = "clean")]
struct CleanArgs {
    /// node name
    #[argh(positional)]
    name: String,
}

/// Enable autostart for a node
#[derive(FromArgs)]
#[argh(subcommand, name = "enable")]
struct EnableArgs {
    /// node name
    #[argh(positional)]
    name: String,
}

/// Disable autostart for a node
#[derive(FromArgs)]
#[argh(subcommand, name = "disable")]
struct DisableArgs {
    /// node name
    #[argh(positional)]
    name: String,
}

/// Search the node marketplace
#[derive(FromArgs)]
#[argh(subcommand, name = "search")]
struct SearchArgs {
    /// search query (matches name, description, tags)
    #[argh(positional, default = "String::new()")]
    query: String,

    /// filter by category
    #[argh(option, short = 'c')]
    category: Option<String>,

    /// filter by tag
    #[argh(option, short = 't')]
    tag: Option<String>,
}

/// Discover available nodes from marketplace sources (with status from daemon)
#[derive(FromArgs)]
#[argh(subcommand, name = "discover")]
struct DiscoverArgs {
    /// output format: table, json (default: table)
    #[argh(option, short = 'f', default = "String::from(\"table\")")]
    format: String,
}

/// Legacy response types kept for tests (no longer used at runtime).
#[cfg(test)]
#[derive(serde::Deserialize)]
pub(crate) struct CommandResponse {
    pub(crate) success: bool,
    pub(crate) message: String,
    #[serde(default)]
    pub(crate) output: String,
}

#[cfg(test)]
#[derive(serde::Deserialize, serde::Serialize)]
pub(crate) struct NodeState {
    pub(crate) name: String,
    pub(crate) path: String,
    pub(crate) status: String,
    pub(crate) installed: bool,
    pub(crate) autostart_enabled: bool,
    pub(crate) version: String,
    pub(crate) description: String,
    pub(crate) node_type: String,
    pub(crate) is_built: bool,
    #[serde(default)]
    pub(crate) base_node: String,
}

#[cfg(test)]
#[derive(serde::Deserialize)]
pub(crate) struct NodeListResponse {
    pub(crate) nodes: Vec<NodeState>,
}

#[cfg(test)]
#[derive(serde::Deserialize)]
pub(crate) struct LogsResponse {
    pub(crate) lines: Vec<String>,
    pub(crate) success: bool,
    #[serde(default)]
    pub(crate) error: Option<String>,
}

impl NodeCommand {
    pub async fn run(self) -> Result<()> {
        match self.action {
            None => {
                Self::print_help();
                Ok(())
            }
            Some(NodeAction::Init(args)) => init_node(args),
            Some(NodeAction::Validate(args)) => validate_node(args),
            Some(NodeAction::List(args)) => {
                if args.base && args.instances {
                    return Err(NodeError::InvalidArgs(
                        "Cannot use --base and --instances together".into(),
                    ));
                }
                list::list_nodes(&args.format, args.base, args.instances).await
            }
            Some(NodeAction::Add(args)) => {
                manage::add_node(
                    &args.source,
                    args.output.as_deref(),
                    &args.branch,
                    args.subdir.as_deref(),
                    args.name.as_deref(),
                    args.config.as_deref(),
                    args.build,
                    args.install,
                )
                .await
            }
            Some(NodeAction::Remove(args)) => {
                manage::remove_node(&args.name, args.delete_files).await
            }
            Some(NodeAction::Instance(args)) => {
                manage::create_instance(
                    &args.base_node,
                    &args.suffix,
                    args.config.as_deref(),
                    args.copy_config,
                    args.install,
                    args.start,
                )
                .await
            }
            Some(NodeAction::Install(args)) => install::handle_install(args).await,
            Some(NodeAction::Uninstall(args)) => send_command(&args.name, "uninstall").await,
            Some(NodeAction::Start(args)) => lifecycle::start_node(&args.name).await,
            Some(NodeAction::Stop(args)) => lifecycle::stop_node(&args.name).await,
            Some(NodeAction::Restart(args)) => lifecycle::restart_node(&args.name).await,
            Some(NodeAction::Logs(args)) => lifecycle::view_logs(args).await,
            Some(NodeAction::Build(args)) => build::build_node(&args.name).await,
            Some(NodeAction::Clean(args)) => send_command(&args.name, "clean").await,
            Some(NodeAction::Enable(args)) => send_command(&args.name, "enable_autostart").await,
            Some(NodeAction::Disable(args)) => send_command(&args.name, "disable_autostart").await,
            Some(NodeAction::Search(args)) => {
                list::search_nodes(&args.query, args.category.as_deref(), args.tag.as_deref())
            }
            Some(NodeAction::Discover(args)) => list::discover_nodes(&args.format).await,
        }
    }

    fn print_help() {
        eprintln!("Node management commands\n");
        eprintln!("Usage: bubbaloop node <command>\n");
        eprintln!("Commands:");
        eprintln!("  init        Initialize a new node from template");
        eprintln!("  validate    Validate a node manifest and directory structure");
        eprintln!("  list        List all registered nodes");
        eprintln!("  add         Add a node from local path or GitHub URL");
        eprintln!("  remove      Remove a node from the registry");
        eprintln!("  instance    Create an instance of a multi-instance node");
        eprintln!(
            "              Example: bubbaloop node instance rtsp-camera terrace -c config.yaml"
        );
        eprintln!("  search      Search the node marketplace");
        eprintln!("  discover    Discover available nodes with status");
        eprintln!("  install     Install a node (or from marketplace by name)");
        eprintln!("  uninstall   Uninstall a node's systemd service");
        eprintln!("  start       Start a node service");
        eprintln!("  stop        Stop a node service");
        eprintln!("  restart     Restart a node service");
        eprintln!("  logs        View logs for a node");
        eprintln!("  build       Build a node");
        eprintln!("  clean       Clean a node's build artifacts");
        eprintln!("  enable      Enable autostart for a node");
        eprintln!("  disable     Disable autostart for a node");
        eprintln!("  (See also: bubbaloop launch  -- launch multi-instance YAML)");
        eprintln!("\nRun 'bubbaloop node <command> --help' for more information.");
    }
}

pub(crate) async fn send_command(name: &str, command: &str) -> Result<()> {
    let client = crate::cli::daemon_client::DaemonClient::connect().await?;
    let msg = client.send_node_command(name, command).await?;
    println!("{}", msg);
    Ok(())
}

pub(crate) fn truncate(s: &str, max: usize) -> String {
    if s.len() <= max {
        return s.to_string();
    }
    // Find the last char that *ends* at or before byte position max-3.
    let target = max.saturating_sub(3);
    let mut end = 0;
    for (i, c) in s.char_indices() {
        let char_end = i + c.len_utf8();
        if char_end > target {
            break;
        }
        end = char_end;
    }
    format!("{}...", &s[..end])
}

fn init_node(args: InitArgs) -> Result<()> {
    // Validate node name before creating any files
    if args.name.is_empty() || args.name.len() > 64 {
        return Err(NodeError::CommandFailed(format!(
            "Node name must be 1-64 characters, got '{}'",
            args.name
        )));
    }
    if !args
        .name
        .chars()
        .all(|c| c.is_alphanumeric() || c == '-' || c == '_')
    {
        return Err(NodeError::CommandFailed(format!(
            "Node name '{}' contains invalid characters (only alphanumeric, hyphens, and underscores allowed)",
            args.name
        )));
    }

    // Validate category
    let category = args.category.to_lowercase();
    if !matches!(category.as_str(), "source" | "sink" | "processor") {
        return Err(NodeError::CommandFailed(format!(
            "Invalid category '{}'. Use: source, sink, or processor",
            args.category
        )));
    }

    // Determine output directory (default: ./<name> in current directory)
    let output_dir = args
        .output
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from(".").join(&args.name));

    // Use shared template module
    let output_dir = templates::create_node_at(
        &args.name,
        &args.node_type,
        &args.author,
        &args.description,
        &category,
        &output_dir,
    )
    .map_err(|e| NodeError::CommandFailed(e.to_string()))?;

    // Copy canonical header.proto if protos/ directory exists
    install::copy_canonical_header_proto(&output_dir);

    let abs_path = output_dir.canonicalize().unwrap_or(output_dir.clone());

    println!(
        "Initialized node '{}' at: {}",
        args.name,
        abs_path.display()
    );
    println!();
    println!("Next steps:");
    println!("  cd {}", output_dir.display());
    if args.node_type.to_lowercase() == "rust" {
        println!("  # Edit src/node.rs with your logic");
        println!("  cargo build --release");
    } else {
        println!("  # Edit main.py with your logic");
        println!("  pip install -r requirements.txt");
    }
    println!();
    println!("To register with bubbaloop daemon:");
    println!("  bubbaloop node add {}", abs_path.display());

    Ok(())
}

fn validate_node(args: ValidateArgs) -> Result<()> {
    let path = PathBuf::from(&args.path);

    // 1. Check node.yaml exists
    let manifest_path = path.join("node.yaml");
    if !manifest_path.exists() {
        println!("FAIL: node.yaml not found at {}", manifest_path.display());
        return Err(NodeError::NotFound("node.yaml".into()));
    }
    println!("OK: node.yaml found");

    // 2. Parse manifest
    let content = std::fs::read_to_string(&manifest_path)?;
    let manifest: serde_yaml::Value = serde_yaml::from_str(&content)
        .map_err(|e| NodeError::CommandFailed(format!("Invalid YAML: {}", e)))?;
    println!("OK: node.yaml parses correctly");

    // 3. Check required fields
    let required = ["name", "version", "type"];
    for field in required {
        if manifest.get(field).is_none() {
            println!("FAIL: Missing required field: {}", field);
            return Err(NodeError::CommandFailed(format!(
                "Missing field: {}",
                field
            )));
        }
    }
    println!("OK: All required fields present (name, version, type)");

    // 4. Validate type
    let node_type = manifest["type"].as_str().unwrap_or("");
    if node_type != "rust" && node_type != "python" {
        println!(
            "WARN: Unknown type '{}' (expected: rust or python)",
            node_type
        );
    }

    // 5. Check command file exists (if specified)
    if let Some(cmd) = manifest.get("command").and_then(|v| v.as_str()) {
        let cmd_path = path.join(cmd.trim_start_matches("./"));
        if cmd_path.exists() {
            println!("OK: Command '{}' exists", cmd);
        } else {
            println!("INFO: Command '{}' does not exist yet (needs build)", cmd);
        }
    }

    println!();
    println!("Validation passed!");
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_truncate_short_string() {
        assert_eq!(truncate("hello", 10), "hello");
    }

    #[test]
    fn test_truncate_exact_length() {
        assert_eq!(truncate("hello", 5), "hello");
    }

    #[test]
    fn test_truncate_long_string() {
        assert_eq!(truncate("hello world", 8), "hello...");
    }

    #[test]
    fn test_truncate_very_long_string() {
        let long = "This is a very long description that exceeds the maximum length";
        // Function takes first (max - 3) chars and adds "..."
        assert_eq!(truncate(long, 30), "This is a very long descrip...");
        assert_eq!(truncate(long, 30).len(), 30);
    }

    #[test]
    fn test_truncate_multibyte_utf8() {
        // Should not panic when truncation would fall inside a multi-byte char
        let s = "cafe\u{0301} is great"; // "café is great" with combining accent
        let result = truncate(s, 8);
        assert!(result.ends_with("..."));
        // Result must be valid UTF-8 and not exceed max bytes
        assert!(result.len() <= 8);

        // Pure multi-byte: each snowman is 3 bytes, 5 snowmen = 15 bytes
        let s = "\u{2603}\u{2603}\u{2603}\u{2603}\u{2603}";
        let result = truncate(s, 10);
        assert!(result.ends_with("..."));
        // 10 - 3 = 7 target bytes, fits 2 snowmen (6 bytes) + "..." = 9
        assert_eq!(result, "\u{2603}\u{2603}...");
    }

    #[test]
    fn test_command_request_serialization() {
        let req = serde_json::json!({
            "command": "start",
            "node_path": "/path/to/node"
        });

        let json = serde_json::to_string(&req).unwrap();
        assert!(json.contains("\"command\""));
        assert!(json.contains("\"start\""));
        assert!(json.contains("\"node_path\""));
    }

    #[test]
    fn test_command_response_deserialization() {
        let json = r#"{"success": true, "message": "Node started", "output": ""}"#;
        let response: CommandResponse = serde_json::from_str(json).unwrap();
        assert!(response.success);
        assert_eq!(response.message, "Node started");
    }

    #[test]
    fn test_command_response_with_output() {
        let json = r#"{"success": true, "message": "Built", "output": "Compiling..."}"#;
        let response: CommandResponse = serde_json::from_str(json).unwrap();
        assert!(response.success);
        assert_eq!(response.output, "Compiling...");
    }

    #[test]
    fn test_node_state_serialization() {
        let node = NodeState {
            name: "test-node".to_string(),
            path: "/path/to/node".to_string(),
            status: "running".to_string(),
            installed: true,
            autostart_enabled: false,
            version: "1.0.0".to_string(),
            description: "Test node".to_string(),
            node_type: "rust".to_string(),
            is_built: true,
            base_node: String::new(),
        };

        let json = serde_json::to_string(&node).unwrap();
        assert!(json.contains("test-node"));
        assert!(json.contains("running"));
    }

    #[test]
    fn test_node_list_response_deserialization() {
        let json = r#"{"nodes": [{"name": "node1", "path": "/path", "status": "running",
                     "installed": true, "autostart_enabled": false, "version": "1.0.0",
                     "description": "Test", "node_type": "rust", "is_built": true}]}"#;
        let response: NodeListResponse = serde_json::from_str(json).unwrap();
        assert_eq!(response.nodes.len(), 1);
        assert_eq!(response.nodes[0].name, "node1");
    }

    /// Validate node name checking logic (mirrors submit_create_node_form validation)
    fn is_valid_node_name(name: &str) -> bool {
        !name.is_empty()
            && name
                .chars()
                .all(|c| c.is_alphanumeric() || c == '-' || c == '_')
            && !name.starts_with('-')
            && !name.starts_with('.')
    }

    #[test]
    fn test_valid_node_names() {
        assert!(is_valid_node_name("my-node"));
        assert!(is_valid_node_name("my_node"));
        assert!(is_valid_node_name("sensor1"));
        assert!(is_valid_node_name("MyNode"));
    }

    #[test]
    fn test_invalid_node_names_path_traversal() {
        assert!(!is_valid_node_name("../etc/passwd"));
        assert!(!is_valid_node_name("../../root"));
        assert!(!is_valid_node_name("foo/bar"));
    }

    #[test]
    fn test_invalid_node_names_special_chars() {
        assert!(!is_valid_node_name("node;evil"));
        assert!(!is_valid_node_name("node name"));
        assert!(!is_valid_node_name("node&evil"));
        assert!(!is_valid_node_name("$HOME"));
    }

    #[test]
    fn test_invalid_node_names_edge_cases() {
        assert!(!is_valid_node_name(""));
        assert!(!is_valid_node_name("-starts-with-dash"));
        assert!(!is_valid_node_name(".hidden"));
    }

    #[test]
    fn test_node_state_base_node_deserialization() {
        let json = r#"{"name": "rtsp-camera-terrace", "path": "/path", "status": "running",
                     "installed": true, "autostart_enabled": false, "version": "1.0.0",
                     "description": "Test", "node_type": "rust", "is_built": true,
                     "base_node": "rtsp-camera"}"#;
        let node: NodeState = serde_json::from_str(json).unwrap();
        assert_eq!(node.base_node, "rtsp-camera");
    }

    #[test]
    fn test_node_state_base_node_defaults_empty() {
        let json = r#"{"name": "openmeteo", "path": "/path", "status": "stopped",
                     "installed": true, "autostart_enabled": false, "version": "1.0.0",
                     "description": "Weather", "node_type": "rust", "is_built": true}"#;
        let node: NodeState = serde_json::from_str(json).unwrap();
        assert_eq!(node.base_node, "");
    }

    #[test]
    fn test_list_filter_base_only() {
        let nodes = [
            NodeState {
                name: "rtsp-camera".to_string(),
                path: "/path".to_string(),
                status: "running".to_string(),
                installed: true,
                autostart_enabled: false,
                version: "1.0.0".to_string(),
                description: "Camera".to_string(),
                node_type: "rust".to_string(),
                is_built: true,
                base_node: String::new(),
            },
            NodeState {
                name: "rtsp-camera-terrace".to_string(),
                path: "/path".to_string(),
                status: "running".to_string(),
                installed: true,
                autostart_enabled: false,
                version: "1.0.0".to_string(),
                description: "Terrace cam".to_string(),
                node_type: "rust".to_string(),
                is_built: true,
                base_node: "rtsp-camera".to_string(),
            },
            NodeState {
                name: "openmeteo".to_string(),
                path: "/path2".to_string(),
                status: "stopped".to_string(),
                installed: true,
                autostart_enabled: false,
                version: "1.0.0".to_string(),
                description: "Weather".to_string(),
                node_type: "rust".to_string(),
                is_built: false,
                base_node: String::new(),
            },
        ];

        // --base filter: only base nodes (base_node is empty)
        let base_only: Vec<_> = nodes.iter().filter(|n| n.base_node.is_empty()).collect();
        assert_eq!(base_only.len(), 2);
        assert_eq!(base_only[0].name, "rtsp-camera");
        assert_eq!(base_only[1].name, "openmeteo");

        // --instances filter: only instances (base_node is non-empty)
        let instances_only: Vec<_> = nodes.iter().filter(|n| !n.base_node.is_empty()).collect();
        assert_eq!(instances_only.len(), 1);
        assert_eq!(instances_only[0].name, "rtsp-camera-terrace");
        assert_eq!(instances_only[0].base_node, "rtsp-camera");
    }

    #[test]
    fn test_instance_name_validation() {
        // Valid instance names
        assert!(is_valid_node_name("rtsp-camera-terrace"));
        assert!(is_valid_node_name("rtsp-camera_1"));
        assert!(is_valid_node_name("cam01"));

        // Invalid: empty, starts with dash, special chars
        assert!(!is_valid_node_name(""));
        assert!(!is_valid_node_name("-starts-with-dash"));
        assert!(!is_valid_node_name("has space"));
        assert!(!is_valid_node_name("has;semicolon"));
        assert!(!is_valid_node_name("../traversal"));
        assert!(!is_valid_node_name(".hidden"));
    }
}
