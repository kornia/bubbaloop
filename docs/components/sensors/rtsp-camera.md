---
description: "RTSP camera node for Bubbaloop. Stream IP cameras via RTSP, publish frames over Zenoh, and feed them to vision detection pipelines."
---

# RTSP Camera

The RTSP Camera sensor captures H264 video streams from IP cameras and publishes them to the Zenoh message bus.

## Overview

| Property | Value |
|----------|-------|
| Binary | `rtsp_camera_node` |
| Config File | `config.yaml` |
| Output | `CompressedImage` (H264) + `RawImage` (RGBA over SHM) |
| Topic Pattern | `bubbaloop/global/{machine_id}/{name}/compressed` |

## Features

- **Zero-copy H264 passthrough** — No decoding on the compressed path, minimal CPU usage
- **Multi-instance support** — Run multiple cameras as separate instances of the same binary
- **SPS/PPS injection** — Stream compatibility for web decoding
- **Raw RGBA over SHM** — Decoded frames published via shared memory for local consumers
- **Hardware acceleration** — NVIDIA NVDEC (Jetson) or CPU fallback
- **Configurable latency and frame rate** — Balance between delay and throughput

## Architecture

```mermaid
flowchart LR
    subgraph RTSP["RTSP Camera"]
        cam[IP Camera]
    end

    subgraph GStreamer["GStreamer Pipeline"]
        rtspsrc[rtspsrc]
        depay[rtph264depay]
        parse[h264parse]
        sink[appsink]
    end

    subgraph Node["Camera Node"]
        zenoh[Zenoh Publisher]
    end

    cam -->|RTSP/RTP| rtspsrc
    rtspsrc --> depay
    depay --> parse
    parse --> sink
    sink -->|H264 NAL units| zenoh
```

## Configuration

### Basic Configuration

Each camera instance has its own `config.yaml`:

```yaml
name: tapo_entrance
url: "rtsp://admin:password@192.168.1.100:554/stream1"
latency: 200
```

### Configuration Fields

| Field | Type | Required | Default | Description |
|-------|------|----------|---------|-------------|
| `name` | string | Yes | — | Unique instance name (used in topic path and health) |
| `url` | string | Yes | — | Full RTSP URL with credentials |
| `latency` | integer | No | `200` | Buffer latency in milliseconds (1-10000) |
| `frame_rate` | integer | No | — | Target publish rate in FPS (1-120, unlimited if unset) |
| `raw_width` | integer | No | `560` | Width of raw RGBA frames (SHM path) |
| `raw_height` | integer | No | `560` | Height of raw RGBA frames (SHM path) |
| `hw_accel` | string | No | `nvidia` | Hardware acceleration: `nvidia` (Jetson NVDEC) or `cpu` |

### RTSP URL Format

```
rtsp://[username:password@]host[:port]/path
```

**Examples:**

```yaml
# With authentication
url: "rtsp://admin:password123@192.168.1.100:554/stream1"

# Without authentication
url: "rtsp://192.168.1.100:554/live"

# Non-standard port
url: "rtsp://camera.local:8554/h264"
```

### Multi-Camera Setup

Each camera runs as a separate instance of the same binary. Register each with a unique name and config:

```bash
bubbaloop node instance rtsp-camera entrance --config configs/entrance.yaml --start
bubbaloop node instance rtsp-camera parking  --config configs/parking.yaml --start
bubbaloop node instance rtsp-camera lobby    --config configs/lobby.yaml --start
```

Each config file is a single camera:

```yaml
# configs/entrance.yaml
name: entrance
url: "rtsp://admin:pass@192.168.1.100:554/stream2"
latency: 200

# configs/parking.yaml
name: parking
url: "rtsp://admin:pass@192.168.1.101:554/stream2"
latency: 200
hw_accel: cpu
```

## Running

### Start Camera Node

```bash
# Via daemon (recommended)
bubbaloop node start rtsp-camera
bubbaloop node logs rtsp-camera -f

# Direct (for development)
pixi run main -- -c config.yaml -e tcp/127.0.0.1:7447
```

### CLI Options

| Option | Description |
|--------|-------------|
| `-c, --config` | Path to configuration YAML file |
| `-e, --endpoint` | Zenoh router endpoint |

### Environment Variables

| Variable | Description |
|----------|-------------|
| `BUBBALOOP_ZENOH_ENDPOINT` | Override Zenoh endpoint (default: `tcp/127.0.0.1:7447`) |
| `BUBBALOOP_MACHINE_ID` | Machine identifier (default: hostname) |
| `RTSP_URL` | Override RTSP URL from config (useful for Docker/env-based deployments) |
| `RUST_LOG` | Logging level (info, debug, trace) |

## Topics

### Published Topics

| Topic | Type | Description |
|-------|------|-------------|
| `bubbaloop/global/{machine_id}/{name}/compressed` | `CompressedImage` | H264 compressed frames (network-visible) |
| `bubbaloop/local/{machine_id}/{name}/raw` | `RawImage` | RGBA decoded frames (SHM-only, same machine) |

### Topic Format

The topic key is derived from the `name` config field. If the name ends with `_camera`, the suffix is stripped:

- `name: tapo_entrance_camera` → topics at `tapo_entrance/compressed` and `tapo_entrance/raw`
- `name: front_door` → topics at `front_door/compressed` and `front_door/raw`

**Example:** Camera named `tapo_terrace` on machine `nvidia_orin00` publishes to:

```
bubbaloop/global/nvidia_orin00/tapo_terrace/compressed   (H264, network-visible)
bubbaloop/local/nvidia_orin00/tapo_terrace/raw           (RGBA, SHM-only)
```

## Message Format

### CompressedImage

| Field | Type | Description |
|-------|------|-------------|
| `header` | Header | Timestamp and metadata |
| `format` | string | Always `"h264"` |
| `data` | bytes | H264 NAL units (Annex B) |

See [Camera API](../../api/camera.md) for the full protobuf definition.

### Header Fields

| Field | Description |
|-------|-------------|
| `acq_time` | Frame acquisition timestamp (nanoseconds) |
| `pub_time` | Message publication timestamp (nanoseconds) |
| `sequence` | Frame sequence number |
| `frame_id` | Camera instance name |
| `machine_id` | Machine identifier (e.g., `nvidia_orin00`) |

## Performance

### CPU Usage

The camera node uses zero-copy H264 passthrough:

| Operation | CPU Impact |
|-----------|------------|
| RTSP receive | Minimal (network I/O) |
| H264 parsing | Near zero (no decode) |
| Protobuf serialization | Minimal |
| Zenoh publish | Minimal |

### Memory Usage

| Cameras | Typical Memory |
|---------|----------------|
| 1 | ~10-20 MB |
| 4 | ~40-80 MB |
| 8 | ~80-160 MB |

### Latency

| Setting | Typical Latency |
|---------|-----------------|
| `latency: 100` | ~150-200ms end-to-end |
| `latency: 200` | ~250-350ms end-to-end |
| `latency: 500` | ~550-700ms end-to-end |

## Stream Selection

Many IP cameras offer multiple streams:

| Stream | Path | Resolution | Bandwidth | Use Case |
|--------|------|------------|-----------|----------|
| Main | `/stream1` | 1080p/4K | High | Recording |
| Sub | `/stream2` | 480p/720p | Low | Monitoring |

!!! tip "Use sub-streams for real-time monitoring"
    Sub-streams reduce bandwidth and improve stability for multi-camera setups.

## Troubleshooting

### Camera not connecting

1. Verify the RTSP URL is correct
2. Test with VLC: `vlc rtsp://...`
3. Check firewall allows RTSP (port 554)
4. Verify credentials

### High latency

1. Reduce `latency` configuration value
2. Use sub-stream instead of main stream
3. Check network bandwidth
4. Use wired connection for multiple cameras

### Dropped frames

1. Increase `latency` configuration value
2. Reduce number of simultaneous cameras
3. Check network congestion
4. Verify camera isn't overloaded

### "Waiting for keyframe" in dashboard

1. Wait a few seconds for the next keyframe
2. Verify camera is streaming H264 (not H265/HEVC)
3. Check `bubbaloop node logs rtsp-camera` for errors

## Next Steps

- [Camera Panel](../../dashboard/panels/camera.md) — View camera streams
- [Camera API](../../api/camera.md) — Message format details
- [Configuration](../../getting-started/configuration.md) — Full configuration reference
