# bubbaloop-daemon

Central service for managing bubbaloop nodes via Zenoh and HTTP.

## Overview

The daemon provides a unified interface for node management, eliminating the need for direct `systemctl` calls from the TUI or dashboard. It maintains authoritative node state in memory and communicates with systemd via D-Bus for better performance and reliability.

## Features

- **Native D-Bus Communication** - Direct systemd integration without shell spawning
- **State Caching** - Maintains node state in memory with periodic refresh
- **Dual Interface**:
  - Zenoh pub/sub for dashboard and MCP integration
  - HTTP REST API for TUI and simple clients
- **Background Builds** - Asynchronous build/clean operations with streaming output
- **Graceful Degradation** - TUI falls back to limited local mode if daemon unavailable

## Architecture

```
┌─────────────────┐     ┌─────────────────┐     ┌─────────────────┐
│   bubbaloop-tui │     │    dashboard    │     │   future MCP    │
└────────┬────────┘     └────────┬────────┘     └────────┬────────┘
         │ HTTP                  │ Zenoh                 │ Zenoh
         └───────────────────────┼───────────────────────┘
                                 │
                    ┌────────────┴────────────┐
                    │    bubbaloop-daemon     │
                    │  ┌───────────────────┐  │
                    │  │   NodeManager     │  │
                    │  │  - state cache    │  │
                    │  │  - build runner   │  │
                    │  └─────────┬─────────┘  │
                    │            │ D-Bus      │
                    │  ┌─────────┴─────────┐  │
                    │  │  systemd user     │  │
                    │  │  services         │  │
                    └──┴───────────────────┴──┘
```

## Usage

```bash
# Run with default settings (HTTP on 8088, Zenoh enabled)
cargo run -p bubbaloop-daemon --release

# Custom HTTP port
cargo run -p bubbaloop-daemon --release -- -p 9000

# HTTP-only mode (no Zenoh)
cargo run -p bubbaloop-daemon --release -- --http-only

# Connect to specific Zenoh endpoint
cargo run -p bubbaloop-daemon --release -- -z tcp/192.168.1.100:7447
```

### CLI Options

| Option | Short | Default | Description |
|--------|-------|---------|-------------|
| `--http-port` | `-p` | 8088 | HTTP server port |
| `--zenoh-endpoint` | `-z` | (auto) | Zenoh endpoint to connect to |
| `--http-only` | | false | Disable Zenoh, run HTTP only |

## HTTP API

### Endpoints

| Method | Path | Description |
|--------|------|-------------|
| GET | `/health` | Health check |
| GET | `/nodes` | List all nodes |
| GET | `/nodes/:name` | Get single node state |
| POST | `/nodes/:name/command` | Execute command on node |
| POST | `/nodes/add` | Add a new node |
| POST | `/refresh` | Refresh all node states |

### Examples

```bash
# List all nodes
curl http://localhost:8088/nodes

# Get single node
curl http://localhost:8088/nodes/cameras

# Start a node
curl -X POST http://localhost:8088/nodes/cameras/command \
  -H "Content-Type: application/json" \
  -d '{"command": "start"}'

# Stop a node
curl -X POST http://localhost:8088/nodes/cameras/command \
  -H "Content-Type: application/json" \
  -d '{"command": "stop"}'

# Build a node
curl -X POST http://localhost:8088/nodes/cameras/command \
  -H "Content-Type: application/json" \
  -d '{"command": "build"}'

# Install systemd service
curl -X POST http://localhost:8088/nodes/cameras/command \
  -H "Content-Type: application/json" \
  -d '{"command": "install"}'

# Add a new node
curl -X POST http://localhost:8088/nodes/add \
  -H "Content-Type: application/json" \
  -d '{"node_path": "/home/user/my-node"}'
```

### Command Types

| Command | Description |
|---------|-------------|
| `start` | Start the node's systemd service |
| `stop` | Stop the node's systemd service |
| `restart` | Restart the node's systemd service |
| `install` | Install systemd service unit |
| `uninstall` | Remove systemd service unit |
| `build` | Run the node's build command |
| `clean` | Run `pixi run clean` |
| `enable_autostart` | Enable service autostart |
| `disable_autostart` | Disable service autostart |
| `add` | Register a new node |
| `remove` | Unregister a node |
| `refresh` | Refresh all node states |

### Response Format

```json
{
  "success": true,
  "message": "Started cameras",
  "output": "",
  "node_state": {
    "name": "cameras",
    "path": "/home/user/bubbaloop/crates/bubbaloop-nodes/rtsp-camera",
    "status": "running",
    "installed": true,
    "autostart_enabled": false,
    "version": "0.1.0",
    "description": "RTSP camera capture node",
    "node_type": "rust",
    "is_built": true,
    "build_output": []
  }
}
```

## Zenoh Topics

| Key Expression | Type | Description |
|----------------|------|-------------|
| `bubbaloop/daemon/nodes` | Publisher | Full NodeList (periodic + on change) |
| `bubbaloop/daemon/nodes/{name}/state` | Publisher | Individual node state updates |
| `bubbaloop/daemon/command` | Queryable | Command request/response |
| `bubbaloop/daemon/events` | Publisher | Node state change events |

### Zenoh Client Example (Rust)

```rust
use zenoh::prelude::*;
use prost::Message;

// Subscribe to node list
let subscriber = session.declare_subscriber("bubbaloop/daemon/nodes").await?;
while let Ok(sample) = subscriber.recv_async().await {
    let list = NodeList::decode(sample.payload().to_bytes())?;
    println!("Nodes: {:?}", list.nodes);
}

// Send command via query
let cmd = NodeCommand {
    command: CommandType::Start as i32,
    node_name: "cameras".to_string(),
    ..Default::default()
};
let replies = session.get("bubbaloop/daemon/command")
    .payload(cmd.encode_to_vec())
    .await?;
```

## Running as a Systemd Service

The daemon can manage itself! Add it to your nodes registry:

```bash
# Register the daemon
echo '{"nodes":[{"path":"/path/to/bubbaloop/crates/bubbaloop-daemon","addedAt":"..."}]}' > ~/.bubbaloop/nodes.json
```

Or create a systemd unit manually:

```ini
# ~/.config/systemd/user/bubbaloop-daemon.service
[Unit]
Description=Bubbaloop Daemon
After=network.target

[Service]
Type=simple
ExecStart=/path/to/bubbaloop-daemon
Restart=on-failure
RestartSec=5
Environment=RUST_LOG=info

[Install]
WantedBy=default.target
```

```bash
systemctl --user daemon-reload
systemctl --user enable bubbaloop-daemon
systemctl --user start bubbaloop-daemon
```

## Development

```bash
# Build
cargo build -p bubbaloop-daemon

# Run with debug logging
RUST_LOG=debug cargo run -p bubbaloop-daemon

# Test HTTP API
curl http://localhost:8088/health

# Monitor Zenoh topics (requires zenoh-cli)
z_sub 'bubbaloop/daemon/**'
```

## Protocol Buffers

The daemon uses protobuf for Zenoh message encoding. Schema is at `protos/bubbaloop/daemon.proto`.

Key messages:
- `NodeState` - Complete state of a single node
- `NodeList` - List of all nodes with timestamp
- `NodeCommand` - Command to execute
- `CommandResult` - Result of command execution
- `NodeEvent` - State change notification
