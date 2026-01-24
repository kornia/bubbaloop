//! HTTP REST API for daemon
//!
//! Provides an HTTP interface for clients that can't use Zenoh directly (e.g., Node.js TUI).

use crate::node_manager::NodeManager;
use crate::proto::{CommandType, NodeCommand};
use axum::{
    extract::{Path, Query, State},
    http::StatusCode,
    routing::{get, post},
    Json, Router,
};
use serde::{Deserialize, Serialize};
use std::sync::Arc;
use tower_http::cors::{Any, CorsLayer};

/// Shared state for HTTP handlers
#[derive(Clone)]
pub struct AppState {
    pub node_manager: Arc<NodeManager>,
}

/// JSON response for node state
#[derive(Serialize)]
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
#[derive(Serialize)]
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

/// Query params for logs
#[derive(Deserialize)]
pub struct LogsQuery {
    #[serde(default = "default_lines")]
    pub lines: u32,
}

fn default_lines() -> u32 {
    100
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

/// GET /nodes - List all nodes
async fn list_nodes(State(state): State<AppState>) -> Json<NodeListResponse> {
    let list = state.node_manager.get_node_list().await;
    Json(NodeListResponse {
        nodes: list.nodes.iter().map(node_state_to_response).collect(),
        timestamp_ms: list.timestamp_ms,
    })
}

/// GET /nodes/:name - Get single node state
async fn get_node(
    State(state): State<AppState>,
    Path(name): Path<String>,
) -> Result<Json<NodeStateResponse>, StatusCode> {
    match state.node_manager.get_node(&name).await {
        Some(node) => Ok(Json(node_state_to_response(&node))),
        None => Err(StatusCode::NOT_FOUND),
    }
}

/// POST /nodes/:name/command - Execute command on node
async fn execute_command(
    State(state): State<AppState>,
    Path(name): Path<String>,
    Json(request): Json<CommandRequest>,
) -> Json<CommandResponse> {
    let cmd = NodeCommand {
        command: parse_command(&request.command) as i32,
        node_name: name,
        node_path: request.node_path,
        request_id: uuid::Uuid::new_v4().to_string(),
    };

    let result = state.node_manager.execute_command(cmd).await;

    Json(CommandResponse {
        success: result.success,
        message: result.message,
        output: result.output,
        node_state: result.node_state.as_ref().map(node_state_to_response),
    })
}

/// POST /nodes/add - Add a new node
async fn add_node(
    State(state): State<AppState>,
    Json(request): Json<CommandRequest>,
) -> Json<CommandResponse> {
    let cmd = NodeCommand {
        command: CommandType::AddNode as i32,
        node_name: String::new(),
        node_path: request.node_path,
        request_id: uuid::Uuid::new_v4().to_string(),
    };

    let result = state.node_manager.execute_command(cmd).await;

    Json(CommandResponse {
        success: result.success,
        message: result.message,
        output: result.output,
        node_state: result.node_state.as_ref().map(node_state_to_response),
    })
}

/// POST /refresh - Refresh all node states
async fn refresh_nodes(State(state): State<AppState>) -> Json<CommandResponse> {
    let cmd = NodeCommand {
        command: CommandType::Refresh as i32,
        node_name: String::new(),
        node_path: String::new(),
        request_id: uuid::Uuid::new_v4().to_string(),
    };

    let result = state.node_manager.execute_command(cmd).await;

    Json(CommandResponse {
        success: result.success,
        message: result.message,
        output: result.output,
        node_state: None,
    })
}

/// GET /nodes/:name/logs - Get service logs
async fn get_logs(
    State(state): State<AppState>,
    Path(name): Path<String>,
    Query(_query): Query<LogsQuery>,
) -> Json<LogsResponse> {
    // Check if node exists
    let node = state.node_manager.get_node(&name).await;
    if node.is_none() {
        return Json(LogsResponse {
            node_name: name,
            lines: vec![],
            success: false,
            error: Some("Node not found".to_string()),
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

            Json(LogsResponse {
                node_name: name,
                lines,
                success: true,
                error: None,
            })
        }
        Err(e) => Json(LogsResponse {
            node_name: name,
            lines: vec![],
            success: false,
            error: Some(format!("Failed to get logs: {}", e)),
        }),
    }
}

/// GET /health - Health check endpoint
async fn health_check() -> &'static str {
    "ok"
}

/// Create the HTTP router
pub fn create_router(node_manager: Arc<NodeManager>) -> Router {
    let state = AppState { node_manager };

    // CORS layer to allow TUI access
    let cors = CorsLayer::new()
        .allow_origin(Any)
        .allow_methods(Any)
        .allow_headers(Any);

    Router::new()
        .route("/health", get(health_check))
        .route("/nodes", get(list_nodes))
        .route("/nodes/add", post(add_node))
        .route("/nodes/{name}", get(get_node))
        .route("/nodes/{name}/logs", get(get_logs))
        .route("/nodes/{name}/command", post(execute_command))
        .route("/refresh", post(refresh_nodes))
        .layer(cors)
        .with_state(state)
}

/// Run the HTTP server
pub async fn run_http_server(
    node_manager: Arc<NodeManager>,
    port: u16,
) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    let app = create_router(node_manager);

    let listener = tokio::net::TcpListener::bind(format!("0.0.0.0:{}", port)).await?;
    log::info!("HTTP server listening on port {}", port);

    axum::serve(listener, app).await?;

    Ok(())
}
