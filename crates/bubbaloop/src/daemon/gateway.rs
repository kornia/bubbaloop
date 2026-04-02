//! Gateway wire format and topic builders for daemon Zenoh messaging.
//!
//! The Gateway is a convention (topic pair + JSON schema), not a process.
//! Messages flow through Zenoh pub/sub between CLI clients and the daemon.
//! Mirrors the agent gateway pattern (`agent/gateway.rs`).

use serde::{Deserialize, Serialize};

// ── Command (CLI → Daemon) ──────────────────────────────────────

/// A command sent to the daemon's inbox topic.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct DaemonCommand {
    /// Correlation ID (UUID) for matching response events.
    pub id: String,
    /// The command to execute.
    pub command: DaemonCommandType,
}

/// Command types the daemon can process.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "snake_case")]
pub enum DaemonCommandType {
    /// List all registered nodes.
    ListNodes,
    /// Start a node by name.
    StartNode { name: String },
    /// Stop a node by name.
    StopNode { name: String },
    /// Restart a node by name.
    RestartNode { name: String },
    /// Get logs for a node.
    GetLogs { name: String },
    /// Install a node from source (marketplace name, path, or GitHub).
    InstallNode {
        source: String,
        name: Option<String>,
        config: Option<String>,
    },
    /// Remove a node by name.
    RemoveNode { name: String },
    /// Build a node by name.
    BuildNode { name: String },
    /// Install a registered node as a systemd service (by name).
    InstallService { name: String },
    /// Uninstall a node by name.
    UninstallNode { name: String },
    /// Clean build artifacts for a node.
    CleanNode { name: String },
    /// Enable autostart for a node.
    EnableAutostart { name: String },
    /// Disable autostart for a node.
    DisableAutostart { name: String },
    /// Query daemon health.
    Health,
    /// Graceful daemon shutdown.
    Shutdown,
}

// ── Event (Daemon → CLI) ────────────────────────────────────────

/// Event type for daemon outbox messages.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "snake_case")]
pub enum DaemonEventType {
    /// Command result (success).
    Result,
    /// Error response.
    Error,
    /// Async notification (e.g., node state change).
    Notification,
    /// Command processing complete — no more events for this correlation ID.
    Done,
}

/// An event emitted by the daemon on its outbox topic.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct DaemonEvent {
    /// Correlation ID matching the command.
    pub id: String,
    /// Event type.
    #[serde(rename = "type")]
    pub event_type: DaemonEventType,
    /// Event payload (result text, error message, etc.).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub text: Option<String>,
}

impl DaemonEvent {
    /// Create a Result event (successful response).
    pub fn result(id: &str, text: &str) -> Self {
        Self {
            id: id.to_string(),
            event_type: DaemonEventType::Result,
            text: Some(text.to_string()),
        }
    }

    /// Create an Error event.
    pub fn error(id: &str, message: &str) -> Self {
        Self {
            id: id.to_string(),
            event_type: DaemonEventType::Error,
            text: Some(message.to_string()),
        }
    }

    /// Create a Notification event (async state change).
    pub fn notification(id: &str, text: &str) -> Self {
        Self {
            id: id.to_string(),
            event_type: DaemonEventType::Notification,
            text: Some(text.to_string()),
        }
    }

    /// Create a Done event (command complete).
    pub fn done(id: &str) -> Self {
        Self {
            id: id.to_string(),
            event_type: DaemonEventType::Done,
            text: None,
        }
    }
}

// ── Manifest (queryable) ────────────────────────────────────────

/// Daemon manifest — advertises capabilities via Zenoh queryable.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct DaemonManifest {
    /// Daemon version (from CARGO_PKG_VERSION).
    pub version: String,
    /// Machine ID where this daemon is running.
    pub machine_id: String,
    /// Daemon uptime in seconds.
    pub uptime_secs: u64,
    /// Number of registered nodes.
    pub node_count: usize,
    /// Number of active agents.
    pub agent_count: usize,
    /// MCP server port.
    pub mcp_port: u16,
}

// ── JSON mirror types (for JSON queryable responses) ────────────

/// JSON-serializable mirror of proto NodeState.
///
/// Used by the nodes queryable instead of prost-generated NodeState.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct NodeStateJson {
    pub name: String,
    pub path: String,
    pub status: i32,
    pub installed: bool,
    pub autostart_enabled: bool,
    pub version: String,
    pub description: String,
    pub node_type: String,
    pub is_built: bool,
    pub last_updated_ms: i64,
    pub build_output: Vec<String>,
    pub health_status: i32,
    pub last_health_check_ms: i64,
    pub machine_id: String,
    pub machine_hostname: String,
    pub machine_ips: Vec<String>,
    pub base_node: String,
    pub config_override: String,
}

/// JSON-serializable mirror of proto NodeList.
///
/// Used by the nodes queryable instead of prost-generated NodeList.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct NodeListJson {
    pub nodes: Vec<NodeStateJson>,
    pub timestamp_ms: i64,
    pub machine_id: String,
}

impl NodeListJson {
    /// Convert from prost-generated NodeList to JSON-serializable form.
    pub fn from_proto(proto: &crate::schemas::daemon::v1::NodeList) -> Self {
        Self {
            nodes: proto
                .nodes
                .iter()
                .map(|n| NodeStateJson {
                    name: n.name.clone(),
                    path: n.path.clone(),
                    status: n.status,
                    installed: n.installed,
                    autostart_enabled: n.autostart_enabled,
                    version: n.version.clone(),
                    description: n.description.clone(),
                    node_type: n.node_type.clone(),
                    is_built: n.is_built,
                    last_updated_ms: n.last_updated_ms,
                    build_output: n.build_output.clone(),
                    health_status: n.health_status,
                    last_health_check_ms: n.last_health_check_ms,
                    machine_id: n.machine_id.clone(),
                    machine_hostname: n.machine_hostname.clone(),
                    machine_ips: n.machine_ips.clone(),
                    base_node: n.base_node.clone(),
                    config_override: n.config_override.clone(),
                })
                .collect(),
            timestamp_ms: proto.timestamp_ms,
            machine_id: proto.machine_id.clone(),
        }
    }
}

/// JSON-serializable command sent to the command queryable.
///
/// Replaces prost-generated NodeCommand for JSON-only wire format.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct NodeCommandJson {
    /// Type of command (e.g. "start", "stop", "get_logs").
    pub command: String,
    /// Target node name.
    pub node_name: String,
    /// Unique request ID for correlation.
    #[serde(default)]
    pub request_id: String,
    /// Target machine ID (empty = respond regardless).
    #[serde(default)]
    pub target_machine: String,
    /// Command issue time (milliseconds since epoch).
    #[serde(default)]
    pub timestamp_ms: i64,
}

/// JSON-serializable reply from the command queryable.
///
/// Replaces prost-generated CommandResult for JSON-only wire format.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct CommandResultJson {
    /// Request ID from the command.
    pub request_id: String,
    /// Whether the command succeeded.
    pub success: bool,
    /// Human-readable message.
    pub message: String,
    /// Additional output (build output, logs, etc.).
    pub output: String,
    /// Which daemon responded.
    pub responding_machine: String,
    /// Response time (milliseconds since epoch).
    pub timestamp_ms: i64,
}

// ── Topic builders ──────────────────────────────────────────────

/// Build the daemon command topic (CLI → Daemon).
///
/// Format: `bubbaloop/{scope}/{machine}/daemon/command`
pub fn command_topic(scope: &str, machine_id: &str) -> String {
    format!("bubbaloop/{}/{}/daemon/command", scope, machine_id)
}

/// Build the daemon events topic (Daemon → CLI).
///
/// Format: `bubbaloop/{scope}/{machine}/daemon/events`
pub fn events_topic(scope: &str, machine_id: &str) -> String {
    format!("bubbaloop/{}/{}/daemon/events", scope, machine_id)
}

/// Build the daemon manifest topic (queryable).
///
/// Format: `bubbaloop/{scope}/{machine}/daemon/manifest`
pub fn manifest_topic(scope: &str, machine_id: &str) -> String {
    format!("bubbaloop/{}/{}/daemon/manifest", scope, machine_id)
}

/// Build a wildcard pattern for discovering daemon manifests on ALL machines.
///
/// Format: `bubbaloop/{scope}/*/daemon/manifest`
pub fn manifest_wildcard(scope: &str) -> String {
    format!("bubbaloop/{}/*/daemon/manifest", scope)
}

/// Build the daemon nodes topic (queryable — returns JSON NodeListJson).
///
/// Format: `bubbaloop/{scope}/{machine}/daemon/nodes`
pub fn nodes_topic(scope: &str, machine_id: &str) -> String {
    format!("bubbaloop/{}/{}/daemon/nodes", scope, machine_id)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn daemon_command_serde_roundtrip_list_nodes() {
        let cmd = DaemonCommand {
            id: "abc-123".to_string(),
            command: DaemonCommandType::ListNodes,
        };
        let json = serde_json::to_string(&cmd).unwrap();
        let parsed: DaemonCommand = serde_json::from_str(&json).unwrap();
        assert_eq!(cmd, parsed);
    }

    #[test]
    fn daemon_command_serde_roundtrip_start_node() {
        let cmd = DaemonCommand {
            id: "abc-123".to_string(),
            command: DaemonCommandType::StartNode {
                name: "camera".to_string(),
            },
        };
        let json = serde_json::to_string(&cmd).unwrap();
        let parsed: DaemonCommand = serde_json::from_str(&json).unwrap();
        assert_eq!(cmd, parsed);
    }

    #[test]
    fn daemon_command_serde_roundtrip_install() {
        let cmd = DaemonCommand {
            id: "abc-123".to_string(),
            command: DaemonCommandType::InstallNode {
                source: "rtsp-camera".to_string(),
                name: Some("entrance-cam".to_string()),
                config: None,
            },
        };
        let json = serde_json::to_string(&cmd).unwrap();
        let parsed: DaemonCommand = serde_json::from_str(&json).unwrap();
        assert_eq!(cmd, parsed);
    }

    #[test]
    fn daemon_command_all_types_roundtrip() {
        let commands = vec![
            DaemonCommandType::ListNodes,
            DaemonCommandType::StartNode {
                name: "cam".to_string(),
            },
            DaemonCommandType::StopNode {
                name: "cam".to_string(),
            },
            DaemonCommandType::RestartNode {
                name: "cam".to_string(),
            },
            DaemonCommandType::GetLogs {
                name: "cam".to_string(),
            },
            DaemonCommandType::InstallNode {
                source: "src".to_string(),
                name: None,
                config: None,
            },
            DaemonCommandType::RemoveNode {
                name: "cam".to_string(),
            },
            DaemonCommandType::BuildNode {
                name: "cam".to_string(),
            },
            DaemonCommandType::InstallService {
                name: "cam".to_string(),
            },
            DaemonCommandType::UninstallNode {
                name: "cam".to_string(),
            },
            DaemonCommandType::CleanNode {
                name: "cam".to_string(),
            },
            DaemonCommandType::EnableAutostart {
                name: "cam".to_string(),
            },
            DaemonCommandType::DisableAutostart {
                name: "cam".to_string(),
            },
            DaemonCommandType::Health,
            DaemonCommandType::Shutdown,
        ];
        for command in commands {
            let cmd = DaemonCommand {
                id: "id".to_string(),
                command,
            };
            let json = serde_json::to_string(&cmd).unwrap();
            let parsed: DaemonCommand = serde_json::from_str(&json).unwrap();
            assert_eq!(cmd, parsed);
        }
    }

    #[test]
    fn daemon_event_result_serde() {
        let event = DaemonEvent::result("id-1", "ok");
        let json = serde_json::to_string(&event).unwrap();
        assert!(json.contains("\"type\":\"result\""));
        let parsed: DaemonEvent = serde_json::from_str(&json).unwrap();
        assert_eq!(event, parsed);
    }

    #[test]
    fn daemon_event_error_serde() {
        let event = DaemonEvent::error("id-1", "node not found");
        let json = serde_json::to_string(&event).unwrap();
        assert!(json.contains("\"type\":\"error\""));
        assert!(json.contains("node not found"));
    }

    #[test]
    fn daemon_event_done_no_text() {
        let event = DaemonEvent::done("id-1");
        let json = serde_json::to_string(&event).unwrap();
        assert!(!json.contains("\"text\""));
    }

    #[test]
    fn daemon_event_all_types_roundtrip() {
        let events = vec![
            DaemonEvent::result("id", "ok"),
            DaemonEvent::error("id", "fail"),
            DaemonEvent::notification("id", "node started"),
            DaemonEvent::done("id"),
        ];
        for event in events {
            let json = serde_json::to_string(&event).unwrap();
            let parsed: DaemonEvent = serde_json::from_str(&json).unwrap();
            assert_eq!(event, parsed);
        }
    }

    #[test]
    fn daemon_manifest_serde_roundtrip() {
        let manifest = DaemonManifest {
            version: "0.0.9-dev".to_string(),
            machine_id: "jetson01".to_string(),
            uptime_secs: 3600,
            node_count: 5,
            agent_count: 2,
            mcp_port: 8088,
        };
        let json = serde_json::to_string(&manifest).unwrap();
        let parsed: DaemonManifest = serde_json::from_str(&json).unwrap();
        assert_eq!(manifest, parsed);
    }

    #[test]
    fn command_topic_format() {
        assert_eq!(
            command_topic("local", "jetson01"),
            "bubbaloop/local/jetson01/daemon/command"
        );
    }

    #[test]
    fn events_topic_format() {
        assert_eq!(
            events_topic("local", "jetson01"),
            "bubbaloop/local/jetson01/daemon/events"
        );
    }

    #[test]
    fn manifest_topic_format() {
        assert_eq!(
            manifest_topic("local", "jetson01"),
            "bubbaloop/local/jetson01/daemon/manifest"
        );
    }

    #[test]
    fn manifest_wildcard_format() {
        assert_eq!(
            manifest_wildcard("local"),
            "bubbaloop/local/*/daemon/manifest"
        );
    }
}
