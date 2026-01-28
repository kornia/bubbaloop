# Bubbaloop

Physical AI camera streaming platform built on Zenoh/ROS-Z.

## Quick Start

```bash
# 1. Start Zenoh router (required for all services to communicate)
zenohd &

# 2. Start Zenoh WebSocket bridge (for dashboard browser access)
zenoh-bridge-remote-api --ws-port 10001 -e tcp/127.0.0.1:7447 &

# 3. Start the daemon (manages all nodes via systemd)
pixi run daemon

# 4. Start the dashboard (in another terminal)
pixi run dashboard
```

## Architecture

```
┌─────────────────┐     ┌─────────────────┐     ┌─────────────────┐
│    Dashboard    │     │   bubbaloop-tui │     │   future MCP    │
│  (React/Vite)   │     │   (Node.js)     │     │                 │
└────────┬────────┘     └────────┬────────┘     └────────┬────────┘
         │ WebSocket             │                       │
         │ (port 10001)          │                       │
         └───────────────────────┼───────────────────────┘
                                 │ Zenoh pub/sub
                    ┌────────────┴────────────┐
                    │   zenoh-bridge-remote   │
                    │   (WebSocket → Zenoh)   │
                    └────────────┬────────────┘
                                 │
                    ┌────────────┴────────────┐
                    │        zenohd           │
                    │   (router on :7447)     │
                    └────────────┬────────────┘
                                 │
         ┌───────────────────────┼───────────────────────┐
         │                       │                       │
┌────────┴────────┐   ┌─────────┴─────────┐   ┌────────┴────────┐
│ bubbaloop-daemon│   │   rtsp-camera     │   │    openmeteo    │
│                 │   │                   │   │                 │
│ • Node registry │   │ • H264 capture    │   │ • Weather API   │
│ • systemd D-Bus │   │ • Protobuf encode │   │ • Forecast data │
│ • State pub/sub │   │ • SHM publish     │   │                 │
└─────────────────┘   └───────────────────┘   └─────────────────┘
```

### Components

| Component | Description | Zenoh Topics |
|-----------|-------------|--------------|
| **zenohd** | Central router, all peers connect here | - |
| **zenoh-bridge-remote-api** | WebSocket bridge for browser | - |
| **bubbaloop-daemon** | Node manager with systemd integration | `bubbaloop/daemon/*` |
| **rtsp-camera** | RTSP camera streaming | `/camera/{name}/compressed`, `camera/{name}/raw_shm` |
| **openmeteo** | Weather data from Open-Meteo API | `/weather/current`, `/weather/hourly`, `/weather/daily` |

### Directory Structure

```
crates/
├── bubbaloop/           # Core library: schemas + plugin SDK
├── bubbaloop-daemon/    # Daemon service (manages nodes)
└── bubbaloop-nodes/     # All node implementations
    ├── rtsp-camera/     # RTSP camera streaming
    ├── openmeteo/       # Weather data
    ├── foxglove/        # Foxglove bridge
    └── recorder/        # MCAP recorder

dashboard/               # React dashboard (Vite + TypeScript)
protos/                  # Protobuf schema definitions
```

## Running Services

### Option 1: Manual (Development)

```bash
# Terminal 1: Zenoh router
zenohd

# Terminal 2: WebSocket bridge
zenoh-bridge-remote-api --ws-port 10001 -e tcp/127.0.0.1:7447

# Terminal 3: Daemon
pixi run daemon

# Terminal 4: Dashboard
pixi run dashboard
```

### Option 2: Systemd Services (Production)

Nodes are managed as systemd user services by the daemon:

```bash
# View all node services
systemctl --user list-units 'bubbaloop-*'

# Start/stop a specific node
systemctl --user start bubbaloop-rtsp-camera
systemctl --user stop bubbaloop-openmeteo

# View logs
systemctl --user status bubbaloop-rtsp-camera
journalctl --user -u bubbaloop-rtsp-camera -f
```

## Pixi Tasks

```bash
# Build all
pixi run build

# Run daemon (node manager)
pixi run daemon

# Run dashboard dev server
pixi run dashboard

# Run individual nodes manually
pixi run cameras -- -c configs/cameras.yaml
pixi run openmeteo -- -c configs/config.yaml

# Development
pixi run check      # cargo check
pixi run test       # cargo test
pixi run fmt        # cargo fmt
pixi run clippy     # cargo clippy
```

## Dashboard Features

- **Camera panels**: Live H264 video from RTSP cameras
- **Nodes panel**: Monitor and control all nodes via Zenoh
  - Start/stop services
  - View service logs (auto-refresh)
  - Build nodes
  - Enable/disable autostart
- **Weather panel**: Current conditions and forecasts
- **Raw data panel**: View any Zenoh topic with protobuf decoding

## Node Development

Each node lives in `crates/bubbaloop-nodes/{name}/` with:

```
{name}/
├── Cargo.toml
├── node.yaml          # Node manifest (name, version, type)
├── configs/           # Configuration files
├── src/
│   ├── lib.rs
│   └── bin/{name}_node.rs
└── .cargo/
    └── config.toml    # target-dir = "target" (local builds)
```

### node.yaml format

```yaml
name: my-node
version: "0.1.0"
description: "My custom node"
type: rust  # or python
build: "pixi run build"  # optional build command
command: "./target/release/my_node"  # optional run command
depends_on:  # optional service dependencies
  - rtsp-camera
  - openmeteo
```

#### Service Dependencies

The `depends_on` field specifies other nodes that must be running before this node starts. When the systemd service is installed, this generates:
- `After=network.target bubbaloop-rtsp-camera.service bubbaloop-openmeteo.service`
- `Requires=bubbaloop-rtsp-camera.service bubbaloop-openmeteo.service`

### Building a node

```bash
cd crates/bubbaloop-nodes/my-node
pixi run cargo build --release
```

## Creating a New Node

### Quick Start (Rust)

```bash
# Initialize a new Rust node (creates in current directory)
bubbaloop node init my-sensor --type rust -d "My custom sensor"
cd my-sensor

# Edit your logic in src/node.rs

# Build
cargo build --release

# Register with daemon and start
bubbaloop node add .
bubbaloop node start my-sensor
bubbaloop node logs my-sensor -f
```

### Quick Start (Python)

```bash
# Initialize a new Python node
bubbaloop node init my-sensor --type python -d "My custom sensor"
cd my-sensor

# Edit your logic in main.py

# Register with daemon and start
bubbaloop node add .
bubbaloop node start my-sensor
bubbaloop node logs my-sensor -f
```

### Two-Step Workflow

**Init** creates the node structure (scaffolding):
```bash
bubbaloop node init my-sensor           # Creates ./my-sensor/
bubbaloop node init my-sensor -o /path  # Creates at specified path
```

**Add** registers an existing node with the daemon:
```bash
bubbaloop node add .                    # Add node in current directory
bubbaloop node add /path/to/my-sensor   # Add node at path
bubbaloop node add user/repo            # Clone from GitHub and add
```

This separation allows:
- Creating nodes anywhere in your filesystem
- Keeping nodes in their own git repos
- Linking multiple nodes from a monorepo
- Unlinking without deleting files

### Adding External Nodes

```bash
# From GitHub (full URL)
bubbaloop node add https://github.com/user/awesome-node

# From GitHub (shorthand)
bubbaloop node add user/awesome-node

# From GitHub with branch
bubbaloop node add user/awesome-node --branch develop

# From local path
bubbaloop node add /path/to/my-node

# Add and auto-build
bubbaloop node add user/awesome-node --build

# Add, build, and install as service
bubbaloop node add user/awesome-node --build --install
```

### Node Lifecycle

```bash
bubbaloop node list              # Show all nodes
bubbaloop node validate ./       # Validate node in current directory
bubbaloop node build my-sensor   # Build (Rust nodes)
bubbaloop node start my-sensor   # Start as service
bubbaloop node stop my-sensor    # Stop service
bubbaloop node logs my-sensor -f # Follow logs
bubbaloop node install my-sensor # Install as systemd service
bubbaloop node enable my-sensor  # Enable autostart
```

### Using TUI for Node Management

```bash
bubbaloop              # Launch TUI (default)
/nodes                 # Type command to go to nodes view
```

In Nodes view:
- **Tab**: Switch between Installed/Discover/Marketplace tabs
- **n**: Create new node (in Discover tab)
- **Enter**: View node details
- **s**: Start/stop node
- **b**: Build node
- **l**: View logs

The binary will be at `target/release/my_node`.

## Distributed Deployment

For multi-machine deployments (e.g., multiple Jetsons with central dashboard), see [docs/distributed-deployment.md](docs/distributed-deployment.md).

### Zenoh Configuration Files

| Config | Use Case | Location |
|--------|----------|----------|
| `configs/zenoh/standalone.json5` | Single-machine development | Default |
| `configs/zenoh/central-router.json5` | Central server (dashboard host) | Server |
| `configs/zenoh/jetson-router.json5` | Each Jetson device | Edge devices |

### Environment Variables

| Variable | Description | Default |
|----------|-------------|---------|
| `BUBBALOOP_ZENOH_ENDPOINT` | Zenoh router endpoint | `tcp/127.0.0.1:7447` |
| `RUST_LOG` | Log level | `info` |

### Quick Multi-Machine Setup

```bash
# On central server
zenohd -c configs/zenoh/central-router.json5

# On each Jetson (edit config first with central IP)
zenohd -c configs/zenoh/jetson-router.json5
BUBBALOOP_ZENOH_ENDPOINT=tcp/127.0.0.1:7447 bubbaloop-daemon
```

## Key Dependencies

- **Zenoh** - Pub/sub messaging (v1.7.x)
- **zenoh-ts** - TypeScript Zenoh client for dashboard
- **ros-z** - ROS 2 compatibility layer
- **prost** - Protobuf serialization
- **GStreamer** - H264 camera capture
- **zbus** - D-Bus client for systemd integration

## Daemon Service Management

The bubbaloop-daemon provides advanced service management features:

### Security Hardening

Generated systemd service units include security directives:

```ini
# Security hardening
NoNewPrivileges=true
ProtectSystem=strict
PrivateTmp=true
ProtectKernelTunables=true
ProtectKernelModules=true
ProtectControlGroups=true
# Robotics-compatible settings (allow RT scheduling and JIT)
RestrictRealtime=false
MemoryDenyWriteExecute=false
```

To apply security hardening to existing services, reinstall them:
```bash
# Via dashboard: Uninstall then Install the node
# Or programmatically via Zenoh commands
```

Analyze service security:
```bash
systemd-analyze --user security bubbaloop-rtsp-camera.service
```

### Real-time State Updates (D-Bus Signals)

The daemon subscribes to systemd D-Bus signals for instant state updates:
- **JobRemoved** - Service started/stopped/failed
- **UnitNew** - Service installed
- **UnitRemoved** - Service uninstalled

This provides <100ms state update latency (vs 5s polling previously). Polling now runs every 30s as a backup sync.

### Build Queue and Timeout

Build operations are managed to prevent issues:
- **Concurrent build prevention**: Only one build per node at a time
- **10-minute timeout**: Builds are killed if they exceed the timeout
- **Process cleanup**: `kill_on_drop(true)` ensures child processes are terminated

Events emitted: `building`, `build_complete`, `build_failed`, `build_timeout`

### Health Monitoring (Zenoh Heartbeats)

Nodes can publish heartbeats to `bubbaloop/nodes/{name}/health` for health monitoring:

```rust
// In your node, publish periodic heartbeats:
session.put("bubbaloop/nodes/my-node/health", "ok").await?;
```

The daemon tracks:
- **HealthStatus**: `UNKNOWN`, `HEALTHY`, `UNHEALTHY`
- **last_health_check_ms**: Timestamp of last heartbeat

A node is marked `UNHEALTHY` if no heartbeat is received for 30 seconds while the service is running.

### Protobuf Schema (daemon.proto)

```protobuf
enum HealthStatus {
  HEALTH_STATUS_UNKNOWN = 0;
  HEALTH_STATUS_HEALTHY = 1;
  HEALTH_STATUS_UNHEALTHY = 2;
}

message NodeState {
  string name = 1;
  string path = 2;
  NodeStatus status = 3;
  bool installed = 4;
  bool autostart_enabled = 5;
  string version = 6;
  string description = 7;
  string node_type = 8;
  bool is_built = 9;
  int64 last_updated_ms = 10;
  repeated string build_output = 11;
  HealthStatus health_status = 12;
  int64 last_health_check_ms = 13;
}
```

## Git Hygiene & Artifacts

### Files that should NOT be committed

The following are automatically ignored via `.gitignore`:

| Pattern | Description |
|---------|-------------|
| `target/`, `debug/` | Rust build artifacts |
| `node_modules/`, `dist/` | Node.js dependencies and builds |
| `.pixi/` | Pixi environment directories |
| `*.pb.js`, `*.pb.d.ts` | Generated protobuf files (dashboard) |
| `*.pyc`, `__pycache__/` | Python bytecode |
| `*.so`, `*.dylib`, `*.dll` | Compiled libraries |

### Files that SHOULD be committed

| File | Reason |
|------|--------|
| `Cargo.lock` | Reproducible Rust builds |
| `pixi.lock` | Reproducible pixi environments |
| `package-lock.json` | Reproducible npm installs |

### Pre-commit checks

Before committing, always run:

```bash
pixi run fmt       # Format Rust code
pixi run clippy    # Lint Rust code
```

## Claude Code Instructions

### Adding new boilerplate patterns

When you create new templates, generated files, or build artifacts:

1. **Update `.gitignore`** - Add patterns for any new generated/build files
2. **Update this section** - Document what should/shouldn't be committed
3. **Check before commit** - Run `git status` to verify no artifacts are staged

### Common patterns to watch for

When adding new node types or build systems, ensure these are ignored:

```gitignore
# Python nodes
**/__pycache__/
**/*.pyc
**/*.egg-info/
**/venv/
**/.venv/

# Rust nodes (already covered by target/)
**/target/

# Generated code
**/*.generated.*
**/generated/

# Build outputs
**/build/
**/out/
```

### Automation checklist

When creating new boilerplate (templates, nodes, etc.):

- [ ] Add build output patterns to `.gitignore`
- [ ] Add generated file patterns to `.gitignore`
- [ ] Document the pattern in this CLAUDE.md file
- [ ] Verify with `git status` before committing
- [ ] Run `cargo fmt` and `pixi run clippy` before commit
