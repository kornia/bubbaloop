<!-- LIVING DOCUMENT: Claude and contributors should update this when architecture changes.
     Source of truth for architecture decisions. See ROADMAP.md for timeline. -->

# Bubbaloop Architecture

Physical AI orchestration built on Zenoh. Sensor-centric design.

## Design DNA

### The Steinberger Principle

> *"Perhaps only apps that rely on specific hardware sensors will remain."*
> — Peter Steinberger, OpenClaw creator (Feb 2026)

**Core thesis**: 80% of software apps will be replaced by AI agents. The surviving 20% are those that **interface with physical reality** — sensors, actuators, hardware.

Therefore:

1. **Sensor nodes are the product** — not the daemon, not the dashboard, not the CLI
2. **The daemon is scaffolding** — useful today, replaceable tomorrow by an AI agent
3. **The dashboard is a viewer** — any Zenoh client can replace it
4. **Data access rights are the moat** — who controls the sensors controls the value
5. **Self-describing nodes are AI-native** — an AI agent should discover and use any node without documentation

### The Steinberger Test

Every design decision must pass this test:

**Does this make the sensor/hardware layer stronger, or does it add app-layer complexity that AI agents will replace?**

If it's app-layer complexity → reject it. If it strengthens sensor nodes → accept it.

---

## Layer Model

```
┌─────────────────────────────────────────────────────────────────┐
│  LAYER 5: AI AGENTS (OpenClaw, Claude, custom)                   │
│  MCP client │ Control plane │ Optional Zenoh for data streams    │
└──────────────────────────┬──────────────────────────────────────┘
                           │ MCP (http://127.0.0.1:8088/mcp)
┌──────────────────────────┴──────────────────────────────────────┐
│  LAYER 4: CONSUMERS + MCP AGGREGATOR                             │
│  Dashboard (React) │ CLI (argh) │ MCP Server (daemon)            │
│  ~15-20 generic tools │ Manifest-driven discovery                │
└──────────────────────────┬──────────────────────────────────────┘
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
│  LAYER 2: SENSOR NODES (the product — self-describing, autonomous)
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
│  LAYER 1: LIFECYCLE (thin, optional)                            │
│  Daemon: install │ start │ stop │ update                        │
│  (systemd integration, marketplace, build queue)                │
└─────────────────────────────────────────────────────────────────┘
```

**Key principle**: Nodes are autonomous and self-describing. AI agents discover capabilities via manifests and interact via the daemon's MCP tools. The daemon is scaffolding that translates between MCP (control plane) and Zenoh (data plane).

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

```json
{
  "name": "network-monitor",
  "version": "0.2.0",
  "language": "rust",
  "description": "Network connectivity monitor",
  "machine_id": "nvidia_orin00",
  "scope": "local",
  "capabilities": ["sensor", "health-check"],
  "requires_hardware": ["network"],
  "publishes": [
    {
      "topic_suffix": "network-monitor/status",
      "full_topic": "bubbaloop/local/nvidia_orin00/network-monitor/status",
      "message_type": "bubbaloop.network_monitor.NetworkStatus",
      "rate_hz": 0.1,
      "qos": { "reliability": "best_effort", "durability": "volatile" }
    }
  ],
  "commands": [
    {
      "name": "ping_host",
      "description": "Ping a specific host and return latency",
      "parameters": { "host": "string", "count": "integer" },
      "returns": "object"
    }
  ],
  "schema_key": "bubbaloop/local/nvidia_orin00/network-monitor/schema",
  "health_key": "bubbaloop/local/nvidia_orin00/network-monitor/health",
  "config_key": "bubbaloop/local/nvidia_orin00/network-monitor/config",
  "command_key": "bubbaloop/local/nvidia_orin00/network-monitor/command",
  "security": {
    "acl_prefix": "bubbaloop/*/nvidia_orin00/network-monitor/**"
  }
}
```

### Schema Contract (Protobuf Nodes)

Every node that publishes protobuf messages **MUST** serve its FileDescriptorSet via a Zenoh queryable. This enables runtime schema discovery for dashboards, AI agents, and cross-node type checking.

**Requirements:**

1. **Declare schema queryable** at `{node-name}/schema` (relative to topic prefix):
   ```rust
   // Rust: NEVER use .complete(true) — blocks wildcard discovery
   let schema_queryable = session
       .declare_queryable(format!("{}/schema", topic_prefix))
       .await?;

   // Python: NEVER use complete=True
   queryable = session.declare_queryable(f"{topic_prefix}/schema")
   ```

2. **Serve FileDescriptorSet bytes** (not JSON):
   ```rust
   // Rust: include compiled descriptor
   pub const DESCRIPTOR: &[u8] = include_bytes!(concat!(env!("OUT_DIR"), "/descriptor.bin"));
   query.reply(query.key_expr().clone(), DESCRIPTOR).await?;

   // Python: reply with query.key_expr property (NOT method)
   with open("descriptor.bin", "rb") as f:
       descriptor_bytes = f.read()
   query.reply(query.key_expr, descriptor_bytes)  # key_expr is a PROPERTY
   ```

3. **Compile descriptor.bin** via `build.rs` (Rust) or protoc (Python):
   ```rust
   // build.rs
   prost_build::Config::new()
       .file_descriptor_set_path(out_dir.join("descriptor.bin"))
       .compile_protos(&["protos/my_node.proto", "protos/header.proto"], &["protos/"])?;
   ```

4. **Include all .proto files** the node uses (including `header.proto` from bubbaloop-schemas):
   - Copy header.proto into node's `protos/` directory
   - Reference it in your message definitions: `import "header.proto";`

**Why this matters:**

- Dashboard auto-discovers all schemas via wildcard query `bubbaloop/**/schema`
- AI agents can introspect message types without reading source code
- Cross-node type safety: verify sender/receiver compatibility at runtime
- Version detection: dashboard can warn about schema mismatches

**Common mistakes** (caught by `./scripts/validate.sh`):

- ❌ Using `.complete(true)` in Rust queryables
- ❌ Using `complete=True` in Python queryables
- ❌ Using `query.key_expr()` as method in Python (it's a property)
- ❌ Serving JSON instead of raw FileDescriptorSet bytes
- ❌ Missing `header.proto` in FileDescriptorSet

### Command Contract (Actuation Nodes)

Nodes that support imperative actions MUST declare a command queryable at `{topic_prefix}/command`. This enables AI agents, CLI tools, and other nodes to trigger actions.

**Protocol:**

- **Query with no payload** → returns list of available commands
- **Query with JSON payload** → executes the command

```json
// Request
{"command": "capture_frame", "params": {"resolution": "1080p"}}

// Success response
{"result": "frame captured", "error": null}

// Error response
{"result": null, "error": "Unknown command 'foo'. Available: [\"capture_frame\"]"}
```

**Manifest integration:** Commands are declared in the manifest `commands` field so AI agents can discover capabilities before sending commands. The `command_key` field points to the queryable.

**ACL implications:** The command queryable accepts `put` operations, making it a **write** endpoint. ACL rules must distinguish between:
- **Read** (subscriber/get): dashboard, monitors
- **Write** (put to command): AI agent, CLI, authorized controllers

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

### Layer 1: Rust Memory Safety (Compile-Time)

| Guarantee | How |
|-----------|-----|
| No buffer overflows | Rust ownership model, no `unsafe` |
| No null pointer crashes | `Option<T>` / `Result<T>` — no null |
| No data races | Rust borrow checker, `Send`/`Sync` traits |
| No use-after-free | Ownership + lifetimes |
| Input validation | Node name regex `[a-zA-Z0-9_-]{1,64}`, no null bytes |
| Command allowlist | Build commands: `cargo`, `pixi`, `npm`, `make`, `python`, `pip` only |
| Path safety | `find_curl()` searches `/usr/bin`, `/usr/local/bin`, `/bin` only |

**Python nodes inherit NONE of these guarantees** — production nodes should graduate to Rust.

### Layer 2: Zenoh Transport Security (Network-Level)

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

### Layer 3: Zenoh Access Control (Per-Key ACLs)

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

### Layer 4: Application-Level Security

| Threat | Mitigation |
|--------|------------|
| Malicious node registration | Daemon validates node name, source repo, checksums before install |
| Config injection | Node configs validated against schema before apply |
| Prototype escalation | Python nodes only start if explicitly enabled in daemon config |
| Log injection | All logs via `log` macros to stderr, never user-controlled format strings |
| Supply chain | `bubbaloop-schemas` is a separate crate, NOT in workspace — explicit dependency |

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

Bubbaloop serves as the physical AI foundation for OpenClaw and other AI agents.

**Dual-plane access:**
- **MCP** (control plane): discover nodes, send commands, manage lifecycle, program rules
- **Zenoh** (data plane): subscribe to high-frequency sensor streams directly

**Convention alignment:** AI agents connecting to bubbaloop should:
1. Use MCP tools for all control operations (never spawn processes)
2. Call `list_nodes` → `list_commands` → `send_command` workflow
3. Use `query_zenoh` for one-off sensor reads
4. Connect to Zenoh directly only for streaming data (cameras, IMUs)
5. Program the agent rule engine via `add_rule`/`remove_rule` MCP tools

**Tool design principle:** The daemon exposes ~15-20 generic MCP tools. Per-node tools are NOT exposed individually — AI tool selection degrades above ~40 tools. Nodes self-describe via manifests; the `send_command` tool dispatches to any node command.

---

## Key Technology Choices

| Choice | Reason |
|--------|--------|
| **Rust for all core components** | Memory safety without GC. No buffer overflows, no use-after-free. Critical for systems that control physical hardware — a segfault in a motor controller is a safety hazard. |
| **Python only for rapid prototyping nodes** | Python nodes are the "onramp" — quick to write, easy to iterate. But production nodes should graduate to Rust. |
| **Zenoh (not DDS, not MQTT)** | Decentralized pub/sub/query with 97% less discovery traffic than DDS. Written in Rust. Zero-copy shared memory. ACLs. Runs cloud-to-edge-to-thing. |
| **Protobuf + Zenoh queryables** | Self-describing message types via FileDescriptorSet serving. Runtime schema introspection without DDS overhead. Vanilla Zenoh API — no abstraction layers. |
| **argh (not clap)** | Minimal CLI parsing. No proc macros, fast compile times. |
| **log + env_logger (not tracing)** | Simple, synchronous logging. Logs to stderr (never pollutes stdout). |
| **thiserror + anyhow** | `thiserror` for library errors, `anyhow` for CLI. Module-specific `type Result<T> = std::result::Result<T, Error>`. |
| **zbus (not systemctl subprocess)** | Direct D-Bus integration for systemd. No shell injection risk. |

---

## Maintaining This Document

- **Update when**: New layers added, node contract changes, security model changes, technology choices change
- **Keep under 200 lines** — link to `.omc/plans/` for full details
- **Run `/validate` after changes** to verify contract consistency
- **Related files**:
  - `ROADMAP.md` — timeline and migration phases
  - `CONTRIBUTING.md` — workflows and processes
  - `CLAUDE.md` — coding conventions and build commands
  - `.omc/plans/sensor-centric-redesign.md` — full architectural plan with philosophy and migration details
