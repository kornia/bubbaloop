# Architecture

How the pieces fit together.

---

## Layer Diagram

```
  CLI / Dashboard / MCP Client
           |
           | Zenoh pub/sub
           |
  +---------+------------------------------------------------+
  |  Daemon (~12-13 MB single binary)                        |
  |                                                          |
  |  Agent Runtime  | MCP Server | Node Manager              |
  |  (incl. Gateway)|              |                          |
  |  Telemetry Watchdog                                      |
  +---------+------------------------------------------------+
           |
           | Zenoh pub/sub
           |
  +--------+--------+--------+
  |        |        |        |
  Camera   IMU    Motor   Weather     (self-describing nodes)
```

One binary. Four subsystems. One data plane.

---

## Three Entry Points

All three share the same daemon-side agent runtime and MCP tools.

**CLI** (`bubbaloop agent chat`)
- Thin Zenoh client. No LLM on the CLI side.
- Publishes to the agent's inbox topic, subscribes to its outbox.
- All LLM processing happens inside the daemon.

**MCP stdio** (`bubbaloop mcp --stdio`)
- For Claude Code and local AI agents.
- No auth required — inherits user permissions (Admin tier).
- Launch: add `bubbaloop mcp --stdio` to your Claude Code config.

**MCP HTTP** (daemon auto-starts on `:8088`)
- For remote agents, dashboards, and external integrations.
- Bearer token auth (`~/.bubbaloop/mcp-token`).
- Localhost only — never binds `0.0.0.0`.

---

## Agent Runtime

The daemon hosts a multi-agent Zenoh gateway. Agents run entirely daemon-side.

**Gateway protocol**

```
CLI client                Daemon (agent runtime)
    |                            |
    |-- publish to inbox ------->|
    |                     [agent loop]
    |                     [LLM turn]
    |                     [tool dispatch]
    |<-- publish to outbox ------|
```

- Shared inbox topic: `bubbaloop/global/{machine_id}/agent/inbox`
- Per-agent outbox: `bubbaloop/global/{machine_id}/agent/{agent_id}/outbox`
- Wire format: JSON (`AgentMessage`, `AgentEvent`)

**Per-agent state**

Each agent has:
- `Soul` — identity.md + capabilities.toml in `~/.bubbaloop/agents/{id}/soul/`
- 4-tier Memory — world state (live SQLite), short-term (RAM), episodic (NDJSON + FTS5), semantic (SQLite)
- Adaptive heartbeat — arousal rises on activity, decays at rest

**Configuration**

Agents defined in `~/.bubbaloop/agents.toml`. Per-agent files at
`~/.bubbaloop/agents/{id}/`. Soul hot-reloads — changes take effect on the next turn.

---

## Daemon

Five subsystems run concurrently inside a single async runtime.

**Node Manager** — builds, installs, starts/stops, and health-monitors nodes via
systemd. Uses D-Bus (`zbus`) — no subprocess spawning.

**Agent Runtime** — multi-agent Zenoh gateway. Described above.

**MCP Server** — 42 MCP tools + agent-internal tools, 3-tier RBAC, stdio + HTTP transports.

**Telemetry Watchdog**

Monitors CPU, RAM, disk on ARM64/x86. Five severity levels drive adaptive
sampling and automatic circuit breakers.

```
Green  < 60% RAM  — normal (30s sampling)
Yellow  60-80%    — warn agent (30s sampling)
Orange  80-90%    — urgent alert (10s sampling)
Red     90-95%    — kill largest non-essential node (10s sampling)
Critical > 95%    — kill ALL non-essential nodes (5s sampling)
```

Config at `~/.bubbaloop/telemetry.toml`. File-watched. Agent can tune thresholds at
runtime via `update_telemetry_config`.

---

## MCP Server

The sole control interface. Zenoh is the data plane only.

**42 MCP tools + agent-internal tools across 11 categories**

| Category | What it covers |
|---|---|
| Discovery | list nodes, health, config, manifest, schema, logs, stream info, commands, capabilities |
| Lifecycle | install, uninstall, start, stop, restart, build, remove, clean, autostart |
| Data | send command, query Zenoh |
| System | system status, machine info |
| Memory | jobs, proposals, clear episodic memory |
| Beliefs | durable subject+predicate assertions with confidence |
| World State | live sensor-derived snapshot |
| Context Providers | wire Zenoh topics → world state |
| Missions | list, pause, resume, cancel |
| Constraints | register and list per-mission safety limits |
| Alerts | register/unregister reactive arousal triggers |

**3-tier RBAC**

| Role | Permissions |
|---|---|
| Viewer | Discovery and read-only data |
| Operator | Viewer + lifecycle, send_command, config writes |
| Admin | Operator + install/uninstall, system tools |

Default for stdio: Admin. Default for HTTP: Viewer.

**Dual-plane design**

MCP handles control and metadata. Zenoh handles data streams. Agents use
`get_stream_info` to learn the Zenoh topic for a node, then subscribe directly
for high-frequency data (video, IMU, etc.).

---

## Node Contract

Every node serves five standard queryables. Short paths for readability — full
paths follow the topic hierarchy below.

```
{node}/schema      -> Protobuf FileDescriptorSet (binary)
{node}/manifest    -> Capabilities, topics, commands (JSON)
{node}/health      -> Status and uptime (JSON)
{node}/config      -> Current configuration (JSON)
{node}/command     -> Imperative actions (JSON request/response)
```

The Node SDK provides `schema` and `health` queryables automatically. `manifest`, `config`, and `command` are conventions that nodes implement as needed.

AI agents discover nodes via the `bubbaloop/**/manifest` wildcard. They read
the manifest to find available commands, then call `send_command` to act.

**Schema rules**

- Reply with raw FileDescriptorSet bytes, not JSON.
- Never use `.complete(true)` (Rust) or `complete=True` (Python) — blocks wildcard queries.
- Python: `query.key_expr` is a property, not a method.

---

## Security Layers

Four layers. Brief summary — see [ARCHITECTURE.md](../../ARCHITECTURE.md) for full details.

**Input validation**
Node names: `[a-zA-Z0-9_-]`, 1-64 chars, no null bytes. Build command allowlist:
`cargo`, `pixi`, `npm`, `make`, `python`, `pip` only. `find_curl()` searches
`/usr/bin`, `/usr/local/bin`, `/bin` only — never PATH.

**RBAC**
Viewer/Operator/Admin tiers enforced at the MCP tool level. Unknown tools default
to Admin. All requests audit-logged.

**Build sandboxing**
Node builds run under the allowlist. Git clone always uses `--` separator.
`bubbaloop-schemas` is a separate crate, not in the workspace — no supply chain
cross-contamination.

**Network isolation**
HTTP and MCP bind localhost only. Zenoh supports mTLS with per-key ACLs. Bearer
token at `~/.bubbaloop/mcp-token` (0600 permissions, never logged).

---

## Data Flow

```
Node --> Protobuf --> Zenoh --> WebSocket Bridge --> Browser / Dashboard
                          \--> Agent (via Zenoh subscription)
```

1. Node serializes data to Protobuf and publishes to its Zenoh topic.
2. Dashboard subscribes via WebSocket bridge, decodes with schema registry.
3. Agents subscribe directly to Zenoh or query via MCP tools.

---

## Topic Hierarchy

```
bubbaloop/{key_space}/{machine_id}/{node_name}/{resource}
           |          |            |            |
           |          |            |            +-- data topic, health, schema
           |          |            +--------------- node identifier  [a-zA-Z0-9_-], 1-64 chars
           |          +---------------------------- machine ID (e.g., nvidia_orin00)
           +--------------------------------------- global (network) or local (SHM-only)
```

Nodes receive machine ID and Zenoh endpoint via environment variables
injected by the daemon into systemd unit files:

```
BUBBALOOP_MACHINE_ID=nvidia_orin00
BUBBALOOP_ZENOH_ENDPOINT=tcp/127.0.0.1:7447
```

---

## Next Steps

- [Messaging](messaging.md) — Zenoh pub/sub patterns and topic conventions
- [Memory](memory.md) — 4-tier agent memory: world state, short-term, episodic, semantic
- [ARCHITECTURE.md](../../ARCHITECTURE.md) — Full security model, technology choices, design rationale
