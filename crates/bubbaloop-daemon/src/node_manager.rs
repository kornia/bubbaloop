//! Node manager with state caching
//!
//! Maintains authoritative state for all nodes and handles commands.

use crate::proto::{
    CommandResult, CommandType, NodeCommand, NodeEvent, NodeList, NodeState, NodeStatus,
};
use crate::registry::{self, NodeManifest};
use crate::systemd::{self, ActiveState, SystemdClient};
use std::collections::HashMap;
use std::sync::Arc;
use std::time::{SystemTime, UNIX_EPOCH};
use thiserror::Error;
use tokio::io::{AsyncBufReadExt, BufReader};
use tokio::process::Command;
use tokio::sync::{broadcast, RwLock};

#[derive(Error, Debug)]
pub enum NodeManagerError {
    #[error("Registry error: {0}")]
    Registry(#[from] crate::registry::RegistryError),

    #[error("Systemd error: {0}")]
    Systemd(#[from] crate::systemd::SystemdError),

    #[error("Node not found: {0}")]
    NodeNotFound(String),

    #[error("Build error: {0}")]
    BuildError(String),

    #[error("IO error: {0}")]
    Io(#[from] std::io::Error),
}

pub type Result<T> = std::result::Result<T, NodeManagerError>;

/// Build state for a node
#[derive(Debug, Clone)]
pub struct BuildState {
    pub status: BuildStatus,
    pub output: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum BuildStatus {
    Idle,
    Building,
    Cleaning,
}

impl Default for BuildState {
    fn default() -> Self {
        Self {
            status: BuildStatus::Idle,
            output: Vec::new(),
        }
    }
}

/// Cached node information
#[derive(Debug, Clone)]
pub struct CachedNode {
    pub path: String,
    pub manifest: Option<NodeManifest>,
    pub status: NodeStatus,
    pub installed: bool,
    pub autostart_enabled: bool,
    pub is_built: bool,
    pub build_state: BuildState,
    pub last_updated_ms: i64,
}

impl CachedNode {
    /// Convert to protobuf NodeState
    pub fn to_proto(&self) -> NodeState {
        let manifest = self.manifest.as_ref();
        NodeState {
            name: manifest
                .map(|m| m.name.clone())
                .unwrap_or_else(|| "unknown".to_string()),
            path: self.path.clone(),
            status: self.status as i32,
            installed: self.installed,
            autostart_enabled: self.autostart_enabled,
            version: manifest
                .map(|m| m.version.clone())
                .unwrap_or_else(|| "0.0.0".to_string()),
            description: manifest.map(|m| m.description.clone()).unwrap_or_default(),
            node_type: manifest
                .map(|m| m.node_type.clone())
                .unwrap_or_else(|| "unknown".to_string()),
            is_built: self.is_built,
            last_updated_ms: self.last_updated_ms,
            build_output: self.build_state.output.clone(),
        }
    }
}

/// Node manager that maintains state for all nodes
pub struct NodeManager {
    /// Cached node states
    nodes: RwLock<HashMap<String, CachedNode>>,
    /// Systemd client
    systemd: SystemdClient,
    /// Channel to broadcast state changes
    event_tx: broadcast::Sender<NodeEvent>,
}

impl NodeManager {
    /// Create a new node manager
    pub async fn new() -> Result<Arc<Self>> {
        let systemd = SystemdClient::new().await?;
        let (event_tx, _) = broadcast::channel(100);

        let manager = Arc::new(Self {
            nodes: RwLock::new(HashMap::new()),
            systemd,
            event_tx,
        });

        // Initial load
        manager.refresh_all().await?;

        Ok(manager)
    }

    /// Subscribe to node events
    pub fn subscribe(&self) -> broadcast::Receiver<NodeEvent> {
        self.event_tx.subscribe()
    }

    /// Get current timestamp in milliseconds
    fn now_ms() -> i64 {
        SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap_or_default()
            .as_millis() as i64
    }

    /// Refresh all node states from registry and systemd
    pub async fn refresh_all(&self) -> Result<()> {
        let registered = registry::list_nodes()?;
        let mut nodes = self.nodes.write().await;

        // Track which nodes we've seen
        let mut seen = std::collections::HashSet::new();

        for (path, manifest) in registered {
            seen.insert(path.clone());

            let name = manifest
                .as_ref()
                .map(|m| m.name.clone())
                .unwrap_or_else(|| "unknown".to_string());

            let service_name = systemd::get_service_name(&name);

            // Get systemd state
            let active_state = self.systemd.get_active_state(&service_name).await?;
            let installed = systemd::is_service_installed(&name);
            let autostart_enabled = if installed {
                self.systemd
                    .is_enabled(&service_name)
                    .await
                    .unwrap_or(false)
            } else {
                false
            };

            let status = if !installed {
                NodeStatus::NotInstalled
            } else {
                match active_state {
                    ActiveState::Active => NodeStatus::Running,
                    ActiveState::Failed => NodeStatus::Failed,
                    ActiveState::Inactive => NodeStatus::Stopped,
                    ActiveState::Activating => NodeStatus::Running,
                    ActiveState::Deactivating => NodeStatus::Stopped,
                    _ => NodeStatus::Stopped,
                }
            };

            let is_built = manifest
                .as_ref()
                .map(|m| registry::check_is_built(&path, m))
                .unwrap_or(false);

            // Preserve build state if exists
            let build_state = nodes
                .get(&path)
                .map(|n| n.build_state.clone())
                .unwrap_or_default();

            // If building/cleaning, override status
            let status = match build_state.status {
                BuildStatus::Building => NodeStatus::Building,
                BuildStatus::Cleaning => NodeStatus::Building,
                BuildStatus::Idle => status,
            };

            let cached = CachedNode {
                path: path.clone(),
                manifest,
                status,
                installed,
                autostart_enabled,
                is_built,
                build_state,
                last_updated_ms: Self::now_ms(),
            };

            nodes.insert(path, cached);
        }

        // Remove nodes that are no longer registered
        nodes.retain(|path, _| seen.contains(path));

        Ok(())
    }

    /// Get the current node list
    pub async fn get_node_list(&self) -> NodeList {
        let nodes = self.nodes.read().await;
        NodeList {
            nodes: nodes.values().map(|n| n.to_proto()).collect(),
            timestamp_ms: Self::now_ms(),
        }
    }

    /// Get a single node's state
    pub async fn get_node(&self, name: &str) -> Option<NodeState> {
        let nodes = self.nodes.read().await;
        nodes
            .values()
            .find(|n| n.manifest.as_ref().map(|m| m.name == name).unwrap_or(false))
            .map(|n| n.to_proto())
    }

    /// Execute a command
    pub async fn execute_command(self: &Arc<Self>, cmd: NodeCommand) -> CommandResult {
        let command_type = CommandType::try_from(cmd.command).unwrap_or(CommandType::Refresh);

        // Special handling for GET_LOGS since it returns data in output field
        if command_type == CommandType::GetLogs {
            let node_state = self.get_node(&cmd.node_name).await;
            return match self.get_logs(&cmd.node_name).await {
                Ok(logs) => CommandResult {
                    request_id: cmd.request_id,
                    success: true,
                    message: "Logs retrieved".to_string(),
                    output: logs,
                    node_state,
                },
                Err(e) => CommandResult {
                    request_id: cmd.request_id,
                    success: false,
                    message: e.to_string(),
                    output: String::new(),
                    node_state,
                },
            };
        }

        let result = match command_type {
            CommandType::Start => self.start_node(&cmd.node_name).await,
            CommandType::Stop => self.stop_node(&cmd.node_name).await,
            CommandType::Restart => self.restart_node(&cmd.node_name).await,
            CommandType::Install => self.install_node(&cmd.node_name).await,
            CommandType::Uninstall => self.uninstall_node(&cmd.node_name).await,
            CommandType::Build => self.build_node(self.clone(), &cmd.node_name).await,
            CommandType::Clean => self.clean_node(self.clone(), &cmd.node_name).await,
            CommandType::EnableAutostart => self.enable_autostart(&cmd.node_name).await,
            CommandType::DisableAutostart => self.disable_autostart(&cmd.node_name).await,
            CommandType::AddNode => self.add_node(&cmd.node_path).await,
            CommandType::RemoveNode => self.remove_node(&cmd.node_name).await,
            CommandType::Refresh => self.refresh_all().await.map(|_| "Refreshed".to_string()),
            CommandType::GetLogs => unreachable!(), // Handled above
        };

        // Get updated node state
        let node_state = self.get_node(&cmd.node_name).await;

        match result {
            Ok(msg) => CommandResult {
                request_id: cmd.request_id,
                success: true,
                message: msg,
                output: String::new(),
                node_state,
            },
            Err(e) => CommandResult {
                request_id: cmd.request_id,
                success: false,
                message: e.to_string(),
                output: String::new(),
                node_state,
            },
        }
    }

    /// Find a node by name and return its path
    async fn find_node_path(&self, name: &str) -> Result<String> {
        let nodes = self.nodes.read().await;
        nodes
            .values()
            .find(|n| n.manifest.as_ref().map(|m| m.name == name).unwrap_or(false))
            .map(|n| n.path.clone())
            .ok_or_else(|| NodeManagerError::NodeNotFound(name.to_string()))
    }

    /// Get logs for a node service
    async fn get_logs(&self, name: &str) -> Result<String> {
        // Check if node exists
        let _path = self.find_node_path(name).await?;

        let service_name = systemd::get_service_name(name);

        // Use _SYSTEMD_USER_UNIT filter for user services (logs are in system journal)
        // This works on systems where --user journal doesn't exist
        let unit_filter = format!("_SYSTEMD_USER_UNIT={}", service_name);
        let journal_output = Command::new("journalctl")
            .args([&unit_filter, "-n", "50", "--no-pager", "-o", "cat"])
            .output()
            .await?;

        let stdout = String::from_utf8_lossy(&journal_output.stdout);
        let lines: Vec<&str> = stdout.lines().collect();

        if lines.is_empty() {
            Ok("No logs available".to_string())
        } else {
            Ok(lines.join("\n"))
        }
    }

    /// Start a node
    async fn start_node(&self, name: &str) -> Result<String> {
        let service_name = systemd::get_service_name(name);
        self.systemd.start_unit(&service_name).await?;

        // Update cache
        self.refresh_all().await?;
        self.emit_event("started", name).await;

        Ok(format!("Started {}", name))
    }

    /// Stop a node
    async fn stop_node(&self, name: &str) -> Result<String> {
        let service_name = systemd::get_service_name(name);
        self.systemd.stop_unit(&service_name).await?;

        // Update cache
        self.refresh_all().await?;
        self.emit_event("stopped", name).await;

        Ok(format!("Stopped {}", name))
    }

    /// Restart a node
    async fn restart_node(&self, name: &str) -> Result<String> {
        let service_name = systemd::get_service_name(name);
        self.systemd.restart_unit(&service_name).await?;

        // Update cache
        self.refresh_all().await?;
        self.emit_event("restarted", name).await;

        Ok(format!("Restarted {}", name))
    }

    /// Install a node's systemd service
    async fn install_node(&self, name: &str) -> Result<String> {
        let path = self.find_node_path(name).await?;
        let nodes = self.nodes.read().await;
        let node = nodes
            .get(&path)
            .ok_or_else(|| NodeManagerError::NodeNotFound(name.to_string()))?;

        let manifest = node
            .manifest
            .as_ref()
            .ok_or_else(|| NodeManagerError::NodeNotFound(name.to_string()))?;

        systemd::install_service(
            &path,
            name,
            &manifest.node_type,
            manifest.command.as_deref(),
        )
        .await?;

        drop(nodes);

        // Update cache
        self.refresh_all().await?;
        self.emit_event("installed", name).await;

        Ok(format!("Installed {}", name))
    }

    /// Uninstall a node's systemd service
    async fn uninstall_node(&self, name: &str) -> Result<String> {
        systemd::uninstall_service(name).await?;

        // Update cache
        self.refresh_all().await?;
        self.emit_event("uninstalled", name).await;

        Ok(format!("Uninstalled {}", name))
    }

    /// Build a node
    async fn build_node(self: &Arc<Self>, manager: Arc<Self>, name: &str) -> Result<String> {
        let path = self.find_node_path(name).await?;

        // Get build command
        let build_cmd = {
            let nodes = self.nodes.read().await;
            let node = nodes
                .get(&path)
                .ok_or_else(|| NodeManagerError::NodeNotFound(name.to_string()))?;
            node.manifest
                .as_ref()
                .and_then(|m| m.build.clone())
                .ok_or_else(|| {
                    NodeManagerError::BuildError("No build command defined".to_string())
                })?
        };

        // Update status to building
        {
            let mut nodes = self.nodes.write().await;
            if let Some(node) = nodes.get_mut(&path) {
                node.build_state.status = BuildStatus::Building;
                node.build_state.output.clear();
                node.status = NodeStatus::Building;
            }
        }

        self.emit_event("building", name).await;

        // Spawn build process
        let name_clone = name.to_string();
        let path_clone = path.clone();

        tokio::spawn(async move {
            let result = run_build_command(&manager, &path_clone, &build_cmd).await;

            // Update status based on result
            let mut nodes = manager.nodes.write().await;
            if let Some(node) = nodes.get_mut(&path_clone) {
                node.build_state.status = BuildStatus::Idle;

                match result {
                    Ok(_) => {
                        node.build_state
                            .output
                            .push("--- Build completed successfully ---".to_string());
                        node.is_built = true;
                    }
                    Err(e) => {
                        node.build_state
                            .output
                            .push(format!("--- Build failed: {} ---", e));
                    }
                }
            }
            drop(nodes);

            // Refresh to get correct status
            let _ = manager.refresh_all().await;
            manager.emit_event("build_complete", &name_clone).await;
        });

        Ok(format!("Building {} (background)", name))
    }

    /// Clean a node
    async fn clean_node(self: &Arc<Self>, manager: Arc<Self>, name: &str) -> Result<String> {
        let path = self.find_node_path(name).await?;

        // Update status to cleaning
        {
            let mut nodes = self.nodes.write().await;
            if let Some(node) = nodes.get_mut(&path) {
                node.build_state.status = BuildStatus::Cleaning;
                node.build_state.output.clear();
                node.status = NodeStatus::Building;
            }
        }

        self.emit_event("cleaning", name).await;

        // Spawn clean process
        let name_clone = name.to_string();
        let path_clone = path.clone();

        tokio::spawn(async move {
            let result = run_build_command(&manager, &path_clone, "pixi run clean").await;

            // Update status
            let mut nodes = manager.nodes.write().await;
            if let Some(node) = nodes.get_mut(&path_clone) {
                node.build_state.status = BuildStatus::Idle;
                node.is_built = false;

                match result {
                    Ok(_) => {
                        node.build_state
                            .output
                            .push("--- Clean completed ---".to_string());
                    }
                    Err(e) => {
                        node.build_state
                            .output
                            .push(format!("--- Clean failed: {} ---", e));
                    }
                }
            }
            drop(nodes);

            let _ = manager.refresh_all().await;
            manager.emit_event("clean_complete", &name_clone).await;
        });

        Ok(format!("Cleaning {} (background)", name))
    }

    /// Enable autostart for a node
    async fn enable_autostart(&self, name: &str) -> Result<String> {
        let service_name = systemd::get_service_name(name);
        self.systemd.enable_unit(&service_name).await?;

        self.refresh_all().await?;
        self.emit_event("autostart_enabled", name).await;

        Ok(format!("Enabled autostart for {}", name))
    }

    /// Disable autostart for a node
    async fn disable_autostart(&self, name: &str) -> Result<String> {
        let service_name = systemd::get_service_name(name);
        self.systemd.disable_unit(&service_name).await?;

        self.refresh_all().await?;
        self.emit_event("autostart_disabled", name).await;

        Ok(format!("Disabled autostart for {}", name))
    }

    /// Add a node to the registry
    async fn add_node(&self, path: &str) -> Result<String> {
        let manifest = registry::register_node(path)?;

        self.refresh_all().await?;
        self.emit_event("added", &manifest.name).await;

        Ok(format!("Added node: {}", manifest.name))
    }

    /// Remove a node from the registry
    async fn remove_node(&self, name: &str) -> Result<String> {
        let path = self.find_node_path(name).await?;
        registry::unregister_node(&path)?;

        self.refresh_all().await?;
        self.emit_event("removed", name).await;

        Ok(format!("Removed node: {}", name))
    }

    /// Emit a node event
    async fn emit_event(&self, event_type: &str, node_name: &str) {
        if let Some(state) = self.get_node(node_name).await {
            let event = NodeEvent {
                event_type: event_type.to_string(),
                node_name: node_name.to_string(),
                state: Some(state),
                timestamp_ms: Self::now_ms(),
            };

            // Ignore send errors (no subscribers)
            let _ = self.event_tx.send(event);
        }
    }
}

/// Run a build/clean command and stream output to the node's build state
async fn run_build_command(manager: &Arc<NodeManager>, path: &str, cmd: &str) -> Result<()> {
    let mut child = Command::new("sh")
        .args(["-c", cmd])
        .current_dir(path)
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::piped())
        .spawn()?;

    let stdout = child.stdout.take();
    let stderr = child.stderr.take();

    // Read stdout
    if let Some(stdout) = stdout {
        let reader = BufReader::new(stdout);
        let mut lines = reader.lines();
        let manager = manager.clone();
        let path = path.to_string();

        tokio::spawn(async move {
            while let Ok(Some(line)) = lines.next_line().await {
                let mut nodes = manager.nodes.write().await;
                if let Some(node) = nodes.get_mut(&path) {
                    node.build_state.output.push(line);
                    // Keep only last 100 lines
                    if node.build_state.output.len() > 100 {
                        node.build_state.output.remove(0);
                    }
                }
            }
        });
    }

    // Read stderr
    if let Some(stderr) = stderr {
        let reader = BufReader::new(stderr);
        let mut lines = reader.lines();
        let manager = manager.clone();
        let path = path.to_string();

        tokio::spawn(async move {
            while let Ok(Some(line)) = lines.next_line().await {
                let mut nodes = manager.nodes.write().await;
                if let Some(node) = nodes.get_mut(&path) {
                    node.build_state.output.push(line);
                    if node.build_state.output.len() > 100 {
                        node.build_state.output.remove(0);
                    }
                }
            }
        });
    }

    // Wait for process to complete
    let status = child.wait().await?;

    if status.success() {
        Ok(())
    } else {
        Err(NodeManagerError::BuildError(format!(
            "Command exited with code {}",
            status.code().unwrap_or(-1)
        )))
    }
}
