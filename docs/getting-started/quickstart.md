# Quickstart

Get started with Bubbaloop in minutes.

## Prerequisites

- Linux (Ubuntu 22.04+, Jetson, Raspberry Pi)
- Node.js 20+ (for TUI)
- Modern browser (Chrome 94+, Edge 94+, Safari 16.4+)
- RTSP cameras on your network (optional for initial testing)

## Installation

### Option 1: Quick Install (Recommended)

```bash
curl -sSL https://github.com/kornia/bubbaloop/releases/latest/download/install.sh | bash
```

### Option 2: Development Install

```bash
git clone https://github.com/kornia/bubbaloop.git
cd bubbaloop
pixi install
```

See [Installation](installation.md) for detailed requirements.

## Running Bubbaloop

### Using the TUI (Terminal UI)

The TUI is the main interface for managing Bubbaloop:

```bash
bubbaloop
```

Or with development install:

```bash
pixi run bubbaloop
```

### TUI Commands

| Key | Action |
|-----|--------|
| `n` | Nodes — View and manage nodes |
| `t` | Topics — Browse Zenoh topics |
| `l` | Logs — View service logs |
| `s` | Settings — Configure options |
| `q` | Quit |

### Starting Services

From the TUI Nodes panel:

1. Select a node (e.g., `rtsp-camera`)
2. Press `s` to start the service
3. Press `l` to view logs

Or start services manually:

```bash
# Start Zenoh router (required)
zenohd &

# Start the daemon (manages nodes)
bubbaloop-daemon &

# Start the TUI
bubbaloop
```

## Configuration

### Camera Configuration

Create `~/.bubbaloop/configs/cameras.yaml`:

```yaml
cameras:
  - name: "entrance"
    url: "rtsp://user:password@192.168.1.100:554/stream1"
    latency: 200

  - name: "backyard"
    url: "rtsp://user:password@192.168.1.101:554/stream1"
    latency: 200
```

### Weather Service

Create `~/.bubbaloop/configs/config.yaml`:

```yaml
weather:
  latitude: 37.7749
  longitude: -122.4194
  interval_secs: 300
```

## Dashboard

The web dashboard provides real-time visualization.

### Starting the Dashboard

With development install:

```bash
pixi run dashboard
```

Access at: http://localhost:5173

### Remote Access

For HTTPS access from other devices:

- URL: `https://<your-ip>:5173`
- Accept the self-signed certificate warning

### Dashboard Features

| Panel | Description |
|-------|-------------|
| Cameras | Live H264 video streams |
| Nodes | Service management |
| Weather | Current conditions and forecasts |
| Raw Data | Browse any Zenoh topic |

## Development Workflow

For contributors building from source:

```bash
# Start everything with process-compose
pixi run up

# Or run services individually:
pixi run bridge      # Zenoh WebSocket bridge
pixi run cameras     # Camera capture
pixi run dashboard   # Web dashboard
pixi run bubbaloop   # Terminal UI
```

## Browser Requirements

| Browser | Minimum Version | Status |
|---------|-----------------|--------|
| Chrome  | 94+ | Recommended |
| Edge    | 94+ | Supported |
| Safari  | 16.4+ | Supported |
| Firefox | - | Not supported |

!!! warning "Firefox not supported"
    Firefox does not support the WebCodecs API required for H264 decoding.

## Next Steps

- [Installation](installation.md) — Detailed installation options
- [Configuration](configuration.md) — Component configuration options
- [Architecture](../concepts/architecture.md) — Understand the system design
- [Dashboard](../dashboard/index.md) — Dashboard features and panels
