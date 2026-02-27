<!-- LIVING DOCUMENT: Update when architecture changes. Source of truth for design decisions. -->

# Bubbaloop Architecture

The open-source AI agent that talks to your cameras, sensors, and robots.
Single Rust binary (~12-13 MB). Runs on Jetson, Raspberry Pi, any Linux.

## Design DNA

### The Steinberger Principle

> *"Perhaps only apps that rely on specific hardware sensors will remain."*
> — Peter Steinberger, OpenClaw creator (Feb 2026)

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
│  │  Agent Layer (planned)                              │  │
│  │  Claude API | Chat | Scheduler (offline Tier 1)     │  │
│  └──────────────────────┬─────────────────────────────┘  │
│  ┌──────────────────────┴─────────────────────────────┐  │
│  │  Memory (planned: SQLite, +1-2 MB)                  │  │
│  │  Conversations | Sensor events | Schedules          │  │
│  └──────────────────────┬─────────────────────────────┘  │
│  ┌──────────────────────┴─────────────────────────────┐  │
│  │  MCP Server (23+ tools) — sole control interface    │  │
│  │  RBAC (Viewer/Operator/Admin) | Bearer token auth   │  │
│  │  PlatformOperations trait | Rate limiting            │  │
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
- `bubbaloop agent` — self-contained hardware AI agent (planned)
- `bubbaloop mcp --stdio` — MCP server for Claude Code / external agents

**Key principle**: Nodes are autonomous and self-describing. The MCP server is the sole control interface — Zenoh is the data plane only. The agent layer adds natural-language hardware control on top.

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

MCP is the **sole control interface**. 23+ generic tools across 5 categories:

| Category | Tools |
|----------|-------|
| **Discovery** | list_nodes, get_node_detail, get_node_schema, get_stream_info, discover_nodes, get_node_manifest, list_commands |
| **Lifecycle** | install_node, uninstall_node, start_node, stop_node, restart_node, build_node, enable_autostart, disable_autostart |
| **Data** | read_sensor, send_command, query_zenoh |
| **Config** | get_node_config, set_node_config |
| **System** | get_system_status, get_machine_info, doctor |

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
| Control | MCP (rmcp) | Standard AI agent interface, 23+ tools |
| Memory | SQLite (rusqlite, planned) | Embedded, +1-2 MB, battle-tested |
| CLI | argh | Minimal, fast compile |
| Logging | log + env_logger | Simple, stderr-only |
| systemd | zbus (D-Bus) | No subprocess spawning, safe |
| LLM | Claude API (reqwest, planned) | Best tool-use, zero new deps |
| HTTP | axum | Dashboard + MCP HTTP transport |

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
  - `docs/plans/2026-02-27-hardware-ai-agent-design.md` — full agent design
