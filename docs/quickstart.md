# 🚀 Quickstart

Get started with Bubbaloop in minutes.

## Prerequisites

- Linux (tested on Ubuntu 22.04, Jetson)
- RTSP cameras on your network
- [Pixi](https://pixi.sh) package manager

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
- Build tools (pkg-config, cmake)

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

Start the multi-camera streaming server:

```bash
pixi run multicam
```

You should see output like:

```
[INFO  multicam] Loaded configuration with 2 cameras
[INFO  zenoh::net::runtime] Using ZID: a7e256ba23b34292b71b9827b3e16bfc
[INFO  multicam] Starting camera 'entrance' from rtsp://...
[INFO  multicam] Starting camera 'backyard' from rtsp://...
[INFO  foxglove::websocket::server] Started server on 0.0.0.0:8765
```

## Visualization

1. Open [Foxglove Studio](https://foxglove.dev/studio)
2. Click **Open connection**
3. Select **Foxglove WebSocket**
4. Enter: `ws://<your-host-ip>:8765`
5. Click **Open**

Your camera streams will appear as CompressedVideo topics:

- `/camera/entrance/compressed`
- `/camera/backyard/compressed`

## Stopping

Press `Ctrl+C` to gracefully shutdown all camera nodes.

## Next Steps

- [Configuration](configuration.md) — Detailed camera configuration options
- [Architecture](architecture.md) — Understand the system design
- [Visualization](visualization.md) — Advanced Foxglove setup

