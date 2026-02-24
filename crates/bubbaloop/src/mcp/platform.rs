//! Clean layer boundary between MCP server and daemon internals.
//!
//! MCP tools call PlatformOperations instead of Arc<NodeManager> directly,
//! making the MCP server testable with mock implementations.

use serde_json::Value;

/// Result type for platform operations.
pub type PlatformResult<T> = Result<T, PlatformError>;

/// Errors from platform operations.
#[derive(Debug, thiserror::Error)]
pub enum PlatformError {
    #[error("Node not found: {0}")]
    NodeNotFound(String),
    #[error("Command failed: {0}")]
    CommandFailed(String),
    #[error("Invalid input: {0}")]
    InvalidInput(String),
    #[error("Internal error: {0}")]
    Internal(String),
}

/// Node summary for list operations.
#[derive(Debug, Clone, serde::Serialize)]
pub struct NodeInfo {
    pub name: String,
    pub status: String,
    pub health: String,
    pub node_type: String,
    pub installed: bool,
    pub is_built: bool,
}

/// Command to execute on a node.
#[derive(Debug, Clone)]
pub enum NodeCommand {
    Start,
    Stop,
    Restart,
    Build,
    GetLogs,
}

/// Abstraction over daemon internals.
///
/// MCP tools call this trait instead of `Arc<NodeManager>` directly.
/// This makes the MCP server testable with mock implementations.
pub trait PlatformOperations: Send + Sync + 'static {
    fn list_nodes(&self) -> impl std::future::Future<Output = PlatformResult<Vec<NodeInfo>>> + Send;
    fn get_node_detail(
        &self,
        name: &str,
    ) -> impl std::future::Future<Output = PlatformResult<Value>> + Send;
    fn execute_command(
        &self,
        name: &str,
        cmd: NodeCommand,
    ) -> impl std::future::Future<Output = PlatformResult<String>> + Send;
    fn get_node_config(
        &self,
        name: &str,
    ) -> impl std::future::Future<Output = PlatformResult<Value>> + Send;
    fn query_zenoh(
        &self,
        key_expr: &str,
    ) -> impl std::future::Future<Output = PlatformResult<String>> + Send;
}

// ── DaemonPlatform: real implementation backed by NodeManager + Zenoh ────

use crate::daemon::node_manager::NodeManager;
use crate::schemas::daemon::v1::{
    CommandType, HealthStatus, NodeCommand as ProtoNodeCommand, NodeStatus,
};
use std::sync::Arc;
use zenoh::Session;

/// Real platform backed by NodeManager + Zenoh session.
pub struct DaemonPlatform {
    pub node_manager: Arc<NodeManager>,
    pub session: Arc<Session>,
    pub scope: String,
    pub machine_id: String,
}

impl PlatformOperations for DaemonPlatform {
    async fn list_nodes(&self) -> PlatformResult<Vec<NodeInfo>> {
        let node_list = self.node_manager.get_node_list().await;
        let nodes = node_list
            .nodes
            .iter()
            .map(|n| {
                let status = NodeStatus::try_from(n.status)
                    .unwrap_or(NodeStatus::Unknown);
                let health = HealthStatus::try_from(n.health_status)
                    .unwrap_or(HealthStatus::Unknown);
                NodeInfo {
                    name: n.name.clone(),
                    status: format!("{:?}", status),
                    health: format!("{:?}", health),
                    node_type: n.node_type.clone(),
                    installed: n.installed,
                    is_built: n.is_built,
                }
            })
            .collect();
        Ok(nodes)
    }

    async fn get_node_detail(&self, name: &str) -> PlatformResult<Value> {
        match self.node_manager.get_node(name).await {
            Some(node) => {
                let status = NodeStatus::try_from(node.status)
                    .unwrap_or(NodeStatus::Unknown);
                let health = HealthStatus::try_from(node.health_status)
                    .unwrap_or(HealthStatus::Unknown);
                let detail = serde_json::json!({
                    "name": node.name,
                    "status": format!("{:?}", status),
                    "health_status": format!("{:?}", health),
                    "node_type": node.node_type,
                    "installed": node.installed,
                    "is_built": node.is_built,
                    "last_health_check_ms": node.last_health_check_ms,
                    "last_updated_ms": node.last_updated_ms,
                    "path": node.path,
                    "version": node.version,
                    "description": node.description,
                    "machine_id": node.machine_id,
                });
                Ok(detail)
            }
            None => Err(PlatformError::NodeNotFound(name.to_string())),
        }
    }

    async fn execute_command(
        &self,
        name: &str,
        cmd: NodeCommand,
    ) -> PlatformResult<String> {
        let cmd_type = match cmd {
            NodeCommand::Start => CommandType::Start,
            NodeCommand::Stop => CommandType::Stop,
            NodeCommand::Restart => CommandType::Restart,
            NodeCommand::Build => CommandType::Build,
            NodeCommand::GetLogs => CommandType::GetLogs,
        };

        let proto_cmd = ProtoNodeCommand {
            command: cmd_type as i32,
            node_name: name.to_string(),
            request_id: uuid::Uuid::new_v4().to_string(),
            timestamp_ms: now_ms(),
            source_machine: "mcp-platform".to_string(),
            target_machine: String::new(),
            node_path: String::new(),
            name_override: String::new(),
            config_override: String::new(),
        };

        let result = self.node_manager.execute_command(proto_cmd).await;
        if result.success {
            if result.output.is_empty() {
                Ok(result.message)
            } else {
                Ok(format!("{}\n{}", result.message, result.output))
            }
        } else {
            Err(PlatformError::CommandFailed(result.message))
        }
    }

    async fn get_node_config(&self, name: &str) -> PlatformResult<Value> {
        let key_expr = format!(
            "bubbaloop/{}/{}/{}/config",
            self.scope, self.machine_id, name
        );
        let text = zenoh_get_text(&self.session, &key_expr).await;
        serde_json::from_str(&text).or_else(|_| Ok(serde_json::json!({ "raw": text })))
    }

    async fn query_zenoh(&self, key_expr: &str) -> PlatformResult<String> {
        Ok(zenoh_get_text(&self.session, key_expr).await)
    }
}

/// Query a Zenoh key expression and return text results.
async fn zenoh_get_text(session: &Session, key_expr: &str) -> String {
    match session
        .get(key_expr)
        .timeout(std::time::Duration::from_secs(3))
        .await
    {
        Ok(replies) => {
            let mut results = Vec::new();
            while let Ok(reply) = replies.recv_async().await {
                match reply.result() {
                    Ok(sample) => {
                        let key = sample.key_expr().to_string();
                        let bytes = sample.payload().to_bytes();
                        match String::from_utf8(bytes.to_vec()) {
                            Ok(text) => results.push(format!("[{}] {}", key, text)),
                            Err(_) => {
                                results.push(format!(
                                    "[{}] <{} bytes binary>",
                                    key,
                                    bytes.len()
                                ))
                            }
                        }
                    }
                    Err(err) => {
                        results.push(format!("Error: {:?}", err.payload().to_bytes()));
                    }
                }
            }
            if results.is_empty() {
                "No responses received".to_string()
            } else {
                results.join("\n")
            }
        }
        Err(e) => format!("Zenoh query failed: {}", e),
    }
}

fn now_ms() -> i64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis() as i64
}

// ── MockPlatform for testing ─────────────────────────────────────────────

#[cfg(test)]
pub mod mock {
    use super::*;
    use std::collections::HashMap;
    use std::sync::Mutex;

    pub struct MockPlatform {
        pub nodes: Mutex<Vec<NodeInfo>>,
        pub configs: Mutex<HashMap<String, Value>>,
    }

    impl Default for MockPlatform {
        fn default() -> Self {
            Self::new()
        }
    }

    impl MockPlatform {
        pub fn new() -> Self {
            Self {
                nodes: Mutex::new(vec![NodeInfo {
                    name: "test-node".to_string(),
                    status: "Running".to_string(),
                    health: "Healthy".to_string(),
                    node_type: "rust".to_string(),
                    installed: true,
                    is_built: true,
                }]),
                configs: Mutex::new(HashMap::new()),
            }
        }
    }

    impl PlatformOperations for MockPlatform {
        async fn list_nodes(&self) -> PlatformResult<Vec<NodeInfo>> {
            Ok(self.nodes.lock().unwrap().clone())
        }

        async fn get_node_detail(&self, name: &str) -> PlatformResult<Value> {
            self.nodes
                .lock()
                .unwrap()
                .iter()
                .find(|n| n.name == name)
                .map(|n| serde_json::to_value(n).unwrap())
                .ok_or_else(|| PlatformError::NodeNotFound(name.to_string()))
        }

        async fn execute_command(
            &self,
            name: &str,
            _cmd: NodeCommand,
        ) -> PlatformResult<String> {
            if self.nodes.lock().unwrap().iter().any(|n| n.name == name) {
                Ok("mock: command executed".to_string())
            } else {
                Err(PlatformError::NodeNotFound(name.to_string()))
            }
        }

        async fn get_node_config(&self, name: &str) -> PlatformResult<Value> {
            self.configs
                .lock()
                .unwrap()
                .get(name)
                .cloned()
                .ok_or_else(|| PlatformError::NodeNotFound(name.to_string()))
        }

        async fn query_zenoh(&self, key_expr: &str) -> PlatformResult<String> {
            Ok(format!("mock: query {}", key_expr))
        }
    }
}

// ── Tests ────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::mock::MockPlatform;
    use super::*;

    #[tokio::test]
    async fn test_mock_list_nodes() {
        let mock = MockPlatform::new();
        let nodes = mock.list_nodes().await.unwrap();
        assert_eq!(nodes.len(), 1);
        assert_eq!(nodes[0].name, "test-node");
    }

    #[tokio::test]
    async fn test_mock_get_node_detail_found() {
        let mock = MockPlatform::new();
        let detail = mock.get_node_detail("test-node").await.unwrap();
        assert_eq!(detail["name"], "test-node");
    }

    #[tokio::test]
    async fn test_mock_get_node_detail_not_found() {
        let mock = MockPlatform::new();
        let err = mock.get_node_detail("missing").await.unwrap_err();
        assert!(matches!(err, PlatformError::NodeNotFound(_)));
    }

    #[tokio::test]
    async fn test_mock_execute_command() {
        let mock = MockPlatform::new();
        let result = mock
            .execute_command("test-node", NodeCommand::Start)
            .await;
        assert!(result.is_ok());
    }

    #[tokio::test]
    async fn test_mock_execute_command_not_found() {
        let mock = MockPlatform::new();
        let result = mock.execute_command("missing", NodeCommand::Start).await;
        assert!(result.is_err());
    }

    #[tokio::test]
    async fn test_mock_query_zenoh() {
        let mock = MockPlatform::new();
        let result = mock
            .query_zenoh("bubbaloop/local/test/data")
            .await
            .unwrap();
        assert!(result.contains("mock: query"));
    }

    #[test]
    fn test_platform_error_display() {
        let err = PlatformError::NodeNotFound("foo".to_string());
        assert_eq!(err.to_string(), "Node not found: foo");
    }

    #[test]
    fn test_node_command_variants() {
        // Verify node command variants exist
        let _ = NodeCommand::Start;
        let _ = NodeCommand::Stop;
        let _ = NodeCommand::Restart;
        let _ = NodeCommand::Build;
        let _ = NodeCommand::GetLogs;
    }
}
