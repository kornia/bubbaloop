# Configuration

Bubbaloop uses YAML configuration files for each component.

## Camera Configuration

The RTSP camera is a standalone node from `bubbaloop-nodes-official`. Each camera instance has its own `config.yaml`:

```bash
# Install and start
bubbaloop node install rtsp-camera
bubbaloop node start rtsp-camera
```

### Basic Configuration

```yaml
name: "front_door"           # Unique instance name (used in topic paths)
url: "rtsp://..."            # RTSP stream URL
latency: 200                 # Buffer latency in milliseconds (optional)
```

### Fields

| Field | Type | Required | Default | Description |
|-------|------|----------|---------|-------------|
| `name` | string | Yes | — | Unique instance name. Used in topics: `bubbaloop/global/{machine_id}/{name}/compressed` |
| `url` | string | Yes | — | Full RTSP URL including credentials if needed |
| `latency` | integer | No | `200` | Stream buffer latency in milliseconds (1-10000) |
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

### Complete Camera Example

```yaml
# config.yaml for a single camera instance
name: front_door
url: "rtsp://tapo_user:tapo_pass@192.168.1.141:554/stream2"
latency: 200
frame_rate: 10
raw_width: 640
raw_height: 480
hw_accel: nvidia
```

For multiple cameras, create separate instances:

```bash
bubbaloop node instance rtsp-camera front-door  --config configs/front_door.yaml --start
bubbaloop node instance rtsp-camera backyard    --config configs/backyard.yaml --start
bubbaloop node instance rtsp-camera garage      --config configs/garage.yaml --start
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

The OpenMeteo weather node is a standalone node from `bubbaloop-nodes-official`:

```bash
bubbaloop node install openmeteo
bubbaloop node start openmeteo
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
# Via CLI flag (node binary)
./rtsp_camera_node -c config.yaml -e tcp/192.168.1.100:7447

# Via environment variable
BUBBALOOP_ZENOH_ENDPOINT=tcp/192.168.1.100:7447 bubbaloop daemon
```

### Priority Order

1. `ZENOH_ENDPOINT` environment variable (highest)
2. `BUBBALOOP_ZENOH_ENDPOINT` environment variable
3. `-e` / `--endpoint` CLI flag
4. Default: `tcp/127.0.0.1:7447`

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
      websocket_port: 10001,
    },
  },
}
```

## Agent Configuration

Agents are configured in `~/.bubbaloop/agents.toml`:

```toml
[agents.jean-clawd]
enabled = true
default = true
provider = "claude"      # "claude" or "ollama"

[agents.camera-expert]
enabled = true
capabilities = ["camera", "rtsp", "video"]
provider = "claude"
```

| Field | Type | Default | Description |
|-------|------|---------|-------------|
| `enabled` | bool | `true` | Whether agent starts with daemon |
| `default` | bool | `false` | Receives messages when no agent specified |
| `capabilities` | list | `[]` | Tags for agent routing |
| `provider` | string | `"claude"` | LLM provider: `"claude"` or `"ollama"` |
| `model` | string | — | Model name override (e.g., `"claude-haiku-4-5-20251001"`, `"qwen3.5:9b"`). Overrides `soul/capabilities.toml` `model_name` when set. |

When no `agents.toml` exists, a single default agent named `jean-clawd` is created automatically.

Use `bubbaloop agent setup [-a <id>]` to configure agents interactively without editing TOML by hand.

## Soul Files

Each agent's personality and behavior are defined by two files in `~/.bubbaloop/agents/{id}/soul/`:

### identity.md

Free-form markdown defining the agent's personality:

```markdown
# Jean-Clawd
I'm a hardware-obsessed shrimp who lives in your Jetson.
I specialize in camera feeds and sensor monitoring.
When I greet users, I always include a shrimp pun.
```

### capabilities.toml

Model and behavior settings:

```toml
model_name = "claude-sonnet-4-20250514"
max_turns = 15
allow_internet = true

# Heartbeat tuning (adaptive interval)
heartbeat_base_interval = 60
heartbeat_min_interval = 5
heartbeat_decay_factor = 0.7

# Approval mode: "auto" = execute immediately, "propose" = save for approval
default_approval_mode = "auto"

# Retry / circuit breaker
max_retries = 3

# Pre-compaction flush threshold (tokens from context limit)
compaction_flush_threshold_tokens = 4000

# Memory retention: delete episodic logs older than N days (0 = keep forever)
episodic_log_retention_days = 30

# Temporal decay: half-life in days for search scoring (0 = no decay)
episodic_decay_half_life_days = 30
```

| Field | Default | Description |
|-------|---------|-------------|
| `model_name` | `claude-sonnet-4-20250514` | LLM model identifier |
| `max_turns` | `15` | Maximum tool-use turns per agent job |
| `allow_internet` | `true` | Whether the agent can make internet requests |
| `heartbeat_base_interval` | `60` | Resting heartbeat interval in seconds |
| `heartbeat_min_interval` | `5` | Minimum heartbeat interval (max arousal) |
| `heartbeat_decay_factor` | `0.7` | Arousal decay factor per calm beat (0.0-1.0) |
| `default_approval_mode` | `"auto"` | Approval mode: `"auto"` or `"propose"` |
| `max_retries` | `3` | Max consecutive failures before circuit breaker |
| `compaction_flush_threshold_tokens` | `4000` | Tokens from context limit before flush |
| `episodic_log_retention_days` | `30` | Days before episodic cleanup (0 = keep forever) |
| `episodic_decay_half_life_days` | `30` | Temporal decay half-life in days (0 = no decay) |

**Fallback chain:** per-agent Soul → global Soul (`~/.bubbaloop/soul/`) → compiled defaults.

**Hot-reload:** Edit files while daemon is running. Changes take effect on the next agent turn.

## Telemetry Configuration

System monitoring is configured in `~/.bubbaloop/telemetry.toml`:

```toml
[telemetry.thresholds]
yellow_pct = 60          # Memory used % → Yellow
orange_pct = 80          # Memory used % → Orange
red_pct = 90             # Memory used % → Red (kills largest non-essential node)
critical_pct = 95        # Memory used % → Critical (kills ALL non-essential nodes)
cpu_warn_pct = 95        # CPU usage % that triggers a warning
cpu_sustained_secs = 60  # Seconds of sustained CPU before trend alert
disk_warn_mb = 1024      # Free disk below this → warning
disk_critical_mb = 200   # Free disk below this → critical

[telemetry.sampling]
idle_secs = 30           # Green/Yellow sampling interval
elevated_secs = 10       # Orange/Red sampling interval
critical_secs = 5        # Critical sampling interval
ring_capacity = 720      # In-memory ring buffer size

[telemetry.circuit_breaker]
enabled = true
cooldown_secs = 30
```

Severity levels (based on memory usage):

| Level | Threshold | Sampling | Action |
|-------|-----------|----------|--------|
| Green | < 60% used | 30s | Normal operation |
| Yellow | 60-80% used | 30s | Warn agent |
| Orange | 80-90% used | 10s | Urgent alert |
| Red | 90-95% used | 10s | Kill largest non-essential node |
| Critical | > 95% used | 5s | Kill ALL non-essential nodes |

Hot-reload: Edit `telemetry.toml` while daemon is running. Agents can also tune thresholds via the `update_telemetry_config` tool.

## Remote Access Configuration

See [Remote Access](../dashboard/remote-access.md) for detailed setup instructions for accessing Bubbaloop from other machines.

## Environment Variables

| Variable | Description | Default |
|----------|-------------|---------|
| `ZENOH_ENDPOINT` | Zenoh router endpoint (highest priority) | `tcp/127.0.0.1:7447` |
| `BUBBALOOP_ZENOH_ENDPOINT` | Zenoh router endpoint (fallback) | `tcp/127.0.0.1:7447` |
| `RUST_LOG` | Logging level | `info` |
| `ANTHROPIC_API_KEY` | Anthropic API key for Claude | — |
| `BUBBALOOP_MCP_PORT` | MCP HTTP server port | `8088` |
| `BUBBALOOP_MACHINE_ID` | Machine identifier | hostname |

## Next Steps

- [Architecture](../concepts/architecture.md) — System design overview
- [RTSP Camera](../components/sensors/rtsp-camera.md) — Camera sensor details
- [OpenMeteo Weather](../components/services/openmeteo.md) — Weather service details
