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

### 4. Build zenoh-bridge-remote-api (one-time)

The dashboard connects to Zenoh via WebSocket. You need the bridge:

```bash
# Clone zenoh-ts repository
git clone https://github.com/eclipse-zenoh/zenoh-ts.git
cd zenoh-ts/zenoh-bridge-remote-api

# Build the bridge
cargo build --release
```

!!! tip "Save the binary path"
    Note the path to `./target/release/zenoh-bridge-remote-api` for later use.

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

### Terminal 1: Start zenoh-bridge-remote-api

```bash
./path/to/zenoh-bridge-remote-api --listen tcp/0.0.0.0:7448 --ws-port 10000
```

You should see:

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

Open http://localhost:5173 in your browser.

### Connect to Zenoh

1. The default endpoint `ws://127.0.0.1:10000` should be pre-filled
2. Click **Connect**
3. The status should change to "Connected"

### View Camera Streams

Your camera streams will automatically appear. If not:

1. Click **Add Camera**
2. Click the edit (✏️) icon
3. Select a topic from the dropdown or enter manually:
   - `0/camera%entrance%compressed/**`
   - `0/camera%backyard%compressed/**`
4. Click **Save**

### Dashboard Features

| Action | How To |
|--------|--------|
| Add Camera | Click "Add Camera" button |
| Edit Camera | Click pencil (✏️) icon |
| Show Metadata | Click info (ⓘ) icon |
| Remove Camera | Click X icon |
| Reorder | Drag by grip handle |
| Maximize | Click expand icon |

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

- Ensure `zenoh-bridge-remote-api` is running in Terminal 1
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
- [Visualization](visualization.md) — Alternative visualization options
