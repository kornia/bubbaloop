# Bubbaloop OpenClaw Integration

This directory contains configuration files for integrating bubbaloop with OpenClaw, an AI agent framework that uses MCP (Model Context Protocol) servers.

## What bubbaloop Provides

Bubbaloop exposes 42 MCP tools for managing physical sensors, cameras, and actuators:

**Discovery (10 tools)**
- `list_nodes` - List all nodes with status and capabilities
- `discover_nodes` - Find all nodes across machines
- `get_node_health` - Check node health status
- `get_node_config` - Read node configuration
- `get_node_manifest` - Get node self-description
- `get_node_schema` - Get protobuf schema of node data
- `get_node_logs` - View recent systemd logs
- `get_stream_info` - Get Zenoh connection params for streaming
- `list_commands` - List available commands for a node
- `discover_capabilities` - Discover node capabilities across fleet

**Lifecycle (10 tools)**
- `install_node` - Install a node from marketplace, path, or GitHub
- `uninstall_node` - Uninstall a node
- `start_node` - Start a stopped node
- `stop_node` - Stop a running node
- `restart_node` - Restart a node
- `build_node` - Build a node from source
- `remove_node` - Remove a node from registry
- `clean_node` - Clean node build artifacts
- `enable_autostart` - Enable systemd autostart
- `disable_autostart` - Disable systemd autostart

**Data (2 tools)**
- `query_zenoh` - Query any Zenoh key expression
- `send_command` - Send commands to nodes

**System (2 tools)**
- `get_system_status` - Overall system health
- `get_machine_info` - Hardware and OS info

**Scheduling & Proposals (6 tools)**
- `list_jobs` - List scheduled jobs
- `delete_job` - Delete a scheduled job
- `list_proposals` - List pending proposals
- `approve_proposal` - Approve a proposal
- `reject_proposal` - Reject a proposal
- `clear_episodic_memory` - Clear agent episodic memory

**Beliefs & World State (4 tools)**
- `get_belief` - Get a belief by subject+predicate
- `update_belief` - Update a belief with confidence
- `list_world_state` - List live sensor-derived state
- `configure_context` - Wire Zenoh topic to world state

**Missions & Safety (6 tools)**
- `list_missions` - List active missions
- `pause_mission` - Pause a running mission
- `resume_mission` - Resume a paused mission
- `cancel_mission` - Cancel a mission
- `register_constraint` - Register safety constraint
- `list_constraints` - List safety constraints

**Alerts & Telemetry (2 tools)**
- `register_alert` - Register reactive alert trigger
- `unregister_alert` - Remove an alert trigger

## Prerequisites

1. Install bubbaloop (place binary in PATH)
2. Start zenohd router: `zenohd`
3. Start bubbaloop daemon: `bubbaloop daemon start`
4. Verify daemon is running: `bubbaloop daemon status`

## Setup

1. Copy the example config to your OpenClaw project:

```bash
cp openclaw/openclaw.json.example ~/.config/openclaw/config.json
```

2. Ensure the bubbaloop daemon is running:

```bash
bubbaloop daemon start
```

3. Verify MCP server is accessible:

```bash
echo '{"jsonrpc":"2.0","id":1,"method":"initialize","params":{"protocolVersion":"2024-11-05","capabilities":{},"clientInfo":{"name":"test","version":"1.0.0"}}}' | bubbaloop mcp --stdio
```

You should see a JSON response with `"capabilities"` and `"serverInfo"`.

## OpenClaw Configuration

OpenClaw will spawn `bubbaloop mcp --stdio` as a child process when your agent starts. The MCP server connects to the local Zenoh router on `tcp/127.0.0.1:7447`.

The `BUBBALOOP_ZENOH_ENDPOINT` environment variable can be customized to connect to a different Zenoh router.

## Available Tools

Once configured, your OpenClaw agent can discover all 42 bubbaloop tools via MCP's `tools/list` method. Each tool includes parameter schemas and descriptions.

Example workflows:

**Check what's connected:**
```
Agent: list_nodes()
```

**Start a camera:**
```
Agent: start_node(node_name="rtsp-camera")
Agent: get_node_health(node_name="rtsp-camera")
```

**Query sensor data:**
```
Agent: query_zenoh(key_expr="bubbaloop/**/temperature/output")
```

**Set up automation:**
```
Agent: add_rule(
  name="high-temp-alert",
  trigger="bubbaloop/**/temperature/output",
  condition={"field":"celsius","operator":">","value":30},
  action={"type":"log","message":"Temperature too high!"}
)
```

## Architecture

Bubbaloop uses a dual-plane architecture:

- **Control Plane (MCP)**: Tool calls for lifecycle, config, commands
- **Data Plane (Zenoh)**: Real-time sensor data streaming

MCP is for control operations. For real-time data streaming (camera frames, high-frequency sensor readings), use `get_stream_info` to obtain Zenoh connection parameters and subscribe directly via a Zenoh client library.

## Troubleshooting

**Tools not showing up:**
- Verify daemon is running: `bubbaloop daemon status`
- Check logs: `journalctl --user -u bubbaloop -n 50`
- Ensure zenohd is running: `pgrep zenohd`

**Permission denied errors:**
- Check that bubbaloop binary is in PATH
- Ensure daemon has correct permissions
- Verify user has access to `~/.bubbaloop/` directory

**Zenoh connection errors:**
- Verify zenohd is running on tcp/127.0.0.1:7447
- Check `BUBBALOOP_ZENOH_ENDPOINT` environment variable
- Try `zenoh-cli query 'bubbaloop/**'` to test connectivity

**Node operations failing:**
- Verify nodes are installed: `bubbaloop node list`
- Check node status: `bubbaloop node status <name>`
- View logs: `bubbaloop node logs <name>`

## SKILL.md Reference

The `SKILL.md` file follows the OpenClaw skill specification. It provides:

- Skill metadata (name, description, requirements)
- Tool reference guide for the AI agent
- Common usage patterns
- Architecture overview

OpenClaw uses this file to understand what the bubbaloop skill provides and how to use it effectively.

## Further Reading

- Bubbaloop documentation: `/home/nvidia/bubbaloop/CLAUDE.md`
- MCP specification: https://modelcontextprotocol.io
- Zenoh documentation: https://zenoh.io
