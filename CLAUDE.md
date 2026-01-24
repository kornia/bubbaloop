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
node_type: rust  # or python
```

### Building a node

```bash
cd crates/bubbaloop-nodes/my-node
pixi run cargo build --release
```

The binary will be at `target/release/my_node`.

## Key Dependencies

- **Zenoh** - Pub/sub messaging (v1.7.x)
- **zenoh-ts** - TypeScript Zenoh client for dashboard
- **ros-z** - ROS 2 compatibility layer
- **prost** - Protobuf serialization
- **GStreamer** - H264 camera capture
- **zbus** - D-Bus client for systemd integration
