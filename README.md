# Bubbaloop

Multi-camera RTSP streaming with ROS-Z and real-time browser visualization.

Captures H264 streams directly from RTSP cameras (zero decode overhead) and publishes them via Zenoh/ROS-Z for visualization in the React Dashboard.

## Requirements

- [Pixi](https://pixi.sh) for dependency management
- [zenoh-bridge-remote-api](https://github.com/eclipse-zenoh/zenoh-ts) for browser connectivity

## Quick Start

### 1. Install dependencies

```bash
pixi install
```

### 2. Start zenoh-bridge-remote-api

The bridge provides WebSocket access for browsers to connect to Zenoh:

```bash
pixi run bridge
```

This will clone, build (first time only), and run the bridge on port 10000.

### 3. Configure cameras

Edit `config.yaml` to add your RTSP cameras:

```yaml
cameras:
  - name: "entrance"
    url: "rtsp://user:pass@192.168.1.10:554/stream"
    latency: 200
  - name: "backyard"
    url: "rtsp://192.168.1.11:554/live"
    latency: 100
```

### 4. Start the camera streams

```bash
pixi run multicam
```

### 5. Start the dashboard

```bash
pixi run dashboard
```

**Local:** http://localhost:5173
**Remote:** https://\<ip\>:5173 (accept self-signed cert)

The dashboard auto-connects, shows live FPS/latency stats, and serves HTTPS with Zenoh WebSocket proxied through the same port.

## Documentation

For detailed documentation, run:

```bash
pixi run docs
```

Or see the [docs/](docs/) directory.
