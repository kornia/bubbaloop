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
BUBBALOOP_ZENOH_ENDPOINT=tcp/192.168.1.100:7447 pixi run cameras
```

### Priority Order

1. `-z` / `--zenoh-endpoint` CLI flag (highest)
2. `BUBBALOOP_ZENOH_ENDPOINT` environment variable
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
| `BUBBALOOP_ZENOH_ENDPOINT` | Zenoh router endpoint | `tcp/127.0.0.1:7447` |
| `RUST_LOG` | Logging level | `warn` |
| `ANTHROPIC_API_KEY` | Anthropic API key for Claude | — |
| `BUBBALOOP_MCP_PORT` | MCP HTTP server port | `8088` |
| `BUBBALOOP_SCOPE` | Zenoh topic scope | `local` |
| `BUBBALOOP_MACHINE_ID` | Machine identifier | hostname |

## Next Steps

- [Architecture](../concepts/architecture.md) — System design overview
- [RTSP Camera](../components/sensors/rtsp-camera.md) — Camera sensor details
- [OpenMeteo Weather](../components/services/openmeteo.md) — Weather service details
