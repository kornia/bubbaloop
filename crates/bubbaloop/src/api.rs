//! REST API for CLI → daemon communication.
//!
//! Lightweight HTTP endpoints that expose `PlatformOperations` to the CLI client.
//! These run alongside MCP on the same axum router (port 8088) and are
//! localhost-only, unauthenticated (same security model as the /health endpoint).

use axum::extract::{Path, State};
use axum::routing::{delete, get, post};
use axum::{Json, Router};
use serde::{Deserialize, Serialize};
use std::sync::Arc;

use crate::mcp::platform::{NodeCommand, PlatformOperations};

/// Node state returned by the list endpoint.
#[derive(Debug, Serialize, Deserialize)]
pub struct ApiNodeState {
    pub name: String,
    pub status: String,
    pub health: String,
    pub node_type: String,
    pub installed: bool,
    pub is_built: bool,
}

/// Response from the list nodes endpoint.
#[derive(Debug, Serialize, Deserialize)]
pub struct ApiNodeListResponse {
    pub nodes: Vec<ApiNodeState>,
}

/// Response from command/add/remove endpoints.
#[derive(Debug, Serialize, Deserialize)]
pub struct ApiCommandResponse {
    pub success: bool,
    pub message: String,
    #[serde(default)]
    pub output: String,
}

/// Request body for the command endpoint.
#[derive(Debug, Deserialize)]
struct CommandRequest {
    command: String,
}

/// Request body for the add-node endpoint.
#[derive(Debug, Deserialize)]
struct AddNodeRequest {
    source: String,
    #[serde(default)]
    name: Option<String>,
    #[serde(default)]
    config: Option<String>,
}

/// Request body for the marketplace install endpoint.
#[derive(Debug, Deserialize)]
struct InstallMarketplaceRequest {
    name: String,
}

fn command_response(result: Result<String, impl std::fmt::Display>) -> Json<ApiCommandResponse> {
    match result {
        Ok(msg) => Json(ApiCommandResponse {
            success: true,
            message: msg,
            output: String::new(),
        }),
        Err(e) => Json(ApiCommandResponse {
            success: false,
            message: e.to_string(),
            output: String::new(),
        }),
    }
}

async fn list_nodes<P: PlatformOperations>(
    State(platform): State<Arc<P>>,
) -> Json<ApiNodeListResponse> {
    match platform.list_nodes().await {
        Ok(nodes) => {
            let api_nodes = nodes
                .into_iter()
                .map(|n| ApiNodeState {
                    name: n.name,
                    status: n.status,
                    health: n.health,
                    node_type: n.node_type,
                    installed: n.installed,
                    is_built: n.is_built,
                })
                .collect();
            Json(ApiNodeListResponse { nodes: api_nodes })
        }
        Err(e) => {
            log::error!("[API] list_nodes error: {}", e);
            Json(ApiNodeListResponse { nodes: vec![] })
        }
    }
}

async fn node_command<P: PlatformOperations>(
    State(platform): State<Arc<P>>,
    Path(name): Path<String>,
    Json(req): Json<CommandRequest>,
) -> Json<ApiCommandResponse> {
    if let Err(e) = crate::validation::validate_node_name(&name) {
        return Json(ApiCommandResponse {
            success: false,
            message: format!("Invalid node name: {}", e),
            output: String::new(),
        });
    }

    let cmd = match parse_command(&req.command) {
        Some(c) => c,
        None => {
            return Json(ApiCommandResponse {
                success: false,
                message: format!("Unknown command: {}", req.command),
                output: String::new(),
            });
        }
    };

    command_response(platform.execute_command(&name, cmd).await)
}

async fn add_node<P: PlatformOperations>(
    State(platform): State<Arc<P>>,
    Json(req): Json<AddNodeRequest>,
) -> Json<ApiCommandResponse> {
    if let Err(e) = crate::validation::validate_install_source(&req.source) {
        return Json(ApiCommandResponse {
            success: false,
            message: format!("Invalid source: {}", e),
            output: String::new(),
        });
    }
    if let Some(ref name) = req.name {
        if let Err(e) = crate::validation::validate_node_name(name) {
            return Json(ApiCommandResponse {
                success: false,
                message: format!("Invalid node name: {}", e),
                output: String::new(),
            });
        }
    }

    command_response(
        platform
            .add_node(
                &req.source,
                req.name.as_deref(),
                req.config.as_deref(),
            )
            .await,
    )
}

async fn install_marketplace<P: PlatformOperations>(
    State(platform): State<Arc<P>>,
    Json(req): Json<InstallMarketplaceRequest>,
) -> Json<ApiCommandResponse> {
    if let Err(e) = crate::validation::validate_node_name(&req.name) {
        return Json(ApiCommandResponse {
            success: false,
            message: format!("Invalid marketplace name: {}", e),
            output: String::new(),
        });
    }
    command_response(platform.install_from_marketplace(&req.name).await)
}

async fn remove_node<P: PlatformOperations>(
    State(platform): State<Arc<P>>,
    Path(name): Path<String>,
) -> Json<ApiCommandResponse> {
    if let Err(e) = crate::validation::validate_node_name(&name) {
        return Json(ApiCommandResponse {
            success: false,
            message: format!("Invalid node name: {}", e),
            output: String::new(),
        });
    }
    command_response(platform.remove_node(&name).await)
}

/// Build the `/api/v1` router. Caller nests this under `/api/v1`.
pub fn api_router<P: PlatformOperations>(platform: Arc<P>) -> Router {
    Router::new()
        .route("/nodes", get(list_nodes::<P>))
        .route("/nodes/{name}/command", post(node_command::<P>))
        .route("/nodes/add", post(add_node::<P>))
        .route("/nodes/install", post(install_marketplace::<P>))
        .route("/nodes/{name}", delete(remove_node::<P>))
        .with_state(platform)
}

fn parse_command(s: &str) -> Option<NodeCommand> {
    match s {
        "start" => Some(NodeCommand::Start),
        "stop" => Some(NodeCommand::Stop),
        "restart" => Some(NodeCommand::Restart),
        "build" => Some(NodeCommand::Build),
        "logs" => Some(NodeCommand::GetLogs),
        "install" => Some(NodeCommand::Install),
        "uninstall" => Some(NodeCommand::Uninstall),
        "clean" => Some(NodeCommand::Clean),
        "enable" => Some(NodeCommand::EnableAutostart),
        "disable" => Some(NodeCommand::DisableAutostart),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_command_valid() {
        assert!(matches!(parse_command("start"), Some(NodeCommand::Start)));
        assert!(matches!(parse_command("stop"), Some(NodeCommand::Stop)));
        assert!(matches!(
            parse_command("restart"),
            Some(NodeCommand::Restart)
        ));
        assert!(matches!(parse_command("build"), Some(NodeCommand::Build)));
        assert!(matches!(parse_command("logs"), Some(NodeCommand::GetLogs)));
        assert!(matches!(
            parse_command("install"),
            Some(NodeCommand::Install)
        ));
        assert!(matches!(
            parse_command("uninstall"),
            Some(NodeCommand::Uninstall)
        ));
        assert!(matches!(parse_command("clean"), Some(NodeCommand::Clean)));
        assert!(matches!(
            parse_command("enable"),
            Some(NodeCommand::EnableAutostart)
        ));
        assert!(matches!(
            parse_command("disable"),
            Some(NodeCommand::DisableAutostart)
        ));
    }

    #[test]
    fn test_parse_command_invalid() {
        assert!(parse_command("unknown").is_none());
        assert!(parse_command("").is_none());
    }

    #[test]
    fn test_api_command_response_serialization() {
        let resp = ApiCommandResponse {
            success: true,
            message: "Node started".into(),
            output: String::new(),
        };
        let json = serde_json::to_string(&resp).unwrap();
        assert!(json.contains("\"success\":true"));
        assert!(json.contains("Node started"));
    }

    #[test]
    fn test_api_node_list_response_serialization() {
        let resp = ApiNodeListResponse {
            nodes: vec![ApiNodeState {
                name: "test".into(),
                status: "Running".into(),
                health: "Healthy".into(),
                node_type: "rust".into(),
                installed: true,
                is_built: true,
            }],
        };
        let json = serde_json::to_string(&resp).unwrap();
        assert!(json.contains("\"name\":\"test\""));
        assert!(json.contains("\"status\":\"Running\""));
    }
}
