# Telemetry Watchdog Design

**Date**: 2026-03-05
**Status**: Approved
**Branch**: `feat/openclaw-agent-rewrite`

## Problem

Bubbaloop runs as a long-lived daemon on resource-constrained edge devices (Jetson 8-16GB RAM, RPi, etc.) and general-purpose machines (x86, macOS). A runaway node or memory leak can silently consume all RAM until the OOM killer strikes — no warning, no graceful recovery, no post-mortem data. The current health system is binary (healthy/unhealthy, 30s heartbeat timeout) with zero resource monitoring.

## Solution

A dedicated telemetry layer in the daemon with three components:

1. **Sampler** — cross-platform system metrics via `sysinfo` crate, adaptive rate
2. **Circuit Breaker** — hard safety net that kills nodes at critical thresholds (no LLM needed)
3. **Agent Bridge** — tools + alert events that let the agent make smart decisions and tune config

### Design Principles

- **Hard safety path is independent from the agent** — circuit breaker fires in ~100ms, no LLM in the loop
- **Agent gets rich data for soft decisions** — trending, history, per-process breakdown
- **Cross-platform** — `sysinfo` crate abstracts Linux (ARM + x86), macOS, Windows
- **Hot-reloadable config** — agent can tune thresholds at runtime without daemon restart

## Data Model

### System Metrics

```rust
struct SystemSnapshot {
    timestamp_ms: i64,
    memory_used_bytes: u64,
    memory_total_bytes: u64,
    memory_available_bytes: u64,
    swap_used_bytes: u64,
    swap_total_bytes: u64,
    cpu_usage_percent: f32,       // system-wide
    load_average_1m: f64,
    disk_used_bytes: u64,
    disk_total_bytes: u64,
    disk_path: String,            // monitored path, default "~/.bubbaloop"
}

struct ProcessSnapshot {
    pid: u32,
    name: String,               // node name or "daemon"
    rss_bytes: u64,
    cpu_percent: f32,
}

struct TelemetrySnapshot {
    system: SystemSnapshot,
    processes: Vec<ProcessSnapshot>,
}
```

### Ring Buffer (Hot Storage)

- Fixed-size `VecDeque<TelemetrySnapshot>` in RAM
- Default capacity: 720 samples (1 hour at 5s, or 6 hours at 30s with adaptive)
- Circuit breaker reads from this — zero allocation, zero I/O

### SQLite Cold Storage

- Separate database: `~/.bubbaloop/telemetry.db` (not shared with agent memory DB)
- Table: `system_telemetry(timestamp_ms INTEGER, snapshot_json TEXT)`
- Batch flush: every 60 seconds, write buffered snapshots
- Retention: configurable, default 7 days, pruned daily
- Agent queries this via dispatch tools

### Adaptive Sampling

Mirrors the existing arousal heartbeat pattern:

| Memory Available | Sample Interval |
|-----------------|-----------------|
| > 40% free | 30s (idle) |
| 20-40% free | 10s (elevated) |
| < 20% free | 5s (critical) |

## Circuit Breaker

### Threshold Levels

| Level | Memory Available | Action | Latency |
|-------|-----------------|--------|---------|
| **Green** | > 40% free | Normal operation, sample at 30s | -- |
| **Yellow** | 20-40% free | Increase sampling to 10s, push alert to agent | ~0ms |
| **Orange** | 10-20% free | Sample at 5s, agent gets urgent alert with top consumers | ~0ms |
| **Red** | < 10% free | Circuit breaker fires: stop largest non-essential node | ~100ms |
| **Critical** | < 5% free | Emergency: stop ALL non-essential nodes | ~100ms |

### Essential vs Non-Essential Nodes

- Nodes tagged `essential: true` in `node.yaml` manifest are killed last (only at Critical)
- The daemon itself is never self-terminated
- Kill order: largest RSS first among non-essential, then among essential

### Circuit Breaker Logic

```
on each sample:
    level = classify(snapshot.memory_available_percent)
    if level >= Red:
        candidates = running_nodes.sort_by(rss, descending)
        candidates = candidates.filter(not essential) if level == Red
        stop(candidates[0])
        log::warn!("[WATCHDOG] stopped {} (RSS {}MB, system at {}% memory)", ...)
        notify_agent(NodeKilledByWatchdog { node, reason, snapshot })
    if level >= Yellow:
        notify_agent(ResourceWarning { level, snapshot })
```

### Cooldown

- After killing a node: 30s cooldown before killing another (prevents cascade)
- If memory doesn't recover after 30s, next sample can kill the next largest

### CPU and Disk Thresholds

- **CPU > 95% for 60s sustained**: warn agent (no auto-kill -- high CPU is often legitimate)
- **Disk < 1GB free**: warn agent
- **Disk < 200MB free**: stop nodes that write to disk

## Agent Bridge

### New Dispatch Tools

**`get_system_telemetry`** -- Current snapshot

```json
{
    "memory_used_percent": 62.3,
    "memory_available_mb": 3042,
    "cpu_usage_percent": 34.1,
    "disk_free_gb": 12.8,
    "watchdog_level": "Yellow",
    "top_processes": [
        {"name": "rtsp-camera", "rss_mb": 412, "cpu_percent": 18.2},
        {"name": "yolo-inference", "rss_mb": 890, "cpu_percent": 45.1}
    ]
}
```

**`get_telemetry_history`** -- SQLite query for trending

```json
// Input: { "duration_minutes": 60 }
// Returns downsampled series (1 point per minute)
{
    "samples": [
        {"timestamp": "2026-03-05T14:00:00Z", "memory_percent": 58.2, "cpu_percent": 30.1, "disk_free_gb": 13.1},
        {"timestamp": "2026-03-05T14:01:00Z", "memory_percent": 59.1, "cpu_percent": 28.4, "disk_free_gb": 13.1}
    ],
    "trend": "memory_rising",
    "rate_per_hour_percent": 2.1
}
```

**`update_telemetry_config`** -- Hot-reload thresholds

```json
// Input (partial update, only provided fields change):
{
    "red_memory_percent": 85,
    "cooldown_secs": 60,
    "idle_interval_secs": 15
}
```

### Agent Alert Events

Pushed to agent inbox via Zenoh (no tool call required):

| Event | When | Agent Action |
|-------|------|-------------|
| `ResourceWarning` | Yellow/Orange threshold crossed | Investigate, throttle jobs, warn user |
| `NodeKilledByWatchdog` | Circuit breaker fired | Decide whether to restart, notify user |
| `ResourceRecovered` | Level drops back to Green | Restart previously killed nodes |
| `TrendAlert` | Memory rising >2%/hour sustained | Preemptive action before thresholds hit |

### System Prompt Injection

Each agent turn gets a resource summary injected:

```
## System Resources
Memory: 62% used (Yellow) | CPU: 34% | Disk: 12.8GB free
Watchdog: 1 node killed in last 24h (yolo-inference, OOM risk)
Trend: memory rising ~2%/hour
```

### Agent-Decided Restarts

When the circuit breaker kills a node, the agent decides if/when to restart:
- Prevents restart loops (a leaked node would just get killed and restarted forever)
- Agent can reason: "that ML node has crashed 3 times today, leave it stopped and notify the user"

## Hot-Reload Configuration

### Mechanism

- `TelemetryConfig` held behind `Arc<RwLock<TelemetryConfig>>`
- File watcher on `~/.bubbaloop/telemetry.toml` (same pattern as Soul hot-reload)
- On file change: parse, validate, swap config, log the diff
- Sampler and circuit breaker read config each tick -- no restart needed

### Agent Tuning Examples

- **ML workload running**: relax red threshold from 90% to 95%
- **Quiet overnight**: tighten yellow to 50% to catch leaks earlier
- **After a kill**: increase cooldown to 120s
- **Trend detected**: switch to 5s sampling immediately

### Guardrails

- `critical_memory_percent` cannot be set above 98% or below 80%
- Sampling interval cannot go below 2s
- Invalid config is rejected, previous config stays active
- Every config change audit-logged: `log::info!("[WATCHDOG] config updated by agent: ...")`

## Configuration File

`~/.bubbaloop/telemetry.toml`:

```toml
[telemetry]
enabled = true

[telemetry.sampling]
idle_interval_secs = 30
elevated_interval_secs = 10
critical_interval_secs = 5
ring_buffer_capacity = 720

[telemetry.thresholds]
yellow_memory_percent = 60
orange_memory_percent = 80
red_memory_percent = 90
critical_memory_percent = 95
cpu_warn_percent = 95
cpu_warn_sustained_secs = 60
disk_warn_mb = 1024
disk_critical_mb = 200

[telemetry.circuit_breaker]
enabled = true
cooldown_secs = 30

[telemetry.storage]
flush_interval_secs = 60
retention_days = 7
db_path = "~/.bubbaloop/telemetry.db"

[telemetry.monitored_disk]
path = "~/.bubbaloop"
```

## Module Structure

```
crates/bubbaloop/src/daemon/telemetry/
  mod.rs              -- TelemetryService: spawns sampler + circuit_breaker tasks
  sampler.rs          -- Adaptive sysinfo reader, feeds ring buffer
  circuit_breaker.rs  -- Threshold evaluation, node kill logic
  storage.rs          -- SQLite cold storage (batch writes, retention pruning)
  types.rs            -- SystemSnapshot, ProcessSnapshot, TelemetrySnapshot, WatchdogLevel

crates/bubbaloop/src/agent/
  dispatch.rs         -- +3 tools: get_system_telemetry, get_telemetry_history, update_telemetry_config
  prompt.rs           -- +resource summary injection into system prompt
```

## Dependencies

- **`sysinfo`** crate -- cross-platform (Linux ARM/x86, macOS, Windows)
- No other new dependencies

## Integration Points

1. **Daemon startup** (`daemon/mod.rs`): spawn `TelemetryService` alongside health monitor, pass `shutdown_rx`
2. **Node manager**: telemetry service gets `Arc<RwLock<NodeManager>>` to stop nodes via existing `stop_node()`
3. **Agent dispatch**: telemetry service exposes `Arc<TelemetryService>` for tool queries
4. **Agent runtime**: subscribe to watchdog events on the agent inbox channel
5. **Config hot-reload**: file watcher on `~/.bubbaloop/telemetry.toml`, same pattern as Soul

## Non-Goals (v1)

- GPU memory tracking (future: Jetson unified memory)
- Network I/O monitoring
- Per-node resource limits / cgroups
- Remote telemetry aggregation (multi-machine)
- Dashboard visualization of metrics
