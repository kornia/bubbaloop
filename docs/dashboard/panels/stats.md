# Stats & Metrics Panel

The Stats panel displays real-time metrics about topic activity, message frequencies, and system health.

## Overview

| Property | Value |
|----------|-------|
| Data Source | Zenoh subscription metadata |
| Display | Metrics table and charts |
| Use Case | Monitoring, debugging |

## Features

- **Topic frequency** — Messages per second (Hz) for each topic
- **Message counts** — Total messages received
- **Active subscriptions** — Currently subscribed topics
- **Connection status** — Zenoh connection health
- **Latency metrics** — End-to-end timing (when available)

## Panel Interface

### Topic List

Displays all active topics:

| Column | Description |
|--------|-------------|
| **Topic** | Topic name/key expression |
| **Hz** | Message frequency |
| **Count** | Total messages |
| **Last** | Time since last message |

### Metrics Summary

Overview statistics:

| Metric | Description |
|--------|-------------|
| **Total Topics** | Number of active topics |
| **Total Messages** | Sum of all message counts |
| **Avg Frequency** | Average Hz across topics |
| **Connection** | WebSocket status |

## Understanding Metrics

### Frequency (Hz)

Message frequency indicates how often data is published:

| Hz | Typical Source |
|----|----------------|
| 25-30 | Camera main stream |
| 10-15 | Camera sub stream |
| 0.03 | Current weather (every 30s) |
| 0.0006 | Hourly forecast (every 30min) |

### Message Count

Running total of messages received since:

- Dashboard load
- Last refresh
- Panel creation

### Last Message

Time since the last message was received:

- **< 1s**: Active, real-time
- **1-5s**: Normal for low-frequency topics
- **> 10s**: May indicate issues

## Topic Patterns

### Camera Topics

```
0/camera%front_door%compressed/**
├── Hz: 25.0
├── Count: 15,234
└── Last: 0.04s
```

### Weather Topics

```
0/weather%current/**
├── Hz: 0.03
├── Count: 42
└── Last: 15s

0/weather%hourly/**
├── Hz: 0.0006
├── Count: 3
└── Last: 180s
```

## Adding a Stats Panel

1. Click **Add Panel**
2. Select "Stats" panel type
3. Panel displays all topic metrics

Or use the TUI `/topics` command for a terminal-based view.

## Diagnostic Use Cases

### Verifying Data Flow

Check that expected topics are active:

1. Start a component (cameras, weather)
2. Observe new topic appears in stats
3. Verify Hz matches expected rate

### Detecting Issues

Identify problems through metrics:

| Symptom | Possible Cause |
|---------|----------------|
| Hz = 0 | Publisher not running |
| Low Hz | Network issues, overload |
| Gaps in data | Connection drops |
| Missing topics | Service not started |

### Performance Monitoring

Track system performance:

- Camera FPS vs. expected
- Weather update intervals
- Message delivery latency

## Connection Metrics

### WebSocket Status

| Status | Description |
|--------|-------------|
| **Connected** | Active WebSocket connection |
| **Reconnecting** | Attempting to reconnect |
| **Disconnected** | No connection |

### Zenoh Router

Information about the Zenoh router:

- Router address
- Protocol version
- Session ID

## Comparing with TUI

The Stats panel provides similar information to the TUI `/topics` command:

| Feature | Dashboard Stats | TUI /topics |
|---------|-----------------|-------------|
| Topic list | Yes | Yes |
| Frequency | Yes | Yes |
| Count | Yes | Yes |
| Visual chart | Yes | No |
| History | Yes | No |
| Export | No | No |

## Performance

### Update Rate

- Stats update every second
- Frequency calculated over rolling window
- Counts increment in real-time

### Resource Usage

- Minimal CPU overhead
- Small memory footprint
- No impact on data flow

## Troubleshooting

### No topics displayed

1. Check that services are running
2. Verify Zenoh bridge connection
3. Refresh the dashboard

### Incorrect frequency

1. Frequency is averaged over time
2. Wait for sufficient samples
3. Check for intermittent connectivity

### Missing expected topics

1. Verify service is publishing
2. Check topic name/pattern
3. Use TUI `/topics` to compare

## Next Steps

- [Messaging](../../concepts/messaging.md) — Understanding Zenoh topics
- [Topics](../../concepts/topics.md) — Topic naming conventions
- [CLI Commands](../../reference/cli.md) — TUI topic monitoring
- [Troubleshooting](../../reference/troubleshooting.md) — Common issues
