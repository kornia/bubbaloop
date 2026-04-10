//! Internal MCP tool dispatch — calls PlatformOperations directly.
//!
//! Carries forward the battle-tested 37 tool definitions from agent/dispatch.rs,
//! adapted to use the new provider types.

use crate::agent::dispatch_security;
use crate::agent::provider::{ContentBlock, ToolDefinition};
use crate::mcp::platform::{
    ConfigureContextParams, NodeCommand, PlatformOperations, RegisterAlertParams,
};
use crate::validation;
use serde_json::{json, Value};
use std::sync::Arc;

/// Result of a tool handler: text output and whether it was an error.
pub(crate) struct ToolResult {
    pub text: String,
    pub is_error: bool,
}

impl ToolResult {
    pub fn success(text: String) -> Self {
        Self {
            text,
            is_error: false,
        }
    }
    pub fn error(text: String) -> Self {
        Self {
            text,
            is_error: true,
        }
    }
}

/// Dispatches MCP tool calls directly to `PlatformOperations` methods.
pub struct Dispatcher<P: PlatformOperations> {
    platform: Arc<P>,
    machine_id: String,
    agent_name: String,
    memory_backend: Option<Arc<tokio::sync::Mutex<crate::agent::memory::MemoryBackend>>>,
    job_notify: Option<Arc<tokio::sync::Notify>>,
    episodic_decay_half_life_days: u32,
    telemetry: Option<Arc<crate::daemon::telemetry::TelemetryService>>,
}

impl<P: PlatformOperations> Dispatcher<P> {
    /// Create a new dispatcher (backward-compatible, no memory).
    pub fn new(platform: Arc<P>, machine_id: String) -> Self {
        Self {
            platform,
            machine_id,
            agent_name: String::new(),
            memory_backend: None,
            job_notify: None,
            episodic_decay_half_life_days: 7,
            telemetry: None,
        }
    }

    /// Attach a telemetry service to enable the telemetry tools.
    pub fn with_telemetry(
        mut self,
        telemetry: Arc<crate::daemon::telemetry::TelemetryService>,
    ) -> Self {
        self.telemetry = Some(telemetry);
        self
    }

    /// Return the telemetry prompt summary, or `None` if telemetry is not attached.
    pub async fn telemetry_prompt_summary(&self) -> Option<String> {
        if let Some(ref telem) = self.telemetry {
            telem.prompt_summary().await
        } else {
            None
        }
    }

    /// Create a dispatcher with memory backend for agent use.
    pub fn new_with_memory(
        platform: Arc<P>,
        machine_id: String,
        agent_name: String,
        memory_backend: Arc<tokio::sync::Mutex<crate::agent::memory::MemoryBackend>>,
        job_notify: Option<Arc<tokio::sync::Notify>>,
        episodic_decay_half_life_days: u32,
    ) -> Self {
        Self {
            platform,
            machine_id,
            agent_name,
            memory_backend: Some(memory_backend),
            job_notify,
            episodic_decay_half_life_days,
            telemetry: None,
        }
    }

    /// Returns Claude-compatible tool definitions for all 37 MCP tools.
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
                input_schema: empty_object.clone(),
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
                    parameters and descriptions."
                    .to_string(),
                input_schema: node_name_schema.clone(),
            },
            ToolDefinition {
                name: "start_node".to_string(),
                description: "Start a stopped node via the daemon. Use when a node should be running but isn't — after a crash, reboot, or user request.".to_string(),
                input_schema: node_name_schema.clone(),
            },
            ToolDefinition {
                name: "stop_node".to_string(),
                description: "Stop a running node via the daemon. Use when a node needs to go offline for maintenance or is misbehaving.".to_string(),
                input_schema: node_name_schema.clone(),
            },
            ToolDefinition {
                name: "restart_node".to_string(),
                description: "Restart a node (stop then start). First fix to try when a node is unhealthy or stuck. Check health after to confirm recovery.".to_string(),
                input_schema: node_name_schema.clone(),
            },
            ToolDefinition {
                name: "get_node_logs".to_string(),
                description: "Get the latest logs from a node's systemd service. Use to diagnose why a node crashed or is unhealthy. Check before restarting to understand root cause.".to_string(),
                input_schema: node_name_schema.clone(),
            },
            ToolDefinition {
                name: "build_node".to_string(),
                description: "Trigger a build for a node. Admin only.".to_string(),
                input_schema: node_name_schema.clone(),
            },
            ToolDefinition {
                name: "remove_node".to_string(),
                description: "Remove a registered node. Admin only. Destructive — node must be reinstalled to use again. Stop first if running.".to_string(),
                input_schema: node_name_schema.clone(),
            },
            ToolDefinition {
                name: "uninstall_node".to_string(),
                description: "Uninstall a node's systemd service. Admin only.".to_string(),
                input_schema: node_name_schema.clone(),
            },
            ToolDefinition {
                name: "clean_node".to_string(),
                description: "Clean a node's build artifacts. Admin only.".to_string(),
                input_schema: node_name_schema.clone(),
            },
            ToolDefinition {
                name: "enable_autostart".to_string(),
                description: "Enable autostart for a node.".to_string(),
                input_schema: node_name_schema.clone(),
            },
            ToolDefinition {
                name: "disable_autostart".to_string(),
                description: "Disable autostart for a node.".to_string(),
                input_schema: node_name_schema.clone(),
            },
            ToolDefinition {
                name: "get_stream_info".to_string(),
                description: "Get Zenoh connection parameters for a node's data stream."
                    .to_string(),
                input_schema: node_name_schema.clone(),
            },
            ToolDefinition {
                name: "get_node_schema".to_string(),
                description: "Get the protobuf schema of a node's data messages.".to_string(),
                input_schema: node_name_schema,
            },
            // ── Custom-parameter tools ──────────────────────────────
            ToolDefinition {
                name: "send_command".to_string(),
                description: "Send a command to a node's command queryable. Use list_commands first to discover available commands.".to_string(),
                input_schema: json!({
                    "type": "object",
                    "properties": {
                        "node_name": { "type": "string", "description": "Node name" },
                        "command": { "type": "string", "description": "Command name" },
                        "params": { "description": "Optional JSON parameters" }
                    },
                    "required": ["node_name", "command"]
                }),
            },
            ToolDefinition {
                name: "query_zenoh".to_string(),
                description: "Query a Zenoh key expression (admin only).".to_string(),
                input_schema: json!({
                    "type": "object",
                    "properties": {
                        "key_expr": { "type": "string", "description": "Zenoh key expression" }
                    },
                    "required": ["key_expr"]
                }),
            },
            ToolDefinition {
                name: "install_node".to_string(),
                description: "Install a node from marketplace, local path, or GitHub. Admin only. Marketplace names are simple (e.g., 'rtsp-camera'). After installing, build and start separately."
                    .to_string(),
                input_schema: json!({
                    "type": "object",
                    "properties": {
                        "source": { "type": "string", "description": "Marketplace name, path, or user/repo" }
                    },
                    "required": ["source"]
                }),
            },
            ToolDefinition {
                name: "discover_capabilities".to_string(),
                description: "Discover available node capabilities grouped by type.".to_string(),
                input_schema: json!({
                    "type": "object",
                    "properties": {
                        "capability": { "type": "string", "description": "Filter by type: sensor, actuator, processor, gateway" }
                    },
                    "required": []
                }),
            },
            // ── Semantic memory tools ──
            ToolDefinition {
                name: "schedule_task".to_string(),
                description: "Schedule a task for the agent to execute later. Supports one-off \
                    and recurring tasks via cron expressions (e.g., '*/15 * * * *' for every 15 minutes). \
                    The agent executes the prompt autonomously when the schedule fires."
                    .to_string(),
                input_schema: json!({
                    "type": "object",
                    "properties": {
                        "prompt": {
                            "type": "string",
                            "description": "The instruction for the agent to execute"
                        },
                        "cron_schedule": {
                            "type": "string",
                            "description": "Optional cron expression for recurring tasks (5 or 6 field)"
                        },
                        "recurrence": {
                            "type": "boolean",
                            "description": "Whether this is a recurring task (default: false)"
                        }
                    },
                    "required": ["prompt"]
                }),
            },
            ToolDefinition {
                name: "list_jobs".to_string(),
                description: "List scheduled jobs. Optionally filter by status: pending, running, \
                    completed, failed, dead_letter."
                    .to_string(),
                input_schema: json!({
                    "type": "object",
                    "properties": {
                        "status": {
                            "type": "string",
                            "description": "Filter by status (optional). One of: pending, running, completed, failed, dead_letter"
                        }
                    },
                    "required": []
                }),
            },
            ToolDefinition {
                name: "delete_job".to_string(),
                description: "Delete a scheduled job by ID. Use list_jobs to find job IDs.".to_string(),
                input_schema: json!({
                    "type": "object",
                    "properties": {
                        "job_id": {
                            "type": "string",
                            "description": "The job ID to delete"
                        }
                    },
                    "required": ["job_id"]
                }),
            },
            ToolDefinition {
                name: "create_proposal".to_string(),
                description: "Create a proposal for human approval before executing a risky action. \
                    Use for destructive operations like removing nodes, changing configs, or any action \
                    that should require human sign-off."
                    .to_string(),
                input_schema: json!({
                    "type": "object",
                    "properties": {
                        "skill": {
                            "type": "string",
                            "description": "The tool or action category (e.g., 'restart_node', 'remove_node')"
                        },
                        "description": {
                            "type": "string",
                            "description": "Human-readable description of what will happen"
                        },
                        "actions": {
                            "type": "string",
                            "description": "JSON array of tool calls to execute if approved"
                        }
                    },
                    "required": ["skill", "description", "actions"]
                }),
            },
            ToolDefinition {
                name: "list_proposals".to_string(),
                description: "List proposals for human-in-the-loop approval. Optionally filter \
                    by status: pending, approved, rejected, expired."
                    .to_string(),
                input_schema: json!({
                    "type": "object",
                    "properties": {
                        "status": {
                            "type": "string",
                            "description": "Filter by status (optional). One of: pending, approved, rejected, expired"
                        }
                    },
                    "required": []
                }),
            },
            // ── World state, context providers & reactive alerts ──
            ToolDefinition {
                name: "list_world_state".to_string(),
                description: "List all live world state entries. World state is the agent's \
                    live, key/value snapshot of sensor-derived facts (e.g. motion.level, \
                    detected.label) populated by context providers. Use this to see the \
                    current environment state before reasoning about what to do."
                    .to_string(),
                input_schema: json!({
                    "type": "object",
                    "properties": {},
                    "required": []
                }),
            },
            ToolDefinition {
                name: "configure_context".to_string(),
                description: "Wire a Zenoh topic pattern to world state. The daemon starts a \
                    background task that watches the topic and writes extracted values into \
                    world state keys, making them visible to the agent's heartbeat and reactive \
                    alerts. Use this to make sensor streams (motion, detections, telemetry) \
                    influence the agent's arousal and prompt context."
                    .to_string(),
                input_schema: json!({
                    "type": "object",
                    "properties": {
                        "mission_id": {
                            "type": "string",
                            "description": "Mission this provider is attached to"
                        },
                        "topic_pattern": {
                            "type": "string",
                            "description": "Zenoh key expression pattern (e.g. 'bubbaloop/global/**/motion/score')"
                        },
                        "world_state_key_template": {
                            "type": "string",
                            "description": "Template for world state key (e.g. 'motion.level' or '{label}.location')"
                        },
                        "value_field": {
                            "type": "string",
                            "description": "JSON field path to extract as the value (e.g. 'motion_score')"
                        },
                        "filter": {
                            "type": "string",
                            "description": "Optional filter expression (e.g. 'label=dog AND confidence>0.85')"
                        },
                        "min_interval_secs": {
                            "type": "integer",
                            "description": "Minimum interval between writes for the same key (seconds)"
                        },
                        "max_age_secs": {
                            "type": "integer",
                            "description": "Maximum age before a world state entry is considered stale (seconds)"
                        },
                        "confidence_field": {
                            "type": "string",
                            "description": "Optional JSON field path to extract confidence from"
                        },
                        "token_budget": {
                            "type": "integer",
                            "description": "Approximate token budget for this provider's world state entries"
                        }
                    },
                    "required": ["mission_id", "topic_pattern", "world_state_key_template", "value_field"]
                }),
            },
            ToolDefinition {
                name: "register_alert".to_string(),
                description: "Register a reactive alert rule. When world state matches the \
                    predicate, the agent's arousal spikes, shortening the heartbeat interval \
                    and making the agent react faster. Combine with configure_context to wire \
                    a sensor topic into world state first, then write a predicate over that key."
                    .to_string(),
                input_schema: json!({
                    "type": "object",
                    "properties": {
                        "mission_id": {
                            "type": "string",
                            "description": "Mission this alert is attached to"
                        },
                        "predicate": {
                            "type": "string",
                            "description": "World state predicate expression (e.g. \"motion.level > 0.05\" or \"toddler.near_stairs = 'true'\")"
                        },
                        "description": {
                            "type": "string",
                            "description": "Human-readable description of this alert"
                        },
                        "debounce_secs": {
                            "type": "integer",
                            "description": "Minimum seconds between consecutive firings (default: 60)"
                        },
                        "arousal_boost": {
                            "type": "number",
                            "description": "Arousal boost when rule fires (default: 2.0)"
                        }
                    },
                    "required": ["mission_id", "predicate", "description"]
                }),
            },
            ToolDefinition {
                name: "unregister_alert".to_string(),
                description: "Unregister a reactive alert rule by its ID.".to_string(),
                input_schema: json!({
                    "type": "object",
                    "properties": {
                        "alert_id": {
                            "type": "string",
                            "description": "The alert ID to remove"
                        }
                    },
                    "required": ["alert_id"]
                }),
            },
            // ── Episodic memory tools ──
            ToolDefinition {
                name: "memory_search".to_string(),
                description: "Search episodic memory for past conversations, tool results, and \
                    agent observations. Uses BM25 full-text search with temporal decay. Returns \
                    the most relevant entries."
                    .to_string(),
                input_schema: json!({
                    "type": "object",
                    "properties": {
                        "query": {
                            "type": "string",
                            "description": "Search query (keywords or phrases)"
                        },
                        "limit": {
                            "type": "integer",
                            "description": "Maximum results to return (default: 10)"
                        }
                    },
                    "required": ["query"]
                }),
            },
            ToolDefinition {
                name: "memory_forget".to_string(),
                description: "Remove matching entries from episodic memory search index. \
                    Use for PII removal, correcting false memories, or user-requested deletion. \
                    Creates an audit trail. NDJSON source files are preserved."
                    .to_string(),
                input_schema: json!({
                    "type": "object",
                    "properties": {
                        "query": {
                            "type": "string",
                            "description": "Search query to match entries to forget"
                        },
                        "reason": {
                            "type": "string",
                            "description": "Reason for forgetting (logged in audit trail)"
                        }
                    },
                    "required": ["query", "reason"]
                }),
            },
            // ── System tools (filesystem + shell) ─────────────────
            ToolDefinition {
                name: "read_file".to_string(),
                description: "Read contents of a file. Returns up to 500 lines. \
                    Use for config files, logs, scripts, or any text file."
                    .to_string(),
                input_schema: json!({
                    "type": "object",
                    "properties": {
                        "path": {
                            "type": "string",
                            "description": "Absolute or relative file path"
                        }
                    },
                    "required": ["path"]
                }),
            },
            ToolDefinition {
                name: "write_file".to_string(),
                description: "Write content to a file inside ~/.bubbaloop/workspace/. \
                    Creates parent directories if needed. Writes outside the workspace are blocked."
                    .to_string(),
                input_schema: json!({
                    "type": "object",
                    "properties": {
                        "path": {
                            "type": "string",
                            "description": "Absolute or relative file path"
                        },
                        "content": {
                            "type": "string",
                            "description": "File content to write"
                        }
                    },
                    "required": ["path", "content"]
                }),
            },
            ToolDefinition {
                name: "run_command".to_string(),
                description: "Run a shell command and return its output. \
                    Captures both stdout and stderr. Use for diagnostics, \
                    system inspection, or any task requiring shell access. \
                    Times out after 30 seconds by default."
                    .to_string(),
                input_schema: json!({
                    "type": "object",
                    "properties": {
                        "command": {
                            "type": "string",
                            "description": "Shell command to execute (passed to /bin/sh -c)"
                        },
                        "timeout_secs": {
                            "type": "integer",
                            "description": "Timeout in seconds (default: 30, max: 300)"
                        }
                    },
                    "required": ["command"]
                }),
            },
            // ── Telemetry watchdog tools ───────────────────────────────
            ToolDefinition {
                name: "get_system_telemetry".to_string(),
                description: "Get current system resource telemetry: memory, CPU, disk usage, \
                    watchdog alert level, and top processes by memory consumption."
                    .to_string(),
                input_schema: empty_object.clone(),
            },
            ToolDefinition {
                name: "get_telemetry_history".to_string(),
                description: "Query historical system telemetry for trend analysis. Returns \
                    downsampled time series. Use to detect memory leaks or resource degradation."
                    .to_string(),
                input_schema: json!({
                    "type": "object",
                    "properties": {
                        "duration_minutes": {
                            "type": "integer",
                            "description": "How many minutes of history to return (default: 60)"
                        }
                    },
                    "required": []
                }),
            },
            ToolDefinition {
                name: "update_telemetry_config".to_string(),
                description: "Update telemetry watchdog thresholds at runtime. Only provided \
                    fields are changed. Guardrails prevent unsafe values."
                    .to_string(),
                input_schema: json!({
                    "type": "object",
                    "properties": {
                        "yellow_pct": { "type": "integer", "description": "Used % for Yellow level" },
                        "orange_pct": { "type": "integer", "description": "Used % for Orange level" },
                        "red_pct": { "type": "integer", "description": "Used % for Red level" },
                        "critical_pct": { "type": "integer", "description": "Used % for Critical level" },
                        "cooldown_secs": { "type": "integer", "description": "Circuit-breaker cooldown seconds" },
                        "idle_secs": { "type": "integer", "description": "Sampling interval when pressure is low" },
                        "elevated_secs": { "type": "integer", "description": "Sampling interval when pressure is elevated" },
                        "critical_secs": { "type": "integer", "description": "Sampling interval when pressure is critical" },
                        "circuit_breaker_enabled": { "type": "boolean", "description": "Enable or disable the circuit breaker" }
                    },
                    "required": []
                }),
            },
            ToolDefinition {
                name: "publish_to_topic".to_string(),
                description: "Publish a message to a Zenoh topic. Use topic \
                    bubbaloop/global/agent/{name}/inbox to address a named agent's inbox. \
                    Inbox messages surface in the recipient's next prompt turn under Recent Events."
                    .to_string(),
                input_schema: json!({
                    "type": "object",
                    "properties": {
                        "topic": {
                            "type": "string",
                            "description": "Zenoh key expression (must start with 'bubbaloop/')"
                        },
                        "message": {
                            "type": "string",
                            "description": "Message text to deliver"
                        }
                    },
                    "required": ["topic", "message"]
                }),
            },
        ]
    }

    /// Dispatch a tool call by name, returning a `ContentBlock::ToolResult`.
    pub async fn call_tool(&self, tool_use_id: &str, name: &str, input: &Value) -> ContentBlock {
        let result = match name {
            "list_nodes" => self.handle_list_nodes().await,
            "discover_nodes" => self.handle_discover_nodes().await,
            "get_system_status" => self.handle_get_system_status().await,
            "get_machine_info" => self.handle_get_machine_info(),
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
            "send_command" => self.handle_send_command(input).await,
            "query_zenoh" => self.handle_query_zenoh(input).await,
            "install_node" => self.handle_install_node(input).await,
            "discover_capabilities" => self.handle_discover_capabilities(input).await,
            "read_file" => self.handle_read_file(input).await,
            "write_file" => self.handle_write_file(input).await,
            "run_command" => self.handle_run_command(input).await,
            "memory_search" => self.handle_memory_search(input).await,
            "memory_forget" => self.handle_memory_forget(input).await,
            "schedule_task" => self.handle_schedule_task(input).await,
            "list_jobs" => self.handle_list_jobs(input).await,
            "delete_job" => self.handle_delete_job(input).await,
            "create_proposal" => self.handle_create_proposal(input).await,
            "list_proposals" => self.handle_list_proposals(input).await,
            "list_world_state" => self.handle_list_world_state().await,
            "configure_context" => self.handle_configure_context(input).await,
            "register_alert" => self.handle_register_alert(input).await,
            "unregister_alert" => self.handle_unregister_alert(input).await,
            "get_system_telemetry" => self.handle_get_system_telemetry().await,
            "get_telemetry_history" => self.handle_get_telemetry_history(input).await,
            "update_telemetry_config" => self.handle_update_telemetry_config(input).await,
            "publish_to_topic" => self.handle_publish_to_topic(input).await,
            _ => ToolResult::error(format!(
                "Unknown tool: {}. Use your available tools: node management \
                 (list_nodes, start_node, etc.), system (read_file, write_file, \
                 run_command), memory (memory_search, memory_forget), jobs/proposals \
                 (schedule_task, list_jobs, delete_job, create_proposal, list_proposals), \
                 world/context/alerts (list_world_state, configure_context, register_alert, unregister_alert), \
                 or telemetry (get_system_telemetry, get_telemetry_history, update_telemetry_config).",
                name
            )),
        };

        ContentBlock::ToolResult {
            tool_use_id: tool_use_id.to_string(),
            content: result.text,
            is_error: if result.is_error { Some(true) } else { None },
        }
    }

    /// List nodes formatted as text for the system prompt.
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

    fn extract_node_name(input: &Value) -> Result<String, ToolResult> {
        let name = input
            .get("node_name")
            .and_then(|v| v.as_str())
            .ok_or_else(|| {
                ToolResult::error("Missing required parameter: node_name".to_string())
            })?;
        validation::validate_node_name(name).map_err(ToolResult::error)?;
        Ok(name.to_string())
    }

    async fn handle_node_command(&self, input: &Value, cmd: NodeCommand) -> ToolResult {
        let node_name = match Self::extract_node_name(input) {
            Ok(n) => n,
            Err(e) => return e,
        };
        match self.platform.execute_command(&node_name, cmd).await {
            Ok(msg) => ToolResult::success(msg),
            Err(e) => ToolResult::error(format!("Error: {}", e)),
        }
    }

    async fn handle_list_nodes(&self) -> ToolResult {
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
                ToolResult::success(text)
            }
            Err(e) => ToolResult::error(format!("Error: {}", e)),
        }
    }

    async fn handle_discover_nodes(&self) -> ToolResult {
        match self.platform.query_zenoh("bubbaloop/**/manifest").await {
            Ok(result) => ToolResult::success(result),
            Err(e) => ToolResult::error(format!("Error: {}", e)),
        }
    }

    async fn handle_get_system_status(&self) -> ToolResult {
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
            "scope": "global",
            "machine_id": self.machine_id,
            "nodes_total": total,
            "nodes_running": running,
            "nodes_healthy": healthy,
            "mcp_server": "running",
        });
        let text = serde_json::to_string_pretty(&status).unwrap_or_else(|_| "{}".to_string());
        ToolResult::success(text)
    }

    fn handle_get_machine_info(&self) -> ToolResult {
        let info = json!({
            "machine_id": self.machine_id,
            "scope": "global",
            "arch": std::env::consts::ARCH,
            "os": std::env::consts::OS,
            "hostname": hostname::get()
                .ok()
                .and_then(|h| h.into_string().ok())
                .unwrap_or_else(|| "unknown".to_string()),
        });
        let text = serde_json::to_string_pretty(&info).unwrap_or_else(|_| "{}".to_string());
        ToolResult::success(text)
    }

    async fn handle_get_node_health(&self, input: &Value) -> ToolResult {
        let node_name = match Self::extract_node_name(input) {
            Ok(n) => n,
            Err(e) => return e,
        };
        match self.platform.get_node_detail(&node_name).await {
            Ok(detail) => {
                let text = serde_json::to_string_pretty(&detail).unwrap_or_default();
                ToolResult::success(text)
            }
            Err(crate::mcp::platform::PlatformError::NodeNotFound(_)) => {
                ToolResult::error(format!("Node '{}' not found", node_name))
            }
            Err(e) => ToolResult::error(format!("Error: {}", e)),
        }
    }

    async fn handle_get_node_config(&self, input: &Value) -> ToolResult {
        let node_name = match Self::extract_node_name(input) {
            Ok(n) => n,
            Err(e) => return e,
        };
        match self.platform.get_node_config(&node_name).await {
            Ok(config) => {
                let text = serde_json::to_string_pretty(&config).unwrap_or_default();
                ToolResult::success(text)
            }
            Err(e) => ToolResult::error(format!("Error: {}", e)),
        }
    }

    async fn handle_get_node_manifest(&self, input: &Value) -> ToolResult {
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
                        ToolResult::success(text)
                    }
                    None => {
                        ToolResult::error(format!("No manifest found for node '{}'", node_name))
                    }
                }
            }
            Err(e) => ToolResult::error(format!("Error: {}", e)),
        }
    }

    async fn handle_list_commands(&self, input: &Value) -> ToolResult {
        let node_name = match Self::extract_node_name(input) {
            Ok(n) => n,
            Err(e) => return e,
        };
        let key_expr = format!(
            "bubbaloop/{}/{}/{}/manifest",
            "global", self.machine_id, node_name
        );
        let manifest_text = match self.platform.query_zenoh(&key_expr).await {
            Ok(text) => text,
            Err(e) => return ToolResult::error(format!("Error: {}", e)),
        };
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
                ToolResult::success(text)
            }
            _ => ToolResult::success(format!("No commands available for node '{}'", node_name)),
        }
    }

    async fn handle_remove_node(&self, input: &Value) -> ToolResult {
        let node_name = match Self::extract_node_name(input) {
            Ok(n) => n,
            Err(e) => return e,
        };
        match self.platform.remove_node(&node_name).await {
            Ok(msg) => ToolResult::success(msg),
            Err(e) => ToolResult::error(format!("Error: {}", e)),
        }
    }

    async fn handle_get_stream_info(&self, input: &Value) -> ToolResult {
        let node_name = match Self::extract_node_name(input) {
            Ok(n) => n,
            Err(e) => return e,
        };
        let info = json!({
            "zenoh_topic": format!(
                "bubbaloop/{}/{}/{}/**",
                "global", self.machine_id, node_name
            ),
            "encoding": "protobuf",
            "endpoint": "tcp/localhost:7447",
        });
        let text = serde_json::to_string_pretty(&info).unwrap_or_else(|_| "{}".to_string());
        ToolResult::success(text)
    }

    async fn handle_get_node_schema(&self, input: &Value) -> ToolResult {
        let node_name = match Self::extract_node_name(input) {
            Ok(n) => n,
            Err(e) => return e,
        };
        let key = format!(
            "bubbaloop/{}/{}/{}/schema",
            "global", self.machine_id, node_name
        );
        match self.platform.query_zenoh(&key).await {
            Ok(result) => ToolResult::success(result),
            Err(e) => ToolResult::error(format!("Error: {}", e)),
        }
    }

    async fn handle_send_command(&self, input: &Value) -> ToolResult {
        // Phase 4: ConstraintEngine.validate_position_goal() must be called here
        // before actuator publish. The actual wiring happens in Phase 5 when the
        // daemon loop is restructured to pass mission context (with constraint set)
        // into the agent dispatch path.
        let node_name = match Self::extract_node_name(input) {
            Ok(n) => n,
            Err(e) => return e,
        };
        let command = match input.get("command").and_then(|v| v.as_str()) {
            Some(cmd) => cmd.to_string(),
            None => return ToolResult::error("Missing required parameter: command".to_string()),
        };
        let params = input.get("params").cloned().unwrap_or(json!({}));

        let key_expr = format!(
            "bubbaloop/{}/{}/{}/command",
            "global", self.machine_id, node_name
        );
        let payload = json!({ "command": command, "params": params });
        let payload_bytes = serde_json::to_vec(&payload).unwrap_or_default();

        match self
            .platform
            .send_zenoh_query(&key_expr, payload_bytes)
            .await
        {
            Ok(results) => {
                if results.is_empty() {
                    ToolResult::success("No response from node (is it running?)".to_string())
                } else {
                    ToolResult::success(results.join("\n"))
                }
            }
            Err(e) => ToolResult::error(format!("Error: {}", e)),
        }
    }

    async fn handle_query_zenoh(&self, input: &Value) -> ToolResult {
        let key_expr = match input.get("key_expr").and_then(|v| v.as_str()) {
            Some(k) => k.to_string(),
            None => return ToolResult::error("Missing required parameter: key_expr".to_string()),
        };
        if let Err(e) = validation::validate_query_key_expr(&key_expr) {
            return ToolResult::error(format!("Validation error: {}", e));
        }
        match self.platform.query_zenoh(&key_expr).await {
            Ok(result) => ToolResult::success(result),
            Err(e) => ToolResult::error(format!("Error: {}", e)),
        }
    }

    async fn handle_install_node(&self, input: &Value) -> ToolResult {
        let source = match input.get("source").and_then(|v| v.as_str()) {
            Some(s) => s.to_string(),
            None => return ToolResult::error("Missing required parameter: source".to_string()),
        };

        let is_marketplace_name = !source.contains('/')
            && !source.starts_with('.')
            && validation::validate_node_name(&source).is_ok();

        let platform_result = if is_marketplace_name {
            self.platform.install_from_marketplace(&source).await
        } else {
            if let Err(e) = validation::validate_install_source(&source) {
                return ToolResult::error(format!("Error: {}", e));
            }
            self.platform.install_node(&source).await
        };

        match platform_result {
            Ok(msg) => ToolResult::success(msg),
            Err(e) => ToolResult::error(format!("Error: {}", e)),
        }
    }

    async fn handle_discover_capabilities(&self, input: &Value) -> ToolResult {
        let capability = input
            .get("capability")
            .and_then(|v| v.as_str())
            .map(|s| s.to_string());

        match self.platform.get_manifests(capability.as_deref()).await {
            Ok(manifests) => {
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
                            .push(json!({ "name": name }));
                    } else {
                        for cap in &capabilities {
                            let cap_str = cap.as_str().unwrap_or("unknown").to_string();
                            grouped
                                .entry(cap_str)
                                .or_default()
                                .push(json!({ "name": name }));
                        }
                    }
                }

                let result = json!({
                    "total_nodes": manifests.len(),
                    "capabilities": grouped,
                });
                let text =
                    serde_json::to_string_pretty(&result).unwrap_or_else(|_| "{}".to_string());
                ToolResult::success(text)
            }
            Err(e) => ToolResult::error(format!("Error: {}", e)),
        }
    }

    async fn handle_read_file(&self, input: &Value) -> ToolResult {
        let path = match input.get("path").and_then(|v| v.as_str()) {
            Some(p) => p,
            None => return ToolResult::error("Missing required parameter: path".to_string()),
        };

        let path = dispatch_security::expand_home(path);

        if let Err(e) = dispatch_security::validate_read_path(&path) {
            return ToolResult::error(e);
        }

        match tokio::fs::read_to_string(&path).await {
            Ok(content) => {
                let lines: Vec<&str> = content.lines().collect();
                let max_lines = 500;
                if lines.len() > max_lines {
                    let truncated: String = lines[..max_lines].join("\n");
                    ToolResult::success(format!(
                        "{}\n\n[Truncated: showing {}/{} lines]",
                        truncated,
                        max_lines,
                        lines.len()
                    ))
                } else {
                    ToolResult::success(content)
                }
            }
            Err(e) => ToolResult::error(format!("Error reading file: {}", e)),
        }
    }

    async fn handle_write_file(&self, input: &Value) -> ToolResult {
        let path = match input.get("path").and_then(|v| v.as_str()) {
            Some(p) => p,
            None => return ToolResult::error("Missing required parameter: path".to_string()),
        };
        let content = match input.get("content").and_then(|v| v.as_str()) {
            Some(c) => c,
            None => return ToolResult::error("Missing required parameter: content".to_string()),
        };

        let path = dispatch_security::expand_home(path);

        if let Err(e) = dispatch_security::validate_write_path(&path) {
            return ToolResult::error(e);
        }

        // Create parent directories if needed
        if let Some(parent) = path.parent() {
            if let Err(e) = tokio::fs::create_dir_all(parent).await {
                return ToolResult::error(format!("Error creating directory: {}", e));
            }
        }

        match tokio::fs::write(&path, content).await {
            Ok(()) => ToolResult::success(format!(
                "Wrote {} bytes to {}",
                content.len(),
                path.display()
            )),
            Err(e) => ToolResult::error(format!("Error writing file: {}", e)),
        }
    }

    async fn handle_run_command(&self, input: &Value) -> ToolResult {
        let command = match input.get("command").and_then(|v| v.as_str()) {
            Some(c) => c,
            None => return ToolResult::error("Missing required parameter: command".to_string()),
        };
        let timeout_secs = input
            .get("timeout_secs")
            .and_then(|v| v.as_u64())
            .unwrap_or(30)
            .min(300);

        if let Err(e) = dispatch_security::validate_command(command) {
            return ToolResult::error(e);
        }

        log::info!("[Agent] run_command: {}", command);

        let result = tokio::time::timeout(
            std::time::Duration::from_secs(timeout_secs),
            tokio::process::Command::new("/bin/sh")
                .arg("-c")
                .arg(command)
                .output(),
        )
        .await;

        match result {
            Ok(Ok(output)) => {
                let stdout = String::from_utf8_lossy(&output.stdout);
                let stderr = String::from_utf8_lossy(&output.stderr);
                let status = output.status;

                let mut text = String::new();
                if !stdout.is_empty() {
                    text.push_str(&stdout);
                }
                if !stderr.is_empty() {
                    if !text.is_empty() {
                        text.push('\n');
                    }
                    text.push_str("[stderr] ");
                    text.push_str(&stderr);
                }
                if text.is_empty() {
                    text = format!(
                        "Command completed with exit code {}",
                        status.code().unwrap_or(-1)
                    );
                }

                // Truncate very long output
                if text.len() > 50_000 {
                    text.truncate(50_000);
                    text.push_str("\n[Output truncated at 50KB]");
                }

                if status.success() {
                    ToolResult::success(text)
                } else {
                    ToolResult::error(format!(
                        "Exit code {}: {}",
                        status.code().unwrap_or(-1),
                        text
                    ))
                }
            }
            Ok(Err(e)) => ToolResult::error(format!("Error executing command: {}", e)),
            Err(_) => {
                ToolResult::error(format!("Command timed out after {} seconds", timeout_secs))
            }
        }
    }

    // ── Memory tool handlers ─────────────────────────────────────────

    async fn handle_memory_search(&self, input: &Value) -> ToolResult {
        let backend = match &self.memory_backend {
            Some(b) => b,
            None => return ToolResult::error("Memory backend not available".to_string()),
        };
        let query = input.get("query").and_then(|v| v.as_str()).unwrap_or("");
        let limit = input.get("limit").and_then(|v| v.as_u64()).unwrap_or(10) as usize;
        let decay = self.episodic_decay_half_life_days;
        let guard = backend.lock().await;
        match guard.episodic.search_with_decay(query, limit, decay) {
            Ok(entries) if entries.is_empty() => {
                ToolResult::success("No matching entries found.".to_string())
            }
            Ok(entries) => {
                let text = entries
                    .iter()
                    .map(|e| format!("[{}] {}: {}", e.timestamp, e.role, e.content))
                    .collect::<Vec<_>>()
                    .join("\n");
                ToolResult::success(text)
            }
            Err(e) => ToolResult::error(format!("Error: {}", e)),
        }
    }

    async fn handle_memory_forget(&self, input: &Value) -> ToolResult {
        let backend = match &self.memory_backend {
            Some(b) => b,
            None => return ToolResult::error("Memory backend not available".to_string()),
        };
        let query = input.get("query").and_then(|v| v.as_str()).unwrap_or("");
        let reason = input
            .get("reason")
            .and_then(|v| v.as_str())
            .unwrap_or("agent requested");
        let guard = backend.lock().await;
        match guard.episodic.forget(query, reason) {
            Ok(0) => ToolResult::success("No matching entries found to forget.".to_string()),
            Ok(n) => ToolResult::success(format!("Forgot {} entries matching '{}'.", n, query)),
            Err(e) => ToolResult::error(format!("Error: {}", e)),
        }
    }

    async fn handle_schedule_task(&self, input: &Value) -> ToolResult {
        let backend = match &self.memory_backend {
            Some(b) => b,
            None => return ToolResult::error("Memory backend not available".to_string()),
        };
        let prompt = input.get("prompt").and_then(|v| v.as_str()).unwrap_or("");
        let cron_schedule = input.get("cron_schedule").and_then(|v| v.as_str());
        let recurrence = input
            .get("recurrence")
            .and_then(|v| v.as_bool())
            .unwrap_or(false);
        let next_run: i64 = match cron_schedule {
            Some(cron) => match crate::agent::scheduler::next_run_after(
                cron,
                crate::agent::scheduler::now_epoch_secs(),
            ) {
                Ok(ts) => ts as i64,
                Err(e) => {
                    return ToolResult::error(format!("Error: invalid cron expression: {}", e));
                }
            },
            None => crate::agent::scheduler::now_epoch_secs() as i64,
        };
        let job_id_new = uuid::Uuid::new_v4().to_string();
        let job = crate::agent::memory::semantic::Job {
            id: job_id_new.clone(),
            cron_schedule: cron_schedule.map(|s| s.to_string()),
            next_run_at: next_run,
            prompt_payload: prompt.to_string(),
            status: "pending".to_string(),
            recurrence,
            retry_count: 0,
            last_error: None,
        };
        let guard = backend.lock().await;
        match guard.semantic.create_job(&job) {
            Ok(()) => {
                if let Some(n) = &self.job_notify {
                    n.notify_one();
                }
                ToolResult::success(format!("Job '{}' scheduled", job_id_new))
            }
            Err(e) => ToolResult::error(format!("Error: {}", e)),
        }
    }

    async fn handle_list_jobs(&self, input: &Value) -> ToolResult {
        let backend = match &self.memory_backend {
            Some(b) => b,
            None => return ToolResult::error("Memory backend not available".to_string()),
        };
        let status = input.get("status").and_then(|v| v.as_str());
        let guard = backend.lock().await;
        match guard.semantic.list_jobs(status) {
            Ok(jobs) if jobs.is_empty() => ToolResult::success("No jobs found.".to_string()),
            Ok(jobs) => ToolResult::success(
                serde_json::to_string_pretty(&jobs)
                    .unwrap_or_else(|_| "Error serializing jobs".to_string()),
            ),
            Err(e) => ToolResult::error(format!("Error: {}", e)),
        }
    }

    async fn handle_delete_job(&self, input: &Value) -> ToolResult {
        let backend = match &self.memory_backend {
            Some(b) => b,
            None => return ToolResult::error("Memory backend not available".to_string()),
        };
        let target_id = input.get("job_id").and_then(|v| v.as_str()).unwrap_or("");
        let guard = backend.lock().await;
        match guard.semantic.delete_job(target_id) {
            Ok(()) => ToolResult::success(format!("Job '{}' deleted", target_id)),
            Err(e) => ToolResult::error(format!("Error: {}", e)),
        }
    }

    async fn handle_create_proposal(&self, input: &Value) -> ToolResult {
        let backend = match &self.memory_backend {
            Some(b) => b,
            None => return ToolResult::error("Memory backend not available".to_string()),
        };
        let skill = input.get("skill").and_then(|v| v.as_str()).unwrap_or("");
        let description = input
            .get("description")
            .and_then(|v| v.as_str())
            .unwrap_or("");
        let actions = input
            .get("actions")
            .and_then(|v| v.as_str())
            .unwrap_or("[]");
        let proposal = crate::agent::memory::semantic::Proposal {
            id: uuid::Uuid::new_v4().to_string(),
            timestamp: crate::agent::memory::now_rfc3339(),
            skill: skill.to_string(),
            description: description.to_string(),
            actions: actions.to_string(),
            status: "pending".to_string(),
            decided_by: None,
            decided_at: None,
        };
        let pid = proposal.id.clone();
        let guard = backend.lock().await;
        match guard.semantic.create_proposal(&proposal) {
            Ok(()) => ToolResult::success(format!("Proposal '{}' created (pending approval)", pid)),
            Err(e) => ToolResult::error(format!("Error: {}", e)),
        }
    }

    async fn handle_list_proposals(&self, input: &Value) -> ToolResult {
        let backend = match &self.memory_backend {
            Some(b) => b,
            None => return ToolResult::error("Memory backend not available".to_string()),
        };
        let status = input.get("status").and_then(|v| v.as_str());
        let guard = backend.lock().await;
        match guard.semantic.list_proposals(status) {
            Ok(proposals) if proposals.is_empty() => {
                ToolResult::success("No proposals found.".to_string())
            }
            Ok(proposals) => ToolResult::success(
                serde_json::to_string_pretty(&proposals)
                    .unwrap_or_else(|_| "Error serializing proposals".to_string()),
            ),
            Err(e) => ToolResult::error(format!("Error: {}", e)),
        }
    }

    // ── World state, context providers & reactive alerts ─────────────

    async fn handle_list_world_state(&self) -> ToolResult {
        match self.platform.list_world_state().await {
            Ok(entries) => ToolResult::success(
                serde_json::to_string_pretty(&entries)
                    .unwrap_or_else(|_| "Error serializing world state".to_string()),
            ),
            Err(e) => ToolResult::error(format!("Error: {}", e)),
        }
    }

    async fn handle_configure_context(&self, input: &Value) -> ToolResult {
        let mission_id = match input.get("mission_id").and_then(|v| v.as_str()) {
            Some(s) => s.to_string(),
            None => {
                return ToolResult::error("Missing required parameter: mission_id".to_string());
            }
        };
        let topic_pattern = match input.get("topic_pattern").and_then(|v| v.as_str()) {
            Some(s) => s.to_string(),
            None => {
                return ToolResult::error("Missing required parameter: topic_pattern".to_string());
            }
        };
        let world_state_key_template = match input
            .get("world_state_key_template")
            .and_then(|v| v.as_str())
        {
            Some(s) => s.to_string(),
            None => {
                return ToolResult::error(
                    "Missing required parameter: world_state_key_template".to_string(),
                );
            }
        };
        let value_field = match input.get("value_field").and_then(|v| v.as_str()) {
            Some(s) => s.to_string(),
            None => {
                return ToolResult::error("Missing required parameter: value_field".to_string());
            }
        };
        let params = ConfigureContextParams {
            mission_id,
            topic_pattern,
            world_state_key_template,
            value_field,
            filter: input
                .get("filter")
                .and_then(|v| v.as_str())
                .map(str::to_string),
            min_interval_secs: input
                .get("min_interval_secs")
                .and_then(|v| v.as_u64())
                .map(|n| n as u32),
            max_age_secs: input
                .get("max_age_secs")
                .and_then(|v| v.as_u64())
                .map(|n| n as u32),
            confidence_field: input
                .get("confidence_field")
                .and_then(|v| v.as_str())
                .map(str::to_string),
            token_budget: input
                .get("token_budget")
                .and_then(|v| v.as_u64())
                .map(|n| n as u32),
        };
        match self.platform.configure_context(params).await {
            Ok(msg) => ToolResult::success(msg),
            Err(e) => ToolResult::error(format!("Error: {}", e)),
        }
    }

    async fn handle_register_alert(&self, input: &Value) -> ToolResult {
        let mission_id = match input.get("mission_id").and_then(|v| v.as_str()) {
            Some(s) => s.to_string(),
            None => {
                return ToolResult::error("Missing required parameter: mission_id".to_string());
            }
        };
        let predicate = match input.get("predicate").and_then(|v| v.as_str()) {
            Some(s) => s.to_string(),
            None => {
                return ToolResult::error("Missing required parameter: predicate".to_string());
            }
        };
        let description = match input.get("description").and_then(|v| v.as_str()) {
            Some(s) => s.to_string(),
            None => {
                return ToolResult::error("Missing required parameter: description".to_string());
            }
        };
        let params = RegisterAlertParams {
            mission_id,
            predicate,
            description,
            debounce_secs: input
                .get("debounce_secs")
                .and_then(|v| v.as_u64())
                .map(|n| n as u32),
            arousal_boost: input.get("arousal_boost").and_then(|v| v.as_f64()),
        };
        match self.platform.register_alert(params).await {
            Ok(msg) => ToolResult::success(msg),
            Err(e) => ToolResult::error(format!("Error: {}", e)),
        }
    }

    async fn handle_unregister_alert(&self, input: &Value) -> ToolResult {
        let alert_id = match input.get("alert_id").and_then(|v| v.as_str()) {
            Some(s) => s.to_string(),
            None => {
                return ToolResult::error("Missing required parameter: alert_id".to_string());
            }
        };
        match self.platform.unregister_alert(alert_id).await {
            Ok(msg) => ToolResult::success(msg),
            Err(e) => ToolResult::error(format!("Error: {}", e)),
        }
    }

    // ── Telemetry tool handlers ──────────────────────────────────────

    async fn handle_get_system_telemetry(&self) -> ToolResult {
        let telemetry = match &self.telemetry {
            Some(t) => t,
            None => return ToolResult::error("Telemetry service not available".to_string()),
        };

        let snapshot = match telemetry.current_snapshot().await {
            Some(s) => s,
            None => return ToolResult::error("No telemetry snapshot available yet".to_string()),
        };
        let level = telemetry.current_level().await;

        let avail = snapshot.system.memory_available_percent();
        let mem_used_pct = 100.0 - avail;
        let mem_available_mb = snapshot.system.memory_available_bytes / (1024 * 1024);
        let disk_free_gb = snapshot.system.disk_free_mb() / 1024;

        let mut top_processes: Vec<&crate::daemon::telemetry::types::ProcessSnapshot> =
            snapshot.processes.iter().collect();
        top_processes.sort_by(|a, b| b.rss_bytes.cmp(&a.rss_bytes));
        top_processes.truncate(10);

        let processes_json: Vec<Value> = top_processes
            .iter()
            .map(|p| {
                json!({
                    "pid": p.pid,
                    "name": p.name,
                    "rss_mb": p.rss_bytes / (1024 * 1024),
                    "cpu_percent": p.cpu_percent,
                })
            })
            .collect();

        let result = json!({
            "memory_used_percent": format!("{:.1}", mem_used_pct),
            "memory_available_mb": mem_available_mb,
            "cpu_usage_percent": format!("{:.1}", snapshot.system.cpu_usage_percent),
            "disk_free_gb": disk_free_gb,
            "watchdog_level": format!("{:?}", level),
            "top_processes": processes_json,
        });
        let text = serde_json::to_string_pretty(&result).unwrap_or_else(|_| "{}".to_string());
        ToolResult::success(text)
    }

    async fn handle_get_telemetry_history(&self, input: &Value) -> ToolResult {
        let telemetry = match &self.telemetry {
            Some(t) => t,
            None => return ToolResult::error("Telemetry service not available".to_string()),
        };

        let duration_minutes = input
            .get("duration_minutes")
            .and_then(|v| v.as_u64())
            .unwrap_or(60);

        let samples = match telemetry.query_history(duration_minutes, 60) {
            Ok(s) => s,
            Err(e) => return ToolResult::error(format!("Error querying history: {}", e)),
        };

        if samples.is_empty() {
            return ToolResult::success(
                "No telemetry history available for the requested period.".to_string(),
            );
        }

        // Calculate memory trend: rate of change in used % per hour.
        let trend_rate_pct_per_hour = if samples.len() >= 2 {
            let first = &samples[0];
            let last = &samples[samples.len() - 1];
            let first_used = 100.0 - first.system.memory_available_percent();
            let last_used = 100.0 - last.system.memory_available_percent();
            let elapsed_ms = (last.system.timestamp_ms - first.system.timestamp_ms).max(1);
            let elapsed_hours = elapsed_ms as f64 / (1000.0 * 3600.0);
            (last_used - first_used) / elapsed_hours
        } else {
            0.0
        };

        let series: Vec<Value> = samples
            .iter()
            .map(|s| {
                json!({
                    "timestamp_ms": s.system.timestamp_ms,
                    "memory_used_percent": format!("{:.1}", 100.0 - s.system.memory_available_percent()),
                    "cpu_usage_percent": format!("{:.1}", s.system.cpu_usage_percent),
                    "disk_free_mb": s.system.disk_free_mb(),
                })
            })
            .collect();

        let result = json!({
            "duration_minutes": duration_minutes,
            "sample_count": samples.len(),
            "memory_trend_pct_per_hour": format!("{:.2}", trend_rate_pct_per_hour),
            "trend_description": if trend_rate_pct_per_hour > 5.0 {
                "rising fast — possible memory leak"
            } else if trend_rate_pct_per_hour > 1.0 {
                "slowly rising"
            } else if trend_rate_pct_per_hour < -1.0 {
                "falling — recovering"
            } else {
                "stable"
            },
            "samples": series,
        });
        let text = serde_json::to_string_pretty(&result).unwrap_or_else(|_| "{}".to_string());
        ToolResult::success(text)
    }

    async fn handle_update_telemetry_config(&self, input: &Value) -> ToolResult {
        let telemetry = match &self.telemetry {
            Some(t) => t,
            None => return ToolResult::error("Telemetry service not available".to_string()),
        };

        log::info!("[MCP] tool=update_telemetry_config");

        // Map flat input fields into the nested config structure expected by update_config.
        let mut updates = serde_json::Map::new();

        // Thresholds subobject
        let mut thresholds = serde_json::Map::new();
        for field in &["yellow_pct", "orange_pct", "red_pct", "critical_pct"] {
            if let Some(v) = input.get(*field) {
                thresholds.insert(field.to_string(), v.clone());
            }
        }
        if !thresholds.is_empty() {
            updates.insert("thresholds".to_string(), Value::Object(thresholds));
        }

        // Sampling subobject
        let mut sampling = serde_json::Map::new();
        for field in &["idle_secs", "elevated_secs", "critical_secs"] {
            if let Some(v) = input.get(*field) {
                sampling.insert(field.to_string(), v.clone());
            }
        }
        if !sampling.is_empty() {
            updates.insert("sampling".to_string(), Value::Object(sampling));
        }

        // Circuit-breaker subobject
        let mut cb = serde_json::Map::new();
        if let Some(v) = input.get("cooldown_secs") {
            cb.insert("cooldown_secs".to_string(), v.clone());
        }
        if let Some(v) = input.get("circuit_breaker_enabled") {
            cb.insert("enabled".to_string(), v.clone());
        }
        if !cb.is_empty() {
            updates.insert("circuit_breaker".to_string(), Value::Object(cb));
        }

        match telemetry.update_config(Value::Object(updates)).await {
            Ok(clamped) if clamped.is_empty() => {
                ToolResult::success("Telemetry config updated successfully.".to_string())
            }
            Ok(clamped) => ToolResult::success(format!(
                "Telemetry config updated. The following fields were clamped by guardrails: {}",
                clamped.join(", ")
            )),
            Err(e) => ToolResult::error(format!("Error updating telemetry config: {}", e)),
        }
    }

    async fn handle_publish_to_topic(&self, input: &Value) -> ToolResult {
        let topic = match input.get("topic").and_then(|v| v.as_str()) {
            Some(t) => t.to_string(),
            None => return ToolResult::error("Missing required parameter: topic".to_string()),
        };
        let message = match input.get("message").and_then(|v| v.as_str()) {
            Some(m) => m.to_string(),
            None => return ToolResult::error("Missing required parameter: message".to_string()),
        };
        if let Err(e) = crate::validation::validate_publish_topic(&topic) {
            return ToolResult::error(format!("Validation error: {}", e));
        }
        let envelope = json!({
            "sender": self.agent_name,
            "message": message,
        });
        log::info!("[Agent] publish_to_topic: {} -> {}", self.agent_name, topic);
        match self
            .platform
            .publish_to_topic(&topic, &envelope.to_string())
            .await
        {
            Ok(()) => ToolResult::success(format!("Published to {}", topic)),
            Err(e) => ToolResult::error(format!("Error: {}", e)),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashSet;
    use zenoh::handlers::FifoChannelHandler;
    use zenoh::pubsub::Subscriber;
    use zenoh::sample::Sample;

    const TOOL_COUNT: usize = 42;

    #[test]
    fn tool_definitions_count() {
        let defs = Dispatcher::<crate::mcp::platform::DaemonPlatform>::tool_definitions();
        assert_eq!(defs.len(), TOOL_COUNT);
    }

    #[test]
    fn tool_definitions_have_required_fields() {
        let defs = Dispatcher::<crate::mcp::platform::DaemonPlatform>::tool_definitions();
        for def in &defs {
            assert!(!def.name.is_empty());
            assert!(!def.description.is_empty());
            assert!(def.input_schema.is_object());
        }
    }

    #[test]
    fn tool_definition_names_unique() {
        let defs = Dispatcher::<crate::mcp::platform::DaemonPlatform>::tool_definitions();
        let mut seen = HashSet::new();
        for def in &defs {
            assert!(seen.insert(def.name.clone()), "duplicate: {}", def.name);
        }
    }

    #[test]
    fn tool_result_success_is_not_error() {
        let r = ToolResult::success("ok".to_string());
        assert!(!r.is_error);
        assert_eq!(r.text, "ok");
    }

    #[test]
    fn tool_result_error_is_error() {
        let r = ToolResult::error("bad".to_string());
        assert!(r.is_error);
        assert_eq!(r.text, "bad");
    }

    // ── E2E: bidirectional agent-to-agent messaging via real Zenoh pub/sub ──
    //
    // Proves the full chain in both directions without a daemon or LLM:
    //
    //   coordinator Dispatcher  →  session.put()  →  worker inbox subscriber
    //     → EpisodicLog entry: "[agent_message from coordinator] hello worker"
    //
    //   worker Dispatcher       →  session.put()  →  coordinator inbox subscriber
    //     → EpisodicLog entry: "[agent_message from worker] hello coordinator"

    /// Open a Zenoh peer session that listens on the given TCP port.
    async fn open_listen_session(port: u16) -> std::sync::Arc<zenoh::Session> {
        let mut cfg = zenoh::Config::default();
        cfg.insert_json5("mode", "\"peer\"").unwrap();
        cfg.insert_json5("listen/endpoints", &format!("[\"tcp/127.0.0.1:{}\"]", port))
            .unwrap();
        cfg.insert_json5("scouting/multicast/enabled", "false")
            .unwrap();
        std::sync::Arc::new(zenoh::open(cfg).await.unwrap())
    }

    /// Open a Zenoh peer session that connects to the given TCP port.
    async fn open_connect_session(port: u16) -> std::sync::Arc<zenoh::Session> {
        let mut cfg = zenoh::Config::default();
        cfg.insert_json5("mode", "\"peer\"").unwrap();
        cfg.insert_json5(
            "connect/endpoints",
            &format!("[\"tcp/127.0.0.1:{}\"]", port),
        )
        .unwrap();
        cfg.insert_json5("scouting/multicast/enabled", "false")
            .unwrap();
        std::sync::Arc::new(zenoh::open(cfg).await.unwrap())
    }

    /// Assert that a tool call returned success (not an error variant).
    fn assert_tool_ok(result: &crate::agent::provider::ContentBlock) {
        match result {
            crate::agent::provider::ContentBlock::ToolResult {
                is_error, content, ..
            } => assert!(
                is_error.is_none(),
                "publish_to_topic returned error: {}",
                content
            ),
            other => panic!("unexpected ContentBlock variant: {:?}", other),
        }
    }

    /// Spawn an inbox subscriber task that mirrors `runtime.rs` exactly.
    ///
    /// Returns a oneshot receiver that fires with the logged content string
    /// once the first message arrives and is written to the episodic log.
    fn spawn_inbox_handler(
        sub: Subscriber<FifoChannelHandler<Sample>>,
        backend: std::sync::Arc<tokio::sync::Mutex<crate::agent::memory::MemoryBackend>>,
        label: &'static str,
    ) -> tokio::sync::oneshot::Receiver<String> {
        let (tx, rx) = tokio::sync::oneshot::channel::<String>();
        tokio::spawn(async move {
            if let Ok(sample) = sub.recv_async().await {
                let bytes: std::borrow::Cow<[u8]> = sample.payload().to_bytes();
                let payload = String::from_utf8_lossy(&bytes).into_owned();
                let (sender, msg) =
                    if let Ok(v) = serde_json::from_str::<serde_json::Value>(&payload) {
                        let s = v["sender"].as_str().unwrap_or("unknown").to_owned();
                        let m = v["message"].as_str().unwrap_or(&payload).to_owned();
                        (s, m)
                    } else {
                        ("unknown-agent".to_owned(), payload)
                    };
                let content = format!("[agent_message from {}] {}", sender, msg);
                eprintln!("[{}] inbox received → \"{}\"", label, content);
                let entry = crate::agent::memory::episodic::EpisodicLog::make_entry(
                    "system", &content, None,
                );
                backend.lock().await.episodic.append(&entry).unwrap();
                let _ = tx.send(content);
            }
        });
        rx
    }

    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    async fn e2e_agent_messaging_pub_sub() {
        const PORT: u16 = 14_789;

        // ── Open two Zenoh peer sessions (no router needed) ──────────────────
        // Both are Arc — cloned so each agent can both subscribe and publish.
        let worker_session = open_listen_session(PORT).await;
        let coordinator_session = open_connect_session(PORT).await;

        eprintln!("\n━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━");
        eprintln!(" Bubbaloop agent messaging");
        eprintln!("━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━\n");

        // ── Real memory backends ─────────────────────────────────────────────
        let worker_tmpdir = tempfile::tempdir().unwrap();
        let coordinator_tmpdir = tempfile::tempdir().unwrap();
        let worker_memory = crate::agent::memory::Memory::open(worker_tmpdir.path()).unwrap();
        let coordinator_memory =
            crate::agent::memory::Memory::open(coordinator_tmpdir.path()).unwrap();

        // ── Both agents subscribe to their own inbox ─────────────────────────
        let worker_inbox = "bubbaloop/local/agent/worker/inbox";
        let coordinator_inbox = "bubbaloop/local/agent/coordinator/inbox";

        let worker_sub = worker_session
            .declare_subscriber(worker_inbox)
            .await
            .unwrap();
        let coordinator_sub = coordinator_session
            .declare_subscriber(coordinator_inbox)
            .await
            .unwrap();

        // Spawn inbox handlers — each fires once and signals via oneshot
        let worker_rx = spawn_inbox_handler(worker_sub, worker_memory.backend.clone(), "worker");
        let coordinator_rx = spawn_inbox_handler(
            coordinator_sub,
            coordinator_memory.backend.clone(),
            "coordinator",
        );

        // Give subscribers a moment to register before the first publish
        tokio::time::sleep(std::time::Duration::from_millis(150)).await;

        // ── Build dispatchers (each backed by its own real Zenoh session) ────
        let coordinator_dispatcher = Dispatcher::new_with_memory(
            std::sync::Arc::new(
                crate::mcp::platform::mock::MockPlatform::new()
                    .with_session(coordinator_session.clone()),
            ),
            "test-machine".to_string(),
            "coordinator".to_string(),
            coordinator_memory.backend.clone(),
            None,
            7,
        );
        let worker_dispatcher = Dispatcher::new_with_memory(
            std::sync::Arc::new(
                crate::mcp::platform::mock::MockPlatform::new()
                    .with_session(worker_session.clone()),
            ),
            "test-machine".to_string(),
            "worker".to_string(),
            worker_memory.backend.clone(),
            None,
            7,
        );

        // ── Step 1: coordinator → worker ─────────────────────────────────────
        eprintln!("[coordinator] publish_to_topic → worker");
        let r = coordinator_dispatcher
            .call_tool(
                "msg-1",
                "publish_to_topic",
                &json!({"topic": worker_inbox, "message": "hello worker"}),
            )
            .await;
        assert_tool_ok(&r);

        let worker_log = tokio::time::timeout(std::time::Duration::from_secs(5), worker_rx)
            .await
            .expect("worker inbox: no message within 5 s")
            .unwrap();
        assert!(
            worker_log.contains("[agent_message from coordinator] hello worker"),
            "worker log: {}",
            worker_log
        );

        // ── Step 2: worker → coordinator ─────────────────────────────────────
        eprintln!("[worker]      publish_to_topic → coordinator");
        let r = worker_dispatcher
            .call_tool(
                "msg-2",
                "publish_to_topic",
                &json!({"topic": coordinator_inbox, "message": "hello coordinator"}),
            )
            .await;
        assert_tool_ok(&r);

        let coordinator_log =
            tokio::time::timeout(std::time::Duration::from_secs(5), coordinator_rx)
                .await
                .expect("coordinator inbox: no message within 5 s")
                .unwrap();
        assert!(
            coordinator_log.contains("[agent_message from worker] hello coordinator"),
            "coordinator log: {}",
            coordinator_log
        );

        eprintln!("\n✓ Bidirectional messaging verified");
        eprintln!("  worker log      : \"{}\"", worker_log);
        eprintln!("  coordinator log : \"{}\"", coordinator_log);
        eprintln!("━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━\n");
    }
}
