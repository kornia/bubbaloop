//! Mock platform for testing — test-only implementation of PlatformOperations.

use super::platform::{NodeCommand, NodeInfo, PlatformError, PlatformOperations, PlatformResult};
use serde_json::Value;
use std::collections::HashMap;
use std::sync::Mutex;

pub struct MockPlatform {
    pub nodes: Mutex<Vec<NodeInfo>>,
    pub configs: Mutex<HashMap<String, Value>>,
    pub manifests: Mutex<Vec<(String, Value)>>,
    pub missions: Mutex<Vec<crate::daemon::mission::Mission>>,
    pub alerts: Mutex<Vec<(String, String, String)>>, // (id, mission_id, predicate)
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
            missions: Mutex::new(Vec::new()),
            alerts: Mutex::new(Vec::new()),
            manifests: Mutex::new(vec![(
                "test-node".to_string(),
                serde_json::json!({
                    "name": "test-node",
                    "version": "1.0.0",
                    "type": "rust",
                    "description": "A test node",
                    "capabilities": ["sensor"],
                    "publishes": [],
                    "subscribes": [],
                    "commands": [],
                }),
            )]),
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

    async fn execute_command(&self, name: &str, cmd: NodeCommand) -> PlatformResult<String> {
        if self.nodes.lock().unwrap().iter().any(|n| n.name == name) {
            Ok(format!("mock: {:?} executed", cmd))
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

    async fn get_manifests(
        &self,
        capability_filter: Option<&str>,
    ) -> PlatformResult<Vec<(String, Value)>> {
        let all = self.manifests.lock().unwrap().clone();
        let results = all
            .into_iter()
            .filter(|(_name, manifest)| {
                if let Some(filter) = capability_filter {
                    let filter_lower = filter.to_lowercase();
                    manifest
                        .get("capabilities")
                        .and_then(|c| c.as_array())
                        .map(|caps| {
                            caps.iter().any(|cap| {
                                cap.as_str()
                                    .map(|s| s.to_lowercase() == filter_lower)
                                    .unwrap_or(false)
                            })
                        })
                        .unwrap_or(false)
                } else {
                    true
                }
            })
            .collect();
        Ok(results)
    }

    async fn install_node(&self, source: &str) -> PlatformResult<String> {
        Ok(format!("mock: installed node from {}", source))
    }

    async fn add_node(
        &self,
        source: &str,
        name_override: Option<&str>,
        _config_override: Option<&str>,
    ) -> PlatformResult<String> {
        let name = name_override.unwrap_or(source);
        Ok(format!("mock: added node {}", name))
    }

    async fn install_from_marketplace(&self, name: &str) -> PlatformResult<String> {
        Ok(format!("mock: installed '{}' from marketplace", name))
    }

    async fn remove_node(&self, name: &str) -> PlatformResult<String> {
        let mut nodes = self.nodes.lock().unwrap();
        let before = nodes.len();
        nodes.retain(|n| n.name != name);
        if nodes.len() < before {
            Ok(format!("mock: removed node {}", name))
        } else {
            Err(PlatformError::NodeNotFound(name.to_string()))
        }
    }

    async fn list_proposals(&self, status_filter: Option<&str>) -> PlatformResult<String> {
        let filter = status_filter.unwrap_or("all");
        Ok(format!("mock: list proposals (filter={})", filter))
    }

    async fn approve_proposal(&self, id: &str, decided_by: &str) -> PlatformResult<String> {
        Ok(format!(
            "mock: proposal '{}' approved by {}",
            id, decided_by
        ))
    }

    async fn reject_proposal(&self, id: &str, decided_by: &str) -> PlatformResult<String> {
        Ok(format!(
            "mock: proposal '{}' rejected by {}",
            id, decided_by
        ))
    }

    async fn schedule_job(
        &self,
        prompt: &str,
        _cron_schedule: Option<&str>,
        _recurrence: bool,
    ) -> PlatformResult<String> {
        Ok(format!("mock: job scheduled: {}", prompt))
    }

    async fn list_jobs(&self, status_filter: Option<&str>) -> PlatformResult<String> {
        let filter = status_filter.unwrap_or("all");
        Ok(format!("mock: list jobs (filter={})", filter))
    }

    async fn delete_job(&self, id: &str) -> PlatformResult<String> {
        Ok(format!("mock: job '{}' deleted", id))
    }

    async fn clear_episodic_memory(&self, older_than_days: u32) -> PlatformResult<String> {
        Ok(format!(
            "mock: cleared episodic memory older than {} days",
            older_than_days
        ))
    }

    async fn configure_context(
        &self,
        params: super::platform::ConfigureContextParams,
    ) -> PlatformResult<String> {
        if params.topic_pattern.is_empty() {
            return Err(PlatformError::InvalidInput(
                "topic_pattern must not be empty".to_string(),
            ));
        }
        if params.world_state_key_template.is_empty() {
            return Err(PlatformError::InvalidInput(
                "world_state_key_template must not be empty".to_string(),
            ));
        }
        let provider_id = format!("cp-mock-{}", params.mission_id);
        Ok(format!(
            "mock: context provider '{}' configured (mission={})",
            provider_id, params.mission_id
        ))
    }

    async fn list_missions(&self) -> PlatformResult<Vec<crate::daemon::mission::Mission>> {
        Ok(self.missions.lock().unwrap().clone())
    }

    async fn update_mission_status(
        &self,
        mission_id: String,
        status: String,
    ) -> PlatformResult<String> {
        let mut missions = self.missions.lock().unwrap();
        if let Some(m) = missions.iter_mut().find(|m| m.id == mission_id) {
            let parsed: crate::daemon::mission::MissionStatus = status
                .parse()
                .map_err(|e: String| PlatformError::InvalidInput(e))?;
            m.status = parsed;
            Ok(format!("Mission '{}' updated to {}", mission_id, status))
        } else {
            Err(PlatformError::NodeNotFound(format!(
                "Mission '{}' not found",
                mission_id
            )))
        }
    }

    async fn register_alert(
        &self,
        params: super::platform::RegisterAlertParams,
    ) -> PlatformResult<String> {
        let alert_id = format!("alert-mock-{}", uuid::Uuid::new_v4());
        self.alerts
            .lock()
            .unwrap()
            .push((alert_id.clone(), params.mission_id, params.predicate));
        Ok(format!("Alert '{}' registered", alert_id))
    }

    async fn unregister_alert(&self, alert_id: String) -> PlatformResult<String> {
        let mut alerts = self.alerts.lock().unwrap();
        let before = alerts.len();
        alerts.retain(|(id, _, _)| *id != alert_id);
        if alerts.len() < before {
            Ok(format!("Alert '{}' unregistered", alert_id))
        } else {
            Err(PlatformError::NodeNotFound(format!(
                "Alert '{}' not found",
                alert_id
            )))
        }
    }
}

// ── Tests ────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    // ── Helper: build a MockPlatform with custom nodes ───────────────

    fn mock_with_nodes(nodes: Vec<NodeInfo>) -> MockPlatform {
        MockPlatform {
            nodes: Mutex::new(nodes),
            manifests: Mutex::new(Vec::new()),
            configs: Mutex::new(HashMap::new()),
            missions: Mutex::new(Vec::new()),
            alerts: Mutex::new(Vec::new()),
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
        assert_eq!(msg, "mock: Start executed");
    }

    #[tokio::test]
    async fn execute_command_stop() {
        let mock = MockPlatform::new();
        let msg = mock
            .execute_command("test-node", NodeCommand::Stop)
            .await
            .unwrap();
        assert_eq!(msg, "mock: Stop executed");
    }

    #[tokio::test]
    async fn execute_command_restart() {
        let mock = MockPlatform::new();
        let msg = mock
            .execute_command("test-node", NodeCommand::Restart)
            .await
            .unwrap();
        assert_eq!(msg, "mock: Restart executed");
    }

    #[tokio::test]
    async fn execute_command_build() {
        let mock = MockPlatform::new();
        let msg = mock
            .execute_command("test-node", NodeCommand::Build)
            .await
            .unwrap();
        assert_eq!(msg, "mock: Build executed");
    }

    #[tokio::test]
    async fn execute_command_get_logs() {
        let mock = MockPlatform::new();
        let msg = mock
            .execute_command("test-node", NodeCommand::GetLogs)
            .await
            .unwrap();
        assert_eq!(msg, "mock: GetLogs executed");
    }

    #[tokio::test]
    async fn execute_command_install() {
        let mock = MockPlatform::new();
        let msg = mock
            .execute_command("test-node", NodeCommand::Install)
            .await
            .unwrap();
        assert_eq!(msg, "mock: Install executed");
    }

    #[tokio::test]
    async fn execute_command_uninstall() {
        let mock = MockPlatform::new();
        let msg = mock
            .execute_command("test-node", NodeCommand::Uninstall)
            .await
            .unwrap();
        assert_eq!(msg, "mock: Uninstall executed");
    }

    #[tokio::test]
    async fn execute_command_clean() {
        let mock = MockPlatform::new();
        let msg = mock
            .execute_command("test-node", NodeCommand::Clean)
            .await
            .unwrap();
        assert_eq!(msg, "mock: Clean executed");
    }

    #[tokio::test]
    async fn execute_command_enable_autostart() {
        let mock = MockPlatform::new();
        let msg = mock
            .execute_command("test-node", NodeCommand::EnableAutostart)
            .await
            .unwrap();
        assert_eq!(msg, "mock: EnableAutostart executed");
    }

    #[tokio::test]
    async fn execute_command_disable_autostart() {
        let mock = MockPlatform::new();
        let msg = mock
            .execute_command("test-node", NodeCommand::DisableAutostart)
            .await
            .unwrap();
        assert_eq!(msg, "mock: DisableAutostart executed");
    }

    #[tokio::test]
    async fn install_from_marketplace_mock() {
        let mock = MockPlatform::new();
        let msg = mock.install_from_marketplace("rtsp-camera").await.unwrap();
        assert!(msg.contains("rtsp-camera"));
        assert!(msg.contains("marketplace"));
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
            "discover_capabilities",
            "list_jobs",
            "list_missions",
        ];
        for tool in &viewer_tools {
            assert_eq!(
                crate::mcp::rbac::required_tier(tool),
                crate::mcp::rbac::Tier::Viewer,
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
            "enable_autostart",
            "disable_autostart",
            "delete_job",
            "pause_mission",
            "resume_mission",
            "cancel_mission",
        ];
        for tool in &operator_tools {
            assert_eq!(
                crate::mcp::rbac::required_tier(tool),
                crate::mcp::rbac::Tier::Operator,
                "Tool '{}' should be Operator tier",
                tool
            );
        }
    }

    #[test]
    fn rbac_all_admin_tools_mapped() {
        let admin_tools = [
            "query_zenoh",
            "install_node",
            "remove_node",
            "build_node",
            "uninstall_node",
            "clean_node",
            "clear_episodic_memory",
            "register_alert",
            "unregister_alert",
        ];
        for tool in &admin_tools {
            assert_eq!(
                crate::mcp::rbac::required_tier(tool),
                crate::mcp::rbac::Tier::Admin,
                "Tool '{}' should be Admin tier",
                tool
            );
        }
    }

    #[test]
    fn rbac_unknown_tool_defaults_to_admin() {
        assert_eq!(
            crate::mcp::rbac::required_tier("totally_unknown_tool"),
            crate::mcp::rbac::Tier::Admin
        );
        assert_eq!(
            crate::mcp::rbac::required_tier(""),
            crate::mcp::rbac::Tier::Admin
        );
    }

    #[test]
    fn rbac_tier_ordering_consistent() {
        use crate::mcp::rbac::Tier;
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
        // Ensure all ten command variants are constructible and debug-printable
        let cmds = vec![
            NodeCommand::Start,
            NodeCommand::Stop,
            NodeCommand::Restart,
            NodeCommand::Build,
            NodeCommand::GetLogs,
            NodeCommand::Install,
            NodeCommand::Uninstall,
            NodeCommand::Clean,
            NodeCommand::EnableAutostart,
            NodeCommand::DisableAutostart,
        ];
        assert_eq!(cmds.len(), 10);
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

    #[tokio::test]
    async fn list_jobs_mock() {
        let mock = MockPlatform::new();
        let msg = mock.list_jobs(Some("pending")).await.unwrap();
        assert!(msg.contains("pending"));
    }

    #[tokio::test]
    async fn delete_job_mock() {
        let mock = MockPlatform::new();
        let msg = mock.delete_job("job-123").await.unwrap();
        assert!(msg.contains("job-123"));
    }

    #[tokio::test]
    async fn clear_episodic_memory_mock() {
        let mock = MockPlatform::new();
        let msg = mock.clear_episodic_memory(30).await.unwrap();
        assert!(msg.contains("30"));
    }

    #[tokio::test]
    async fn configure_context_tool_validates_empty_topic_pattern() {
        let mock = MockPlatform::new();
        let params = crate::mcp::platform::ConfigureContextParams {
            mission_id: "m1".to_string(),
            topic_pattern: String::new(),
            world_state_key_template: "{label}.location".to_string(),
            value_field: "location".to_string(),
            filter: None,
            min_interval_secs: None,
            max_age_secs: None,
            confidence_field: None,
            token_budget: None,
        };
        let err = mock.configure_context(params).await.unwrap_err();
        assert!(matches!(err, PlatformError::InvalidInput(_)));
    }

    #[tokio::test]
    async fn configure_context_tool_saves_valid_config() {
        let mock = MockPlatform::new();
        let params = crate::mcp::platform::ConfigureContextParams {
            mission_id: "m1".to_string(),
            topic_pattern: "bubbaloop/**/detections".to_string(),
            world_state_key_template: "{label}.location".to_string(),
            value_field: "location".to_string(),
            filter: Some("confidence>0.8".to_string()),
            min_interval_secs: Some(10),
            max_age_secs: Some(120),
            confidence_field: Some("confidence".to_string()),
            token_budget: Some(100),
        };
        let msg = mock.configure_context(params).await.unwrap();
        assert!(msg.contains("cp-mock-"));
        assert!(msg.contains("m1"));
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

    // ════════════════════════════════════════════════════════════════════
    // 5. Mission lifecycle tests
    // ════════════════════════════════════════════════════════════════════

    #[tokio::test]
    async fn list_missions_empty() {
        let mock = MockPlatform::new();
        let missions = mock.list_missions().await.unwrap();
        assert!(missions.is_empty());
    }

    #[tokio::test]
    async fn pause_resume_mission() {
        use crate::daemon::mission::{Mission, MissionStatus};
        let mock = MockPlatform::new();
        mock.missions.lock().unwrap().push(Mission {
            id: "dog-watch".to_string(),
            markdown: "Watch the dog.".to_string(),
            status: MissionStatus::Active,
            expires_at: None,
            resources: vec![],
            sub_mission_ids: vec![],
            depends_on: vec![],
            compiled_at: 1700000000,
        });

        // Pause
        let msg = mock
            .update_mission_status("dog-watch".to_string(), "paused".to_string())
            .await
            .unwrap();
        assert!(msg.contains("paused"));
        assert_eq!(
            mock.missions.lock().unwrap()[0].status,
            MissionStatus::Paused
        );

        // Resume
        let msg = mock
            .update_mission_status("dog-watch".to_string(), "active".to_string())
            .await
            .unwrap();
        assert!(msg.contains("active"));
        assert_eq!(
            mock.missions.lock().unwrap()[0].status,
            MissionStatus::Active
        );
    }

    #[tokio::test]
    async fn cancel_mission_not_found() {
        let mock = MockPlatform::new();
        let err = mock
            .update_mission_status("nonexistent".to_string(), "cancelled".to_string())
            .await
            .unwrap_err();
        assert!(matches!(err, PlatformError::NodeNotFound(_)));
    }

    // ════════════════════════════════════════════════════════════════════
    // 6. Reactive alert tests
    // ════════════════════════════════════════════════════════════════════

    #[tokio::test]
    async fn register_alert_saves_rule() {
        let mock = MockPlatform::new();
        let params = crate::mcp::platform::RegisterAlertParams {
            mission_id: "m1".to_string(),
            predicate: "toddler.near_stairs = true".to_string(),
            debounce_secs: Some(30),
            arousal_boost: Some(3.0),
            description: "Toddler near stairs".to_string(),
        };
        let msg = mock.register_alert(params).await.unwrap();
        assert!(msg.contains("alert-mock-"));
        assert_eq!(mock.alerts.lock().unwrap().len(), 1);
    }

    #[tokio::test]
    async fn unregister_alert_removes_it() {
        let mock = MockPlatform::new();
        let params = crate::mcp::platform::RegisterAlertParams {
            mission_id: "m1".to_string(),
            predicate: "temp > 100".to_string(),
            debounce_secs: None,
            arousal_boost: None,
            description: "High temp".to_string(),
        };
        let msg = mock.register_alert(params).await.unwrap();
        // Extract the alert ID from the response
        let alert_id = msg
            .strip_prefix("Alert '")
            .and_then(|s| s.strip_suffix("' registered"))
            .unwrap()
            .to_string();

        assert_eq!(mock.alerts.lock().unwrap().len(), 1);

        let msg = mock.unregister_alert(alert_id.clone()).await.unwrap();
        assert!(msg.contains("unregistered"));
        assert!(mock.alerts.lock().unwrap().is_empty());
    }

    #[tokio::test]
    async fn unregister_alert_not_found() {
        let mock = MockPlatform::new();
        let err = mock
            .unregister_alert("nonexistent".to_string())
            .await
            .unwrap_err();
        assert!(matches!(err, PlatformError::NodeNotFound(_)));
    }
}
