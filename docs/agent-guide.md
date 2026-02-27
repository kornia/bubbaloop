# Bubbaloop Agent Guide

> This document teaches AI agents how to use bubbaloop via MCP (Model Context Protocol). Read this before calling any tools.

## Connection

### stdio (local agent)

```bash
bubbaloop mcp --stdio
```

Reads JSON-RPC from stdin, writes to stdout. Logs go to `~/.bubbaloop/mcp-stdio.log`.

**Authentication:** None (process boundary provides implicit trust per MCP spec).

### HTTP (remote agent)

MCP server runs on `http://127.0.0.1:8088/mcp` when daemon is active.

**Authentication:** `Authorization: Bearer <token>` (token at `~/.bubbaloop/mcp-token`)

**Rate limits:** 100 request burst, ~1 req/sec sustained replenishment.

## Quick Start Workflow

1. `list_nodes` → see all skillets with status
2. `get_node_health` → get details for a specific skillet
3. `get_node_schema` → understand its data format
4. `get_stream_info` → get Zenoh topic for live data
5. `send_command` → trigger actions on the skillet
6. `add_rule` → automate responses to sensor events

## Architecture: Dual-Plane Model

**MCP** = control plane (tool calls, JSON responses, max ~100 req/s)
**Zenoh** = data plane (sensor streams, protobuf, 1000s msg/s)

Never route streaming data through MCP. Use `get_stream_info` to get Zenoh connection params for direct subscription.

### Why Two Planes?

- **MCP** is request/response: great for "give me status" or "start this node"
- **Zenoh** is pub/sub: great for "stream all temperature readings"
- Mixing them creates bottlenecks and violates MCP transport limits

## Tool Reference

### Discovery Tools

#### `list_nodes`

**Tier:** Viewer (read-only)

List all registered nodes with their status, capabilities, and topics.

**Parameters:** None

**Returns:** JSON array of node summaries:
```json
[
  {
    "name": "rtsp-camera",
    "status": "Running",
    "health": "Healthy",
    "installed": true,
    "is_built": true,
    "node_type": "sensor"
  }
]
```

**Example workflow:**
```
Agent: list_nodes
→ Get overview of all nodes
→ Pick interesting ones for detailed inspection
```

---

#### `get_node_health`

**Tier:** Viewer

Get detailed health status of a specific node including uptime and resource usage.

**Parameters:**
- `node_name` (string, required): Name of the node (e.g., "rtsp-camera", "openmeteo")

**Returns:** JSON object with:
- `name`: Node name
- `status`: Current status ("Running", "Stopped", "Failed", "Building")
- `health`: Health state ("Healthy", "Degraded", "Unhealthy", "Unknown")
- `installed`: Whether node source is installed
- `is_built`: Whether binary is built
- `uptime_seconds`: How long the node has been running (if running)
- `last_heartbeat`: Timestamp of last heartbeat

**Example:**
```json
{
  "node_name": "rtsp-camera",
  "status": "Running",
  "health": "Healthy",
  "uptime_seconds": 3627,
  "last_heartbeat": "2026-02-26T15:23:41Z"
}
```

---

#### `discover_nodes`

**Tier:** Viewer

Discover all nodes across all machines by querying manifests on `bubbaloop/**/manifest`. Returns self-describing nodes with their capabilities.

**Parameters:** None

**Returns:** Multi-line text with one manifest per line, formatted as:
```
[bubbaloop/local/machine_id/node_name/manifest] {"name":"...","version":"...","capabilities":[...]}
```

**Use case:** Fleet-wide discovery in multi-machine deployments.

---

#### `get_node_manifest`

**Tier:** Viewer

Get the manifest (self-description) of a node including its capabilities, published topics, commands, and hardware requirements.

**Parameters:**
- `node_name` (string, required): Name of the node

**Returns:** JSON manifest:
```json
{
  "name": "rtsp-camera",
  "version": "0.1.0",
  "description": "RTSP video stream capture",
  "capabilities": ["video_capture", "motion_detection"],
  "publishes": [
    {"topic": "frame", "schema": "VideoFrame", "rate_hz": 30}
  ],
  "commands": [
    {"name": "capture_frame", "params": {"resolution": "string"}}
  ],
  "hardware": {"arch": "aarch64", "min_memory_mb": 512}
}
```

---

#### `list_commands`

**Tier:** Viewer

List available commands for a specific node with their parameters and descriptions. Use this before `send_command` to discover what actions a node supports.

**Parameters:**
- `node_name` (string, required): Name of the node

**Returns:** JSON array of command definitions:
```json
[
  {
    "name": "capture_frame",
    "description": "Capture a single frame",
    "parameters": [
      {"name": "resolution", "type": "string", "default": "1080p"}
    ]
  },
  {
    "name": "set_exposure",
    "parameters": [
      {"name": "value", "type": "number", "required": true}
    ]
  }
]
```

---

#### `get_node_schema`

**Tier:** Viewer

Get the protobuf schema of a node's data messages. Returns the schema in human-readable format (proto3 syntax) if available.

**Parameters:**
- `node_name` (string, required): Name of the node

**Returns:** Protobuf schema definition or error message if not available.

**Example output:**
```proto
syntax = "proto3";

message VideoFrame {
  uint64 timestamp_ns = 1;
  bytes image_data = 2;
  uint32 width = 3;
  uint32 height = 4;
  string encoding = 5;
}
```

---

#### `get_stream_info`

**Tier:** Viewer

Get Zenoh connection parameters for subscribing to a node's data stream. Returns topic pattern, encoding, and endpoint. Use this to set up streaming data access outside MCP.

**Parameters:**
- `node_name` (string, required): Name of the node

**Returns:** JSON with Zenoh connection info:
```json
{
  "zenoh_topic": "bubbaloop/local/nvidia_orin00/rtsp-camera/**",
  "encoding": "protobuf",
  "endpoint": "tcp/localhost:7447",
  "note": "Subscribe to this topic via Zenoh client library for real-time data. MCP is control-plane only."
}
```

**Usage:** Pass this info to a Zenoh client library (Python: `zenoh-python`, Rust: `zenoh`) to subscribe directly to the data stream.

---

### Lifecycle Tools

#### `start_node`

**Tier:** Operator

Start a stopped node via the daemon. The node must be installed and built.

**Parameters:**
- `node_name` (string, required): Name of the node

**Returns:** Success or error message.

**Example:** `start_node(node_name="rtsp-camera")`

---

#### `stop_node`

**Tier:** Operator

Stop a running node via the daemon.

**Parameters:**
- `node_name` (string, required): Name of the node

**Returns:** Success or error message.

---

#### `restart_node`

**Tier:** Operator

Restart a node (stop then start).

**Parameters:**
- `node_name` (string, required): Name of the node

**Returns:** Success or error message.

---

#### `build_node`

**Tier:** Admin

Trigger a build for a node. Builds the node's source code using its configured build command (Cargo, pixi, npm, etc.).

**Parameters:**
- `node_name` (string, required): Name of the node

**Returns:** Success or error message.

**Note:** Builds can take several minutes. Check logs with `get_node_logs` to monitor progress.

---

### Data & Command Tools

#### `send_command`

**Tier:** Operator

Send a command to a node's command queryable. The node must support the command — call `list_commands` first to see available commands.

**Parameters:**
- `node_name` (string, required): Name of the node
- `command` (string, required): Command name (must be listed in the node's manifest)
- `params` (object, optional): JSON parameters for the command (default: `{}`)

**Returns:** Command result or error message.

**Example:**
```json
{
  "node_name": "rtsp-camera",
  "command": "capture_frame",
  "params": {"resolution": "1080p"}
}
```

**Response:**
```json
{
  "status": "ok",
  "frame_path": "/tmp/frame_12345.jpg"
}
```

---

#### `get_node_config`

**Tier:** Operator

Get the current configuration of a node by querying its Zenoh config queryable.

**Parameters:**
- `node_name` (string, required): Name of the node

**Returns:** JSON configuration object (node-specific schema).

**Example response:**
```json
{
  "rtsp_url": "rtsp://192.168.1.100:554/stream",
  "framerate": 30,
  "resolution": "1920x1080"
}
```

---

#### `get_node_logs`

**Tier:** Operator

Get the latest logs from a node's systemd service.

**Parameters:**
- `node_name` (string, required): Name of the node

**Returns:** Plain text log output (last 50 lines by default).

**Use case:** Debug node failures or monitor startup progress.

---

### Automation Tools

#### `get_agent_status`

**Tier:** Viewer

Get the agent's current status including active rules, recent triggers, and human overrides.

**Parameters:** None

**Returns:** JSON status object:
```json
{
  "rules_count": 3,
  "active_rules": 2,
  "recent_triggers": 47,
  "human_overrides_active": false
}
```

---

#### `list_agent_rules`

**Tier:** Viewer

List all agent rules with their triggers, conditions, and actions.

**Parameters:** None

**Returns:** JSON array of rules:
```json
[
  {
    "name": "high-temp-alert",
    "trigger": "bubbaloop/**/telemetry/status",
    "condition": {
      "field": "cpu_temp",
      "operator": ">",
      "value": 80
    },
    "action": {
      "type": "log",
      "message": "CPU temperature critical!",
      "level": "error"
    },
    "enabled": true
  }
]
```

---

#### `add_rule`

**Tier:** Operator

Add a new rule to the agent rule engine. Rules trigger actions when sensor data matches conditions.

**Parameters:**
- `name` (string, required): Rule name (unique identifier, 1-64 chars, alphanumeric + `-_`)
- `trigger` (string, required): Zenoh key expression pattern to subscribe to (e.g., `"bubbaloop/**/telemetry/status"`)
- `condition` (object, optional): Condition to evaluate (see Condition Format below)
- `action` (object, required): Action to execute (see Action Types below)

**Returns:** Success or error message.

**Condition Format:**
```json
{
  "field": "cpu_temp",         // JSON field path (supports dot notation: "nested.field")
  "operator": ">",             // ==, !=, >, <, >=, <=, contains
  "value": 80                  // Value to compare against (any JSON type)
}
```

**Action Types:**

1. **Log action:**
```json
{
  "type": "log",
  "message": "CPU too hot!",
  "level": "warn"              // error, warn, info, debug (default: warn)
}
```

2. **Command action:**
```json
{
  "type": "command",
  "node": "fan-controller",
  "command": "set_speed",
  "params": {"speed": "high"}
}
```

3. **Publish action:**
```json
{
  "type": "publish",
  "topic": "bubbaloop/local/alerts/critical",
  "payload": {"alert": "high_temp", "value": 85}
}
```

**Example:**
```json
{
  "name": "high-temp-alert",
  "trigger": "bubbaloop/**/telemetry/status",
  "condition": {
    "field": "cpu_temp",
    "operator": ">",
    "value": 80
  },
  "action": {
    "type": "command",
    "node": "fan-controller",
    "command": "set_speed",
    "params": {"speed": "high"}
  }
}
```

---

#### `remove_rule`

**Tier:** Operator

Remove a rule from the agent rule engine by name.

**Parameters:**
- `rule_name` (string, required): Name of the rule to remove

**Returns:** Success or error message.

---

#### `update_rule`

**Tier:** Operator

Update an existing rule in the agent rule engine. Provide the full rule definition — it replaces the rule with the same name.

**Parameters:** Same as `add_rule`

**Returns:** Success or error message.

---

#### `test_rule`

**Tier:** Operator

Test a rule's condition against sample data without executing the action. Returns whether the condition matches. Useful for debugging rules before deploying them.

**Parameters:**
- `rule_name` (string, required): Name of the rule to test
- `sample_data` (object, required): Sample data to evaluate the rule condition against

**Returns:** JSON result:
```json
{
  "matched": true,
  "field_value": 85,
  "condition": "cpu_temp > 80"
}
```

**Example:**
```json
{
  "rule_name": "high-temp-alert",
  "sample_data": {
    "cpu_temp": 85,
    "gpu_temp": 70
  }
}
```

---

#### `get_events`

**Tier:** Viewer

Get recent agent events (rule triggers, actions taken). Returns the last N events from the trigger log.

**Parameters:** None

**Returns:** JSON array of event records:
```json
[
  {
    "timestamp": "2026-02-26T15:23:41Z",
    "rule": "high-temp-alert",
    "matched": true,
    "action_taken": "command(fan-controller, set_speed)"
  }
]
```

---

### System Tools

#### `get_system_status`

**Tier:** Viewer

Get overall system status including daemon health, node count, and Zenoh connection state.

**Parameters:** None

**Returns:** JSON status summary:
```json
{
  "scope": "local",
  "machine_id": "nvidia_orin00",
  "nodes_total": 12,
  "nodes_running": 10,
  "nodes_healthy": 9,
  "mcp_server": "running",
  "agent_available": true
}
```

**Use case:** Health check before performing operations.

---

#### `get_machine_info`

**Tier:** Viewer

Get machine hardware and OS information: architecture, hostname, OS version.

**Parameters:** None

**Returns:** JSON machine info:
```json
{
  "machine_id": "nvidia_orin00",
  "scope": "local",
  "arch": "aarch64",
  "os": "linux",
  "hostname": "jetson-orin"
}
```

---

#### `query_zenoh`

**Tier:** Admin

Query a Zenoh key expression (admin only). Key must start with `bubbaloop/`. Returns up to 100 results.

**Parameters:**
- `key_expr` (string, required): Full Zenoh key expression to query (e.g., `"bubbaloop/local/nvidia_orin00/openmeteo/status"`)

**Returns:** Multi-line text with one result per line:
```
[bubbaloop/local/nvidia_orin00/openmeteo/status] {"temperature":22.5,"pressure":1013}
```

**Use case:** Low-level debugging, custom queries not covered by other tools.

**Security note:** Admin-only to prevent unauthorized data access.

---

## RBAC Tiers

Bubbaloop uses three authorization tiers. Each tool requires a minimum tier to execute.

| Tier | Access Level | Example Tools |
|------|--------------|---------------|
| **Viewer** | Read-only monitoring | `list_nodes`, `get_node_health`, `get_system_status`, `list_agent_rules`, `discover_nodes` |
| **Operator** | Day-to-day operations | `start_node`, `stop_node`, `send_command`, `add_rule`, `remove_rule`, `get_node_config` |
| **Admin** | System modification | `build_node`, `query_zenoh`, `install_node`, `remove_node` |

**Default tier:** In single-user localhost mode, all requests are granted Admin tier.

**Token format:** `~/.bubbaloop/mcp-token` contains `<token>:<tier>` (e.g., `bb_abc123:operator`)

**Permission model:** Higher tiers inherit lower tier permissions (Admin can do everything, Operator can do Viewer tasks).

---

## Skillets (Nodes)

A "skillet" is a self-describing sensor/actuator capability. Each skillet:

- **Has a manifest** (JSON) describing its capabilities, published topics, commands, hardware requirements
- **Publishes data** on Zenoh topics (protobuf-encoded for efficiency)
- **Accepts commands** via its command queryable (JSON request/response)
- **Reports health** via periodic heartbeats on `bubbaloop/{scope}/{machine_id}/{node_name}/heartbeat`
- **Serves its schema** for runtime introspection on `bubbaloop/{scope}/{machine_id}/{node_name}/schema`

### Skillet Lifecycle States

1. **Installed:** Source code cloned to `~/.bubbaloop/nodes/{node_name}/`
2. **Built:** Binary compiled to `~/.bubbaloop/nodes/{node_name}/target/release/{node_name}`
3. **Running:** systemd service active, publishing to Zenoh
4. **Healthy:** Receiving heartbeats within expected interval

### Skillet Discovery Pattern

```
1. list_nodes                    # Get names and status
2. get_node_manifest             # Understand capabilities
3. list_commands                 # See available actions
4. get_node_schema               # Decode data format
5. get_stream_info               # Get Zenoh topic for streaming
```

---

## Automation Rules

The rule engine monitors Zenoh topics and triggers actions based on conditions. Rules are the primary way to create autonomous behavior.

### Rule Structure

```json
{
  "name": "unique-rule-id",
  "trigger": "bubbaloop/**/telemetry/status",
  "condition": {
    "field": "cpu_temp",
    "operator": ">",
    "value": 80
  },
  "action": {
    "type": "command",
    "node": "fan-controller",
    "command": "set_speed",
    "params": {"speed": "high"}
  },
  "enabled": true
}
```

### Trigger Patterns

Zenoh key expressions support wildcards:
- `*` matches one segment (e.g., `bubbaloop/*/nvidia_orin00/*/status`)
- `**` matches multiple segments (e.g., `bubbaloop/**/telemetry/status`)

### Condition Operators

- `==`, `!=`: Equality (works on strings, numbers, booleans)
- `>`, `<`, `>=`, `<=`: Comparison (numbers only)
- `contains`: Substring match (strings only)

### Field Path Resolution

Supports dot notation for nested fields:
```json
{
  "field": "sensors.temperature.cpu",
  "operator": ">",
  "value": 80
}
```

### Action Types Summary

1. **Log:** Write to system logs (levels: error, warn, info, debug)
2. **Command:** Send command to a node's command queryable
3. **Publish:** Publish custom JSON to a Zenoh topic

### Rule Best Practices

- **Test first:** Use `test_rule` with sample data before deploying
- **Specific triggers:** Use narrow key expressions to reduce noise
- **Idempotent actions:** Commands should be safe to repeat (rules may trigger multiple times)
- **Monitor events:** Check `get_events` to see rule activity

---

## Error Handling

### Tool Error Format

All tool errors return success responses with error text (MCP pattern):
```json
{
  "content": [
    {"type": "text", "text": "Error: Node not found: nonexistent-node"}
  ]
}
```

### Common Error Patterns

- **Validation error:** `"Validation error: Node name must be 1-64 characters, alphanumeric + -_"`
- **Node not found:** `"Error: Node not found: <name>"`
- **Permission denied:** ErrorData with INVALID_REQUEST code
- **Zenoh timeout:** `"Error: No response from node (is it running?)"`

### Validation Rules

**Node names:** 1-64 characters, `[a-zA-Z0-9_-]` only
**Rule names:** 1-64 characters, `[a-zA-Z0-9_-]` only
**Key expressions:** Must start with `bubbaloop/`
**Commands:** Must be listed in node's manifest

---

## Best Practices

### Discovery Workflow

Always discover before acting:
1. `list_nodes` → Get overview
2. `get_node_health` → Check specific node status
3. `list_commands` → See available actions
4. `send_command` → Execute action

### Streaming Data

- **Never** poll with repeated tool calls (violates MCP rate limits)
- **Always** use `get_stream_info` → subscribe to Zenoh topic directly
- MCP is for control plane, Zenoh is for data plane

### Automation

- Use rules for continuous monitoring (runs in daemon, not via MCP calls)
- Rules are more efficient than polling loops
- Test rules with `test_rule` before deployment

### System Health

- Check `get_system_status` before bulk operations
- Use `get_node_logs` to diagnose failures
- Monitor `get_events` for rule activity

### Performance

- Batch independent operations where possible (MCP rate limit: 100 burst, 1/sec sustained)
- Use Zenoh direct subscription for high-frequency data (1000s msg/sec supported)
- Keep rule conditions simple (evaluated on every message)

---

## Advanced Topics

### Multi-Machine Deployments

Use `discover_nodes` to find all nodes across the fleet. Trigger patterns like `bubbaloop/**/sensor/temperature` will match across all machines in the Zenoh network.

### Zenoh Key Structure

```
bubbaloop/{scope}/{machine_id}/{node_name}/{topic}
```

- `scope`: Deployment environment (default: "local")
- `machine_id`: Unique machine identifier (hostname-based)
- `node_name`: Skillet instance name
- `topic`: Published topic (manifest, status, schema, command, etc.)

### Protobuf Decoding

1. Get schema: `get_node_schema(node_name="...")`
2. Subscribe to Zenoh topic (from `get_stream_info`)
3. Decode bytes with protobuf library (Python: `protobuf`, Rust: `prost`)

### Rule Engine Internals

- Rules persist in `~/.bubbaloop/rules.json`
- Evaluated in-process by the daemon (low latency)
- Condition evaluation happens on every matching message
- Actions execute asynchronously (don't block message processing)

---

## Troubleshooting

### "No response from node (is it running?)"

Check node status with `get_node_health`. If stopped, use `start_node`. If unhealthy, check `get_node_logs`.

### "Permission denied: tool requires admin tier"

Your token has insufficient permissions. Check `~/.bubbaloop/mcp-token` tier setting.

### "Validation error: ..."

Parameter format is invalid. Check the Tool Reference section for correct parameter schemas.

### "Agent not available"

The daemon was started without the agent flag (`--agent`). Rules and agent status tools require agent mode.

### Rate limit exceeded

HTTP transport limits: 100 burst, 1/sec sustained. Space out requests or use Zenoh direct subscription for data.

---

## Quick Reference

### Essential Command Sequence

```
# Discovery
list_nodes → get_node_health → get_node_manifest → list_commands

# Control
start_node / stop_node / restart_node / send_command

# Automation
add_rule → test_rule → get_events

# Data
get_stream_info → (subscribe to Zenoh topic externally)

# Health
get_system_status → get_node_logs
```

### Tool Count by Tier

- **Viewer:** 13 tools (read-only)
- **Operator:** 12 tools (operations)
- **Admin:** 6 tools (system modification)
- **Total:** 24 tools

### Key Paths

- Token: `~/.bubbaloop/mcp-token`
- Rules: `~/.bubbaloop/rules.json`
- Nodes: `~/.bubbaloop/nodes/{node_name}/`
- Logs: `~/.bubbaloop/mcp-stdio.log` (stdio mode)

---

## Example: Temperature Monitoring Flow

```
1. list_nodes
   → Find "temperature-sensor" node

2. get_node_health(node_name="temperature-sensor")
   → Verify it's running and healthy

3. get_node_schema(node_name="temperature-sensor")
   → Understand data format (protobuf schema)

4. get_stream_info(node_name="temperature-sensor")
   → Get Zenoh topic: "bubbaloop/local/nvidia_orin00/temperature-sensor/reading"

5. add_rule(
     name="high-temp-alert",
     trigger="bubbaloop/**/temperature-sensor/reading",
     condition={"field": "celsius", "operator": ">", "value": 30},
     action={"type": "log", "message": "Temperature exceeds 30°C!", "level": "warn"}
   )
   → Automate monitoring

6. get_events()
   → Check if rule has triggered
```

Now the daemon continuously monitors temperature and logs alerts without further MCP calls.

---

## Summary

- **24 tools** across 3 RBAC tiers (Viewer, Operator, Admin)
- **Dual-plane architecture:** MCP for control, Zenoh for data
- **Rules engine** for autonomous behavior (no polling loops)
- **Self-describing nodes** with manifests, schemas, commands
- **Multi-machine support** via Zenoh network discovery

Read the tool reference, understand the dual-plane model, use rules for automation, and always prefer Zenoh direct subscription over MCP polling for data streams.
