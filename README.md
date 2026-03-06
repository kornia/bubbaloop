# 🦐 Bubbaloop

> *"Shrimp-fried cameras, shrimp-grilled sensors, shrimp-sauteed robots..."*
> — Bubba, on all the ways to talk to your hardware 🦐

**The open-source Hardware AI agent.** Talk to your cameras, sensors, and robots in natural language. Manage federated IoT/robotics fleets and automate physical systems — all from a single 13 MB Rust binary.

## Why Bubbaloop?

AI agents revolutionized software engineering. **Bubbaloop brings that same power to hardware.**

| | General AI Agents | **Bubbaloop** |
|---|---|---|
| **Focus** | Software tasks, coding, browsing | **Cameras, sensors, robots, IoT** |
| **Runtime** | TypeScript / ~200 MB | **Rust / ~13 MB** |
| **Data plane** | None | **Zenoh (zero-copy pub/sub)** |
| **Hardware** | None | **Self-describing sensor nodes** |
| **Runs on** | Desktop / cloud | **Jetson, RPi, any Linux ARM64/x86** |
| **MCP role** | Client (consumes tools) | **Server (37 tools, 3-tier RBAC)** |
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
| `bubbaloop` | Single ~13 MB binary: CLI + daemon + MCP server |
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

# Start daemon (runs agent runtime + MCP server + node manager)
bubbaloop up

# Talk to your hardware via Claude AI (agents run daemon-side)
bubbaloop agent chat "What sensors do I have?"
bubbaloop agent chat                   # Interactive REPL
bubbaloop agent chat -a camera-expert "describe the video feed"
bubbaloop agent list                   # Show running agents + models

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

37 tools total (30 MCP + 7 agent-internal). Configure Claude Code via `.mcp.json` (already in project root).

**Agent-internal tools** (daemon-side only, not exposed via MCP): `memory_search`, `memory_forget`, `schedule_task`, `create_proposal`, `read_file`, `write_file`, `run_command`.

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
  ├─ MCP Server (30 tools)         │
  ├─ Telemetry Watchdog            │
  ├─ Agent Runtime (multi-agent)   │
  └─ Systemd D-Bus (zbus)         │
                                   │
Nodes (self-describing) ───────────┘
  ├─ rtsp-camera  [schema|manifest|health|config|command]
  ├─ openmeteo    [schema|manifest|health|config|command]
  └─ custom...    [schema|manifest|health|config|command]
```

The daemon hosts the **agent runtime** (multi-agent Zenoh gateway) alongside the MCP server. Agents are configured via `~/.bubbaloop/agents.toml` with per-agent identity and memory in `~/.bubbaloop/agents/{id}/`. The CLI is a thin Zenoh client — all LLM processing runs daemon-side.

**Per-agent features:**
- **Soul**: `identity.md` (personality) + `capabilities.toml` (model, heartbeat). Hot-reload on file change.
- **3-Tier Memory**: RAM (current turn) → NDJSON logs (episodic, BM25 search) → SQLite (jobs, proposals).
- **Adaptive Heartbeat**: Arousal-based decay — active agents check in frequently, idle agents stay quiet.
- **Telemetry Watchdog**: CPU/RAM/disk monitoring with circuit breakers and 5 severity levels.

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

- **Quickstart**: See [docs/getting-started/quickstart.md](docs/getting-started/quickstart.md)
- **Agent guide**: See [docs/agent-guide.md](docs/agent-guide.md) for multi-agent setup and MCP tools
- **Architecture**: See [ARCHITECTURE.md](ARCHITECTURE.md) for design decisions
- **Roadmap**: See [ROADMAP.md](ROADMAP.md) for what's next
- **Coding standards**: See [CLAUDE.md](CLAUDE.md) for conventions
- **CLI reference**: `bubbaloop --help` or `bubbaloop node --help`

## License

Apache-2.0
