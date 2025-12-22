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

### 2. Configure cameras

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

### 3. Start everything

```bash
pixi run up
```

This launches the zenoh bridge, camera streams, and dashboard using [process-compose](https://github.com/F1bonacc1/process-compose).

**Local:** http://localhost:5173
**Remote:** https://\<ip\>:5173 (accept self-signed cert)

### Running services individually

```bash
pixi run bridge      # Start zenoh bridge
pixi run cameras    # Start camera streams
pixi run dashboard   # Start dashboard
```

## Documentation

For detailed documentation, run:

```bash
pixi run docs
```

Or see the [docs/](docs/) directory.
