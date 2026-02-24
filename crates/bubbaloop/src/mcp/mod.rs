//! MCP (Model Context Protocol) server for AI agent integration.
//!
//! Exposes bubbaloop node operations as MCP tools that any LLM can call.
//! Runs as an HTTP server on port 8088 inside the daemon process.

use crate::agent::Agent;
use crate::daemon::node_manager::NodeManager;
use crate::schemas::daemon::v1::{CommandType, NodeCommand};
use crate::validation;
use rmcp::handler::server::tool::ToolRouter;
use rmcp::handler::server::wrapper::Parameters;
use rmcp::model::*;
use rmcp::{tool, tool_handler, tool_router, ServerHandler};
use schemars::JsonSchema;
use serde::Deserialize;
use std::sync::Arc;
use zenoh::Session;

/// Default port for the MCP HTTP server.
pub const MCP_PORT: u16 = 8088;

/// Bubbaloop MCP server — wraps Zenoh operations as MCP tools.
#[derive(Clone)]
pub struct BubbaLoopMcpServer {
    session: Arc<Session>,
    node_manager: Arc<NodeManager>,
    agent: Option<Arc<Agent>>,
    tool_router: ToolRouter<Self>,
    scope: String,
    machine_id: String,
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

#[derive(Deserialize, JsonSchema)]
struct AddRuleRequest {
    /// Rule name (unique identifier)
    name: String,
    /// Zenoh key expression pattern to subscribe to (e.g., "bubbaloop/**/telemetry/status")
    trigger: String,
    /// Optional condition: JSON object with "field", "operator" (==, !=, >, <, >=, <=, contains), "value"
    #[serde(default)]
    condition: Option<serde_json::Value>,
    /// Action: JSON object with "type" (log/command/publish) and type-specific fields
    action: serde_json::Value,
}

#[derive(Deserialize, JsonSchema)]
struct RuleNameRequest {
    /// Name of the rule to remove or look up
    rule_name: String,
}

// ── Tool implementations ──────────────────────────────────────────

#[tool_router]
impl BubbaLoopMcpServer {
    pub fn new(
        session: Arc<Session>,
        node_manager: Arc<NodeManager>,
        agent: Option<Arc<Agent>>,
        scope: String,
        machine_id: String,
    ) -> Self {
        Self {
            session,
            node_manager,
            agent,
            tool_router: Self::tool_router(),
            scope,
            machine_id,
        }
    }

    #[tool(description = "List all registered nodes with their status, capabilities, and topics. Returns node name, status (running/stopped/etc), type, and whether it's built.")]
    async fn list_nodes(&self) -> Result<CallToolResult, rmcp::ErrorData> {
        let node_list = self.node_manager.get_node_list().await;
        let nodes: Vec<serde_json::Value> = node_list
            .nodes
            .iter()
            .map(|n| {
                serde_json::json!({
                    "name": n.name,
                    "status": format!("{:?}", crate::schemas::daemon::v1::NodeStatus::try_from(n.status).unwrap_or(crate::schemas::daemon::v1::NodeStatus::Unknown)),
                    "installed": n.installed,
                    "is_built": n.is_built,
                    "node_type": n.node_type,
                    "path": n.path,
                    "machine_id": n.machine_id,
                })
            })
            .collect();
        Ok(CallToolResult::success(vec![Content::text(
            serde_json::to_string_pretty(&nodes).unwrap_or_else(|_| "[]".to_string()),
        )]))
    }

    #[tool(description = "Get detailed health status of a specific node including uptime.")]
    async fn get_node_health(
        &self,
        Parameters(req): Parameters<NodeNameRequest>,
    ) -> Result<CallToolResult, rmcp::ErrorData> {
        if let Err(e) = validation::validate_node_name(&req.node_name) {
            return Ok(CallToolResult::success(vec![Content::text(e)]));
        }
        match self.node_manager.get_node(&req.node_name).await {
            Some(node) => {
                let health = serde_json::json!({
                    "name": node.name,
                    "status": format!("{:?}", crate::schemas::daemon::v1::NodeStatus::try_from(node.status).unwrap_or(crate::schemas::daemon::v1::NodeStatus::Unknown)),
                    "health_status": format!("{:?}", crate::schemas::daemon::v1::HealthStatus::try_from(node.health_status).unwrap_or(crate::schemas::daemon::v1::HealthStatus::Unknown)),
                    "last_health_check_ms": node.last_health_check_ms,
                    "last_updated_ms": node.last_updated_ms,
                });
                Ok(CallToolResult::success(vec![Content::text(
                    serde_json::to_string_pretty(&health).unwrap_or_default(),
                )]))
            }
            None => Ok(CallToolResult::success(vec![Content::text(format!(
                "Node '{}' not found",
                req.node_name
            ))])),
        }
    }

    #[tool(description = "Get the current configuration of a node by querying its Zenoh config queryable.")]
    async fn get_node_config(
        &self,
        Parameters(req): Parameters<NodeNameRequest>,
    ) -> Result<CallToolResult, rmcp::ErrorData> {
        if let Err(e) = validation::validate_node_name(&req.node_name) {
            return Ok(CallToolResult::success(vec![Content::text(e)]));
        }
        let result = self.zenoh_get_text(&format!("bubbaloop/{}/{}/{}/config", self.scope, self.machine_id, req.node_name)).await;
        Ok(CallToolResult::success(vec![Content::text(result)]))
    }

    #[tool(description = "Get the manifest (self-description) of a node including its capabilities, published topics, commands, and hardware requirements.")]
    async fn get_node_manifest(
        &self,
        Parameters(req): Parameters<NodeNameRequest>,
    ) -> Result<CallToolResult, rmcp::ErrorData> {
        if let Err(e) = validation::validate_node_name(&req.node_name) {
            return Ok(CallToolResult::success(vec![Content::text(e)]));
        }
        let result = self.zenoh_get_text(&format!("bubbaloop/{}/{}/{}/manifest", self.scope, self.machine_id, req.node_name)).await;
        Ok(CallToolResult::success(vec![Content::text(result)]))
    }

    #[tool(description = "List available commands for a specific node with their parameters and descriptions. Use this before send_command to discover what actions a node supports.")]
    async fn list_commands(
        &self,
        Parameters(req): Parameters<NodeNameRequest>,
    ) -> Result<CallToolResult, rmcp::ErrorData> {
        if let Err(e) = validation::validate_node_name(&req.node_name) {
            return Ok(CallToolResult::success(vec![Content::text(e)]));
        }
        let manifest_text = self.zenoh_get_text(&format!("bubbaloop/{}/{}/{}/manifest", self.scope, self.machine_id, req.node_name)).await;
        // Try to parse the manifest and extract commands
        // The manifest text is formatted as "[key_expr] json_body" from zenoh_get_text
        let commands = manifest_text
            .lines()
            .filter_map(|line| {
                // zenoh_get_text formats as "[key] json"
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
            _ => Ok(CallToolResult::success(vec![Content::text(
                format!("No commands available for node '{}'", req.node_name),
            )])),
        }
    }

    #[tool(description = "Send a command to a node's command queryable. The node must support the command — call list_commands first to see available commands. Example: node_name='rtsp-camera', command='capture_frame', params={\"resolution\": \"1080p\"}. Returns the command result or error.")]
    async fn send_command(
        &self,
        Parameters(req): Parameters<SendCommandRequest>,
    ) -> Result<CallToolResult, rmcp::ErrorData> {
        if let Err(e) = validation::validate_node_name(&req.node_name) {
            return Ok(CallToolResult::success(vec![Content::text(e)]));
        }
        let key_expr = format!("bubbaloop/{}/{}/{}/command", self.scope, self.machine_id, req.node_name);
        let payload = serde_json::json!({
            "command": req.command,
            "params": req.params,
        });
        let payload_bytes = serde_json::to_vec(&payload).unwrap_or_default();

        match self
            .session
            .get(&key_expr)
            .payload(zenoh::bytes::ZBytes::from(payload_bytes))
            .timeout(std::time::Duration::from_secs(5))
            .await
        {
            Ok(replies) => {
                let mut results = Vec::new();
                while let Ok(reply) = replies.recv_async().await {
                    match reply.result() {
                        Ok(sample) => {
                            let bytes = sample.payload().to_bytes();
                            match String::from_utf8(bytes.to_vec()) {
                                Ok(text) => results.push(text),
                                Err(_) => results.push(format!("<{} bytes binary>", bytes.len())),
                            }
                        }
                        Err(err) => {
                            results.push(format!("Error: {:?}", err.payload().to_bytes()));
                        }
                    }
                }
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
                "Zenoh query failed: {}",
                e
            ))])),
        }
    }

    #[tool(description = "Start a stopped node via the daemon. The node must be installed and built.")]
    async fn start_node(
        &self,
        Parameters(req): Parameters<NodeNameRequest>,
    ) -> Result<CallToolResult, rmcp::ErrorData> {
        if let Err(e) = validation::validate_node_name(&req.node_name) {
            return Ok(CallToolResult::success(vec![Content::text(e)]));
        }
        let result = self.execute_daemon_command(CommandType::Start, &req.node_name).await;
        Ok(CallToolResult::success(vec![Content::text(result)]))
    }

    #[tool(description = "Stop a running node via the daemon.")]
    async fn stop_node(
        &self,
        Parameters(req): Parameters<NodeNameRequest>,
    ) -> Result<CallToolResult, rmcp::ErrorData> {
        if let Err(e) = validation::validate_node_name(&req.node_name) {
            return Ok(CallToolResult::success(vec![Content::text(e)]));
        }
        let result = self.execute_daemon_command(CommandType::Stop, &req.node_name).await;
        Ok(CallToolResult::success(vec![Content::text(result)]))
    }

    #[tool(description = "Restart a node (stop then start).")]
    async fn restart_node(
        &self,
        Parameters(req): Parameters<NodeNameRequest>,
    ) -> Result<CallToolResult, rmcp::ErrorData> {
        if let Err(e) = validation::validate_node_name(&req.node_name) {
            return Ok(CallToolResult::success(vec![Content::text(e)]));
        }
        let result = self.execute_daemon_command(CommandType::Restart, &req.node_name).await;
        Ok(CallToolResult::success(vec![Content::text(result)]))
    }

    #[tool(description = "Get the latest logs from a node's systemd service.")]
    async fn get_node_logs(
        &self,
        Parameters(req): Parameters<NodeNameRequest>,
    ) -> Result<CallToolResult, rmcp::ErrorData> {
        if let Err(e) = validation::validate_node_name(&req.node_name) {
            return Ok(CallToolResult::success(vec![Content::text(e)]));
        }
        let result = self.execute_daemon_command(CommandType::GetLogs, &req.node_name).await;
        Ok(CallToolResult::success(vec![Content::text(result)]))
    }

    #[tool(description = "Query a Zenoh key expression (admin only). Key must start with 'bubbaloop/'. Returns up to 100 results.")]
    async fn query_zenoh(
        &self,
        Parameters(req): Parameters<QueryTopicRequest>,
    ) -> Result<CallToolResult, rmcp::ErrorData> {
        if let Err(e) = crate::validation::validate_query_key_expr(&req.key_expr) {
            return Ok(CallToolResult::success(vec![Content::text(format!(
                "Validation error: {}",
                e
            ))]));
        }
        let result = self.zenoh_get_text(&req.key_expr).await;
        Ok(CallToolResult::success(vec![Content::text(result)]))
    }

    #[tool(description = "Discover all nodes across all machines by querying manifests. Returns a list of all self-describing nodes with their capabilities.")]
    async fn discover_nodes(&self) -> Result<CallToolResult, rmcp::ErrorData> {
        let result = self.zenoh_get_text("bubbaloop/**/manifest").await;
        Ok(CallToolResult::success(vec![Content::text(result)]))
    }

    #[tool(description = "Get the agent's current status including active rules, recent triggers, and human overrides.")]
    async fn get_agent_status(&self) -> Result<CallToolResult, rmcp::ErrorData> {
        match &self.agent {
            Some(agent) => {
                let status = agent.get_status().await;
                Ok(CallToolResult::success(vec![Content::text(
                    serde_json::to_string_pretty(&status).unwrap_or_else(|_| "{}".to_string()),
                )]))
            }
            None => Ok(CallToolResult::success(vec![Content::text(
                "Agent not available",
            )])),
        }
    }

    #[tool(description = "List all agent rules with their triggers, conditions, and actions.")]
    async fn list_agent_rules(&self) -> Result<CallToolResult, rmcp::ErrorData> {
        match &self.agent {
            Some(agent) => {
                let rules = agent.get_rules().await;
                Ok(CallToolResult::success(vec![Content::text(
                    serde_json::to_string_pretty(&rules).unwrap_or_else(|_| "[]".to_string()),
                )]))
            }
            None => Ok(CallToolResult::success(vec![Content::text(
                "Agent not available",
            )])),
        }
    }

    #[tool(description = "Add a new rule to the agent rule engine. Rules trigger actions when sensor data matches conditions. Example: trigger=\"bubbaloop/**/telemetry/status\", condition={\"field\": \"cpu_temp\", \"operator\": \">\", \"value\": 80}, action={\"type\": \"log\", \"message\": \"CPU too hot!\"}")]
    async fn add_rule(
        &self,
        Parameters(req): Parameters<AddRuleRequest>,
    ) -> Result<CallToolResult, rmcp::ErrorData> {
        match &self.agent {
            Some(agent) => {
                let rule = match Self::parse_rule_request(req) {
                    Ok(r) => r,
                    Err(msg) => return Ok(CallToolResult::success(vec![Content::text(msg)])),
                };
                match agent.add_rule(rule).await {
                    Ok(msg) => Ok(CallToolResult::success(vec![Content::text(msg)])),
                    Err(e) => Ok(CallToolResult::success(vec![Content::text(format!("Error: {}", e))])),
                }
            }
            None => Ok(CallToolResult::success(vec![Content::text("Agent not available")])),
        }
    }

    #[tool(description = "Remove a rule from the agent rule engine by name.")]
    async fn remove_rule(
        &self,
        Parameters(req): Parameters<RuleNameRequest>,
    ) -> Result<CallToolResult, rmcp::ErrorData> {
        if let Err(e) = validation::validate_rule_name(&req.rule_name) {
            return Ok(CallToolResult::success(vec![Content::text(e)]));
        }
        match &self.agent {
            Some(agent) => {
                match agent.remove_rule(&req.rule_name).await {
                    Ok(msg) => Ok(CallToolResult::success(vec![Content::text(msg)])),
                    Err(e) => Ok(CallToolResult::success(vec![Content::text(format!("Error: {}", e))])),
                }
            }
            None => Ok(CallToolResult::success(vec![Content::text("Agent not available")])),
        }
    }

    #[tool(description = "Update an existing rule in the agent rule engine. Provide the full rule definition — it replaces the rule with the same name.")]
    async fn update_rule(
        &self,
        Parameters(req): Parameters<AddRuleRequest>,
    ) -> Result<CallToolResult, rmcp::ErrorData> {
        match &self.agent {
            Some(agent) => {
                let rule = match Self::parse_rule_request(req) {
                    Ok(r) => r,
                    Err(msg) => return Ok(CallToolResult::success(vec![Content::text(msg)])),
                };
                match agent.update_rule(rule).await {
                    Ok(msg) => Ok(CallToolResult::success(vec![Content::text(msg)])),
                    Err(e) => Ok(CallToolResult::success(vec![Content::text(format!("Error: {}", e))])),
                }
            }
            None => Ok(CallToolResult::success(vec![Content::text("Agent not available")])),
        }
    }
}

// ── ServerHandler implementation ──────────────────────────────────

#[tool_handler]
impl ServerHandler for BubbaLoopMcpServer {
    fn get_info(&self) -> ServerInfo {
        ServerInfo {
            protocol_version: ProtocolVersion::V_2024_11_05,
            capabilities: ServerCapabilities::builder().enable_tools().build(),
            server_info: Implementation::from_build_env(),
            instructions: Some(
                "Bubbaloop physical AI orchestration platform. Recommended workflow:\n\
                 1) list_nodes — see all registered nodes with status\n\
                 2) list_commands or get_node_manifest — discover a node's capabilities and commands\n\
                 3) send_command — trigger actions on a node (command names from step 2)\n\
                 4) query_zenoh — one-off sensor data reads from any Zenoh key expression\n\
                 Node lifecycle: start_node, stop_node, restart_node, get_node_logs\n\
                 Agent rules: get_agent_status, list_agent_rules, add_rule, remove_rule, update_rule\n\
                 Fleet discovery: discover_nodes — find all self-describing nodes across machines"
                    .into(),
            ),
        }
    }
}

// ── Helper methods ────────────────────────────────────────────────

impl BubbaLoopMcpServer {
    /// Validate and parse an AddRuleRequest into a Rule.
    /// Returns Err(String) with a user-facing message on validation failure.
    fn parse_rule_request(req: AddRuleRequest) -> Result<crate::agent::Rule, String> {
        validation::validate_rule_name(&req.name)?;
        validation::validate_trigger_pattern(&req.trigger)?;
        let condition = match req.condition {
            Some(v) => Some(serde_json::from_value(v).map_err(|e| {
                format!(
                    "Invalid condition format: {}. Use {{\"field\": \"...\", \"operator\": \">\", \"value\": ...}}",
                    e
                )
            })?),
            None => None,
        };
        let action: crate::agent::Action =
            serde_json::from_value(req.action).map_err(|e| {
                format!(
                    "Invalid action format: {}. Use {{\"type\": \"log\", \"message\": \"...\"}} or {{\"type\": \"command\", \"node\": \"...\", \"command\": \"...\"}}",
                    e
                )
            })?;
        Ok(crate::agent::Rule {
            name: req.name,
            trigger: req.trigger,
            condition,
            action,
            enabled: true,
        })
    }

    /// Execute a daemon command (start/stop/restart/etc.) via the NodeManager.
    async fn execute_daemon_command(&self, cmd_type: CommandType, node_name: &str) -> String {
        let cmd = NodeCommand {
            command: cmd_type as i32,
            node_name: node_name.to_string(),
            request_id: uuid::Uuid::new_v4().to_string(),
            timestamp_ms: now_ms(),
            source_machine: "mcp".to_string(),
            target_machine: String::new(),
            node_path: String::new(),
            name_override: String::new(),
            config_override: String::new(),
        };

        let result = self.node_manager.execute_command(cmd).await;
        if result.success {
            if result.output.is_empty() {
                result.message
            } else {
                format!("{}\n{}", result.message, result.output)
            }
        } else {
            format!("Error: {}", result.message)
        }
    }

    /// Query a Zenoh key expression and return text results.
    async fn zenoh_get_text(&self, key_expr: &str) -> String {
        match self
            .session
            .get(key_expr)
            .timeout(std::time::Duration::from_secs(3))
            .await
        {
            Ok(replies) => {
                let mut results = Vec::new();
                while let Ok(reply) = replies.recv_async().await {
                    match reply.result() {
                        Ok(sample) => {
                            let key = sample.key_expr().to_string();
                            let bytes = sample.payload().to_bytes();
                            match String::from_utf8(bytes.to_vec()) {
                                Ok(text) => results.push(format!("[{}] {}", key, text)),
                                Err(_) => {
                                    results.push(format!("[{}] <{} bytes binary>", key, bytes.len()))
                                }
                            }
                        }
                        Err(err) => {
                            results.push(format!("Error: {:?}", err.payload().to_bytes()));
                        }
                    }
                }
                if results.is_empty() {
                    "No responses received (are nodes running?)".to_string()
                } else {
                    results.join("\n")
                }
            }
            Err(e) => format!("Zenoh query failed: {}", e),
        }
    }
}

fn now_ms() -> i64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis() as i64
}

/// Start the MCP HTTP server on the given port.
///
/// Mounts the StreamableHttpService at `/mcp` and blocks until shutdown.
pub async fn run_mcp_server(
    session: Arc<Session>,
    node_manager: Arc<NodeManager>,
    agent: Option<Arc<Agent>>,
    port: u16,
    mut shutdown_rx: tokio::sync::watch::Receiver<()>,
) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    use rmcp::transport::streamable_http_server::{
        session::local::LocalSessionManager, StreamableHttpService,
    };

    let session_for_factory = session;
    let manager_for_factory = node_manager;
    let agent_for_factory = agent;
    let scope = std::env::var("BUBBALOOP_SCOPE").unwrap_or_else(|_| "local".to_string());
    let machine_id = crate::daemon::util::get_machine_id();

    let mcp_service = StreamableHttpService::new(
        move || Ok(BubbaLoopMcpServer::new(
            session_for_factory.clone(),
            manager_for_factory.clone(),
            agent_for_factory.clone(),
            scope.clone(),
            machine_id.clone(),
        )),
        LocalSessionManager::default().into(),
        Default::default(),
    );

    let router = axum::Router::new().nest_service("/mcp", mcp_service);

    let bind_addr = format!("127.0.0.1:{}", port);
    let listener = tokio::net::TcpListener::bind(&bind_addr).await?;
    log::info!("MCP server listening on http://{}/mcp", bind_addr);

    axum::serve(listener, router)
        .with_graceful_shutdown(async move {
            shutdown_rx.changed().await.ok();
        })
        .await?;

    log::info!("MCP server stopped.");
    Ok(())
}
