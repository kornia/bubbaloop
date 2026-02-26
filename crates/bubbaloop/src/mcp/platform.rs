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
    fn list_nodes(&self)
        -> impl std::future::Future<Output = PlatformResult<Vec<NodeInfo>>> + Send;
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

    /// Send a Zenoh query with a payload (e.g., for node commands).
    ///
    /// Returns the collected reply strings.
    fn send_zenoh_query(
        &self,
        key_expr: &str,
        payload: Vec<u8>,
    ) -> impl std::future::Future<Output = PlatformResult<Vec<String>>> + Send;
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
                let status = NodeStatus::try_from(n.status).unwrap_or(NodeStatus::Unknown);
                let health =
                    HealthStatus::try_from(n.health_status).unwrap_or(HealthStatus::Unknown);
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
                let status = NodeStatus::try_from(node.status).unwrap_or(NodeStatus::Unknown);
                let health =
                    HealthStatus::try_from(node.health_status).unwrap_or(HealthStatus::Unknown);
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

    async fn execute_command(&self, name: &str, cmd: NodeCommand) -> PlatformResult<String> {
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

    async fn send_zenoh_query(
        &self,
        key_expr: &str,
        payload: Vec<u8>,
    ) -> PlatformResult<Vec<String>> {
        match self
            .session
            .get(key_expr)
            .payload(zenoh::bytes::ZBytes::from(payload))
            .timeout(std::time::Duration::from_secs(5))
            .await
        {
            Ok(replies) => {
                let mut results = Vec::new();
                while let Ok(reply) = replies.recv_async().await {
                    match reply.result() {
                        Ok(sample) => {
                            let bytes = sample.payload().to_bytes();
                            match String::from_utf8(bytes.to_vec()) {
                                Ok(text) => results.push(text),
                                Err(_) => results.push(format!("<{} bytes binary>", bytes.len())),
                            }
                        }
                        Err(err) => {
                            results.push(format!("Error: {:?}", err.payload().to_bytes()));
                        }
                    }
                }
                Ok(results)
            }
            Err(e) => Err(PlatformError::Internal(format!(
                "Zenoh query failed: {}",
                e
            ))),
        }
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
                                results.push(format!("[{}] <{} bytes binary>", key, bytes.len()))
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

#[cfg(any(test, feature = "test-harness"))]
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

        async fn execute_command(&self, name: &str, _cmd: NodeCommand) -> PlatformResult<String> {
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

        async fn send_zenoh_query(
            &self,
            key_expr: &str,
            _payload: Vec<u8>,
        ) -> PlatformResult<Vec<String>> {
            Ok(vec![format!("mock: zenoh_query {}", key_expr)])
        }
    }
}

// ── Tests ────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::mock::MockPlatform;
    use super::*;
    use std::collections::HashMap;
    use std::sync::Mutex;

    // ── Helper: build a MockPlatform with custom nodes ───────────────

    fn mock_with_nodes(nodes: Vec<NodeInfo>) -> MockPlatform {
        MockPlatform {
            nodes: Mutex::new(nodes),
            configs: Mutex::new(HashMap::new()),
        }
    }

    // ════════════════════════════════════════════════════════════════════
    // 1. MockPlatform contract tests
    // ════════════════════════════════════════════════════════════════════

    #[tokio::test]
    async fn list_nodes_returns_default_node() {
        let mock = MockPlatform::new();
        let nodes = mock.list_nodes().await.unwrap();
        assert_eq!(nodes.len(), 1);
        assert_eq!(nodes[0].name, "test-node");
        assert_eq!(nodes[0].status, "Running");
        assert_eq!(nodes[0].health, "Healthy");
        assert_eq!(nodes[0].node_type, "rust");
        assert!(nodes[0].installed);
        assert!(nodes[0].is_built);
    }

    #[tokio::test]
    async fn list_nodes_empty() {
        let mock = mock_with_nodes(vec![]);
        let nodes = mock.list_nodes().await.unwrap();
        assert!(nodes.is_empty());
    }

    #[tokio::test]
    async fn list_nodes_multiple() {
        let nodes = vec![
            NodeInfo {
                name: "camera".to_string(),
                status: "Running".to_string(),
                health: "Healthy".to_string(),
                node_type: "python".to_string(),
                installed: true,
                is_built: true,
            },
            NodeInfo {
                name: "detector".to_string(),
                status: "Stopped".to_string(),
                health: "Unknown".to_string(),
                node_type: "rust".to_string(),
                installed: true,
                is_built: false,
            },
            NodeInfo {
                name: "tracker".to_string(),
                status: "Building".to_string(),
                health: "Unknown".to_string(),
                node_type: "python".to_string(),
                installed: false,
                is_built: false,
            },
        ];
        let mock = mock_with_nodes(nodes);
        let result = mock.list_nodes().await.unwrap();
        assert_eq!(result.len(), 3);
        assert_eq!(result[0].name, "camera");
        assert_eq!(result[1].name, "detector");
        assert_eq!(result[2].name, "tracker");
    }

    #[tokio::test]
    async fn get_node_detail_existing() {
        let mock = MockPlatform::new();
        let detail = mock.get_node_detail("test-node").await.unwrap();
        assert_eq!(detail["name"], "test-node");
        assert_eq!(detail["status"], "Running");
        assert_eq!(detail["health"], "Healthy");
        assert_eq!(detail["node_type"], "rust");
        assert_eq!(detail["installed"], true);
        assert_eq!(detail["is_built"], true);
    }

    #[tokio::test]
    async fn get_node_detail_missing() {
        let mock = MockPlatform::new();
        let err = mock.get_node_detail("missing-node").await.unwrap_err();
        match err {
            PlatformError::NodeNotFound(name) => assert_eq!(name, "missing-node"),
            other => panic!("Expected NodeNotFound, got: {:?}", other),
        }
    }

    #[tokio::test]
    async fn get_node_detail_selects_correct_node() {
        let nodes = vec![
            NodeInfo {
                name: "alpha".to_string(),
                status: "Running".to_string(),
                health: "Healthy".to_string(),
                node_type: "rust".to_string(),
                installed: true,
                is_built: true,
            },
            NodeInfo {
                name: "beta".to_string(),
                status: "Stopped".to_string(),
                health: "Unknown".to_string(),
                node_type: "python".to_string(),
                installed: false,
                is_built: false,
            },
        ];
        let mock = mock_with_nodes(nodes);
        let detail = mock.get_node_detail("beta").await.unwrap();
        assert_eq!(detail["name"], "beta");
        assert_eq!(detail["status"], "Stopped");
    }

    #[tokio::test]
    async fn execute_command_start() {
        let mock = MockPlatform::new();
        let msg = mock
            .execute_command("test-node", NodeCommand::Start)
            .await
            .unwrap();
        assert_eq!(msg, "mock: command executed");
    }

    #[tokio::test]
    async fn execute_command_stop() {
        let mock = MockPlatform::new();
        let msg = mock
            .execute_command("test-node", NodeCommand::Stop)
            .await
            .unwrap();
        assert_eq!(msg, "mock: command executed");
    }

    #[tokio::test]
    async fn execute_command_restart() {
        let mock = MockPlatform::new();
        let msg = mock
            .execute_command("test-node", NodeCommand::Restart)
            .await
            .unwrap();
        assert_eq!(msg, "mock: command executed");
    }

    #[tokio::test]
    async fn execute_command_build() {
        let mock = MockPlatform::new();
        let msg = mock
            .execute_command("test-node", NodeCommand::Build)
            .await
            .unwrap();
        assert_eq!(msg, "mock: command executed");
    }

    #[tokio::test]
    async fn execute_command_get_logs() {
        let mock = MockPlatform::new();
        let msg = mock
            .execute_command("test-node", NodeCommand::GetLogs)
            .await
            .unwrap();
        assert_eq!(msg, "mock: command executed");
    }

    #[tokio::test]
    async fn execute_command_missing_node() {
        let mock = MockPlatform::new();
        let err = mock
            .execute_command("ghost", NodeCommand::Start)
            .await
            .unwrap_err();
        match err {
            PlatformError::NodeNotFound(name) => assert_eq!(name, "ghost"),
            other => panic!("Expected NodeNotFound, got: {:?}", other),
        }
    }

    #[tokio::test]
    async fn get_node_config_existing() {
        let mock = MockPlatform::new();
        mock.configs.lock().unwrap().insert(
            "test-node".to_string(),
            serde_json::json!({"fps": 30, "resolution": "1080p"}),
        );
        let config = mock.get_node_config("test-node").await.unwrap();
        assert_eq!(config["fps"], 30);
        assert_eq!(config["resolution"], "1080p");
    }

    #[tokio::test]
    async fn get_node_config_missing() {
        let mock = MockPlatform::new();
        let err = mock.get_node_config("no-config").await.unwrap_err();
        assert!(matches!(err, PlatformError::NodeNotFound(_)));
    }

    #[tokio::test]
    async fn config_round_trip() {
        let mock = MockPlatform::new();
        let original = serde_json::json!({
            "capture_mode": "continuous",
            "interval_ms": 100,
            "enabled": true,
            "tags": ["cam1", "front"],
        });
        mock.configs
            .lock()
            .unwrap()
            .insert("my-cam".to_string(), original.clone());
        let retrieved = mock.get_node_config("my-cam").await.unwrap();
        assert_eq!(retrieved, original);
    }

    #[tokio::test]
    async fn query_zenoh_formats_key() {
        let mock = MockPlatform::new();
        let result = mock
            .query_zenoh("bubbaloop/local/jetson1/openmeteo/status")
            .await
            .unwrap();
        assert_eq!(
            result,
            "mock: query bubbaloop/local/jetson1/openmeteo/status"
        );
    }

    #[tokio::test]
    async fn query_zenoh_wildcard() {
        let mock = MockPlatform::new();
        let result = mock.query_zenoh("bubbaloop/**/manifest").await.unwrap();
        assert!(result.contains("bubbaloop/**/manifest"));
    }

    // ════════════════════════════════════════════════════════════════════
    // 2. Validation integration tests
    // ════════════════════════════════════════════════════════════════════

    // ── validate_node_name ───────────────────────────────────────────

    #[test]
    fn validate_node_name_simple() {
        assert!(crate::validation::validate_node_name("my-node").is_ok());
    }

    #[test]
    fn validate_node_name_with_underscores() {
        assert!(crate::validation::validate_node_name("rtsp_camera_01").is_ok());
    }

    #[test]
    fn validate_node_name_max_length() {
        let name = "a".repeat(64);
        assert!(crate::validation::validate_node_name(&name).is_ok());
    }

    #[test]
    fn validate_node_name_single_char() {
        assert!(crate::validation::validate_node_name("x").is_ok());
    }

    #[test]
    fn validate_node_name_empty_rejected() {
        let err = crate::validation::validate_node_name("").unwrap_err();
        assert!(err.contains("1-64 characters"));
    }

    #[test]
    fn validate_node_name_too_long_rejected() {
        let name = "a".repeat(65);
        let err = crate::validation::validate_node_name(&name).unwrap_err();
        assert!(err.contains("1-64 characters"));
    }

    #[test]
    fn validate_node_name_path_traversal_rejected() {
        assert!(crate::validation::validate_node_name("../../../etc/passwd").is_err());
    }

    #[test]
    fn validate_node_name_special_chars_rejected() {
        assert!(crate::validation::validate_node_name("node;rm -rf /").is_err());
        assert!(crate::validation::validate_node_name("node$HOME").is_err());
        assert!(crate::validation::validate_node_name("node<script>").is_err());
    }

    #[test]
    fn validate_node_name_spaces_rejected() {
        assert!(crate::validation::validate_node_name("my node").is_err());
    }

    #[test]
    fn validate_node_name_slash_rejected() {
        assert!(crate::validation::validate_node_name("a/b").is_err());
    }

    // ── validate_query_key_expr ─────────────────────────────────────

    #[test]
    fn validate_query_key_expr_valid_full_path() {
        assert!(crate::validation::validate_query_key_expr(
            "bubbaloop/local/jetson1/openmeteo/status"
        )
        .is_ok());
    }

    #[test]
    fn validate_query_key_expr_valid_wildcard() {
        assert!(
            crate::validation::validate_query_key_expr("bubbaloop/**/telemetry/status").is_ok()
        );
    }

    #[test]
    fn validate_query_key_expr_wildcard_only_rejected() {
        assert!(crate::validation::validate_query_key_expr("bubbaloop/**").is_err());
        assert!(crate::validation::validate_query_key_expr("bubbaloop/*").is_err());
    }

    #[test]
    fn validate_query_key_expr_missing_prefix() {
        let err = crate::validation::validate_query_key_expr("other/path").unwrap_err();
        assert!(err.contains("bubbaloop/"));
    }

    #[test]
    fn validate_query_key_expr_too_long() {
        let long_key = format!("bubbaloop/{}", "a".repeat(510));
        assert!(crate::validation::validate_query_key_expr(&long_key).is_err());
    }

    #[test]
    fn validate_query_key_expr_empty() {
        assert!(crate::validation::validate_query_key_expr("").is_err());
    }

    // ── validate_rule_name ──────────────────────────────────────────

    #[test]
    fn validate_rule_name_valid() {
        assert!(crate::validation::validate_rule_name("high-temp-alert").is_ok());
        assert!(crate::validation::validate_rule_name("cpu_monitor_01").is_ok());
    }

    #[test]
    fn validate_rule_name_empty_rejected() {
        assert!(crate::validation::validate_rule_name("").is_err());
    }

    #[test]
    fn validate_rule_name_special_chars_rejected() {
        assert!(crate::validation::validate_rule_name("rule with spaces").is_err());
        assert!(crate::validation::validate_rule_name("rule/slash").is_err());
        assert!(crate::validation::validate_rule_name("rule.dot").is_err());
    }

    #[test]
    fn validate_rule_name_too_long_rejected() {
        let name = "r".repeat(65);
        assert!(crate::validation::validate_rule_name(&name).is_err());
    }

    // ── validate_trigger_pattern ────────────────────────────────────

    #[test]
    fn validate_trigger_pattern_valid() {
        assert!(
            crate::validation::validate_trigger_pattern("bubbaloop/**/telemetry/status").is_ok()
        );
        assert!(
            crate::validation::validate_trigger_pattern("bubbaloop/local/node1/metrics").is_ok()
        );
    }

    #[test]
    fn validate_trigger_pattern_empty_rejected() {
        assert!(crate::validation::validate_trigger_pattern("").is_err());
    }

    #[test]
    fn validate_trigger_pattern_wrong_prefix() {
        assert!(crate::validation::validate_trigger_pattern("**").is_err());
        assert!(crate::validation::validate_trigger_pattern("other/topic").is_err());
    }

    #[test]
    fn validate_trigger_pattern_too_long() {
        let long_trigger = format!("bubbaloop/{}", "a".repeat(260));
        assert!(crate::validation::validate_trigger_pattern(&long_trigger).is_err());
    }

    // ── validate_publish_topic ──────────────────────────────────────

    #[test]
    fn validate_publish_topic_valid() {
        assert!(
            crate::validation::validate_publish_topic("bubbaloop/local/jetson1/my-node/data")
                .is_ok()
        );
    }

    #[test]
    fn validate_publish_topic_wildcards_rejected() {
        assert!(crate::validation::validate_publish_topic("bubbaloop/*/data").is_err());
        assert!(crate::validation::validate_publish_topic("bubbaloop/**/data").is_err());
    }

    #[test]
    fn validate_publish_topic_missing_prefix() {
        let err = crate::validation::validate_publish_topic("other/topic").unwrap_err();
        assert!(err.contains("bubbaloop/"));
    }

    #[test]
    fn validate_publish_topic_empty() {
        assert!(crate::validation::validate_publish_topic("").is_err());
    }

    #[test]
    fn validate_publish_topic_invalid_chars() {
        assert!(crate::validation::validate_publish_topic("bubbaloop/bad topic!").is_err());
    }

    // ════════════════════════════════════════════════════════════════════
    // 3. RBAC mapping tests
    // ════════════════════════════════════════════════════════════════════

    #[test]
    fn rbac_all_viewer_tools_mapped() {
        let viewer_tools = [
            "list_nodes",
            "get_node_health",
            "discover_nodes",
            "get_node_manifest",
            "list_commands",
            "get_stream_info",
            "get_system_status",
            "get_machine_info",
            "get_node_schema",
        ];
        for tool in &viewer_tools {
            assert_eq!(
                super::super::rbac::required_tier(tool),
                super::super::rbac::Tier::Viewer,
                "Tool '{}' should be Viewer tier",
                tool
            );
        }
    }

    #[test]
    fn rbac_all_operator_tools_mapped() {
        let operator_tools = [
            "start_node",
            "stop_node",
            "restart_node",
            "get_node_config",
            "send_command",
            "get_node_logs",
        ];
        for tool in &operator_tools {
            assert_eq!(
                super::super::rbac::required_tier(tool),
                super::super::rbac::Tier::Operator,
                "Tool '{}' should be Operator tier",
                tool
            );
        }
    }

    #[test]
    fn rbac_all_admin_tools_mapped() {
        let admin_tools = ["query_zenoh", "install_node", "remove_node", "build_node"];
        for tool in &admin_tools {
            assert_eq!(
                super::super::rbac::required_tier(tool),
                super::super::rbac::Tier::Admin,
                "Tool '{}' should be Admin tier",
                tool
            );
        }
    }

    #[test]
    fn rbac_unknown_tool_defaults_to_admin() {
        assert_eq!(
            super::super::rbac::required_tier("totally_unknown_tool"),
            super::super::rbac::Tier::Admin
        );
        assert_eq!(
            super::super::rbac::required_tier(""),
            super::super::rbac::Tier::Admin
        );
    }

    #[test]
    fn rbac_tier_ordering_consistent() {
        use super::super::rbac::Tier;
        // Admin >= Operator >= Viewer
        assert!(Tier::Admin.has_permission(Tier::Admin));
        assert!(Tier::Admin.has_permission(Tier::Operator));
        assert!(Tier::Admin.has_permission(Tier::Viewer));
        assert!(!Tier::Operator.has_permission(Tier::Admin));
        assert!(Tier::Operator.has_permission(Tier::Operator));
        assert!(Tier::Operator.has_permission(Tier::Viewer));
        assert!(!Tier::Viewer.has_permission(Tier::Admin));
        assert!(!Tier::Viewer.has_permission(Tier::Operator));
        assert!(Tier::Viewer.has_permission(Tier::Viewer));
    }

    // ════════════════════════════════════════════════════════════════════
    // 4. PlatformError tests
    // ════════════════════════════════════════════════════════════════════

    #[test]
    fn error_display_node_not_found() {
        let err = PlatformError::NodeNotFound("my-node".to_string());
        assert_eq!(err.to_string(), "Node not found: my-node");
    }

    #[test]
    fn error_display_command_failed() {
        let err = PlatformError::CommandFailed("build timed out".to_string());
        assert_eq!(err.to_string(), "Command failed: build timed out");
    }

    #[test]
    fn error_display_invalid_input() {
        let err = PlatformError::InvalidInput("bad name".to_string());
        assert_eq!(err.to_string(), "Invalid input: bad name");
    }

    #[test]
    fn error_display_internal() {
        let err = PlatformError::Internal("panic in worker".to_string());
        assert_eq!(err.to_string(), "Internal error: panic in worker");
    }

    #[test]
    fn error_is_debug_send_sync() {
        fn assert_send_sync<T: std::fmt::Debug + Send + Sync>() {}
        assert_send_sync::<PlatformError>();
    }

    #[test]
    fn error_is_std_error() {
        // PlatformError implements std::error::Error via thiserror
        let err: Box<dyn std::error::Error> =
            Box::new(PlatformError::NodeNotFound("x".to_string()));
        assert!(err.to_string().contains("Node not found"));
    }

    #[test]
    fn node_command_all_variants_exist() {
        // Ensure all five command variants are constructible and debug-printable
        let cmds = vec![
            NodeCommand::Start,
            NodeCommand::Stop,
            NodeCommand::Restart,
            NodeCommand::Build,
            NodeCommand::GetLogs,
        ];
        assert_eq!(cmds.len(), 5);
        for cmd in &cmds {
            let debug = format!("{:?}", cmd);
            assert!(!debug.is_empty());
        }
    }

    #[test]
    fn node_info_is_serializable() {
        let info = NodeInfo {
            name: "test".to_string(),
            status: "Running".to_string(),
            health: "Healthy".to_string(),
            node_type: "rust".to_string(),
            installed: true,
            is_built: true,
        };
        let json = serde_json::to_value(&info).unwrap();
        assert_eq!(json["name"], "test");
        assert_eq!(json["status"], "Running");
    }

    #[test]
    fn mock_platform_default_matches_new() {
        let from_new = MockPlatform::new();
        let from_default = MockPlatform::default();
        let nodes_new = from_new.nodes.lock().unwrap();
        let nodes_default = from_default.nodes.lock().unwrap();
        assert_eq!(nodes_new.len(), nodes_default.len());
        assert_eq!(nodes_new[0].name, nodes_default[0].name);
    }
}
