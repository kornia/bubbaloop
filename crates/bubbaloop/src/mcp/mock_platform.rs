//! Mock platform for testing — test-only implementation of PlatformOperations.

use super::platform::{
    AlertInfo, NodeCommand, NodeInfo, PlatformError, PlatformOperations, PlatformResult,
};
use serde_json::Value;
use std::collections::HashMap;
use std::sync::{Arc, Mutex};

pub struct MockPlatform {
    pub nodes: Mutex<Vec<NodeInfo>>,
    pub configs: Mutex<HashMap<String, Value>>,
    pub manifests: Mutex<Vec<(String, Value)>>,
    pub missions: Mutex<Vec<crate::daemon::mission::Mission>>,
    pub alerts: Mutex<Vec<AlertInfo>>,
    pub constraints: Mutex<Vec<(String, String, crate::daemon::constraints::Constraint)>>, // (id, mission_id, constraint)
    pub beliefs: Mutex<Vec<crate::agent::memory::semantic::Belief>>,
    pub world_state: Mutex<Vec<crate::agent::memory::WorldStateEntry>>,
    /// Optional real Zenoh session for e2e tests that need actual pub/sub.
    pub zenoh_session: Option<Arc<zenoh::Session>>,
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
            constraints: Mutex::new(Vec::new()),
            beliefs: Mutex::new(Vec::new()),
            world_state: Mutex::new(Vec::new()),
            manifests: Mutex::new(vec![(
                "test-node".to_string(),
                serde_json::json!({
                    "name": "test-node",
                    "version": "1.0.0",
                    "type": "rust",
                    "description": "A test node",
                    "capabilities": ["sensor"],
                }),
            )]),
            zenoh_session: None,
        }
    }

    /// Attach a real Zenoh session so `publish_to_topic` makes actual pub/sub calls.
    /// Used by e2e tests that verify the full Zenoh delivery path.
    pub fn with_session(mut self, session: Arc<zenoh::Session>) -> Self {
        self.zenoh_session = Some(session);
        self
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
        // Mirror the daemon's default substitution so the in-memory
        // state reflects what would actually be persisted.
        let debounce_secs = params
            .debounce_secs
            .unwrap_or(crate::daemon::reactive::DEFAULT_DEBOUNCE_SECS);
        let arousal_boost = params
            .arousal_boost
            .unwrap_or(crate::daemon::reactive::DEFAULT_AROUSAL_BOOST);
        self.alerts.lock().unwrap().push(AlertInfo {
            id: alert_id.clone(),
            mission_id: params.mission_id,
            predicate: params.predicate,
            debounce_secs,
            arousal_boost,
            description: params.description,
            // The mock doesn't track provider state, so we never
            // report dangling fields — that analysis lives in the
            // daemon implementation.
            dangling_fields: Vec::new(),
        });
        Ok(format!("Alert '{}' registered", alert_id))
    }

    async fn unregister_alert(&self, alert_id: String) -> PlatformResult<String> {
        let mut alerts = self.alerts.lock().unwrap();
        let before = alerts.len();
        alerts.retain(|a| a.id != alert_id);
        if alerts.len() < before {
            Ok(format!("Alert '{}' unregistered", alert_id))
        } else {
            Err(PlatformError::NodeNotFound(format!(
                "Alert '{}' not found",
                alert_id
            )))
        }
    }

    async fn list_alerts(&self, mission_id: Option<String>) -> PlatformResult<Vec<AlertInfo>> {
        let alerts = self.alerts.lock().unwrap();
        let out: Vec<AlertInfo> = match mission_id {
            Some(mid) => alerts
                .iter()
                .filter(|a| a.mission_id == mid)
                .cloned()
                .collect(),
            None => alerts.clone(),
        };
        Ok(out)
    }

    async fn register_constraint(
        &self,
        params: super::platform::RegisterConstraintParams,
    ) -> PlatformResult<String> {
        use crate::daemon::constraints::Constraint;

        let constraint: Constraint = match params.constraint_type.as_str() {
            "max_velocity" => {
                let v: f64 = serde_json::from_str(&params.params_json)
                    .map_err(|e| PlatformError::InvalidInput(format!("invalid param: {}", e)))?;
                Constraint::MaxVelocity(v)
            }
            "workspace" => {
                #[derive(serde::Deserialize)]
                struct W {
                    x: (f64, f64),
                    y: (f64, f64),
                    z: (f64, f64),
                }
                let w: W = serde_json::from_str(&params.params_json)
                    .map_err(|e| PlatformError::InvalidInput(format!("invalid param: {}", e)))?;
                Constraint::Workspace {
                    x: w.x,
                    y: w.y,
                    z: w.z,
                }
            }
            "forbidden_zone" => {
                #[derive(serde::Deserialize)]
                struct Fz {
                    center: [f64; 3],
                    radius: f64,
                }
                let fz: Fz = serde_json::from_str(&params.params_json)
                    .map_err(|e| PlatformError::InvalidInput(format!("invalid param: {}", e)))?;
                Constraint::ForbiddenZone {
                    center: fz.center,
                    radius: fz.radius,
                }
            }
            "max_force" => {
                let v: f64 = serde_json::from_str(&params.params_json)
                    .map_err(|e| PlatformError::InvalidInput(format!("invalid param: {}", e)))?;
                Constraint::MaxForce(v)
            }
            other => {
                return Err(PlatformError::InvalidInput(format!(
                    "unknown constraint type '{}'",
                    other
                )));
            }
        };

        let constraint_id = format!("cst-mock-{}", uuid::Uuid::new_v4());
        self.constraints.lock().unwrap().push((
            constraint_id.clone(),
            params.mission_id.clone(),
            constraint,
        ));
        Ok(format!(
            "Constraint '{}' registered (mission={})",
            constraint_id, params.mission_id
        ))
    }

    async fn list_constraints(
        &self,
        mission_id: String,
    ) -> PlatformResult<Vec<(String, crate::daemon::constraints::Constraint)>> {
        let all = self.constraints.lock().unwrap();
        Ok(all
            .iter()
            .filter(|(_, mid, _)| *mid == mission_id)
            .map(|(id, _, c)| (id.clone(), c.clone()))
            .collect())
    }

    async fn get_belief(
        &self,
        subject: String,
        predicate: String,
    ) -> PlatformResult<Option<crate::agent::memory::semantic::Belief>> {
        let beliefs = self.beliefs.lock().unwrap();
        Ok(beliefs
            .iter()
            .find(|b| b.subject == subject && b.predicate == predicate)
            .cloned())
    }

    async fn update_belief(
        &self,
        params: super::platform::UpdateBeliefParams,
    ) -> PlatformResult<String> {
        let mut beliefs = self.beliefs.lock().unwrap();
        // Remove existing belief with same subject+predicate if any
        beliefs.retain(|b| !(b.subject == params.subject && b.predicate == params.predicate));
        beliefs.push(crate::agent::memory::semantic::Belief {
            id: format!("belief-mock-{}", uuid::Uuid::new_v4()),
            subject: params.subject.clone(),
            predicate: params.predicate.clone(),
            value: params.value,
            confidence: params.confidence,
            source: params.source.unwrap_or_else(|| "mcp".to_string()),
            first_observed: 0,
            last_confirmed: 0,
            confirmation_count: 1,
            contradiction_count: 0,
            notes: params.notes,
        });
        Ok(format!(
            "Belief ({}, {}) updated",
            params.subject, params.predicate
        ))
    }

    async fn list_world_state(&self) -> PlatformResult<Vec<crate::agent::memory::WorldStateEntry>> {
        Ok(self.world_state.lock().unwrap().clone())
    }

    async fn publish_to_topic(&self, topic: &str, message: &str) -> PlatformResult<()> {
        if let Some(ref session) = self.zenoh_session {
            session
                .put(topic, message)
                .await
                .map_err(|e| PlatformError::Internal(format!("Zenoh put failed: {}", e)))?;
        }
        log::debug!("[MockPlatform] publish_to_topic: {}", topic);
        Ok(())
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
            constraints: Mutex::new(Vec::new()),
            beliefs: Mutex::new(Vec::new()),
            world_state: Mutex::new(Vec::new()),
            zenoh_session: None,
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
            "list_constraints",
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
            "register_constraint",
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
    async fn list_alerts_returns_registered_rules() {
        let mock = MockPlatform::new();
        let p = crate::mcp::platform::RegisterAlertParams {
            mission_id: "m1".to_string(),
            predicate: "temp > 100".to_string(),
            debounce_secs: Some(45),
            arousal_boost: Some(3.5),
            description: "hot".to_string(),
        };
        mock.register_alert(p).await.unwrap();

        let listed = mock.list_alerts(None).await.unwrap();
        assert_eq!(listed.len(), 1);
        let a = &listed[0];
        assert_eq!(a.mission_id, "m1");
        assert_eq!(a.predicate, "temp > 100");
        assert_eq!(a.debounce_secs, 45);
        assert!((a.arousal_boost - 3.5).abs() < f64::EPSILON);
        assert_eq!(a.description, "hot");
        // Mock doesn't track providers, so dangling_fields is always empty.
        assert!(a.dangling_fields.is_empty());
    }

    #[tokio::test]
    async fn list_alerts_filters_by_mission() {
        let mock = MockPlatform::new();
        for (mid, pred) in [("m1", "a = 1"), ("m2", "b = 2"), ("m1", "c = 3")] {
            mock.register_alert(crate::mcp::platform::RegisterAlertParams {
                mission_id: mid.to_string(),
                predicate: pred.to_string(),
                debounce_secs: None,
                arousal_boost: None,
                description: String::new(),
            })
            .await
            .unwrap();
        }

        let all = mock.list_alerts(None).await.unwrap();
        assert_eq!(all.len(), 3);

        let only_m1 = mock.list_alerts(Some("m1".to_string())).await.unwrap();
        assert_eq!(only_m1.len(), 2);
        assert!(only_m1.iter().all(|a| a.mission_id == "m1"));
    }

    #[tokio::test]
    async fn list_alerts_applies_defaults_on_register() {
        // When debounce_secs/arousal_boost are omitted, the mock must
        // substitute the same defaults the daemon uses, so introspection
        // reflects the values that would actually take effect.
        let mock = MockPlatform::new();
        mock.register_alert(crate::mcp::platform::RegisterAlertParams {
            mission_id: "m1".to_string(),
            predicate: "x = 1".to_string(),
            debounce_secs: None,
            arousal_boost: None,
            description: String::new(),
        })
        .await
        .unwrap();

        let listed = mock.list_alerts(None).await.unwrap();
        assert_eq!(listed.len(), 1);
        assert_eq!(
            listed[0].debounce_secs,
            crate::daemon::reactive::DEFAULT_DEBOUNCE_SECS
        );
        assert!(
            (listed[0].arousal_boost - crate::daemon::reactive::DEFAULT_AROUSAL_BOOST).abs()
                < f64::EPSILON
        );
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

    // ════════════════════════════════════════════════════════════════════
    // 7. Constraint tests
    // ════════════════════════════════════════════════════════════════════

    #[tokio::test]
    async fn register_constraint_workspace() {
        let mock = MockPlatform::new();
        let params = crate::mcp::platform::RegisterConstraintParams {
            mission_id: "m1".to_string(),
            constraint_type: "workspace".to_string(),
            params_json: r#"{"x":[-1.0,1.0],"y":[-1.0,1.0],"z":[0.0,2.0]}"#.to_string(),
        };
        let msg = mock.register_constraint(params).await.unwrap();
        assert!(msg.contains("cst-mock-"));
        assert!(msg.contains("m1"));

        let constraints = mock.list_constraints("m1".to_string()).await.unwrap();
        assert_eq!(constraints.len(), 1);
    }

    #[tokio::test]
    async fn list_constraints_empty() {
        let mock = MockPlatform::new();
        let constraints = mock
            .list_constraints("nonexistent".to_string())
            .await
            .unwrap();
        assert!(constraints.is_empty());
    }

    #[tokio::test]
    async fn register_constraint_invalid_type() {
        let mock = MockPlatform::new();
        let params = crate::mcp::platform::RegisterConstraintParams {
            mission_id: "m1".to_string(),
            constraint_type: "nonexistent_type".to_string(),
            params_json: "{}".to_string(),
        };
        let err = mock.register_constraint(params).await.unwrap_err();
        assert!(matches!(err, PlatformError::InvalidInput(_)));
    }

    // ════════════════════════════════════════════════════════════════════
    // 8. Belief & world state tests
    // ════════════════════════════════════════════════════════════════════

    #[tokio::test]
    async fn get_belief_not_found_returns_none() {
        let mock = MockPlatform::new();
        let result = mock
            .get_belief("nonexistent".to_string(), "nothing".to_string())
            .await
            .unwrap();
        assert!(result.is_none());
    }

    #[tokio::test]
    async fn update_belief_creates_or_updates() {
        let mock = MockPlatform::new();
        let params = crate::mcp::platform::UpdateBeliefParams {
            subject: "dog".to_string(),
            predicate: "eats_at".to_string(),
            value: "08:00".to_string(),
            confidence: 0.9,
            source: Some("observation".to_string()),
            notes: None,
        };
        let msg = mock.update_belief(params).await.unwrap();
        assert!(msg.contains("dog"));
        assert!(msg.contains("eats_at"));

        // Verify it can be retrieved
        let belief = mock
            .get_belief("dog".to_string(), "eats_at".to_string())
            .await
            .unwrap();
        assert!(belief.is_some());
        let b = belief.unwrap();
        assert_eq!(b.value, "08:00");
        assert!((b.confidence - 0.9).abs() < f64::EPSILON);

        // Update it
        let params2 = crate::mcp::platform::UpdateBeliefParams {
            subject: "dog".to_string(),
            predicate: "eats_at".to_string(),
            value: "09:00".to_string(),
            confidence: 0.95,
            source: None,
            notes: Some("updated".to_string()),
        };
        mock.update_belief(params2).await.unwrap();

        let belief2 = mock
            .get_belief("dog".to_string(), "eats_at".to_string())
            .await
            .unwrap()
            .unwrap();
        assert_eq!(belief2.value, "09:00");
    }

    #[tokio::test]
    async fn list_world_state_returns_snapshot() {
        let mock = MockPlatform::new();
        // Empty by default
        let entries = mock.list_world_state().await.unwrap();
        assert!(entries.is_empty());

        // Add an entry and verify
        mock.world_state
            .lock()
            .unwrap()
            .push(crate::agent::memory::WorldStateEntry {
                key: "cam.status".to_string(),
                value: "online".to_string(),
                confidence: 0.95,
                source_topic: Some("topic/cam".to_string()),
                source_node: None,
                last_seen_at: 1000,
                max_age_secs: 300,
                stale: false,
            });

        let entries = mock.list_world_state().await.unwrap();
        assert_eq!(entries.len(), 1);
        assert_eq!(entries[0].key, "cam.status");
    }
}
