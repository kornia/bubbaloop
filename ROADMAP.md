# Bubbaloop Roadmap

<!-- LIVING DOCUMENT: Update checkboxes as work completes. See ARCHITECTURE.md for design details. -->

> The open-source AI agent that talks to your cameras, sensors, and robots.

---

## Design DNA

> **"Perhaps only apps that rely on specific hardware sensors will remain."**
> — Peter Steinberger, OpenClaw creator (Feb 2026)

**The Steinberger Test**: Does this make the sensor/hardware layer stronger, or does it add app-layer complexity that AI agents will replace?

**Principles**:
1. **Sensor drivers are the product** — not the daemon, not the dashboard
2. **As small as possible** — single Rust binary, ~12-13MB, runs on Jetson/RPi/any Linux
3. **YAML, not code** — common sensors configured in 5 lines, no programming required
4. **MCP-native** — AI agents discover and control hardware via MCP tools
5. **Offline-first** — scheduled actions run without LLM or internet
6. **Secure by default** — Rust, sandboxed nodes, no skill injection vectors

## Vision

```bash
# Install
curl -sSL https://get.bubbaloop.com | bash

# Configure a sensor
cat > ~/.bubbaloop/skills/camera.yaml << 'EOF'
name: front-door
driver: rtsp
config:
  url: rtsp://192.168.1.100/stream
EOF

# Talk to your hardware
bubbaloop agent

> What sensors do I have?
> Check the camera every 15 minutes and restart if it goes down
> Alert me if anyone's at the front door
```

**Five minutes from install to natural-language hardware control.**

---

## Architecture

```
┌─────────────────────────────────────────────────────────┐
│  BUBBALOOP  (single binary, ~12-13 MB)                   │
│                                                          │
│  ┌────────────────────────────────────────────────────┐  │
│  │  Agent Layer                                        │  │
│  │  Claude API | Chat | Scheduler (offline Tier 1)     │  │
│  └──────────────────────┬─────────────────────────────┘  │
│  ┌──────────────────────┴─────────────────────────────┐  │
│  │  Memory (SQLite, +1-2 MB)                           │  │
│  │  Conversations | Sensor events | Schedules          │  │
│  └──────────────────────┬─────────────────────────────┘  │
│  ┌──────────────────────┴─────────────────────────────┐  │
│  │  MCP Server (23+ tools) — sole control interface    │  │
│  └──────────────────────┬─────────────────────────────┘  │
│  ┌──────────────────────┴─────────────────────────────┐  │
│  │  Daemon (passive skill runtime)                     │  │
│  │  Node manager | systemd/D-Bus | Marketplace         │  │
│  └──────────────────────┬─────────────────────────────┘  │
│  ┌──────────────────────┴─────────────────────────────┐  │
│  │  Zenoh Data Plane (zero-copy, real-time)            │  │
│  └──────┬───────────┬───────────┬─────────────────────┘  │
└─────────┼───────────┼───────────┼────────────────────────┘
          │           │           │
     ┌────┴───┐  ┌────┴───┐  ┌───┴────┐
     │ Camera │  │  IMU   │  │ Motor  │   Sensor drivers (nodes)
     └────────┘  └────────┘  └────────┘
```

**Two entry points, same core:**
- `bubbaloop agent` — self-contained hardware AI agent
- `bubbaloop mcp --stdio` — MCP server for Claude Code / OpenClaw

---

## What's Built (Foundation)

### v0.0.1–v0.0.6: MCP-Native Sensor Runtime

- [x] Single binary: CLI + daemon + MCP server
- [x] 23 MCP tools (discovery, lifecycle, data, config, system)
- [x] MCP is sole control interface — Zenoh for data only
- [x] Marketplace with precompiled binaries (ARM64 + x86_64)
- [x] Full node lifecycle via MCP: install, uninstall, start, stop, restart, autostart
- [x] Self-describing nodes: manifest, schema, health, config, command queryables
- [x] Node SDK: 50-line Rust nodes with `bubbaloop-node-sdk`
- [x] Protobuf schema discovery via `bubbaloop/**/schema` wildcard
- [x] 3-tier RBAC (Viewer/Operator/Admin) + bearer token auth
- [x] systemd integration via D-Bus (zbus)
- [x] Dashboard (React + Vite)
- [x] 358 unit tests + 47 MCP integration tests
- [x] TUI removed — codebase simplified to ~14K lines

**Binary size: 11 MB.** Runs on NVIDIA Jetson, Raspberry Pi, any Linux ARM64/x86_64.

---

## What's Next

### Phase 1: YAML Skill Loader + Driver Mapping

**Goal:** Zero-code sensor configuration. Write 5 lines of YAML, not a Rust project.

```yaml
# ~/.bubbaloop/skills/front-camera.yaml
name: front-door
driver: rtsp
config:
  url: rtsp://192.168.1.100/stream
  decoder: auto
```

**Deliverables:**
- [x] `~/.bubbaloop/skills/*.yaml` loader — parse driver + config at startup
- [x] Driver registry: map `driver: rtsp` → marketplace node `rtsp-camera`
- [x] Auto-install: download precompiled binary if driver not present
- [x] Config injection: YAML config → node env vars / config.yaml
- [x] `bubbaloop up` command: load all skills, ensure nodes running
- [x] Built-in driver catalog (v1):

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

**New deps:** None. **New code:** ~300-500 lines.

---

### Phase 2: Agent Loop (Claude API)

**Goal:** Natural language hardware control. `bubbaloop agent` starts a chat.

```
> What sensors do I have?
  front-door (RTSP camera, running, 30fps)
  system-telemetry (running, CPU 42C)

> Start recording from the front door
  [calls send_command("front-door", "start_recording")]
  Recording started.
```

**Deliverables:**
- [x] `bubbaloop agent` CLI command (terminal chat interface)
- [x] Claude API integration via `reqwest` (tool_use for MCP tools)
- [x] Internal MCP tool dispatch (call tools without HTTP round-trip)
- [x] System prompt injection: sensor inventory, node status, active schedules
- [x] Multi-turn conversation loop with tool use
- [ ] HTTP chat endpoint for future dashboard integration

**New deps:** None (`reqwest` already in dep tree). **New code:** ~500-800 lines.

---

### Phase 3: SQLite Memory

**Goal:** "What happened at the front door yesterday?" — sensor-native memory.

**Constraint:** +1-2 MB binary size only. No Arrow, no DataFusion, no heavy frameworks.

```sql
-- Three tables, one embedded database
conversations (id, timestamp, role, content, tool_calls)
sensor_events (id, timestamp, node_name, event_type, details)
schedules     (id, name, cron, actions, tier, last_run, next_run)
```

**Deliverables:**
- [x] SQLite via `rusqlite` (static libsqlite3) at `~/.bubbaloop/memory.db`
- [x] `conversations` table with timestamp indexing
- [x] `sensor_events` table: health changes, crashes, alerts with timestamps
- [x] `schedules` table: active jobs + execution history
- [ ] Daemon event hook: write sensor events as they happen (no polling)
- [x] Context injection: recent events included in agent system prompt
- [ ] Future: add `sqlite-vec` (~200KB) for vector search if needed

**New deps:** `rusqlite` (+1-2 MB). **New code:** ~300-500 lines.

**Size budget:**
```
Current binary:     11 MB
+ rusqlite:          1-2 MB
+ cron parser:       ~50 KB
─────────────────────────────
Target:             ~12-13 MB
```

---

### Phase 4: Scheduling

**Goal:** Autonomous hardware management without always-on LLM.

**Key insight:** OpenClaw keeps the LLM running 24/7 (~$5-10/day). Bubbaloop's Tier 1 schedules run offline with zero LLM calls.

#### Tier 1: Declarative (no LLM, works offline)

```yaml
# In skill files
schedule: "*/15 * * * *"
actions:
  - check_all_health
  - if_unhealthy: restart
  - log_event: "health check completed"
```

Built-in actions: `check_all_health`, `restart`, `capture_frame`, `start_node`, `stop_node`, `send_command`, `log_event`, `store_event`, `notify`.

#### Tier 2: Conversational (LLM on-demand)

```
> Every morning at 8am, summarize overnight camera activity
  Created job "morning-summary" (Tier 2, 1 Claude call/day).
```

**Deliverables:**
- [x] Tier 1 cron executor with built-in action set (offline, no LLM)
- [x] YAML `schedule:` + `actions:` syntax in skill files
- [ ] Tier 2 conversational schedules stored in SQLite
- [ ] Rate limiting: configurable max LLM calls/day
- [ ] `bubbaloop jobs` CLI: list, pause, resume, delete
- [ ] Execution history logged in SQLite

| | OpenClaw | Bubbaloop |
|---|---|---|
| Health check every 15min | 96 LLM calls/day | 0 (Tier 1) |
| Morning summary | 1 call/day | 1 call/day (Tier 2) |
| Crash recovery | Always-on LLM | 0 (Tier 1) |
| **Daily cost** | ~$5-10 | ~$0.05 |

**New deps:** `cron` crate (~50KB). **New code:** ~200-400 lines.

---

### Phase 5: Polish + "5 Minutes to Magic"

**Goal:** Install → configure → chat in under 5 minutes.

**Deliverables:**
- [ ] `curl -sSL https://get.bubbaloop.com | bash` install script
- [x] `bubbaloop login` — API key setup (browser + paste + validate + save)
- [ ] First-run wizard: detect hardware, suggest skills
- [ ] `bubbaloop agent` auto-loads skills, auto-installs drivers, starts chat
- [ ] AI-assisted skill creation: "Add my garage camera at rtsp://..."
- [ ] Conversational scheduling: "Check cameras every hour"

---

## Future (Not in v1)

These are out of scope but represent natural evolution:

- **Local LLM** — Ollama support for fully offline agent operation
- **Hardware discovery** — USB/V4L2/mDNS auto-detection of connected sensors
- **Multi-channel** — WhatsApp/Telegram/Discord (via OpenClaw bridge or native)
- **Fleet** — Cloud sync of memory and schedules across machines
- **Voice** — Speech-to-text for hands-free robot control
- **Visual** — Camera frame analysis in Claude conversations (multimodal)
- **Security hardening** — Zenoh mTLS, per-node ACLs, Python sandboxing

---

## Comparison

| | OpenClaw | NanoClaw | PicoClaw | **Bubbaloop** |
|---|---|---|---|---|
| **Language** | TypeScript | Python | Go | **Rust** |
| **Binary** | ~200 MB (Node.js) | ~50 MB (Python) | ~10 MB | **~12-13 MB** |
| **Focus** | General agent | Container security | Ultra-light edge | **Sensors/hardware** |
| **MCP role** | Client (consumes) | Client | Planned | **Server (provides)** |
| **Data plane** | None | None | None | **Zenoh (zero-copy)** |
| **Hardware** | None | None | Runs on $10 hw | **Drives hardware** |
| **Scheduling** | LLM-dependent | LLM-dependent | Basic cron | **Tier 1 offline + Tier 2 LLM** |
| **Memory** | Markdown files | Markdown files | SQLite | **SQLite (sensor-native)** |
| **Edge-ready** | No | Docker only | Yes | **Yes (Jetson, RPi)** |

---

## Technology Stack

| Component | Technology | Why |
|-----------|------------|-----|
| Runtime | Rust + Tokio | Memory safety, small binary, edge-ready |
| Data plane | Zenoh | Zero-copy pub/sub, decentralized, Rust-native |
| Schemas | Protobuf + prost | Self-describing, runtime introspection |
| Control | MCP (rmcp) | Standard AI agent interface, 23+ tools |
| Memory | SQLite (rusqlite) | Embedded, +1-2 MB, battle-tested everywhere |
| CLI | argh | Minimal, fast compile |
| Logging | log + env_logger | Simple, stderr-only |
| systemd | zbus (D-Bus) | No subprocess spawning, safe |
| LLM | Claude API (reqwest) | Best tool-use, zero new deps |

---

## Design Documents

- `docs/plans/2026-02-27-hardware-ai-agent-design.md` — Full agent design (architecture, memory, scheduling, security)
- `docs/plans/2026-02-28-agent-implementation-design.md` — Agent implementation design (Phases 2-4)
- `docs/plans/2026-02-28-agent-implementation.md` — Step-by-step implementation plan
- `ARCHITECTURE.md` — Layer model, node contract, security, technology choices
- `CONTRIBUTING.md` — Agentic workflows, agent tiers, validation
