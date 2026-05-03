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
pub(crate) struct ToolsetNameRequest {
    /// Name of the toolset (e.g., "node", "mission", "belief"). Use
    /// `toolset_list` to discover available toolsets.
    toolset_name: String,
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

#[derive(Debug, Deserialize, JsonSchema, Default)]
pub(crate) struct DataflowParams {
    /// If true, include topics that were declared but have never fired (ever_fired=false).
    /// Default false — only currently-live, actually-firing edges are shown.
    #[serde(default)]
    include_declared_but_unused: bool,
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
pub(crate) struct ListAlertsRequest {
    /// Optional mission filter — omit to list alerts across all missions.
    #[serde(default)]
    mission_id: Option<String>,
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

#[derive(Debug, Deserialize, JsonSchema)]
pub(crate) struct GetBeliefRequest {
    /// Subject of the belief (e.g. "front_door_camera").
    subject: String,
    /// Predicate / relation (e.g. "is_reliable").
    predicate: String,
}

#[derive(Debug, Deserialize, JsonSchema)]
pub(crate) struct UpdateBeliefRequest {
    /// Subject of the belief (e.g. "front_door_camera").
    subject: String,
    /// Predicate / relation (e.g. "is_reliable").
    predicate: String,
    /// Value (e.g. "true", "mostly", JSON).
    value: String,
    /// Confidence (0.0-1.0).
    confidence: f64,
    /// How this belief was formed (e.g. "observation", "user_told_me").
    #[serde(default)]
    source: Option<String>,
    /// Free-form notes.
    #[serde(default)]
    notes: Option<String>,
}

// ── Helpers ───────────────────────────────────────────────────────

/// Build the wire payload sent to a node's `/command` queryable.
///
/// **Flat envelope**: when `params` is a JSON object, its keys are spread into
/// the top-level alongside `"command"` — i.e. the wire format is
/// `{"command": "...", ...params}`, not `{"command": "...", "params": {...}}`.
/// This matches what nodes using `#[serde(tag = "command")]` (or the OBS-style
/// command vocabulary) expect to parse. Closes alignment-notes gap #11.
///
/// **Fallback**: when `params` isn't a JSON object (null, scalar, array), the
/// payload uses the nested form `{command, params}` so legacy nodes that parse
/// either shape keep working.
///
/// **Collision handling**: if the caller's `params` happens to contain its own
/// `"command"` key, OUR `command` value wins — re-inserted last so a malicious
/// caller can't redirect the command via the params bag.
pub(crate) fn build_command_payload(command: &str, params: &serde_json::Value) -> Vec<u8> {
    let payload = match params.as_object() {
        Some(params_obj) => {
            let mut flat: serde_json::Map<String, serde_json::Value> = params_obj.clone();
            flat.insert(
                "command".to_string(),
                serde_json::Value::String(command.to_string()),
            );
            serde_json::Value::Object(flat)
        }
        None => serde_json::json!({
            "command": command,
            "params": params,
        }),
    };
    serde_json::to_vec(&payload).unwrap_or_default()
}

#[cfg(test)]
mod payload_tests {
    use super::build_command_payload;
    use serde_json::json;

    /// Decode a payload back to JSON for assertions.
    fn decode(bytes: &[u8]) -> serde_json::Value {
        serde_json::from_slice(bytes).expect("payload was not valid JSON")
    }

    #[test]
    fn object_params_get_flattened_alongside_command() {
        let payload = build_command_payload(
            "start_recording",
            &json!({"output_path": "/tmp/x.mp4", "duration_secs": 30}),
        );
        let v = decode(&payload);
        // Flat shape: top-level keys are command + each param.
        assert_eq!(v["command"], "start_recording");
        assert_eq!(v["output_path"], "/tmp/x.mp4");
        assert_eq!(v["duration_secs"], 30);
        // No nested `params` field.
        assert!(v.get("params").is_none(), "expected flat shape, got nested");
    }

    #[test]
    fn empty_object_params_yields_just_command() {
        let payload = build_command_payload("get_status", &json!({}));
        let v = decode(&payload);
        assert_eq!(v["command"], "get_status");
        assert!(v.get("params").is_none());
        assert_eq!(v.as_object().map(|o| o.len()), Some(1));
    }

    #[test]
    fn null_params_falls_back_to_nested_form() {
        let payload = build_command_payload("stop_recording", &serde_json::Value::Null);
        let v = decode(&payload);
        assert_eq!(v["command"], "stop_recording");
        // Null can't be flattened — nested form preserves the explicit null.
        assert_eq!(v["params"], serde_json::Value::Null);
    }

    #[test]
    fn array_params_falls_back_to_nested_form() {
        let payload = build_command_payload("multi", &json!([1, 2, 3]));
        let v = decode(&payload);
        assert_eq!(v["command"], "multi");
        assert_eq!(v["params"], json!([1, 2, 3]));
    }

    #[test]
    fn scalar_params_falls_back_to_nested_form() {
        let payload = build_command_payload("volume_set", &json!(42));
        let v = decode(&payload);
        assert_eq!(v["command"], "volume_set");
        assert_eq!(v["params"], 42);
    }

    #[test]
    fn caller_command_in_params_cannot_override() {
        // Defense against a caller passing `params: {"command": "evil"}`.
        // Our explicit `command` parameter must win.
        let payload = build_command_payload(
            "real_command",
            &json!({"command": "evil_command", "data": 1}),
        );
        let v = decode(&payload);
        assert_eq!(
            v["command"], "real_command",
            "params.command must NOT override the real command"
        );
        assert_eq!(v["data"], 1);
    }

    #[test]
    fn nested_objects_in_params_are_preserved() {
        let payload = build_command_payload(
            "configure",
            &json!({
                "video": {"codec": "h264", "bitrate": 4_000_000},
                "audio": null,
            }),
        );
        let v = decode(&payload);
        assert_eq!(v["command"], "configure");
        assert_eq!(v["video"]["codec"], "h264");
        assert_eq!(v["video"]["bitrate"], 4_000_000);
        assert_eq!(v["audio"], serde_json::Value::Null);
    }

    /// Wire-format reference test: regression-protect the exact JSON bytes a
    /// recorder using `#[serde(tag = "command", rename_all = "snake_case")]`
    /// would parse successfully into its `Command::Start` variant.
    #[test]
    fn recorder_compatible_wire_format() {
        let payload = build_command_payload(
            "start",
            &json!({
                "topic_patterns": ["bubbaloop/global/*/cam/**"],
                "output_dir": "/tmp/clips",
                "chunk_duration_secs": 60,
            }),
        );
        let v = decode(&payload);
        // Reproduces the exact shape recorder/src/commands.rs expects.
        assert_eq!(v["command"], "start");
        assert_eq!(v["topic_patterns"][0], "bubbaloop/global/*/cam/**");
        assert_eq!(v["output_dir"], "/tmp/clips");
        assert_eq!(v["chunk_duration_secs"], 60);
    }
}

// ── Tool implementations ──────────────────────────────────────────

#[tool_router]
impl<P: PlatformOperations> BubbaLoopMcpServer<P> {
    pub fn new(
        platform: std::sync::Arc<P>,
        auth_token: Option<String>,
        machine_id: String,
    ) -> Self {
        Self {
            platform,
            auth_token,
            tool_router: Self::tool_router(),
            machine_id,
            mode_ceiling: super::rbac::Tier::Admin,
            enabled_toolsets: std::sync::Arc::new(std::sync::RwLock::new(
                super::toolsets::default_enabled(),
            )),
        }
    }

    /// Apply a tier ceiling — typically derived from `$BUBBALOOP_MCP_MODE`.
    /// Caller cannot invoke any tool requiring a tier higher than this ceiling.
    pub fn with_mode_ceiling(mut self, ceiling: super::rbac::Tier) -> Self {
        self.mode_ceiling = ceiling;
        self
    }

    /// Override the initial enabled-toolsets set (typically used by tests
    /// that want a known starting state). Production callers get the value
    /// from `$BUBBALOOP_MCP_TOOLSETS` automatically via `new()`.
    #[cfg(any(test, feature = "test-harness"))]
    pub fn with_enabled_toolsets(self, toolsets: std::collections::HashSet<String>) -> Self {
        if let Ok(mut guard) = self.enabled_toolsets.write() {
            *guard = toolsets;
        }
        self
    }

    #[tool(
        name = "node_list",
        description = "List all registered nodes with their status, capabilities, and topics. Returns node name, status (running/stopped/etc), type, and whether it's built.",
        annotations(
            title = "List nodes",
            read_only_hint = true,
            destructive_hint = false,
            idempotent_hint = true,
            open_world_hint = false
        )
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

    #[tool(
        name = "node_health",
        description = "Get detailed health status of a specific node including uptime.",
        annotations(
            title = "Node health",
            read_only_hint = true,
            destructive_hint = false,
            idempotent_hint = true,
            open_world_hint = false
        )
    )]
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
        name = "node_config",
        description = "Get the current configuration of a node by querying its Zenoh config queryable.",
        annotations(
            title = "Node configuration",
            read_only_hint = true,
            destructive_hint = false,
            idempotent_hint = true,
            open_world_hint = false
        )
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
        name = "node_manifest",
        description = "Get the full manifest for a node, including capabilities, published topics, commands, and hardware requirements.",
        annotations(
            title = "Node manifest",
            read_only_hint = true,
            destructive_hint = false,
            idempotent_hint = true,
            open_world_hint = false
        )
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
        name = "node_commands",
        description = "List available commands for a specific node with their parameters and descriptions. Use this before send_command to discover what actions a node supports.",
        annotations(
            title = "Node commands",
            read_only_hint = true,
            destructive_hint = false,
            idempotent_hint = true,
            open_world_hint = false
        )
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
            "global", self.machine_id, req.node_name
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
        name = "node_command_send",
        description = "Send a command to a node's command queryable. The node must support the command — call list_commands first to see available commands. Example: node_name='rtsp-camera', command='capture_frame', params={\"resolution\": \"1080p\"}. Returns the command result or error.",
        annotations(
            title = "Send node command",
            read_only_hint = false,
            destructive_hint = true,
            idempotent_hint = false,
            open_world_hint = true
        )
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
            "global", self.machine_id, req.node_name
        );
        let payload_bytes = build_command_payload(&req.command, &req.params);

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
        name = "node_start",
        description = "Start a stopped node via the daemon. The node must be installed and built.",
        annotations(
            title = "Start node",
            read_only_hint = false,
            destructive_hint = false,
            idempotent_hint = true,
            open_world_hint = false
        )
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

    #[tool(
        name = "node_stop",
        description = "Stop a running node via the daemon.",
        annotations(
            title = "Stop node",
            read_only_hint = false,
            destructive_hint = false,
            idempotent_hint = true,
            open_world_hint = false
        )
    )]
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

    #[tool(
        name = "node_restart",
        description = "Restart a node (stop then start).",
        annotations(
            title = "Restart node",
            read_only_hint = false,
            destructive_hint = false,
            idempotent_hint = true,
            open_world_hint = false
        )
    )]
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

    #[tool(
        name = "node_logs",
        description = "Get the latest logs from a node's systemd service.",
        annotations(
            title = "Node logs",
            read_only_hint = true,
            destructive_hint = false,
            idempotent_hint = true,
            open_world_hint = false
        )
    )]
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
        name = "zenoh_query",
        description = "Query a Zenoh key expression (admin only). Key must start with 'bubbaloop/'. Returns up to 100 results.",
        annotations(
            title = "Zenoh query",
            read_only_hint = true,
            destructive_hint = false,
            idempotent_hint = true,
            open_world_hint = true
        )
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
        name = "dataflow_graph",
        description = "Reconstruct the runtime dataflow DAG by querying every node's CBOR-encoded `manifest` queryable. Returns nodes (instance + role + machine_id + node_kind + started_at_ns) and edges (publisher_instance → subscriber_instance per topic). Edge inference uses per-topic liveness: by default only `still_live && ever_fired` topics produce edges, so the graph reflects what is *actually firing right now*. Set `include_declared_but_unused=true` to also include topics that were declared but have never received/emitted a sample. Surfaces orphan_inputs (subscribers with no producer) and unconsumed_outputs (publishers with no subscriber). Single source of truth for who-feeds-whom — no config grepping required.",
        annotations(
            title = "Dataflow graph",
            read_only_hint = true,
            destructive_hint = false,
            idempotent_hint = true,
            open_world_hint = false
        )
    )]
    async fn dataflow(
        &self,
        Parameters(params): Parameters<DataflowParams>,
    ) -> Result<CallToolResult, rmcp::ErrorData> {
        log::info!(
            "[MCP] tool=dataflow include_declared_but_unused={}",
            params.include_declared_but_unused
        );
        let payload = match self
            .platform
            .query_zenoh_raw("bubbaloop/**/manifest", std::time::Duration::from_secs(2))
            .await
        {
            Ok(p) => p,
            Err(e) => {
                return Ok(CallToolResult::success(vec![Content::text(format!(
                    "Error: {}",
                    e
                ))]))
            }
        };

        let graph = build_dataflow_graph(&payload, params.include_declared_but_unused);
        let body = serde_json::to_string_pretty(&graph)
            .unwrap_or_else(|_| "{\"error\":\"serialize\"}".to_string());
        Ok(CallToolResult::success(vec![Content::text(body)]))
    }

    #[tool(
        name = "node_discover",
        description = "Discover all nodes across all machines by querying manifests. Returns a list of all self-describing nodes with their capabilities.",
        annotations(
            title = "Discover nodes",
            read_only_hint = true,
            destructive_hint = false,
            idempotent_hint = true,
            open_world_hint = false
        )
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
        name = "capability_discover",
        description = "Discover available node capabilities. Returns nodes grouped by capability type (sensor, actuator, processor, gateway). Optionally filter by a single capability type.",
        annotations(
            title = "Discover capabilities",
            read_only_hint = true,
            destructive_hint = false,
            idempotent_hint = true,
            open_world_hint = false
        )
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
        name = "node_stream_info",
        description = "Get Zenoh connection parameters for subscribing to a node's data stream. Returns topic pattern, encoding, and endpoint. Use this to set up streaming data access outside MCP.",
        annotations(
            title = "Node stream info",
            read_only_hint = true,
            destructive_hint = false,
            idempotent_hint = true,
            open_world_hint = false
        )
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
            "zenoh_topic": format!("bubbaloop/{}/{}/{}/**", "global", self.machine_id, req.node_name),
            "encoding": "protobuf",
            "endpoint": "tcp/localhost:7447",
            "note": "Subscribe to this topic via Zenoh client library for real-time data. MCP is control-plane only."
        });
        Ok(CallToolResult::success(vec![Content::text(
            serde_json::to_string_pretty(&info).unwrap_or_default(),
        )]))
    }

    #[tool(
        name = "system_status",
        description = "Get overall system status including daemon health, node count, and Zenoh connection state.",
        annotations(
            title = "System status",
            read_only_hint = true,
            destructive_hint = false,
            idempotent_hint = true,
            open_world_hint = false
        )
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
            "scope": "global",
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
        name = "machine_info",
        description = "Get machine hardware and OS information: architecture, hostname, OS version.",
        annotations(
            title = "Machine info",
            read_only_hint = true,
            destructive_hint = false,
            idempotent_hint = true,
            open_world_hint = false
        )
    )]
    async fn get_machine_info(&self) -> Result<CallToolResult, rmcp::ErrorData> {
        log::info!("[MCP] tool=get_machine_info");
        let info = serde_json::json!({
            "machine_id": self.machine_id,
            "scope": "global",
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
        name = "node_build",
        description = "Trigger a build for a node. Builds the node's source code using its configured build command (Cargo, pixi, etc.). Admin only.",
        annotations(
            title = "Build node",
            read_only_hint = false,
            destructive_hint = true,
            idempotent_hint = false,
            open_world_hint = false
        )
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
        name = "node_install",
        description = "Install a node from the marketplace, a local path, or GitHub repository. Accepts a marketplace name (e.g., 'rtsp-camera'), a local directory path (e.g., '/path/to/my-node'), or GitHub format (e.g., 'user/repo'). Downloads precompiled binaries when available, registers the node with the daemon, and creates the systemd service. Admin only.",
        annotations(
            title = "Install node",
            read_only_hint = false,
            destructive_hint = true,
            idempotent_hint = false,
            open_world_hint = true
        )
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
        name = "node_remove",
        description = "Remove a registered node. Stops the node first if it is running, then removes it from the daemon registry. Admin only.",
        annotations(
            title = "Remove node",
            read_only_hint = false,
            destructive_hint = true,
            idempotent_hint = true,
            open_world_hint = false
        )
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
        name = "node_uninstall",
        description = "Uninstall a node's systemd service. Stops the node if running and removes the systemd service file, but keeps the node registered. Admin only.",
        annotations(
            title = "Uninstall node",
            read_only_hint = false,
            destructive_hint = true,
            idempotent_hint = true,
            open_world_hint = false
        )
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
        name = "node_clean",
        description = "Clean a node's build artifacts and cached data. Useful for forcing a rebuild. Admin only.",
        annotations(
            title = "Clean node artifacts",
            read_only_hint = false,
            destructive_hint = true,
            idempotent_hint = true,
            open_world_hint = false
        )
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
        name = "node_autostart_enable",
        description = "Enable autostart for a node. The node's systemd service will start automatically on boot.",
        annotations(
            title = "Enable autostart",
            read_only_hint = false,
            destructive_hint = false,
            idempotent_hint = true,
            open_world_hint = false
        )
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
        name = "node_autostart_disable",
        description = "Disable autostart for a node. The node's systemd service will no longer start automatically on boot.",
        annotations(
            title = "Disable autostart",
            read_only_hint = false,
            destructive_hint = false,
            idempotent_hint = true,
            open_world_hint = false
        )
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
        name = "node_schema",
        description = "Get the protobuf schema of a node's data messages. Returns the schema in human-readable format if available.",
        annotations(
            title = "Node schema",
            read_only_hint = true,
            destructive_hint = false,
            idempotent_hint = true,
            open_world_hint = false
        )
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
            "global", self.machine_id, req.node_name
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
        name = "proposal_list",
        description = "List agent proposals awaiting human approval. Optionally filter by status: 'pending', 'approved', 'rejected'.",
        annotations(
            title = "List proposals",
            read_only_hint = true,
            destructive_hint = false,
            idempotent_hint = true,
            open_world_hint = false
        )
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
        name = "proposal_approve",
        description = "Approve a pending agent proposal. The proposed actions will be executed by the agent on its next heartbeat.",
        annotations(
            title = "Approve proposal",
            read_only_hint = false,
            destructive_hint = false,
            idempotent_hint = false,
            open_world_hint = false
        )
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
        name = "proposal_reject",
        description = "Reject a pending agent proposal. The proposed actions will be discarded.",
        annotations(
            title = "Reject proposal",
            read_only_hint = false,
            destructive_hint = false,
            idempotent_hint = false,
            open_world_hint = false
        )
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
        name = "job_list",
        description = "List agent jobs with optional status filter. Returns all scheduled, running, completed, and failed jobs.",
        annotations(
            title = "List jobs",
            read_only_hint = true,
            destructive_hint = false,
            idempotent_hint = true,
            open_world_hint = false
        )
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
        name = "job_delete",
        description = "Delete a specific agent job by ID. Removes the job from the scheduler. Operator only.",
        annotations(
            title = "Delete job",
            read_only_hint = false,
            destructive_hint = true,
            idempotent_hint = true,
            open_world_hint = false
        )
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
        name = "memory_episodic_clear",
        description = "Clear episodic memory logs older than the specified number of days. Removes NDJSON files and FTS5 index entries. Admin only.",
        annotations(
            title = "Clear episodic memory",
            read_only_hint = false,
            destructive_hint = true,
            idempotent_hint = true,
            open_world_hint = false
        )
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
        name = "context_configure",
        description = "Configure a context provider: a daemon background task that subscribes to a Zenoh topic pattern and writes extracted field values into world state. Required: mission_id, topic_pattern (e.g. 'bubbaloop/**/vision/detections'), world_state_key_template (e.g. '{label}.location'), value_field (JSON path to extract). Optional: filter (e.g. 'confidence>0.8'), min_interval_secs, max_age_secs, confidence_field. Use list_world_state to see results. Admin only.",
        annotations(
            title = "Configure context provider",
            read_only_hint = false,
            destructive_hint = false,
            idempotent_hint = false,
            open_world_hint = false
        )
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

    #[tool(
        name = "mission_list",
        description = "List all missions with their status, expiry, and resources.",
        annotations(
            title = "List missions",
            read_only_hint = true,
            destructive_hint = false,
            idempotent_hint = true,
            open_world_hint = false
        )
    )]
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
        name = "mission_pause",
        description = "Pause an active mission. The agent will stop working on it until resumed.",
        annotations(
            title = "Pause mission",
            read_only_hint = false,
            destructive_hint = false,
            idempotent_hint = true,
            open_world_hint = false
        )
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

    #[tool(
        name = "mission_resume",
        description = "Resume a paused mission. The agent will continue working on it.",
        annotations(
            title = "Resume mission",
            read_only_hint = false,
            destructive_hint = false,
            idempotent_hint = true,
            open_world_hint = false
        )
    )]
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
        name = "mission_cancel",
        description = "Cancel an active or paused mission. The agent will stop working on it permanently.",
        annotations(
            title = "Cancel mission",
            read_only_hint = false,
            destructive_hint = true,
            idempotent_hint = true,
            open_world_hint = false
        )
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
        name = "alert_register",
        description = "Register a reactive alert rule. When the world state matches the predicate, the agent's arousal spikes without an LLM call. Admin only.",
        annotations(
            title = "Register alert",
            read_only_hint = false,
            destructive_hint = false,
            idempotent_hint = false,
            open_world_hint = false
        )
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

        // Validate at the MCP boundary so mock and daemon backends reject
        // identical inputs identically. `into_config` is the single source
        // of truth for default substitution — see
        // `RegisterAlertParams::into_config`. The placeholder id is fine:
        // the real one is minted by the backend; `validate()` only needs
        // it to be non-empty.
        if let Err(e) = params.clone().into_config("preview".to_string()).validate() {
            log::warn!("[MCP] register_alert rejected: {}", e);
            return Ok(CallToolResult::success(vec![Content::text(format!(
                "Error: {}",
                e
            ))]));
        }

        match self.platform.register_alert(params).await {
            Ok(msg) => Ok(CallToolResult::success(vec![Content::text(msg)])),
            Err(e) => Ok(CallToolResult::success(vec![Content::text(format!(
                "Error: {}",
                e
            ))])),
        }
    }

    #[tool(
        name = "alert_unregister",
        description = "Unregister a reactive alert rule by ID. Admin only.",
        annotations(
            title = "Unregister alert",
            read_only_hint = false,
            destructive_hint = true,
            idempotent_hint = true,
            open_world_hint = false
        )
    )]
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

    #[tool(
        name = "alert_list",
        description = "List reactive alert rules with full introspection. \
            Each entry includes predicate, debounce_secs, arousal_boost, description, \
            and `dangling_fields` — world-state keys the predicate references \
            that no registered context provider appears to produce. A non-empty \
            dangling_fields list is a red flag: the rule may never fire, or may \
            fire on stale/ghost values (see incident 2026-04-10). \
            Optional mission_id filter.",
        annotations(
            title = "List alerts",
            read_only_hint = true,
            destructive_hint = false,
            idempotent_hint = true,
            open_world_hint = false
        )
    )]
    async fn list_alerts(
        &self,
        Parameters(req): Parameters<ListAlertsRequest>,
    ) -> Result<CallToolResult, rmcp::ErrorData> {
        log::info!("[MCP] tool=list_alerts mission_id={:?}", req.mission_id);
        match self.platform.list_alerts(req.mission_id).await {
            Ok(alerts) => {
                let json = serde_json::to_string_pretty(&alerts)
                    .unwrap_or_else(|e| format!("Error serializing: {}", e));
                Ok(CallToolResult::success(vec![Content::text(json)]))
            }
            Err(e) => Ok(CallToolResult::success(vec![Content::text(format!(
                "Error: {}",
                e
            ))])),
        }
    }

    // ── Constraint tools ────────────────────────────────────────────

    #[tool(
        name = "constraint_register",
        description = "Register a safety constraint for a mission. Constraints are checked before any actuator command (fail-closed). Admin only.",
        annotations(
            title = "Register constraint",
            read_only_hint = false,
            destructive_hint = false,
            idempotent_hint = false,
            open_world_hint = false
        )
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

    #[tool(
        name = "constraint_list",
        description = "List all safety constraints for a mission. Viewer only.",
        annotations(
            title = "List constraints",
            read_only_hint = true,
            destructive_hint = false,
            idempotent_hint = true,
            open_world_hint = false
        )
    )]
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

    // ── Belief tools ──────────────────────────────────────────────────

    #[tool(
        name = "belief_get",
        description = "Get a single belief by subject and predicate. Returns the belief as JSON or 'not found'.",
        annotations(
            title = "Get belief",
            read_only_hint = true,
            destructive_hint = false,
            idempotent_hint = true,
            open_world_hint = false
        )
    )]
    async fn get_belief(
        &self,
        Parameters(req): Parameters<GetBeliefRequest>,
    ) -> Result<CallToolResult, rmcp::ErrorData> {
        log::info!(
            "[MCP] tool=get_belief subject={} predicate={}",
            req.subject,
            req.predicate
        );
        match self.platform.get_belief(req.subject, req.predicate).await {
            Ok(Some(belief)) => Ok(CallToolResult::success(vec![Content::text(
                serde_json::to_string_pretty(&belief).unwrap_or_else(|_| "{}".to_string()),
            )])),
            Ok(None) => Ok(CallToolResult::success(vec![Content::text("not found")])),
            Err(e) => Ok(CallToolResult::success(vec![Content::text(format!(
                "Error: {}",
                e
            ))])),
        }
    }

    #[tool(
        name = "belief_update",
        description = "Create or update a belief about the world. Beliefs are durable assertions the agent holds (e.g. 'the dog eats at 08:00'). Operator only.",
        annotations(
            title = "Update belief",
            read_only_hint = false,
            destructive_hint = true,
            idempotent_hint = true,
            open_world_hint = false
        )
    )]
    async fn update_belief(
        &self,
        Parameters(req): Parameters<UpdateBeliefRequest>,
    ) -> Result<CallToolResult, rmcp::ErrorData> {
        log::info!(
            "[MCP] tool=update_belief subject={} predicate={}",
            req.subject,
            req.predicate
        );

        let params = platform::UpdateBeliefParams {
            subject: req.subject,
            predicate: req.predicate,
            value: req.value,
            confidence: req.confidence,
            source: req.source,
            notes: req.notes,
        };

        match self.platform.update_belief(params).await {
            Ok(msg) => Ok(CallToolResult::success(vec![Content::text(msg)])),
            Err(e) => Ok(CallToolResult::success(vec![Content::text(format!(
                "Error: {}",
                e
            ))])),
        }
    }

    #[tool(
        name = "world_state_list",
        description = "List all world state entries. Returns the current world state snapshot as JSON.",
        annotations(
            title = "World state",
            read_only_hint = true,
            destructive_hint = false,
            idempotent_hint = true,
            open_world_hint = false
        )
    )]
    async fn list_world_state(&self) -> Result<CallToolResult, rmcp::ErrorData> {
        log::info!("[MCP] tool=list_world_state");
        match self.platform.list_world_state().await {
            Ok(entries) => Ok(CallToolResult::success(vec![Content::text(
                serde_json::to_string_pretty(&entries).unwrap_or_else(|_| "[]".to_string()),
            )])),
            Err(e) => Ok(CallToolResult::success(vec![Content::text(format!(
                "Error: {}",
                e
            ))])),
        }
    }

    // ── Toolset meta-tools (gap #12 dynamic disclosure) ──────────────
    //
    // Three always-visible tools that let an agent inspect and broaden the
    // currently-enabled toolset surface at runtime. Pattern lifted from the
    // GitHub MCP server's `X-MCP-Toolsets` header (Jan 2026 changelog).

    #[tool(
        name = "toolset_list",
        description = "List all available toolsets, the count of tools each contains, and which are currently enabled for this session. Use this first to discover what's reachable; use `toolset_enable` to activate one, then re-fetch the tool list.",
        annotations(
            title = "List toolsets",
            read_only_hint = true,
            destructive_hint = false,
            idempotent_hint = true,
            open_world_hint = false
        )
    )]
    async fn toolset_list(&self) -> Result<CallToolResult, rmcp::ErrorData> {
        log::info!("[MCP] tool=toolset_list");
        let enabled = self
            .enabled_toolsets
            .read()
            .map(|g| g.clone())
            .unwrap_or_default();
        let mut rows: Vec<serde_json::Value> = super::toolsets::toolset_definitions()
            .iter()
            .map(|(name, tools)| {
                serde_json::json!({
                    "name": name,
                    "tool_count": tools.len(),
                    "enabled": enabled.contains(*name),
                })
            })
            .collect();
        // Stable, alphabetical for predictable output.
        rows.sort_by(|a, b| {
            a["name"]
                .as_str()
                .unwrap_or("")
                .cmp(b["name"].as_str().unwrap_or(""))
        });
        let body = serde_json::json!({
            "toolsets": rows,
            "enabled_count": enabled.len(),
            "total_count": super::toolsets::toolset_definitions().len(),
            "note": "Call `toolset_enable` with a toolset name to activate it for this session. Then re-fetch tools/list.",
        });
        Ok(CallToolResult::success(vec![Content::text(
            serde_json::to_string_pretty(&body).unwrap_or_else(|_| "{}".to_string()),
        )]))
    }

    #[tool(
        name = "toolset_get_tools",
        description = "List the tool names contained in a specific toolset, without enabling it. Useful for previewing what `toolset_enable` would expose.",
        annotations(
            title = "Get tools in a toolset",
            read_only_hint = true,
            destructive_hint = false,
            idempotent_hint = true,
            open_world_hint = false
        )
    )]
    async fn toolset_get_tools(
        &self,
        Parameters(req): Parameters<ToolsetNameRequest>,
    ) -> Result<CallToolResult, rmcp::ErrorData> {
        log::info!("[MCP] tool=toolset_get_tools name={}", req.toolset_name);
        match super::toolsets::toolset_definitions().get(req.toolset_name.as_str()) {
            Some(tools) => {
                let body = serde_json::json!({
                    "toolset": req.toolset_name,
                    "tool_count": tools.len(),
                    "tools": tools,
                });
                Ok(CallToolResult::success(vec![Content::text(
                    serde_json::to_string_pretty(&body).unwrap_or_else(|_| "{}".to_string()),
                )]))
            }
            None => {
                let known: Vec<&&str> = super::toolsets::toolset_definitions().keys().collect();
                Ok(CallToolResult::success(vec![Content::text(format!(
                    "Error: unknown toolset '{}'. Known: {:?}",
                    req.toolset_name, known
                ))]))
            }
        }
    }

    #[tool(
        name = "toolset_enable",
        description = "Enable a toolset for this session — its tools become visible on the next `tools/list` call. Idempotent: enabling an already-enabled toolset is a no-op.",
        annotations(
            title = "Enable a toolset",
            // Read-only from the caller's perspective: it doesn't mutate the
            // bubbaloop state, only the per-session visibility filter.
            read_only_hint = true,
            destructive_hint = false,
            idempotent_hint = true,
            open_world_hint = false
        )
    )]
    async fn toolset_enable(
        &self,
        Parameters(req): Parameters<ToolsetNameRequest>,
    ) -> Result<CallToolResult, rmcp::ErrorData> {
        log::info!("[MCP] tool=toolset_enable name={}", req.toolset_name);
        if !super::toolsets::toolset_definitions().contains_key(req.toolset_name.as_str()) {
            let known: Vec<&&str> = super::toolsets::toolset_definitions().keys().collect();
            return Ok(CallToolResult::success(vec![Content::text(format!(
                "Error: unknown toolset '{}'. Known: {:?}",
                req.toolset_name, known
            ))]));
        }
        let was_already_enabled = match self.enabled_toolsets.write() {
            Ok(mut guard) => !guard.insert(req.toolset_name.clone()),
            Err(_) => {
                return Ok(CallToolResult::success(vec![Content::text(
                    "Error: enabled_toolsets state lock poisoned",
                )]))
            }
        };
        // TODO(gap #12 follow-up): emit `notifications/tools/list_changed`
        // here so MCP clients re-fetch the tool list automatically. rmcp
        // 0.15 supports this via the connection context but the API surface
        // is still being finalised; for v1, callers must re-call tools/list
        // manually after enabling. Documented in alignment-notes.
        let body = serde_json::json!({
            "toolset": req.toolset_name,
            "was_already_enabled": was_already_enabled,
            "tool_count": super::toolsets::toolset_definitions()
                .get(req.toolset_name.as_str())
                .map(|v| v.len())
                .unwrap_or(0),
            "note": "Re-call `tools/list` to see the newly-visible tools.",
        });
        Ok(CallToolResult::success(vec![Content::text(
            serde_json::to_string_pretty(&body).unwrap_or_else(|_| "{}".to_string()),
        )]))
    }
}

// ── Dataflow graph reconstruction ──────────────────────────────────
//
// The `dataflow` MCP tool fetches every node's CBOR-encoded
// `manifest` queryable, decodes it, and assembles a single
// graph: `nodes` (one per live instance) plus `edges` (one per
// publisher→subscriber match on absolute topic suffix). Edge
// inference uses per-topic liveness — by default only `still_live &&
// ever_fired` topics participate, so the agent sees the live DAG
// rather than a config-derived snapshot.

#[derive(Debug, Clone, serde::Deserialize, serde::Serialize, Default)]
struct IoEntry {
    topic: String,
    #[serde(default)]
    ever_fired: bool,
    #[serde(default = "default_true")]
    still_live: bool,
    #[serde(default)]
    declared_at_ns: u64,
}

fn default_true() -> bool {
    true
}

#[derive(Debug, Clone, serde::Deserialize)]
struct WireManifest {
    instance_name: String,
    machine_id: String,
    #[serde(default = "default_role")]
    role: String,
    #[serde(default)]
    inputs: Vec<IoEntry>,
    #[serde(default)]
    outputs: Vec<IoEntry>,
    #[serde(default)]
    schema_version: u32,
    #[serde(default)]
    started_at_ns: u64,
    #[serde(default = "default_kind")]
    node_kind: String,
}

fn default_role() -> String {
    "unknown".to_string()
}

fn default_kind() -> String {
    "unknown".to_string()
}

fn entry_active(e: &IoEntry, include_declared_but_unused: bool) -> bool {
    if include_declared_but_unused {
        e.still_live
    } else {
        e.still_live && e.ever_fired
    }
}

#[derive(Debug, serde::Serialize)]
struct DataflowNode {
    instance: String,
    role: String,
    machine_id: String,
    node_kind: String,
    started_at_ns: u64,
    schema_version: u32,
}

#[derive(Debug, serde::Serialize)]
struct DataflowEdge {
    from_instance: String,
    to_instance: String,
    topic: String,
}

#[derive(Debug, serde::Serialize)]
struct DataflowDangling {
    instance: String,
    topic: String,
}

#[derive(Debug, serde::Serialize)]
struct DataflowGraph {
    nodes: Vec<DataflowNode>,
    edges: Vec<DataflowEdge>,
    orphan_inputs: Vec<DataflowDangling>,
    unconsumed_outputs: Vec<DataflowDangling>,
}

/// Build a dataflow graph from raw `(key, cbor_payload)` pairs returned
/// by querying `bubbaloop/**/manifest`. Replies that fail to decode are
/// dropped silently — the agent gets a partial graph rather than a hard
/// error, matching the behavior of `discover_nodes`. Edge inference
/// uses `entry_active` so by default only live, actually-firing topics
/// show up.
fn build_dataflow_graph(
    replies: &[(String, Vec<u8>)],
    include_declared_but_unused: bool,
) -> DataflowGraph {
    let mut manifests: Vec<WireManifest> = Vec::new();
    for (key, payload) in replies {
        match ciborium::from_reader::<WireManifest, _>(&payload[..]) {
            Ok(m) => manifests.push(m),
            Err(e) => {
                log::debug!(
                    "[dataflow] dropping undecodable manifest from {}: {}",
                    key,
                    e
                );
            }
        }
    }
    // Dedup by (machine_id, instance_name) — the same node can answer
    // through multiple peers; keep the first reply per identity.
    manifests
        .sort_by(|a, b| (&a.machine_id, &a.instance_name).cmp(&(&b.machine_id, &b.instance_name)));
    manifests.dedup_by(|a, b| a.machine_id == b.machine_id && a.instance_name == b.instance_name);

    let nodes: Vec<DataflowNode> = manifests
        .iter()
        .map(|m| DataflowNode {
            instance: m.instance_name.clone(),
            role: m.role.clone(),
            machine_id: m.machine_id.clone(),
            node_kind: m.node_kind.clone(),
            started_at_ns: m.started_at_ns,
            schema_version: m.schema_version,
        })
        .collect();

    let mut edges: Vec<DataflowEdge> = Vec::new();
    let mut produced: std::collections::HashSet<String> = Default::default();
    for producer in &manifests {
        for out in producer
            .outputs
            .iter()
            .filter(|e| entry_active(e, include_declared_but_unused))
        {
            produced.insert(out.topic.clone());
            for consumer in &manifests {
                if consumer
                    .inputs
                    .iter()
                    .any(|i| entry_active(i, include_declared_but_unused) && i.topic == out.topic)
                {
                    edges.push(DataflowEdge {
                        from_instance: producer.instance_name.clone(),
                        to_instance: consumer.instance_name.clone(),
                        topic: out.topic.clone(),
                    });
                }
            }
        }
    }

    let mut orphan_inputs = Vec::new();
    for m in &manifests {
        for inp in m
            .inputs
            .iter()
            .filter(|e| entry_active(e, include_declared_but_unused))
        {
            if !produced.contains(&inp.topic) {
                orphan_inputs.push(DataflowDangling {
                    instance: m.instance_name.clone(),
                    topic: inp.topic.clone(),
                });
            }
        }
    }

    let mut unconsumed_outputs = Vec::new();
    for m in &manifests {
        for out in m
            .outputs
            .iter()
            .filter(|e| entry_active(e, include_declared_but_unused))
        {
            let any_consumer = manifests.iter().any(|c| {
                c.inputs
                    .iter()
                    .any(|i| entry_active(i, include_declared_but_unused) && i.topic == out.topic)
            });
            if !any_consumer {
                unconsumed_outputs.push(DataflowDangling {
                    instance: m.instance_name.clone(),
                    topic: out.topic.clone(),
                });
            }
        }
    }

    DataflowGraph {
        nodes,
        edges,
        orphan_inputs,
        unconsumed_outputs,
    }
}

#[cfg(test)]
mod dataflow_tests {
    use super::*;

    fn cbor(m: &serde_json::Value) -> Vec<u8> {
        let mut buf = Vec::new();
        ciborium::into_writer(m, &mut buf).unwrap();
        buf
    }

    /// Helper: build an active (still_live && ever_fired) IoEntry JSON value.
    fn io(topic: &str) -> serde_json::Value {
        serde_json::json!({
            "topic": topic,
            "ever_fired": true,
            "still_live": true,
            "declared_at_ns": 1u64,
        })
    }

    /// Helper: declared-but-never-fired IoEntry.
    fn io_idle(topic: &str) -> serde_json::Value {
        serde_json::json!({
            "topic": topic,
            "ever_fired": false,
            "still_live": true,
            "declared_at_ns": 1u64,
        })
    }

    /// Helper: undeclared / torn-down IoEntry.
    fn io_dead(topic: &str) -> serde_json::Value {
        serde_json::json!({
            "topic": topic,
            "ever_fired": true,
            "still_live": false,
            "declared_at_ns": 1u64,
        })
    }

    #[test]
    fn builds_simple_pipeline_graph() {
        let cam = cbor(&serde_json::json!({
            "instance_name": "cam",
            "machine_id": "m1",
            "role": "source",
            "inputs": [],
            "outputs": [io("cam/raw")],
            "schema_version": 2,
            "started_at_ns": 1u64,
            "node_kind": "rust",
        }));
        let det = cbor(&serde_json::json!({
            "instance_name": "det",
            "machine_id": "m1",
            "role": "processor",
            "inputs": [io("cam/raw")],
            "outputs": [io("det/boxes")],
            "schema_version": 2,
            "started_at_ns": 2u64,
            "node_kind": "python",
        }));
        let g = build_dataflow_graph(&[("k1".into(), cam), ("k2".into(), det)], false);
        assert_eq!(g.nodes.len(), 2);
        assert_eq!(g.edges.len(), 1);
        assert_eq!(g.edges[0].from_instance, "cam");
        assert_eq!(g.edges[0].to_instance, "det");
        assert_eq!(g.edges[0].topic, "cam/raw");
        assert_eq!(g.unconsumed_outputs.len(), 1);
        assert_eq!(g.unconsumed_outputs[0].topic, "det/boxes");
        assert!(g.orphan_inputs.is_empty());
    }

    #[test]
    fn flags_orphan_input() {
        let det = cbor(&serde_json::json!({
            "instance_name": "det",
            "machine_id": "m1",
            "role": "processor",
            "inputs": [io("ghost/raw")],
            "outputs": [],
            "schema_version": 2,
            "started_at_ns": 1u64,
            "node_kind": "python",
        }));
        let g = build_dataflow_graph(&[("k".into(), det)], false);
        assert_eq!(g.orphan_inputs.len(), 1);
        assert_eq!(g.orphan_inputs[0].topic, "ghost/raw");
    }

    #[test]
    fn dedupes_repeat_replies() {
        let cam = cbor(&serde_json::json!({
            "instance_name": "cam",
            "machine_id": "m1",
            "role": "source",
            "outputs": [io("cam/raw")],
            "schema_version": 2,
        }));
        let g = build_dataflow_graph(&[("k1".into(), cam.clone()), ("k2".into(), cam)], false);
        assert_eq!(g.nodes.len(), 1);
    }

    #[test]
    fn drops_undecodable_payloads() {
        let g = build_dataflow_graph(&[("k".into(), vec![0xff, 0xff, 0xff])], false);
        assert!(g.nodes.is_empty());
    }

    #[test]
    fn undeclared_entries_produce_no_edges_by_default() {
        // cam tore down cam/raw; det still subscribes. No edge should appear.
        let cam = cbor(&serde_json::json!({
            "instance_name": "cam",
            "machine_id": "m1",
            "role": "source",
            "outputs": [io_dead("cam/raw")],
            "schema_version": 2,
        }));
        let det = cbor(&serde_json::json!({
            "instance_name": "det",
            "machine_id": "m1",
            "role": "processor",
            "inputs": [io("cam/raw")],
            "schema_version": 2,
        }));
        let g = build_dataflow_graph(&[("k1".into(), cam), ("k2".into(), det)], false);
        assert!(g.edges.is_empty());
        // det's input is orphaned since cam's output is no longer live.
        assert_eq!(g.orphan_inputs.len(), 1);
        assert_eq!(g.orphan_inputs[0].topic, "cam/raw");
    }

    #[test]
    fn declared_but_never_fired_hidden_by_default_but_visible_when_opted_in() {
        // cam declared cam/raw but never called put(). det declared a
        // subscriber but never received anything.
        let cam = cbor(&serde_json::json!({
            "instance_name": "cam",
            "machine_id": "m1",
            "role": "source",
            "outputs": [io_idle("cam/raw")],
            "schema_version": 2,
        }));
        let det = cbor(&serde_json::json!({
            "instance_name": "det",
            "machine_id": "m1",
            "role": "processor",
            "inputs": [io_idle("cam/raw")],
            "schema_version": 2,
        }));
        // Default: no edges (nothing ever fired).
        let g = build_dataflow_graph(
            &[("k1".into(), cam.clone()), ("k2".into(), det.clone())],
            false,
        );
        assert!(g.edges.is_empty());
        // Opt-in: edge is surfaced so operators can spot wired-but-idle pipelines.
        let g2 = build_dataflow_graph(&[("k1".into(), cam), ("k2".into(), det)], true);
        assert_eq!(g2.edges.len(), 1);
        assert_eq!(g2.edges[0].topic, "cam/raw");
    }
}
