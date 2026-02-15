# Running Bubbaloop

How to run the full stack: daemon, nodes, dashboard, MCP server, and agent rule engine.

## Prerequisites

```bash
git clone https://github.com/kornia/bubbaloop.git
cd bubbaloop
pixi install
```

## 1. Core Services

Bubbaloop needs three core services: Zenoh router, WebSocket bridge, and the daemon.

**If installed via `bubbaloop doctor --fix`** (systemd services):

```bash
# Check status
systemctl --user status bubbaloop-zenohd bubbaloop-bridge bubbaloop-daemon

# Restart if needed
systemctl --user restart bubbaloop-daemon
```

**Manual start (development)**:

```bash
# Terminal 1: Zenoh router
zenohd

# Terminal 2: WebSocket bridge (for dashboard)
zenoh-bridge-remote-api --ws-port 10001 -e tcp/127.0.0.1:7447 -m client --no-multicast-scouting

# Terminal 3: Daemon
pixi run daemon
```

## 2. Nodes (Sensor Data Sources)

```bash
# Weather node (no hardware needed — great for testing)
pixi run openmeteo

# Camera node (needs RTSP camera URL in config)
pixi run cameras

# Or manage via CLI
bubbaloop node list
bubbaloop node start openmeteo
bubbaloop node logs openmeteo -f
```

## 3. Dashboard (Web UI)

```bash
pixi run dashboard    # Dev server at http://localhost:5173
```

Shows live sensor data, node status, fleet overview. Connects to Zenoh via WebSocket bridge on port 10001.

## 4. MCP Server (AI Agent Integration)

The MCP server lets any LLM control sensor nodes via the Model Context Protocol.

```bash
# Build with MCP feature
cargo build --release --features mcp

# Run daemon with MCP enabled
RUST_LOG=info cargo run --bin bubbaloop --package bubbaloop --release --features mcp -- daemon
# → MCP server: http://127.0.0.1:8088/mcp
```

The `.mcp.json` in the repo root configures Claude Code to connect automatically.

**Available MCP tools**: `list_nodes`, `get_node_manifest`, `send_command`, `start_node`, `stop_node`, `restart_node`, `get_node_logs`, `get_node_config`, `get_node_health`, `query_zenoh`, `discover_nodes`, `get_agent_status`, `list_agent_rules`

## 5. Agent Rule Engine

The rule engine starts automatically with the daemon. Define rules in `~/.bubbaloop/rules.yaml`:

```yaml
rules:
  - name: "high-temp-alert"
    trigger: "bubbaloop/**/telemetry/status"
    condition:
      field: "cpu_temp"
      operator: ">"
      value: 80.0
    action:
      type: "log"
      message: "CPU temperature exceeds 80C"

  - name: "capture-on-motion"
    trigger: "bubbaloop/**/motion/detected"
    condition:
      field: "confidence"
      operator: ">="
      value: 0.8
    action:
      type: "command"
      node: "rtsp-camera"
      command: "capture_frame"
      params:
        resolution: "1080p"
```

**Operators**: `==`, `!=`, `>`, `>=`, `<`, `<=`, `contains`

**Action types**:
- `log` — write to daemon log
- `command` — send command to a node's command queryable
- `publish` — publish JSON to a Zenoh topic

**Human overrides**: Publish to `bubbaloop/{scope}/{machine_id}/human/override/**` to temporarily suppress rules for a node.

## 6. Quick Start (All-in-One)

```bash
# Start bridge + build + run camera + weather nodes
pixi run up

# In another terminal: start dashboard
pixi run dashboard
```

## 7. Verify Everything

```bash
# Status check
bubbaloop status
bubbaloop node list

# Interactive TUI
bubbaloop

# Diagnostics
bubbaloop doctor
bubbaloop doctor --fix    # Auto-fix common issues

# Run tests
pixi run test             # Rust tests
cd dashboard && npm test  # Dashboard tests

# Full validation (35 checks)
bash scripts/validate.sh
```

## Troubleshooting

| Problem | Fix |
|---------|-----|
| Dashboard shows no data | Check bridge is running: `pgrep zenoh-bridge` |
| Node not starting | Check logs: `bubbaloop node logs <name>` |
| Zenoh timeout | Verify zenohd: `pgrep zenohd`, restart if missing |
| Duplicate daemon | `bubbaloop doctor --fix` or kill stale process |
| TUI disconnected | `systemctl --user restart bubbaloop-daemon` |
