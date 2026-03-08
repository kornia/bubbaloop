//! MCP tool parameter types and handler implementations.
//!
//! All `#[tool]`-annotated methods live here, dispatched by the `ToolRouter`
//! built via `#[tool_router]` on `BubbaLoopMcpServer`.

use super::platform::{self, PlatformOperations};
use super::BubbaLoopMcpServer;
use crate::validation;
use rmcp::handler::server::wrapper::Parameters;
use rmcp::model::*;
use rmcp::{tool, tool_router};
use schemars::JsonSchema;
use serde::Deserialize;

// ── Tool parameter types ──────────────────────────────────────────

#[derive(Deserialize, JsonSchema)]
pub(crate) struct NodeNameRequest {
    /// Name of the node (e.g., "rtsp-camera", "openmeteo")
    node_name: String,
}

#[derive(Deserialize, JsonSchema)]
pub(crate) struct SendCommandRequest {
    /// Name of the node to send the command to
    node_name: String,
    /// Command name (must be listed in the node's manifest)
    command: String,
    /// Optional JSON parameters for the command
    #[serde(default)]
    params: serde_json::Value,
}

#[derive(Deserialize, JsonSchema)]
pub(crate) struct QueryTopicRequest {
    /// Full Zenoh key expression to query (e.g., "bubbaloop/local/nvidia_orin00/openmeteo/status")
    key_expr: String,
}

#[derive(Deserialize, JsonSchema)]
pub(crate) struct InstallNodeRequest {
    /// Source path: local directory path or GitHub "user/repo" format
    source: String,
}

#[derive(Debug, Deserialize, JsonSchema)]
pub(crate) struct DiscoverCapabilitiesParams {
    /// Filter by capability type: "sensor", "actuator", "processor", "gateway". Omit for all.
    #[serde(default)]
    capability: Option<String>,
}

#[derive(Debug, Deserialize, JsonSchema)]
pub(crate) struct ListProposalsParams {
    /// Filter by status: "pending", "approved", "rejected". Omit for all.
    #[serde(default)]
    status: Option<String>,
}

#[derive(Debug, Deserialize, JsonSchema)]
pub(crate) struct ProposalIdRequest {
    /// ID of the proposal.
    proposal_id: String,
    /// Who is making this decision (e.g., "user", "mcp-client").
    #[serde(default = "default_decided_by")]
    decided_by: String,
}

fn default_decided_by() -> String {
    "mcp".to_string()
}

#[derive(Debug, Deserialize, JsonSchema)]
pub(crate) struct ListJobsParams {
    /// Filter by status: "pending", "running", "completed", "failed". Omit for all.
    #[serde(default)]
    status: Option<String>,
}

#[derive(Debug, Deserialize, JsonSchema)]
pub(crate) struct DeleteJobRequest {
    /// ID of the job to delete.
    job_id: String,
}

#[derive(Debug, Deserialize, JsonSchema)]
pub(crate) struct ClearEpisodicMemoryRequest {
    /// Delete episodic log files older than this many days.
    older_than_days: u32,
}

#[derive(Debug, Deserialize, JsonSchema)]
pub(crate) struct MissionIdRequest {
    /// ID of the mission.
    mission_id: String,
}

#[derive(Debug, Deserialize, JsonSchema)]
pub(crate) struct RegisterAlertRequest {
    /// Mission this alert is attached to.
    mission_id: String,
    /// World state predicate expression (e.g. "toddler.near_stairs = 'true'").
    predicate: String,
    /// Minimum seconds between consecutive firings (default: 60).
    #[serde(default)]
    debounce_secs: Option<u32>,
    /// Arousal boost when rule fires (default: 2.0).
    #[serde(default)]
    arousal_boost: Option<f64>,
    /// Human-readable description of this alert.
    description: String,
}

#[derive(Debug, Deserialize, JsonSchema)]
pub(crate) struct AlertIdRequest {
    /// ID of the alert to unregister.
    alert_id: String,
}

#[derive(Debug, Deserialize, JsonSchema)]
pub(crate) struct ConfigureContextRequest {
    /// Mission this provider is attached to.
    mission_id: String,
    /// Zenoh key expression pattern (e.g. "bubbaloop/**/vision/detections").
    topic_pattern: String,
    /// Template for world state key (e.g. "{label}.location").
    world_state_key_template: String,
    /// JSON field path to extract as the value.
    value_field: String,
    /// Optional filter expression (e.g. "label=dog AND confidence>0.85").
    #[serde(default)]
    filter: Option<String>,
    /// Minimum interval between writes for the same key (seconds).
    #[serde(default)]
    min_interval_secs: Option<u32>,
    /// Maximum age before a world state entry is considered stale (seconds).
    #[serde(default)]
    max_age_secs: Option<u32>,
    /// Optional JSON field path to extract confidence from.
    #[serde(default)]
    confidence_field: Option<String>,
    /// Approximate token budget for this provider's world state entries.
    #[serde(default)]
    token_budget: Option<u32>,
}

#[derive(Debug, Deserialize, JsonSchema)]
pub(crate) struct RegisterConstraintRequest {
    /// Mission this constraint is attached to.
    mission_id: String,
    /// Constraint type: "workspace", "max_velocity", "forbidden_zone", "max_force"
    constraint_type: String,
    /// JSON object with constraint-specific fields.
    params_json: String,
}

#[derive(Debug, Deserialize, JsonSchema)]
pub(crate) struct ListConstraintsRequest {
    /// Mission ID to list constraints for.
    mission_id: String,
}

// ── Tool implementations ──────────────────────────────────────────

#[tool_router]
impl<P: PlatformOperations> BubbaLoopMcpServer<P> {
    pub fn new(
        platform: std::sync::Arc<P>,
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
        description = "Install a node from the marketplace, a local path, or GitHub repository. Accepts a marketplace name (e.g., 'rtsp-camera'), a local directory path (e.g., '/path/to/my-node'), or GitHub format (e.g., 'user/repo'). Downloads precompiled binaries when available, registers the node with the daemon, and creates the systemd service. Admin only."
    )]
    async fn install_node(
        &self,
        Parameters(req): Parameters<InstallNodeRequest>,
    ) -> Result<CallToolResult, rmcp::ErrorData> {
        log::info!("[MCP] tool=install_node source={}", req.source);

        // Route based on source format:
        // - Simple name (no `/`, no `.` prefix, valid node name) → marketplace
        // - Path/URL → existing install_node flow
        let is_marketplace_name = !req.source.contains('/')
            && !req.source.starts_with('.')
            && validation::validate_node_name(&req.source).is_ok();

        let result = if is_marketplace_name {
            self.platform.install_from_marketplace(&req.source).await
        } else {
            if let Err(e) = validation::validate_install_source(&req.source) {
                return Ok(CallToolResult::success(vec![Content::text(format!(
                    "Error: {}",
                    e
                ))]));
            }
            self.platform.install_node(&req.source).await
        };

        match result {
            Ok(msg) => Ok(CallToolResult::success(vec![Content::text(msg)])),
            Err(e) => Ok(CallToolResult::success(vec![Content::text(format!(
                "Error: {}",
                e
            ))])),
        }
    }

    #[tool(
        description = "Remove a registered node. Stops the node first if it is running, then removes it from the daemon registry. Admin only."
    )]
    async fn remove_node(
        &self,
        Parameters(req): Parameters<NodeNameRequest>,
    ) -> Result<CallToolResult, rmcp::ErrorData> {
        log::info!("[MCP] tool=remove_node node={}", req.node_name);
        if let Err(e) = validation::validate_node_name(&req.node_name) {
            return Ok(CallToolResult::success(vec![Content::text(e)]));
        }
        match self.platform.remove_node(&req.node_name).await {
            Ok(msg) => Ok(CallToolResult::success(vec![Content::text(msg)])),
            Err(e) => Ok(CallToolResult::success(vec![Content::text(format!(
                "Error: {}",
                e
            ))])),
        }
    }

    #[tool(
        description = "Uninstall a node's systemd service. Stops the node if running and removes the systemd service file, but keeps the node registered. Admin only."
    )]
    async fn uninstall_node(
        &self,
        Parameters(req): Parameters<NodeNameRequest>,
    ) -> Result<CallToolResult, rmcp::ErrorData> {
        log::info!("[MCP] tool=uninstall_node node={}", req.node_name);
        if let Err(e) = validation::validate_node_name(&req.node_name) {
            return Ok(CallToolResult::success(vec![Content::text(e)]));
        }
        match self
            .platform
            .execute_command(&req.node_name, platform::NodeCommand::Uninstall)
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
        description = "Clean a node's build artifacts and cached data. Useful for forcing a rebuild. Admin only."
    )]
    async fn clean_node(
        &self,
        Parameters(req): Parameters<NodeNameRequest>,
    ) -> Result<CallToolResult, rmcp::ErrorData> {
        log::info!("[MCP] tool=clean_node node={}", req.node_name);
        if let Err(e) = validation::validate_node_name(&req.node_name) {
            return Ok(CallToolResult::success(vec![Content::text(e)]));
        }
        match self
            .platform
            .execute_command(&req.node_name, platform::NodeCommand::Clean)
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
        description = "Enable autostart for a node. The node's systemd service will start automatically on boot."
    )]
    async fn enable_autostart(
        &self,
        Parameters(req): Parameters<NodeNameRequest>,
    ) -> Result<CallToolResult, rmcp::ErrorData> {
        log::info!("[MCP] tool=enable_autostart node={}", req.node_name);
        if let Err(e) = validation::validate_node_name(&req.node_name) {
            return Ok(CallToolResult::success(vec![Content::text(e)]));
        }
        match self
            .platform
            .execute_command(&req.node_name, platform::NodeCommand::EnableAutostart)
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
        description = "Disable autostart for a node. The node's systemd service will no longer start automatically on boot."
    )]
    async fn disable_autostart(
        &self,
        Parameters(req): Parameters<NodeNameRequest>,
    ) -> Result<CallToolResult, rmcp::ErrorData> {
        log::info!("[MCP] tool=disable_autostart node={}", req.node_name);
        if let Err(e) = validation::validate_node_name(&req.node_name) {
            return Ok(CallToolResult::success(vec![Content::text(e)]));
        }
        match self
            .platform
            .execute_command(&req.node_name, platform::NodeCommand::DisableAutostart)
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

    // ── Agent proposal tools ─────────────────────────────────────────

    #[tool(
        description = "List agent proposals awaiting human approval. Optionally filter by status: 'pending', 'approved', 'rejected'."
    )]
    async fn list_proposals(
        &self,
        Parameters(params): Parameters<ListProposalsParams>,
    ) -> Result<CallToolResult, rmcp::ErrorData> {
        log::info!("[MCP] tool=list_proposals filter={:?}", params.status);
        match self.platform.list_proposals(params.status.as_deref()).await {
            Ok(result) => Ok(CallToolResult::success(vec![Content::text(result)])),
            Err(e) => Ok(CallToolResult::success(vec![Content::text(format!(
                "Error: {}",
                e
            ))])),
        }
    }

    #[tool(
        description = "Approve a pending agent proposal. The proposed actions will be executed by the agent on its next heartbeat."
    )]
    async fn approve_proposal(
        &self,
        Parameters(req): Parameters<ProposalIdRequest>,
    ) -> Result<CallToolResult, rmcp::ErrorData> {
        log::info!(
            "[MCP] tool=approve_proposal id={} by={}",
            req.proposal_id,
            req.decided_by
        );
        match self
            .platform
            .approve_proposal(&req.proposal_id, &req.decided_by)
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
        description = "Reject a pending agent proposal. The proposed actions will be discarded."
    )]
    async fn reject_proposal(
        &self,
        Parameters(req): Parameters<ProposalIdRequest>,
    ) -> Result<CallToolResult, rmcp::ErrorData> {
        log::info!(
            "[MCP] tool=reject_proposal id={} by={}",
            req.proposal_id,
            req.decided_by
        );
        match self
            .platform
            .reject_proposal(&req.proposal_id, &req.decided_by)
            .await
        {
            Ok(msg) => Ok(CallToolResult::success(vec![Content::text(msg)])),
            Err(e) => Ok(CallToolResult::success(vec![Content::text(format!(
                "Error: {}",
                e
            ))])),
        }
    }

    // ── Memory admin tools ──────────────────────────────────────────

    #[tool(
        description = "List agent jobs with optional status filter. Returns all scheduled, running, completed, and failed jobs."
    )]
    async fn list_jobs(
        &self,
        Parameters(params): Parameters<ListJobsParams>,
    ) -> Result<CallToolResult, rmcp::ErrorData> {
        log::info!("[MCP] tool=list_jobs filter={:?}", params.status);
        match self.platform.list_jobs(params.status.as_deref()).await {
            Ok(result) => Ok(CallToolResult::success(vec![Content::text(result)])),
            Err(e) => Ok(CallToolResult::success(vec![Content::text(format!(
                "Error: {}",
                e
            ))])),
        }
    }

    #[tool(
        description = "Delete a specific agent job by ID. Removes the job from the scheduler. Operator only."
    )]
    async fn delete_job(
        &self,
        Parameters(req): Parameters<DeleteJobRequest>,
    ) -> Result<CallToolResult, rmcp::ErrorData> {
        log::info!("[MCP] tool=delete_job id={}", req.job_id);
        match self.platform.delete_job(&req.job_id).await {
            Ok(msg) => Ok(CallToolResult::success(vec![Content::text(msg)])),
            Err(e) => Ok(CallToolResult::success(vec![Content::text(format!(
                "Error: {}",
                e
            ))])),
        }
    }

    #[tool(
        description = "Clear episodic memory logs older than the specified number of days. Removes NDJSON files and FTS5 index entries. Admin only."
    )]
    async fn clear_episodic_memory(
        &self,
        Parameters(req): Parameters<ClearEpisodicMemoryRequest>,
    ) -> Result<CallToolResult, rmcp::ErrorData> {
        log::info!(
            "[MCP] tool=clear_episodic_memory older_than_days={}",
            req.older_than_days
        );
        match self
            .platform
            .clear_episodic_memory(req.older_than_days)
            .await
        {
            Ok(msg) => Ok(CallToolResult::success(vec![Content::text(msg)])),
            Err(e) => Ok(CallToolResult::success(vec![Content::text(format!(
                "Error: {}",
                e
            ))])),
        }
    }

    // ── Context provider tools ────────────────────────────────────

    #[tool(
        description = "Configure a context provider: a daemon background task that watches a Zenoh topic pattern and writes extracted values to world state. The agent can then see live sensor data in its world state without LLM involvement. Admin only."
    )]
    async fn configure_context(
        &self,
        Parameters(req): Parameters<ConfigureContextRequest>,
    ) -> Result<CallToolResult, rmcp::ErrorData> {
        log::info!("[MCP] tool=configure_context mission_id={}", req.mission_id);

        if req.topic_pattern.is_empty() {
            return Ok(CallToolResult::success(vec![Content::text(
                "Error: topic_pattern must not be empty",
            )]));
        }
        if req.world_state_key_template.is_empty() {
            return Ok(CallToolResult::success(vec![Content::text(
                "Error: world_state_key_template must not be empty",
            )]));
        }

        let params = platform::ConfigureContextParams {
            mission_id: req.mission_id,
            topic_pattern: req.topic_pattern,
            world_state_key_template: req.world_state_key_template,
            value_field: req.value_field,
            filter: req.filter,
            min_interval_secs: req.min_interval_secs,
            max_age_secs: req.max_age_secs,
            confidence_field: req.confidence_field,
            token_budget: req.token_budget,
        };

        match self.platform.configure_context(params).await {
            Ok(msg) => Ok(CallToolResult::success(vec![Content::text(msg)])),
            Err(e) => Ok(CallToolResult::success(vec![Content::text(format!(
                "Error: {}",
                e
            ))])),
        }
    }

    // ── Mission lifecycle tools ─────────────────────────────────────

    #[tool(description = "List all missions with their status, expiry, and resources.")]
    async fn list_missions(&self) -> Result<CallToolResult, rmcp::ErrorData> {
        log::info!("[MCP] tool=list_missions");
        match self.platform.list_missions().await {
            Ok(missions) => Ok(CallToolResult::success(vec![Content::text(
                serde_json::to_string_pretty(&missions).unwrap_or_else(|_| "[]".to_string()),
            )])),
            Err(e) => Ok(CallToolResult::success(vec![Content::text(format!(
                "Error: {}",
                e
            ))])),
        }
    }

    #[tool(
        description = "Pause an active mission. The agent will stop working on it until resumed."
    )]
    async fn pause_mission(
        &self,
        Parameters(req): Parameters<MissionIdRequest>,
    ) -> Result<CallToolResult, rmcp::ErrorData> {
        log::info!("[MCP] tool=pause_mission id={}", req.mission_id);
        match self
            .platform
            .update_mission_status(req.mission_id, "paused".to_string())
            .await
        {
            Ok(msg) => Ok(CallToolResult::success(vec![Content::text(msg)])),
            Err(e) => Ok(CallToolResult::success(vec![Content::text(format!(
                "Error: {}",
                e
            ))])),
        }
    }

    #[tool(description = "Resume a paused mission. The agent will continue working on it.")]
    async fn resume_mission(
        &self,
        Parameters(req): Parameters<MissionIdRequest>,
    ) -> Result<CallToolResult, rmcp::ErrorData> {
        log::info!("[MCP] tool=resume_mission id={}", req.mission_id);
        match self
            .platform
            .update_mission_status(req.mission_id, "active".to_string())
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
        description = "Cancel an active or paused mission. The agent will stop working on it permanently."
    )]
    async fn cancel_mission(
        &self,
        Parameters(req): Parameters<MissionIdRequest>,
    ) -> Result<CallToolResult, rmcp::ErrorData> {
        log::info!("[MCP] tool=cancel_mission id={}", req.mission_id);
        match self
            .platform
            .update_mission_status(req.mission_id, "cancelled".to_string())
            .await
        {
            Ok(msg) => Ok(CallToolResult::success(vec![Content::text(msg)])),
            Err(e) => Ok(CallToolResult::success(vec![Content::text(format!(
                "Error: {}",
                e
            ))])),
        }
    }

    // ── Reactive alert tools ────────────────────────────────────────

    #[tool(
        description = "Register a reactive alert rule. When the world state matches the predicate, the agent's arousal spikes without an LLM call. Admin only."
    )]
    async fn register_alert(
        &self,
        Parameters(req): Parameters<RegisterAlertRequest>,
    ) -> Result<CallToolResult, rmcp::ErrorData> {
        log::info!("[MCP] tool=register_alert mission_id={}", req.mission_id);

        let params = platform::RegisterAlertParams {
            mission_id: req.mission_id,
            predicate: req.predicate,
            debounce_secs: req.debounce_secs,
            arousal_boost: req.arousal_boost,
            description: req.description,
        };

        match self.platform.register_alert(params).await {
            Ok(msg) => Ok(CallToolResult::success(vec![Content::text(msg)])),
            Err(e) => Ok(CallToolResult::success(vec![Content::text(format!(
                "Error: {}",
                e
            ))])),
        }
    }

    #[tool(description = "Unregister a reactive alert rule by ID. Admin only.")]
    async fn unregister_alert(
        &self,
        Parameters(req): Parameters<AlertIdRequest>,
    ) -> Result<CallToolResult, rmcp::ErrorData> {
        log::info!("[MCP] tool=unregister_alert id={}", req.alert_id);
        match self.platform.unregister_alert(req.alert_id).await {
            Ok(msg) => Ok(CallToolResult::success(vec![Content::text(msg)])),
            Err(e) => Ok(CallToolResult::success(vec![Content::text(format!(
                "Error: {}",
                e
            ))])),
        }
    }

    // ── Constraint tools ────────────────────────────────────────────

    #[tool(
        description = "Register a safety constraint for a mission. Constraints are checked before any actuator command (fail-closed). Admin only."
    )]
    async fn register_constraint(
        &self,
        Parameters(req): Parameters<RegisterConstraintRequest>,
    ) -> Result<CallToolResult, rmcp::ErrorData> {
        log::info!(
            "[MCP] tool=register_constraint mission_id={} type={}",
            req.mission_id,
            req.constraint_type
        );

        let params = platform::RegisterConstraintParams {
            mission_id: req.mission_id,
            constraint_type: req.constraint_type,
            params_json: req.params_json,
        };

        match self.platform.register_constraint(params).await {
            Ok(msg) => Ok(CallToolResult::success(vec![Content::text(msg)])),
            Err(e) => Ok(CallToolResult::success(vec![Content::text(format!(
                "Error: {}",
                e
            ))])),
        }
    }

    #[tool(description = "List all safety constraints for a mission. Viewer only.")]
    async fn list_constraints(
        &self,
        Parameters(req): Parameters<ListConstraintsRequest>,
    ) -> Result<CallToolResult, rmcp::ErrorData> {
        log::info!("[MCP] tool=list_constraints mission_id={}", req.mission_id);

        match self.platform.list_constraints(req.mission_id).await {
            Ok(constraints) => {
                let text = serde_json::to_string_pretty(
                    &constraints
                        .iter()
                        .map(|(id, c)| {
                            serde_json::json!({
                                "id": id,
                                "constraint": c,
                            })
                        })
                        .collect::<Vec<_>>(),
                )
                .unwrap_or_else(|_| "[]".to_string());
                Ok(CallToolResult::success(vec![Content::text(text)]))
            }
            Err(e) => Ok(CallToolResult::success(vec![Content::text(format!(
                "Error: {}",
                e
            ))])),
        }
    }
}
