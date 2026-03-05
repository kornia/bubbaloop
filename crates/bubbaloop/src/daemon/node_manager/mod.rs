//! Node manager with state caching
//!
//! Maintains authoritative state for all nodes and handles commands.
//! Split into submodules: build, health, lifecycle.

pub mod build;
pub mod health;
pub mod lifecycle;

use crate::daemon::registry::{self, NodeManifest};
use crate::daemon::systemd::{self, ActiveState, SystemdClient, SystemdSignalEvent};
use crate::schemas::daemon::v1::{
    CommandResult, CommandType, HealthStatus, NodeCommand, NodeEvent, NodeList, NodeState,
    NodeStatus,
};
use std::collections::{HashMap, HashSet};
use std::sync::Arc;
use std::time::{Duration, SystemTime, UNIX_EPOCH};
use thiserror::Error;
use tokio::sync::{broadcast, Mutex, RwLock};

pub use build::{BuildState, BuildStatus};

/// Absolute path to journalctl — never rely on PATH for system binaries.
pub(crate) const JOURNALCTL_PATH: &str = "/usr/bin/journalctl";

#[derive(Error, Debug)]
pub enum NodeManagerError {
    #[error("Registry error: {0}")]
    Registry(#[from] crate::daemon::registry::RegistryError),

    #[error("Systemd error: {0}")]
    Systemd(#[from] crate::daemon::systemd::SystemdError),

    #[error("Node not found: {0}")]
    NodeNotFound(String),

    #[error("Build error: {0}")]
    BuildError(String),

    #[error("Build already in progress for: {0}")]
    AlreadyBuilding(String),

    #[error("Build timed out for: {0}")]
    BuildTimeout(String),

    #[error("IO error: {0}")]
    Io(#[from] std::io::Error),
}

pub type Result<T> = std::result::Result<T, NodeManagerError>;

/// Get all non-loopback IP addresses of this machine
fn get_machine_ips() -> Vec<String> {
    // Use `hostname -I` which returns all IPs (works on Linux)
    if let Ok(output) = std::process::Command::new("hostname").arg("-I").output() {
        if output.status.success() {
            return String::from_utf8_lossy(&output.stdout)
                .split_whitespace()
                .map(String::from)
                .collect();
        }
    }
    Vec::new()
}

/// Convert a systemd ActiveState to a NodeStatus.
///
/// Extracted as a helper to deduplicate the mapping in `refresh_node` and `refresh_all`.
fn active_state_to_node_status(active_state: ActiveState, installed: bool) -> NodeStatus {
    if !installed {
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
    /// Health status based on heartbeat monitoring
    pub health_status: HealthStatus,
    /// Last heartbeat timestamp (milliseconds since epoch)
    pub last_health_check_ms: i64,
    /// Instance name override (for multi-instance nodes)
    pub name_override: Option<String>,
    /// Config file path override (for multi-instance nodes)
    pub config_override: Option<String>,
}

impl CachedNode {
    /// Get the effective name for this node (name_override or manifest name)
    pub fn effective_name(&self) -> String {
        if let Some(ref ov) = self.name_override {
            return ov.clone();
        }
        self.manifest
            .as_ref()
            .map(|m| m.name.clone())
            .unwrap_or_else(|| "unknown".to_string())
    }

    /// Convert to protobuf NodeState (requires machine info from caller)
    pub fn to_proto(
        &self,
        machine_id: &str,
        machine_hostname: &str,
        machine_ips: &[String],
    ) -> NodeState {
        let manifest = self.manifest.as_ref();
        let base_name = manifest.map(|m| m.name.clone()).unwrap_or_default();
        NodeState {
            name: self.effective_name(),
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
            health_status: self.health_status as i32,
            last_health_check_ms: self.last_health_check_ms,
            machine_id: machine_id.to_string(),
            machine_hostname: machine_hostname.to_string(),
            machine_ips: machine_ips.to_vec(),
            base_node: if self.name_override.is_some() && !base_name.is_empty() {
                base_name
            } else {
                String::new()
            },
            config_override: self.config_override.clone().unwrap_or_default(),
        }
    }
}

/// Node manager that maintains state for all nodes
pub struct NodeManager {
    /// Cached node states
    pub(crate) nodes: RwLock<HashMap<String, CachedNode>>,
    /// Systemd client
    pub(crate) systemd: SystemdClient,
    /// Channel to broadcast state changes
    pub(crate) event_tx: broadcast::Sender<NodeEvent>,
    /// Nodes currently being built (prevents concurrent builds)
    pub(crate) building_nodes: Mutex<HashSet<String>>,
    /// Machine identifier
    pub(crate) machine_id: String,
    /// Machine hostname
    pub(crate) machine_hostname: String,
    /// Machine IP addresses
    pub(crate) machine_ips: Vec<String>,
}

impl NodeManager {
    /// Create a new node manager
    pub async fn new() -> Result<Arc<Self>> {
        let systemd = SystemdClient::new().await?;
        let (event_tx, _) = broadcast::channel(100);

        let machine_id = super::util::get_machine_id();

        // Get machine hostname
        let machine_hostname = hostname::get()
            .map(|h| h.to_string_lossy().to_string())
            .unwrap_or_else(|_| "unknown".to_string());

        let machine_ips = get_machine_ips();

        log::info!(
            "NodeManager using machine_id: {}, hostname: {}, ips: {:?}",
            machine_id,
            machine_hostname,
            machine_ips
        );

        let manager = Arc::new(Self {
            nodes: RwLock::new(HashMap::new()),
            systemd,
            event_tx,
            building_nodes: Mutex::new(HashSet::new()),
            machine_id,
            machine_hostname,
            machine_ips,
        });

        // Initial load
        manager.refresh_all().await?;

        Ok(manager)
    }

    /// Subscribe to node events
    pub fn subscribe(&self) -> broadcast::Receiver<NodeEvent> {
        self.event_tx.subscribe()
    }

    /// Start listening to systemd D-Bus signals for real-time updates.
    ///
    /// IMPORTANT: The signal handler must NOT make D-Bus calls (no `refresh_node`).
    /// Doing so causes a deadlock: signal backpressure fills zbus internal buffers,
    /// which blocks the D-Bus message router, which prevents method-call replies
    /// from being dispatched, which hangs the D-Bus calls in the signal handler.
    ///
    /// Instead, we collect dirty nodes and schedule a debounced refresh on a
    /// separate task that runs after a short delay, coalescing rapid signal bursts.
    pub async fn start_signal_listener(self: Arc<Self>) -> Result<()> {
        let mut signal_rx = self.systemd.subscribe_to_signals().await?;

        tokio::spawn(async move {
            log::info!("Signal listener started");
            // Debounce: collect signals for up to 500ms, then refresh once
            let debounce = Duration::from_millis(500);

            loop {
                // Wait for at least one signal
                let first = match signal_rx.recv().await {
                    Some(ev) => ev,
                    None => {
                        log::warn!("Signal listener ended (channel closed)");
                        break;
                    }
                };

                // Collect node names and event types from this burst
                let mut pending: Vec<(String, String)> = Vec::new();
                Self::push_signal_event(&first, &mut pending);

                // Drain any additional signals within the debounce window
                let deadline = tokio::time::Instant::now() + debounce;
                while let Ok(Some(ev)) = tokio::time::timeout_at(deadline, signal_rx.recv()).await {
                    Self::push_signal_event(&ev, &mut pending);
                }

                // Deduplicate: only refresh each node once per burst
                pending.sort();
                pending.dedup();

                log::debug!(
                    "Signal burst: {} events for {} unique nodes",
                    pending.len(),
                    pending
                        .iter()
                        .map(|(n, _)| n.as_str())
                        .collect::<std::collections::HashSet<_>>()
                        .len()
                );

                // Refresh on a separate spawned task so we don't block signal
                // reception. The spawned task uses its own D-Bus calls, which
                // won't deadlock because we've already drained the signal burst.
                let manager = Arc::clone(&self);
                tokio::spawn(async move {
                    for (name, event_type) in pending {
                        if let Err(e) = manager.refresh_node(&name).await {
                            log::warn!("Failed to refresh node {} after signal: {}", name, e);
                        }
                        manager.emit_event(&event_type, &name).await;
                    }
                });
            }
        });

        Ok(())
    }

    /// Extract node name and event type from a signal, pushing to the batch if relevant.
    fn push_signal_event(event: &SystemdSignalEvent, batch: &mut Vec<(String, String)>) {
        let (node_name, event_type) = match event {
            SystemdSignalEvent::JobRemoved {
                node_name, result, ..
            } => {
                let event_type = match result.as_str() {
                    "done" => "state_changed",
                    "failed" => "failed",
                    "canceled" => "stopped",
                    _ => "state_changed",
                };
                (node_name.clone(), event_type)
            }
            SystemdSignalEvent::UnitNew { node_name, .. } => (node_name.clone(), "installed"),
            SystemdSignalEvent::UnitRemoved { node_name, .. } => (node_name.clone(), "uninstalled"),
        };

        if let Some(name) = node_name {
            batch.push((name, event_type.to_string()));
        }
    }

    /// Refresh a single node's state
    pub async fn refresh_node(&self, name: &str) -> Result<()> {
        let service_name = systemd::get_service_name(name);

        // Get systemd state
        let active_state = self.systemd.get_active_state(&service_name).await?;
        let installed = systemd::is_service_installed(name);
        let autostart_enabled = if installed {
            self.systemd
                .is_enabled(&service_name)
                .await
                .unwrap_or(false)
        } else {
            false
        };

        let status = active_state_to_node_status(active_state, installed);

        // Update the node in our cache
        let mut nodes = self.nodes.write().await;
        for node in nodes.values_mut() {
            if node.effective_name() == name {
                // Preserve build state status override
                node.status = if matches!(
                    node.build_state.status,
                    BuildStatus::Building | BuildStatus::Cleaning
                ) {
                    NodeStatus::Building
                } else {
                    status
                };
                node.installed = installed;
                node.autostart_enabled = autostart_enabled;
                node.last_updated_ms = Self::now_ms();
                break;
            }
        }

        Ok(())
    }

    /// Get current timestamp in milliseconds
    pub(crate) fn now_ms() -> i64 {
        SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap_or_default()
            .as_millis() as i64
    }

    /// Refresh all node states from registry and systemd
    pub async fn refresh_all(&self) -> Result<()> {
        let registered = registry::list_nodes()?;
        let mut nodes = self.nodes.write().await;

        // Track which keys we've seen (keyed by effective_name)
        let mut seen = std::collections::HashSet::new();

        for (entry, manifest) in registered {
            // Compute effective name: name_override if present, otherwise manifest name
            let eff_name = manifest
                .as_ref()
                .map(|m| registry::effective_name(&entry, m))
                .unwrap_or_else(|| {
                    entry
                        .name_override
                        .clone()
                        .unwrap_or_else(|| "unknown".to_string())
                });

            // Use effective name as the HashMap key so each instance is distinct
            let key = eff_name.clone();
            seen.insert(key.clone());

            let service_name = systemd::get_service_name(&eff_name);

            // Get systemd state
            let active_state = self.systemd.get_active_state(&service_name).await?;
            let installed = systemd::is_service_installed(&eff_name);
            let autostart_enabled = if installed {
                self.systemd
                    .is_enabled(&service_name)
                    .await
                    .unwrap_or(false)
            } else {
                false
            };

            let status = active_state_to_node_status(active_state, installed);

            let is_built = manifest
                .as_ref()
                .map(|m| registry::check_is_built(&entry.path, m))
                .unwrap_or(false);

            // Preserve build state and health state if exists
            let (build_state, health_status, last_health_check_ms) = nodes
                .get(&key)
                .map(|n| {
                    (
                        n.build_state.clone(),
                        n.health_status,
                        n.last_health_check_ms,
                    )
                })
                .unwrap_or((BuildState::default(), HealthStatus::Unknown, 0));

            // If building/cleaning, override status
            let status = if matches!(
                build_state.status,
                BuildStatus::Building | BuildStatus::Cleaning
            ) {
                NodeStatus::Building
            } else {
                status
            };

            let cached = CachedNode {
                path: entry.path.clone(),
                manifest,
                status,
                installed,
                autostart_enabled,
                is_built,
                build_state,
                last_updated_ms: Self::now_ms(),
                health_status,
                last_health_check_ms,
                name_override: entry.name_override.clone(),
                config_override: entry.config_override.clone(),
            };

            nodes.insert(key, cached);
        }

        // Remove nodes that are no longer registered
        nodes.retain(|key, _| seen.contains(key));

        Ok(())
    }

    /// Get the current node list
    pub async fn get_node_list(&self) -> NodeList {
        let nodes = self.nodes.read().await;
        log::debug!("get_node_list: nodes HashMap has {} entries", nodes.len());
        NodeList {
            nodes: nodes
                .values()
                .map(|n| n.to_proto(&self.machine_id, &self.machine_hostname, &self.machine_ips))
                .collect(),
            timestamp_ms: Self::now_ms(),
            machine_id: self.machine_id.clone(),
        }
    }

    /// Get cached manifests from all registered nodes.
    ///
    /// Returns `(effective_name, NodeManifest)` for every node that has a manifest.
    /// Uses the static manifests loaded from `node.yaml` at registration time.
    pub async fn get_cached_manifests(&self) -> Vec<(String, NodeManifest)> {
        let nodes = self.nodes.read().await;
        nodes
            .values()
            .filter_map(|n| n.manifest.as_ref().map(|m| (n.effective_name(), m.clone())))
            .collect()
    }

    /// Get a single node's state
    pub async fn get_node(&self, name: &str) -> Option<NodeState> {
        let nodes = self.nodes.read().await;
        nodes
            .values()
            .find(|n| n.effective_name() == name)
            .map(|n| n.to_proto(&self.machine_id, &self.machine_hostname, &self.machine_ips))
    }

    /// Execute a command
    pub async fn execute_command(self: &Arc<Self>, cmd: NodeCommand) -> CommandResult {
        let command_type = CommandType::try_from(cmd.command).unwrap_or(CommandType::Refresh);
        log::debug!(
            "execute_command: type={:?} node={}",
            command_type,
            cmd.node_name
        );

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
                    timestamp_ms: Self::now_ms(),
                    responding_machine: self.machine_id.clone(),
                },
                Err(e) => CommandResult {
                    request_id: cmd.request_id,
                    success: false,
                    message: e.to_string(),
                    output: String::new(),
                    node_state,
                    timestamp_ms: Self::now_ms(),
                    responding_machine: self.machine_id.clone(),
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
            CommandType::AddNode => {
                let name_ov = if cmd.name_override.is_empty() {
                    None
                } else {
                    Some(cmd.name_override.as_str())
                };
                let config_ov = if cmd.config_override.is_empty() {
                    None
                } else {
                    Some(cmd.config_override.as_str())
                };
                self.add_node(&cmd.node_path, name_ov, config_ov).await
            }
            CommandType::RemoveNode => self.remove_node(&cmd.node_name).await,
            CommandType::Refresh => self.refresh_all().await.map(|_| "Refreshed".to_string()),
            CommandType::GetLogs => Ok("Logs handled above".to_string()),
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
                timestamp_ms: Self::now_ms(),
                responding_machine: self.machine_id.clone(),
            },
            Err(e) => CommandResult {
                request_id: cmd.request_id,
                success: false,
                message: e.to_string(),
                output: String::new(),
                node_state,
                timestamp_ms: Self::now_ms(),
                responding_machine: self.machine_id.clone(),
            },
        }
    }

    /// Find a node by effective name and return its path
    pub(crate) async fn find_node_path(&self, name: &str) -> Result<String> {
        let nodes = self.nodes.read().await;
        nodes
            .values()
            .find(|n| n.effective_name() == name)
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
        let journal_output = tokio::process::Command::new(JOURNALCTL_PATH)
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

    /// Refresh cache and emit an event in the background (non-blocking).
    /// Used by start/stop/restart/install/uninstall to avoid blocking the reply.
    pub(crate) fn spawn_refresh_and_emit(self: &Arc<Self>, event_type: &str, name: &str) {
        let manager = Arc::clone(self);
        let event_type = event_type.to_string();
        let node_name = name.to_string();
        tokio::spawn(async move {
            if let Err(e) = manager.refresh_all().await {
                log::warn!("Failed to refresh after {}: {}", event_type, e);
            }
            manager.emit_event(&event_type, &node_name).await;
        });
    }

    /// Emit a node event
    pub(crate) async fn emit_event(&self, event_type: &str, node_name: &str) {
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

#[cfg(test)]
mod tests {
    use super::*;
    use prost::Message;

    #[test]
    fn test_cached_node_base_node_with_override() {
        let node = CachedNode {
            path: "/path/to/rtsp-camera".to_string(),
            manifest: Some(NodeManifest {
                name: "rtsp-camera".to_string(),
                version: "0.1.0".to_string(),
                description: "RTSP camera node".to_string(),
                node_type: "rust".to_string(),
                ..Default::default()
            }),
            status: NodeStatus::Stopped,
            installed: false,
            autostart_enabled: false,
            is_built: false,
            build_state: BuildState::default(),
            last_updated_ms: 0,
            health_status: HealthStatus::Unknown,
            last_health_check_ms: 0,
            name_override: Some("rtsp-camera-terrace".to_string()),
            config_override: None,
        };

        let proto = node.to_proto("machine1", "host1", &[]);
        assert_eq!(proto.name, "rtsp-camera-terrace");
        assert_eq!(proto.base_node, "rtsp-camera");
    }

    #[test]
    fn test_cached_node_base_node_without_override() {
        let node = CachedNode {
            path: "/path/to/openmeteo".to_string(),
            manifest: Some(NodeManifest {
                name: "openmeteo".to_string(),
                version: "0.1.0".to_string(),
                description: "Weather node".to_string(),
                node_type: "rust".to_string(),
                ..Default::default()
            }),
            status: NodeStatus::Running,
            installed: true,
            autostart_enabled: false,
            is_built: true,
            build_state: BuildState::default(),
            last_updated_ms: 0,
            health_status: HealthStatus::Unknown,
            last_health_check_ms: 0,
            name_override: None,
            config_override: None,
        };

        let proto = node.to_proto("machine1", "host1", &[]);
        assert_eq!(proto.name, "openmeteo");
        assert_eq!(proto.base_node, "");
    }

    /// Simulate registering the same rtsp-camera node 3 times with different
    /// name_overrides (terrace, garage, entrance) and verify the full proto
    /// output for each instance plus a plain node without override.
    #[test]
    fn test_multi_instance_cameras_base_node_tracking() {
        let camera_manifest = NodeManifest {
            name: "rtsp-camera".to_string(),
            version: "0.2.0".to_string(),
            description: "RTSP camera node".to_string(),
            node_type: "rust".to_string(),
            build: Some("cargo build --release".to_string()),
            command: Some("./target/release/cameras_node".to_string()),
            ..Default::default()
        };

        let instances = vec![
            (
                "rtsp-camera-terrace",
                Some("rtsp-camera-terrace"),
                Some("/etc/bubbaloop/terrace.yaml"),
            ),
            (
                "rtsp-camera-garage",
                Some("rtsp-camera-garage"),
                Some("/etc/bubbaloop/garage.yaml"),
            ),
            ("rtsp-camera-entrance", Some("rtsp-camera-entrance"), None),
        ];

        let mut protos = Vec::new();

        for (expected_name, name_override, config_override) in &instances {
            let node = CachedNode {
                path: "/opt/nodes/rtsp-camera".to_string(),
                manifest: Some(camera_manifest.clone()),
                status: NodeStatus::Running,
                installed: true,
                autostart_enabled: true,
                is_built: true,
                build_state: BuildState::default(),
                last_updated_ms: 1700000000000,
                health_status: HealthStatus::Healthy,
                last_health_check_ms: 1700000000000,
                name_override: name_override.map(|s| s.to_string()),
                config_override: config_override.map(|s| s.to_string()),
            };

            assert_eq!(node.effective_name(), *expected_name);

            let proto = node.to_proto("jetson_1", "jetson-1.local", &["10.0.0.42".to_string()]);
            protos.push(proto);
        }

        // All instances should report "rtsp-camera" as their base_node
        for (i, proto) in protos.iter().enumerate() {
            let (expected_name, _, _) = &instances[i];
            assert_eq!(proto.name, *expected_name, "instance {} name mismatch", i);
            assert_eq!(
                proto.base_node, "rtsp-camera",
                "instance {} should have base_node='rtsp-camera'",
                i
            );
            assert_eq!(proto.version, "0.2.0");
            assert_eq!(proto.machine_id, "jetson_1");
        }

        // Now verify a plain node (no override) has empty base_node
        let plain_node = CachedNode {
            path: "/opt/nodes/openmeteo".to_string(),
            manifest: Some(NodeManifest {
                name: "openmeteo".to_string(),
                version: "0.1.0".to_string(),
                description: "Weather".to_string(),
                node_type: "rust".to_string(),
                ..Default::default()
            }),
            status: NodeStatus::Running,
            installed: true,
            autostart_enabled: false,
            is_built: true,
            build_state: BuildState::default(),
            last_updated_ms: 0,
            health_status: HealthStatus::Unknown,
            last_health_check_ms: 0,
            name_override: None,
            config_override: None,
        };

        let plain_proto = plain_node.to_proto("jetson_1", "jetson-1.local", &[]);
        assert_eq!(plain_proto.name, "openmeteo");
        assert_eq!(plain_proto.base_node, "");

        // Build a NodeList containing all 4 nodes and verify it round-trips
        let node_list = NodeList {
            nodes: {
                let mut v = protos.clone();
                v.push(plain_proto.clone());
                v
            },
            timestamp_ms: 1700000000000,
            machine_id: "jetson_1".to_string(),
        };

        assert_eq!(node_list.nodes.len(), 4);

        // Encode to protobuf bytes and decode back
        let mut buf = Vec::new();
        prost::Message::encode(&node_list, &mut buf).unwrap();
        let decoded = NodeList::decode(&buf[..]).unwrap();

        assert_eq!(decoded.nodes.len(), 4);
        // Instances should preserve base_node through encode/decode
        assert_eq!(decoded.nodes[0].name, "rtsp-camera-terrace");
        assert_eq!(decoded.nodes[0].base_node, "rtsp-camera");
        assert_eq!(decoded.nodes[1].name, "rtsp-camera-garage");
        assert_eq!(decoded.nodes[1].base_node, "rtsp-camera");
        assert_eq!(decoded.nodes[2].name, "rtsp-camera-entrance");
        assert_eq!(decoded.nodes[2].base_node, "rtsp-camera");
        // Plain node should have empty base_node
        assert_eq!(decoded.nodes[3].name, "openmeteo");
        assert_eq!(decoded.nodes[3].base_node, "");
    }

    #[test]
    fn test_cached_node_effective_name_with_override() {
        let node = CachedNode {
            path: "/path/to/rtsp-camera".to_string(),
            manifest: Some(NodeManifest {
                name: "rtsp-camera".to_string(),
                version: "0.1.0".to_string(),
                description: "RTSP camera node".to_string(),
                node_type: "rust".to_string(),
                ..Default::default()
            }),
            status: NodeStatus::Stopped,
            installed: false,
            autostart_enabled: false,
            is_built: false,
            build_state: BuildState::default(),
            last_updated_ms: 0,
            health_status: HealthStatus::Unknown,
            last_health_check_ms: 0,
            name_override: Some("rtsp-camera-terrace".to_string()),
            config_override: None,
        };
        assert_eq!(node.effective_name(), "rtsp-camera-terrace");
    }

    #[test]
    fn test_cached_node_effective_name_without_override() {
        let node = CachedNode {
            path: "/path/to/openmeteo".to_string(),
            manifest: Some(NodeManifest {
                name: "openmeteo".to_string(),
                version: "0.1.0".to_string(),
                description: "Weather".to_string(),
                node_type: "rust".to_string(),
                ..Default::default()
            }),
            status: NodeStatus::Running,
            installed: true,
            autostart_enabled: false,
            is_built: true,
            build_state: BuildState::default(),
            last_updated_ms: 0,
            health_status: HealthStatus::Unknown,
            last_health_check_ms: 0,
            name_override: None,
            config_override: None,
        };
        assert_eq!(node.effective_name(), "openmeteo");
    }

    #[test]
    fn test_cached_node_effective_name_no_manifest() {
        let node = CachedNode {
            path: "/path/to/unknown".to_string(),
            manifest: None,
            status: NodeStatus::Stopped,
            installed: false,
            autostart_enabled: false,
            is_built: false,
            build_state: BuildState::default(),
            last_updated_ms: 0,
            health_status: HealthStatus::Unknown,
            last_health_check_ms: 0,
            name_override: None,
            config_override: None,
        };
        assert_eq!(node.effective_name(), "unknown");
    }

    #[test]
    fn test_journalctl_uses_absolute_path() {
        assert!(JOURNALCTL_PATH.starts_with('/'));
    }
}
