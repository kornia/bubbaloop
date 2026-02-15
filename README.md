# Bubbaloop

AI-native orchestration for Physical AI — multi-camera streaming, fleet management, and real-time visualization built on Zenoh.

## Quick Install

```bash
# One-line install (Linux x86_64/ARM64)
curl -sSL https://github.com/kornia/bubbaloop/releases/latest/download/install.sh | bash
source ~/.bashrc

# Verify
bubbaloop status
```

## What Gets Installed

| Component | Description |
|-----------|-------------|
| `zenohd` | Pub/sub router on port 7447 |
| `zenoh-bridge-remote-api` | WebSocket bridge on port 10001 |
| `bubbaloop` | Single 7MB binary: CLI + TUI + daemon |
| Dashboard | Web UI at http://localhost:8080 |

All run as systemd user services with autostart enabled.

## Basic Usage

```bash
# Launch TUI (interactive terminal UI)
bubbaloop

# Non-interactive status check (for scripts/agents)
bubbaloop status

# System diagnostics with auto-fix
bubbaloop doctor --fix

# Node management
bubbaloop node list
bubbaloop node add user/repo          # Add from GitHub
bubbaloop node build my-node          # Build
bubbaloop node start my-node          # Start service
bubbaloop node logs my-node -f        # Follow logs
```

## Node Lifecycle

```bash
# 1. Create a new node
bubbaloop node init my-sensor --node-type rust

# 2. Register with daemon
bubbaloop node add ./my-sensor

# 3. Build
bubbaloop node build my-sensor

# 4. Install as systemd service
bubbaloop node install my-sensor

# 5. Start
bubbaloop node start my-sensor

# 6. View logs
bubbaloop node logs my-sensor
```

## Multi-Instance Nodes

For nodes that can run multiple instances (e.g., cameras):

```bash
# Add base node once
bubbaloop node add ~/.bubbaloop/nodes/rtsp-camera

# Create instances with different configs
bubbaloop node instance rtsp-camera terrace --config config-terrace.yaml
bubbaloop node instance rtsp-camera entrance --config config-entrance.yaml

# Each runs as separate service
bubbaloop node start rtsp-camera-terrace
bubbaloop node start rtsp-camera-entrance
```

## Development

```bash
git clone https://github.com/kornia/bubbaloop.git
cd bubbaloop
pixi install
pixi run build     # Build all
pixi run test      # Run tests
pixi run daemon    # Run daemon
pixi run dashboard # Run web dashboard
```

## Service Management

```bash
# View all services
systemctl --user list-units 'bubbaloop-*'

# Restart daemon
systemctl --user restart bubbaloop-daemon

# View logs
journalctl --user -u bubbaloop-daemon -f
```

## Troubleshooting

```bash
# Quick diagnostics
bubbaloop doctor

# Auto-fix common issues
bubbaloop doctor --fix

# JSON output for scripting
bubbaloop doctor --json
```

Common issues:
- **TUI disconnected**: `bubbaloop doctor --fix` restarts stale services
- **Zenoh timeout**: Check `pgrep zenohd`, restart if missing
- **Build fails**: Check `bubbaloop node logs <name>` for errors

## Documentation

- **Full docs**: `pixi run docs` or see [docs/](docs/)
- **Agent guidelines**: See [CLAUDE.md](CLAUDE.md) for architecture and coding standards
- **CLI reference**: `bubbaloop --help` or `bubbaloop node --help`

## AI Agent Integration (MCP)

Bubbaloop includes an MCP (Model Context Protocol) server that lets any LLM control your sensor nodes via natural language. Build with `--features mcp` to enable.

```bash
# Build with MCP support
cargo build --release --features mcp

# The daemon starts the MCP server on port 8088
bubbaloop daemon
# → MCP server: http://127.0.0.1:8088/mcp
```

**Available MCP tools:**

| Tool | Description |
|------|-------------|
| `list_nodes` | List all nodes with status |
| `get_node_manifest` | Get a node's capabilities and topics |
| `send_command` | Send a command to a node |
| `start_node` / `stop_node` | Control node lifecycle |
| `get_node_logs` | Read node service logs |
| `discover_nodes` | Fleet-wide manifest discovery |
| `query_zenoh` | Query any Zenoh key expression |
| `get_agent_status` | View agent rule engine status |
| `list_agent_rules` | List active automation rules |

Configure Claude Code to use it via `.mcp.json` (already in project root).

## Agent Rule Engine

The daemon includes a lightweight rule engine for autonomous sensor-to-action automation. Define rules in `~/.bubbaloop/rules.yaml`:

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

Rules support: `==`, `!=`, `>`, `>=`, `<`, `<=`, `contains` operators. Actions: `log`, `command` (send to node), `publish` (to Zenoh topic). Human overrides via `bubbaloop/{scope}/{machine_id}/human/override/**` topic.

## Node Contract

Every node is self-describing with standard queryables:

```
{node}/schema      → Protobuf FileDescriptorSet (binary)
{node}/manifest    → Capabilities, topics, commands (JSON)
{node}/health      → Status and uptime (JSON)
{node}/config      → Current configuration (JSON)
{node}/command     → Imperative actions (JSON request/response)
```

AI agents discover nodes via `bubbaloop/**/manifest` wildcard query, then interact through commands and data subscriptions.

## Architecture

```
                    ┌──────────────────────────────────┐
                    │   AI Agent (Claude via MCP)       │
                    │   http://127.0.0.1:8088/mcp      │
                    └──────────────┬───────────────────┘
                                   │
Dashboard (React) ─┬─ WebSocket ───┤─── Zenoh pub/sub
TUI (ratatui) ─────┤               │
CLI ───────────────┘               │
                                   │
Daemon ────────────────────────────┤
  ├─ Node Manager (lifecycle)      │
  ├─ Agent Rule Engine (rules.yaml)│
  ├─ MCP Server (feature-gated)   │
  └─ Systemd D-Bus (zbus)         │
                                   │
Nodes (self-describing) ───────────┘
  ├─ rtsp-camera  [schema|manifest|health|config|command]
  ├─ openmeteo    [schema|manifest|health|config|command]
  └─ custom...    [schema|manifest|health|config|command]
```

## License

Apache-2.0
