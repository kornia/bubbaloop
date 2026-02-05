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

## Architecture

```
Dashboard (React) ─┬─ WebSocket ─── zenoh-bridge ─┬─ Zenoh pub/sub
TUI (ratatui) ─────┤                              │
CLI ───────────────┘                              │
                                                  │
Daemon ───────────────────────────────────────────┤
  ├─ Node Manager (lifecycle, builds)            │
  ├─ Registry (~/.bubbaloop/nodes.json)          │
  └─ Systemd D-Bus (zbus)                        │
                                                  │
Nodes ────────────────────────────────────────────┘
  ├─ rtsp-camera (RTSP → H264 → Zenoh)
  ├─ openmeteo (weather API → Zenoh)
  └─ custom nodes...
```

## License

Apache-2.0
