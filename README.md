# Bubbaloop

Multi-camera RTSP streaming with ROS-Z and Foxglove.

Captures H264 streams directly from RTSP cameras (zero decode overhead) and publishes them via Zenoh/ROS-Z for visualization in Foxglove Studio.

## Requirements

- [Pixi](https://pixi.sh) for dependency management

## Quick Start

```bash
# Install dependencies
pixi install

# Edit camera configuration
vim config.yaml

# Run
pixi run multicam
```

## Configuration

Edit `config.yaml` to add your RTSP cameras:

```yaml
cameras:
  - name: "front"
    url: "rtsp://user:pass@192.168.1.10:554/stream"
    latency: 200
  - name: "rear"
    url: "rtsp://192.168.1.11:554/live"
    latency: 100
```

## Visualization

Connect Foxglove Studio to `ws://localhost:8765` to view all camera streams.