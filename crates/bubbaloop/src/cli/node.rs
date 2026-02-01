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

    /// skip the build step (marketplace install only)
    #[argh(switch)]
    no_build: bool,
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
        eprintln!("  search      Search the node marketplace");
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
        eprintln!("\nRun 'bubbaloop node <command> --help' for more information.");
    }
}

async fn get_zenoh_session() -> Result<zenoh::Session> {
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
    // Determine output directory (default: ./<name> in current directory)
    let output_dir = if let Some(output) = args.output {
        PathBuf::from(output)
    } else {
        PathBuf::from(".").join(&args.name)
    };

    // Use shared template module
    let output_dir = templates::create_node_at(
        &args.name,
        &args.node_type,
        &args.author,
        &args.description,
        &output_dir,
    )
    .map_err(|e| NodeError::CommandFailed(e.to_string()))?;

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
    let session = get_zenoh_session().await?;

    // Retry up to 3 times with 1 second delay between retries
    let mut last_error = None;
    let mut best_data: Option<NodeListResponse> = None;

    for attempt in 1..=3 {
        let replies_result = session
            .get("bubbaloop/daemon/api/nodes")
            .target(QueryTarget::BestMatching)
            .timeout(std::time::Duration::from_secs(30))
            .await;

        match replies_result {
            Ok(replies) => {
                let replies: Vec<_> = replies.into_iter().collect();

                // Find the first response with actual nodes, or use any response
                for reply in replies {
                    if let Ok(sample) = reply.into_result() {
                        let payload = sample.payload().to_bytes();
                        if let Ok(data) = serde_json::from_slice::<NodeListResponse>(&payload) {
                            if !data.nodes.is_empty() {
                                best_data = Some(data);
                                break; // Found response with nodes, use it
                            } else if best_data.is_none() {
                                best_data = Some(data); // Keep first empty response as fallback
                            }
                        }
                    }
                }

                if best_data.is_some() {
                    break; // Success, exit retry loop
                }

                // No valid response, retry
                if attempt < 3 {
                    tokio::time::sleep(std::time::Duration::from_secs(1)).await;
                }
            }
            Err(e) => {
                last_error = Some(NodeError::Zenoh(e.to_string()));
                if attempt < 3 {
                    tokio::time::sleep(std::time::Duration::from_secs(1)).await;
                }
            }
        }
    }

    if let Some(data) = best_data {
        if args.format == "json" {
            println!("{}", serde_json::to_string_pretty(&data.nodes)?);
        } else if data.nodes.is_empty() {
            println!("No nodes registered. Use 'bubbaloop node add <path>' to add one.");
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
    } else if let Some(err) = last_error {
        return Err(err);
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
        "node_path": node_path
    }))?;

    // Retry up to 3 times with 1 second delay between retries
    let mut node_name: Option<String> = None;
    let mut last_error = None;

    for attempt in 1..=3 {
        let replies_result = session
            .get("bubbaloop/daemon/api/nodes/add")
            .payload(payload.clone())
            .target(QueryTarget::BestMatching)
            .timeout(std::time::Duration::from_secs(30))
            .await;

        match replies_result {
            Ok(replies) => {
                let replies: Vec<_> = replies.into_iter().collect();
                let mut success = false;

                for reply in replies {
                    if let Ok(sample) = reply.into_result() {
                        let data: CommandResponse =
                            serde_json::from_slice(&sample.payload().to_bytes())?;
                        if data.success {
                            println!("Added node from: {}", node_path);
                            // Extract node name from path for build/install
                            node_name = extract_node_name(&node_path).ok();
                            success = true;
                            break;
                        } else {
                            last_error = Some(NodeError::CommandFailed(data.message));
                        }
                    }
                }

                if success {
                    break; // Success, exit retry loop
                }

                // Failed, retry
                if attempt < 3 {
                    tokio::time::sleep(std::time::Duration::from_secs(1)).await;
                }
            }
            Err(e) => {
                last_error = Some(NodeError::Zenoh(e.to_string()));
                if attempt < 3 {
                    tokio::time::sleep(std::time::Duration::from_secs(1)).await;
                }
            }
        }
    }

    // Check if we failed after all retries
    if node_name.is_none() {
        if let Some(err) = last_error {
            session
                .close()
                .await
                .map_err(|e| NodeError::Zenoh(e.to_string()))?;
            return Err(err);
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
        std::path::PathBuf::from(out)
    } else {
        let home = std::env::var("HOME").unwrap_or_else(|_| "/tmp".to_string());
        std::path::PathBuf::from(home)
            .join(".bubbaloop")
            .join("nodes")
            .join(repo_name)
    };

    if target_dir.exists() {
        return Err(NodeError::GitClone(format!(
            "Directory already exists: {}",
            target_dir.display()
        )));
    }

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

async fn send_command(name: &str, command: &str) -> Result<()> {
    let session = get_zenoh_session().await?;

    let payload = serde_json::to_string(&serde_json::json!({
        "command": command
    }))?;

    let key = format!("bubbaloop/daemon/api/nodes/{}/command", name);

    // Retry up to 3 times with 1 second delay between retries
    let mut last_error = None;
    let mut success = false;

    for attempt in 1..=3 {
        let replies_result = session
            .get(&key)
            .payload(payload.clone())
            .target(QueryTarget::BestMatching)
            .timeout(std::time::Duration::from_secs(30))
            .await;

        match replies_result {
            Ok(replies) => {
                let replies: Vec<_> = replies.into_iter().collect();

                for reply in replies {
                    if let Ok(sample) = reply.into_result() {
                        let data: CommandResponse =
                            serde_json::from_slice(&sample.payload().to_bytes())?;
                        if data.success {
                            println!("{}", data.message);
                            if !data.output.is_empty() {
                                println!("{}", data.output);
                            }
                            success = true;
                            break;
                        } else {
                            last_error = Some(NodeError::CommandFailed(data.message));
                        }
                    }
                }

                if success {
                    break; // Success, exit retry loop
                }

                // Failed, retry
                if attempt < 3 {
                    tokio::time::sleep(std::time::Duration::from_secs(1)).await;
                }
            }
            Err(e) => {
                last_error = Some(NodeError::Zenoh(e.to_string()));
                if attempt < 3 {
                    tokio::time::sleep(std::time::Duration::from_secs(1)).await;
                }
            }
        }
    }

    session
        .close()
        .await
        .map_err(|e| NodeError::Zenoh(e.to_string()))?;

    if !success {
        if let Some(err) = last_error {
            return Err(err);
        }
    }

    Ok(())
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
        // Node is registered -> standard systemd install
        return send_command(&args.name, "install").await;
    }

    // Not registered -> try marketplace lookup
    println!(
        "Node '{}' not registered. Checking marketplace...",
        args.name
    );

    // Try refreshing the cache first, fall back to existing cache
    let _ = registry::refresh_cache();
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

    println!("Found '{}' in marketplace ({})", entry.name, entry.repo);

    // Validate repo before constructing URL
    registry::validate_repo(&entry.repo)
        .map_err(|e| NodeError::InvalidUrl(format!("Invalid registry repo: {}", e)))?;

    // Clone from GitHub
    let url = format!("https://github.com/{}", entry.repo);
    let base_path = clone_from_github(&url, None, &args.branch)?;

    // Resolve subdir
    let node_path = resolve_node_path(&base_path, Some(&entry.subdir))?;

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

    // Build (unless --no-build)
    if !args.no_build {
        println!("Building {}...", args.name);
        send_command(&args.name, "build").await?;
    }

    // Install as systemd service
    println!("Installing {} as systemd service...", args.name);
    send_command(&args.name, "install").await?;

    println!("\nInstalled '{}' from {}", args.name, entry.repo);

    Ok(())
}

fn search_nodes(args: SearchArgs) -> Result<()> {
    println!("Refreshing marketplace registry...");
    let _ = registry::refresh_cache();
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
        "{:<20} {:<10} {:<8} {:<12} DESCRIPTION",
        "NAME", "VERSION", "TYPE", "CATEGORY"
    );
    println!("{}", "-".repeat(78));
    for node in &results {
        println!(
            "{:<20} {:<10} {:<8} {:<12} {}",
            node.name,
            node.version,
            node.node_type,
            node.category,
            truncate(&node.description, 30)
        );
    }
    println!();
    println!("Install with: bubbaloop node install <name>");

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
    fn test_discover_nodes_ignores_files() {
        let dir = tempfile::tempdir().unwrap();
        // Create a file named "node.yaml" at root (not a subdir)
        std::fs::write(dir.path().join("node.yaml"), "name: root").unwrap();
        // Create a regular file (not a directory)
        std::fs::write(dir.path().join("README.md"), "hello").unwrap();

        let nodes = discover_nodes_in_subdirs(dir.path());
        assert_eq!(nodes.len(), 0); // Only scans directories, not root
    }
}
