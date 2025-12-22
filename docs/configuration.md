# ⚙️ Configuration

Bubbaloop uses a simple YAML configuration file to define cameras.

## Configuration File

By default, the application reads `config.yaml` from the current directory. You can specify a different path:

```bash
pixi run cameras -- -c /path/to/config.yaml
```

## Camera Configuration

Each camera entry requires the following fields:

```yaml
cameras:
  - name: "camera_name"      # Unique identifier (used in topic names)
    url: "rtsp://..."        # RTSP stream URL
    latency: 200             # Buffer latency in milliseconds (optional)
```

### Fields

| Field | Type | Required | Default | Description |
|-------|------|----------|---------|-------------|
| `name` | string | ✅ | — | Unique camera identifier. Used in ROS topic names: `/camera/{name}/compressed` |
| `url` | string | ✅ | — | Full RTSP URL including credentials if needed |
| `latency` | integer | ❌ | `200` | Stream buffer latency in milliseconds. Lower = less delay, higher = more stable |

## RTSP URL Format

```
rtsp://[username:password@]host[:port]/path
```

### Examples

```yaml
# With authentication
url: "rtsp://admin:password123@192.168.1.100:554/stream1"

# Without authentication
url: "rtsp://192.168.1.100:554/live"

# Non-standard port
url: "rtsp://camera.local:8554/h264"
```

## Complete Example

```yaml
cameras:
  # Front door camera - Tapo C200
  - name: "front_door"
    url: "rtsp://tapo_user:tapo_pass@192.168.1.141:554/stream2"
    latency: 200

  # Backyard camera - Higher latency for WiFi stability
  - name: "backyard"
    url: "rtsp://admin:admin@192.168.1.142:554/h264"
    latency: 500

  # Garage camera - Local wired connection
  - name: "garage"
    url: "rtsp://192.168.1.143:554/live"
    latency: 100
```

## Stream Selection

Many cameras offer multiple streams:

| Stream | Typical Path | Resolution | Use Case |
|--------|--------------|------------|----------|
| Main | `/stream1` | Full HD | Recording |
| Sub | `/stream2` | Lower | Live preview |

!!! tip "Use sub-streams for monitoring"
    For real-time monitoring with multiple cameras, use sub-streams (`stream2`) to reduce bandwidth and CPU usage.

## Latency Tuning

The `latency` parameter controls the GStreamer buffer size:

- **Lower values (50-100ms)**: Less delay, but may cause stuttering on unstable networks
- **Higher values (200-500ms)**: More stable, but increased delay
- **Default (200ms)**: Good balance for most setups

```yaml
# Low latency for local wired cameras
- name: "wired_cam"
  url: "rtsp://192.168.1.10:554/stream"
  latency: 100

# Higher latency for WiFi cameras
- name: "wifi_cam"
  url: "rtsp://192.168.1.20:554/stream"
  latency: 400
```

## Topic Naming

Each camera publishes to a ROS-Z topic based on its name:

```
/camera/{name}/compressed
```

For example, a camera named `front_door` publishes to:

```
/camera/front_door/compressed
```

The message type is `bubbaloop.camera.v1.CompressedImage` containing:

- `header`: Timestamp and sequence information
- `format`: Always `"h264"`
- `data`: Raw H264 NAL units (Annex B format)
