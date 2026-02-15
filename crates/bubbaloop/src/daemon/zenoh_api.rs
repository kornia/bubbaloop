//! Zenoh REST API for daemon
//!
//! Provides a Zenoh queryable interface that mirrors the HTTP API.
//! Clients can query these key expressions like REST endpoints.
//!
//! ## API Endpoints
//!
//! | Key Expression | Description | Payload |
//! |----------------|-------------|---------|
//! | `bubbaloop/{machine_id}/daemon/api/health` | Health check | None |
//! | `bubbaloop/{machine_id}/daemon/api/nodes` | List all nodes | None |
//! | `bubbaloop/{machine_id}/daemon/api/nodes/add` | Add a node | JSON: `{"node_path": "..."}` |
//! | `bubbaloop/{machine_id}/daemon/api/nodes/{name}` | Get single node | None |
//! | `bubbaloop/{machine_id}/daemon/api/nodes/{name}/logs` | Get node logs | None |
//! | `bubbaloop/{machine_id}/daemon/api/nodes/{name}/command` | Execute command | JSON: `{"command": "start"}` |
//! | `bubbaloop/{machine_id}/daemon/api/refresh` | Refresh all nodes | None |
//! | `bubbaloop/{machine_id}/daemon/api/schemas` | Get protobuf FileDescriptorSet | None |

use crate::daemon::node_manager::NodeManager;
use crate::schemas::daemon::v1::{CommandType, NodeCommand};
use serde::{Deserialize, Serialize};
use std::sync::Arc;
use tokio::sync::watch;
use zenoh::bytes::ZBytes;
use zenoh::Session;

use crate::daemon::util::get_machine_id;

/// Key expressions for API endpoints
pub mod api_keys {
    // Legacy keys (for backward compatibility)
    /// Base prefix for all API endpoints (legacy)
    pub const API_PREFIX_LEGACY: &str = "bubbaloop/daemon/api";

    /// Wildcard for all API endpoints (legacy)
    pub const API_WILDCARD_LEGACY: &str = "bubbaloop/daemon/api/**";

    // New machine-scoped keys
    /// Get base prefix for API with machine_id
    pub fn api_prefix(machine_id: &str) -> String {
        format!("bubbaloop/{}/daemon/api", machine_id)
    }

    /// Get wildcard for all API endpoints with machine_id
    pub fn api_wildcard(machine_id: &str) -> String {
        format!("bubbaloop/{}/daemon/api/**", machine_id)
    }

    /// Get health check key with machine_id
    pub fn health_key(machine_id: &str) -> String {
        format!("bubbaloop/{}/daemon/api/health", machine_id)
    }

    /// Get nodes list key with machine_id
    pub fn nodes_key(machine_id: &str) -> String {
        format!("bubbaloop/{}/daemon/api/nodes", machine_id)
    }

    /// Get nodes add key with machine_id
    pub fn nodes_add_key(machine_id: &str) -> String {
        format!("bubbaloop/{}/daemon/api/nodes/add", machine_id)
    }

    /// Get refresh key with machine_id
    pub fn refresh_key(machine_id: &str) -> String {
        format!("bubbaloop/{}/daemon/api/refresh", machine_id)
    }
}

/// JSON response for node state
#[derive(Serialize, Deserialize)]
pub struct NodeStateResponse {
    pub name: String,
    pub path: String,
    pub status: String,
    pub installed: bool,
    pub autostart_enabled: bool,
    pub version: String,
    pub description: String,
    pub node_type: String,
    pub is_built: bool,
    pub build_output: Vec<String>,
    pub base_node: String,
    #[serde(default)]
    pub config_override: String,
    #[serde(default)]
    pub last_updated_ms: i64,
    #[serde(default)]
    pub health_status: String,
    #[serde(default)]
    pub last_health_check_ms: i64,
    #[serde(default)]
    pub machine_id: String,
    #[serde(default)]
    pub machine_hostname: String,
    #[serde(default)]
    pub machine_ips: Vec<String>,
}

/// JSON response for node list
#[derive(Serialize, Deserialize)]
pub struct NodeListResponse {
    pub nodes: Vec<NodeStateResponse>,
    pub timestamp_ms: i64,
    #[serde(default)]
    pub machine_id: String,
}

/// JSON request for commands
#[derive(Serialize, Deserialize)]
pub struct CommandRequest {
    pub command: String,
    #[serde(default)]
    pub node_path: String,
    /// Instance name override (for multi-instance nodes)
    #[serde(default)]
    pub name: Option<String>,
    /// Config file path override (for multi-instance nodes)
    #[serde(default)]
    pub config: Option<String>,
}

/// JSON response for commands
#[derive(Serialize, Deserialize)]
pub struct CommandResponse {
    pub success: bool,
    pub message: String,
    pub output: String,
    pub node_state: Option<NodeStateResponse>,
}

/// JSON response for logs
#[derive(Serialize, Deserialize)]
pub struct LogsResponse {
    pub node_name: String,
    pub lines: Vec<String>,
    pub success: bool,
    pub error: Option<String>,
}

/// JSON response for health check
#[derive(Serialize, Deserialize)]
pub struct HealthResponse {
    pub status: String,
}

/// JSON error response
#[derive(Serialize, Deserialize)]
pub struct ErrorResponse {
    pub error: String,
    pub code: u16,
}

fn status_to_string(status: i32) -> &'static str {
    match status {
        0 => "unknown",
        1 => "stopped",
        2 => "running",
        3 => "failed",
        4 => "installing",
        5 => "building",
        6 => "not-installed",
        _ => "unknown",
    }
}

fn health_status_to_string(health_status: i32) -> &'static str {
    match health_status {
        0 => "unknown",
        1 => "healthy",
        2 => "unhealthy",
        _ => "unknown",
    }
}

fn node_state_to_response(state: &crate::schemas::daemon::v1::NodeState) -> NodeStateResponse {
    NodeStateResponse {
        name: state.name.clone(),
        path: state.path.clone(),
        status: status_to_string(state.status).to_string(),
        installed: state.installed,
        autostart_enabled: state.autostart_enabled,
        version: state.version.clone(),
        description: state.description.clone(),
        node_type: state.node_type.clone(),
        is_built: state.is_built,
        build_output: state.build_output.clone(),
        base_node: state.base_node.clone(),
        config_override: state.config_override.clone(),
        last_updated_ms: state.last_updated_ms,
        health_status: health_status_to_string(state.health_status).to_string(),
        last_health_check_ms: state.last_health_check_ms,
        machine_id: state.machine_id.clone(),
        machine_hostname: state.machine_hostname.clone(),
        machine_ips: state.machine_ips.clone(),
    }
}

fn parse_command(cmd: &str) -> Option<CommandType> {
    match cmd.to_lowercase().as_str() {
        "start" => Some(CommandType::Start),
        "stop" => Some(CommandType::Stop),
        "restart" => Some(CommandType::Restart),
        "install" => Some(CommandType::Install),
        "uninstall" => Some(CommandType::Uninstall),
        "build" => Some(CommandType::Build),
        "clean" => Some(CommandType::Clean),
        "enable" | "enable_autostart" | "enable-autostart" => Some(CommandType::EnableAutostart),
        "disable" | "disable_autostart" | "disable-autostart" => {
            Some(CommandType::DisableAutostart)
        }
        "add" | "add_node" => Some(CommandType::AddNode),
        "remove" | "remove_node" => Some(CommandType::RemoveNode),
        "refresh" => Some(CommandType::Refresh),
        "logs" | "get_logs" | "get-logs" => Some(CommandType::GetLogs),
        _ => None,
    }
}

/// Zenoh API service for the daemon
pub struct ZenohApiService {
    session: Arc<Session>,
    node_manager: Arc<NodeManager>,
    machine_id: String,
}

impl ZenohApiService {
    /// Create a new Zenoh API service
    pub fn new(session: Arc<Session>, node_manager: Arc<NodeManager>) -> Self {
        let machine_id = get_machine_id();
        log::info!("Zenoh API service using machine_id: {}", machine_id);
        Self {
            session,
            node_manager,
            machine_id,
        }
    }

    /// Get current timestamp in milliseconds
    fn now_ms() -> i64 {
        use std::time::{SystemTime, UNIX_EPOCH};
        SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_millis() as i64
    }

    /// Run the Zenoh API service
    pub async fn run(self, mut shutdown: watch::Receiver<()>) -> Result<(), zenoh::Error> {
        log::info!("Starting Zenoh API service...");

        // Declare queryables on both legacy and new paths for backward compatibility.
        // NOTE: We intentionally do NOT use .complete(true) here. That flag tells Zenoh
        // this queryable is authoritative for the key expression, which would block
        // wildcard queries (e.g., "bubbaloop/**/schema") from reaching node queryables.

        // Legacy path
        let queryable_legacy = self
            .session
            .declare_queryable(api_keys::API_WILDCARD_LEGACY)
            .await?;

        log::info!(
            "Declared queryable on {} for REST-like API (legacy)",
            api_keys::API_WILDCARD_LEGACY
        );

        // New machine-scoped path
        let new_wildcard = api_keys::api_wildcard(&self.machine_id);
        let queryable_new = self.session.declare_queryable(&new_wildcard).await?;

        log::info!(
            "Declared queryable on {} for REST-like API (machine-scoped)",
            new_wildcard
        );

        loop {
            tokio::select! {
                // Handle shutdown
                _ = shutdown.changed() => {
                    log::info!("Zenoh API service shutting down...");
                    break;
                }

                // Handle incoming queries on legacy path
                query = queryable_legacy.recv_async() => {
                    match query {
                        Ok(query) => {
                            self.handle_query(&query).await;
                        }
                        Err(e) => {
                            log::warn!("Query receive error (legacy): {}", e);
                        }
                    }
                }

                // Handle incoming queries on new path
                query = queryable_new.recv_async() => {
                    match query {
                        Ok(query) => {
                            self.handle_query(&query).await;
                        }
                        Err(e) => {
                            log::warn!("Query receive error (new): {}", e);
                        }
                    }
                }
            }
        }

        Ok(())
    }

    /// Handle an incoming query by routing to the appropriate handler
    async fn handle_query(&self, query: &zenoh::query::Query) {
        let key_expr = query.key_expr().as_str();
        log::info!("API query received on: {}", key_expr);

        // Try to parse path from either legacy or new format
        let path = if let Some(p) = key_expr.strip_prefix(api_keys::API_PREFIX_LEGACY) {
            p.trim_start_matches('/')
        } else if let Some(p) = key_expr.strip_prefix(&api_keys::api_prefix(&self.machine_id)) {
            p.trim_start_matches('/')
        } else {
            // Fallback - just use the key as-is
            log::warn!("Query path doesn't match expected prefix: {}", key_expr);
            key_expr
        };

        // Route to appropriate handler
        // The "schemas" endpoint returns raw bytes (FileDescriptorSet), not JSON
        if path == "schemas" {
            self.handle_schemas(query).await;
            return;
        }

        let response = match path {
            "health" => self.handle_health().await,
            "nodes" => self.handle_list_nodes().await,
            "nodes/add" => self.handle_add_node(query).await,
            "refresh" => self.handle_refresh().await,
            _ if path.starts_with("nodes/") => {
                // Parse /nodes/{name} or /nodes/{name}/logs or /nodes/{name}/command
                let remaining = path.strip_prefix("nodes/").unwrap();
                self.handle_node_request(remaining, query).await
            }
            _ => {
                // Unknown endpoint
                serde_json::to_string(&ErrorResponse {
                    error: format!("Unknown endpoint: {}", path),
                    code: 404,
                })
                .unwrap_or_else(|_| r#"{"error":"Unknown endpoint","code":404}"#.to_string())
            }
        };

        // Send reply
        match query
            .reply(query.key_expr(), ZBytes::from(response.clone()))
            .await
        {
            Ok(_) => log::debug!("Reply sent for {}", key_expr),
            Err(e) => log::error!("Failed to send reply for {}: {}", key_expr, e),
        }
    }

    /// Handle GET /schemas — returns raw FileDescriptorSet bytes
    async fn handle_schemas(&self, query: &zenoh::query::Query) {
        let key_expr = query.key_expr().as_str();
        let descriptor_bytes = crate::DESCRIPTOR;
        match query
            .reply(query.key_expr(), ZBytes::from(descriptor_bytes.to_vec()))
            .await
        {
            Ok(_) => log::debug!("Schemas reply sent for {}", key_expr),
            Err(e) => log::error!("Failed to send schemas reply for {}: {}", key_expr, e),
        }
    }

    /// Handle GET /health
    async fn handle_health(&self) -> String {
        serde_json::to_string(&HealthResponse {
            status: "ok".to_string(),
        })
        .unwrap_or_else(|_| r#"{"status":"ok"}"#.to_string())
    }

    /// Handle GET /nodes
    async fn handle_list_nodes(&self) -> String {
        let list = self.node_manager.get_node_list().await;
        let response = NodeListResponse {
            nodes: list.nodes.iter().map(node_state_to_response).collect(),
            timestamp_ms: list.timestamp_ms,
            machine_id: self.machine_id.clone(),
        };
        serde_json::to_string(&response)
            .unwrap_or_else(|e| format!(r#"{{"error":"Failed to serialize: {}","code":500}}"#, e))
    }

    /// Handle POST /nodes/add
    async fn handle_add_node(&self, query: &zenoh::query::Query) -> String {
        let request: CommandRequest = match self.get_json_payload(query) {
            Ok(req) => req,
            Err(e) => {
                return serde_json::to_string(&ErrorResponse {
                    error: e,
                    code: 400,
                })
                .unwrap_or_else(|_| r#"{"error":"Bad request","code":400}"#.to_string())
            }
        };

        let cmd = NodeCommand {
            command: CommandType::AddNode as i32,
            node_name: String::new(),
            node_path: request.node_path,
            request_id: uuid::Uuid::new_v4().to_string(),
            timestamp_ms: Self::now_ms(),
            source_machine: "api".to_string(), // API calls don't have a source machine
            target_machine: self.machine_id.clone(),
            name_override: request.name.unwrap_or_default(),
            config_override: request.config.unwrap_or_default(),
        };

        let result = self.node_manager.execute_command(cmd).await;

        serde_json::to_string(&CommandResponse {
            success: result.success,
            message: result.message,
            output: result.output,
            node_state: result.node_state.as_ref().map(node_state_to_response),
        })
        .unwrap_or_else(|e| format!(r#"{{"error":"Failed to serialize: {}","code":500}}"#, e))
    }

    /// Handle POST /refresh
    async fn handle_refresh(&self) -> String {
        let cmd = NodeCommand {
            command: CommandType::Refresh as i32,
            node_name: String::new(),
            node_path: String::new(),
            request_id: uuid::Uuid::new_v4().to_string(),
            timestamp_ms: Self::now_ms(),
            source_machine: "api".to_string(),
            target_machine: self.machine_id.clone(),
            name_override: String::new(),
            config_override: String::new(),
        };

        let result = self.node_manager.execute_command(cmd).await;

        serde_json::to_string(&CommandResponse {
            success: result.success,
            message: result.message,
            output: result.output,
            node_state: None,
        })
        .unwrap_or_else(|e| format!(r#"{{"error":"Failed to serialize: {}","code":500}}"#, e))
    }

    /// Handle requests to /nodes/{name}/*
    async fn handle_node_request(&self, path: &str, query: &zenoh::query::Query) -> String {
        // Parse path segments
        let parts: Vec<&str> = path.split('/').collect();

        match parts.as_slice() {
            // /nodes/{name}
            [name] => self.handle_get_node(name).await,
            // /nodes/{name}/logs
            [name, "logs"] => self.handle_get_logs(name).await,
            // /nodes/{name}/command
            [name, "command"] => self.handle_execute_command(name, query).await,
            _ => serde_json::to_string(&ErrorResponse {
                error: format!("Unknown node endpoint: {}", path),
                code: 404,
            })
            .unwrap_or_else(|_| r#"{"error":"Unknown endpoint","code":404}"#.to_string()),
        }
    }

    /// Handle GET /nodes/{name}
    async fn handle_get_node(&self, name: &str) -> String {
        match self.node_manager.get_node(name).await {
            Some(node) => {
                serde_json::to_string(&node_state_to_response(&node)).unwrap_or_else(|e| {
                    format!(r#"{{"error":"Failed to serialize: {}","code":500}}"#, e)
                })
            }
            None => serde_json::to_string(&ErrorResponse {
                error: format!("Node '{}' not found", name),
                code: 404,
            })
            .unwrap_or_else(|_| r#"{"error":"Not found","code":404}"#.to_string()),
        }
    }

    /// Handle GET /nodes/{name}/logs
    ///
    /// Delegates to node_manager via the GetLogs command instead of spawning
    /// systemctl directly (which violates the zbus-only convention).
    async fn handle_get_logs(&self, name: &str) -> String {
        if self.node_manager.get_node(name).await.is_none() {
            return serde_json::to_string(&ErrorResponse {
                error: format!("Node '{}' not found", name),
                code: 404,
            })
            .unwrap_or_else(|_| r#"{"error":"Not found","code":404}"#.to_string());
        }

        let cmd = NodeCommand {
            command: CommandType::GetLogs as i32,
            node_name: name.to_string(),
            node_path: String::new(),
            request_id: uuid::Uuid::new_v4().to_string(),
            timestamp_ms: Self::now_ms(),
            source_machine: "api".to_string(),
            target_machine: self.machine_id.clone(),
            name_override: String::new(),
            config_override: String::new(),
        };

        let result = self.node_manager.execute_command(cmd).await;

        let lines: Vec<String> = if result.output.is_empty() {
            vec![]
        } else {
            result.output.lines().map(|s| s.to_string()).collect()
        };

        serde_json::to_string(&LogsResponse {
            node_name: name.to_string(),
            lines,
            success: result.success,
            error: if result.success {
                None
            } else {
                Some(result.message)
            },
        })
        .unwrap_or_else(|e| format!(r#"{{"error":"Failed to serialize: {}","code":500}}"#, e))
    }

    /// Handle POST /nodes/{name}/command
    async fn handle_execute_command(&self, name: &str, query: &zenoh::query::Query) -> String {
        let request: CommandRequest = match self.get_json_payload(query) {
            Ok(req) => req,
            Err(e) => {
                return serde_json::to_string(&ErrorResponse {
                    error: e,
                    code: 400,
                })
                .unwrap_or_else(|_| r#"{"error":"Bad request","code":400}"#.to_string())
            }
        };

        let command_type = match parse_command(&request.command) {
            Some(ct) => ct,
            None => {
                return serde_json::to_string(&ErrorResponse {
                    error: format!("Unknown command: '{}'. Valid commands: start, stop, restart, install, uninstall, build, clean, enable, disable, add, remove, refresh, logs", request.command),
                    code: 400,
                })
                .unwrap_or_else(|_| r#"{"error":"Unknown command","code":400}"#.to_string())
            }
        };

        let cmd = NodeCommand {
            command: command_type as i32,
            node_name: name.to_string(),
            node_path: request.node_path,
            request_id: uuid::Uuid::new_v4().to_string(),
            timestamp_ms: Self::now_ms(),
            source_machine: "api".to_string(),
            target_machine: self.machine_id.clone(),
            name_override: request.name.unwrap_or_default(),
            config_override: request.config.unwrap_or_default(),
        };

        let result = self.node_manager.execute_command(cmd).await;

        serde_json::to_string(&CommandResponse {
            success: result.success,
            message: result.message,
            output: result.output,
            node_state: result.node_state.as_ref().map(node_state_to_response),
        })
        .unwrap_or_else(|e| format!(r#"{{"error":"Failed to serialize: {}","code":500}}"#, e))
    }

    /// Extract JSON payload from query
    fn get_json_payload<T: for<'de> Deserialize<'de>>(
        &self,
        query: &zenoh::query::Query,
    ) -> Result<T, String> {
        let payload = query
            .payload()
            .ok_or_else(|| "No payload in query".to_string())?;

        let bytes = payload.to_bytes();
        serde_json::from_slice(&bytes).map_err(|e| format!("Failed to parse JSON: {}", e))
    }
}

/// Create and run the Zenoh API server
///
/// This replaces the HTTP server with a Zenoh queryable-based API.
/// Clients can query `bubbaloop/daemon/api/*` endpoints with JSON payloads.
pub async fn run_zenoh_api_server(
    session: Arc<Session>,
    node_manager: Arc<NodeManager>,
    shutdown: watch::Receiver<()>,
) -> Result<(), zenoh::Error> {
    let service = ZenohApiService::new(session, node_manager);
    service.run(shutdown).await
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::schemas::daemon::v1::{CommandType, NodeState, NodeStatus};

    // status_to_string tests
    #[test]
    fn test_status_to_string_all_known() {
        assert_eq!(status_to_string(0), "unknown");
        assert_eq!(status_to_string(1), "stopped");
        assert_eq!(status_to_string(2), "running");
        assert_eq!(status_to_string(3), "failed");
        assert_eq!(status_to_string(4), "installing");
        assert_eq!(status_to_string(5), "building");
        assert_eq!(status_to_string(6), "not-installed");
        // Guard: catches proto enum additions
        assert_eq!(NodeStatus::NotInstalled as i32, 6);
    }

    #[test]
    fn test_status_to_string_unknown_values() {
        assert_eq!(status_to_string(7), "unknown");
        assert_eq!(status_to_string(-1), "unknown");
        assert_eq!(status_to_string(100), "unknown");
    }

    // parse_command tests
    fn assert_cmd(input: &str, expected: CommandType) {
        assert_eq!(
            parse_command(input).unwrap() as i32,
            expected as i32,
            "parse_command({input:?})"
        );
    }

    #[test]
    fn test_parse_command_basic() {
        assert_cmd("start", CommandType::Start);
        assert_cmd("stop", CommandType::Stop);
        assert_cmd("restart", CommandType::Restart);
        assert_cmd("install", CommandType::Install);
        assert_cmd("uninstall", CommandType::Uninstall);
        assert_cmd("build", CommandType::Build);
        assert_cmd("clean", CommandType::Clean);
        assert_cmd("refresh", CommandType::Refresh);
    }

    #[test]
    fn test_parse_command_case_insensitive() {
        for input in ["START", "Start", "sTaRt"] {
            assert_cmd(input, CommandType::Start);
        }
        assert_cmd("STOP", CommandType::Stop);
        assert_cmd("ReStArT", CommandType::Restart);
    }

    #[test]
    fn test_parse_command_aliases() {
        for input in ["enable", "enable_autostart", "enable-autostart"] {
            assert_cmd(input, CommandType::EnableAutostart);
        }
        for input in ["disable", "disable_autostart", "disable-autostart"] {
            assert_cmd(input, CommandType::DisableAutostart);
        }
        for input in ["add", "add_node"] {
            assert_cmd(input, CommandType::AddNode);
        }
        for input in ["remove", "remove_node"] {
            assert_cmd(input, CommandType::RemoveNode);
        }
    }

    #[test]
    fn test_parse_command_unknown_returns_none() {
        for input in ["unknown_command", "foo", "", "arbitrary-string"] {
            assert!(
                parse_command(input).is_none(),
                "expected None for {input:?}"
            );
        }
    }

    // node_state_to_response tests
    #[test]
    fn test_node_state_to_response_maps_fields() {
        let state = NodeState {
            name: "test-node".to_string(),
            path: "/path/to/node".to_string(),
            status: NodeStatus::Running as i32,
            installed: true,
            autostart_enabled: true,
            version: "1.0.0".to_string(),
            description: "A test node".to_string(),
            node_type: "rust".to_string(),
            is_built: true,
            build_output: vec!["line1".to_string()],
            base_node: "base".to_string(),
            config_override: "/etc/config.yaml".to_string(),
            ..Default::default()
        };
        let resp = node_state_to_response(&state);
        assert_eq!(resp.name, "test-node");
        assert_eq!(resp.path, "/path/to/node");
        assert_eq!(resp.status, "running");
        assert!(resp.installed);
        assert!(resp.autostart_enabled);
        assert_eq!(resp.version, "1.0.0");
        assert_eq!(resp.description, "A test node");
        assert_eq!(resp.node_type, "rust");
        assert!(resp.is_built);
        assert_eq!(resp.build_output, vec!["line1"]);
        assert_eq!(resp.base_node, "base");
        assert_eq!(resp.config_override, "/etc/config.yaml");
    }

    #[test]
    fn test_node_state_to_response_default_status() {
        let state = NodeState::default();
        let resp = node_state_to_response(&state);
        assert_eq!(resp.status, "unknown"); // status 0 = unknown
    }

    // api_keys module tests
    #[test]
    fn test_api_keys_legacy_constants() {
        assert_eq!(api_keys::API_PREFIX_LEGACY, "bubbaloop/daemon/api");
        assert_eq!(api_keys::API_WILDCARD_LEGACY, "bubbaloop/daemon/api/**");
    }

    #[test]
    fn test_api_keys_machine_scoped() {
        let machine_id = "my-machine";

        assert_eq!(
            api_keys::api_prefix(machine_id),
            "bubbaloop/my-machine/daemon/api"
        );
        assert_eq!(
            api_keys::api_wildcard(machine_id),
            "bubbaloop/my-machine/daemon/api/**"
        );
        assert_eq!(
            api_keys::health_key(machine_id),
            "bubbaloop/my-machine/daemon/api/health"
        );
        assert_eq!(
            api_keys::nodes_key(machine_id),
            "bubbaloop/my-machine/daemon/api/nodes"
        );
        assert_eq!(
            api_keys::nodes_add_key(machine_id),
            "bubbaloop/my-machine/daemon/api/nodes/add"
        );
        assert_eq!(
            api_keys::refresh_key(machine_id),
            "bubbaloop/my-machine/daemon/api/refresh"
        );
    }

    #[test]
    fn test_api_keys_special_machine_id() {
        let machine_id = "jetson_01-v2";

        assert_eq!(
            api_keys::api_prefix(machine_id),
            "bubbaloop/jetson_01-v2/daemon/api"
        );
        assert_eq!(
            api_keys::api_wildcard(machine_id),
            "bubbaloop/jetson_01-v2/daemon/api/**"
        );
        assert_eq!(
            api_keys::health_key(machine_id),
            "bubbaloop/jetson_01-v2/daemon/api/health"
        );
        assert_eq!(
            api_keys::nodes_key(machine_id),
            "bubbaloop/jetson_01-v2/daemon/api/nodes"
        );
        assert_eq!(
            api_keys::nodes_add_key(machine_id),
            "bubbaloop/jetson_01-v2/daemon/api/nodes/add"
        );
        assert_eq!(
            api_keys::refresh_key(machine_id),
            "bubbaloop/jetson_01-v2/daemon/api/refresh"
        );
    }

    #[test]
    fn test_parse_command_logs_aliases() {
        for input in ["logs", "get_logs", "get-logs", "LOGS", "Get-Logs"] {
            assert_cmd(input, CommandType::GetLogs);
        }
    }

    // Serde round-trip tests
    #[test]
    fn test_node_state_response_serde_roundtrip() {
        let resp = NodeStateResponse {
            name: "camera".to_string(),
            path: "/opt/nodes/camera".to_string(),
            status: "running".to_string(),
            installed: true,
            autostart_enabled: false,
            version: "1.2.3".to_string(),
            description: "Camera node".to_string(),
            node_type: "rust".to_string(),
            is_built: true,
            build_output: vec!["ok".to_string()],
            base_node: "".to_string(),
            config_override: "".to_string(),
            last_updated_ms: 1234567890,
            health_status: "healthy".to_string(),
            last_health_check_ms: 1234567800,
            machine_id: "jetson-01".to_string(),
            machine_hostname: "jetson-01.local".to_string(),
            machine_ips: vec!["192.168.1.10".to_string()],
        };
        let json = serde_json::to_string(&resp).unwrap();
        let deser: NodeStateResponse = serde_json::from_str(&json).unwrap();
        assert_eq!(deser.name, "camera");
        assert_eq!(deser.status, "running");
        assert!(deser.installed);
        assert_eq!(deser.last_updated_ms, 1234567890);
        assert_eq!(deser.health_status, "healthy");
        assert_eq!(deser.machine_id, "jetson-01");
    }

    #[test]
    fn test_node_list_response_serde_roundtrip() {
        let resp = NodeListResponse {
            nodes: vec![],
            timestamp_ms: 1234567890,
            machine_id: "jetson-01".to_string(),
        };
        let json = serde_json::to_string(&resp).unwrap();
        let deser: NodeListResponse = serde_json::from_str(&json).unwrap();
        assert_eq!(deser.timestamp_ms, 1234567890);
        assert_eq!(deser.machine_id, "jetson-01");
        assert!(deser.nodes.is_empty());
    }

    #[test]
    fn test_node_list_response_machine_id_default() {
        // machine_id should default to empty string when missing (backward compat)
        let json = r#"{"nodes":[],"timestamp_ms":0}"#;
        let deser: NodeListResponse = serde_json::from_str(json).unwrap();
        assert_eq!(deser.machine_id, "");
    }

    #[test]
    fn test_command_request_serde_roundtrip() {
        let req = CommandRequest {
            command: "start".to_string(),
            node_path: "/path".to_string(),
            name: Some("my-instance".to_string()),
            config: None,
        };
        let json = serde_json::to_string(&req).unwrap();
        let deser: CommandRequest = serde_json::from_str(&json).unwrap();
        assert_eq!(deser.command, "start");
        assert_eq!(deser.node_path, "/path");
        assert_eq!(deser.name, Some("my-instance".to_string()));
        assert_eq!(deser.config, None);
    }

    #[test]
    fn test_command_request_minimal() {
        // Only command is required; others default
        let json = r#"{"command":"stop"}"#;
        let deser: CommandRequest = serde_json::from_str(json).unwrap();
        assert_eq!(deser.command, "stop");
        assert_eq!(deser.node_path, "");
        assert_eq!(deser.name, None);
        assert_eq!(deser.config, None);
    }

    #[test]
    fn test_command_response_serde_roundtrip() {
        let resp = CommandResponse {
            success: true,
            message: "Started".to_string(),
            output: "ok\n".to_string(),
            node_state: None,
        };
        let json = serde_json::to_string(&resp).unwrap();
        let deser: CommandResponse = serde_json::from_str(&json).unwrap();
        assert!(deser.success);
        assert_eq!(deser.message, "Started");
        assert!(deser.node_state.is_none());
    }

    #[test]
    fn test_logs_response_serde_roundtrip() {
        let resp = LogsResponse {
            node_name: "weather".to_string(),
            lines: vec!["line1".to_string(), "line2".to_string()],
            success: true,
            error: None,
        };
        let json = serde_json::to_string(&resp).unwrap();
        let deser: LogsResponse = serde_json::from_str(&json).unwrap();
        assert_eq!(deser.node_name, "weather");
        assert_eq!(deser.lines.len(), 2);
        assert!(deser.success);
        assert!(deser.error.is_none());
    }

    #[test]
    fn test_logs_response_with_error() {
        let resp = LogsResponse {
            node_name: "bad".to_string(),
            lines: vec![],
            success: false,
            error: Some("Node not found".to_string()),
        };
        let json = serde_json::to_string(&resp).unwrap();
        let deser: LogsResponse = serde_json::from_str(&json).unwrap();
        assert!(!deser.success);
        assert_eq!(deser.error, Some("Node not found".to_string()));
    }

    #[test]
    fn test_health_response_serde_roundtrip() {
        let resp = HealthResponse {
            status: "ok".to_string(),
        };
        let json = serde_json::to_string(&resp).unwrap();
        let deser: HealthResponse = serde_json::from_str(&json).unwrap();
        assert_eq!(deser.status, "ok");
    }

    #[test]
    fn test_error_response_serde_roundtrip() {
        let resp = ErrorResponse {
            error: "Not found".to_string(),
            code: 404,
        };
        let json = serde_json::to_string(&resp).unwrap();
        let deser: ErrorResponse = serde_json::from_str(&json).unwrap();
        assert_eq!(deser.error, "Not found");
        assert_eq!(deser.code, 404);
    }
}
