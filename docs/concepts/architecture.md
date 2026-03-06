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
  |  Agent Runtime  |  MCP Server  |  Node Manager           |
  |                 |              |                          |
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

- Shared inbox topic: `bubbaloop/{scope}/{machine_id}/agent/inbox`
- Per-agent outbox: `bubbaloop/{scope}/{machine_id}/agent/{agent_id}/outbox`
- Wire format: JSON (`AgentMessage`, `AgentEvent`)

**Per-agent state**

Each agent has:
- `Soul` — identity.md + capabilities.toml in `~/.bubbaloop/agents/{id}/soul/`
- 3-tier Memory — short-term (RAM), episodic (NDJSON + FTS5), semantic (SQLite)
- Adaptive heartbeat — arousal rises on activity, decays at rest

**Configuration**

Agents defined in `~/.bubbaloop/agents.toml`. Per-agent files at
`~/.bubbaloop/agents/{id}/`. Soul hot-reloads — changes take effect on the next turn.

---

## Daemon

Four subsystems run concurrently inside a single async runtime.

**Node Manager** — builds, installs, starts/stops, and health-monitors nodes via
systemd. Uses D-Bus (`zbus`) — no subprocess spawning.

**Agent Runtime** — multi-agent Zenoh gateway. Described above.

**MCP Server** — 30 MCP tools + 10 agent-internal tools (37 total), 3-tier RBAC, stdio + HTTP transports.

**Telemetry Watchdog**

Monitors CPU, RAM, disk, and GPU on ARM64/x86. Five severity levels drive adaptive
sampling and automatic circuit breakers.

```
Green  < 60% RAM  — normal (30s sampling)
Yellow  60-80%    — warn agent (10s sampling)
Orange  80-90%    — urgent alert (5s sampling)
Red     90-95%    — kill largest non-essential node
Critical > 95%    — kill ALL non-essential nodes
```

Config at `~/.bubbaloop/telemetry.toml`. File-watched. Agent can tune thresholds at
runtime via `update_telemetry_config`.

---

## MCP Server

The sole control interface. Zenoh is the data plane only.

**37 tools across 7 categories**

| Category | What it covers |
|---|---|
| Node lifecycle | install, uninstall, start, stop, restart, build, clean, autostart |
| Fleet discovery | list nodes, health, config, manifest, schema, stream info |
| Agent memory | search, forget, semantic store |
| Telemetry | system metrics, history, config |
| Scheduling | schedule task, list jobs, delete job |
| Proposals | create, list proposals |
| System | machine info, read/write file, run command |

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
bubbaloop/{scope}/{machine_id}/{node_name}/{resource}
          |        |            |           |
          |        |            |           +-- schema, manifest, health, config, command
          |        |            +-------------- node identifier  [a-zA-Z0-9_-], 1-64 chars
          |        +--------------------------- machine ID (e.g., nvidia_orin00)
          +------------------------------------ scope (local / edge / cloud)
```

Nodes receive scope, machine ID, and Zenoh endpoint via environment variables
injected by the daemon into systemd unit files:

```
BUBBALOOP_SCOPE=local
BUBBALOOP_MACHINE_ID=nvidia_orin00
BUBBALOOP_ZENOH_ENDPOINT=tcp/127.0.0.1:7447
```

---

## Next Steps

- [Messaging](messaging.md) — Zenoh pub/sub patterns and topic conventions
- [Memory](memory.md) — 3-tier agent memory: short-term, episodic, semantic
- [ARCHITECTURE.md](../../ARCHITECTURE.md) — Full security model, technology choices, design rationale
