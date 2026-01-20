# Quickstart

Get started with Bubbaloop in minutes.

## Prerequisites

- Linux (tested on Ubuntu 22.04, Jetson)
- [Pixi](https://pixi.sh) package manager
- Modern browser (Chrome 94+, Edge 94+, or Safari 16.4+)
- RTSP cameras on your network (optional for initial testing)

## Installation

### 1. Install Pixi

```bash
curl -fsSL https://pixi.sh/install.sh | sh
```

!!! note "Restart your shell"
    After installing Pixi, restart your terminal or run `source ~/.bashrc` to update your PATH.

### 2. Clone the Repository

```bash
git clone https://github.com/kornia/bubbaloop.git
cd bubbaloop
```

### 3. Install Dependencies

```bash
pixi install
```

This installs all required dependencies including Rust, GStreamer, Node.js, and build tools.

See [Installation](installation.md) for detailed requirements.

## Configuration

Create or edit `config.yaml` with your camera settings:

```yaml
cameras:
  - name: "entrance"
    url: "rtsp://user:password@192.168.1.100:554/stream1"
    latency: 200

  - name: "backyard"
    url: "rtsp://user:password@192.168.1.101:554/stream1"
    latency: 200
```

See [Configuration](configuration.md) for detailed options.

## Running

### Start everything (recommended)

```bash
pixi run up
```

This uses [process-compose](https://github.com/F1bonacc1/process-compose) to launch all services:

- **bridge** — Zenoh WebSocket bridge
- **cameras** — RTSP camera capture
- **dashboard** — React dashboard

Press `Ctrl+C` to stop all services.

### Run services individually

If you prefer separate terminals:

```bash
# Terminal 1: Zenoh bridge (WebSocket for dashboard)
pixi run bridge

# Terminal 2: Camera capture
pixi run cameras

# Terminal 3: Web dashboard
pixi run dashboard
```

## Visualization

**Local:** http://localhost:5173
**Remote:** https://\<your-ip\>:5173 (accept self-signed cert)

### Connection

The dashboard auto-connects via built-in proxy. Check the header for status:

- **Connected** — Green dot, ready to stream
- **Connecting** — Yellow pulsing
- **Error** — Red dot, click refresh to retry

### View Camera Streams

Cameras appear automatically. To add manually:

1. Click **Add Camera**
2. Click the edit icon
3. Select topic or enter: `0/camera%entrance%compressed/**`
4. Click **Save**

### Live Stats

Each camera displays: **FPS** · **frame count** · **resolution** · **LIVE/INIT** status

## Stopping

Press `Ctrl+C` to gracefully shutdown all services.

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

- [Installation](installation.md) — Detailed installation requirements
- [Configuration](configuration.md) — Component configuration options
- [Architecture](../concepts/architecture.md) — Understand the system design
- [Dashboard](../dashboard/index.md) — Dashboard features and panels
