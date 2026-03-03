# 🦐 Bubbaloop

> *"Shrimp-fried cameras, shrimp-grilled sensors, shrimp-sauteed robots..."*
> — Bubba, on all the ways to talk to your hardware 🦐

**The open-source Hardware AI agent.** Talk to your cameras, sensors, and robots in natural language. Manage federated IoT/robotics fleets and automate physical systems — all from a single 12 MB Rust binary.

## Why Bubbaloop?

AI agents revolutionized software engineering. **Bubbaloop brings that same power to hardware.**

| | General AI Agents | **Bubbaloop** |
|---|---|---|
| **Focus** | Software tasks, coding, browsing | **Cameras, sensors, robots, IoT** |
| **Runtime** | TypeScript / ~200 MB | **Rust / ~12 MB** |
| **Data plane** | None | **Zenoh (zero-copy pub/sub)** |
| **Hardware** | None | **Self-describing sensor nodes** |
| **Runs on** | Desktop / cloud | **Jetson, RPi, any Linux ARM64/x86** |
| **MCP role** | Client (consumes tools) | **Server (provides 23+ tools)** |
| **Scheduling** | Always-on LLM (~$5-10/day) | **Offline Tier 1 + LLM Tier 2 (~$0.05/day)** |

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
| `bubbaloop` | Single ~12 MB binary: CLI + daemon + MCP server |
| Dashboard | Web UI at http://localhost:8080 |

All run as systemd user services with autostart enabled.

## Login & Authentication

```bash
# Option 1: API Key (pay-as-you-go)
bubbaloop login
# → Choose [1], paste your key from console.anthropic.com

# Option 2: Claude Subscription (Pro/Max/Team)
claude setup-token    # Run in Claude Code CLI first
bubbaloop login
# → Choose [2], paste the sk-ant-oat01-* token

# Check auth status
bubbaloop login --status

# Remove credentials
bubbaloop logout
```

## Basic Usage

```bash
# Check system status
bubbaloop status

# Talk to your hardware via Claude AI
bubbaloop agent

# System diagnostics with auto-fix
bubbaloop doctor --fix

# Node management
bubbaloop node list
bubbaloop node add user/repo          # Add from GitHub
bubbaloop node build my-node          # Build
bubbaloop node start my-node          # Start service
bubbaloop node logs my-node -f        # Follow logs

# Load skills and start sensor nodes
bubbaloop up
```

## Node Lifecycle

```bash
# 1. Create a new node (generates SDK-based scaffold)
bubbaloop node init my-sensor --node-type rust
# Edit src/main.rs — implement Node trait (init + run)
# The SDK handles Zenoh session, health, schema, config, shutdown

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

## YAML Skills (Zero-Code Sensors)

```yaml
# ~/.bubbaloop/skills/front-camera.yaml
name: front-door
driver: rtsp
config:
  url: rtsp://192.168.1.100/stream
```

```bash
# Load all skills, auto-install drivers, start nodes
bubbaloop up
```

## AI Agent Integration (MCP)

Bubbaloop includes an MCP (Model Context Protocol) server — the sole control interface for AI agents. The daemon starts it automatically on port 8088.

```bash
# MCP over stdio (for Claude Code / local agents)
bubbaloop mcp --stdio

# MCP over HTTP (daemon mode, auto-started)
bubbaloop daemon
# → MCP server: http://127.0.0.1:8088/mcp
```

**Available MCP tools:**

| Tool | Description |
|------|-------------|
| `list_nodes` | List all nodes with status |
| `get_node_manifest` | Get a node's capabilities and topics |
| `send_command` | Send a command to a node |
| `install_node` / `uninstall_node` | Install or remove nodes |
| `start_node` / `stop_node` | Control node lifecycle |
| `get_node_logs` | Read node service logs |
| `discover_nodes` | Fleet-wide manifest discovery |
| `query_zenoh` | Query any Zenoh key expression |

Configure Claude Code to use it via `.mcp.json` (already in project root).

## Architecture

```
                    ┌──────────────────────────────────┐
                    │   AI Agent (Claude via MCP)       │
                    │   http://127.0.0.1:8088/mcp      │
                    └──────────────┬───────────────────┘
                                   │
Dashboard (React) ─┬─ WebSocket ───┤─── Zenoh pub/sub
CLI ───────────────┘               │
                                   │
Daemon ────────────────────────────┤
  ├─ Node Manager (lifecycle)      │
  ├─ MCP Server (23+ tools)       │
  ├─ Agent Layer (Claude API)      │
  └─ Systemd D-Bus (zbus)         │
                                   │
Nodes (self-describing) ───────────┘
  ├─ rtsp-camera  [schema|manifest|health|config|command]
  ├─ openmeteo    [schema|manifest|health|config|command]
  └─ custom...    [schema|manifest|health|config|command]
```

The daemon is a **passive skill runtime** — external AI agents (Claude Code, etc.) control everything through MCP. No autonomous decision-making.

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
- **Zenoh timeout**: Check `pgrep zenohd`, restart if missing
- **Build fails**: Check `bubbaloop node logs <name>` for errors
- **Auth failed**: Run `bubbaloop login --status` to check credentials

## Documentation

- **Full docs**: `pixi run docs` or see [docs/](docs/)
- **Architecture**: See [ARCHITECTURE.md](ARCHITECTURE.md) for design decisions
- **Roadmap**: See [ROADMAP.md](ROADMAP.md) for what's next
- **Agent guidelines**: See [CLAUDE.md](CLAUDE.md) for coding standards
- **CLI reference**: `bubbaloop --help` or `bubbaloop node --help`

## License

Apache-2.0
