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

**Agent model authentication** resolves in order: API key (`ANTHROPIC_API_KEY` env var) → OAuth bearer token (from `bubbaloop login`). API key takes precedence when both are configured.

## Creating and Managing Agents

Bubbaloop runs a multi-agent runtime inside the daemon. Each agent is an independent LLM reasoning loop with its own identity (Soul), memory, and capabilities. The CLI is a thin Zenoh pub/sub client — all LLM processing happens daemon-side.

### How It Works

```
┌──────────────┐     Zenoh pub/sub      ┌─────────────────────────────────────┐
│  CLI client   │◄─────────────────────►│          Daemon (bubbaloop up)       │
│               │   inbox → shared      │                                     │
│ agent chat    │   outbox ← per-agent  │  ┌───────────┐  ┌───────────┐      │
│ agent list    │                       │  │ jean-clawd │  │cam-expert │ ...  │
└──────────────┘                        │  │ Soul+Mem   │  │ Soul+Mem  │      │
                                        │  └───────────┘  └───────────┘      │
                                        │       ▲ shared DaemonPlatform       │
                                        │       │ (MCP tools, node mgr)       │
                                        └───────────────────────────────────┘
```

1. **CLI** publishes a JSON message to the shared inbox Zenoh topic
2. **Runtime** (inside daemon) routes the message to the right agent
3. **Agent** processes the turn (LLM calls, tool use) and streams events to its outbox topic
4. **CLI** subscribes to all outbox topics, filters by correlation ID, and renders the response

### Step 1: Configure Agents

Create `~/.bubbaloop/agents.toml`:

```toml
# The default agent — receives all unaddressed messages
[agents.jean-clawd]
enabled = true
default = true

# A specialist agent — target with `bubbaloop agent chat -a camera-expert`
[agents.camera-expert]
enabled = true
capabilities = ["camera", "rtsp", "video"]
```

**Fields:**

| Field | Type | Default | Description |
|-------|------|---------|-------------|
| `enabled` | bool | `true` | Whether the agent starts with the daemon |
| `default` | bool | `false` | Routes unaddressed messages here (exactly one should be `true`) |
| `capabilities` | string[] | `[]` | Keyword tags (future: capability-based routing) |
| `provider` | string | `"claude"` | LLM provider: `"claude"` or `"ollama"` |
| `model` | string | — | Model override (e.g., `"claude-haiku-4-5-20251001"`). Overrides `soul/capabilities.toml` model_name when set. |

If no `agents.toml` exists, the runtime creates a single default agent named `jean-clawd`.

**Interactive setup wizard:**

```bash
bubbaloop agent setup              # Configure existing agent (interactive selection)
bubbaloop agent setup -a my-agent  # Create or configure a specific agent
```

The wizard lets you choose provider (Claude or Ollama), pick a model, and — for new agents — write an initial `identity.md`. No daemon required.

### Step 2: Customize the Agent's Soul

Each agent has a per-agent directory at `~/.bubbaloop/agents/{agent_id}/`. Create the soul files:

**`~/.bubbaloop/agents/camera-expert/soul/identity.md`** — system prompt:

```markdown
You are CamBot, an AI agent specialized in video surveillance and RTSP cameras
through the Bubbaloop skill runtime.

Your focus: managing RTSP camera feeds, diagnosing video issues, configuring streams.

When given a task, DO it — use your tools, get results, report back.
Be concise. Report what you did and the result.
```

**`~/.bubbaloop/agents/camera-expert/soul/capabilities.toml`** — model and tuning:

```toml
model_name = "claude-sonnet-4-20250514"
max_turns = 15
allow_internet = true

# Heartbeat tuning (adaptive interval in seconds)
heartbeat_base_interval = 60
heartbeat_min_interval = 5
heartbeat_decay_factor = 0.7

# "auto" = execute immediately, "propose" = save for human approval
default_approval_mode = "auto"

# Retry / circuit breaker
max_retries = 3

# Memory retention: delete episodic logs older than N days (0 = keep forever)
episodic_log_retention_days = 30

# Context compaction: flush working state to episodic memory when input tokens
# exceed this threshold, enabling recovery after LLM context truncation
compaction_flush_threshold_tokens = 4000

# Temporal decay: half-life (days) for BM25 search relevance scoring.
# Older episodic entries are demoted. 0 = no decay.
episodic_decay_half_life_days = 30
```

If soul files don't exist, the agent falls back to the global soul at `~/.bubbaloop/soul/`, then to compiled-in defaults.

Soul files are **hot-reloaded** — edit them while the daemon is running and changes take effect on the next turn.

### Step 3: Start the Daemon

```bash
bubbaloop up
```

The daemon starts the agent runtime, which:
1. Reads `~/.bubbaloop/agents.toml` (or uses default config)
2. For each enabled agent: creates `~/.bubbaloop/agents/{id}/` directory, loads Soul, initializes Claude provider, opens per-agent Memory (episodic NDJSON + semantic SQLite)
3. Subscribes to the shared Zenoh inbox
4. Registers per-agent manifest queryables
5. Spawns per-agent tokio tasks (event loops)

Look for these log lines to confirm:
```
[Runtime] Agent 'jean-clawd' ready (default=true)
[Runtime] Agent 'camera-expert' ready (default=false)
[Runtime] Agent runtime started: 2 agent(s), inbox=bubbaloop/local/{machine}/agent/inbox
```

### Step 4: Interact via CLI

```bash
# Single message — plain stdout, good for scripting/piping
bubbaloop agent chat "what is the system status?"

# Interactive TUI REPL — two-panel layout, scrollable history
bubbaloop agent chat

# TUI REPL with tool debug info (see every tool call + result)
bubbaloop agent chat -v

# Target a specific agent
bubbaloop agent chat -a camera-expert "describe the video feed"

# List all running agents
bubbaloop agent list
```

**TUI layout:** The top panel shows scrollable conversation history. The bottom panel (always visible, green border) is the input line. Use ↑/↓ or PageUp/PageDown to scroll history while the agent is responding. Press Ctrl-C or type `q` on an empty input line to exit.

**TUI colours:** cyan = agent name, green = agent text, yellow = tool calls, gray = tool results, red = errors, bold white = your messages.

`agent list` queries all manifest Zenoh queryables and prints:
```
ID                   NAME                      DEFAULT    MODEL                          CAPABILITIES
-----------------------------------------------------------------------------------------------
jean-clawd           Bubbaloop                 yes        claude-sonnet-4-20250514
camera-expert        CamBot                               claude-sonnet-4-20250514       camera, rtsp, video
```

### Per-Agent Directory Layout

```
~/.bubbaloop/agents/{agent_id}/
├── soul/
│   ├── identity.md          # System prompt (markdown)
│   └── capabilities.toml    # Model config, heartbeat tuning
├── memory/                  # Episodic logs (NDJSON, one file per day)
└── memory.db                # Semantic memory (SQLite: jobs, search index)
```

Each agent owns its memory exclusively — no sharing between agents.

### Robustness & Error Recovery

The agent loop includes multiple layers of fault tolerance:

**Timeouts:**
- **Turn timeout:** 120 seconds per LLM turn (prevents stuck API calls)
- **Tool-call timeout:** 30 seconds per individual tool execution (prevents runaway tools)

**Tool result truncation:**
- Tool outputs exceeding 4096 characters are truncated with a `[truncated]` marker
- Prevents large outputs (e.g., verbose logs) from blowing up the LLM context window

**Provider retry with exponential backoff:**
- Retries on transient HTTP errors (429 rate limit, 5xx server errors)
- Exponential backoff: 1s, 2s, 4s (base * 2^attempt)
- Maximum 3 retries before propagating the error

**Pre-compaction context recovery:**
- When input tokens approach the context limit, the agent flushes working state to episodic memory
- On subsequent turns, the most recent flush is recovered and injected into the system prompt as "Previously Persisted Context"
- This ensures continuity across LLM context truncations

**Job retry with circuit breaker:**
- Failed jobs retry with exponential backoff: 10s * 2^retry_count
- Maximum retries configurable via `max_retries` in capabilities.toml (default: 3)
- After exhausting retries, jobs are dead-lettered (`failed_requires_approval` status)

### Zenoh Gateway Topics

```
bubbaloop/{scope}/{machine}/agent/inbox                ← shared intake (all messages)
bubbaloop/{scope}/{machine}/agent/{agent_id}/outbox    ← per-agent streamed response
bubbaloop/{scope}/{machine}/agent/{agent_id}/manifest  ← agent capabilities (queryable)
```

**Wire format (JSON):**

Inbox (CLI → Daemon):
```json
{"id": "uuid", "text": "user message", "agent": "camera-expert"}
```

Outbox (Daemon → CLI):
```json
{"id": "uuid", "type": "delta", "text": "token..."}
{"id": "uuid", "type": "tool", "text": "get_system_status"}
{"id": "uuid", "type": "tool_result", "text": "..."}
{"id": "uuid", "type": "error", "text": "API error: 429"}
{"id": "uuid", "type": "done"}
```

### Message Routing

1. If the inbox message has an explicit `agent` field → route to that agent
2. Otherwise → route to the default agent (`default = true` in config)
3. If the target agent's inbox is full (capacity 32) → message is dropped with a warning

### Building a Channel Adapter

Because the gateway is a Zenoh convention (not hardcoded to the CLI), you can build adapters for any channel by publishing to the inbox topic and subscribing to outbox topics. The wire format is the same regardless of source.

---

## Quick Start Workflow

1. `list_nodes` → see all skillets with status
2. `get_node_health` → get details for a specific skillet
3. `get_node_schema` → understand its data format
4. `get_stream_info` → get Zenoh topic for live data
5. `send_command` → trigger actions on the skillet
6. `schedule_task` → automate recurring agent jobs

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

### Scheduling Tools

#### `schedule_task`

**Tier:** Operator

Schedule a task for the agent to execute later. Supports one-off and recurring tasks via cron expressions. Uses `tokio::Notify` for immediate pickup.

**Parameters:**
- `prompt` (string, required): The instruction for the agent to execute
- `cron_schedule` (string, optional): Cron expression for recurring tasks (5 or 6 field)
- `recurrence` (boolean, optional): Whether this is a recurring task (default: false)

**Returns:** Job ID on success.

**Example (one-off):**
```json
{
  "prompt": "Check all node health and report"
}
```

**Example (recurring):**
```json
{
  "prompt": "Run health patrol on all sensors",
  "cron_schedule": "*/15 * * * *",
  "recurrence": true
}
```

---

#### `list_jobs`

**Tier:** Viewer

List scheduled jobs. Optionally filter by status.

**Parameters:**
- `status` (string, optional): Filter by status — pending, running, completed, failed, failed_requires_approval

---

#### `delete_job`

**Tier:** Operator

Delete a scheduled job by ID.

**Parameters:**
- `job_id` (string, required): The job ID to delete

---

### Memory Tools

Memory tools operate on the agent's **per-agent** SQLite database at `~/.bubbaloop/agents/{agent_id}/memory.db`. Each agent has isolated memory — no shared global state.

#### `memory_search`

**Tier:** Viewer

Search episodic memory for past conversations, tool results, and agent observations. Uses BM25 full-text search with temporal decay.

**Parameters:**
- `query` (string, required): Search query (keywords or phrases)
- `limit` (integer, optional): Maximum results to return (default: 10)

---

#### `memory_forget`

**Tier:** Admin

Remove matching entries from episodic memory search index. Use for PII removal, correcting false memories, or user-requested deletion. Creates an audit trail.

**Parameters:**
- `query` (string, required): Search query to match entries to forget
- `reason` (string, required): Reason for forgetting (logged in audit trail)

---

#### `create_proposal`

**Tier:** Operator

Create a proposal for human approval before executing a risky action. Use for destructive operations like removing nodes or changing configs.

**Parameters:**
- `skill` (string, required): The tool or action category (e.g., "restart_node", "remove_node")
- `description` (string, required): Human-readable description of what will happen
- `actions` (string, required): JSON array of tool calls to execute if approved

---

#### `list_proposals`

**Tier:** Viewer

List proposals for human-in-the-loop approval.

**Parameters:**
- `status` (string, optional): Filter by status — pending, approved, rejected, expired

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

#### `read_file`

**Tier:** Operator

Read the contents of a file. Returns up to 500 lines. Use for config files, logs, scripts, or any text file on the system.

**Parameters:**
- `path` (string, required): Absolute or relative file path (supports `~/` expansion)

**Returns:** File contents as plain text. Long files are truncated at 500 lines.

**Security:** Sensitive files are blocked: SSH keys (`id_rsa`, `id_ed25519`), `.pem`/`.key`/`.p12` files, `.env` (not `.env.example`), `/etc/shadow`, `/etc/sudoers`, `master.key`.

**Example:** `read_file(path="/etc/hostname")`

---

#### `write_file`

**Tier:** Operator

Write content to a file inside `~/.bubbaloop/workspace/`. Creates parent directories if needed. Writes outside the workspace are blocked.

**Parameters:**
- `path` (string, required): File path (relative paths resolve inside workspace)
- `content` (string, required): File content to write

**Returns:** Confirmation with byte count and path.

**Security:** All writes are scoped to `~/.bubbaloop/workspace/`. Symlink escape prevention via path canonicalization. Directory auto-created on first use.

**Example:**
```json
{
  "path": "scripts/monitor.py",
  "content": "#!/usr/bin/env python3\nprint('Hello')"
}
```

---

#### `run_command`

**Tier:** Operator

Run a shell command and return its output. Captures both stdout and stderr. Use for diagnostics, system inspection, or any task requiring shell access.

**Parameters:**
- `command` (string, required): Shell command to execute (passed to `/bin/sh -c`)
- `timeout_secs` (integer, optional): Timeout in seconds (default: 30, max: 300)

**Returns:** Command output (stdout + stderr). Long output truncated at 50KB.

**Security:** 10-category blocklist enforced (see Security Model section below). Safe operations like `ls`, `df`, `cat`, `pixi`, `cargo` are allowed.

**Example:** `run_command(command="df -h")`

---

## RBAC Tiers

Bubbaloop uses three authorization tiers. Each tool requires a minimum tier to execute.

| Tier | Access Level | MCP Tools |
|------|--------------|-----------|
| **Viewer** (14) | Read-only monitoring | `list_nodes`, `get_node_health`, `get_node_schema`, `get_stream_info`, `get_system_status`, `get_machine_info`, `discover_nodes`, `get_node_manifest`, `list_commands`, `discover_capabilities`, `list_proposals`, `list_jobs`, `get_system_telemetry`, `get_telemetry_history` |
| **Operator** (11) | Day-to-day operations | `start_node`, `stop_node`, `restart_node`, `get_node_config`, `send_command`, `get_node_logs`, `enable_autostart`, `disable_autostart`, `approve_proposal`, `reject_proposal`, `delete_job` |
| **Admin** (8) | System modification | `install_node`, `remove_node`, `build_node`, `query_zenoh`, `uninstall_node`, `clean_node`, `clear_episodic_memory`, `update_telemetry_config` |

**Default tier:** In single-user localhost mode, all requests are granted Admin tier.

**Token format:** `~/.bubbaloop/mcp-token` contains `<token>:<tier>` (e.g., `bb_abc123:operator`)

**Permission model:** Higher tiers inherit lower tier permissions (Admin can do everything, Operator can do Viewer tasks).

RBAC enforcement is in `mcp/rbac.rs` and `mcp/mod.rs` — all MCP tool calls pass through tier validation. Path and command validation for agent-internal tools (`read_file`, `write_file`, `run_command`) is in `dispatch_security.rs`.

---

## Security Model

The agent's system tools (`read_file`, `write_file`, `run_command`) enforce
defence-in-depth security to prevent damage to existing platforms.

### Read Access
- Can read any file on the filesystem
- **Blocked:** SSH keys (`id_rsa`, `id_ed25519`, `id_ecdsa`, `id_dsa`), `.pem`/`.key`/`.p12`/`.pfx`/`.jks` files,
  `.env` (not `.env.example`/`.env.template`/`.env.sample`), `/etc/shadow`, `/etc/sudoers`, `master.key`

### Write Access
- **Scoped to `~/.bubbaloop/workspace/`** — all writes outside are rejected
- Symlink escape prevention via path canonicalization
- Directory auto-created on first use

### Shell Commands

10-category blocklist:

1. **Privilege escalation** — `sudo`, `su` (requires manual execution)
2. **Destructive filesystem** — `rm -rf /`, `mkfs`, `dd if=`, fork bombs
3. **System control** — `shutdown`, `reboot`, `kill`, `killall`, `pkill`, `iptables`, `mount`
4. **Non-bubbaloop service management** — `systemctl stop/disable/mask <non-bubbaloop>` blocked; bubbaloop services allowed
5. **System package managers** — `apt`, `apt-get`, `dpkg`, `yum`, `dnf`, `pacman`, `snap`, `flatpak` (use `pixi`/`pip` for project deps)
6. **Network mutation** — `ifconfig up/down`, `ip link set`, `ip route`, `ip addr`
7. **Remote code execution** — `curl | sh`, `wget | bash` (plain curl/wget for data is fine)
8. **Container destruction** — `docker rm/stop/kill`, `podman rm/stop/kill` (`docker ps/logs/inspect` allowed)
9. **Git destructive ops** — `push --force`, `reset --hard`, `clean -f` (normal git operations allowed)
10. **`rm` scoped** — only files in `~/.bubbaloop/workspace/` and `/tmp` can be removed

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

## Task Scheduling

The scheduling system lets agents execute tasks autonomously. Use `schedule_task` for one-off or recurring jobs.

### Scheduling Pattern

```
schedule_task(prompt="Check all node health", cron_schedule="*/15 * * * *", recurrence=true)
→ Creates a recurring job that runs every 15 minutes
→ Agent processes the prompt autonomously on each trigger
```

### Job Lifecycle

1. **pending** — waiting for next scheduled run
2. **running** — agent is processing the job
3. **completed** — job finished successfully
4. **failed** — job failed, will retry with exponential backoff
5. **failed_requires_approval** — exhausted retries, needs human intervention

### Best Practices

- Use `list_jobs` to monitor job status
- Use `delete_job` to cancel recurring jobs
- Jobs survive daemon restarts (persisted in SQLite)
- Failed jobs retry automatically — check `list_jobs(status="failed")` for stuck jobs

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

- Use `schedule_task` with cron for recurring monitoring (runs in daemon, not via MCP calls)
- Scheduled jobs are more efficient than polling loops
- Check `list_jobs` to monitor scheduled job status

### System Health

- Check `get_system_status` before bulk operations
- Use `get_node_logs` to diagnose failures
- Use `list_jobs(status="failed")` to find stuck jobs

### Performance

- Batch independent operations where possible (MCP rate limit: 100 burst, 1/sec sustained)
- Use Zenoh direct subscription for high-frequency data (1000s msg/sec supported)
- Keep scheduled job prompts focused and specific

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

---

## Troubleshooting

### "No response from node (is it running?)"

Check node status with `get_node_health`. If stopped, use `start_node`. If unhealthy, check `get_node_logs`.

### "Permission denied: tool requires admin tier"

Your token has insufficient permissions. Check `~/.bubbaloop/mcp-token` tier setting.

### "Validation error: ..."

Parameter format is invalid. Check the Tool Reference section for correct parameter schemas.

### "Agent not available"

The daemon was started without the agent runtime. Ensure `bubbaloop up` starts with agent support and `~/.bubbaloop/agents.toml` exists.

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

# Scheduling
schedule_task → list_jobs → delete_job

# Data
get_stream_info → (subscribe to Zenoh topic externally)

# Health
get_system_status → get_node_logs
```

### Tool Count by Tier

**MCP tools** (exposed to external clients via MCP server): 30 tools

- **Viewer:** 14 tools (read-only discovery and status)
- **Operator:** 11 tools (lifecycle, config, commands)
- **Admin:** 8 tools (install, build, system)

**Agent-internal tools** (available only to the LLM agent, not via MCP): 7 additional tools

- `memory_search`, `memory_forget`, `schedule_task`, `create_proposal`, `read_file`, `write_file`, `run_command`

**Total:** 37 tools in agent dispatch (30 MCP + 7 agent-only)

### Key Paths

- Token: `~/.bubbaloop/mcp-token`
- Agents: `~/.bubbaloop/agents/{agent_id}/`
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

5. schedule_task(
     prompt="Check temperature-sensor health and report any anomalies",
     cron_schedule="*/15 * * * *",
     recurrence=true
   )
   → Automate monitoring every 15 minutes

6. list_jobs()
   → Verify job is scheduled and check results
```

Now the agent autonomously monitors temperature every 15 minutes without further MCP calls.

---

## Summary

- **37 tools** (30 MCP + 7 agent-only) across 3 RBAC tiers (Viewer, Operator, Admin)
- **Dual-plane architecture:** MCP for control, Zenoh for data
- **Task scheduling** for autonomous behavior (cron jobs, retry with circuit breaker)
- **Robustness** — turn/tool timeouts, provider retry, context recovery, result truncation
- **Self-describing nodes** with manifests, schemas, commands
- **Multi-machine support** via Zenoh network discovery

Read the tool reference, understand the dual-plane model, use scheduled tasks for automation, and always prefer Zenoh direct subscription over MCP polling for data streams.
