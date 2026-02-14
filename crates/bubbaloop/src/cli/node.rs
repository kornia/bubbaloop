//! Node management CLI commands
//!
//! Commands for managing bubbaloop nodes from the command line.
//! These interact with the daemon via Zenoh to manage systemd services.

use argh::FromArgs;
use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};
use std::process::Command;
use thiserror::Error;
use zenoh::query::QueryTarget;

use crate::registry;
use crate::templates;

#[derive(Debug, Error)]
pub enum NodeError {
    #[error("Zenoh error: {0}")]
    Zenoh(String),
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

/// Copy canonical header.proto to a node's protos/ directory if it exists.
/// This ensures nodes use the correct version of header.proto.
fn copy_canonical_header_proto(node_path: &Path) {
    let protos_dir = node_path.join("protos");
    if !protos_dir.exists() {
        return;
    }

    let dest = protos_dir.join("header.proto");
    if let Err(e) = std::fs::write(&dest, crate::HEADER_PROTO) {
        log::warn!(
            "Could not copy canonical header.proto to {}: {}",
            dest.display(),
            e
        );
    } else {
        log::info!("Copied canonical header.proto to {}", dest.display());
    }
}

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
struct InstallArgs {
    /// node name (registered node or marketplace name)
    #[argh(positional)]
    name: String,

    /// git branch for marketplace install (default: main)
    #[argh(option, short = 'b', default = "String::from(\"main\")")]
    branch: String,

    /// also build the node (marketplace install only)
    #[argh(switch)]
    build: bool,
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
struct LogsArgs {
    /// node name
    #[argh(positional)]
    name: String,

    /// number of lines to show (default: 50)
    #[argh(option, short = 'n', default = "50")]
    lines: usize,

    /// follow log output
    #[argh(switch, short = 'f')]
    follow: bool,
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

/// Response from daemon API
#[derive(Deserialize)]
struct CommandResponse {
    success: bool,
    message: String,
    #[serde(default)]
    output: String,
}

#[derive(Deserialize, Serialize)]
struct NodeState {
    name: String,
    path: String,
    status: String,
    installed: bool,
    autostart_enabled: bool,
    version: String,
    description: String,
    node_type: String,
    is_built: bool,
    #[serde(default)]
    base_node: String,
}

#[derive(Deserialize)]
struct NodeListResponse {
    nodes: Vec<NodeState>,
}

#[derive(Deserialize)]
struct LogsResponse {
    lines: Vec<String>,
    success: bool,
    #[serde(default)]
    error: Option<String>,
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
            Some(NodeAction::List(args)) => list_nodes(args).await,
            Some(NodeAction::Add(args)) => add_node(args).await,
            Some(NodeAction::Remove(args)) => remove_node(args).await,
            Some(NodeAction::Instance(args)) => create_instance(args).await,
            Some(NodeAction::Install(args)) => handle_install(args).await,
            Some(NodeAction::Uninstall(args)) => send_command(&args.name, "uninstall").await,
            Some(NodeAction::Start(args)) => send_command(&args.name, "start").await,
            Some(NodeAction::Stop(args)) => send_command(&args.name, "stop").await,
            Some(NodeAction::Restart(args)) => send_command(&args.name, "restart").await,
            Some(NodeAction::Logs(args)) => view_logs(args).await,
            Some(NodeAction::Build(args)) => send_command(&args.name, "build").await,
            Some(NodeAction::Clean(args)) => send_command(&args.name, "clean").await,
            Some(NodeAction::Enable(args)) => send_command(&args.name, "enable").await,
            Some(NodeAction::Disable(args)) => send_command(&args.name, "disable").await,
            Some(NodeAction::Search(args)) => search_nodes(args),
            Some(NodeAction::Discover(args)) => discover_nodes(args).await,
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

pub(crate) async fn get_zenoh_session() -> Result<zenoh::Session> {
    // Connect to local zenoh router, or custom endpoint via env var
    let mut config = zenoh::Config::default();

    // Run as client mode - only connect to router, don't listen
    config
        .insert_json5("mode", "\"client\"")
        .map_err(|e| NodeError::Zenoh(e.to_string()))?;

    let endpoint = std::env::var("BUBBALOOP_ZENOH_ENDPOINT")
        .unwrap_or_else(|_| "tcp/127.0.0.1:7447".to_string());
    config
        .insert_json5("connect/endpoints", &format!("[\"{}\"]", endpoint))
        .map_err(|e| NodeError::Zenoh(e.to_string()))?;

    // Disable all scouting to avoid connecting to remote peers via Tailscale
    config
        .insert_json5("scouting/multicast/enabled", "false")
        .map_err(|e| NodeError::Zenoh(e.to_string()))?;
    config
        .insert_json5("scouting/gossip/enabled", "false")
        .map_err(|e| NodeError::Zenoh(e.to_string()))?;

    let session = zenoh::open(config)
        .await
        .map_err(|e| NodeError::Zenoh(e.to_string()))?;
    Ok(session)
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
        &output_dir,
    )
    .map_err(|e| NodeError::CommandFailed(e.to_string()))?;

    // Copy canonical header.proto if protos/ directory exists
    copy_canonical_header_proto(&output_dir);

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

async fn list_nodes(args: ListArgs) -> Result<()> {
    if args.base && args.instances {
        return Err(NodeError::InvalidArgs(
            "Cannot use --base and --instances together".into(),
        ));
    }

    let session = get_zenoh_session().await?;

    // Retry up to 3 times with 1 second delay between retries
    let mut best_data: Option<NodeListResponse> = None;

    for attempt in 1..=3 {
        let replies_result = session
            .get("bubbaloop/daemon/api/nodes")
            .target(QueryTarget::BestMatching)
            .timeout(std::time::Duration::from_secs(30))
            .await;

        if let Ok(replies) = replies_result {
            for reply in replies {
                if let Ok(sample) = reply.into_result() {
                    if let Ok(data) =
                        serde_json::from_slice::<NodeListResponse>(&sample.payload().to_bytes())
                    {
                        if !data.nodes.is_empty() || best_data.is_none() {
                            best_data = Some(data);
                            if !best_data.as_ref().unwrap().nodes.is_empty() {
                                break;
                            }
                        }
                    }
                }
            }

            if best_data.as_ref().is_some_and(|d| !d.nodes.is_empty()) {
                break;
            }
        }

        if attempt < 3 {
            tokio::time::sleep(std::time::Duration::from_secs(1)).await;
        }
    }

    if let Some(mut data) = best_data {
        // Apply --base / --instances filter
        if args.base {
            data.nodes.retain(|n| n.base_node.is_empty());
        } else if args.instances {
            data.nodes.retain(|n| !n.base_node.is_empty());
        }

        if args.format == "json" {
            println!("{}", serde_json::to_string_pretty(&data.nodes)?);
        } else if data.nodes.is_empty() {
            println!("No nodes registered. Use 'bubbaloop node add <path>' to add one.");
        } else {
            let has_instances = data.nodes.iter().any(|n| !n.base_node.is_empty());
            if has_instances {
                println!(
                    "{:<20} {:<10} {:<16} {:<12} {:<8} DESCRIPTION",
                    "NAME", "STATUS", "BASE", "TYPE", "BUILT"
                );
                println!("{}", "-".repeat(86));
                for node in data.nodes {
                    let built = if node.is_built { "yes" } else { "no" };
                    let base = if node.base_node.is_empty() {
                        "-"
                    } else {
                        &node.base_node
                    };
                    println!(
                        "{:<20} {:<10} {:<16} {:<12} {:<8} {}",
                        node.name,
                        node.status,
                        base,
                        node.node_type,
                        built,
                        truncate(&node.description, 30)
                    );
                }
            } else {
                println!(
                    "{:<20} {:<10} {:<12} {:<8} DESCRIPTION",
                    "NAME", "STATUS", "TYPE", "BUILT"
                );
                println!("{}", "-".repeat(70));
                for node in data.nodes {
                    let built = if node.is_built { "yes" } else { "no" };
                    println!(
                        "{:<20} {:<10} {:<12} {:<8} {}",
                        node.name,
                        node.status,
                        node.node_type,
                        built,
                        truncate(&node.description, 30)
                    );
                }
            }
        }
    } else {
        println!("No response from daemon. Is it running?");
    }

    session
        .close()
        .await
        .map_err(|e| NodeError::Zenoh(e.to_string()))?;
    Ok(())
}

/// Discover node.yaml files in immediate subdirectories of a path.
/// Returns Vec<(node_name, subdir_name, node_type)>.
fn discover_nodes_in_subdirs(base_path: &Path) -> Vec<(String, String, String)> {
    let manifest_field = |manifest: &serde_yaml::Value, key: &str| -> String {
        manifest
            .get(key)
            .and_then(|v| v.as_str())
            .unwrap_or("unknown")
            .to_string()
    };

    let mut nodes: Vec<_> = std::fs::read_dir(base_path)
        .into_iter()
        .flatten()
        .flatten()
        .filter(|e| e.path().is_dir())
        .filter_map(|entry| {
            let yaml_path = entry.path().join("node.yaml");
            let content = std::fs::read_to_string(&yaml_path).ok()?;
            match serde_yaml::from_str::<serde_yaml::Value>(&content) {
                Ok(manifest) => {
                    let subdir = entry.file_name().to_string_lossy().to_string();
                    Some((
                        manifest_field(&manifest, "name"),
                        subdir,
                        manifest_field(&manifest, "type"),
                    ))
                }
                Err(e) => {
                    log::warn!("Skipping {}: invalid node.yaml: {}", yaml_path.display(), e);
                    None
                }
            }
        })
        .collect();

    nodes.sort_by(|a, b| a.0.cmp(&b.0));
    nodes
}

/// Resolve the node path, applying --subdir if set, or discovering nodes if needed.
fn resolve_node_path(base_path: &str, subdir: Option<&str>) -> Result<String> {
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

async fn add_node(args: AddArgs) -> Result<()> {
    // Normalize source URL
    let source = normalize_git_url(&args.source);

    let base_path = if is_git_url(&source) {
        // Clone from GitHub
        clone_from_github(&source, args.output.as_deref(), &args.branch)?
    } else {
        // Local path
        let path = Path::new(&args.source);
        if !path.exists() {
            return Err(NodeError::NotFound(args.source));
        }
        path.canonicalize()?.to_string_lossy().to_string()
    };

    // Resolve the actual node path (handles --subdir and multi-node discovery)
    let node_path = resolve_node_path(&base_path, args.subdir.as_deref())?;

    // Add to daemon via Zenoh
    let session = get_zenoh_session().await?;

    let payload = serde_json::to_string(&serde_json::json!({
        "command": "add",
        "node_path": node_path,
        "name": args.name,
        "config": args.config,
    }))?;

    let mut node_name: Option<String> = None;

    for attempt in 1..=3 {
        if let Ok(replies) = session
            .get("bubbaloop/daemon/api/nodes/add")
            .payload(payload.clone())
            .target(QueryTarget::BestMatching)
            .timeout(std::time::Duration::from_secs(30))
            .await
        {
            for reply in replies {
                if let Ok(sample) = reply.into_result() {
                    let data: CommandResponse =
                        serde_json::from_slice(&sample.payload().to_bytes())?;
                    if data.success {
                        println!("Added node from: {}", node_path);
                        node_name = extract_node_name(&node_path).ok();
                        break;
                    } else if attempt == 3 {
                        session
                            .close()
                            .await
                            .map_err(|e| NodeError::Zenoh(e.to_string()))?;
                        return Err(NodeError::CommandFailed(data.message));
                    }
                }
            }

            if node_name.is_some() {
                break;
            }
        }

        if attempt < 3 {
            tokio::time::sleep(std::time::Duration::from_secs(1)).await;
        }
    }

    // Optional: build
    if args.build {
        if let Some(ref name) = node_name {
            println!("Building node...");
            send_command(name, "build").await?;
        }
    }

    // Optional: install
    if args.install {
        if let Some(ref name) = node_name {
            println!("Installing as service...");
            send_command(name, "install").await?;
        }
    }

    session
        .close()
        .await
        .map_err(|e| NodeError::Zenoh(e.to_string()))?;
    Ok(())
}

/// Create an instance of a multi-instance node (like rtsp-camera)
///
/// This command creates a named instance from an already-registered base node.
/// The instance will use the base node's binary but with its own config file.
///
/// # Arguments
/// * `args.base_node` - Name of the registered base node (e.g., "rtsp-camera")
/// * `args.suffix` - Instance suffix (e.g., "terrace" creates "rtsp-camera-terrace")
/// * `args.config` - Path to config file for this instance
/// * `args.copy_config` - Copy example config from base node's configs/ directory
/// * `args.install` - Install as systemd service after creating
/// * `args.start` - Start the instance after creating (implies --install)
///
/// # Example
/// ```bash
/// bubbaloop node instance rtsp-camera terrace --config ~/.bubbaloop/configs/rtsp-camera-terrace.yaml
/// ```
async fn create_instance(args: InstanceArgs) -> Result<()> {
    // Validate suffix (same rules as node names: alphanumeric, hyphens, underscores)
    if args.suffix.is_empty() || args.suffix.len() > 64 {
        return Err(NodeError::InvalidArgs(
            "Instance suffix must be 1-64 characters".into(),
        ));
    }
    if !args
        .suffix
        .chars()
        .all(|c| c.is_alphanumeric() || c == '-' || c == '_')
    {
        return Err(NodeError::InvalidArgs(
            "Instance suffix can only contain alphanumeric characters, hyphens, and underscores"
                .into(),
        ));
    }
    if args.suffix.starts_with('-') || args.suffix.starts_with('_') {
        return Err(NodeError::InvalidArgs(
            "Instance suffix cannot start with hyphen or underscore".into(),
        ));
    }

    // Build the full instance name: base-suffix
    let instance_name = format!("{}-{}", args.base_node, args.suffix);

    // First, find the base node to get its path
    let session = get_zenoh_session().await?;

    // Query the daemon for the base node's path
    let query_payload = serde_json::to_string(&serde_json::json!({
        "command": "get_node",
        "name": args.base_node,
    }))?;

    let replies = session
        .get("bubbaloop/daemon/api/nodes")
        .payload(query_payload)
        .target(QueryTarget::BestMatching)
        .timeout(std::time::Duration::from_secs(10))
        .await
        .map_err(|e| NodeError::Zenoh(e.to_string()))?;

    let mut base_node_path: Option<String> = None;

    for reply in replies {
        if let Ok(sample) = reply.into_result() {
            let data: NodeListResponse = serde_json::from_slice(&sample.payload().to_bytes())?;
            // Find the base node (must have empty base_node field, meaning it's not an instance)
            for node in data.nodes {
                if node.name == args.base_node && node.base_node.is_empty() {
                    base_node_path = Some(node.path.clone());
                    break;
                }
            }
        }
    }

    let base_path = base_node_path.ok_or_else(|| {
        NodeError::NotFound(format!(
            "Base node '{}' not found. Add it first with: bubbaloop node add <path>",
            args.base_node
        ))
    })?;

    // Handle config file
    let config_path = if args.copy_config {
        // Copy example config from base node's configs/ directory
        let configs_dir = Path::new(&base_path).join("configs");
        if !configs_dir.exists() {
            return Err(NodeError::NotFound(format!(
                "No configs/ directory found in base node at {}",
                base_path
            )));
        }

        // Look for an example config
        let example_config = find_example_config(&configs_dir)?;

        // Create ~/.bubbaloop/configs/ if needed
        let dest_dir = dirs::home_dir()
            .ok_or_else(|| NodeError::Io(std::io::Error::other("HOME not set")))?
            .join(".bubbaloop")
            .join("configs");
        std::fs::create_dir_all(&dest_dir)?;

        // Copy to ~/.bubbaloop/configs/<instance-name>.yaml
        let dest_path = dest_dir.join(format!("{}.yaml", instance_name));
        std::fs::copy(&example_config, &dest_path)?;

        println!("Copied example config to: {}", dest_path.display());
        println!("Edit this file to configure your instance before starting.");

        Some(dest_path.to_string_lossy().to_string())
    } else if let Some(ref config) = args.config {
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

    // Register the instance with the daemon
    let add_payload = serde_json::to_string(&serde_json::json!({
        "command": "add",
        "node_path": base_path,
        "name": instance_name,
        "config": config_path,
    }))?;

    let replies = session
        .get("bubbaloop/daemon/api/nodes/add")
        .payload(add_payload)
        .target(QueryTarget::BestMatching)
        .timeout(std::time::Duration::from_secs(30))
        .await
        .map_err(|e| NodeError::Zenoh(e.to_string()))?;

    let mut success = false;
    for reply in replies {
        if let Ok(sample) = reply.into_result() {
            let data: CommandResponse = serde_json::from_slice(&sample.payload().to_bytes())?;
            if data.success {
                println!(
                    "Created instance '{}' from base node '{}'",
                    instance_name, args.base_node
                );
                success = true;
            } else {
                session
                    .close()
                    .await
                    .map_err(|e| NodeError::Zenoh(e.to_string()))?;
                return Err(NodeError::CommandFailed(data.message));
            }
        }
    }

    if !success {
        session
            .close()
            .await
            .map_err(|e| NodeError::Zenoh(e.to_string()))?;
        return Err(NodeError::Zenoh("No response from daemon".into()));
    }

    // Close the session before send_command (which opens its own)
    session
        .close()
        .await
        .map_err(|e| NodeError::Zenoh(e.to_string()))?;

    // Optional: install and/or start
    if args.start || args.install {
        println!("Installing instance as systemd service...");
        send_command(&instance_name, "install").await?;
    }

    if args.start {
        println!("Starting instance...");
        send_command(&instance_name, "start").await?;
    }

    Ok(())
}

/// Find an example config file in the configs/ directory
fn find_example_config(configs_dir: &Path) -> Result<PathBuf> {
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

fn normalize_git_url(source: &str) -> String {
    // If it's an existing local path, return it unchanged
    if std::path::Path::new(source).exists() {
        return source.to_string();
    }
    if source.starts_with("https://") || source.starts_with("git@") {
        source.to_string()
    } else if source.starts_with("github.com/") {
        format!("https://{}", source)
    } else if source.contains('/')
        && !source.contains(':')
        && !source.starts_with('/')
        && !source.starts_with('.')
    {
        // Shorthand: user/repo -> https://github.com/user/repo
        format!("https://github.com/{}", source)
    } else {
        source.to_string()
    }
}

fn is_git_url(source: &str) -> bool {
    source.starts_with("https://github.com/")
        || source.starts_with("git@github.com:")
        || (source.contains("github.com/") && !source.starts_with('/') && !source.starts_with('.'))
}

fn extract_node_name(path: &str) -> Result<String> {
    let node_yaml = Path::new(path).join("node.yaml");
    if node_yaml.exists() {
        let content = std::fs::read_to_string(&node_yaml)?;
        let manifest: serde_yaml::Value =
            serde_yaml::from_str(&content).map_err(|e| NodeError::CommandFailed(e.to_string()))?;
        if let Some(name) = manifest.get("name").and_then(|v| v.as_str()) {
            return Ok(name.to_string());
        }
    }
    // Fallback to directory name
    Path::new(path)
        .file_name()
        .map(|s| s.to_string_lossy().to_string())
        .ok_or_else(|| NodeError::CommandFailed("Cannot extract node name".into()))
}

fn clone_from_github(url: &str, output: Option<&str>, branch: &str) -> Result<String> {
    // Prevent argument injection via branch or URL starting with '-'
    if branch.starts_with('-') {
        return Err(NodeError::InvalidUrl(format!(
            "Invalid branch name: {}",
            branch
        )));
    }
    if url.starts_with('-') {
        return Err(NodeError::InvalidUrl(format!("Invalid URL: {}", url)));
    }

    // Extract repo name from URL
    let repo_name = url
        .trim_end_matches('/')
        .trim_end_matches(".git")
        .rsplit('/')
        .next()
        .ok_or_else(|| NodeError::InvalidUrl(url.to_string()))?;

    // Determine target directory
    let target_dir = if let Some(out) = output {
        PathBuf::from(out)
    } else {
        let home = std::env::var("HOME").unwrap_or_else(|_| "/tmp".to_string());
        PathBuf::from(home)
            .join(".bubbaloop")
            .join("nodes")
            .join(repo_name)
    };

    if target_dir.exists() {
        // Reuse existing clone (e.g., installing a second node from the same multi-node repo)
        println!("Using existing clone at {}", target_dir.display());
    } else {
        // Create parent directory
        if let Some(parent) = target_dir.parent() {
            std::fs::create_dir_all(parent)?;
        }

        println!(
            "Cloning {} (branch: {}) to {}...",
            url,
            branch,
            target_dir.display()
        );

        // Clone the repository with branch
        let clone_output = Command::new("git")
            .args([
                "clone",
                "--depth",
                "1",
                "--branch",
                branch,
                "--", // Prevent URL from being treated as an option
                url,
                &target_dir.to_string_lossy(),
            ])
            .output()?;

        if !clone_output.status.success() {
            let stderr = String::from_utf8_lossy(&clone_output.stderr);
            return Err(NodeError::GitClone(stderr.to_string()));
        }

        println!("Cloned successfully!");
    }

    // Check for node.yaml
    let manifest = target_dir.join("node.yaml");
    if !manifest.exists() {
        eprintln!("Warning: No node.yaml found in repository. You may need to create one.");
    }

    Ok(target_dir.to_string_lossy().to_string())
}

async fn remove_node(args: RemoveArgs) -> Result<()> {
    let session = get_zenoh_session().await?;

    let payload = serde_json::to_string(&serde_json::json!({
        "command": "remove"
    }))?;

    let key = format!("bubbaloop/daemon/api/nodes/{}/command", args.name);
    let replies: Vec<_> = session
        .get(&key)
        .payload(payload)
        .target(QueryTarget::BestMatching)
        .timeout(std::time::Duration::from_secs(30))
        .await
        .map_err(|e| NodeError::Zenoh(e.to_string()))?
        .into_iter()
        .collect();

    for reply in replies {
        if let Ok(sample) = reply.into_result() {
            let data: CommandResponse = serde_json::from_slice(&sample.payload().to_bytes())?;
            if data.success {
                println!("Removed node: {}", args.name);
            } else {
                return Err(NodeError::CommandFailed(data.message));
            }
        }
    }

    if args.delete_files {
        // Get node path first, then delete
        eprintln!("Note: File deletion not implemented yet. Remove files manually.");
    }

    session
        .close()
        .await
        .map_err(|e| NodeError::Zenoh(e.to_string()))?;
    Ok(())
}

pub(crate) async fn send_command(name: &str, command: &str) -> Result<()> {
    let session = get_zenoh_session().await?;
    let payload = serde_json::to_string(&serde_json::json!({"command": command}))?;
    let key = format!("bubbaloop/daemon/api/nodes/{}/command", name);

    let mut last_error = None;

    for attempt in 1..=3 {
        if let Ok(replies) = session
            .get(&key)
            .payload(payload.clone())
            .target(QueryTarget::BestMatching)
            .timeout(std::time::Duration::from_secs(30))
            .await
        {
            for reply in replies {
                if let Ok(sample) = reply.into_result() {
                    let data: CommandResponse =
                        serde_json::from_slice(&sample.payload().to_bytes())?;
                    if data.success {
                        println!("{}", data.message);
                        if !data.output.is_empty() {
                            println!("{}", data.output);
                        }
                        session
                            .close()
                            .await
                            .map_err(|e| NodeError::Zenoh(e.to_string()))?;
                        return Ok(());
                    }
                    last_error = Some(NodeError::CommandFailed(data.message));
                }
            }
        }

        if attempt < 3 {
            tokio::time::sleep(std::time::Duration::from_secs(1)).await;
        }
    }

    session
        .close()
        .await
        .map_err(|e| NodeError::Zenoh(e.to_string()))?;
    Err(last_error.unwrap_or_else(|| NodeError::Zenoh("No response from daemon".into())))
}

/// Detect the current CPU architecture and return the GitHub release arch suffix.
fn detect_arch() -> Result<&'static str> {
    match std::env::consts::ARCH {
        "x86_64" => Ok("amd64"),
        "aarch64" => Ok("arm64"),
        other => Err(NodeError::CommandFailed(format!(
            "Unsupported architecture: {}",
            other
        ))),
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

/// Download a file from a URL to a local path using curl.
fn download_file(url: &str, dest: &Path) -> Result<()> {
    let curl = find_curl()
        .ok_or_else(|| NodeError::CommandFailed("curl not found in standard paths".into()))?;

    let output = Command::new(curl)
        .args([
            "-sSfL",
            "--connect-timeout",
            "10",
            "--max-time",
            "300",
            "-o",
            &dest.to_string_lossy(),
            "--",
            url,
        ])
        .output()?;

    if output.status.success() {
        Ok(())
    } else {
        let stderr = String::from_utf8_lossy(&output.stderr);
        Err(NodeError::CommandFailed(format!(
            "Download failed: {}",
            stderr
        )))
    }
}

/// Download a small text file (e.g., checksum) and return its contents.
fn download_text(url: &str) -> Result<String> {
    let curl = find_curl()
        .ok_or_else(|| NodeError::CommandFailed("curl not found in standard paths".into()))?;

    let output = Command::new(curl)
        .args([
            "-sSfL",
            "--connect-timeout",
            "10",
            "--max-time",
            "30",
            "--",
            url,
        ])
        .output()?;

    if output.status.success() {
        Ok(String::from_utf8_lossy(&output.stdout).to_string())
    } else {
        let stderr = String::from_utf8_lossy(&output.stderr);
        Err(NodeError::CommandFailed(format!(
            "Download failed: {}",
            stderr
        )))
    }
}

/// Verify that a file matches an expected SHA256 checksum.
///
/// The `expected` string should be in the format output by `sha256sum`:
/// `<hex_hash>  <filename>\n` or just `<hex_hash>`.
fn verify_sha256(path: &Path, expected: &str) -> Result<()> {
    let output = Command::new("sha256sum")
        .arg(path)
        .output()
        .map_err(|e| NodeError::CommandFailed(format!("sha256sum not found: {}", e)))?;

    if !output.status.success() {
        return Err(NodeError::CommandFailed("sha256sum failed".into()));
    }

    let actual_line = String::from_utf8_lossy(&output.stdout);
    let actual_hash = actual_line.split_whitespace().next().unwrap_or("");
    let expected_hash = expected.split_whitespace().next().unwrap_or("");

    if actual_hash == expected_hash {
        Ok(())
    } else {
        Err(NodeError::CommandFailed(format!(
            "Checksum mismatch: expected {}, got {}",
            expected_hash, actual_hash
        )))
    }
}

/// Set a file as executable (chmod 755).
fn set_executable(path: &Path) -> Result<()> {
    use std::os::unix::fs::PermissionsExt;
    let perms = std::fs::Permissions::from_mode(0o755);
    std::fs::set_permissions(path, perms)?;
    Ok(())
}

/// Attempt to download a precompiled binary for a registry node.
///
/// Returns the node directory path on success. On failure, the caller should
/// fall back to cloning and building from source.
fn try_download_precompiled(entry: &registry::RegistryNode) -> Result<String> {
    let binary_name = entry
        .binary
        .as_ref()
        .ok_or_else(|| NodeError::CommandFailed("No binary field in registry entry".into()))?;

    if entry.node_type != "rust" {
        return Err(NodeError::CommandFailed("Not a Rust node".into()));
    }

    let arch = detect_arch()?;

    let url = registry::precompiled_url(entry, arch)
        .ok_or_else(|| NodeError::CommandFailed("Cannot construct download URL".into()))?;
    let checksum_url = format!("{}.sha256", url);

    // Create node directory: ~/.bubbaloop/nodes/<repo-name>/<subdir>/
    let repo_name = entry
        .repo
        .rsplit('/')
        .next()
        .unwrap_or("bubbaloop-nodes-official");
    let node_dir = crate::daemon::registry::get_bubbaloop_home()
        .join("nodes")
        .join(repo_name)
        .join(&entry.subdir);
    let binary_dir = node_dir.join("target").join("release");
    std::fs::create_dir_all(&binary_dir)?;

    // Write a minimal node.yaml so the daemon can read it
    let node_yaml = format!(
        "name: {}\nversion: {}\ntype: {}\ndescription: \"{}\"\ncommand: \"./target/release/{}\"\n",
        entry.name, entry.version, entry.node_type, entry.description, binary_name
    );
    std::fs::write(node_dir.join("node.yaml"), node_yaml)?;

    // Download checksum first (fast fail if release doesn't exist)
    log::info!("Downloading checksum from {}", checksum_url);
    let expected_checksum = download_text(&checksum_url)?;

    // Download binary
    let binary_path = binary_dir.join(binary_name);
    log::info!("Downloading binary from {}", url);
    println!("Downloading precompiled binary...");
    download_file(&url, &binary_path)?;

    // Verify checksum
    verify_sha256(&binary_path, &expected_checksum)?;

    // Make executable
    set_executable(&binary_path)?;

    Ok(node_dir.to_string_lossy().to_string())
}

/// Handle `node install`: if the node is already registered with the daemon,
/// install it as a systemd service (existing behavior). Otherwise, look up the
/// name in the marketplace registry, clone, register, build, and install.
async fn handle_install(args: InstallArgs) -> Result<()> {
    // First, check if node is already registered with the daemon
    let session = get_zenoh_session().await?;

    let replies_result = session
        .get("bubbaloop/daemon/api/nodes")
        .target(QueryTarget::BestMatching)
        .timeout(std::time::Duration::from_secs(10))
        .await;

    let mut is_registered = false;

    if let Ok(replies) = replies_result {
        for reply in replies {
            if let Ok(sample) = reply.into_result() {
                let payload = sample.payload().to_bytes();
                if let Ok(data) = serde_json::from_slice::<NodeListResponse>(&payload) {
                    is_registered = data.nodes.iter().any(|n| n.name == args.name);
                }
            }
        }
    }

    session
        .close()
        .await
        .map_err(|e| NodeError::Zenoh(e.to_string()))?;

    if is_registered {
        log::info!(
            "node install: '{}' is registered, installing systemd service",
            args.name
        );
        return send_command(&args.name, "install").await;
    }

    // Not registered -> try marketplace lookup
    log::info!(
        "node install: '{}' not registered, checking marketplace",
        args.name
    );
    println!(
        "Node '{}' not registered. Checking marketplace...",
        args.name
    );

    if let Err(e) = registry::refresh_cache() {
        log::warn!("registry refresh failed: {}", e);
        eprintln!("Warning: could not refresh registry (using cache): {}", e);
    }
    let nodes = registry::load_cached_registry();

    let entry = match registry::find_by_name(&nodes, &args.name) {
        Some(entry) => entry,
        None => {
            // Search for suggestions
            let suggestions = registry::search_registry(&nodes, &args.name, None, None);
            let mut msg = format!("Node '{}' not found in registry.", args.name);
            if !suggestions.is_empty() {
                msg.push_str("\n\nDid you mean:");
                for s in suggestions.iter().take(5) {
                    msg.push_str(&format!("\n  {}", s.name));
                }
            }
            msg.push_str("\n\nTry: bubbaloop node search");
            return Err(NodeError::NotFound(msg));
        }
    };

    log::info!(
        "node install: found '{}' in marketplace (repo={}, subdir={})",
        entry.name,
        entry.repo,
        entry.subdir
    );
    println!("Found '{}' in marketplace ({})", entry.name, entry.repo);

    // Validate repo before constructing URL
    registry::validate_repo(&entry.repo)
        .map_err(|e| NodeError::InvalidUrl(format!("Invalid registry repo: {}", e)))?;

    // Try precompiled binary first (fast path)
    match try_download_precompiled(&entry) {
        Ok(node_path) => {
            println!("Downloaded precompiled binary for '{}'", args.name);

            // Register with daemon
            let session = get_zenoh_session().await?;
            let payload = serde_json::to_string(&serde_json::json!({
                "command": "add",
                "node_path": node_path
            }))?;

            let mut registered_ok = false;
            for attempt in 1..=3 {
                let replies_result = session
                    .get("bubbaloop/daemon/api/nodes/add")
                    .payload(payload.clone())
                    .target(QueryTarget::BestMatching)
                    .timeout(std::time::Duration::from_secs(30))
                    .await;

                match replies_result {
                    Ok(replies) => {
                        for reply in replies {
                            if let Ok(sample) = reply.into_result() {
                                let data: CommandResponse =
                                    serde_json::from_slice(&sample.payload().to_bytes())?;
                                if data.success {
                                    registered_ok = true;
                                    break;
                                }
                            }
                        }
                        if registered_ok {
                            break;
                        }
                    }
                    Err(_) if attempt < 3 => {
                        tokio::time::sleep(std::time::Duration::from_secs(1)).await;
                    }
                    Err(e) => {
                        session
                            .close()
                            .await
                            .map_err(|e| NodeError::Zenoh(e.to_string()))?;
                        return Err(NodeError::Zenoh(e.to_string()));
                    }
                }
            }

            session
                .close()
                .await
                .map_err(|e| NodeError::Zenoh(e.to_string()))?;

            if !registered_ok {
                return Err(NodeError::CommandFailed(
                    "Failed to register node with daemon".into(),
                ));
            }

            println!("Registered node: {}", args.name);

            // Install as systemd service
            println!("Installing {} as systemd service...", args.name);
            send_command(&args.name, "install").await?;

            log::info!(
                "node install: completed precompiled install of '{}' from {}",
                args.name,
                entry.repo
            );
            println!(
                "\nInstalled '{}' from {} (precompiled)",
                args.name, entry.repo
            );
            return Ok(());
        }
        Err(e) => {
            log::info!(
                "Precompiled binary not available ({}), building from source",
                e
            );
            println!("Precompiled binary not available, building from source...");
        }
    }

    // Clone from GitHub
    let url = format!("https://github.com/{}", entry.repo);
    log::info!("node install: cloning {} branch={}", url, args.branch);
    let base_path = clone_from_github(&url, None, &args.branch)?;

    // Resolve subdir
    let node_path = resolve_node_path(&base_path, Some(&entry.subdir))?;

    // Copy canonical header.proto if protos/ directory exists
    copy_canonical_header_proto(Path::new(&node_path));

    // Register with daemon
    let session = get_zenoh_session().await?;

    let payload = serde_json::to_string(&serde_json::json!({
        "command": "add",
        "node_path": node_path
    }))?;

    let mut registered_ok = false;

    for attempt in 1..=3 {
        let replies_result = session
            .get("bubbaloop/daemon/api/nodes/add")
            .payload(payload.clone())
            .target(QueryTarget::BestMatching)
            .timeout(std::time::Duration::from_secs(30))
            .await;

        match replies_result {
            Ok(replies) => {
                for reply in replies {
                    if let Ok(sample) = reply.into_result() {
                        let data: CommandResponse =
                            serde_json::from_slice(&sample.payload().to_bytes())?;
                        if data.success {
                            log::info!("node install: registered '{}' with daemon", args.name);
                            println!("Registered node: {}", args.name);
                            registered_ok = true;
                            break;
                        }
                    }
                }
                if registered_ok {
                    break;
                }
            }
            Err(_) if attempt < 3 => {
                tokio::time::sleep(std::time::Duration::from_secs(1)).await;
            }
            Err(e) => {
                session
                    .close()
                    .await
                    .map_err(|e| NodeError::Zenoh(e.to_string()))?;
                return Err(NodeError::Zenoh(e.to_string()));
            }
        }
    }

    session
        .close()
        .await
        .map_err(|e| NodeError::Zenoh(e.to_string()))?;

    if !registered_ok {
        return Err(NodeError::CommandFailed(
            "Failed to register node with daemon".into(),
        ));
    }

    // Build only if --build is passed
    if args.build {
        println!("Building {}...", args.name);
        send_command(&args.name, "build").await?;
    }

    // Install as systemd service
    println!("Installing {} as systemd service...", args.name);
    send_command(&args.name, "install").await?;

    log::info!(
        "node install: completed marketplace install of '{}' from {}",
        args.name,
        entry.repo
    );
    println!("\nInstalled '{}' from {}", args.name, entry.repo);

    Ok(())
}

fn search_nodes(args: SearchArgs) -> Result<()> {
    log::info!(
        "node search: query={:?} category={:?} tag={:?}",
        args.query,
        args.category,
        args.tag
    );
    println!("Refreshing marketplace registry...");
    if let Err(e) = registry::refresh_cache() {
        log::warn!("registry refresh failed: {}", e);
        eprintln!("Warning: could not refresh registry (using cache): {}", e);
    }
    let all_nodes = registry::load_cached_registry();

    if all_nodes.is_empty() {
        println!("No nodes found in marketplace registry.");
        println!("The registry cache may not have been fetched yet.");
        return Ok(());
    }

    let results = registry::search_registry(
        &all_nodes,
        &args.query,
        args.category.as_deref(),
        args.tag.as_deref(),
    );

    if results.is_empty() {
        println!("No nodes matching your search.");
        if !args.query.is_empty() || args.category.is_some() || args.tag.is_some() {
            println!("Try: bubbaloop node search  (no arguments to list all)");
        }
        return Ok(());
    }

    println!(
        "{:<20} {:<10} {:<8} {:<12} {:<30} REPO",
        "NAME", "VERSION", "TYPE", "CATEGORY", "DESCRIPTION"
    );
    println!("{}", "-".repeat(110));
    for node in &results {
        println!(
            "{:<20} {:<10} {:<8} {:<12} {:<30} {}",
            node.name,
            node.version,
            node.node_type,
            node.category,
            truncate(&node.description, 28),
            node.repo
        );
    }
    println!();
    println!("Install with: bubbaloop node install <name>");

    Ok(())
}

async fn discover_nodes(args: DiscoverArgs) -> Result<()> {
    // Refresh marketplace cache
    if let Err(e) = registry::refresh_cache() {
        log::warn!("registry refresh failed: {}", e);
        eprintln!("Warning: could not refresh registry (using cache): {}", e);
    }
    let all_marketplace = registry::load_cached_registry();

    // Query daemon for registered nodes
    let registered: Vec<NodeState> = match get_zenoh_session().await {
        Ok(session) => {
            let result = session
                .get("bubbaloop/daemon/api/nodes")
                .target(QueryTarget::BestMatching)
                .timeout(std::time::Duration::from_secs(5))
                .await;
            let mut nodes = Vec::new();
            if let Ok(replies) = result {
                for reply in replies {
                    if let Ok(sample) = reply.into_result() {
                        if let Ok(data) =
                            serde_json::from_slice::<NodeListResponse>(&sample.payload().to_bytes())
                        {
                            nodes = data.nodes;
                            break;
                        }
                    }
                }
            }
            let _ = session.close().await;
            nodes
        }
        Err(_) => Vec::new(),
    };

    if all_marketplace.is_empty() {
        println!(
            "No nodes found in marketplace. The registry cache may not have been fetched yet."
        );
        return Ok(());
    }

    #[derive(serde::Serialize)]
    struct DiscoverEntry {
        name: String,
        version: String,
        node_type: String,
        is_added: bool,
        is_built: bool,
        instance_count: usize,
        repo: String,
        description: String,
    }

    let entries: Vec<DiscoverEntry> = all_marketplace
        .iter()
        .map(|node| {
            let reg = registered
                .iter()
                .find(|r| r.name == node.name && r.base_node.is_empty());
            let is_added = reg.is_some();
            let is_built = reg.map(|r| r.is_built).unwrap_or(false);
            let instance_count = registered
                .iter()
                .filter(|r| r.base_node == node.name)
                .count();
            DiscoverEntry {
                name: node.name.clone(),
                version: node.version.clone(),
                node_type: node.node_type.clone(),
                is_added,
                is_built,
                instance_count,
                repo: node.repo.clone(),
                description: truncate(&node.description, 28),
            }
        })
        .collect();

    if args.format == "json" {
        println!("{}", serde_json::to_string_pretty(&entries)?);
    } else {
        println!(
            "{:<20} {:<10} {:<8} {:<6} {:<6} {:<5} {:<30} DESCRIPTION",
            "NAME", "VERSION", "TYPE", "ADDED", "BUILT", "INST", "REPO"
        );
        println!("{}", "-".repeat(115));
        for e in &entries {
            let added = if e.is_added { "yes" } else { "-" };
            let built = if e.is_added && e.is_built {
                "yes"
            } else if e.is_added {
                "no"
            } else {
                "-"
            };
            let inst = if e.instance_count > 0 {
                format!("{}", e.instance_count)
            } else {
                "-".to_string()
            };
            println!(
                "{:<20} {:<10} {:<8} {:<6} {:<6} {:<5} {:<30} {}",
                e.name, e.version, e.node_type, added, built, inst, e.repo, e.description
            );
        }
        println!();
        println!("Add with: bubbaloop node add <name>");
        println!("Create instance: bubbaloop node add <path> --name <instance-name>");
    }

    Ok(())
}

async fn view_logs(args: LogsArgs) -> Result<()> {
    if args.follow {
        // Use journalctl directly for follow mode
        let service = format!("bubbaloop-{}.service", args.name);
        let status = Command::new("journalctl")
            .args(["--user", "-u", &service, "-f", "--no-pager"])
            .status()?;

        if !status.success() {
            // Fallback to systemctl status
            let _ = Command::new("systemctl")
                .args(["--user", "status", "-l", "--no-pager", &service])
                .status();
        }
        return Ok(());
    }

    let session = get_zenoh_session().await?;

    let key = format!("bubbaloop/daemon/api/nodes/{}/logs", args.name);
    let replies: Vec<_> = session
        .get(&key)
        .target(QueryTarget::BestMatching)
        .timeout(std::time::Duration::from_secs(30))
        .await
        .map_err(|e| NodeError::Zenoh(e.to_string()))?
        .into_iter()
        .collect();

    for reply in replies {
        if let Ok(sample) = reply.into_result() {
            let data: LogsResponse = serde_json::from_slice(&sample.payload().to_bytes())?;
            if data.success {
                for line in data.lines.iter().take(args.lines) {
                    println!("{}", line);
                }
            } else if let Some(error) = data.error {
                return Err(NodeError::CommandFailed(error));
            }
        }
    }

    session
        .close()
        .await
        .map_err(|e| NodeError::Zenoh(e.to_string()))?;
    Ok(())
}

fn truncate(s: &str, max: usize) -> String {
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_normalize_git_url_full_https() {
        assert_eq!(
            normalize_git_url("https://github.com/user/repo"),
            "https://github.com/user/repo"
        );
    }

    #[test]
    fn test_normalize_git_url_ssh() {
        assert_eq!(
            normalize_git_url("git@github.com:user/repo.git"),
            "git@github.com:user/repo.git"
        );
    }

    #[test]
    fn test_normalize_git_url_with_github_prefix() {
        assert_eq!(
            normalize_git_url("github.com/user/repo"),
            "https://github.com/user/repo"
        );
    }

    #[test]
    fn test_normalize_git_url_shorthand() {
        assert_eq!(
            normalize_git_url("user/repo"),
            "https://github.com/user/repo"
        );
    }

    #[test]
    fn test_normalize_git_url_local_path() {
        assert_eq!(normalize_git_url("/path/to/node"), "/path/to/node");
    }

    #[test]
    fn test_normalize_git_url_relative_path() {
        // Relative paths starting with . should be preserved as local paths
        assert_eq!(normalize_git_url("./node"), "./node");
        assert_eq!(normalize_git_url("../my-node"), "../my-node");
        assert_eq!(normalize_git_url("./path/to/node"), "./path/to/node");
    }

    #[test]
    fn test_is_git_url_https() {
        assert!(is_git_url("https://github.com/user/repo"));
    }

    #[test]
    fn test_is_git_url_ssh() {
        assert!(is_git_url("git@github.com:user/repo.git"));
    }

    #[test]
    fn test_is_git_url_with_prefix() {
        assert!(is_git_url("github.com/user/repo"));
    }

    #[test]
    fn test_is_git_url_local_path() {
        assert!(!is_git_url("/path/to/node"));
    }

    #[test]
    fn test_is_git_url_relative_path() {
        assert!(!is_git_url("./node"));
    }

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

    #[test]
    fn test_logs_response_deserialization() {
        let json = r#"{"lines": ["line1", "line2"], "success": true}"#;
        let response: LogsResponse = serde_json::from_str(json).unwrap();
        assert!(response.success);
        assert_eq!(response.lines.len(), 2);
        assert_eq!(response.lines[0], "line1");
    }

    #[test]
    fn test_logs_response_with_error() {
        let json = r#"{"lines": [], "success": false, "error": "Node not found"}"#;
        let response: LogsResponse = serde_json::from_str(json).unwrap();
        assert!(!response.success);
        assert_eq!(response.error, Some("Node not found".to_string()));
    }

    #[test]
    fn test_clone_rejects_branch_argument_injection() {
        // Branch starting with '-' could be interpreted as a git flag
        let result = clone_from_github("https://github.com/user/repo", None, "--upload-pack=evil");
        assert!(result.is_err());
        let err = result.unwrap_err().to_string();
        assert!(err.contains("Invalid branch name"));
    }

    #[test]
    fn test_clone_rejects_url_argument_injection() {
        let result = clone_from_github("--upload-pack=evil", None, "main");
        assert!(result.is_err());
        let err = result.unwrap_err().to_string();
        assert!(err.contains("Invalid URL"));
    }

    #[test]
    fn test_clone_accepts_valid_branch() {
        // This will fail at the git clone step (no network), but should not
        // fail at the argument validation step. We check by verifying the error
        // is NOT about an invalid branch/URL.
        let result = clone_from_github(
            "https://github.com/user/repo",
            Some("/tmp/bubbaloop-test-nonexistent"),
            "main",
        );
        // Either succeeds or fails for a reason other than argument injection
        if let Err(e) = result {
            let msg = e.to_string();
            assert!(!msg.contains("Invalid branch name"));
            assert!(!msg.contains("Invalid URL"));
        }
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

    // Multi-node repo support tests (Phase 3)

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
    fn test_discover_nodes_in_subdirs() {
        let dir = tempfile::tempdir().unwrap();

        for (name, node_type) in &[("sensor", "rust"), ("bridge", "python")] {
            let subdir = dir.path().join(name);
            std::fs::create_dir(&subdir).unwrap();
            std::fs::write(
                subdir.join("node.yaml"),
                format!("name: {}\nversion: \"0.1.0\"\ntype: {}", name, node_type),
            )
            .unwrap();
        }

        let nodes = discover_nodes_in_subdirs(dir.path());
        assert_eq!(nodes.len(), 2);
        // Sorted by name
        assert_eq!(nodes[0].0, "bridge");
        assert_eq!(nodes[0].2, "python");
        assert_eq!(nodes[1].0, "sensor");
        assert_eq!(nodes[1].2, "rust");
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

    #[test]
    fn test_discover_nodes_ignores_files() {
        let dir = tempfile::tempdir().unwrap();
        // Create a file named "node.yaml" at root (not a subdir)
        std::fs::write(dir.path().join("node.yaml"), "name: root").unwrap();
        // Create a regular file (not a directory)
        std::fs::write(dir.path().join("README.md"), "hello").unwrap();

        let nodes = discover_nodes_in_subdirs(dir.path());
        assert_eq!(nodes.len(), 0); // Only scans directories, not root
    }

    // ==================== Instance command tests ====================

    /// Test that instance suffix validation rejects invalid characters
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

    /// Test find_example_config function
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

    // ==================== Precompiled binary tests ====================

    #[test]
    fn test_detect_arch() {
        let arch = detect_arch();
        assert!(arch.is_ok());
        let arch = arch.unwrap();
        #[cfg(target_arch = "x86_64")]
        assert_eq!(arch, "amd64");
        #[cfg(target_arch = "aarch64")]
        assert_eq!(arch, "arm64");
    }

    #[test]
    fn test_precompiled_url_construction() {
        let entry = registry::RegistryNode {
            name: "system-telemetry".into(),
            version: "0.1.0".into(),
            node_type: "rust".into(),
            description: "System metrics".into(),
            category: "monitoring".into(),
            tags: vec![],
            repo: "kornia/bubbaloop-nodes-official".into(),
            subdir: "system-telemetry".into(),
            binary: Some("system_telemetry_node".into()),
        };

        let url = registry::precompiled_url(&entry, "arm64");
        assert_eq!(
            url.as_deref(),
            Some("https://github.com/kornia/bubbaloop-nodes-official/releases/latest/download/system-telemetry-linux-arm64")
        );
    }

    #[test]
    fn test_precompiled_url_python_returns_none() {
        let entry = registry::RegistryNode {
            name: "network-monitor".into(),
            version: "0.1.0".into(),
            node_type: "python".into(),
            description: "Network monitor".into(),
            category: "monitoring".into(),
            tags: vec![],
            repo: "kornia/bubbaloop-nodes-official".into(),
            subdir: "network-monitor".into(),
            binary: None,
        };

        assert!(registry::precompiled_url(&entry, "arm64").is_none());
    }

    #[test]
    fn test_verify_sha256_valid() {
        let dir = tempfile::tempdir().unwrap();
        let file_path = dir.path().join("test_binary");
        std::fs::write(&file_path, b"hello world\n").unwrap();

        // Compute expected checksum
        let output = Command::new("sha256sum").arg(&file_path).output().unwrap();
        let expected = String::from_utf8_lossy(&output.stdout).to_string();

        let result = verify_sha256(&file_path, &expected);
        assert!(result.is_ok());
    }

    #[test]
    fn test_verify_sha256_mismatch() {
        let dir = tempfile::tempdir().unwrap();
        let file_path = dir.path().join("test_binary");
        std::fs::write(&file_path, b"hello world\n").unwrap();

        let wrong_checksum =
            "0000000000000000000000000000000000000000000000000000000000000000  test_binary";
        let result = verify_sha256(&file_path, wrong_checksum);
        assert!(result.is_err());
        assert!(result
            .unwrap_err()
            .to_string()
            .contains("Checksum mismatch"));
    }

    #[test]
    fn test_set_executable() {
        use std::os::unix::fs::PermissionsExt;
        let dir = tempfile::tempdir().unwrap();
        let file_path = dir.path().join("test_bin");
        std::fs::write(&file_path, b"#!/bin/sh\necho hi").unwrap();

        set_executable(&file_path).unwrap();
        let perms = std::fs::metadata(&file_path).unwrap().permissions();
        assert_eq!(perms.mode() & 0o777, 0o755);
    }

    #[test]
    fn test_try_download_precompiled_no_binary_field() {
        let entry = registry::RegistryNode {
            name: "test".into(),
            version: "0.1.0".into(),
            node_type: "rust".into(),
            description: "test".into(),
            category: "test".into(),
            tags: vec![],
            repo: "user/repo".into(),
            subdir: "test".into(),
            binary: None,
        };
        let result = try_download_precompiled(&entry);
        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains("No binary field"));
    }

    #[test]
    fn test_try_download_precompiled_python_node() {
        let entry = registry::RegistryNode {
            name: "test".into(),
            version: "0.1.0".into(),
            node_type: "python".into(),
            description: "test".into(),
            category: "test".into(),
            tags: vec![],
            repo: "user/repo".into(),
            subdir: "test".into(),
            binary: Some("test_bin".into()),
        };
        let result = try_download_precompiled(&entry);
        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains("Not a Rust node"));
    }

    #[test]
    fn test_copy_canonical_header_proto_creates_file() {
        use tempfile::tempdir;

        let tmp = tempdir().unwrap();
        let node_path = tmp.path();
        let protos_dir = node_path.join("protos");
        std::fs::create_dir_all(&protos_dir).unwrap();

        // Call the function
        copy_canonical_header_proto(node_path);

        // Verify header.proto was created
        let header_path = protos_dir.join("header.proto");
        assert!(header_path.exists(), "header.proto should be created");

        // Verify content matches the embedded canonical version
        let written = std::fs::read(&header_path).unwrap();
        assert_eq!(
            written,
            crate::HEADER_PROTO,
            "Written header.proto should match canonical version"
        );
    }

    #[test]
    fn test_copy_canonical_header_proto_no_protos_dir() {
        use tempfile::tempdir;

        let tmp = tempdir().unwrap();
        let node_path = tmp.path();

        // Don't create protos/ dir
        copy_canonical_header_proto(node_path);

        // Should not panic, just silently skip
        let header_path = node_path.join("protos").join("header.proto");
        assert!(
            !header_path.exists(),
            "Should not create header.proto without protos/ dir"
        );
    }

    #[test]
    fn test_copy_canonical_header_proto_overwrites_existing() {
        use tempfile::tempdir;

        let tmp = tempdir().unwrap();
        let node_path = tmp.path();
        let protos_dir = node_path.join("protos");
        std::fs::create_dir_all(&protos_dir).unwrap();

        // Create an old/different header.proto
        let header_path = protos_dir.join("header.proto");
        std::fs::write(&header_path, b"old header content").unwrap();

        // Call the function
        copy_canonical_header_proto(node_path);

        // Verify it was overwritten with canonical version
        let written = std::fs::read(&header_path).unwrap();
        assert_eq!(
            written,
            crate::HEADER_PROTO,
            "Should overwrite old header.proto with canonical version"
        );
        assert_ne!(written, b"old header content");
    }
}
