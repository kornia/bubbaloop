use std::sync::Arc;
use std::time::Duration;

use anyhow::{anyhow, Result};
use prost::Message;
use serde::{Deserialize, Serialize};
use tokio::sync::mpsc;
use zenoh::Session;

use crate::schemas::{NodeList, NodeStatus};
use crate::tui::app::NodeInfo;

#[allow(dead_code)]
const API_PREFIX: &str = "bubbaloop/daemon/api";
const API_HEALTH: &str = "bubbaloop/daemon/api/health";
const API_NODES: &str = "bubbaloop/daemon/api/nodes";
const API_NODES_ADD: &str = "bubbaloop/daemon/api/nodes/add";

/// Subscription topic for real-time node list updates (protobuf)
const NODES_TOPIC: &str = "bubbaloop/daemon/nodes";

#[derive(Debug, Serialize, Deserialize)]
struct HealthResponse {
    status: String,
}

#[derive(Debug, Serialize, Deserialize)]
struct NodeState {
    name: String,
    path: String,
    status: String,
    #[allow(dead_code)]
    installed: bool,
    #[allow(dead_code)]
    autostart_enabled: bool,
    version: String,
    description: String,
    node_type: String,
    is_built: bool,
    build_output: Vec<String>,
}

#[derive(Debug, Serialize, Deserialize)]
struct NodeListResponse {
    nodes: Vec<NodeState>,
    #[allow(dead_code)]
    timestamp_ms: u64,
}

#[derive(Debug, Serialize, Deserialize)]
struct CommandRequest {
    command: String,
    node_path: String,
}

#[derive(Debug, Serialize, Deserialize)]
struct CommandResponse {
    success: bool,
    message: String,
    #[allow(dead_code)]
    output: String,
}

/// Convert protobuf NodeStatus to string
fn proto_status_to_string(status: i32) -> String {
    match NodeStatus::try_from(status) {
        Ok(NodeStatus::Stopped) => "stopped".to_string(),
        Ok(NodeStatus::Running) => "running".to_string(),
        Ok(NodeStatus::Failed) => "failed".to_string(),
        Ok(NodeStatus::Installing) => "installing".to_string(),
        Ok(NodeStatus::Building) => "building".to_string(),
        Ok(NodeStatus::NotInstalled) => "not-installed".to_string(),
        _ => "unknown".to_string(),
    }
}

/// Decode protobuf NodeList to Vec<NodeInfo>
fn decode_node_list(bytes: &[u8]) -> Result<Vec<NodeInfo>> {
    let node_list =
        NodeList::decode(bytes).map_err(|e| anyhow!("Failed to decode NodeList: {}", e))?;

    Ok(node_list
        .nodes
        .into_iter()
        .map(|n| NodeInfo {
            name: n.name,
            path: n.path,
            version: n.version,
            node_type: n.node_type,
            description: n.description,
            status: proto_status_to_string(n.status),
            is_built: n.is_built,
            build_output: n.build_output,
        })
        .collect())
}

#[derive(Clone)]
pub struct DaemonClient {
    session: Arc<Session>,
}

impl DaemonClient {
    pub async fn new() -> Result<Self> {
        let mut config = zenoh::Config::default();

        // Run as client mode - only connect to router, don't listen
        config.insert_json5("mode", "\"client\"").ok();

        // Use env var or default to localhost
        let endpoint = std::env::var("BUBBALOOP_ZENOH_ENDPOINT")
            .unwrap_or_else(|_| "tcp/127.0.0.1:7447".to_string());
        config
            .insert_json5("connect/endpoints", &format!("[\"{}\"]", endpoint))
            .ok();

        // Disable all scouting to avoid connecting to remote peers via Tailscale
        config
            .insert_json5("scouting/multicast/enabled", "false")
            .ok();
        config.insert_json5("scouting/gossip/enabled", "false").ok();

        let session = zenoh::open(config)
            .await
            .map_err(|e| anyhow!("Failed to connect to zenoh: {}", e))?;
        Ok(Self {
            session: Arc::new(session),
        })
    }

    async fn query<T: for<'de> Deserialize<'de>>(
        &self,
        key_expr: &str,
        payload: Option<&str>,
    ) -> Result<T> {
        let mut getter = self.session.get(key_expr);

        if let Some(p) = payload {
            getter = getter.payload(p.as_bytes());
        }

        let replies = getter
            .timeout(Duration::from_secs(5))
            .await
            .map_err(|e| {
                anyhow!(
                    "Zenoh query failed for '{}': {} (check if daemon is running and zenoh router is accessible)",
                    key_expr,
                    e
                )
            })?;

        while let Ok(reply) = replies.recv_async().await {
            match reply.result() {
                Ok(sample) => {
                    let bytes = sample.payload().to_bytes();
                    let text = String::from_utf8_lossy(&bytes);
                    let result: T = serde_json::from_str(&text).map_err(|e| {
                        anyhow!("Failed to parse response from '{}': {}", key_expr, e)
                    })?;
                    return Ok(result);
                }
                Err(e) => {
                    log::debug!("Received error reply for '{}': {:?}", key_expr, e);
                }
            }
        }

        Err(anyhow!(
            "No reply received for '{}' (5s timeout) - daemon may be unavailable",
            key_expr
        ))
    }

    pub async fn is_available(&self) -> bool {
        match self.query::<HealthResponse>(API_HEALTH, None).await {
            Ok(response) => response.status == "ok",
            Err(e) => {
                // Log at debug level to avoid spamming in TUI
                log::debug!("Daemon health check failed: {}", e);
                false
            }
        }
    }

    pub async fn list_nodes(&self) -> Result<Vec<NodeInfo>> {
        let response: NodeListResponse = self.query(API_NODES, None).await?;
        Ok(response
            .nodes
            .into_iter()
            .map(|n| NodeInfo {
                name: n.name,
                path: n.path,
                version: n.version,
                node_type: n.node_type,
                description: n.description,
                status: n.status,
                is_built: n.is_built,
                build_output: n.build_output,
            })
            .collect())
    }

    pub async fn execute_command(&self, node_name: &str, command: &str) -> Result<()> {
        let key_expr = format!("{}/{}/command", API_NODES, node_name);
        let payload = serde_json::to_string(&CommandRequest {
            command: command.to_string(),
            node_path: String::new(),
        })?;

        let response: CommandResponse = self.query(&key_expr, Some(&payload)).await?;

        if response.success {
            Ok(())
        } else {
            Err(anyhow!("{}", response.message))
        }
    }

    /// Fire-and-forget: send a Zenoh query and consume replies in the background.
    /// Used when the caller relies on the subscription for state confirmation
    /// rather than waiting for the reply inline.
    async fn fire_and_forget(&self, key_expr: &str, payload: &str) -> Result<()> {
        let replies = self
            .session
            .get(key_expr)
            .payload(payload.as_bytes())
            .timeout(Duration::from_secs(5))
            .await
            .map_err(|e| anyhow!("Failed to send to '{}': {}", key_expr, e))?;

        // Consume replies in background so Zenoh doesn't log warnings
        tokio::spawn(async move {
            while let Ok(reply) = replies.recv_async().await {
                if let Err(e) = reply.result() {
                    log::debug!("Reply error: {:?}", e);
                }
            }
        });

        Ok(())
    }

    /// Send a command without waiting for the reply.
    /// Use the subscription for real-time state confirmation instead.
    pub async fn send_command(&self, node_name: &str, command: &str) -> Result<()> {
        let key_expr = format!("{}/{}/command", API_NODES, node_name);
        let payload = serde_json::to_string(&CommandRequest {
            command: command.to_string(),
            node_path: String::new(),
        })?;
        self.fire_and_forget(&key_expr, &payload).await
    }

    /// Send an add_node command without waiting for the reply.
    pub async fn send_add_node(&self, path: &str) -> Result<()> {
        let payload = serde_json::to_string(&CommandRequest {
            command: "add".to_string(),
            node_path: path.to_string(),
        })?;
        self.fire_and_forget(API_NODES_ADD, &payload).await
    }

    pub async fn add_node(&self, path: &str) -> Result<()> {
        let payload = serde_json::to_string(&CommandRequest {
            command: "add".to_string(),
            node_path: path.to_string(),
        })?;

        let response: CommandResponse = self.query(API_NODES_ADD, Some(&payload)).await?;

        if response.success {
            Ok(())
        } else {
            Err(anyhow!("{}", response.message))
        }
    }

    /// Subscribe to node list updates via protobuf pub/sub
    /// Returns a receiver that yields NodeList updates in real-time
    /// This is the non-blocking approach used by the dashboard
    pub async fn subscribe_nodes(&self) -> Result<mpsc::Receiver<Vec<NodeInfo>>> {
        let (tx, rx) = mpsc::channel(16);

        let subscriber = self
            .session
            .declare_subscriber(NODES_TOPIC)
            .await
            .map_err(|e| anyhow!("Failed to subscribe to {}: {}", NODES_TOPIC, e))?;

        // Spawn task to handle subscription
        let _session = self.session.clone();
        tokio::spawn(async move {
            loop {
                match subscriber.recv_async().await {
                    Ok(sample) => {
                        let bytes = sample.payload().to_bytes();
                        match decode_node_list(&bytes) {
                            Ok(nodes) => {
                                if tx.send(nodes).await.is_err() {
                                    // Receiver dropped, exit
                                    break;
                                }
                            }
                            Err(e) => {
                                log::debug!("Failed to decode node list: {}", e);
                            }
                        }
                    }
                    Err(e) => {
                        log::warn!("Subscription error: {}", e);
                        tokio::time::sleep(Duration::from_secs(1)).await;
                    }
                }
            }
        });

        Ok(rx)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_health_response_deserialization() {
        let json = r#"{"status": "ok"}"#;
        let response: HealthResponse = serde_json::from_str(json).unwrap();
        assert_eq!(response.status, "ok");
    }

    #[test]
    fn test_health_response_not_ok() {
        let json = r#"{"status": "error"}"#;
        let response: HealthResponse = serde_json::from_str(json).unwrap();
        assert_eq!(response.status, "error");
    }

    #[test]
    fn test_node_state_deserialization() {
        let json = r#"{
            "name": "test-node",
            "path": "/home/user/nodes/test",
            "status": "running",
            "installed": true,
            "autostart_enabled": false,
            "version": "0.1.0",
            "description": "A test node",
            "node_type": "rust",
            "is_built": true,
            "build_output": ["Compiling...", "Finished"]
        }"#;

        let node: NodeState = serde_json::from_str(json).unwrap();
        assert_eq!(node.name, "test-node");
        assert_eq!(node.path, "/home/user/nodes/test");
        assert_eq!(node.status, "running");
        assert_eq!(node.version, "0.1.0");
        assert_eq!(node.node_type, "rust");
        assert!(node.is_built);
    }

    #[test]
    fn test_node_list_response_deserialization() {
        let json = r#"{
            "nodes": [
                {
                    "name": "node1",
                    "path": "/path/to/node1",
                    "status": "running",
                    "installed": true,
                    "autostart_enabled": true,
                    "version": "1.0.0",
                    "description": "First node",
                    "node_type": "rust",
                    "is_built": true,
                    "build_output": []
                },
                {
                    "name": "node2",
                    "path": "/path/to/node2",
                    "status": "stopped",
                    "installed": false,
                    "autostart_enabled": false,
                    "version": "0.5.0",
                    "description": "Second node",
                    "node_type": "python",
                    "is_built": false,
                    "build_output": []
                }
            ],
            "timestamp_ms": 1609459200000
        }"#;

        let response: NodeListResponse = serde_json::from_str(json).unwrap();
        assert_eq!(response.nodes.len(), 2);
        assert_eq!(response.nodes[0].name, "node1");
        assert_eq!(response.nodes[1].name, "node2");
        assert_eq!(response.timestamp_ms, 1609459200000);
    }

    #[test]
    fn test_command_request_serialization() {
        let request = CommandRequest {
            command: "start".to_string(),
            node_path: "/path/to/node".to_string(),
        };

        let json = serde_json::to_string(&request).unwrap();
        assert!(json.contains("\"command\""));
        assert!(json.contains("\"start\""));
        assert!(json.contains("\"node_path\""));
        assert!(json.contains("/path/to/node"));
    }

    #[test]
    fn test_command_request_various_commands() {
        let commands = vec!["start", "stop", "restart", "build", "install"];

        for cmd in commands {
            let request = CommandRequest {
                command: cmd.to_string(),
                node_path: "/test".to_string(),
            };
            let json = serde_json::to_string(&request).unwrap();
            assert!(json.contains(cmd));
        }
    }

    #[test]
    fn test_command_response_deserialization_success() {
        let json = r#"{
            "success": true,
            "message": "Node started successfully",
            "output": "Starting node..."
        }"#;

        let response: CommandResponse = serde_json::from_str(json).unwrap();
        assert!(response.success);
        assert_eq!(response.message, "Node started successfully");
        assert_eq!(response.output, "Starting node...");
    }

    #[test]
    fn test_command_response_deserialization_failure() {
        let json = r#"{
            "success": false,
            "message": "Failed to start node",
            "output": "Error: node not found"
        }"#;

        let response: CommandResponse = serde_json::from_str(json).unwrap();
        assert!(!response.success);
        assert_eq!(response.message, "Failed to start node");
    }

    #[test]
    fn test_command_response_minimal() {
        let json = r#"{
            "success": true,
            "message": "OK",
            "output": ""
        }"#;

        let response: CommandResponse = serde_json::from_str(json).unwrap();
        assert!(response.success);
        assert_eq!(response.message, "OK");
        assert_eq!(response.output, "");
    }

    #[test]
    fn test_node_state_to_node_info_conversion() {
        let node_state = NodeState {
            name: "sensor".to_string(),
            path: "/path/to/sensor".to_string(),
            status: "active".to_string(),
            installed: true,
            autostart_enabled: true,
            version: "2.0.0".to_string(),
            description: "Temperature sensor".to_string(),
            node_type: "python".to_string(),
            is_built: true,
            build_output: vec![],
        };

        let node_info = NodeInfo {
            name: node_state.name.clone(),
            path: node_state.path.clone(),
            version: node_state.version.clone(),
            node_type: node_state.node_type.clone(),
            description: node_state.description.clone(),
            status: node_state.status.clone(),
            is_built: node_state.is_built,
            build_output: node_state.build_output.clone(),
        };

        assert_eq!(node_info.name, "sensor");
        assert_eq!(node_info.path, "/path/to/sensor");
        assert_eq!(node_info.status, "active");
        assert_eq!(node_info.node_type, "python");
        assert!(node_info.is_built);
    }
}
