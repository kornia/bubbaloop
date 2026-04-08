//! MCP (Model Context Protocol) server for AI agent integration.
//!
//! Exposes bubbaloop node operations as MCP tools that any LLM can call.
//! Runs as an HTTP server on port 8088 inside the daemon process.

pub mod auth;
pub mod daemon_platform;
#[cfg(any(test, feature = "test-harness"))]
pub mod mock_platform;
pub mod platform;
pub mod rbac;
mod tools;

use platform::PlatformOperations;
use rmcp::handler::server::tool::ToolRouter;
use rmcp::model::*;
use rmcp::ServerHandler;
use std::sync::Arc;

/// Axum middleware that enforces Bearer token authentication.
///
/// Extracts the `Authorization: Bearer <token>` header and validates it
/// against the expected token using constant-time comparison. Returns 401
/// if the token is missing or invalid.
async fn bearer_auth_middleware(
    headers: axum::http::HeaderMap,
    state: axum::extract::State<String>,
    request: axum::extract::Request,
    next: axum::middleware::Next,
) -> axum::response::Response {
    let expected_token = &*state;
    let auth_header = headers
        .get(axum::http::header::AUTHORIZATION)
        .and_then(|v| v.to_str().ok());

    match auth_header {
        Some(header_value) if auth::validate_token(header_value, expected_token) => {
            log::debug!("[AUTH] Bearer token validated successfully");
            next.run(request).await
        }
        Some(_) => {
            log::warn!("[AUTH] Invalid bearer token presented");
            axum::response::Response::builder()
                .status(axum::http::StatusCode::UNAUTHORIZED)
                .header("WWW-Authenticate", "Bearer")
                .body(axum::body::Body::from("Unauthorized: invalid token"))
                .unwrap_or_else(|_| {
                    axum::response::Response::new(axum::body::Body::from("Unauthorized"))
                })
        }
        None => {
            log::warn!("[AUTH] Missing Authorization header");
            axum::response::Response::builder()
                .status(axum::http::StatusCode::UNAUTHORIZED)
                .header("WWW-Authenticate", "Bearer")
                .body(axum::body::Body::from(
                    "Unauthorized: missing Authorization header",
                ))
                .unwrap_or_else(|_| {
                    axum::response::Response::new(axum::body::Body::from("Unauthorized"))
                })
        }
    }
}

/// Default port for the MCP HTTP server.
pub const MCP_PORT: u16 = 8088;

/// Bubbaloop MCP server — wraps Zenoh operations as MCP tools.
///
/// Generic over `P: PlatformOperations` so production uses `DaemonPlatform`
/// and tests can plug in `MockPlatform`.
pub struct BubbaLoopMcpServer<P: PlatformOperations = platform::DaemonPlatform> {
    pub(crate) platform: Arc<P>,
    #[allow(dead_code)] // TODO(phase-3): Used for per-token tier differentiation
    pub(crate) auth_token: Option<String>,
    pub(crate) tool_router: ToolRouter<Self>,
    pub(crate) machine_id: String,
}

// Manual Clone impl: P doesn't need Clone because it's behind Arc.
impl<P: PlatformOperations> Clone for BubbaLoopMcpServer<P> {
    fn clone(&self) -> Self {
        Self {
            platform: self.platform.clone(),
            auth_token: self.auth_token.clone(),
            tool_router: self.tool_router.clone(),
            machine_id: self.machine_id.clone(),
        }
    }
}

// ── ServerHandler implementation ──────────────────────────────────
//
// NOTE: We manually implement call_tool/list_tools/get_tool instead of using
// #[tool_handler] so we can insert RBAC authorization before dispatching.

impl<P: PlatformOperations> ServerHandler for BubbaLoopMcpServer<P> {
    fn get_info(&self) -> ServerInfo {
        ServerInfo {
            protocol_version: ProtocolVersion::V_2024_11_05,
            capabilities: ServerCapabilities::builder().enable_tools().build(),
            server_info: Implementation::from_build_env(),
            instructions: Some(
                "Bubbaloop skill runtime for AI agents. Controls physical sensor nodes via MCP.\n\n\
                 **Discovery:** list_nodes, get_node_health, get_node_schema, get_stream_info, discover_capabilities\n\
                 **Lifecycle:** start_node, stop_node, restart_node, build_node, install_node, remove_node, uninstall_node, clean_node\n\
                 **Autostart:** enable_autostart, disable_autostart\n\
                 **Data:** send_command, get_stream_info (returns Zenoh topic for streaming)\n\
                 **Config:** get_node_config, get_node_manifest, list_commands\n\
                 **Proposals:** list_proposals, approve_proposal, reject_proposal\n\
                 **Memory:** list_jobs, delete_job, clear_episodic_memory\n\
                 **Beliefs:** update_belief, get_belief — durable agent beliefs (subject+predicate model, e.g. subject='front_door_camera' predicate='is_reliable')\n\
                 **World State:** list_world_state — live sensor-derived key/value snapshot\n\
                 **Context Providers:** configure_context — wire a Zenoh topic pattern to world state (daemon background task)\n\
                 **Missions:** list_missions, pause_mission, resume_mission, cancel_mission — YAML-file-driven goals (~/.bubbaloop/agents/{id}/missions/)\n\
                 **Constraints:** register_constraint, list_constraints — per-mission safety limits (workspace/max_velocity/forbidden_zone/max_force)\n\
                 **Alerts:** register_alert, unregister_alert — reactive rules that spike arousal when world state matches\n\
                 **System:** get_system_status, get_machine_info, query_zenoh, discover_nodes\n\n\
                 install_node accepts marketplace names (e.g., 'rtsp-camera'), local paths, or GitHub 'user/repo' format.\n\
                 Use discover_capabilities to find nodes by capability (sensor, actuator, processor, gateway).\n\
                 Use get_node_manifest for full node details including topics, commands, and requirements.\n\
                 Streaming data flows through Zenoh (not MCP). Use get_stream_info to get Zenoh connection params.\n\
                 Missions are created by dropping a YAML file into ~/.bubbaloop/agents/{id}/missions/ — the daemon picks them up automatically.\n\
                 Constraint params_json format: workspace={\"x\":[-1,1],\"y\":[-1,1],\"z\":[0,2]}, max_velocity=1.5, forbidden_zone={\"center\":[0,0,0],\"radius\":0.3}, max_force=50.0\n\
                 Auth: Bearer token required (see ~/.bubbaloop/mcp-token)."
                    .into(),
            ),
        }
    }

    async fn call_tool(
        &self,
        request: CallToolRequestParams,
        context: rmcp::service::RequestContext<rmcp::RoleServer>,
    ) -> Result<CallToolResult, rmcp::ErrorData> {
        // RBAC authorization check.
        // Bearer token auth is enforced at the HTTP middleware layer.
        // Currently all authenticated callers receive Admin tier.
        // TODO(phase-3): Encode tier in token (e.g., bb_admin_<uuid>, bb_viewer_<uuid>)
        // and extract it here to enable per-token tier differentiation.
        let required = rbac::required_tier(&request.name);
        let caller_tier = rbac::Tier::Admin;
        if !caller_tier.has_permission(required) {
            log::warn!(
                "RBAC denied: tool '{}' requires {} tier, caller has {} tier",
                request.name,
                required,
                caller_tier
            );
            return Err(rmcp::model::ErrorData::new(
                rmcp::model::ErrorCode::INVALID_REQUEST,
                format!(
                    "Permission denied: tool '{}' requires {} tier, caller has {} tier",
                    request.name, required, caller_tier
                ),
                None,
            ));
        }

        // Delegate to the tool router
        let tcc = rmcp::handler::server::tool::ToolCallContext::new(self, request, context);
        self.tool_router.call(tcc).await
    }

    async fn list_tools(
        &self,
        _request: Option<PaginatedRequestParams>,
        _context: rmcp::service::RequestContext<rmcp::RoleServer>,
    ) -> Result<ListToolsResult, rmcp::ErrorData> {
        Ok(ListToolsResult {
            tools: self.tool_router.list_all(),
            meta: None,
            next_cursor: None,
        })
    }

    fn get_tool(&self, name: &str) -> Option<Tool> {
        self.tool_router.get(name).cloned()
    }
}

/// Run MCP server on stdio (stdin/stdout).
///
/// No authentication on stdio — process boundary provides implicit trust
/// (per MCP spec: "STDIO transport SHOULD NOT use OAuth 2.1").
/// Logs should be redirected to a file before calling this function to avoid
/// corrupting the MCP JSON-RPC protocol on stdout/stderr.
pub async fn run_mcp_stdio(
    session: Arc<zenoh::Session>,
    node_manager: Arc<crate::daemon::node_manager::NodeManager>,
) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    use rmcp::ServiceExt;

    let machine_id = crate::daemon::util::get_machine_id();

    let platform = Arc::new(platform::DaemonPlatform::new(
        node_manager,
        session,
        machine_id.clone(),
    ));

    let server = BubbaLoopMcpServer::new(
        platform, None, // No auth token for stdio
        machine_id,
    );

    // rmcp stdio transport: reads JSON-RPC from stdin, writes to stdout
    let service = server.serve(rmcp::transport::io::stdio()).await?;
    service.waiting().await?;

    Ok(())
}

/// Start the MCP HTTP server on the given port.
///
/// Mounts the StreamableHttpService at `/mcp` and blocks until shutdown.
pub async fn run_mcp_server(
    session: Arc<zenoh::Session>,
    node_manager: Arc<crate::daemon::node_manager::NodeManager>,
    port: u16,
    mut shutdown_rx: tokio::sync::watch::Receiver<()>,
) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    use rmcp::transport::streamable_http_server::{
        session::local::LocalSessionManager, StreamableHttpService,
    };

    // Load or generate auth token
    let token =
        auth::load_or_generate_token().map_err(|e| format!("Failed to load MCP token: {}", e))?;
    log::info!("MCP authentication enabled (token in ~/.bubbaloop/mcp-token)");
    log::info!("Bearer token auth enforced on /mcp and /api/v1 routes");

    let machine_id = crate::daemon::util::get_machine_id();

    let health_manager = node_manager.clone();

    let platform = Arc::new(platform::DaemonPlatform::new(
        node_manager,
        session,
        machine_id.clone(),
    ));

    let api_router = crate::api::api_router(platform.clone());

    // Build auth layer before mcp_service closure consumes `token`.
    // /mcp and /api/v1 require bearer token; /health remains unauthenticated
    // for liveness probes.
    let auth_layer = axum::middleware::from_fn_with_state(token.clone(), bearer_auth_middleware);

    let mcp_service = StreamableHttpService::new(
        move || {
            Ok(BubbaLoopMcpServer::new(
                platform.clone(),
                Some(token.clone()),
                machine_id.clone(),
            ))
        },
        LocalSessionManager::default().into(),
        Default::default(),
    );

    // Rate limiting: burst of 100 requests, ~1 req/sec sustained replenishment
    let governor_conf = tower_governor::governor::GovernorConfigBuilder::default()
        .per_second(1)
        .burst_size(100)
        .finish()
        .ok_or("Failed to configure rate limiter")?;

    // Periodic cleanup of stale rate-limit entries (respects shutdown signal)
    let governor_limiter = governor_conf.limiter().clone();
    let mut cleanup_shutdown = shutdown_rx.clone();
    tokio::spawn(async move {
        loop {
            tokio::select! {
                _ = tokio::time::sleep(std::time::Duration::from_secs(60)) => {
                    governor_limiter.retain_recent();
                }
                _ = cleanup_shutdown.changed() => break,
            }
        }
    });

    let authenticated_routes = axum::Router::new()
        .nest("/api/v1", api_router)
        .nest_service("/mcp", mcp_service)
        .layer(auth_layer);

    let router = axum::Router::new()
        .route(
            "/health",
            axum::routing::get(move || {
                let mgr = health_manager.clone();
                async move {
                    use crate::schemas::NodeStatus;
                    let node_list = mgr.get_node_list().await;
                    let running = node_list
                        .nodes
                        .iter()
                        .filter(|n| NodeStatus::try_from(n.status) == Ok(NodeStatus::Running))
                        .count();
                    axum::Json(serde_json::json!({
                        "status": "ok",
                        "version": env!("CARGO_PKG_VERSION"),
                        "nodes_total": node_list.nodes.len(),
                        "nodes_running": running,
                    }))
                }
            }),
        )
        .merge(authenticated_routes)
        .layer(tower_governor::GovernorLayer::new(governor_conf));

    let bind_addr = format!("127.0.0.1:{}", port);
    let listener = tokio::net::TcpListener::bind(&bind_addr).await?;
    log::info!(
        "MCP server listening on http://{}/mcp (rate limit: 100 burst, 1/sec sustained)",
        bind_addr
    );

    axum::serve(
        listener,
        router.into_make_service_with_connect_info::<std::net::SocketAddr>(),
    )
    .with_graceful_shutdown(async move {
        shutdown_rx.changed().await.ok();
    })
    .await?;

    log::info!("MCP server stopped.");
    Ok(())
}
