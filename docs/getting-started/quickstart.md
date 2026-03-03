# Quickstart

Get started with Bubbaloop in minutes.

## Prerequisites

- Linux (Ubuntu 22.04+, Jetson, Raspberry Pi)
- Modern browser (Chrome 94+, Edge 94+, Safari 16.4+) for dashboard

## Step 1: Install Bubbaloop

```bash
curl -sSL https://github.com/kornia/bubbaloop/releases/latest/download/install.sh | bash
source ~/.bashrc
```

This installs Zenoh router, WebSocket bridge, and the bubbaloop daemon as systemd services.

### Verify

```bash
bubbaloop doctor --fix    # Auto-fix any issues
bubbaloop status          # Should show daemon + zenoh running
```

## Step 2: Install a Camera Node

```bash
bubbaloop node install rtsp-camera
```

This downloads the precompiled binary from the marketplace and registers it with the daemon.

## Step 3: Configure and Start

```bash
# Create a config for your camera
mkdir -p ~/.bubbaloop/configs
cat > ~/.bubbaloop/configs/entrance.yaml << 'EOF'
name: entrance
publish_topic: camera/entrance/compressed
url: "rtsp://user:password@192.168.1.100:554/stream1"
latency: 200
decoder: cpu
width: 1280
height: 720
EOF

# Create an instance and start it
bubbaloop node instance rtsp-camera entrance \
  --config ~/.bubbaloop/configs/entrance.yaml --start
```

## Step 4: Verify

```bash
bubbaloop node list                    # Should show rtsp-camera-entrance running
bubbaloop node logs rtsp-camera-entrance -f   # Live logs
bubbaloop debug subscribe "camera/entrance/compressed"  # See frames
```

See [Installation](installation.md) for detailed requirements.

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

## Service Management

Services are managed via systemd:

```bash
# View status
systemctl --user status zenohd
systemctl --user status bubbaloop-daemon

# Restart
systemctl --user restart bubbaloop-daemon

# View logs
journalctl --user -u bubbaloop-daemon -f
```

## Dashboard

The web dashboard provides real-time visualization.

### Starting the Dashboard

With development install:

```bash
pixi run dashboard
```

Access at: http://localhost:5173

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
git clone https://github.com/kornia/bubbaloop.git
cd bubbaloop
pixi install

# Start everything with process-compose
pixi run up

# Or run services individually:
pixi run daemon      # Bubbaloop daemon
pixi run bubbaloop   # Terminal UI
pixi run dashboard   # Web dashboard
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
