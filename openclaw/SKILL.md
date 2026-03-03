---
name: bubbaloop-physical-ai
description: "Manage physical sensors, cameras, and actuators via the bubbaloop skill runtime. Query real-time sensor data, control node lifecycle, and monitor hardware health."
metadata:
  openclaw:
    requires:
      bins: ["bubbaloop"]
    os: ["linux"]
---

# Bubbaloop Physical AI Skill

You have access to a physical sensor network managed by bubbaloop. Use the bubbaloop MCP tools to interact with hardware.

## Quick Reference

### Discover what's available
- `list_nodes` — See all registered sensor/actuator nodes and their status
- `get_node_detail(name)` — Get detailed info about a specific node
- `get_system_status` — Overview of the entire system

### Read sensor data
- `query_zenoh(key_expression)` — Query any Zenoh topic for current data
  - Use `bubbaloop/**/output` to find all sensor outputs
  - Use `bubbaloop/**/health/*` to check node health
  - Use `bubbaloop/**/manifest` to discover node capabilities

### Control nodes
- `start_node(name)` — Start a stopped node
- `stop_node(name)` — Stop a running node
- `restart_node(name)` — Restart a node
- `install_node(name)` — Install a node as a system service
- `build_node(name)` — Build a node from source

### Monitor
- `get_node_health(name)` — Check if a node is healthy
- `get_node_logs(name)` — View recent logs
- `get_stream_info(name)` — Get Zenoh connection params for streaming data

### Configure
- `get_node_config(name)` — Read node configuration
- `send_command(name, command, params)` — Send commands to nodes

## Common Patterns

### "What sensors are available?"
Call `list_nodes` to see all nodes. Each node's status shows if it's Running, Stopped, or Failed.

### "What's the temperature?"
1. Call `list_nodes` to find temperature-related nodes
2. Call `query_zenoh("bubbaloop/**/output")` to read current values
3. Parse the protobuf or JSON response

### "Start monitoring cameras"
1. `start_node("rtsp-camera")` to ensure the camera node is running
2. `get_stream_info("rtsp-camera")` to get the Zenoh topic for live frames

### "Something seems wrong"
1. `get_system_status` for overview
2. `get_node_health(name)` for specific node health
3. `get_node_logs(name)` for recent logs

## Architecture

bubbaloop uses a dual-plane model:
- **MCP (Control Plane)**: You interact here — lifecycle, config, commands
- **Zenoh (Data Plane)**: Real-time sensor data flows here — pub/sub, queryables

All topics follow: `bubbaloop/{scope}/{machine}/{node}/{resource}`
