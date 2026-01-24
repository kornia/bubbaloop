//! Zenoh REST API for daemon
//!
//! Provides a Zenoh queryable interface that mirrors the HTTP API.
//! Clients can query these key expressions like REST endpoints.
//!
//! ## API Endpoints
//!
//! | Key Expression | Description | Payload |
//! |----------------|-------------|---------|
//! | `bubbaloop/daemon/api/health` | Health check | None |
//! | `bubbaloop/daemon/api/nodes` | List all nodes | None |
//! | `bubbaloop/daemon/api/nodes/add` | Add a node | JSON: `{"node_path": "..."}` |
//! | `bubbaloop/daemon/api/nodes/{name}` | Get single node | None |
//! | `bubbaloop/daemon/api/nodes/{name}/logs` | Get node logs | None |
//! | `bubbaloop/daemon/api/nodes/{name}/command` | Execute command | JSON: `{"command": "start"}` |
//! | `bubbaloop/daemon/api/refresh` | Refresh all nodes | None |

use crate::node_manager::NodeManager;
use crate::proto::{CommandType, NodeCommand};
use serde::{Deserialize, Serialize};
use std::sync::Arc;
use tokio::sync::watch;
use zenoh::bytes::ZBytes;
use zenoh::Session;

/// Key expressions for API endpoints
pub mod api_keys {
    /// Base prefix for all API endpoints
    pub const API_PREFIX: &str = "bubbaloop/daemon/api";

    /// Health check
    pub const HEALTH: &str = "bubbaloop/daemon/api/health";

    /// List all nodes
    pub const NODES: &str = "bubbaloop/daemon/api/nodes";

    /// Add a new node
    pub const NODES_ADD: &str = "bubbaloop/daemon/api/nodes/add";

    /// Refresh all node states
    pub const REFRESH: &str = "bubbaloop/daemon/api/refresh";

    /// Wildcard for all API endpoints (used for queryable declaration)
    pub const API_WILDCARD: &str = "bubbaloop/daemon/api/**";
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
}

/// JSON response for node list
#[derive(Serialize, Deserialize)]
pub struct NodeListResponse {
    pub nodes: Vec<NodeStateResponse>,
    pub timestamp_ms: i64,
}

/// JSON request for commands
#[derive(Deserialize)]
pub struct CommandRequest {
    pub command: String,
    #[serde(default)]
    pub node_path: String,
}

/// JSON response for commands
#[derive(Serialize)]
pub struct CommandResponse {
    pub success: bool,
    pub message: String,
    pub output: String,
    pub node_state: Option<NodeStateResponse>,
}

/// JSON response for logs
#[derive(Serialize)]
pub struct LogsResponse {
    pub node_name: String,
    pub lines: Vec<String>,
    pub success: bool,
    pub error: Option<String>,
}

/// JSON response for health check
#[derive(Serialize)]
pub struct HealthResponse {
    pub status: String,
}

/// JSON error response
#[derive(Serialize)]
pub struct ErrorResponse {
    pub error: String,
    pub code: u16,
}

fn status_to_string(status: i32) -> String {
    match status {
        0 => "unknown".to_string(),
        1 => "stopped".to_string(),
        2 => "running".to_string(),
        3 => "failed".to_string(),
        4 => "installing".to_string(),
        5 => "building".to_string(),
        6 => "not-installed".to_string(),
        _ => "unknown".to_string(),
    }
}

fn node_state_to_response(state: &crate::proto::NodeState) -> NodeStateResponse {
    NodeStateResponse {
        name: state.name.clone(),
        path: state.path.clone(),
        status: status_to_string(state.status),
        installed: state.installed,
        autostart_enabled: state.autostart_enabled,
        version: state.version.clone(),
        description: state.description.clone(),
        node_type: state.node_type.clone(),
        is_built: state.is_built,
        build_output: state.build_output.clone(),
    }
}

fn parse_command(cmd: &str) -> CommandType {
    match cmd.to_lowercase().as_str() {
        "start" => CommandType::Start,
        "stop" => CommandType::Stop,
        "restart" => CommandType::Restart,
        "install" => CommandType::Install,
        "uninstall" => CommandType::Uninstall,
        "build" => CommandType::Build,
        "clean" => CommandType::Clean,
        "enable_autostart" | "enable-autostart" => CommandType::EnableAutostart,
        "disable_autostart" | "disable-autostart" => CommandType::DisableAutostart,
        "add" | "add_node" => CommandType::AddNode,
        "remove" | "remove_node" => CommandType::RemoveNode,
        "refresh" => CommandType::Refresh,
        _ => CommandType::Refresh,
    }
}

/// Zenoh API service for the daemon
pub struct ZenohApiService {
    session: Arc<Session>,
    node_manager: Arc<NodeManager>,
}

impl ZenohApiService {
    /// Create a new Zenoh API service
    pub fn new(session: Arc<Session>, node_manager: Arc<NodeManager>) -> Self {
        Self {
            session,
            node_manager,
        }
    }

    /// Run the Zenoh API service
    pub async fn run(self, mut shutdown: watch::Receiver<()>) -> Result<(), zenoh::Error> {
        log::info!("Starting Zenoh API service...");

        // Declare a single queryable with wildcard to handle all API endpoints
        let queryable = self
            .session
            .declare_queryable(api_keys::API_WILDCARD)
            .await?;

        log::info!(
            "Declared queryable on {} for REST-like API",
            api_keys::API_WILDCARD
        );

        loop {
            tokio::select! {
                // Handle shutdown
                _ = shutdown.changed() => {
                    log::info!("Zenoh API service shutting down...");
                    break;
                }

                // Handle incoming queries
                query = queryable.recv_async() => {
                    match query {
                        Ok(query) => {
                            self.handle_query(&query).await;
                        }
                        Err(e) => {
                            log::warn!("Query receive error: {}", e);
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
        log::debug!("Received API query on {}", key_expr);

        // Parse the key expression to determine the endpoint
        let path = key_expr
            .strip_prefix(api_keys::API_PREFIX)
            .unwrap_or(key_expr);
        let path = path.trim_start_matches('/');

        // Route to appropriate handler
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
        if let Err(e) = query.reply(query.key_expr(), ZBytes::from(response)).await {
            log::warn!("Failed to send reply: {}", e);
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
        };
        serde_json::to_string(&response).unwrap_or_else(|e| {
            format!(r#"{{"error":"Failed to serialize: {}","code":500}}"#, e)
        })
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
            Some(node) => serde_json::to_string(&node_state_to_response(&node)).unwrap_or_else(
                |e| format!(r#"{{"error":"Failed to serialize: {}","code":500}}"#, e),
            ),
            None => serde_json::to_string(&ErrorResponse {
                error: format!("Node '{}' not found", name),
                code: 404,
            })
            .unwrap_or_else(|_| r#"{"error":"Not found","code":404}"#.to_string()),
        }
    }

    /// Handle GET /nodes/{name}/logs
    async fn handle_get_logs(&self, name: &str) -> String {
        // Check if node exists
        let node = self.node_manager.get_node(name).await;
        if node.is_none() {
            return serde_json::to_string(&LogsResponse {
                node_name: name.to_string(),
                lines: vec![],
                success: false,
                error: Some("Node not found".to_string()),
            })
            .unwrap_or_else(|_| {
                r#"{"node_name":"","lines":[],"success":false,"error":"Node not found"}"#
                    .to_string()
            });
        }

        // Get logs using systemctl status (works even without journald)
        let service_name = format!("bubbaloop-{}.service", name);
        let output = tokio::process::Command::new("systemctl")
            .args(["--user", "status", "-l", "--no-pager", &service_name])
            .output()
            .await;

        match output {
            Ok(output) => {
                let stdout = String::from_utf8_lossy(&output.stdout);
                let stderr = String::from_utf8_lossy(&output.stderr);
                let mut lines: Vec<String> = stdout.lines().map(|s| s.to_string()).collect();

                // Also try journalctl as backup
                if let Ok(journal_output) = tokio::process::Command::new("journalctl")
                    .args(["--user", "-u", &service_name, "-n", "50", "--no-pager"])
                    .output()
                    .await
                {
                    let journal_stdout = String::from_utf8_lossy(&journal_output.stdout);
                    for line in journal_stdout.lines() {
                        if !line.contains("No entries") && !line.contains("No journal files") {
                            lines.push(line.to_string());
                        }
                    }
                }

                // Add stderr if any
                if !stderr.is_empty() {
                    lines.push("--- stderr ---".to_string());
                    for line in stderr.lines() {
                        lines.push(line.to_string());
                    }
                }

                serde_json::to_string(&LogsResponse {
                    node_name: name.to_string(),
                    lines,
                    success: true,
                    error: None,
                })
                .unwrap_or_else(|e| {
                    format!(r#"{{"error":"Failed to serialize: {}","code":500}}"#, e)
                })
            }
            Err(e) => serde_json::to_string(&LogsResponse {
                node_name: name.to_string(),
                lines: vec![],
                success: false,
                error: Some(format!("Failed to get logs: {}", e)),
            })
            .unwrap_or_else(|_| {
                r#"{"node_name":"","lines":[],"success":false,"error":"Failed to get logs"}"#
                    .to_string()
            }),
        }
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

        let cmd = NodeCommand {
            command: parse_command(&request.command) as i32,
            node_name: name.to_string(),
            node_path: request.node_path,
            request_id: uuid::Uuid::new_v4().to_string(),
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
