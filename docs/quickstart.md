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

You need **three terminals** to run the complete system:

### Terminal 1: Start zenoh-bridge

```bash
pixi run bridge
```

First run will clone and build the bridge. You should see:

```
[INFO  zenoh_bridge_remote_api] Listening on tcp/0.0.0.0:7448
[INFO  zenoh_bridge_remote_api] WebSocket server listening on port 10000
```

### Terminal 2: Start camera capture

```bash
cd bubbaloop
pixi run multicam
```

You should see:

```
[INFO  multicam] Loaded configuration with 2 cameras
[INFO  zenoh::net::runtime] Using ZID: a7e256ba23b34292b71b9827b3e16bfc
[INFO  multicam] Starting camera 'entrance' from rtsp://...
[INFO  multicam] Starting camera 'backyard' from rtsp://...
```

### Terminal 3: Start dashboard

```bash
cd bubbaloop
pixi run dashboard
```

You should see:

```
> dashboard@0.0.0 dev
> vite

  VITE v5.x.x  ready in xxx ms

  ➜  Local:   http://localhost:5173/
```

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

Press `Ctrl+C` in each terminal to gracefully shutdown.

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

- Verify `pixi run multicam` is running in Terminal 2
- Check the camera URL is correct in `config.yaml`
- Look at Terminal 2 for error messages

### "WebCodecs not supported"

- Use Chrome 94+, Edge 94+, or Safari 16.4+
- Access via `localhost` (secure context required)

## Next Steps

- [Configuration](configuration.md) — Detailed camera configuration options
- [Architecture](architecture.md) — Understand the system design
- [Visualization](visualization.md) — Dashboard details and troubleshooting
