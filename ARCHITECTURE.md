<!-- LIVING DOCUMENT: Update when architecture changes. Source of truth for design decisions. -->

# 🦐 Bubbaloop Architecture

The open-source AI agent that talks to your cameras, sensors, and robots.
Single Rust binary (~12-13 MB). Runs on Jetson, Raspberry Pi, any Linux.

## Design DNA

### The Steinberger Principle

> *"Perhaps only apps that rely on specific hardware sensors will remain."*
> — Peter Steinberger (Feb 2026)

**Core thesis**: 80% of software apps will be replaced by AI agents. The surviving 20% are those that **interface with physical reality** — sensors, actuators, hardware.

Therefore:

1. **Sensor drivers are the product** — not the daemon, not the dashboard, not the CLI
2. **As small as possible** — single Rust binary, ~12-13 MB, runs on embedded hardware
3. **YAML, not code** — common sensors configured in 5 lines, no programming required
4. **MCP-native** — AI agents discover and control hardware via MCP tools
5. **Offline-first** — scheduled actions run without LLM or internet
6. **Secure by default** — Rust, sandboxed nodes, no skill injection vectors

### The Steinberger Test

Every design decision must pass this test:

**Does this make the sensor/hardware layer stronger, or does it add app-layer complexity that AI agents will replace?**

If it's app-layer complexity → reject it. If it strengthens sensor drivers → accept it.

---

## Layer Model

```
┌─────────────────────────────────────────────────────────┐
│  BUBBALOOP  (single binary, ~12-13 MB)                   │
│                                                          │
│  ┌────────────────────────────────────────────────────┐  │
│  │  Agent Runtime (multi-agent, Zenoh gateway)        │  │
│  │  Soul | EventSink | Heartbeat | per-agent Memory   │  │
│  └──────────────────────┬─────────────────────────────┘  │
│  ┌──────────────────────┴─────────────────────────────┐  │
│  │  4-Tier Memory + Mission Engine                     │  │
│  │  World State (SQLite) | Short-term (RAM)            │  │
│  │  Episodic (NDJSON/FTS5) | Semantic (SQLite)         │  │
│  └──────────────────────┬─────────────────────────────┘  │
│  ┌──────────────────────┴─────────────────────────────┐  │
│  │  MCP Server (49 tools) — sole control interface     │  │
│  │  RBAC (Viewer/Operator/Admin) | Bearer token auth   │  │
│  │  PlatformOperations trait | Rate limiting            │  │
│  └──────────────────────┬─────────────────────────────┘  │
│  ┌──────────────────────┴─────────────────────────────┐  │
│  │  Daemon (skill runtime + agent host)                │  │
│  │  Node manager | systemd/D-Bus | Marketplace         │  │
│  │  Telemetry watchdog (memory/CPU/disk monitoring)    │  │
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

**Entry points:**
- `bubbaloop agent setup` — interactive wizard: configure provider, model, and identity. No daemon needed.
- `bubbaloop agent chat` — thin Zenoh CLI client (LLM runs daemon-side)
- `bubbaloop agent list` — discover running agents via manifest queryables
- `bubbaloop mcp --stdio` — MCP server for Claude Code / external agents

**Key principle**: Nodes are autonomous and self-describing. The MCP server is the sole control interface — Zenoh is the data plane only. The agent runtime runs inside the daemon, with agents configured via `~/.bubbaloop/agents.toml` and per-agent state in `~/.bubbaloop/agents/{id}/`.

**Agent identity model:**
- **Agent ID** (`agents.toml` key, e.g. `jean-clawd`) — immutable routing key; determines filesystem path and Zenoh topics
- **Display name / personality** (`~/.bubbaloop/agents/{id}/soul/identity.md`) — freely editable plain text; soul watcher hot-reloads without daemon restart
- **First-run onboarding** — if no `identity.md` exists and a `.needs-onboarding` marker is present (written by `agent setup`), the daemon injects an onboarding system prompt on the first chat turn. The LLM interviews the user, writes `identity.md`, and clears the marker automatically.

**Per-agent model override:** `agents.toml` supports an optional `model` field that overrides `Soul.capabilities.model_name` at the provider level. This lets different agents run different models without touching soul files.

---

## Node Contract

Every sensor node MUST implement these standard queryables:

```
bubbaloop/{scope}/{machine_id}/{node_name}/schema      → FileDescriptorSet bytes
bubbaloop/{scope}/{machine_id}/{node_name}/manifest    → JSON manifest
bubbaloop/{scope}/{machine_id}/{node_name}/health      → "ok" | error details
bubbaloop/{scope}/{machine_id}/{node_name}/config      → JSON config (GET/SET)
bubbaloop/{scope}/{machine_id}/{node_name}/command     → JSON command interface
```

Standard node publishers:

```
bubbaloop/{scope}/{machine_id}/{publish_topic}          → Data (protobuf or JSON)
bubbaloop/{scope}/{machine_id}/health/{node_name}       → Periodic heartbeat
```

### Zenoh Encoding (Required)

Every publish MUST set the Zenoh `Encoding` field:
- Protobuf: `Encoding::APPLICATION_PROTOBUF.with_schema("fully.qualified.TypeName")`
- JSON: `Encoding::APPLICATION_JSON`

The dashboard reads `sample.encoding()` to pick the decoder instantly — no schema discovery race.
Use the SDK's `publisher_proto()` / `publisher_json()` which set encoding automatically.

### Manifest Fields (Required)

Each node serves a JSON manifest at `{topic_prefix}/manifest` with:
- **Identity**: name, version, language, description, machine_id, scope
- **Hardware**: capabilities, requires_hardware
- **Data**: publishes (topics, message types, rate, QoS)
- **Control**: commands (name, description, parameters, returns)
- **Queryables**: schema_key, health_key, config_key, command_key
- **Security**: acl_prefix for Zenoh ACL rules

See node templates in `bubbaloop-nodes-official` for full examples.

### Schema Contract

Nodes that publish **protobuf** messages MUST serve their FileDescriptorSet via `{node-name}/schema` queryable. JSON-only nodes do not need a schema queryable.

**Key rules:**
- Compile descriptor.bin via `build.rs` (Rust) or use `msg.DESCRIPTOR.file.serialized_pb` (Python)
- Include all .proto files (including `header.proto` from bubbaloop-schemas)
- NEVER use `.complete(true)` (Rust) or `complete=True` (Python) — blocks wildcard queries
- Reply with raw FileDescriptorSet bytes, not JSON
- Python: `query.key_expr` is a property, not a method

### Daemon Wire Format

The daemon gateway uses **JSON for all messages** (manifest, node list, commands, events). No protobuf on the daemon side. The dashboard decodes daemon messages with `JSON.parse()` — no schema needed.

### Command Contract

Nodes that support imperative actions MUST declare `{topic_prefix}/command` queryable.

**Protocol:**
- Query with no payload → list of available commands
- Query with JSON payload → executes the command

```json
{"command": "capture_frame", "params": {"resolution": "1080p"}}
→ {"result": "frame captured", "error": null}
```

---

## Physical AI Memory & Mission Engine

Implemented in v0.0.11. Full design: `docs/plans/2026-03-08-physical-ai-memory-mission-implementation.md`

### 4-Tier Memory Model

| Tier | Storage | Who Writes | Token Budget |
|------|---------|------------|--------------|
| **0 — Live World State** | SQLite `world_state` table | Context Providers (rule engine, no LLM) | Injected at top of every turn |
| **1 — Short-term** | RAM `Vec<Message>` | LLM turns | Active turn only |
| **2 — Episodic** | NDJSON + FTS5 | LLM turns | BM25 search recall |
| **3 — Semantic** | SQLite | LLM + belief engine | Beliefs + jobs + proposals |

**Causal chains**: `episodic_meta` table (regular table, NOT FTS5) links entries via `cause_id` for `"motor hot → reduced speed → cooled"` recall chains. FTS5 virtual tables do not support `ALTER TABLE` — use join instead.

### Beliefs (`daemon/belief_updater.rs`)

Durable assertions the agent holds about the world, modeled as **subject + predicate → value** triples with confidence scores.

- `update_belief` (Operator): creates or updates a belief, increments `confirmation_count` on repeated update
- `get_belief` (Viewer): retrieves a belief as JSON or `"not found"`
- `spawn_belief_decay_task`: daemon background task that periodically reduces `confidence` by the configured rate
- Token overlap scoring confirms/contradicts existing beliefs from observations

```
# Example MCP session — tracking camera reliability
update_belief subject="front_door_camera" predicate="is_reliable" value="true" confidence=0.95 source="heartbeat_monitor"
→ Belief (front_door_camera, is_reliable) updated with confidence 0.95

get_belief subject="front_door_camera" predicate="is_reliable"
→ {"id": "...", "subject": "front_door_camera", "predicate": "is_reliable",
   "value": "true", "confidence": 0.95, "confirmation_count": 1, ...}

# After more heartbeat confirmations:
update_belief subject="front_door_camera" predicate="is_reliable" value="true" confidence=0.97
→ confirmation_count becomes 2, confidence updated to 0.97
```

### Context Providers (`daemon/context_provider.rs`)

Daemon background tasks that subscribe to Zenoh topics and write to world state — no LLM involved. Filter expression: `field=value AND field2>number`. World state key templates support `{field}` substitution from payload. Each provider opens its own rusqlite connection (WAL mode allows concurrent reads).

```
# Wire a vision node's detections into world state
configure_context
  mission_id="security-patrol"
  topic_pattern="bubbaloop/**/vision/detections"
  world_state_key_template="{label}.location"
  value_field="label"
  filter="confidence>0.8"

# Now list_world_state shows live entries like:
# [{"key": "person.location", "value": "hallway", "confidence": 0.92, ...}]
```

### Mission Engine

Missions are the unit of persistent agent intent. The model, DAG evaluator, and MCP control tools are fully implemented in v0.0.11. The file-watcher is implemented and tested but **not yet wired into the main agent runtime loop** — missions are currently inserted programmatically or via the MCP platform layer.

**Mission lifecycle** (`daemon/mission.rs`):
- States: `Active → Paused → Resumed → Cancelled | Completed | Failed`
- Each mission is a markdown string stored in SQLite with optional `resources` (JSON array of locked actuator IDs), `depends_on` (other mission IDs), and `expires_at` (Unix timestamp)
- Filename stem becomes the mission ID: `dog-monitor.md` → ID `"dog-monitor"`

**MCP control tools** (all require a `mission_id`):
```
list_missions
→ [{"id": "security-patrol", "status": "active", "compiled_at": 1772990000, ...}]

pause_mission   mission_id="security-patrol"   → "Mission security-patrol paused"
resume_mission  mission_id="security-patrol"   → "Mission security-patrol resumed"
cancel_mission  mission_id="security-patrol"   → "Mission security-patrol cancelled"

# Unknown mission ID → graceful error:
pause_mission   mission_id="nonexistent"       → "Error: mission not found"
```

**Mission DAG** (`DagEvaluator::ready_missions()`): resolves `depends_on` dependencies — a mission only becomes `Active` when all its dependencies are `Completed`. `run_micro_turn()` validates sub-mission preconditions with a single LLM call (no tools, no episodic write) before activation.

**Safety Layer** (`daemon/constraints.rs`):

```
# Register a workspace constraint for a mission (fail-closed)
register_constraint
  mission_id="robot-arm-task"
  constraint_type="workspace"
  params_json='{"x": [-1.0, 1.0], "y": [-1.0, 1.0], "z": [0.0, 2.0]}'

# Other constraint types:
#   max_velocity    params_json='1.5'                          (m/s)
#   forbidden_zone  params_json='{"center":[0,0,0],"radius":0.3}'
#   max_force       params_json='50.0'                         (N)

list_constraints mission_id="robot-arm-task"
→ [{"constraint": {"Workspace": {"x": [-1.0, 1.0], "y": [-1.0, 1.0], "z": [0.0, 2.0]}}}]
```

`ConstraintEngine` validates position goals before any actuator command (Allow | Deny | ValidatorError). `ResourceRegistry` holds exclusive actuator locks with `Drop`-guard release. `CompiledFallback`: `StopActuators | PauseAllMissions | AlertAgent | HaltAndWait` — NO arbitrary Zenoh publish variant.

**Reactive Pre-filter** (`daemon/reactive.rs`):

```
# Spike arousal when toddler is near stairs — no LLM involved
register_alert
  mission_id="childproof-home"
  predicate="toddler.near_stairs = 'true'"
```

Rule engine fires in milliseconds with per-rule debounce (`AtomicI64 last_fired_at`).

**Federated Agents** (`daemon/federated.rs`): World state gossip over Zenoh. Remote entries namespaced `remote:{machine_id}.{key}`. Pattern: `bubbaloop/**/agent/*/world_state`.

### Safety Invariants

| ID | Invariant | Enforcer |
|----|-----------|----------|
| I2 | Constraint violations synchronously rejected before any actuator command | `ConstraintEngine::validate_position_goal()` |
| I3 | Fallback actions cannot publish arbitrary Zenoh payloads | `CompiledFallback` enum — no `PublishZenoh` variant |
| I5 | rusqlite `Connection` never shared across async boundaries | `block_in_place()` for all DB calls |
| I6 | Resource locks released on drop | `ResourceGuard` Drop impl |

---

## Telemetry Watchdog

The daemon runs a cross-platform resource watchdog (`daemon/telemetry/`) that prevents OOM crashes on edge devices. Uses the `sysinfo` crate (Linux ARM/x86, macOS).

**Architecture:**
- **Sampler** — Adaptive sysinfo reads (5-30s based on pressure), feeds in-memory ring buffer
- **Circuit Breaker** — Hard safety net, kills nodes at Red/Critical thresholds (~100ms, no LLM needed)
- **Storage** — SQLite cold store (`~/.bubbaloop/telemetry.db`), 7-day retention, batch flush
- **Agent Bridge** — 3 dispatch tools + alert events + system prompt injection

**Threshold levels:**

| Level | Memory Used | Action |
|-------|------------|--------|
| Green | < 60% | Normal (30s sampling) |
| Yellow | 60-80% | Warn agent (30s sampling) |
| Orange | 80-90% | Urgent alert (10s sampling) |
| Red | 90-95% | Kill largest non-essential node (10s sampling) |
| Critical | > 95% | Kill ALL non-essential nodes (5s sampling) |

**Agent tools:** `get_system_telemetry`, `get_telemetry_history`, `update_telemetry_config`

**Hot-reload:** Config at `~/.bubbaloop/telemetry.toml`, file-watched with guardrails (critical threshold 80-98%, min sampling 2s). Agent can tune thresholds at runtime.

**Design doc:** `docs/plans/2026-03-05-telemetry-watchdog-design.md`

---

## Topic Hierarchy

```
bubbaloop/{scope}/{machine_id}/{node_name}/{...}
         ┬─────┬  ┬──────────┬  ┬────────┬
         │     │  │          │  │        └─ Node-specific paths
         │     │  │          │  └────────── Node identifier (1-64 chars, [a-zA-Z0-9_-])
         │     │  │          └───────────── Machine identifier (e.g., nvidia_orin00)
         │     │  └──────────────────────── Scope (local/edge/cloud)
         │     └─────────────────────────── Namespace prefix
         └───────────────────────────────── Project prefix
```

### Environment Variables (Node Runtime)

Every node receives (injected by daemon via systemd unit files):

```bash
BUBBALOOP_SCOPE=local
BUBBALOOP_MACHINE_ID=nvidia_orin00
BUBBALOOP_ZENOH_ENDPOINT=tcp/127.0.0.1:7447  # Optional override
```

---

## MCP Server

MCP is the **sole control interface**. 39 MCP tools + 10 agent-internal (49 total) across categories:

| Category | Tools |
|----------|-------|
| **Discovery** | list_nodes, discover_nodes, get_node_health, get_node_config, get_node_manifest, get_node_schema, get_stream_info, list_commands, discover_capabilities |
| **Lifecycle** | install_node, uninstall_node, start_node, stop_node, restart_node, build_node, remove_node, clean_node, enable_autostart, disable_autostart |
| **Data** | send_command, query_zenoh |
| **System** | get_system_status, get_machine_info |
| **Memory** | list_jobs, delete_job, list_proposals, clear_episodic_memory |
| **Beliefs** | update_belief, get_belief — durable subject+predicate assertions with confidence tracking |
| **World State** | list_world_state — live sensor-derived snapshot injected into every agent turn |
| **Context Providers** | configure_context — wire Zenoh topic → world state (no LLM); topic_pattern + value_field + optional filter |
| **Missions** | list_missions, pause_mission, resume_mission, cancel_mission — YAML-file-driven (`~/.bubbaloop/agents/{id}/missions/`) |
| **Constraints** | register_constraint, list_constraints — per-mission safety limits; params_json formats: `workspace={"x":[-1,1],"y":[-1,1],"z":[0,2]}`, `max_velocity=1.5`, `forbidden_zone={"center":[0,0,0],"radius":0.3}`, `max_force=50.0` |
| **Alerts** | register_alert, unregister_alert — reactive arousal triggers when world state predicate matches |
| **Agent-internal** | read_file, write_file, run_command, memory_search, memory_forget, schedule_task, create_proposal, get_system_telemetry, get_telemetry_history, update_telemetry_config |

### Transport Options

| Transport | Use Case | Auth |
|-----------|----------|------|
| **stdio** | `bubbaloop mcp --stdio` — local agents (Claude Code) | None (inherits user permissions) |
| **HTTP** | Daemon on :8088 — dashboards, remote clients | Bearer token (`~/.bubbaloop/mcp-token`) |

### RBAC

| Role | Allowed Operations |
|------|-------------------|
| **Viewer** | Discovery, read-only data |
| **Operator** | Viewer + lifecycle (start/stop/restart), send_command, config |
| **Admin** | Operator + install/uninstall, system, machine config |

**Default for stdio:** Admin. **Default for HTTP:** Viewer.

### Tool Design Philosophy

**Generic tools, not per-node tools.** AI tool selection degrades above ~40 tools, so per-node tools would create a combinatorial explosion. Instead: nodes self-describe via manifests, `send_command` dispatches to any node's command interface, agent reads manifest to discover available commands.

### Streaming Data Access

**MCP provides metadata; Zenoh provides data streams.** MCP/JSON is too slow for 30 FPS video or 1 kHz IMU data. Agents use `get_stream_info` to learn Zenoh topics, then connect directly to Zenoh for high-frequency streams.

---

## Security Layers (Defense in Depth)

### Layer 1: MCP Authentication & Authorization

| Feature | Implementation |
|---------|----------------|
| **Bearer token auth** | Token at `~/.bubbaloop/mcp-token`, verified on every request |
| **RBAC tiers** | Viewer/Operator/Admin with tool-level permissions |
| **Rate limiting** | tower-governor middleware, per-client limits |
| **Transport security** | HTTP on localhost only (127.0.0.1:8088) or stdio |
| **Audit logging** | All MCP requests logged with client identity and timestamp |
| **Dispatch security** | `dispatch_security.rs` validates RBAC before every tool call |

### Authentication Methods

| Method | How | Stored At |
|--------|-----|-----------|
| **API Key** | `bubbaloop login` → option 1 → paste key | `~/.bubbaloop/anthropic-key` (0600) |
| **Claude Subscription** | `claude setup-token` → `bubbaloop login` → option 2 → paste token | `~/.bubbaloop/oauth-credentials.json` (0600) |
| **Environment** | `ANTHROPIC_API_KEY` env var (highest priority) | Process environment |

**OAuth tokens** (`sk-ant-oat01-*`) use Claude CLI identity headers (`user-agent: claude-cli`, `anthropic-beta` betas). Resolution order: env var → OAuth → API key file.

### Layer 2: Rust Memory Safety (Compile-Time)

| Guarantee | How |
|-----------|-----|
| No buffer overflows | Rust ownership model, no `unsafe` |
| No null pointer crashes | `Option<T>` / `Result<T>` |
| No data races | Borrow checker, `Send`/`Sync` traits |
| Input validation | Node name regex `[a-zA-Z0-9_-]{1,64}`, no null bytes |
| Command allowlist | Build commands: `cargo`, `pixi`, `npm`, `make`, `python`, `pip` only |
| Path safety | `find_curl()` searches `/usr/bin`, `/usr/local/bin`, `/bin` only |

### Layer 3: Zenoh Transport Security

- **mTLS**: Both sides authenticate. Prevents unauthorized peers.
- **Per-key ACLs**: Each node sandboxed to its own key prefix.
- **Known limitation**: Zenoh uses hop-by-hop TLS (not end-to-end).

### Layer 4: Application-Level Security

| Threat | Mitigation |
|--------|------------|
| Malicious node registration | Daemon validates node name, source repo, checksums |
| Config injection | Node configs validated against schema before apply |
| Log injection | All logs via `log` macros to stderr, no user-controlled format strings |
| Supply chain | `bubbaloop-schemas` is a separate crate, NOT in workspace |
| MCP token leakage | Token file 0600 permissions, never logged or transmitted |

---

## Key Technology Choices

| Component | Technology | Why |
|-----------|------------|-----|
| Runtime | Rust + Tokio | Memory safety, small binary, edge-ready |
| Data plane | Zenoh | Zero-copy pub/sub, decentralized, Rust-native |
| Schemas | Protobuf + prost | Self-describing, runtime introspection |
| Control | MCP (rmcp) | Standard AI agent interface, 39 MCP tools + 10 agent-internal |
| Memory | SQLite (rusqlite) + NDJSON | 4-tier: world state (live SQLite) + RAM + episodic (NDJSON/FTS5) + semantic (SQLite). World state updated by context providers, not LLM. |
| CLI | argh | Minimal, fast compile |
| Logging | log + env_logger | Simple, stderr-only |
| systemd | zbus (D-Bus) | No subprocess spawning, safe |
| LLM | ModelProvider trait (reqwest) | Claude (OAuth + API key) and Ollama (local, tool calling) |
| HTTP | axum | Dashboard + MCP HTTP transport |
| Telemetry | sysinfo | CPU/RAM/disk monitoring, cross-platform |

**Removed technologies:**
- **TUI (ratatui/crossterm)** — Removed in v0.0.6. Codebase simplified to ~14K lines.
- **Zenoh API queryables** — All access through MCP. Zenoh is data plane only.
- **Agent rule engine** — Replaced by two-tier scheduling (planned).

---

## Maintaining This Document

- **Update when**: Layers change, node contract changes, security model changes, technology choices change
- **Keep under 300 lines** — link to design docs for details
- **Related files**:
  - `ROADMAP.md` — implementation phases
  - `CONTRIBUTING.md` — workflows and processes
  - `CLAUDE.md` — coding conventions and build commands
  - `docs/plans/2026-03-03-openclaw-agent-rewrite-design.md` — OpenClaw agent rewrite design
  - `docs/plans/2026-02-27-hardware-ai-agent-design.md` — full agent design
