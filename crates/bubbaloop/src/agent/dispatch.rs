//! Internal MCP tool dispatch — calls PlatformOperations directly.
//!
//! Maps tool names to platform method calls without going through the full
//! MCP server stack (avoids constructing `RequestContext`).

use crate::agent::claude::{ContentBlock, ToolDefinition};
use crate::mcp::platform::{NodeCommand, PlatformOperations};
use crate::validation;
use serde_json::{json, Value};
use std::sync::Arc;

/// Dispatches MCP tool calls directly to `PlatformOperations` methods.
pub struct Dispatcher<P: PlatformOperations> {
    platform: Arc<P>,
    scope: String,
    machine_id: String,
}

impl<P: PlatformOperations> Dispatcher<P> {
    /// Create a new dispatcher.
    pub fn new(platform: Arc<P>, scope: String, machine_id: String) -> Self {
        Self {
            platform,
            scope,
            machine_id,
        }
    }

    /// Returns Claude-compatible tool definitions for all 24 MCP tools.
    ///
    /// Names, descriptions, and schemas match the MCP server exactly.
    pub fn tool_definitions() -> Vec<ToolDefinition> {
        let empty_object = json!({
            "type": "object",
            "properties": {},
            "required": []
        });

        let node_name_schema = json!({
            "type": "object",
            "properties": {
                "node_name": {
                    "type": "string",
                    "description": "Name of the node (e.g., \"rtsp-camera\", \"openmeteo\")"
                }
            },
            "required": ["node_name"]
        });

        vec![
            // ── No-parameter tools ──────────────────────────────────
            ToolDefinition {
                name: "list_nodes".to_string(),
                description: "List all registered nodes with their status, capabilities, \
                    and topics. Returns node name, status (running/stopped/etc), type, \
                    and whether it's built."
                    .to_string(),
                input_schema: empty_object.clone(),
            },
            ToolDefinition {
                name: "discover_nodes".to_string(),
                description: "Discover all nodes across all machines by querying manifests. \
                    Returns a list of all self-describing nodes with their capabilities."
                    .to_string(),
                input_schema: empty_object.clone(),
            },
            ToolDefinition {
                name: "get_system_status".to_string(),
                description: "Get overall system status including daemon health, node count, \
                    and Zenoh connection state."
                    .to_string(),
                input_schema: empty_object.clone(),
            },
            ToolDefinition {
                name: "get_machine_info".to_string(),
                description: "Get machine hardware and OS information: architecture, \
                    hostname, OS version."
                    .to_string(),
                input_schema: empty_object,
            },
            // ── Single node_name tools ──────────────────────────────
            ToolDefinition {
                name: "get_node_health".to_string(),
                description: "Get detailed health status of a specific node including uptime."
                    .to_string(),
                input_schema: node_name_schema.clone(),
            },
            ToolDefinition {
                name: "get_node_config".to_string(),
                description: "Get the current configuration of a node by querying its \
                    Zenoh config queryable."
                    .to_string(),
                input_schema: node_name_schema.clone(),
            },
            ToolDefinition {
                name: "get_node_manifest".to_string(),
                description: "Get the full manifest for a node, including capabilities, \
                    published topics, commands, and hardware requirements."
                    .to_string(),
                input_schema: node_name_schema.clone(),
            },
            ToolDefinition {
                name: "list_commands".to_string(),
                description: "List available commands for a specific node with their \
                    parameters and descriptions. Use this before send_command to discover \
                    what actions a node supports."
                    .to_string(),
                input_schema: node_name_schema.clone(),
            },
            ToolDefinition {
                name: "start_node".to_string(),
                description: "Start a stopped node via the daemon. The node must be \
                    installed and built."
                    .to_string(),
                input_schema: node_name_schema.clone(),
            },
            ToolDefinition {
                name: "stop_node".to_string(),
                description: "Stop a running node via the daemon.".to_string(),
                input_schema: node_name_schema.clone(),
            },
            ToolDefinition {
                name: "restart_node".to_string(),
                description: "Restart a node (stop then start).".to_string(),
                input_schema: node_name_schema.clone(),
            },
            ToolDefinition {
                name: "get_node_logs".to_string(),
                description: "Get the latest logs from a node's systemd service.".to_string(),
                input_schema: node_name_schema.clone(),
            },
            ToolDefinition {
                name: "build_node".to_string(),
                description: "Trigger a build for a node. Builds the node's source code \
                    using its configured build command (Cargo, pixi, etc.). Admin only."
                    .to_string(),
                input_schema: node_name_schema.clone(),
            },
            ToolDefinition {
                name: "remove_node".to_string(),
                description: "Remove a registered node. Stops the node first if it is \
                    running, then removes it from the daemon registry. Admin only."
                    .to_string(),
                input_schema: node_name_schema.clone(),
            },
            ToolDefinition {
                name: "uninstall_node".to_string(),
                description: "Uninstall a node's systemd service. Stops the node if \
                    running and removes the systemd service file, but keeps the node \
                    registered. Admin only."
                    .to_string(),
                input_schema: node_name_schema.clone(),
            },
            ToolDefinition {
                name: "clean_node".to_string(),
                description: "Clean a node's build artifacts and cached data. Useful for \
                    forcing a rebuild. Admin only."
                    .to_string(),
                input_schema: node_name_schema.clone(),
            },
            ToolDefinition {
                name: "enable_autostart".to_string(),
                description: "Enable autostart for a node. The node's systemd service \
                    will start automatically on boot."
                    .to_string(),
                input_schema: node_name_schema.clone(),
            },
            ToolDefinition {
                name: "disable_autostart".to_string(),
                description: "Disable autostart for a node. The node's systemd service \
                    will no longer start automatically on boot."
                    .to_string(),
                input_schema: node_name_schema.clone(),
            },
            ToolDefinition {
                name: "get_stream_info".to_string(),
                description: "Get Zenoh connection parameters for subscribing to a node's \
                    data stream. Returns topic pattern, encoding, and endpoint. Use this \
                    to set up streaming data access outside MCP."
                    .to_string(),
                input_schema: node_name_schema.clone(),
            },
            ToolDefinition {
                name: "get_node_schema".to_string(),
                description: "Get the protobuf schema of a node's data messages. Returns \
                    the schema in human-readable format if available."
                    .to_string(),
                input_schema: node_name_schema,
            },
            // ── Custom-parameter tools ──────────────────────────────
            ToolDefinition {
                name: "send_command".to_string(),
                description: "Send a command to a node's command queryable. The node must \
                    support the command — call list_commands first to see available commands. \
                    Example: node_name='rtsp-camera', command='capture_frame', \
                    params={\"resolution\": \"1080p\"}. Returns the command result or error."
                    .to_string(),
                input_schema: json!({
                    "type": "object",
                    "properties": {
                        "node_name": {
                            "type": "string",
                            "description": "Name of the node to send the command to"
                        },
                        "command": {
                            "type": "string",
                            "description": "Command name (must be listed in the node's manifest)"
                        },
                        "params": {
                            "description": "Optional JSON parameters for the command"
                        }
                    },
                    "required": ["node_name", "command"]
                }),
            },
            ToolDefinition {
                name: "query_zenoh".to_string(),
                description: "Query a Zenoh key expression (admin only). Key must start \
                    with 'bubbaloop/'. Returns up to 100 results."
                    .to_string(),
                input_schema: json!({
                    "type": "object",
                    "properties": {
                        "key_expr": {
                            "type": "string",
                            "description": "Full Zenoh key expression to query (e.g., \
                                \"bubbaloop/local/nvidia_orin00/openmeteo/status\")"
                        }
                    },
                    "required": ["key_expr"]
                }),
            },
            ToolDefinition {
                name: "install_node".to_string(),
                description: "Install a node from the marketplace, a local path, or \
                    GitHub repository. Accepts a marketplace name (e.g., 'rtsp-camera'), \
                    a local directory path (e.g., '/path/to/my-node'), or GitHub format \
                    (e.g., 'user/repo'). Downloads precompiled binaries when available, \
                    registers the node with the daemon, and creates the systemd service. \
                    Admin only."
                    .to_string(),
                input_schema: json!({
                    "type": "object",
                    "properties": {
                        "source": {
                            "type": "string",
                            "description": "Source path: marketplace name, local directory \
                                path, or GitHub \"user/repo\" format"
                        }
                    },
                    "required": ["source"]
                }),
            },
            ToolDefinition {
                name: "discover_capabilities".to_string(),
                description: "Discover available node capabilities. Returns nodes grouped \
                    by capability type (sensor, actuator, processor, gateway). Optionally \
                    filter by a single capability type."
                    .to_string(),
                input_schema: json!({
                    "type": "object",
                    "properties": {
                        "capability": {
                            "type": "string",
                            "description": "Filter by capability type: \"sensor\", \
                                \"actuator\", \"processor\", \"gateway\". Omit for all."
                        }
                    },
                    "required": []
                }),
            },
        ]
    }

    /// Dispatch a tool call by name, returning a `ContentBlock::ToolResult`.
    ///
    /// Maps all 24 tool names to the corresponding `PlatformOperations` method.
    pub async fn call_tool(&self, tool_use_id: &str, name: &str, input: &Value) -> ContentBlock {
        let (text, is_error) = match name {
            // ── No-parameter tools ──────────────────────────────────
            "list_nodes" => self.handle_list_nodes().await,
            "discover_nodes" => self.handle_discover_nodes().await,
            "get_system_status" => self.handle_get_system_status().await,
            "get_machine_info" => self.handle_get_machine_info(),

            // ── Single node_name tools ──────────────────────────────
            "get_node_health" => self.handle_get_node_health(input).await,
            "get_node_config" => self.handle_get_node_config(input).await,
            "get_node_manifest" => self.handle_get_node_manifest(input).await,
            "list_commands" => self.handle_list_commands(input).await,
            "start_node" => self.handle_node_command(input, NodeCommand::Start).await,
            "stop_node" => self.handle_node_command(input, NodeCommand::Stop).await,
            "restart_node" => self.handle_node_command(input, NodeCommand::Restart).await,
            "get_node_logs" => self.handle_node_command(input, NodeCommand::GetLogs).await,
            "build_node" => self.handle_node_command(input, NodeCommand::Build).await,
            "remove_node" => self.handle_remove_node(input).await,
            "uninstall_node" => {
                self.handle_node_command(input, NodeCommand::Uninstall)
                    .await
            }
            "clean_node" => self.handle_node_command(input, NodeCommand::Clean).await,
            "enable_autostart" => {
                self.handle_node_command(input, NodeCommand::EnableAutostart)
                    .await
            }
            "disable_autostart" => {
                self.handle_node_command(input, NodeCommand::DisableAutostart)
                    .await
            }
            "get_stream_info" => self.handle_get_stream_info(input).await,
            "get_node_schema" => self.handle_get_node_schema(input).await,

            // ── Custom-parameter tools ──────────────────────────────
            "send_command" => self.handle_send_command(input).await,
            "query_zenoh" => self.handle_query_zenoh(input).await,
            "install_node" => self.handle_install_node(input).await,
            "discover_capabilities" => self.handle_discover_capabilities(input).await,

            _ => (format!("Unknown tool: {}", name), Some(true)),
        };

        ContentBlock::ToolResult {
            tool_use_id: tool_use_id.to_string(),
            content: text,
            is_error,
        }
    }

    /// List nodes and format as a text summary for the agent system prompt.
    pub async fn get_node_inventory(&self) -> String {
        match self.platform.list_nodes().await {
            Ok(nodes) if nodes.is_empty() => "No nodes registered.".to_string(),
            Ok(nodes) => {
                let mut lines = Vec::with_capacity(nodes.len() + 1);
                lines.push(format!("{} node(s) registered:", nodes.len()));
                for n in &nodes {
                    lines.push(format!(
                        "  - {} [status={}, health={}, type={}, installed={}, built={}]",
                        n.name, n.status, n.health, n.node_type, n.installed, n.is_built,
                    ));
                }
                lines.join("\n")
            }
            Err(e) => format!("Failed to list nodes: {}", e),
        }
    }

    // ── Internal helpers ─────────────────────────────────────────────

    /// Extract and validate `node_name` from JSON input.
    fn extract_node_name(input: &Value) -> Result<String, (String, Option<bool>)> {
        let name = input
            .get("node_name")
            .and_then(|v| v.as_str())
            .ok_or_else(|| {
                (
                    "Missing required parameter: node_name".to_string(),
                    Some(true),
                )
            })?;
        validation::validate_node_name(name).map_err(|e| (e, Some(true)))?;
        Ok(name.to_string())
    }

    /// Extract `node_name`, validate, and execute a `NodeCommand`.
    async fn handle_node_command(&self, input: &Value, cmd: NodeCommand) -> (String, Option<bool>) {
        let node_name = match Self::extract_node_name(input) {
            Ok(n) => n,
            Err(e) => return e,
        };
        match self.platform.execute_command(&node_name, cmd).await {
            Ok(msg) => (msg, None),
            Err(e) => (format!("Error: {}", e), Some(true)),
        }
    }

    // ── No-parameter tool handlers ───────────────────────────────────

    async fn handle_list_nodes(&self) -> (String, Option<bool>) {
        match self.platform.list_nodes().await {
            Ok(nodes) => {
                let json_nodes: Vec<Value> = nodes
                    .iter()
                    .map(|n| {
                        json!({
                            "name": n.name,
                            "status": n.status,
                            "health": n.health,
                            "installed": n.installed,
                            "is_built": n.is_built,
                            "node_type": n.node_type,
                        })
                    })
                    .collect();
                let text =
                    serde_json::to_string_pretty(&json_nodes).unwrap_or_else(|_| "[]".to_string());
                (text, None)
            }
            Err(e) => (format!("Error: {}", e), Some(true)),
        }
    }

    async fn handle_discover_nodes(&self) -> (String, Option<bool>) {
        match self.platform.query_zenoh("bubbaloop/**/manifest").await {
            Ok(result) => (result, None),
            Err(e) => (format!("Error: {}", e), Some(true)),
        }
    }

    async fn handle_get_system_status(&self) -> (String, Option<bool>) {
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
        let status = json!({
            "scope": self.scope,
            "machine_id": self.machine_id,
            "nodes_total": total,
            "nodes_running": running,
            "nodes_healthy": healthy,
            "mcp_server": "running",
        });
        let text = serde_json::to_string_pretty(&status).unwrap_or_else(|_| "{}".to_string());
        (text, None)
    }

    fn handle_get_machine_info(&self) -> (String, Option<bool>) {
        let info = json!({
            "machine_id": self.machine_id,
            "scope": self.scope,
            "arch": std::env::consts::ARCH,
            "os": std::env::consts::OS,
            "hostname": hostname::get()
                .ok()
                .and_then(|h| h.into_string().ok())
                .unwrap_or_else(|| "unknown".to_string()),
        });
        let text = serde_json::to_string_pretty(&info).unwrap_or_else(|_| "{}".to_string());
        (text, None)
    }

    // ── Node-name tool handlers ──────────────────────────────────────

    async fn handle_get_node_health(&self, input: &Value) -> (String, Option<bool>) {
        let node_name = match Self::extract_node_name(input) {
            Ok(n) => n,
            Err(e) => return e,
        };
        match self.platform.get_node_detail(&node_name).await {
            Ok(detail) => {
                let text = serde_json::to_string_pretty(&detail).unwrap_or_default();
                (text, None)
            }
            Err(crate::mcp::platform::PlatformError::NodeNotFound(_)) => {
                (format!("Node '{}' not found", node_name), Some(true))
            }
            Err(e) => (format!("Error: {}", e), Some(true)),
        }
    }

    async fn handle_get_node_config(&self, input: &Value) -> (String, Option<bool>) {
        let node_name = match Self::extract_node_name(input) {
            Ok(n) => n,
            Err(e) => return e,
        };
        match self.platform.get_node_config(&node_name).await {
            Ok(config) => {
                let text = serde_json::to_string_pretty(&config).unwrap_or_default();
                (text, None)
            }
            Err(e) => (format!("Error: {}", e), Some(true)),
        }
    }

    async fn handle_get_node_manifest(&self, input: &Value) -> (String, Option<bool>) {
        let node_name = match Self::extract_node_name(input) {
            Ok(n) => n,
            Err(e) => return e,
        };
        match self.platform.get_manifests(None).await {
            Ok(manifests) => {
                let found = manifests.into_iter().find(|(name, _)| name == &node_name);
                match found {
                    Some((_name, manifest)) => {
                        let text = serde_json::to_string_pretty(&manifest).unwrap_or_default();
                        (text, None)
                    }
                    None => (
                        format!("No manifest found for node '{}'", node_name),
                        Some(true),
                    ),
                }
            }
            Err(e) => (format!("Error: {}", e), Some(true)),
        }
    }

    async fn handle_list_commands(&self, input: &Value) -> (String, Option<bool>) {
        let node_name = match Self::extract_node_name(input) {
            Ok(n) => n,
            Err(e) => return e,
        };
        let key_expr = format!(
            "bubbaloop/{}/{}/{}/manifest",
            self.scope, self.machine_id, node_name
        );
        let manifest_text = match self.platform.query_zenoh(&key_expr).await {
            Ok(text) => text,
            Err(e) => return (format!("Error: {}", e), Some(true)),
        };
        // Try to parse the manifest and extract commands.
        // query_zenoh formats as "[key_expr] json_body".
        let commands = manifest_text
            .lines()
            .filter_map(|line| {
                let json_start = line.find(']').map(|i| i + 2)?;
                let json_str = line.get(json_start..)?;
                let manifest: Value = serde_json::from_str(json_str).ok()?;
                manifest.get("commands").cloned()
            })
            .next();

        match commands {
            Some(cmds) if cmds.is_array() && !cmds.as_array().unwrap().is_empty() => {
                let text = serde_json::to_string_pretty(&cmds).unwrap_or_else(|_| "[]".to_string());
                (text, None)
            }
            _ => (
                format!("No commands available for node '{}'", node_name),
                None,
            ),
        }
    }

    async fn handle_remove_node(&self, input: &Value) -> (String, Option<bool>) {
        let node_name = match Self::extract_node_name(input) {
            Ok(n) => n,
            Err(e) => return e,
        };
        match self.platform.remove_node(&node_name).await {
            Ok(msg) => (msg, None),
            Err(e) => (format!("Error: {}", e), Some(true)),
        }
    }

    async fn handle_get_stream_info(&self, input: &Value) -> (String, Option<bool>) {
        let node_name = match Self::extract_node_name(input) {
            Ok(n) => n,
            Err(e) => return e,
        };
        let info = json!({
            "zenoh_topic": format!(
                "bubbaloop/{}/{}/{}/**",
                self.scope, self.machine_id, node_name
            ),
            "encoding": "protobuf",
            "endpoint": "tcp/localhost:7447",
            "note": "Subscribe to this topic via Zenoh client library for real-time \
                data. MCP is control-plane only.",
        });
        let text = serde_json::to_string_pretty(&info).unwrap_or_else(|_| "{}".to_string());
        (text, None)
    }

    async fn handle_get_node_schema(&self, input: &Value) -> (String, Option<bool>) {
        let node_name = match Self::extract_node_name(input) {
            Ok(n) => n,
            Err(e) => return e,
        };
        let key = format!(
            "bubbaloop/{}/{}/{}/schema",
            self.scope, self.machine_id, node_name
        );
        match self.platform.query_zenoh(&key).await {
            Ok(result) => (result, None),
            Err(e) => (format!("Error: {}", e), Some(true)),
        }
    }

    // ── Custom-parameter tool handlers ───────────────────────────────

    async fn handle_send_command(&self, input: &Value) -> (String, Option<bool>) {
        let node_name = match Self::extract_node_name(input) {
            Ok(n) => n,
            Err(e) => return e,
        };
        let command = match input.get("command").and_then(|v| v.as_str()) {
            Some(cmd) => cmd.to_string(),
            None => {
                return (
                    "Missing required parameter: command".to_string(),
                    Some(true),
                )
            }
        };
        let params = input.get("params").cloned().unwrap_or(json!({}));

        let key_expr = format!(
            "bubbaloop/{}/{}/{}/command",
            self.scope, self.machine_id, node_name
        );
        let payload = json!({
            "command": command,
            "params": params,
        });
        let payload_bytes = serde_json::to_vec(&payload).unwrap_or_default();

        match self
            .platform
            .send_zenoh_query(&key_expr, payload_bytes)
            .await
        {
            Ok(results) => {
                if results.is_empty() {
                    ("No response from node (is it running?)".to_string(), None)
                } else {
                    (results.join("\n"), None)
                }
            }
            Err(e) => (format!("Error: {}", e), Some(true)),
        }
    }

    async fn handle_query_zenoh(&self, input: &Value) -> (String, Option<bool>) {
        let key_expr = match input.get("key_expr").and_then(|v| v.as_str()) {
            Some(k) => k.to_string(),
            None => {
                return (
                    "Missing required parameter: key_expr".to_string(),
                    Some(true),
                )
            }
        };
        if let Err(e) = validation::validate_query_key_expr(&key_expr) {
            return (format!("Validation error: {}", e), Some(true));
        }
        match self.platform.query_zenoh(&key_expr).await {
            Ok(result) => (result, None),
            Err(e) => (format!("Error: {}", e), Some(true)),
        }
    }

    async fn handle_install_node(&self, input: &Value) -> (String, Option<bool>) {
        let source = match input.get("source").and_then(|v| v.as_str()) {
            Some(s) => s.to_string(),
            None => return ("Missing required parameter: source".to_string(), Some(true)),
        };

        // Route based on source format (same logic as MCP server):
        // - Simple name (no `/`, no `.` prefix, valid node name) -> marketplace
        // - Path/URL -> existing install_node flow
        let is_marketplace_name = !source.contains('/')
            && !source.starts_with('.')
            && validation::validate_node_name(&source).is_ok();

        let result = if is_marketplace_name {
            self.platform.install_from_marketplace(&source).await
        } else {
            if let Err(e) = validation::validate_install_source(&source) {
                return (format!("Error: {}", e), Some(true));
            }
            self.platform.install_node(&source).await
        };

        match result {
            Ok(msg) => (msg, None),
            Err(e) => (format!("Error: {}", e), Some(true)),
        }
    }

    async fn handle_discover_capabilities(&self, input: &Value) -> (String, Option<bool>) {
        let capability = input
            .get("capability")
            .and_then(|v| v.as_str())
            .map(|s| s.to_string());

        match self.platform.get_manifests(capability.as_deref()).await {
            Ok(manifests) => {
                // Group nodes by capability type (same logic as MCP server).
                let mut grouped: std::collections::HashMap<String, Vec<Value>> =
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
                            .push(json!({
                                "name": name,
                                "description": manifest.get("description")
                                    .and_then(|d| d.as_str()).unwrap_or(""),
                                "version": manifest.get("version")
                                    .and_then(|v| v.as_str()).unwrap_or(""),
                            }));
                    } else {
                        for cap in &capabilities {
                            let cap_str = cap.as_str().unwrap_or("unknown").to_string();
                            grouped.entry(cap_str).or_default().push(json!({
                                "name": name,
                                "description": manifest.get("description")
                                    .and_then(|d| d.as_str()).unwrap_or(""),
                                "version": manifest.get("version")
                                    .and_then(|v| v.as_str()).unwrap_or(""),
                                "publishes": manifest.get("publishes")
                                    .cloned().unwrap_or(json!([])),
                                "commands": manifest.get("commands")
                                    .cloned().unwrap_or(json!([])),
                            }));
                        }
                    }
                }

                let result = json!({
                    "total_nodes": manifests.len(),
                    "capabilities": grouped,
                });
                let text =
                    serde_json::to_string_pretty(&result).unwrap_or_else(|_| "{}".to_string());
                (text, None)
            }
            Err(e) => (format!("Error: {}", e), Some(true)),
        }
    }
}

// ── Tests ───────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashSet;

    /// Expected number of tools exposed by the dispatcher.
    const TOOL_COUNT: usize = 24;

    #[test]
    fn tool_definitions_count() {
        let defs = Dispatcher::<crate::mcp::platform::DaemonPlatform>::tool_definitions();
        assert_eq!(
            defs.len(),
            TOOL_COUNT,
            "expected {} tool definitions, got {}",
            TOOL_COUNT,
            defs.len()
        );
    }

    #[test]
    fn tool_definitions_have_required_fields() {
        let defs = Dispatcher::<crate::mcp::platform::DaemonPlatform>::tool_definitions();
        for def in &defs {
            assert!(!def.name.is_empty(), "tool name must not be empty");
            assert!(
                !def.description.is_empty(),
                "tool '{}' must have a description",
                def.name
            );
            assert!(
                def.input_schema.is_object(),
                "tool '{}' input_schema must be an object",
                def.name
            );
            assert_eq!(
                def.input_schema.get("type").and_then(|v| v.as_str()),
                Some("object"),
                "tool '{}' input_schema type must be \"object\"",
                def.name
            );
        }
    }

    #[test]
    fn tool_definition_names_unique() {
        let defs = Dispatcher::<crate::mcp::platform::DaemonPlatform>::tool_definitions();
        let mut seen = HashSet::new();
        for def in &defs {
            assert!(
                seen.insert(def.name.clone()),
                "duplicate tool name: {}",
                def.name
            );
        }
    }
}
