# Bubbaloop

AI-native orchestration for Physical AI, built on Zenoh.

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
│    Dashboard    │     │  bubbaloop TUI  │     │   future MCP    │
│  (React/Vite)   │     │   (ratatui)     │     │                 │
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
│bubbaloop daemon │   │   rtsp-camera     │   │    openmeteo    │
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
| **bubbaloop daemon** | Node manager with systemd integration (built into CLI) | `bubbaloop/daemon/*` |
| **rtsp-camera** | RTSP camera streaming | `/camera/{name}/compressed` |
| **openmeteo** | Weather data from Open-Meteo API | `/weather/current`, `/weather/hourly`, `/weather/daily` |

### Directory Structure

```
crates/
├── bubbaloop/           # Single binary: CLI + TUI + daemon
└── bubbaloop-schemas/   # Protobuf schemas for node communication (standalone, not in workspace)

dashboard/               # React dashboard (Vite + TypeScript)
scripts/                 # Install script, activation scripts
configs/                 # Zenoh configuration files
docs/                    # MkDocs documentation site
```

**Note:** Official nodes (rtsp-camera, openmeteo, inference, system-telemetry, network-monitor) live in the
separate [bubbaloop-nodes-official](https://github.com/kornia/bubbaloop-nodes-official) repo.
Each node is self-contained with its own protobuf definitions (see [Self-Contained Node Proto Pattern](#self-contained-node-proto-pattern)).

### Single Binary Architecture

The `bubbaloop` binary is a single ~7MB statically-linked Rust binary that includes:

- **CLI** (`bubbaloop status`, `bubbaloop node ...`, `bubbaloop doctor`, `bubbaloop debug ...`)
- **TUI** (`bubbaloop tui` or just `bubbaloop` — ratatui-based terminal UI)
- **Daemon** (`bubbaloop daemon` — long-running node manager with systemd/D-Bus integration)

The systemd service is named `bubbaloop-daemon.service` but runs `bubbaloop daemon` as its ExecStart.
The old separate `bubbaloop-daemon` binary no longer exists.

### CLI Subcommands

```
bubbaloop                     # Show help
bubbaloop tui                 # Launch ratatui TUI
bubbaloop status              # Show service status (table format)
bubbaloop status -f json      # Show service status (JSON)
bubbaloop doctor              # Run all system diagnostics
bubbaloop doctor -c zenoh     # Check Zenoh connectivity only
bubbaloop doctor -c daemon    # Check daemon health only
bubbaloop doctor --json       # Output diagnostics as JSON
bubbaloop doctor --fix        # Auto-fix common issues
bubbaloop daemon              # Run the daemon (node manager)
bubbaloop daemon -z <endpoint>  # Connect to specific Zenoh endpoint
bubbaloop daemon --strict     # Exit if another daemon already running
bubbaloop node init <name>    # Scaffold a new node
bubbaloop node add <path|url> # Register node with daemon
bubbaloop node instance <base> <suffix>  # Create instance of multi-instance node
bubbaloop node list           # List registered nodes
bubbaloop node list --base    # List only base nodes (no instances)
bubbaloop node list --instances # List only instances
bubbaloop node validate <path>  # Validate node.yaml
bubbaloop node build <name>   # Build a node
bubbaloop node install <name> # Install as systemd service
bubbaloop node uninstall <name> # Remove systemd service
bubbaloop node start <name>   # Start node service
bubbaloop node stop <name>    # Stop node service
bubbaloop node restart <name> # Restart node service
bubbaloop node logs <name>    # View node logs
bubbaloop node enable <name>  # Enable autostart
bubbaloop node remove <name>  # Unregister node from daemon
bubbaloop debug info          # Show Zenoh connection info
bubbaloop debug topics        # List active Zenoh topics
bubbaloop debug subscribe <key> # Subscribe to Zenoh topic
bubbaloop debug query <key>   # Query Zenoh endpoint
bubbaloop marketplace list    # List node registry sources
bubbaloop marketplace add <name> <path>  # Add a node source
bubbaloop marketplace remove <name>      # Remove a source
bubbaloop marketplace enable <name>      # Enable a source
bubbaloop marketplace disable <name>     # Disable a source
```

**Note**: `marketplace` manages *sources* (registries where nodes come from), not nodes themselves.
Use `node` commands to manage actual nodes. Pattern follows Claude Code's plugin architecture:
- `marketplace` = where to find nodes (sources)
- `node` = actual node management (add, instance, install, start, etc.)

### Source Code Map (`crates/bubbaloop/src/`)

```
bin/
  bubbaloop.rs              # Entry point: parses CLI args, dispatches to subcommands

lib.rs                      # Library root: protobuf schemas, descriptor utilities, ros-z impls
config.rs                   # Topic configuration and path helpers
templates.rs                # Rust/Python node scaffolding templates

cli/                        # CLI command implementations
  mod.rs                    # Re-exports NodeCommand, DebugCommand, MarketplaceCommand
  doctor.rs                 # System diagnostics: zenoh, daemon, services health checks
  node.rs                   # Node CRUD: init, add, instance, build, start, stop, logs
  marketplace.rs            # Marketplace source management (list, add, remove, enable, disable)
  status.rs                 # Non-interactive status display (table/json/yaml)
  debug.rs                  # Low-level Zenoh debugging (topics, subscribe, query, info)

daemon/                     # Long-running daemon process
  mod.rs                    # Daemon entry point: run() function
  node_manager.rs           # Core logic: node lifecycle, build queue, health monitoring
  registry.rs               # Node registry: persists node.yaml manifests to ~/.bubbaloop/
  systemd.rs                # systemd integration: install/uninstall/start/stop via D-Bus (zbus)
  zenoh_api.rs              # Zenoh queryable handlers: /api/health, /api/nodes, /api/command
  zenoh_service.rs          # Zenoh service: pub/sub for state broadcasting

tui/                        # ratatui terminal UI
  mod.rs                    # TUI entry point: run() function
  app.rs                    # App state machine: handles keys, ticks, daemon client
  daemon/
    mod.rs                  # Daemon client module
    client.rs               # Zenoh-based client for querying daemon API
  config/
    mod.rs                  # Config module
    registry.rs             # TUI config registry
  ui/
    mod.rs                  # UI rendering dispatch
    home.rs                 # Home screen with status overview
    services.rs             # Systemd services management view
    nodes/
      mod.rs                # Nodes view module
      list.rs               # Node list: installed, discover, marketplace tabs
      detail.rs             # Node detail panel
      logs.rs               # Live log viewer (journalctl)
    components/
      mod.rs                # Shared UI components
      spinner.rs            # Animated flower spinner
```

### Workspace Crates

| Crate | Binary | Description |
|-------|--------|-------------|
| `crates/bubbaloop` | `bubbaloop` | Main binary: CLI + TUI + daemon. Depends on zenoh, ratatui, zbus, prost |

### Standalone Crates (not in workspace)

| Crate | Binary | Description |
|-------|--------|-------------|
| `crates/bubbaloop-schemas` | (library) | Protobuf schemas + utilities. Features: `ros-z`, `descriptor`, `config` |

Official nodes live in the [bubbaloop-nodes-official](https://github.com/kornia/bubbaloop-nodes-official) repo:

| Node | Binary | Description |
|------|--------|-------------|
| `rtsp-camera` | `cameras_node` | RTSP camera capture via GStreamer, publishes H264 frames |
| `openmeteo` | `openmeteo_node` | Weather data from Open-Meteo API, publishes forecasts |
| `inference` | `inference_node` | ML inference node |
| `system-telemetry` | `system_telemetry_node` | System metrics (CPU, memory, disk, network, load) |
| `network-monitor` | (python) | Network connectivity monitor (HTTP, DNS, ping) |

Nodes are self-contained with their own protos. Install from marketplace with:
```bash
bubbaloop node install rtsp-camera  # Install from marketplace
bubbaloop node add kornia/bubbaloop-nodes-official --subdir rtsp-camera  # Or from GitHub
```

### Protobuf Schema Workflow

All proto source files live in `crates/bubbaloop-schemas/protos/` (single source of truth):

| Proto File | Module | Key Types |
|------------|--------|-----------|
| `header.proto` | `schemas::header::v1` | `Header` (timestamp, frame_id, seq) |
| `camera.proto` | `schemas::camera::v1` | `CompressedImage`, `RawImage` |
| `weather.proto` | `schemas::weather::v1` | `CurrentWeather`, `HourlyForecast`, `DailyForecast` |
| `daemon.proto` | `schemas::daemon::v1` | `NodeState`, `NodeStatus`, `HealthStatus`, `NodeCommand` |
| `machine.proto` | `schemas::machine::v1` | `MachineInfo`, `MachineList`, `MachineHeartbeat` |
| `system_telemetry.proto` | `schemas::system_telemetry::v1` | `SystemMetrics`, `CpuMetrics`, etc. |
| `network_monitor.proto` | `schemas::network_monitor::v1` | `NetworkStatus`, `HealthCheck`, etc. |

Compilation: `prost-build` compiles `.proto` files at build time via `build.rs`. Generated Rust code goes
to `OUT_DIR` and is included via `include!()` macros in `lib.rs`. All types derive `serde::Serialize`
and `serde::Deserialize`. A `descriptor.bin` FileDescriptorSet is also generated for runtime schema access.

The `bubbaloop-schemas` crate (`crates/bubbaloop-schemas/`) is a standalone crate (not in the workspace).
Both the main `bubbaloop` crate and external nodes compile protos from the same source directory.
External nodes depend on `bubbaloop-schemas` via git (not the full `bubbaloop` crate).

### Zenoh Topic Conventions

```
bubbaloop/daemon/api/health          # Daemon health queryable (query/reply)
bubbaloop/daemon/api/nodes           # Node list queryable (query/reply)
bubbaloop/daemon/api/command         # Node command queryable (query/reply)
bubbaloop/daemon/nodes               # Node state pub/sub (broadcast)
bubbaloop/nodes/{name}/health        # Node heartbeat (pub/sub)

/camera/{name}/compressed            # H264 compressed frames (pub/sub)
/weather/current                     # Current weather conditions (pub/sub)
/weather/hourly                      # Hourly forecast (pub/sub)
/weather/daily                       # Daily forecast (pub/sub)
```

Communication patterns:
- **Query/Reply** for one-time requests (health checks, node commands, node list)
- **Pub/Sub** for continuous streams (video frames, weather, node state changes)

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
# === Orchestration ===
pixi run up                  # Launch all services via process-compose

# === Build ===
pixi run build               # cargo build --release (all crates)

# === Run Individual Services ===
pixi run daemon              # Run daemon (bubbaloop daemon)
pixi run tui                 # Run TUI (bubbaloop tui)
pixi run dashboard           # Run React dashboard dev server
pixi run cameras             # Run RTSP camera node
pixi run openmeteo           # Run weather node
pixi run inference           # Run inference node

# === Development ===
pixi run check               # cargo check
pixi run test                # cargo test
pixi run fmt                 # cargo fmt --all
pixi run clippy              # cargo clippy --all-targets --all-features -- -D warnings
pixi run lint                # fmt-check + clippy combined
pixi run pre-commit-run      # Run all pre-commit hooks

# === Documentation ===
pixi run docs                # Serve docs locally (mkdocs serve)
pixi run docs-build          # Build static docs site

# === Dashboard ===
pixi run dashboard-install   # npm install
pixi run dashboard-proto     # Generate protobuf TypeScript bindings
pixi run dashboard-build     # Production build

# === Zenoh ===
pixi run bridge              # Build and run zenoh-bridge-remote-api
pixi run zenohd-client       # Run zenohd with client config (~/.bubbaloop/zenoh.cli.json5)
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

Each node is a standalone crate (in its own repo or directory) with:

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

### Self-Contained Node Proto Pattern

Nodes are fully self-contained with their own protobuf definitions. This ensures:
- No external proto dependencies at build time
- Vendors control their own message schemas
- Nodes work independently without needing `bubbaloop-schemas` crate

**Important**: Always use the official bubbaloop `Header` schema for message headers. Copy it from
`crates/bubbaloop-schemas/protos/header.proto` to your node's `protos/` directory.

#### Python Node Directory Structure (with protos)

```
{name}/
├── node.yaml            # Node manifest with build command
├── pixi.toml            # Dependencies (grpcio-tools, protobuf)
├── build_proto.py       # Compiles protos to src/
├── main.py              # Node entry point
├── config.yaml          # Runtime configuration
├── protos/
│   ├── header.proto     # Official bubbaloop Header (copy from bubbaloop-schemas)
│   └── {name}.proto     # Node-specific messages (imports header.proto)
└── src/
    ├── __init__.py      # Generated by build_proto.py
    ├── header_pb2.py    # Generated from header.proto
    └── {name}_pb2.py    # Generated from {name}.proto
```

#### Step 1: Copy Official Header Proto

```bash
cp /path/to/bubbaloop/crates/bubbaloop-schemas/protos/header.proto protos/
```

The official Header schema (`bubbaloop.header.v1.Header`) contains:
- `acq_time` / `pub_time`: Timestamps in nanoseconds
- `sequence`: Message sequence number
- `frame_id`: Source identifier
- `machine_id`: Hostname
- `scope`: Deployment scope (from `BUBBALOOP_SCOPE` env var)

#### Step 2: Create Node Proto (importing Header)

```protobuf
// protos/my_node.proto
syntax = "proto3";
package mynode.v1;

import "header.proto";

message MyNodeData {
  bubbaloop.header.v1.Header header = 1;
  // Your fields here
  float value = 2;
}
```

#### Step 3: node.yaml with Build Command

```yaml
name: my-node
version: "0.1.0"
type: python
description: My custom node with protobuf
build: "pixi run build"
command: "pixi run run"
```

#### Step 4: pixi.toml with Proto Dependencies

```toml
[workspace]
name = "my-node"
version = "0.1.0"
channels = ["conda-forge"]
platforms = ["linux-64", "linux-aarch64"]

[tasks]
build = "python build_proto.py"
run = "python main.py -c config.yaml"

[dependencies]
python = ">=3.10"
pyyaml = ">=6.0"

[pypi-dependencies]
eclipse-zenoh = ">=1.7.0"
grpcio-tools = ">=1.60.0"
protobuf = ">=4.25.0"
```

#### Step 5: build_proto.py Script

```python
#!/usr/bin/env python3
"""Build protobuf Python bindings from local protos/ directory."""
import subprocess
import sys
from pathlib import Path

def main():
    script_dir = Path(__file__).parent.resolve()
    proto_dir = script_dir / "protos"
    out_dir = script_dir / "src"
    out_dir.mkdir(parents=True, exist_ok=True)
    (out_dir / "__init__.py").write_text("# Generated protobuf modules\n")

    for proto_file in proto_dir.glob("*.proto"):
        print(f"Compiling {proto_file.name}...")
        result = subprocess.run([
            sys.executable, "-m", "grpc_tools.protoc",
            f"-I{proto_dir}", f"--python_out={out_dir}", str(proto_file)
        ], capture_output=True, text=True)
        if result.returncode != 0:
            print(f"Error: {result.stderr}")
            return result.returncode
    print(f"Generated Python bindings in {out_dir}")
    return 0

if __name__ == "__main__":
    sys.exit(main())
```

#### Step 6: Import in main.py

```python
import sys
from pathlib import Path
sys.path.insert(0, str(Path(__file__).parent / "src"))

from header_pb2 import Header
from my_node_pb2 import MyNodeData
```

#### Rust Nodes

Use `prost-build` in `build.rs` to compile from local `./protos/`:

```rust
// build.rs
fn main() {
    prost_build::Config::new()
        .type_attribute(".", "#[derive(serde::Serialize, serde::Deserialize)]")
        .compile_protos(&["protos/my_node.proto", "protos/header.proto"], &["protos/"])
        .expect("Failed to compile protos");
}
```

#### Full Node Lifecycle

```bash
# 1. Initialize (creates scaffold)
bubbaloop node init my-node --node-type python -d "My node"

# 2. Add protos (copy header.proto, create your proto)
cp /path/to/bubbaloop/crates/bubbaloop-schemas/protos/header.proto my-node/protos/

# 3. Add build_proto.py and update pixi.toml (see above)

# 4. Register with daemon
bubbaloop node add /path/to/my-node

# 5. Build (compiles protos)
bubbaloop node build my-node
# Or directly: cd my-node && pixi run build

# 6. Install and start
bubbaloop node install my-node
bubbaloop node start my-node

# 7. Verify
bubbaloop node logs my-node
bubbaloop debug subscribe "my-node/output"

# 8. Cleanup
bubbaloop node stop my-node
bubbaloop node uninstall my-node
bubbaloop node remove my-node
```

#### Proto Guidelines

- **Always use official Header**: Copy `header.proto` from bubbaloop-schemas for interoperability
- **Version your packages**: Use `package mynode.v1;` for future compatibility
- **One proto file per node**: Keep it simple with `{node_name}.proto` + `header.proto`
- **Zenoh topic convention**: Publish to `{node-name}/data` or similar namespaced topics

### Building a node

```bash
cd my-node
cargo build --release
```

## Creating a New Node

### Quick Start (Rust)

```bash
# Initialize a new Rust node (creates in current directory)
bubbaloop node init my-sensor --node-type rust -d "My custom sensor"
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
bubbaloop node init my-sensor --node-type python -d "My custom sensor"
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

### Multi-Instance Nodes

Generic nodes like `rtsp-camera` can run multiple instances with different configurations. Each instance:
- Shares the same binary/code (base node)
- Has its own config file in `~/.bubbaloop/configs/`
- Runs as a separate systemd service

#### Creating Instances with `node instance`

The `bubbaloop node instance` command creates instances from a registered base node:

```bash
# 1. Add the base node first (only once)
bubbaloop node add ~/.bubbaloop/nodes/bubbaloop-nodes-official/rtsp-camera

# 2. Create instance with config file
bubbaloop node instance rtsp-camera terrace --config ~/.bubbaloop/configs/rtsp-camera-terrace.yaml

# Or copy example config from base node and edit later
bubbaloop node instance rtsp-camera garden --copy-config

# Or create, install, and start in one command
bubbaloop node instance rtsp-camera entrance --config config.yaml --install --start
```

**Command syntax**: `bubbaloop node instance <base-node> <suffix> [options]`

| Option | Description |
|--------|-------------|
| `-c, --config <path>` | Config file for this instance (required for most nodes) |
| `--copy-config` | Copy example from base node's `configs/` directory |
| `--install` | Install as systemd service after creating |
| `--start` | Start after creating (implies `--install`) |

**Instance naming**: The suffix is appended to the base node name:
- Base: `rtsp-camera`, Suffix: `terrace` → Instance: `rtsp-camera-terrace`

#### Listing Instances

```bash
bubbaloop node list              # Shows all nodes with BASE column
bubbaloop node list --base       # Show only base nodes (no instances)
bubbaloop node list --instances  # Show only instances
```

Output shows the relationship:
```
NAME                 STATUS     BASE             TYPE   BUILT
rtsp-camera          not-installed -            rust   yes
rtsp-camera-terrace  running    rtsp-camera     rust   yes
rtsp-camera-entrance running    rtsp-camera     rust   yes
```

#### Config File Location

Instance configs are stored in `~/.bubbaloop/configs/`:
```
~/.bubbaloop/configs/
├── rtsp-camera-terrace.yaml
├── rtsp-camera-entrance.yaml
└── rtsp-camera-garden.yaml
```

The config path is passed to the binary via `-c <path>` when the service starts.

#### Base Node Example Config

Base nodes should include example configs in their `configs/` directory:
```
rtsp-camera/
├── node.yaml
├── configs/
│   ├── terrace.yaml      # Example config for terrace camera
│   └── entrance.yaml     # Example config for entrance camera
└── ...
```

These are used by `--copy-config` to bootstrap new instances.

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
BUBBALOOP_ZENOH_ENDPOINT=tcp/127.0.0.1:7447 bubbaloop daemon
```

## Key Dependencies

| Crate | Version | Purpose |
|-------|---------|---------|
| `zenoh` | 1.7 | Pub/sub messaging, query/reply |
| `ros-z` | git main | ROS 2 compatibility layer over Zenoh |
| `prost` / `prost-build` | 0.14 | Protobuf serialization and code generation |
| `ratatui` | 0.29 | Terminal UI framework |
| `crossterm` | 0.28 | Terminal input/output backend |
| `zbus` | — | D-Bus client for systemd integration |
| `tokio` | 1.0 (full) | Async runtime |
| `gstreamer` | 0.24 | H264 camera capture and video processing |
| `argh` | 0.1 | CLI argument parsing |
| `reqwest` | 0.12 | HTTP client (weather API) |
| `zenoh-ts` | — | TypeScript Zenoh client (dashboard) |

## Testing

```bash
pixi run test                    # Run all tests
cargo test -p bubbaloop          # Test main crate only
cargo test -p rtsp_camera        # Test specific node
```

Tests are co-located with source code using `#[cfg(test)] mod tests` blocks. Key test areas:
- `crates/bubbaloop/src/cli/node.rs` — argument injection prevention, node name validation, git clone safety
- `crates/bubbaloop/src/daemon/node_manager.rs` — build command validation, health extraction
- `crates/bubbaloop/src/tui/ui/nodes/list.rs` — path truncation logic

## Troubleshooting

Quick diagnostics: `bubbaloop doctor --fix` (auto-fixes common issues).

Common issues:
- **TUI "Daemon: disconnected"**: Check `ps aux | grep "bubbaloop daemon"` for duplicates. Kill extras, restart: `systemctl --user restart bubbaloop-daemon`
- **Zenoh timeout**: Ensure zenohd is running: `pgrep zenohd || zenohd &`
- **Binary mismatch**: `cp target/release/bubbaloop ~/.bubbaloop/bin/ && systemctl --user restart bubbaloop-daemon`
- **TUI crash in Claude Code**: TUI requires interactive TTY. Use `bubbaloop status` for non-interactive checks

See [docs/troubleshooting.md](docs/troubleshooting.md) for comprehensive guide.

## Daemon Internals

The daemon (`bubbaloop daemon`) manages node lifecycle via systemd D-Bus (zbus):

- **State updates**: D-Bus signals for <100ms latency (JobRemoved, UnitNew, UnitRemoved). 30s polling as backup
- **Build queue**: One build per node, 10-minute timeout, `kill_on_drop(true)`
- **Health monitoring**: Nodes publish heartbeats to `bubbaloop/nodes/{name}/health`. Marked UNHEALTHY after 30s silence
- **Security hardening**: Generated systemd units include `NoNewPrivileges=true`, `ProtectSystem=strict`, etc.
- **Build validation**: Allowlisted prefixes (`cargo`, `pixi`, `npm`, `make`, `python`, `pip`), rejects shell metacharacters and newlines

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

## Node System Architecture (For Agents)

This section documents the internal architecture of the node management system for future agents working on this codebase.

### Node Types and Roles

| Role | Description | Can Install/Start | Example |
|------|-------------|-------------------|---------|
| **Standalone** | Single-purpose node, runs one instance | Yes | `openmeteo`, `network-monitor` |
| **Base (Template)** | Multi-instance node, provides code but needs config | Yes (but usually shouldn't) | `rtsp-camera` |
| **Instance** | Configured instance of a base node | Yes | `rtsp-camera-terrace` |

### Key Data Structures

**Registry Entry** (`daemon/registry.rs:NodeEntry`):
```rust
struct NodeEntry {
    path: String,              // Path to node directory
    added_at: String,          // Timestamp
    name_override: Option<String>,   // Instance name (e.g., "rtsp-camera-terrace")
    config_override: Option<String>, // Config path for this instance
}
```

**Cached Node** (`daemon/node_manager.rs:CachedNode`):
```rust
struct CachedNode {
    path: String,
    manifest: Option<NodeManifest>,
    status: NodeStatus,
    name_override: Option<String>,
    config_override: Option<String>,
    // ... health, build state, etc.
}
```

### Instance Creation Flow

```
1. User runs: bubbaloop node instance rtsp-camera terrace --config path/to/config.yaml

2. CLI (node.rs:create_instance):
   - Validates suffix (alphanumeric, hyphens, underscores only)
   - Constructs instance name: "rtsp-camera-terrace"
   - Queries daemon for base node path
   - Sends "add" command with name_override and config_override

3. Daemon (node_manager.rs):
   - Registers in nodes.json with name_override="rtsp-camera-terrace"
   - Creates CachedNode with overrides

4. Install/Start:
   - Systemd service: bubbaloop-rtsp-camera-terrace.service
   - Command includes: -c /path/to/config.yaml
```

### File Locations

| Type | Location |
|------|----------|
| Registry | `~/.bubbaloop/nodes.json` |
| Instance configs | `~/.bubbaloop/configs/{instance-name}.yaml` |
| Systemd services | `~/.config/systemd/user/bubbaloop-{name}.service` |
| Node code | `~/.bubbaloop/nodes/{repo-name}/{node-name}/` |

### Code Locations

| Feature | File | Key Functions |
|---------|------|---------------|
| Instance CLI | `cli/node.rs` | `create_instance()`, `InstanceArgs` |
| Registry | `daemon/registry.rs` | `register_node()`, `effective_name()` |
| Node Manager | `daemon/node_manager.rs` | `CachedNode`, `to_proto()` |
| Systemd | `daemon/systemd.rs` | `install_service()`, `start_service()` |

### Testing Multi-Instance Nodes

```bash
# Full cycle test
bubbaloop node add ~/.bubbaloop/nodes/bubbaloop-nodes-official/rtsp-camera
bubbaloop node instance rtsp-camera test1 --copy-config
bubbaloop node instance rtsp-camera test2 --copy-config
bubbaloop node list --instances   # Should show test1, test2
bubbaloop node install rtsp-camera-test1
bubbaloop node start rtsp-camera-test1
bubbaloop node logs rtsp-camera-test1
bubbaloop node stop rtsp-camera-test1
bubbaloop node remove rtsp-camera-test1
bubbaloop node remove rtsp-camera-test2
```

### Proto Pattern for Nodes

Nodes should be self-contained with their own protos:

```
my-node/
├── node.yaml         # build: "pixi run build"
├── pixi.toml         # [workspace], grpcio-tools, protobuf
├── build_proto.py    # Compiles protos/*.proto → src/*_pb2.py
├── main.py           # from header_pb2 import Header
├── protos/
│   ├── header.proto  # Copy from bubbaloop-schemas (official Header)
│   └── my_node.proto # import "header.proto"
└── src/
    ├── header_pb2.py # Generated
    └── my_node_pb2.py
```

Always use the official `bubbaloop.header.v1.Header` for message headers (copy `header.proto` from `crates/bubbaloop-schemas/protos/`).

## Agent Guidelines

### Coding Style

- **Rust edition**: 2021, async/await with tokio
- **Error handling**: `thiserror` for library errors, `anyhow` for application-level
- **Naming**: snake_case for files/functions, CamelCase for types, SCREAMING_SNAKE for constants
- **File size target**: ~500 LOC per module. Split if larger
- **Imports**: group by std, external crates, internal modules
- **Tests**: co-located `#[cfg(test)] mod tests` blocks, not separate files (except integration tests)
- **Protobuf types**: versioned modules (`schemas::camera::v1::CompressedImage`), re-exported at `schemas::` level
- **Async**: all Zenoh and systemd operations are async. Use `tokio::spawn` for background tasks
- **Logging**: use `log::info!`, `log::warn!`, `log::debug!`, `log::error!` instead of `println!`/`eprintln!` for operational messages. Reserve `println!` for CLI user-facing output only (e.g., command results, tables). The daemon and TUI should exclusively use `log::` macros

### IMPORTANT: Do and Don't

**DO:**
- Run `pixi run fmt` and `pixi run clippy` before any commit
- Run `pixi run check` after modifying Rust code to catch compile errors early
- Validate build command inputs (allowlisted prefixes, reject shell metacharacters)
- Validate node names (alphanumeric, hyphens, underscores only; no path traversal)
- Use `--` separator in git clone commands to prevent argument injection
- Add tests for security-sensitive code paths
- Keep this CLAUDE.md updated when architecture changes
- Update `.gitignore` when adding new generated/build file patterns
- **Include tests with every PR** — any new feature, bug fix, or refactor must include relevant unit tests. Tests ensure future agents can verify correctness and detect regressions. Use `tempfile` for filesystem tests, co-located `#[cfg(test)] mod tests` blocks
- When adding proto schema changes, verify both `bubbaloop-schemas` and the main `bubbaloop` crate compile (both compile from `crates/bubbaloop-schemas/protos/`)

**DON'T:**
- Don't run `bubbaloop tui` from Claude Code — it requires an interactive TTY
- Don't use `bubbaloop-daemon` as a binary name — it's now `bubbaloop daemon` (subcommand)
- Don't edit files in `target/` or `OUT_DIR` — they are generated
- Don't commit `.env`, credentials, or `target/` directories
- Don't pass unsanitized user input to `std::process::Command` without validation
- Don't add `crates/bubbaloop-schemas/` to the workspace — it's intentionally standalone
- Don't run `git push --force` to main
- Don't combine `git` and `path` in Cargo dependency specs — Cargo rejects this. Use `git` only (Cargo discovers crates by scanning the repo) or `path` only (local dev). For `bubbaloop-schemas` in external nodes: `{ git = "https://github.com/kornia/bubbaloop.git", branch = "main" }`. For local dev nodes: `{ path = "../../bubbaloop-schemas" }`. Pin to a tag for stability: `{ git = "...", tag = "schemas-v0.1.0" }`

### Commit Style

Conventional commits: `feat:`, `fix:`, `chore:`, `docs:`, `refactor:`, `test:`

### Git Hygiene

**Never commit:** `target/`, `node_modules/`, `dist/`, `.pixi/`, `*.pb.js`, `*.pb.d.ts`, `*.pyc`, `*.so`

**Always commit:** `Cargo.lock`, `pixi.lock`, `package-lock.json`

**Pre-commit:** `pixi run fmt && pixi run clippy`

### Verification Workflow

After making code changes, verify in this order:

1. `pixi run check` — fast compile check
2. `pixi run clippy` — lint (must pass with zero warnings, `-D warnings` is enforced)
3. `pixi run test` — run tests
4. `pixi run fmt` — format check
