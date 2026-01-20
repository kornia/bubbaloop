# Configuration

Bubbaloop uses YAML configuration files for each component.

## Camera Configuration

By default, the camera service reads `config.yaml` from the current directory:

```bash
pixi run cameras -- -c /path/to/config.yaml
```

### Basic Configuration

```yaml
cameras:
  - name: "camera_name"      # Unique identifier (used in topic names)
    url: "rtsp://..."        # RTSP stream URL
    latency: 200             # Buffer latency in milliseconds (optional)
```

### Fields

| Field | Type | Required | Default | Description |
|-------|------|----------|---------|-------------|
| `name` | string | Yes | — | Unique camera identifier. Used in topic names: `/camera/{name}/compressed` |
| `url` | string | Yes | — | Full RTSP URL including credentials if needed |
| `latency` | integer | No | `200` | Stream buffer latency in milliseconds |

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

### Complete Camera Example

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

### Stream Selection

Many cameras offer multiple streams:

| Stream | Typical Path | Resolution | Use Case |
|--------|--------------|------------|----------|
| Main | `/stream1` | Full HD | Recording |
| Sub | `/stream2` | Lower | Live preview |

!!! tip "Use sub-streams for monitoring"
    For real-time monitoring with multiple cameras, use sub-streams (`stream2`) to reduce bandwidth and CPU usage.

### Latency Tuning

The `latency` parameter controls the GStreamer buffer size:

| Value | Use Case |
|-------|----------|
| 50-100ms | Low latency for local wired cameras |
| 200ms | Default, good balance for most setups |
| 300-500ms | WiFi cameras or unstable networks |

## Weather Service Configuration

The OpenMeteo weather service uses its own configuration file:

```bash
pixi run weather -- -c crates/openmeteo/configs/config.yaml
```

### Weather Configuration

```yaml
# Location configuration
location:
  auto_discover: false       # Set to true for IP-based location
  latitude: 41.4167
  longitude: 1.9667
  timezone: "Europe/Madrid"

# Fetch intervals (optional)
fetch:
  current_interval_secs: 30      # Poll current weather
  hourly_interval_secs: 1800     # Poll hourly forecast (30 min)
  daily_interval_secs: 10800     # Poll daily forecast (3 hours)
  hourly_forecast_hours: 48      # Hours of hourly forecast
  daily_forecast_days: 7         # Days of daily forecast
```

### Location Options

| Field | Description |
|-------|-------------|
| `auto_discover` | When `true`, location is determined from IP address |
| `latitude` | Latitude in decimal degrees |
| `longitude` | Longitude in decimal degrees |
| `timezone` | IANA timezone identifier (e.g., "America/New_York") |

## Zenoh Configuration

### Default Endpoint

By default, services connect to a local Zenoh router at `tcp/127.0.0.1:7447`.

### Override Endpoint

```bash
# Via CLI flag
pixi run cameras -- -z tcp/192.168.1.100:7447

# Via environment variable
ZENOH_ENDPOINT=tcp/192.168.1.100:7447 pixi run cameras
```

### Priority Order

1. `ZENOH_ENDPOINT` environment variable (highest)
2. `-z` / `--zenoh-endpoint` CLI flag
3. Default: `tcp/127.0.0.1:7447`

### Server Configuration

For running Zenoh as a router, use `zenoh.json5`:

```json5
{
  mode: "router",
  listen: {
    endpoints: ["tcp/0.0.0.0:7447"],
  },
  plugins: {
    remote_api: {
      websocket_port: 10000,
    },
  },
}
```

## Remote Access Configuration

See [Remote Access](../dashboard/remote-access.md) for detailed setup instructions for accessing Bubbaloop from other machines.

## Environment Variables

| Variable | Description | Default |
|----------|-------------|---------|
| `ZENOH_ENDPOINT` | Zenoh router endpoint | `tcp/127.0.0.1:7447` |
| `RUST_LOG` | Logging level | `info` |

## Next Steps

- [Architecture](../concepts/architecture.md) — System design overview
- [RTSP Camera](../components/sensors/rtsp-camera.md) — Camera sensor details
- [OpenMeteo Weather](../components/services/openmeteo.md) — Weather service details
