# Node Marketplace

The Bubbaloop node ecosystem allows anyone to create, share, and install nodes.

## Installing Nodes

### From GitHub

```bash
# Single-node repository
bubbaloop node add user/my-sensor --build --install

# Multi-node repository (use --subdir)
bubbaloop node add kornia/bubbaloop-nodes-official --subdir rtsp-camera --build --install
```

### From Local Path

```bash
bubbaloop node add /path/to/my-sensor --build --install
```

## Official Nodes

Official nodes are maintained at [kornia/bubbaloop-nodes-official](https://github.com/kornia/bubbaloop-nodes-official).

| Node | Category | Description |
|------|----------|-------------|
| rtsp-camera | camera | RTSP camera capture with hardware H264 decode |
| openmeteo | weather | Open-Meteo weather data publisher |
| foxglove | bridge | Foxglove Studio visualization bridge |
| recorder | recording | MCAP file recorder for ROS-Z topics |
| inference | inference | ML inference for camera processing |

Install any official node:

```bash
bubbaloop node add kornia/bubbaloop-nodes-official --subdir <name> --build --install
```

## Node Registry

Each multi-node repository contains a `nodes.yaml` registry listing available nodes:

```yaml
nodes:
  - name: rtsp-camera
    description: "RTSP camera capture with hardware H264 decode"
    version: "0.1.0"
    type: rust
    category: camera
    tags: [video, rtsp, gstreamer, h264]
    repo: kornia/bubbaloop-nodes-official
    subdir: rtsp-camera
    binary: cameras_node
```

## Categories

| Category | Description |
|----------|-------------|
| camera | Video capture and streaming |
| weather | Environmental data sources |
| bridge | Protocol bridges and visualization |
| recording | Data recording and playback |
| inference | ML model inference |
| telemetry | System monitoring and metrics |

## Publishing Your Node

1. Create your node: `bubbaloop node init my-sensor --type rust`
2. Implement and test locally
3. Push to GitHub
4. Others install with: `bubbaloop node add your-username/my-sensor`

### Quality Guidelines

- Include a `node.yaml` manifest with accurate metadata
- Include a `README.md` with setup instructions
- Include a `config.yaml` example configuration
- Test that `pixi run build` succeeds from a clean checkout
- Pin dependency versions for reproducible builds (commit `Cargo.lock`)

## Future: Node Search

```bash
# Coming soon
bubbaloop node search camera
bubbaloop node search --category weather
bubbaloop node search --tag gstreamer
```
