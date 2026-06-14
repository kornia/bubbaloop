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
pub mod storage_tools;
pub mod storage_tools_write;
mod tools;
pub mod toolsets;

use platform::PlatformOperations;
use rmcp::handler::server::tool::ToolRouter;
use rmcp::model::*;
use rmcp::ServerHandler;
use std::collections::HashSet;
use std::sync::{Arc, RwLock};

/// Process-wide lock used by tests that mutate environment variables. Tests
/// that call `set_var` / `remove_var` MUST acquire this before mutating to
/// avoid races with other tests in the same process. Cargo runs tests in a
/// single binary by default.
#[cfg(test)]
pub(crate) static ENV_TEST_LOCK: std::sync::Mutex<()> = std::sync::Mutex::new(());

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

/// Parse `$BUBBALOOP_MCP_MODE` into an RBAC tier ceiling.
///
/// Values: `readonly` -> Viewer, `safe` -> Operator, `admin` (or unset) -> Admin.
/// Pattern lifted from the Red Hat kubernetes-mcp-server `--read-only` /
/// `--disable-destructive` flags — clean separation between *what the deployment
/// permits* and *what the caller is authorized for*.
pub fn mode_from_env() -> rbac::Tier {
    match std::env::var("BUBBALOOP_MCP_MODE").ok().as_deref() {
        Some("readonly") => rbac::Tier::Viewer,
        Some("safe") => rbac::Tier::Operator,
        Some("admin") | None => rbac::Tier::Admin,
        Some("") => rbac::Tier::Admin,
        Some(other) => {
            log::warn!(
                "Unknown BUBBALOOP_MCP_MODE='{}', defaulting to admin",
                other
            );
            rbac::Tier::Admin
        }
    }
}

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
    /// Tier ceiling from $BUBBALOOP_MCP_MODE — caller cannot exceed this.
    /// Default: Admin (no ceiling). Set to Viewer for read-only mode,
    /// Operator to disable destructive ops.
    pub(crate) mode_ceiling: rbac::Tier,
    /// Per-session enabled toolsets (gap #12 dynamic disclosure). Mutated
    /// at runtime via the `toolset_enable` meta-tool; initialised from
    /// `$BUBBALOOP_MCP_TOOLSETS` (default: all enabled, backwards compatible).
    /// `Arc<RwLock<...>>` because reads (every `list_tools` / `get_tool`
    /// call) vastly outnumber writes (only `toolset_enable` mutates).
    pub(crate) enabled_toolsets: Arc<RwLock<HashSet<String>>>,
    /// Live/finished replay sessions backing `storage_replay_*` (§6). Shared via
    /// `Arc` across server clones so `start` and `status` see the same registry.
    pub(crate) replay_sessions: storage_tools_write::ReplayRegistry,
}

// Manual Clone impl: P doesn't need Clone because it's behind Arc.
impl<P: PlatformOperations> Clone for BubbaLoopMcpServer<P> {
    fn clone(&self) -> Self {
        Self {
            platform: self.platform.clone(),
            auth_token: self.auth_token.clone(),
            tool_router: self.tool_router.clone(),
            machine_id: self.machine_id.clone(),
            mode_ceiling: self.mode_ceiling,
            // Share toolset state via Arc — clones see the same enable/disable mutations.
            enabled_toolsets: self.enabled_toolsets.clone(),
            // Share the replay registry so status/stop reach start's sessions.
            replay_sessions: self.replay_sessions.clone(),
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
                 **Discovery:** node_list, node_health, node_schema, node_stream_info, capability_discover, node_discover, dataflow_graph\n\
                 **Lifecycle:** node_start, node_stop, node_restart, node_build, node_install, node_remove, node_uninstall, node_clean\n\
                 **Autostart:** node_autostart_enable, node_autostart_disable\n\
                 **Data:** node_command_send, node_stream_info (returns Zenoh topic for streaming)\n\
                 **Config:** node_config, node_manifest, node_commands\n\
                 **Proposals:** proposal_list, proposal_approve, proposal_reject\n\
                 **Memory:** job_list, job_delete, memory_episodic_clear\n\
                 **Beliefs:** belief_update, belief_get — durable agent beliefs (subject+predicate model, e.g. subject='front_door_camera' predicate='is_reliable')\n\
                 **World State:** world_state_list — live sensor-derived key/value snapshot\n\
                 **Context Providers:** context_configure — wire a Zenoh topic pattern to world state (daemon background task)\n\
                 **Missions:** mission_list, mission_pause, mission_resume, mission_cancel — YAML-file-driven goals (~/.bubbaloop/agents/{id}/missions/)\n\
                 **Constraints:** constraint_register, constraint_list — per-mission safety limits (workspace/max_velocity/forbidden_zone/max_force)\n\
                 **Alerts:** alert_register, alert_unregister, alert_list — reactive rules that spike arousal when world state matches (alert_list surfaces dangling world-state refs)\n\
                 **Telemetry:** telemetry_get, telemetry_history, telemetry_config_update — CPU/mem/disk/temp snapshot + history + threshold tuning\n\
                 **System:** system_status, machine_info, zenoh_query\n\
                 **Toolsets (meta — always visible):** toolset_list, toolset_enable, toolset_get_tools — narrow or broaden the live tool surface for context-window-limited models. Default: all 11 toolsets enabled. Set BUBBALOOP_MCP_TOOLSETS=node,system to pre-narrow at deploy.\n\n\
                 node_install accepts marketplace names (e.g., 'rtsp-camera'), local paths, or GitHub 'user/repo' format.\n\
                 Use capability_discover to find nodes by capability (sensor, actuator, processor, gateway).\n\
                 Use node_manifest for full node details including topics, commands, and requirements.\n\
                 Streaming data flows through Zenoh (not MCP). Use node_stream_info to get Zenoh connection params.\n\
                 node_logs accepts an optional `lines` param (default 50, max 5000).\n\
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
        // Mode-ceiling check (BUBBALOOP_MCP_MODE env var). The ceiling caps
        // *what tools are reachable*, independent of the caller's own tier.
        // Rationale: matches the Red Hat kubernetes-mcp-server pattern of
        // --read-only / --disable-destructive flags — fast win for safe
        // deployments without redoing token semantics.
        if (required as u8) > (self.mode_ceiling as u8) {
            log::warn!(
                "Mode-restricted: tool '{}' needs {} tier but BUBBALOOP_MCP_MODE permits at most {}",
                request.name, required, self.mode_ceiling
            );
            return Err(rmcp::model::ErrorData::new(
                rmcp::model::ErrorCode::INVALID_REQUEST,
                format!(
                    "Mode-restricted: tool '{}' requires {} tier but BUBBALOOP_MCP_MODE permits at most {}",
                    request.name, required, self.mode_ceiling
                ),
                None,
            ));
        }
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
        // Gap #12 dynamic toolsets — filter the full router output to only
        // include tools whose name is in an enabled toolset (or is one of the
        // always-visible meta-tools). Read-locked the whole filter pass so
        // we never clone the HashSet.
        let enabled = self
            .enabled_toolsets
            .read()
            .unwrap_or_else(|p| p.into_inner());
        let filtered: Vec<Tool> = self
            .tool_router
            .list_all()
            .into_iter()
            .filter(|t| toolsets::is_visible(&t.name, &enabled))
            .collect();
        drop(enabled);
        Ok(ListToolsResult {
            tools: filtered,
            meta: None,
            next_cursor: None,
        })
    }

    fn get_tool(&self, name: &str) -> Option<Tool> {
        // Mirror the visibility filter on get_tool — a tool the client
        // can't see via list_tools should also not be retrievable by name.
        // The dispatch path (call_tool) is intentionally NOT filtered: an
        // RBAC-passing caller that knows the name can still invoke it,
        // matching GitHub MCP's `X-MCP-Tools` semantics where the header
        // narrows visibility but doesn't gate execution.
        let enabled = self.enabled_toolsets.read().ok()?;
        if toolsets::is_visible(name, &enabled) {
            self.tool_router.get(name).cloned()
        } else {
            None
        }
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

    // stdio MCP is process-scoped; no shutdown channel. Live provider spawn is
    // not available — persisted providers are picked up by the daemon on next start.
    let platform = Arc::new(platform::DaemonPlatform::new(
        node_manager,
        session,
        machine_id.clone(),
        None,
    ));

    let server = BubbaLoopMcpServer::new(
        platform, None, // No auth token for stdio
        machine_id,
    )
    .with_mode_ceiling(mode_from_env());

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

    // Clone session before it's moved into platform (needed for chat handler and manifest)
    let session_for_chat = session.clone();
    let session_for_manifest = session.clone();

    // HTTP MCP lives under the daemon's shutdown. Forward `shutdown_rx` so
    // `context_configure` can spawn context providers live and still tear them
    // down cleanly on daemon shutdown.
    let platform = Arc::new(platform::DaemonPlatform::new(
        node_manager,
        session,
        machine_id.clone(),
        Some(shutdown_rx.clone()),
    ));

    // Register daemon manifest queryable (discovery: http_endpoint + version)
    crate::daemon::daemon_manifest::register_daemon_manifest(
        &session_for_manifest,
        &machine_id,
        port,
        shutdown_rx.clone(),
    )
    .await;

    let api_router = crate::api::api_router(platform.clone());

    // Chat router: POST /api/v1/agents/{id}/turns → SSE
    let chat_state = Arc::new(crate::daemon::chat_route::ChatState {
        session: session_for_chat,
        machine_id: machine_id.clone(),
        auth_token: token.clone(),
    });
    let chat_router = crate::daemon::chat_route::chat_router(chat_state);

    // Build auth layer before mcp_service closure consumes `token`.
    // /mcp and /api/v1 require bearer token; /health remains unauthenticated
    // for liveness probes.
    let auth_layer = axum::middleware::from_fn_with_state(token.clone(), bearer_auth_middleware);

    let mcp_service = StreamableHttpService::new(
        move || {
            Ok(
                BubbaLoopMcpServer::new(platform.clone(), Some(token.clone()), machine_id.clone())
                    .with_mode_ceiling(mode_from_env()),
            )
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

    // Merge api_router and chat_router before nesting under /api/v1.
    // Each sub-router carries its own state type — axum supports this via merge.
    let combined_api = axum::Router::new().merge(api_router).merge(chat_router);

    let authenticated_routes = axum::Router::new()
        .nest("/api/v1", combined_api)
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

    // Bind address: BUBBALOOP_HTTP_BIND overrides default localhost-only.
    // Setting 0.0.0.0 is required for dashboard access from a remote laptop.
    // Default is localhost-only per CLAUDE.md security convention.
    let bind_addr =
        std::env::var("BUBBALOOP_HTTP_BIND").unwrap_or_else(|_| format!("127.0.0.1:{}", port));

    // CORS: fail-closed. BUBBALOOP_DASHBOARD_ORIGIN must be set explicitly.
    // If unset, the CORS layer is not mounted — browsers are blocked by
    // missing CORS headers, but non-browser clients (CLI, curl) still work.
    let router = if let Ok(origin) = std::env::var("BUBBALOOP_DASHBOARD_ORIGIN") {
        let allowed_origin = origin
            .parse::<axum::http::HeaderValue>()
            .map_err(|e| format!("Invalid BUBBALOOP_DASHBOARD_ORIGIN: {}", e))?;
        let cors = tower_http::cors::CorsLayer::new()
            .allow_origin(allowed_origin)
            .allow_methods([
                axum::http::Method::GET,
                axum::http::Method::POST,
                axum::http::Method::OPTIONS,
            ])
            .allow_headers([
                axum::http::header::AUTHORIZATION,
                axum::http::header::CONTENT_TYPE,
            ]);
        log::info!("CORS enabled for origin: {}", origin);
        router.layer(cors)
    } else {
        log::info!("CORS disabled (set BUBBALOOP_DASHBOARD_ORIGIN to enable browser access)");
        router
    };

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

#[cfg(test)]
mod mode_tests {
    use super::*;

    /// Serialize env-var manipulation across the whole crate's tests via the
    /// process-wide `ENV_TEST_LOCK`. Without this, two tests calling
    /// `set_var`/`remove_var` concurrently can each observe values set by
    /// the other.
    fn with_env<F: FnOnce()>(value: Option<&str>, f: F) {
        let _guard = super::ENV_TEST_LOCK
            .lock()
            .unwrap_or_else(|p| p.into_inner());
        let saved = std::env::var("BUBBALOOP_MCP_MODE").ok();
        match value {
            Some(v) => std::env::set_var("BUBBALOOP_MCP_MODE", v),
            None => std::env::remove_var("BUBBALOOP_MCP_MODE"),
        }
        f();
        match saved {
            Some(v) => std::env::set_var("BUBBALOOP_MCP_MODE", v),
            None => std::env::remove_var("BUBBALOOP_MCP_MODE"),
        }
    }

    #[test]
    fn mode_from_env_parses_known_values() {
        with_env(Some("readonly"), || {
            assert_eq!(mode_from_env(), rbac::Tier::Viewer);
        });
        with_env(Some("safe"), || {
            assert_eq!(mode_from_env(), rbac::Tier::Operator);
        });
        with_env(Some("admin"), || {
            assert_eq!(mode_from_env(), rbac::Tier::Admin);
        });
    }

    #[test]
    fn mode_from_env_defaults_to_admin_when_unset_or_empty() {
        with_env(None, || {
            assert_eq!(mode_from_env(), rbac::Tier::Admin);
        });
        with_env(Some(""), || {
            assert_eq!(mode_from_env(), rbac::Tier::Admin);
        });
    }

    #[test]
    fn mode_from_env_falls_back_to_admin_on_unknown() {
        with_env(Some("loose-cannon"), || {
            assert_eq!(mode_from_env(), rbac::Tier::Admin);
        });
    }
}
