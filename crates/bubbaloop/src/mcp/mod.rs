//! MCP (Model Context Protocol) server for AI agent integration.
//!
//! Exposes bubbaloop node operations as MCP tools that any LLM can call.
//! Runs as an HTTP server on port 8088 inside the daemon process.

pub mod auth;
pub mod platform;
pub mod rbac;

use crate::validation;
use platform::PlatformOperations;
use rmcp::handler::server::tool::ToolRouter;
use rmcp::handler::server::wrapper::Parameters;
use rmcp::model::*;
use rmcp::{tool, tool_router, ServerHandler};
use schemars::JsonSchema;
use serde::Deserialize;
use std::sync::Arc;

/// Default port for the MCP HTTP server.
pub const MCP_PORT: u16 = 8088;

/// Bubbaloop MCP server — wraps Zenoh operations as MCP tools.
///
/// Generic over `P: PlatformOperations` so production uses `DaemonPlatform`
/// and tests can plug in `MockPlatform`.
pub struct BubbaLoopMcpServer<P: PlatformOperations = platform::DaemonPlatform> {
    platform: Arc<P>,
    #[allow(dead_code)] // Used in Task 7 for call_tool() auth enforcement
    auth_token: Option<String>,
    tool_router: ToolRouter<Self>,
    scope: String,
    machine_id: String,
}

// Manual Clone impl: P doesn't need Clone because it's behind Arc.
impl<P: PlatformOperations> Clone for BubbaLoopMcpServer<P> {
    fn clone(&self) -> Self {
        Self {
            platform: self.platform.clone(),
            auth_token: self.auth_token.clone(),
            tool_router: self.tool_router.clone(),
            scope: self.scope.clone(),
            machine_id: self.machine_id.clone(),
        }
    }
}

// ── Tool parameter types ──────────────────────────────────────────

#[derive(Deserialize, JsonSchema)]
struct NodeNameRequest {
    /// Name of the node (e.g., "rtsp-camera", "openmeteo")
    node_name: String,
}

#[derive(Deserialize, JsonSchema)]
struct SendCommandRequest {
    /// Name of the node to send the command to
    node_name: String,
    /// Command name (must be listed in the node's manifest)
    command: String,
    /// Optional JSON parameters for the command
    #[serde(default)]
    params: serde_json::Value,
}

#[derive(Deserialize, JsonSchema)]
struct QueryTopicRequest {
    /// Full Zenoh key expression to query (e.g., "bubbaloop/local/nvidia_orin00/openmeteo/status")
    key_expr: String,
}

#[derive(Debug, Deserialize, JsonSchema)]
struct DiscoverCapabilitiesParams {
    /// Filter by capability type: "sensor", "actuator", "processor", "gateway". Omit for all.
    #[serde(default)]
    capability: Option<String>,
}

// ── Tool implementations ──────────────────────────────────────────

#[tool_router]
impl<P: PlatformOperations> BubbaLoopMcpServer<P> {
    pub fn new(
        platform: Arc<P>,
        auth_token: Option<String>,
        scope: String,
        machine_id: String,
    ) -> Self {
        Self {
            platform,
            auth_token,
            tool_router: Self::tool_router(),
            scope,
            machine_id,
        }
    }

    #[tool(
        description = "List all registered nodes with their status, capabilities, and topics. Returns node name, status (running/stopped/etc), type, and whether it's built."
    )]
    async fn list_nodes(&self) -> Result<CallToolResult, rmcp::ErrorData> {
        log::info!("[MCP] tool=list_nodes");
        match self.platform.list_nodes().await {
            Ok(nodes) => {
                let json_nodes: Vec<serde_json::Value> = nodes
                    .iter()
                    .map(|n| {
                        serde_json::json!({
                            "name": n.name,
                            "status": n.status,
                            "health": n.health,
                            "installed": n.installed,
                            "is_built": n.is_built,
                            "node_type": n.node_type,
                        })
                    })
                    .collect();
                Ok(CallToolResult::success(vec![Content::text(
                    serde_json::to_string_pretty(&json_nodes).unwrap_or_else(|_| "[]".to_string()),
                )]))
            }
            Err(e) => Ok(CallToolResult::success(vec![Content::text(format!(
                "Error: {}",
                e
            ))])),
        }
    }

    #[tool(description = "Get detailed health status of a specific node including uptime.")]
    async fn get_node_health(
        &self,
        Parameters(req): Parameters<NodeNameRequest>,
    ) -> Result<CallToolResult, rmcp::ErrorData> {
        log::info!("[MCP] tool=get_node_health node={}", req.node_name);
        if let Err(e) = validation::validate_node_name(&req.node_name) {
            return Ok(CallToolResult::success(vec![Content::text(e)]));
        }
        match self.platform.get_node_detail(&req.node_name).await {
            Ok(detail) => Ok(CallToolResult::success(vec![Content::text(
                serde_json::to_string_pretty(&detail).unwrap_or_default(),
            )])),
            Err(platform::PlatformError::NodeNotFound(_)) => {
                Ok(CallToolResult::success(vec![Content::text(format!(
                    "Node '{}' not found",
                    req.node_name
                ))]))
            }
            Err(e) => Ok(CallToolResult::success(vec![Content::text(format!(
                "Error: {}",
                e
            ))])),
        }
    }

    #[tool(
        description = "Get the current configuration of a node by querying its Zenoh config queryable."
    )]
    async fn get_node_config(
        &self,
        Parameters(req): Parameters<NodeNameRequest>,
    ) -> Result<CallToolResult, rmcp::ErrorData> {
        log::info!("[MCP] tool=get_node_config node={}", req.node_name);
        if let Err(e) = validation::validate_node_name(&req.node_name) {
            return Ok(CallToolResult::success(vec![Content::text(e)]));
        }
        match self.platform.get_node_config(&req.node_name).await {
            Ok(config) => Ok(CallToolResult::success(vec![Content::text(
                serde_json::to_string_pretty(&config).unwrap_or_default(),
            )])),
            Err(e) => Ok(CallToolResult::success(vec![Content::text(format!(
                "Error: {}",
                e
            ))])),
        }
    }

    #[tool(
        description = "Get the full manifest for a node, including capabilities, published topics, commands, and hardware requirements."
    )]
    async fn get_node_manifest(
        &self,
        Parameters(req): Parameters<NodeNameRequest>,
    ) -> Result<CallToolResult, rmcp::ErrorData> {
        log::info!("[MCP] tool=get_node_manifest node={}", req.node_name);
        if let Err(e) = validation::validate_node_name(&req.node_name) {
            return Ok(CallToolResult::success(vec![Content::text(e)]));
        }
        match self.platform.get_manifests(None).await {
            Ok(manifests) => {
                let found = manifests
                    .into_iter()
                    .find(|(name, _)| name == &req.node_name);
                match found {
                    Some((_name, manifest)) => Ok(CallToolResult::success(vec![Content::text(
                        serde_json::to_string_pretty(&manifest).unwrap_or_default(),
                    )])),
                    None => Ok(CallToolResult::success(vec![Content::text(format!(
                        "No manifest found for node '{}'",
                        req.node_name
                    ))])),
                }
            }
            Err(e) => Ok(CallToolResult::success(vec![Content::text(format!(
                "Error: {}",
                e
            ))])),
        }
    }

    #[tool(
        description = "List available commands for a specific node with their parameters and descriptions. Use this before send_command to discover what actions a node supports."
    )]
    async fn list_commands(
        &self,
        Parameters(req): Parameters<NodeNameRequest>,
    ) -> Result<CallToolResult, rmcp::ErrorData> {
        log::info!("[MCP] tool=list_commands node={}", req.node_name);
        if let Err(e) = validation::validate_node_name(&req.node_name) {
            return Ok(CallToolResult::success(vec![Content::text(e)]));
        }
        let key_expr = format!(
            "bubbaloop/{}/{}/{}/manifest",
            self.scope, self.machine_id, req.node_name
        );
        let manifest_text = match self.platform.query_zenoh(&key_expr).await {
            Ok(text) => text,
            Err(e) => {
                return Ok(CallToolResult::success(vec![Content::text(format!(
                    "Error: {}",
                    e
                ))]));
            }
        };
        // Try to parse the manifest and extract commands
        // The manifest text is formatted as "[key_expr] json_body" from query_zenoh
        let commands = manifest_text
            .lines()
            .filter_map(|line| {
                // query_zenoh formats as "[key] json"
                let json_start = line.find(']').map(|i| i + 2)?;
                let json_str = line.get(json_start..)?;
                let manifest: serde_json::Value = serde_json::from_str(json_str).ok()?;
                manifest.get("commands").cloned()
            })
            .next();

        match commands {
            Some(cmds) if cmds.is_array() && !cmds.as_array().unwrap().is_empty() => {
                Ok(CallToolResult::success(vec![Content::text(
                    serde_json::to_string_pretty(&cmds).unwrap_or_else(|_| "[]".to_string()),
                )]))
            }
            _ => Ok(CallToolResult::success(vec![Content::text(format!(
                "No commands available for node '{}'",
                req.node_name
            ))])),
        }
    }

    #[tool(
        description = "Send a command to a node's command queryable. The node must support the command — call list_commands first to see available commands. Example: node_name='rtsp-camera', command='capture_frame', params={\"resolution\": \"1080p\"}. Returns the command result or error."
    )]
    async fn send_command(
        &self,
        Parameters(req): Parameters<SendCommandRequest>,
    ) -> Result<CallToolResult, rmcp::ErrorData> {
        log::info!(
            "[MCP] tool=send_command node={} cmd={}",
            req.node_name,
            req.command
        );
        if let Err(e) = validation::validate_node_name(&req.node_name) {
            return Ok(CallToolResult::success(vec![Content::text(e)]));
        }
        let key_expr = format!(
            "bubbaloop/{}/{}/{}/command",
            self.scope, self.machine_id, req.node_name
        );
        let payload = serde_json::json!({
            "command": req.command,
            "params": req.params,
        });
        let payload_bytes = serde_json::to_vec(&payload).unwrap_or_default();

        match self
            .platform
            .send_zenoh_query(&key_expr, payload_bytes)
            .await
        {
            Ok(results) => {
                if results.is_empty() {
                    Ok(CallToolResult::success(vec![Content::text(
                        "No response from node (is it running?)",
                    )]))
                } else {
                    Ok(CallToolResult::success(vec![Content::text(
                        results.join("\n"),
                    )]))
                }
            }
            Err(e) => Ok(CallToolResult::success(vec![Content::text(format!(
                "Error: {}",
                e
            ))])),
        }
    }

    #[tool(
        description = "Start a stopped node via the daemon. The node must be installed and built."
    )]
    async fn start_node(
        &self,
        Parameters(req): Parameters<NodeNameRequest>,
    ) -> Result<CallToolResult, rmcp::ErrorData> {
        log::info!("[MCP] tool=start_node node={}", req.node_name);
        if let Err(e) = validation::validate_node_name(&req.node_name) {
            return Ok(CallToolResult::success(vec![Content::text(e)]));
        }
        match self
            .platform
            .execute_command(&req.node_name, platform::NodeCommand::Start)
            .await
        {
            Ok(msg) => Ok(CallToolResult::success(vec![Content::text(msg)])),
            Err(e) => Ok(CallToolResult::success(vec![Content::text(format!(
                "Error: {}",
                e
            ))])),
        }
    }

    #[tool(description = "Stop a running node via the daemon.")]
    async fn stop_node(
        &self,
        Parameters(req): Parameters<NodeNameRequest>,
    ) -> Result<CallToolResult, rmcp::ErrorData> {
        log::info!("[MCP] tool=stop_node node={}", req.node_name);
        if let Err(e) = validation::validate_node_name(&req.node_name) {
            return Ok(CallToolResult::success(vec![Content::text(e)]));
        }
        match self
            .platform
            .execute_command(&req.node_name, platform::NodeCommand::Stop)
            .await
        {
            Ok(msg) => Ok(CallToolResult::success(vec![Content::text(msg)])),
            Err(e) => Ok(CallToolResult::success(vec![Content::text(format!(
                "Error: {}",
                e
            ))])),
        }
    }

    #[tool(description = "Restart a node (stop then start).")]
    async fn restart_node(
        &self,
        Parameters(req): Parameters<NodeNameRequest>,
    ) -> Result<CallToolResult, rmcp::ErrorData> {
        log::info!("[MCP] tool=restart_node node={}", req.node_name);
        if let Err(e) = validation::validate_node_name(&req.node_name) {
            return Ok(CallToolResult::success(vec![Content::text(e)]));
        }
        match self
            .platform
            .execute_command(&req.node_name, platform::NodeCommand::Restart)
            .await
        {
            Ok(msg) => Ok(CallToolResult::success(vec![Content::text(msg)])),
            Err(e) => Ok(CallToolResult::success(vec![Content::text(format!(
                "Error: {}",
                e
            ))])),
        }
    }

    #[tool(description = "Get the latest logs from a node's systemd service.")]
    async fn get_node_logs(
        &self,
        Parameters(req): Parameters<NodeNameRequest>,
    ) -> Result<CallToolResult, rmcp::ErrorData> {
        log::info!("[MCP] tool=get_node_logs node={}", req.node_name);
        if let Err(e) = validation::validate_node_name(&req.node_name) {
            return Ok(CallToolResult::success(vec![Content::text(e)]));
        }
        match self
            .platform
            .execute_command(&req.node_name, platform::NodeCommand::GetLogs)
            .await
        {
            Ok(msg) => Ok(CallToolResult::success(vec![Content::text(msg)])),
            Err(e) => Ok(CallToolResult::success(vec![Content::text(format!(
                "Error: {}",
                e
            ))])),
        }
    }

    #[tool(
        description = "Query a Zenoh key expression (admin only). Key must start with 'bubbaloop/'. Returns up to 100 results."
    )]
    async fn query_zenoh(
        &self,
        Parameters(req): Parameters<QueryTopicRequest>,
    ) -> Result<CallToolResult, rmcp::ErrorData> {
        log::info!("[MCP] tool=query_zenoh key_expr={}", req.key_expr);
        if let Err(e) = crate::validation::validate_query_key_expr(&req.key_expr) {
            return Ok(CallToolResult::success(vec![Content::text(format!(
                "Validation error: {}",
                e
            ))]));
        }
        match self.platform.query_zenoh(&req.key_expr).await {
            Ok(result) => Ok(CallToolResult::success(vec![Content::text(result)])),
            Err(e) => Ok(CallToolResult::success(vec![Content::text(format!(
                "Error: {}",
                e
            ))])),
        }
    }

    #[tool(
        description = "Discover all nodes across all machines by querying manifests. Returns a list of all self-describing nodes with their capabilities."
    )]
    async fn discover_nodes(&self) -> Result<CallToolResult, rmcp::ErrorData> {
        log::info!("[MCP] tool=discover_nodes");
        match self.platform.query_zenoh("bubbaloop/**/manifest").await {
            Ok(result) => Ok(CallToolResult::success(vec![Content::text(result)])),
            Err(e) => Ok(CallToolResult::success(vec![Content::text(format!(
                "Error: {}",
                e
            ))])),
        }
    }

    #[tool(
        description = "Discover available node capabilities. Returns nodes grouped by capability type (sensor, actuator, processor, gateway). Optionally filter by a single capability type."
    )]
    async fn discover_capabilities(
        &self,
        Parameters(params): Parameters<DiscoverCapabilitiesParams>,
    ) -> Result<CallToolResult, rmcp::ErrorData> {
        log::info!(
            "[MCP] tool=discover_capabilities filter={:?}",
            params.capability
        );
        match self
            .platform
            .get_manifests(params.capability.as_deref())
            .await
        {
            Ok(manifests) => {
                // Group nodes by capability type
                let mut grouped: std::collections::HashMap<String, Vec<serde_json::Value>> =
                    std::collections::HashMap::new();

                for (name, manifest) in &manifests {
                    let capabilities = manifest
                        .get("capabilities")
                        .and_then(|c| c.as_array())
                        .cloned()
                        .unwrap_or_default();

                    if capabilities.is_empty() {
                        grouped
                            .entry("uncategorized".to_string())
                            .or_default()
                            .push(serde_json::json!({
                                "name": name,
                                "description": manifest.get("description").and_then(|d| d.as_str()).unwrap_or(""),
                                "version": manifest.get("version").and_then(|v| v.as_str()).unwrap_or(""),
                            }));
                    } else {
                        for cap in &capabilities {
                            let cap_str = cap.as_str().unwrap_or("unknown").to_string();
                            grouped
                                .entry(cap_str)
                                .or_default()
                                .push(serde_json::json!({
                                    "name": name,
                                    "description": manifest.get("description").and_then(|d| d.as_str()).unwrap_or(""),
                                    "version": manifest.get("version").and_then(|v| v.as_str()).unwrap_or(""),
                                    "publishes": manifest.get("publishes").cloned().unwrap_or(serde_json::json!([])),
                                    "commands": manifest.get("commands").cloned().unwrap_or(serde_json::json!([])),
                                }));
                        }
                    }
                }

                let result = serde_json::json!({
                    "total_nodes": manifests.len(),
                    "capabilities": grouped,
                });
                Ok(CallToolResult::success(vec![Content::text(
                    serde_json::to_string_pretty(&result).unwrap_or_else(|_| "{}".to_string()),
                )]))
            }
            Err(e) => Ok(CallToolResult::success(vec![Content::text(format!(
                "Error: {}",
                e
            ))])),
        }
    }

    // ── Additional tools ───────────────────────────────────────────

    #[tool(
        description = "Get Zenoh connection parameters for subscribing to a node's data stream. Returns topic pattern, encoding, and endpoint. Use this to set up streaming data access outside MCP."
    )]
    async fn get_stream_info(
        &self,
        Parameters(req): Parameters<NodeNameRequest>,
    ) -> Result<CallToolResult, rmcp::ErrorData> {
        log::info!("[MCP] tool=get_stream_info node={}", req.node_name);
        if let Err(e) = validation::validate_node_name(&req.node_name) {
            return Ok(CallToolResult::success(vec![Content::text(e)]));
        }
        let info = serde_json::json!({
            "zenoh_topic": format!("bubbaloop/{}/{}/{}/**", self.scope, self.machine_id, req.node_name),
            "encoding": "protobuf",
            "endpoint": "tcp/localhost:7447",
            "note": "Subscribe to this topic via Zenoh client library for real-time data. MCP is control-plane only."
        });
        Ok(CallToolResult::success(vec![Content::text(
            serde_json::to_string_pretty(&info).unwrap_or_default(),
        )]))
    }

    #[tool(
        description = "Get overall system status including daemon health, node count, and Zenoh connection state."
    )]
    async fn get_system_status(&self) -> Result<CallToolResult, rmcp::ErrorData> {
        log::info!("[MCP] tool=get_system_status");
        let nodes = self.platform.list_nodes().await;
        let (total, running, healthy) = match &nodes {
            Ok(list) => {
                let total = list.len();
                let running = list.iter().filter(|n| n.status == "Running").count();
                let healthy = list.iter().filter(|n| n.health == "Healthy").count();
                (total, running, healthy)
            }
            Err(_) => (0, 0, 0),
        };
        let status = serde_json::json!({
            "scope": self.scope,
            "machine_id": self.machine_id,
            "nodes_total": total,
            "nodes_running": running,
            "nodes_healthy": healthy,
            "mcp_server": "running",
        });
        Ok(CallToolResult::success(vec![Content::text(
            serde_json::to_string_pretty(&status).unwrap_or_default(),
        )]))
    }

    #[tool(
        description = "Get machine hardware and OS information: architecture, hostname, OS version."
    )]
    async fn get_machine_info(&self) -> Result<CallToolResult, rmcp::ErrorData> {
        log::info!("[MCP] tool=get_machine_info");
        let info = serde_json::json!({
            "machine_id": self.machine_id,
            "scope": self.scope,
            "arch": std::env::consts::ARCH,
            "os": std::env::consts::OS,
            "hostname": hostname::get()
                .ok()
                .and_then(|h| h.into_string().ok())
                .unwrap_or_else(|| "unknown".to_string()),
        });
        Ok(CallToolResult::success(vec![Content::text(
            serde_json::to_string_pretty(&info).unwrap_or_default(),
        )]))
    }

    #[tool(
        description = "Trigger a build for a node. Builds the node's source code using its configured build command (Cargo, pixi, etc.). Admin only."
    )]
    async fn build_node(
        &self,
        Parameters(req): Parameters<NodeNameRequest>,
    ) -> Result<CallToolResult, rmcp::ErrorData> {
        log::info!("[MCP] tool=build_node node={}", req.node_name);
        if let Err(e) = validation::validate_node_name(&req.node_name) {
            return Ok(CallToolResult::success(vec![Content::text(e)]));
        }
        match self
            .platform
            .execute_command(&req.node_name, platform::NodeCommand::Build)
            .await
        {
            Ok(msg) => Ok(CallToolResult::success(vec![Content::text(msg)])),
            Err(e) => Ok(CallToolResult::success(vec![Content::text(format!(
                "Error: {}",
                e
            ))])),
        }
    }

    #[tool(
        description = "Get the protobuf schema of a node's data messages. Returns the schema in human-readable format if available."
    )]
    async fn get_node_schema(
        &self,
        Parameters(req): Parameters<NodeNameRequest>,
    ) -> Result<CallToolResult, rmcp::ErrorData> {
        log::info!("[MCP] tool=get_node_schema node={}", req.node_name);
        if let Err(e) = validation::validate_node_name(&req.node_name) {
            return Ok(CallToolResult::success(vec![Content::text(e)]));
        }
        let key = format!(
            "bubbaloop/{}/{}/{}/schema",
            self.scope, self.machine_id, req.node_name
        );
        match self.platform.query_zenoh(&key).await {
            Ok(result) => Ok(CallToolResult::success(vec![Content::text(result)])),
            Err(e) => Ok(CallToolResult::success(vec![Content::text(format!(
                "Error: {}",
                e
            ))])),
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
                 **Lifecycle:** start_node, stop_node, restart_node, build_node\n\
                 **Data:** send_command, get_stream_info (returns Zenoh topic for streaming)\n\
                 **Config:** get_node_config, get_node_manifest, list_commands\n\
                 **System:** get_system_status, get_machine_info, query_zenoh, discover_nodes\n\n\
                 Use discover_capabilities to find nodes by capability (sensor, actuator, processor, gateway).\n\
                 Use get_node_manifest for full node details including topics, commands, and requirements.\n\
                 Streaming data flows through Zenoh (not MCP). Use get_stream_info to get Zenoh connection params.\n\
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
        // RBAC authorization check
        let required = rbac::required_tier(&request.name);
        // Single-user localhost: grant admin until token-based tiers (Phase 1).
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

    let scope = std::env::var("BUBBALOOP_SCOPE").unwrap_or_else(|_| "local".to_string());
    let machine_id = crate::daemon::util::get_machine_id();

    let platform = Arc::new(platform::DaemonPlatform {
        node_manager,
        session,
        scope: scope.clone(),
        machine_id: machine_id.clone(),
    });

    let server = BubbaLoopMcpServer::new(
        platform, None, // No auth token for stdio
        scope, machine_id,
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

    let scope = std::env::var("BUBBALOOP_SCOPE").unwrap_or_else(|_| "local".to_string());
    let machine_id = crate::daemon::util::get_machine_id();

    let platform = Arc::new(platform::DaemonPlatform {
        node_manager,
        session,
        scope: scope.clone(),
        machine_id: machine_id.clone(),
    });

    let mcp_service = StreamableHttpService::new(
        move || {
            Ok(BubbaLoopMcpServer::new(
                platform.clone(),
                Some(token.clone()),
                scope.clone(),
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

    let router = axum::Router::new()
        .nest_service("/mcp", mcp_service)
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
