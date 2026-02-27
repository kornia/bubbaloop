# Bubbaloop: Hardware AI Agent

> The open-source AI agent that talks to your cameras, sensors, and robots.

**Date:** 2026-02-27
**Status:** Design approved, pending implementation planning

---

## 1. Vision

Bubbaloop becomes a **self-contained Hardware AI Agent** — like OpenClaw, but for physical hardware. One binary that connects to sensors, cameras, and actuators, controlled through natural language.

**Not an OpenClaw fork.** A new category: **Hardware AI Agent.**

| | OpenClaw | Bubbaloop |
|---|---|---|
| Talks to | Messaging platforms | Physical sensors & actuators |
| Skills | Text behaviors (5,705 in ClawHub) | Sensor drivers (YAML configs) |
| Memory | Markdown files | SQLite (embedded, sensor-native) |
| Language | TypeScript | Rust |
| Edge-ready | No (needs Mac/desktop) | Yes (Jetson, RPi, any Linux) |
| Data plane | None | Zenoh (zero-copy, real-time) |
| Scheduling | LLM-dependent (expensive) | Pre-resolved actions (cheap, offline) |
| Security | ClawHavoc-vulnerable | Sandboxed Rust, no skill injection |
| LLM | Required always-on | Only for new decisions; scheduled actions run without LLM |

**One-liner pitch:** *"The open-source AI agent that talks to your cameras, sensors, and robots."*

---

## 2. Architecture

```
┌─────────────────────────────────────────────────────────┐
│  BUBBALOOP  (single binary)                              │
│                                                          │
│  ┌────────────────────────────────────────────────────┐  │
│  │  Agent Layer                                        │  │
│  │  Claude API | Conversation loop | Tool dispatch     │  │
│  │  Chat interface (terminal / HTTP)                   │  │
│  │  Scheduler (cron-like, pre-resolved actions)        │  │
│  └──────────────────────┬─────────────────────────────┘  │
│                          │                                │
│  ┌──────────────────────┴─────────────────────────────┐  │
│  │  Memory (SQLite — embedded, no external server)    │  │
│  │  Conversations | Sensor events | Schedules          │  │
│  │  Vector search for "what happened yesterday?"       │  │
│  └──────────────────────┬─────────────────────────────┘  │
│                          │                                │
│  ┌──────────────────────┴─────────────────────────────┐  │
│  │  MCP Server (existing — 23+ tools)                  │  │
│  │  Discovery | Lifecycle | Data | Config | System     │  │
│  │  Exposed internally to agent + externally via stdio │  │
│  └──────────────────────┬─────────────────────────────┘  │
│                          │                                │
│  ┌──────────────────────┴─────────────────────────────┐  │
│  │  Daemon (existing — passive skill runtime)          │  │
│  │  Node manager | systemd/D-Bus | Marketplace         │  │
│  └──────────────────────┬─────────────────────────────┘  │
│                          │                                │
│  ┌──────────────────────┴─────────────────────────────┐  │
│  │  Zenoh Data Plane (existing)                        │  │
│  └──────┬───────────┬───────────┬─────────────────────┘  │
└─────────┼───────────┼───────────┼────────────────────────┘
          │           │           │
     ┌────┴───┐  ┌────┴───┐  ┌───┴────┐
     │ Camera │  │  IMU   │  │ Motor  │   Sensor drivers (nodes)
     └────────┘  └────────┘  └────────┘
```

**Key principle: layers, not rewrites.** The agent layer sits ON TOP of the existing MCP server. Everything below the agent layer is unchanged.

**Two entry points, same core:**
- `bubbaloop agent` — self-contained, chat with your hardware
- `bubbaloop mcp --stdio` — plugin for Claude Code / OpenClaw (unchanged)

---

## 3. Agent Loop

Minimal — not a full agent framework. Just enough to connect Claude to the MCP tools.

```
User message (or scheduled trigger)
     │
     ▼
┌─────────────┐
│ Claude API   │  system prompt includes installed sensor inventory
│ (tool_use)   │
└──────┬──────┘
       │ tool calls
       ▼
┌─────────────┐
│ MCP Tools    │  same 23 tools, called internally (not over HTTP)
│ (internal)   │
└──────┬──────┘
       │ results
       ▼
┌─────────────┐
│ Claude API   │  continues reasoning with tool results
│ (response)   │
└──────┬──────┘
       │
       ▼
  Response displayed + Memory updated (SQLite)
```

**System prompt injection:** On every turn, current sensor inventory is injected:
```
You have 3 sensors:
- front-camera: RTSP, running, 30fps, 1080p
- garage-camera: RTSP, running, 15fps, 720p
- system-telemetry: running, CPU 42C, RAM 68%

You have 1 scheduled job:
- health-patrol: every 15min, check all health, restart if unhealthy
```

### What's built-in vs. YAGNI

| Built-in | NOT built-in |
|----------|-------------|
| Conversation loop (Claude API) | Multi-agent orchestration |
| Tool dispatch to internal MCP | Multi-channel (WhatsApp, Discord) |
| SQLite memory | Streaming media through the agent |
| Terminal chat + HTTP API | Skill marketplace / ClawHub equivalent |
| Scheduler (cron-like) | Plugin system |
| YAML skill loader | |

---

## 4. Scheduling

**The scheduling problem:** Robotics/IoT systems need autonomous behavior — check camera health every 15 minutes, record timelapse every hour, restart crashed nodes immediately. OpenClaw solves this by keeping the LLM always-on, which is expensive and requires internet.

**Bubbaloop's approach:** Two-tier scheduling.

### Tier 1: Declarative Schedules (no LLM, runs offline)

Pre-resolved action chains defined in YAML. The daemon executes them directly — no Claude API call needed. Cheap, fast, works offline.

```yaml
# ~/.bubbaloop/skills/health-patrol.yaml
name: health-patrol
schedule: "*/15 * * * *"         # every 15 minutes
actions:
  - check_all_health             # built-in action
  - if_unhealthy: restart        # conditional
  - log_event: "health check completed"

# ~/.bubbaloop/skills/timelapse.yaml
name: office-timelapse
schedule: "0 * * * *"            # every hour
requires:
  - front-camera
actions:
  - capture_frame:
      node: front-camera
      params: { resolution: "4k" }
  - store_event: "timelapse frame captured"
```

Built-in actions for Tier 1 (no LLM needed):

| Action | What it does |
|--------|-------------|
| `check_all_health` | Query health for all running nodes |
| `restart` | Restart unhealthy nodes |
| `capture_frame` | Send capture command to camera node |
| `start_node` / `stop_node` | Node lifecycle |
| `send_command` | Generic command dispatch to any node |
| `log_event` | Write to SQLite sensor_events table |
| `store_event` | Log + vector embed for semantic search |
| `notify` | Print to terminal / write to notification queue |

### Tier 2: Conversational Schedules (LLM-powered)

For complex decisions that need reasoning. The agent calls Claude only when Tier 1 escalates, or when the user creates schedules via chat.

```
You:   "Every morning at 8am, check if the garage camera recorded
        any motion overnight and summarize it for me"

Agent: (creates a scheduled job internally)
       - Schedule: "0 8 * * *"
       - Action: query sensor_events for garage-camera motion events
       - Action: summarize with Claude API (Tier 2 — needs LLM)
       - Action: display summary

       "Done. I'll check the garage camera every morning at 8 and
        give you a summary. This will use one Claude API call per day."
```

### Cost Model

| | OpenClaw | Bubbaloop |
|---|---|---|
| Health check every 15min | 96 LLM calls/day | 0 LLM calls (Tier 1) |
| Morning summary | 1 LLM call/day | 1 LLM call/day (Tier 2) |
| Crash recovery | Always-on LLM | 0 LLM calls (Tier 1) |
| **Daily cost (est.)** | ~$5-10 (always-on) | ~$0.05 (on-demand) |

---

## 5. Memory with SQLite

**Constraint: binary size.** Current bubbaloop is 11MB. Memory layer must add minimal weight.

| Option considered | Binary cost | Verdict |
|-------------------|-------------|---------|
| SQLite (Arrow + DataFusion) | +20-50MB | **Rejected** — triples binary size |
| SQLite (rusqlite, static) | +1-2MB | **Selected** — battle-tested, 12-13MB total |
| redb (pure Rust KV) | +300KB | Considered for future if SQLite too heavy |

Three tables in a single SQLite database:

```
SQLite (~/.bubbaloop/memory.db)
├── conversations    — chat history (text search via FTS5)
├── sensor_events    — timestamped events (health, anomalies, alerts)
└── schedules        — active scheduled jobs + execution history
```

### Schema

```sql
-- Chat history
CREATE TABLE conversations (
    id          TEXT PRIMARY KEY,
    timestamp   TEXT NOT NULL,
    role        TEXT NOT NULL,  -- 'user' / 'assistant' / 'tool'
    content     TEXT NOT NULL,
    tool_calls  TEXT            -- JSON, nullable
);
CREATE INDEX idx_conv_ts ON conversations(timestamp);

-- Sensor events (health changes, crashes, alerts)
CREATE TABLE sensor_events (
    id          TEXT PRIMARY KEY,
    timestamp   TEXT NOT NULL,
    node_name   TEXT NOT NULL,
    event_type  TEXT NOT NULL,  -- 'health_change' / 'started' / 'crashed' / 'alert'
    details     TEXT            -- JSON
);
CREATE INDEX idx_events_node_ts ON sensor_events(node_name, timestamp);

-- Scheduled jobs (Tier 1 + Tier 2)
CREATE TABLE schedules (
    id          TEXT PRIMARY KEY,
    name        TEXT NOT NULL UNIQUE,
    cron        TEXT NOT NULL,
    actions     TEXT NOT NULL,  -- JSON action chain
    tier        INTEGER NOT NULL DEFAULT 1,
    last_run    TEXT,
    next_run    TEXT,
    created_by  TEXT NOT NULL   -- 'yaml' or 'conversation'
);
```

### Example Queries

```sql
-- "Has the garage camera been reliable this week?"
SELECT event_type, timestamp, details FROM sensor_events
WHERE node_name = 'garage-camera'
  AND timestamp > datetime('now', '-7 days')
ORDER BY timestamp DESC;

-- "What did we talk about last time?"
SELECT role, content FROM conversations
ORDER BY timestamp DESC LIMIT 20;

-- "Show me all scheduled jobs"
SELECT name, cron, tier, last_run FROM schedules;
```

**Future: vector search.** If semantic search becomes needed, add `sqlite-vec` extension (~200KB). For v1, time-based + node-name queries are sufficient — the agent can summarize results via Claude.

### Size Budget

```
Current binary:     11 MB
+ rusqlite:          1-2 MB  (static libsqlite3)
+ reqwest:           0 MB    (already in dep tree via zenoh)
+ cron parser:       ~50 KB
─────────────────────────────
Target:             ~12-13 MB
```

---

## 6. Sensor Skills as YAML Configs

### The Driver Model

Users write 5-line YAML. Bubbaloop maps `driver:` to a marketplace node, downloads precompiled binary, injects config, starts.

```yaml
# ~/.bubbaloop/skills/front-camera.yaml
name: front-door
driver: rtsp
config:
  url: rtsp://192.168.1.100/stream
  decoder: auto              # auto-detect NVDEC on Jetson, VA-API on Intel
  resolution: 1080p
  fps: 30
```

### Driver Resolution

```
YAML skill file               Bubbaloop runtime
───────────────                ─────────────────
driver: rtsp          →   1. Lookup "rtsp" in driver registry
                          2. Map to marketplace node "rtsp-camera"
                          3. Check if already installed
                          4. If not: download precompiled binary (ARM64/x86)
                          5. Inject config as env vars + config.yaml
                          6. Create systemd service
                          7. Start node
                          8. Register in MCP (auto-discovered)
```

### Built-in Driver Catalog (v1)

| Driver | Marketplace Node | Use Case |
|--------|-----------------|----------|
| `rtsp` | rtsp-camera | IP cameras, NVRs |
| `v4l2` | v4l2-camera | USB webcams, CSI cameras |
| `serial` | serial-bridge | Arduino, UART, RS-485 |
| `gpio` | gpio-controller | Buttons, LEDs, relays |
| `http-poll` | http-sensor | REST APIs, weather services |
| `mqtt` | mqtt-bridge | Home automation, industrial |
| `modbus` | modbus-bridge | Industrial IoT, PLCs |
| `system` | system-telemetry | CPU, RAM, disk, temperature |

Custom hardware with no matching driver → full node SDK (Rust/Python).

### Skills with Schedules

Skills can combine a driver with scheduled actions:

```yaml
# skills/parking-monitor.yaml
name: parking-monitor
driver: rtsp
config:
  url: rtsp://192.168.1.200/stream

schedule: "*/10 * * * *"        # every 10 minutes
actions:
  - capture_frame:
      node: parking-monitor
  - store_event: "parking snapshot"
```

---

## 7. The "5 Minutes to Magic" Experience

```bash
# 1. Install (30 seconds)
curl -sSL https://get.bubbaloop.com | bash

# 2. Configure a sensor (1 minute)
mkdir -p ~/.bubbaloop/skills
cat > ~/.bubbaloop/skills/camera.yaml << 'EOF'
name: front-door
driver: rtsp
config:
  url: rtsp://192.168.1.100/stream
EOF

# 3. Set API key (10 seconds)
export ANTHROPIC_API_KEY=sk-ant-...

# 4. Go
bubbaloop agent

> What sensors do I have?

  I found 1 configured sensor:
  - front-door (RTSP camera at 192.168.1.100)

  Installing rtsp-camera driver... done.
  Camera is streaming at 30fps, 1080p. What would you like to do?

> Check it every 15 minutes and restart if it goes down

  Created scheduled job "front-door-health" (every 15 min).
  This runs automatically — no API calls needed.

> Also alert me if anyone's at the front door

  I'll need a person detection model. Installing yolo-inference
  from marketplace... done.

  Connected: front-door → yolo-inference pipeline.
  I'll notify you when a person is detected.
```

---

## 8. What Stays, What Changes, What's New

### Existing (unchanged)

| Component | Lines | Status |
|-----------|-------|--------|
| Zenoh data plane | — | Unchanged |
| Daemon / node manager | ~3,500 | Unchanged |
| MCP server (23 tools) | ~2,400 | Unchanged (+ internal calling adapter ~50 lines) |
| CLI (node, debug, doctor, status) | ~5,400 | Unchanged |
| Node SDK | ~800 | Unchanged |
| Marketplace | ~700 | Unchanged |
| Dashboard | ~5,000 | Unchanged |

### New code

| Component | Est. Lines | Module |
|-----------|-----------|--------|
| Agent loop (Claude API + tool dispatch) | 500-800 | `agent/mod.rs`, `agent/loop.rs` |
| SQLite memory | 300-500 | `agent/memory.rs` |
| Scheduler (cron executor) | 200-400 | `agent/scheduler.rs` |
| YAML skill loader | 200-300 | `agent/skills.rs` |
| Driver → node mapper | 100-200 | `agent/drivers.rs` |
| `bubbaloop agent` CLI command | 100-150 | `cli/agent.rs` |
| **Total new code** | **~1,400-2,350** | |

### New dependencies

| Crate | Purpose | Size Impact |
|-------|---------|-------------|
| `rusqlite` (static) | Embedded database for memory | +1-2MB |
| `reqwest` | Claude API client | 0 (already in dep tree) |
| `cron` | Cron expression parsing | ~50KB |

**Total binary size target: ~12-13MB** (up from 11MB). No heavy frameworks (Arrow, DataFusion, etc.).

---

## 9. Security Considerations

### vs. OpenClaw's security issues

| Threat | OpenClaw | Bubbaloop |
|--------|----------|-----------|
| Skill injection (ClawHavoc) | 341 malicious skills in ClawHub | No marketplace injection — drivers are curated, precompiled binaries with checksums |
| Credential storage | Plaintext | API key via env var only, never persisted to disk by bubbaloop |
| System access | Admin-level, unsandboxed | Node sandboxing via systemd + Zenoh ACLs |
| Supply chain | npm ecosystem risks | Rust compiled binaries, no runtime dependencies |
| LLM prompt injection | Skills can inject prompts | Skills are YAML configs, not prompt templates. System prompt is hardcoded. |

### Agent-specific security

- Claude API key: environment variable only (`ANTHROPIC_API_KEY`), never persisted to disk or logs
- Tool permissions: agent uses same RBAC as external MCP clients (Admin for local)
- Scheduled actions: Tier 1 actions are a closed set (cannot execute arbitrary code)
- Tier 2 schedules: rate-limited to prevent runaway API costs (configurable max calls/day)
- SQLite: file permissions 0600, stored in `~/.bubbaloop/memory.db`

---

## 10. Future Directions (not in v1)

These are explicitly out of scope for the initial implementation but represent natural evolution:

- **Multi-channel:** WhatsApp/Telegram/Discord integration (via OpenClaw bridge or native)
- **Local LLM:** Ollama support for fully offline operation
- **Multi-agent:** Specialized sub-agents (one per sensor type)
- **Fleet:** Cloud sync of memory and schedules across machines
- **Voice:** Speech-to-text → agent → text-to-speech for hands-free robot control
- **Visual:** Camera frame analysis in Claude conversations (multimodal)

---

## 11. Success Criteria

The design succeeds if:

1. **5 minutes to first conversation** — install + one YAML skill + `bubbaloop agent`
2. **Zero code for common sensors** — the 8 built-in drivers cover 80% of use cases
3. **Scheduling works offline** — Tier 1 jobs run without internet or LLM
4. **Memory is useful** — "what happened yesterday?" returns meaningful answers
5. **Costs < $1/day** — Tier 1 scheduling keeps LLM calls minimal
6. **Still works as MCP server** — `bubbaloop mcp --stdio` unchanged for Claude Code/OpenClaw users
