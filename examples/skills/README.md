# Skill YAML Examples Gallery

Skills are intent declarations. A skill tells the daemon what to run and tells the agent what to care about. You write 5–30 lines of YAML; the daemon handles process lifecycle, health monitoring, and Zenoh routing; the agent handles interpretation, alerting, and natural-language interaction.

## Quick Start

```bash
# Copy a skill to your skills directory
cp examples/skills/simple/weather.yaml ~/.bubbaloop/skills/

# Start all enabled skills
bubbaloop up

# Check running skills
bubbaloop skill list

# View skill drivers available on this system
bubbaloop skill drivers
```

## Gallery

### Simple Skills (v2 format — single driver)

Single-driver skills use the flat `driver:` + `config:` syntax. They are the easiest to write and share.

| File | Driver | What it does |
|------|--------|-------------|
| `simple/weather.yaml` | `http-poll` | Polls Open-Meteo free API every 5 min. No API key required. Alerts on freezing/wind. |
| `simple/system-monitor.yaml` | `system` | CPU, RAM, disk, temperature every 10s. Alerts on high resource usage. |
| `simple/port-scanner.yaml` | `exec` | Checks if port 8080 is open every 30s using netcat. |
| `simple/webhook-receiver.yaml` | `webhook` | Receives GitHub webhooks on localhost:9090. Alerts on new PRs and CI failures. |

### Pipeline Skills (v3 format — multi-operator)

Pipeline skills use `operators:` + `links:` to compose multiple drivers into a data-flow graph. `version: 3` activates pipeline mode.

| File | Drivers | What it does |
|------|---------|-------------|
| `pipeline/entrance-watch.yaml` | `rtsp` + `webhook` | RTSP camera at two frame rates. Night-hours motion alerting. Requires `rtsp-camera` node. |
| `pipeline/greenhouse.yaml` | `http-poll` x3 + `webhook` | Three HTTP sensors (temp, humidity, light) with per-sensor alerting and hourly summaries. |
| `pipeline/home-presence.yaml` | `system` + `exec` | Pings home router to detect presence. Logs arrivals/departures. Evening activity summary. |
| `pipeline/dev-camera.yaml` | `exec` x2 | Synthetic camera stream for testing pipelines without hardware. Replace with `rtsp`/`v4l2` when ready. |

### Agent-Focused Skills

These skills are data-light and intent-heavy. The driver gives the agent context; the `intent:` field tells the agent what analysis to perform.

| File | Driver | What it does |
|------|--------|-------------|
| `agent/security-brief.yaml` | `system` | Daily 8am security brief: health + anomalies + recommended actions. Structured 3-section format. |
| `agent/maintenance-check.yaml` | `exec` | Monthly maintenance checklists + immediate disk alerts. Tracks uptime and disk usage. |

## Skill Format Reference

### v2 Format (single driver)

```yaml
name: my-skill            # Required. Must match [a-zA-Z0-9_-]{1,64}.
driver: http-poll         # Required. See 'bubbaloop skill drivers' for valid names.
enabled: true             # Optional. Set false to skip on 'bubbaloop up'.

config:                   # Driver-specific parameters.
  url: "https://..."
  interval_secs: 60

intent: |                 # Optional but recommended. Plain English for the agent.
  What to monitor and what actions to take.

on:                       # Optional. Declarative event handlers.
  - trigger: data         # Event source. "data" = any published sample.
    condition: "value > 80"   # Simple predicate on the JSON payload.
    action: notify        # Action: notify | log | agent.wake | zenoh.publish
    message: "Value is ${value}"   # Template with ${field} substitution.
    cooldown: 5m          # Minimum time between firings. Supports: s, m, h.
    between: "23:00-06:00"   # Only fire during this time window (optional).

schedule: "0 8 * * *"    # Optional. Cron expression for periodic agent check-ins.
```

### v3 Format (pipeline)

```yaml
name: my-pipeline
version: 3               # Required to activate pipeline mode.

vars:                    # Optional. Shared variables for ${substitution}.
  alert_url: "http://localhost:9090"

operators:               # List of driver instances.
  - id: source           # Unique within this pipeline. Used in links.
    driver: http-poll    # Driver name.
    config:
      url: "https://..."
    outputs:             # Optional. Override rate per output port.
      data:
        rate: 1fps

  - id: sink
    driver: webhook
    config:
      url: "${alert_url}"

links:                   # Data-flow wiring.
  - from: source/data    # Format: {operator_id}/{output_name} or just {operator_id}.
    to: sink             # Operator ID or reserved: dashboard | log | null.
  - from: source
    to: dashboard

intent: |                # Same as v2. Agent reads this at startup.
  What to watch and how to respond.

on:                      # Same as v2. Trigger uses {operator_id}/{event_name}.
  - trigger: source/data
    condition: "value > 100"
    action: notify
    message: "High value: ${value}"
    cooldown: 10m

schedule: "0 * * * *"   # Same as v2.
```

## Available Drivers

Run `bubbaloop skill drivers` to see the full list. At the time of writing:

### Builtin Drivers (no download required)

| Driver | Description | Key config |
|--------|-------------|-----------|
| `http-poll` | REST APIs, HTTP sensors | `url`, `interval_secs`, `timeout_secs` |
| `system` | CPU, RAM, disk, temperature | `interval_secs` |
| `exec` | Shell command on interval, publishes stdout | `command`, `interval_secs` |
| `webhook` | HTTP POST listener, publishes body | `port`, `path` |
| `tcp-listen` | Raw TCP listener | `port`, `bind` |

### Marketplace Drivers (require node binary download)

| Driver | Marketplace node | Description |
|--------|-----------------|-------------|
| `rtsp` | `rtsp-camera` | IP cameras, NVRs, ONVIF |
| `v4l2` | `v4l2-camera` | USB webcams, CSI cameras |
| `serial` | `serial-bridge` | Arduino, UART, RS-485 |
| `gpio` | `gpio-controller` | Buttons, LEDs, relays |
| `mqtt` | `mqtt-bridge` | Home automation, industrial MQTT |
| `modbus` | `modbus-bridge` | Industrial IoT, PLCs |

Install a marketplace driver: `bubbaloop node install rtsp-camera`

## Writing Good `intent:` Fields

The `intent:` field is the primary interface between you and the agent. Vague intents produce vague behavior.

**Too vague:**
```yaml
intent: Monitor the system.
```

**Good:**
```yaml
intent: |
  Monitor CPU and memory every hour.
  Alert if CPU stays above 80% for more than 3 consecutive readings.
  At 8am daily: summarize last 24h resource usage in 2 sentences.
  If disk is above 85%: suggest specific cleanup commands.
```

Guidelines:
- State what data to monitor (be specific about field names if known)
- State conditions that trigger alerts (include thresholds and units)
- State the format of the output (number of lines, tone, structure)
- State what NOT to do (no repeated alerts, no generic advice)
- Reference time windows explicitly ("between midnight and 6am", "daily at 8am")

## The `on:` Handler Syntax

`on:` handlers are evaluated by the EventBridge at the data layer — they are fast, deterministic, and run on every sample. The agent only wakes when a handler fires (or on schedule).

```yaml
on:
  - trigger: data            # "data" for v2; "{op_id}/{event}" for v3 pipelines
    condition: "field > 80"  # Optional. Simple predicate. Omit to fire on every sample.
    action: notify           # notify | log | agent.wake | zenoh.publish
    message: "Alert: ${field} is ${field_value}"   # ${} templates expand from payload
    cooldown: 5m             # Prevent alert storms. Formats: 30s, 5m, 1h, 24h
    between: "22:00-07:00"  # Optional time window filter (local time, 24h format)
```

Supported actions:
- `notify` — wake the agent with the message; agent can escalate or log
- `log` — write to daemon log only; agent is not woken
- `agent.wake` — wake the agent without a specific message (agent reads current state)
- `zenoh.publish` — publish message to a Zenoh topic (future)

## Tips

**Skills are shareable dotfiles.** Copy any `.yaml` file from this gallery, edit the URLs and thresholds, and share it. Recipients with the same driver run it immediately — no code, no compilation.

**Disable without deleting.** Add `enabled: false` to a skill file to skip it on `bubbaloop up` without removing the file.

**v2 and v3 are compatible.** The daemon normalizes v2 skills to single-operator v3 internally. You can start with a v2 skill and add `version: 3` + `operators:` later without changing the published topic names (v2 skill name becomes the operator ID).

**Test with dev-camera.yaml first.** Before setting up a real camera pipeline, run `pipeline/dev-camera.yaml` to verify your pipeline topology and dashboard connection work end-to-end.

**Variables reduce duplication.** In v3 skills, put repeated values (IPs, endpoints, names) in `vars:` and reference them as `${var_name}` in operator configs and messages.
