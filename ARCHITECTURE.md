<!-- LIVING DOCUMENT: Claude and contributors should update this when architecture changes.
     Source of truth for architecture decisions. See ROADMAP.md for timeline. -->

# Bubbaloop Architecture

Physical AI orchestration built on Zenoh and MCP. Skillet-centric design.

## Design DNA

### The Steinberger Principle

> *"Perhaps only apps that rely on specific hardware sensors will remain."*
> — Peter Steinberger, OpenClaw creator (Feb 2026)

**Core thesis**: 80% of software apps will be replaced by AI agents. The surviving 20% are those that **interface with physical reality** — sensors, actuators, hardware.

Therefore:

1. **Skillets (nodes) are the product** — not the daemon, not the dashboard, not the CLI
2. **The daemon is an MCP gateway** — it translates between AI agents (MCP) and hardware (Zenoh)
3. **The dashboard is a viewer** — any Zenoh client can replace it
4. **Data access rights are the moat** — who controls the sensors controls the value
5. **Self-describing skillets are AI-native** — an AI agent should discover and use any skillet without documentation

### The Steinberger Test

Every design decision must pass this test:

**Does this make the sensor/hardware layer stronger, or does it add app-layer complexity that AI agents will replace?**

If it's app-layer complexity → reject it. If it strengthens skillets (nodes) → accept it.

---

## Layer Model

```
┌─────────────────────────────────────────────────────────────────┐
│  LAYER 5: AI AGENTS (OpenClaw, Claude, custom)                   │
│  MCP client (stdio or HTTP) │ Control plane primary              │
│  Optional direct Zenoh access for high-frequency streams         │
└──────────────────────────┬──────────────────────────────────────┘
                           │ MCP (http://127.0.0.1:8088 or stdio)
╔══════════════════════════╧══════════════════════════════════════╗
║  LAYER 4: MCP GATEWAY (daemon core — passive skill runtime)    ║
║  RBAC (Viewer/Operator/Admin) │ Bearer token auth               ║
║  PlatformOperations trait │ Rate limiting (tower-governor)      ║
║                                                                 ║
║  Discovery: list_nodes, get_node_detail, get_node_schema,      ║
║             get_stream_info, discover_nodes, get_node_manifest, ║
║             list_commands                                       ║
║  Lifecycle: install_node, start_node, stop_node, restart_node, ║
║             build_node                                          ║
║  Data:      read_sensor, send_command, query_zenoh             ║
║  Config:    get_node_config, set_node_config                   ║
║  System:    get_system_status, get_machine_info, doctor        ║
║                                                                 ║
║  External AI agents control via MCP — no autonomous decisions.  ║
╚══════════════════════════╤══════════════════════════════════════╝
                           │
╔══════════════════════════╧══════════════════════════════════════╗
║  LAYER 3: ZENOH DATA PLANE (decentralized, secure)             ║
║  ┌─────────┐  ┌──────┐  ┌──────┐  ┌─────────┐  ┌───────────┐ ║
║  │ Pub/Sub │  │Query │  │ SHM  │  │   ACL   │  │   mTLS    │ ║
║  │         │  │-able │  │ Zero │  │  Per-key│  │  Per-peer │ ║
║  │         │  │      │  │ Copy │  │  access │  │  identity │ ║
║  └─────────┘  └──────┘  └──────┘  └─────────┘  └───────────┘ ║
╚══════════════════════════╤══════════════════════════════════════╝
                           │
┌──────────────────────────┴──────────────────────────────────────┐
│  LAYER 2: SKILLETS (nodes) — the product, self-describing       │
│                                                                 │
│  ┌─────────────┐  ┌─────────────┐  ┌─────────────┐            │
│  │ Camera Node │  │  Telemetry  │  │   Weather   │ ...        │
│  │   (Rust)    │  │   (Rust)    │  │  (Python)   │            │
│  │             │  │             │  │             │            │
│  │ ┌─────────┐│  │ ┌─────────┐│  │ ┌─────────┐│            │
│  │ │manifest ││  │ │manifest ││  │ │manifest ││            │
│  │ │schema   ││  │ │schema   ││  │ │schema   ││            │
│  │ │health   ││  │ │health   ││  │ │health   ││            │
│  │ │config   ││  │ │config   ││  │ │config   ││            │
│  │ └─────────┘│  │ └─────────┘│  │ └─────────┘│            │
│  └─────────────┘  └─────────────┘  └─────────────┘            │
└─────────────────────────────────────────────────────────────────┘

┌─────────────────────────────────────────────────────────────────┐
│  LAYER 1: LIFECYCLE (thin scaffolding — passive runtime)        │
│  Daemon: install │ start │ stop │ update │ build queue          │
│  (systemd/zbus integration, marketplace, node registry)         │
│  Role: Skill runtime, not autonomous agent.                     │
└─────────────────────────────────────────────────────────────────┘
```

**Key principle**: Skillets (nodes) are autonomous and self-describing. AI agents discover capabilities via MCP tools and interact through the MCP Gateway. The daemon is an MCP-to-Zenoh translator that enables AI agents to control physical hardware without understanding Zenoh internals.

---

## Skillet Contract (Node Contract)

Every sensor skillet (node) MUST implement these standard queryables:

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

Each skillet serves a JSON manifest at `{topic_prefix}/manifest` with:
- **Identity**: name, version, language, description, machine_id, scope
- **Hardware**: capabilities, requires_hardware
- **Data**: publishes (topics, message types, rate, QoS)
- **Control**: commands (name, description, parameters, returns)
- **Queryables**: schema_key, health_key, config_key, command_key
- **Security**: acl_prefix for Zenoh ACL rules

See node templates in `bubbaloop-nodes-official` for full examples.

### Schema Contract (Protobuf Skillets)

Every skillet that publishes protobuf messages MUST serve its FileDescriptorSet via `{node-name}/schema` queryable. This enables runtime schema discovery for dashboards and AI agents.

**Key rules:**
- Compile descriptor.bin via `build.rs` (Rust) or protoc (Python)
- Include all .proto files (including `header.proto` from bubbaloop-schemas)
- NEVER use `.complete(true)` (Rust) or `complete=True` (Python) — blocks wildcard queries
- Reply with raw FileDescriptorSet bytes, not JSON
- Python: `query.key_expr` is a property, not a method

**Why:** Dashboard auto-discovers schemas via `bubbaloop/**/schema` wildcard. AI agents introspect message types without reading source code.

### Command Contract (Actuation Skillets)

Skillets that support imperative actions MUST declare `{topic_prefix}/command` queryable.

**Protocol:**
- Query with no payload → list of available commands
- Query with JSON payload → executes the command

```json
{"command": "capture_frame", "params": {"resolution": "1080p"}}
→ {"result": "frame captured", "error": null}
```

Commands are declared in manifest `commands` field. The command queryable is a **write** endpoint (accepts `put` operations), requiring ACL rules to distinguish read (subscriber/get) vs. write (put to command).

---

## Topic Hierarchy

All topics follow this pattern:

```
bubbaloop/{scope}/{machine_id}/{node_name}/{...}
         ┬─────┬  ┬──────────┬  ┬────────┬
         │     │  │          │  │        └─ Node-specific paths
         │     │  │          │  └────────── Node identifier (1-64 chars, [a-zA-Z0-9_-])
         │     │  │          └───────────── Machine identifier (e.g., nvidia_orin00)
         │     │  └──────────────────────── Scoping for multi-tenant deployments
         │     └─────────────────────────── Namespace prefix
         └───────────────────────────────── Project prefix
```

### Scopes

- `local`: Single-machine, localhost-only
- `edge`: Multi-machine LAN
- `cloud`: Multi-site WAN
- Custom scopes for multi-tenant deployments

### Environment Variables (Node Runtime)

Every node receives:

```bash
BUBBALOOP_SCOPE=local
BUBBALOOP_MACHINE_ID=nvidia_orin00
BUBBALOOP_ZENOH_ENDPOINT=tcp/127.0.0.1:7447  # Optional override
```

These are injected by the daemon via systemd unit files.

---

## Security Layers (Defense in Depth)

### Layer 1: MCP Authentication & Authorization (API Gateway)

| Feature | Implementation |
|---------|----------------|
| **Bearer token auth** | Token stored at `~/.bubbaloop/mcp-token`, verified on every request |
| **RBAC tiers** | Viewer (read-only), Operator (day-to-day), Admin (system modification) |
| **Rate limiting** | tower-governor middleware, per-client limits |
| **Transport security** | HTTP on localhost (127.0.0.1:8088) or stdio for local agents |
| **Tool-level permissions** | Discovery/Data tools → Viewer, Lifecycle → Operator, System → Admin |
| **Audit logging** | All MCP requests logged with client identity and timestamp |

**stdio transport:** Used by local AI agents (e.g., `bubbaloop mcp --stdio`). Inherits user's filesystem permissions. No network exposure.

**HTTP transport:** Used by remote dashboards/clients. Requires bearer token. Always binds to localhost only.

### Layer 2: Rust Memory Safety (Compile-Time)

| Guarantee | How |
|-----------|-----|
| No buffer overflows | Rust ownership model, no `unsafe` |
| No null pointer crashes | `Option<T>` / `Result<T>` — no null |
| No data races | Rust borrow checker, `Send`/`Sync` traits |
| No use-after-free | Ownership + lifetimes |
| Input validation | Node name regex `[a-zA-Z0-9_-]{1,64}`, no null bytes |
| Command allowlist | Build commands: `cargo`, `pixi`, `npm`, `make`, `python`, `pip` only |
| Path safety | `find_curl()` searches `/usr/bin`, `/usr/local/bin`, `/bin` only |

**Python nodes inherit NONE of these guarantees** — production skillets should graduate to Rust.

### Layer 3: Zenoh Transport Security (Network-Level)

```toml
# zenoh-config.json5 — multi-machine production
{
  transport: {
    unicast: {
      tls: {
        root_ca_certificate: "/etc/bubbaloop/ca.pem",
        server_certificate: "/etc/bubbaloop/server.pem",
        server_private_key: "/etc/bubbaloop/server.key"
      }
    }
  }
}
```

**mTLS**: Both sides authenticate. Prevents unauthorized peers from joining the Zenoh network.

**Known limitation**: Zenoh uses hop-by-hop TLS (NOT end-to-end). Mitigation: application-level encryption for sensitive sensor data (e.g., camera feeds).

### Layer 4: Zenoh Access Control (Per-Key ACLs)

```toml
{
  access_control: {
    enabled: true,
    default_permission: "deny",
    rules: [
      {
        id: "network-monitor-publish",
        permission: "allow",
        messages: ["put", "declare_subscriber"],
        key_exprs: ["bubbaloop/*/nvidia_orin00/network-monitor/**"],
        interfaces: ["lo0"]
      },
      {
        id: "dashboard-read-only",
        permission: "allow",
        messages: ["get", "declare_subscriber"],
        key_exprs: ["bubbaloop/**"],
        interfaces: ["lo0"]
      }
    ]
  }
}
```

This ensures:
- A compromised weather node **cannot** publish fake camera data
- The dashboard **cannot** send commands (read-only)
- Each node is sandboxed to its own key prefix

### Layer 5: Application-Level Security

| Threat | Mitigation |
|--------|------------|
| Malicious node registration | Daemon validates node name, source repo, checksums before install |
| Config injection | Node configs validated against schema before apply |
| Prototype escalation | Python nodes only start if explicitly enabled in daemon config |
| Log injection | All logs via `log` macros to stderr, never user-controlled format strings |
| Supply chain | `bubbaloop-schemas` is a separate crate, NOT in workspace — explicit dependency |
| MCP token leakage | Token file has 0600 permissions, never logged or transmitted |

---

## Open-Core Boundary

| Component | Free | Paid |
|-----------|------|------|
| **Sensor nodes** (the product) | All node code, templates, schemas | Certified nodes with vendor SLA |
| **Local runtime** (scaffolding) | CLI, daemon, TUI | — |
| **Single-machine dashboard** (viewer) | Full local dashboard | — |
| **Multi-machine fleet** (ops) | — | Fleet dashboard, cross-machine views |
| **Cloud sync** (ops) | — | Time-series retention, backup |
| **OTA updates** (ops) | — | Rolling/canary deployments |
| **Security management** (ops) | — | mTLS auto-rotation, enterprise ACLs |
| **Analytics** (ops) | — | Fleet health, anomaly detection |

Free tier: **Up to 10 machines per organization** (industry standard).

Paid tiers:
- **Startup**: $99/mo — up to 50 machines
- **Team**: $499/mo — up to 500 machines
- **Enterprise**: Custom pricing — unlimited, on-premise, SLA

---

## OpenClaw Integration

Bubbaloop serves as the **physical AI foundation** for OpenClaw and other AI agents. The MCP-first refactor makes bubbaloop the canonical example of a hardware control server that AI agents can use without custom SDKs.

### Primary Use Case: AI Agent Workflow

**Step-by-step integration:**

1. **Initialize connection** — Agent connects via stdio transport:
   ```bash
   bubbaloop mcp --stdio
   ```
   Inherits user's filesystem permissions. No bearer token needed for local stdio.

2. **Discover hardware** — Agent calls `list_nodes` to see available skillets:
   ```json
   {"jsonrpc": "2.0", "method": "tools/call", "params": {"name": "list_nodes"}}
   ```
   Returns: `["camera-front", "network-monitor", "telemetry"]`

3. **Introspect capabilities** — Agent calls `get_node_detail` for each skillet:
   ```json
   {"name": "get_node_detail", "params": {"node_name": "camera-front"}}
   ```
   Returns: manifest with `commands`, `publishes`, `schema_key`, etc.

4. **Get streaming data info** — Agent calls `get_stream_info` to learn Zenoh topics:
   ```json
   {"name": "get_stream_info", "params": {"node_name": "camera-front"}}
   ```
   Returns: `{"topic": "bubbaloop/local/nvidia_orin00/camera-front/frames", "message_type": "bubbaloop.camera.Frame", "rate_hz": 30}`

5. **Control hardware** — Agent calls `send_command` to actuate:
   ```json
   {"name": "send_command", "params": {"node_name": "camera-front", "command": "capture_frame", "params": {"resolution": "1080p"}}}
   ```
   Returns: `{"result": "frame captured", "error": null}`

6. **Program automation** — Agent calls `add_rule` to create reactive rules:
   ```json
   {"name": "add_rule", "params": {"name": "network-failover", "trigger": "event", "conditions": [{"field": "node_name", "op": "eq", "value": "network-monitor"}], "actions": [{"type": "send_command", "node": "router", "command": "switch_interface"}]}}
   ```

### Transport Options

| Transport | Use Case | Auth | Performance |
|-----------|----------|------|-------------|
| **stdio** | Local AI agents (OpenClaw, Claude Code) | None (inherits user permissions) | Low latency, no network |
| **HTTP** | Remote dashboards, multi-machine agents | Bearer token (`~/.bubbaloop/mcp-token`) | Higher latency, rate-limited |

### RBAC Implications for AI Agents

| Role | Allowed Operations | Typical Agent |
|------|-------------------|---------------|
| **Viewer** | Discovery (list_nodes, get_node_detail, get_stream_info), read-only data | Monitoring agent, dashboard |
| **Operator** | Viewer + Lifecycle (start/stop/restart), send_command, read/write config | Day-to-day automation (OpenClaw) |
| **Admin** | Operator + System (install_node, build_node, doctor, machine config) | System management agent |

**Default for stdio:** Admin role (assumes local user is trusted).
**Default for HTTP:** Viewer role (must upgrade via config for remote clients).

### Automation Pattern

The daemon is a **passive skill runtime** — it does not have an autonomous rule engine. AI agents can program reactive behaviors by:

1. **Direct MCP calls** — Agent continuously monitors state and executes MCP tools
2. **External orchestrators** — OpenClaw, n8n, Home Assistant, etc. subscribe to Zenoh topics and call MCP
3. **Node-level logic** — Skillets can implement local reactive behavior (e.g., camera node auto-adjusts exposure)

**Example:** Auto-restart failed skillets via external agent:
```python
# OpenClaw workflow
while True:
    status = mcp_call("get_system_status")
    for node in status["nodes"]:
        if node["health"] == "unhealthy":
            mcp_call("restart_node", {"node_name": node["name"]})
    await asyncio.sleep(60)
```

### Streaming Data Access

**MCP tools provide metadata; Zenoh provides data streams.**

- **get_stream_info** returns the Zenoh topic for high-frequency data
- Agent connects directly to Zenoh for streams (cameras, IMUs, lidar)
- Agent uses `query_zenoh` MCP tool for one-off reads (current config, health)

**Why not stream through MCP?** MCP/JSON is too slow for 30 FPS video or 1 kHz IMU data. Zenoh has zero-copy shared memory and QoS controls.

### Tool Design Philosophy

**Generic tools, not per-node tools.** The daemon exposes tools across categories (Discovery, Lifecycle, Data, Config, System). AI tool selection degrades above ~40 tools, so per-skillet tools would create a combinatorial explosion.

Instead:
- Skillets self-describe via manifests
- `send_command` dispatches to any skillet's command interface
- Agent reads manifest to know which commands exist
- Keeps MCP tool count manageable (generic tools vs. per-skillet proliferation)

---

## Key Technology Choices

| Choice | Reason |
|--------|--------|
| **Rust for all core components** | Memory safety without GC. No buffer overflows, no use-after-free. Critical for systems that control physical hardware — a segfault in a motor controller is a safety hazard. |
| **Python only for rapid prototyping nodes** | Python nodes are the "onramp" — quick to write, easy to iterate. But production skillets should graduate to Rust. |
| **Zenoh (not DDS, not MQTT)** | Decentralized pub/sub/query with 97% less discovery traffic than DDS. Written in Rust. Zero-copy shared memory. ACLs. Runs cloud-to-edge-to-thing. |
| **Protobuf + Zenoh queryables** | Self-describing message types via FileDescriptorSet serving. Runtime schema introspection without DDS overhead. Vanilla Zenoh API — no abstraction layers. |
| **MCP (Model Context Protocol)** | Standard AI agent interface. 24 generic tools. RBAC + bearer token auth. stdio and HTTP transports. Replaces custom REST APIs and CLI scraping. |
| **rmcp crate** | Rust MCP server implementation. Integrates with axum for HTTP, stdio for local agents. |
| **tower-governor** | Rate limiting middleware for MCP HTTP endpoint. Per-client token bucket algorithm. |
| **argh (not clap)** | Minimal CLI parsing. No proc macros, fast compile times. |
| **log + env_logger (not tracing)** | Simple, synchronous logging. Logs to stderr (never pollutes stdout). |
| **thiserror + anyhow** | `thiserror` for library errors, `anyhow` for CLI. Module-specific `type Result<T> = std::result::Result<T, Error>`. |
| **zbus (not systemctl subprocess)** | Direct D-Bus integration for systemd. No shell injection risk. |
| **PlatformOperations trait** | Abstracts MCP tools from daemon internals. Enables testing with mock implementations. Clean separation between API surface and business logic. |

**Removed technologies:**
- **Zenoh API queryables (zenoh_api.rs deleted)** — All external access now goes through MCP. The daemon no longer exposes `bubbaloop/api/**` queryables. Zenoh is the data plane only; MCP is the control plane.
- **Agent rule engine** — Daemon is a passive skill runtime. External AI agents (OpenClaw, etc.) implement automation logic via MCP.

**Feature flags:**
- **TUI** (`--features tui`) — Terminal UI is optional. Most deployments use headless daemon + MCP.
- **Dashboard** (`dashboard` feature) — Embeds React dashboard via `rust-embed` and `axum`.

---

## Maintaining This Document

- **Update when**: New layers added, skillet contract changes, security model changes, technology choices change, MCP tools added/removed
- **Keep under 450 lines** — link to `.omc/plans/` for full details
- **Run `/validate` after changes** to verify contract consistency
- **Related files**:
  - `ROADMAP.md` — timeline and migration phases
  - `CONTRIBUTING.md` — workflows and processes
  - `CLAUDE.md` — coding conventions and build commands
  - `.omc/plans/mcp-first-refactor.md` — MCP implementation plan and migration details
