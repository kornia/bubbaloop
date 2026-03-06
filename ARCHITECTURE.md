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
│  │  3-Tier Memory                                      │  │
│  │  Short-term (RAM) | Episodic (NDJSON) | Semantic DB │  │
│  └──────────────────────┬─────────────────────────────┘  │
│  ┌──────────────────────┴─────────────────────────────┐  │
│  │  MCP Server (30 tools) — sole control interface      │  │
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

**Three entry points, same core:**
- `bubbaloop agent chat` — thin Zenoh CLI client (LLM runs daemon-side)
- `bubbaloop agent list` — discover running agents via manifest queryables
- `bubbaloop mcp --stdio` — MCP server for Claude Code / external agents

**Key principle**: Nodes are autonomous and self-describing. The MCP server is the sole control interface — Zenoh is the data plane only. The agent runtime runs inside the daemon, with agents configured via `~/.bubbaloop/agents.toml` and per-agent state in `~/.bubbaloop/agents/{id}/`.

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
bubbaloop/{scope}/{machine_id}/{publish_topic}          → Protobuf sensor data
bubbaloop/{scope}/{machine_id}/health/{node_name}       → Periodic heartbeat
```

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

Every node that publishes protobuf messages MUST serve its FileDescriptorSet via `{node-name}/schema` queryable. This enables runtime schema discovery for dashboards and AI agents.

**Key rules:**
- Compile descriptor.bin via `build.rs` (Rust) or protoc (Python)
- Include all .proto files (including `header.proto` from bubbaloop-schemas)
- NEVER use `.complete(true)` (Rust) or `complete=True` (Python) — blocks wildcard queries
- Reply with raw FileDescriptorSet bytes, not JSON
- Python: `query.key_expr` is a property, not a method

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

MCP is the **sole control interface**. 30 MCP tools + 7 agent-internal (37 total) across categories:

| Category | Tools |
|----------|-------|
| **Discovery** | list_nodes, discover_nodes, get_node_health, get_node_config, get_node_manifest, get_node_schema, get_stream_info, list_commands, discover_capabilities |
| **Lifecycle** | install_node, uninstall_node, start_node, stop_node, restart_node, build_node, remove_node, clean_node, enable_autostart, disable_autostart |
| **Data** | send_command, query_zenoh |
| **System** | get_system_status, get_machine_info |
| **Memory** | list_jobs, delete_job, list_proposals |
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
| Control | MCP (rmcp) | Standard AI agent interface, 30 MCP tools + 7 agent-internal |
| Memory | SQLite (rusqlite) + NDJSON | 3-tier: RAM + episodic (NDJSON/FTS5) + semantic (SQLite). Episodic FTS5 index and semantic store share `memory.db` per agent. |
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
