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
│  LAYER 4: CONSUMERS (replaceable)                               │
│  AI Agent (MCP) │ Dashboard (React) │ CLI (argh) │ Any Zenoh   │
│  client                                                         │
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

**Key principle**: Nodes are autonomous. An AI agent can discover, understand, and consume any node directly — the daemon is optional for discovery, required only for lifecycle operations.

---

## Node Contract

Every sensor node MUST implement these standard queryables:

```
bubbaloop/{scope}/{machine_id}/{node_name}/schema      → FileDescriptorSet bytes
bubbaloop/{scope}/{machine_id}/{node_name}/manifest    → JSON manifest
bubbaloop/{scope}/{machine_id}/{node_name}/health      → "ok" | error details
bubbaloop/{scope}/{machine_id}/{node_name}/config      → JSON config (GET/SET)
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
  "schema_key": "bubbaloop/local/nvidia_orin00/network-monitor/schema",
  "health_key": "bubbaloop/local/nvidia_orin00/network-monitor/health",
  "security": {
    "acl_prefix": "bubbaloop/*/nvidia_orin00/network-monitor/**"
  }
}
```

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

## Key Technology Choices

| Choice | Reason |
|--------|--------|
| **Rust for all core components** | Memory safety without GC. No buffer overflows, no use-after-free. Critical for systems that control physical hardware — a segfault in a motor controller is a safety hazard. |
| **Python only for rapid prototyping nodes** | Python nodes are the "onramp" — quick to write, easy to iterate. But production nodes should graduate to Rust. |
| **Zenoh (not DDS, not MQTT)** | Decentralized pub/sub/query with 97% less discovery traffic than DDS. Written in Rust. Zero-copy shared memory. ACLs. Runs cloud-to-edge-to-thing. |
| **ros-z (ZettaScale)** | ROS 2 message type interop without the DDS baggage. `MessageTypeInfo` trait gives us runtime schema introspection. Protobuf feature enables prost integration. |
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
