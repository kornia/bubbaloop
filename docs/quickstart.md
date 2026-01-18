# 🚀 Quickstart

Get started with Bubbaloop in minutes.

## Prerequisites

- Linux (tested on Ubuntu 22.04, Jetson)
- RTSP cameras on your network
- [Pixi](https://pixi.sh) package manager
- Modern browser (Chrome 94+, Edge 94+, or Safari 16.4+)

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

This installs all required dependencies including:

- Rust toolchain
- GStreamer and plugins
- Node.js (for dashboard)
- Build tools (pkg-config, cmake)

### 4. Build zenoh-bridge-remote-api (automatic)

The bridge is built automatically when you run `pixi run bridge` for the first time.

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

### Remote access (TUI from laptop to robot)

For accessing cameras from another machine:

```bash
# On server (robot):
zenohd -c zenoh.json5           # Terminal 1: Zenoh router
pixi run cameras-server         # Terminal 2: Cameras → router

# On client (laptop):
pixi run bubbaloop              # Terminal 1: TUI (run /server to set IP)
pixi run zenohd-client          # Terminal 2: Local router (uses TUI config)
```

In the TUI, use `/server` to configure the robot's IP once. This generates the zenohd config automatically.

See [Configuration](configuration.md#remote-access-setup) for detailed setup.

## Visualization

**Local:** http://localhost:5173
**Remote:** https://\<your-ip\>:5173 (accept self-signed cert)

### Connection

The dashboard auto-connects via built-in proxy. Check the header for status:

- 🟢 **Connected** — ready to stream
- 🟡 **Connecting** — establishing connection
- 🔴 **Error** — click ↻ to retry

### View Camera Streams

Cameras appear automatically. To add manually:

1. Click **Add Camera**
2. Click ✏️ to edit
3. Select topic or enter: `0/camera%entrance%compressed/**`
4. Click **Save**

### Dashboard Features

| Action | How |
|--------|-----|
| Add Camera | "Add Camera" button |
| Edit | ✏️ icon |
| Metadata | ⓘ icon (shows latency, timestamps) |
| Remove | ✕ icon |
| Reorder | Drag grip handle |
| Maximize | Expand icon |

### Live Stats

Each camera displays: **FPS** · **frame count** · **resolution** · **LIVE/INIT** status

## Stopping

Press `Ctrl+C` to gracefully shutdown (stops all services if using `pixi run up`).

## Browser Requirements

| Browser | Minimum Version | Status |
|---------|-----------------|--------|
| Chrome  | 94+ | ✅ Recommended |
| Edge    | 94+ | ✅ Supported |
| Safari  | 16.4+ | ✅ Supported |
| Firefox | - | ❌ Not supported |

!!! warning "Firefox not supported"
    Firefox does not support the WebCodecs API required for H264 decoding.

## Troubleshooting

### "WebSocket disconnected"

- Ensure `pixi run bridge` is running in Terminal 1
- Check it's listening on port 10000

### "Waiting for keyframe"

- Verify `pixi run cameras` is running in Terminal 2
- Check the camera URL is correct in `config.yaml`
- Look at Terminal 2 for error messages

### "WebCodecs not supported"

- Use Chrome 94+, Edge 94+, or Safari 16.4+
- Access via `localhost` (secure context required)

## Next Steps

- [Configuration](configuration.md) — Detailed camera configuration options
- [Architecture](architecture.md) — Understand the system design
- [Visualization](visualization.md) — Dashboard details and troubleshooting
