# RTSP Camera

The RTSP Camera sensor captures H264 video streams from IP cameras and publishes them to the Zenoh message bus.

## Overview

| Property | Value |
|----------|-------|
| Binary | `cameras_node` |
| Config File | `config.yaml` |
| Output | `CompressedImage` (H264) |
| Topic Pattern | `/camera/{name}/compressed` |

## Features

- **Zero-copy H264 passthrough** — No decoding, minimal CPU usage
- **Multi-camera support** — Run multiple cameras simultaneously
- **SPS/PPS injection** — Stream compatibility for web decoding
- **Configurable latency** — Balance between delay and stability

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

```yaml
cameras:
  - name: "front_door"
    url: "rtsp://admin:password@192.168.1.100:554/stream1"
    latency: 200
```

### Configuration Fields

| Field | Type | Required | Default | Description |
|-------|------|----------|---------|-------------|
| `name` | string | Yes | — | Unique identifier, used in topic name |
| `url` | string | Yes | — | Full RTSP URL with credentials |
| `latency` | integer | No | `200` | Buffer latency in milliseconds |

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

### Multi-Camera Configuration

```yaml
cameras:
  - name: "entrance"
    url: "rtsp://admin:pass@192.168.1.100:554/stream2"
    latency: 200

  - name: "parking"
    url: "rtsp://admin:pass@192.168.1.101:554/stream2"
    latency: 200

  - name: "lobby"
    url: "rtsp://admin:pass@192.168.1.102:554/stream2"
    latency: 300
```

## Running

### Start Camera Service

```bash
# Default config (config.yaml)
pixi run cameras

# Custom config file
pixi run cameras -- -c /path/to/config.yaml

# Custom Zenoh endpoint
pixi run cameras -- -z tcp/192.168.1.50:7447
```

### CLI Options

| Option | Description |
|--------|-------------|
| `-c, --config` | Path to configuration file |
| `-z, --zenoh-endpoint` | Zenoh router endpoint |

### Environment Variables

| Variable | Description |
|----------|-------------|
| `ZENOH_ENDPOINT` | Override Zenoh endpoint |
| `RUST_LOG` | Logging level (info, debug, trace) |

## Topics

### Published Topics

| Topic | Type | Description |
|-------|------|-------------|
| `bubbaloop/{scope}/{machine_id}/camera/{name}/compressed` | `CompressedImage` | H264 compressed frames |

### Topic Format

```
bubbaloop/{scope}/{machine_id}/camera/{name}/compressed
```

**Example:** Camera named `front_door` on machine `nvidia_orin00` publishes to:

```
bubbaloop/local/nvidia_orin00/camera/front_door/compressed
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
| `frame_id` | Camera name |

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
3. Check `pixi run cameras` logs for errors

## Next Steps

- [Camera Panel](../../dashboard/panels/camera.md) — View camera streams
- [Camera API](../../api/camera.md) — Message format details
- [Configuration](../../getting-started/configuration.md) — Full configuration reference
