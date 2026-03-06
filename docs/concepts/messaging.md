# Messaging

Zenoh pub/sub, queryables, and the agent gateway protocol.

---

## Vanilla Zenoh Overview

Zenoh is a pub/sub/query protocol built for robotics and IoT. Three core patterns:

| Pattern | When to use |
|---------|-------------|
| Pub/Sub | Continuous data streams (sensor frames, telemetry) |
| Queryable | On-demand fetch (schema, config, health) |
| Liveliness | Node presence detection |

Low latency. No broker required. Runs over TCP, UDP, WebSocket, or shared memory.

### Client Mode is Mandatory

**ALWAYS use `mode: "client"`** when connecting nodes to the zenohd router.

Peer mode does direct P2P — messages never route through the daemon and are invisible to other clients.

```
WRONG: mode = "peer"   <- nodes can't see each other through the router
RIGHT: mode = "client" <- all traffic flows through zenohd
```

Rust:
```rust
zenoh_config.insert_json5("mode", r#""client""#)?;
```

Python:
```python
conf.insert_json5('mode', '"client"')
```

---

## Topic Convention

All topics follow one pattern:

```
bubbaloop/{scope}/{machine_id}/{node_name}/{resource}
```

| Field | Description | Example |
|-------|-------------|---------|
| `scope` | Logical grouping | `local`, `prod`, `lab` |
| `machine_id` | Hostname (hyphens → underscores) | `nvidia_orin00` |
| `node_name` | Node binary name | `camera`, `openmeteo` |
| `resource` | Data type or sub-topic | `compressed`, `health`, `schema` |

Examples:

| Topic | Description |
|-------|-------------|
| `bubbaloop/local/nvidia_orin00/camera/compressed` | Camera frames (protobuf) |
| `bubbaloop/local/nvidia_orin00/openmeteo/current` | Current weather reading |
| `bubbaloop/local/nvidia_orin00/camera/health` | Camera node heartbeat |
| `bubbaloop/local/nvidia_orin00/camera/schema` | FileDescriptorSet bytes |
| `bubbaloop/local/nvidia_orin00/daemon/api/nodes` | Daemon node list |

Hostname sanitization: `nvidia-orin-00` becomes `nvidia_orin_00`. Hyphens in topic paths break wildcard matching.

---

## Message Patterns

### Rust Publisher

```rust
use prost::Message;

let publisher = session
    .declare_publisher("bubbaloop/local/my_machine/camera/compressed")
    .await?;

publisher.put(image.encode_to_vec()).await?;
```

### Rust Subscriber

```rust
let subscriber = session
    .declare_subscriber("bubbaloop/local/my_machine/camera/**")
    .await?;

while let Ok(sample) = subscriber.recv_async().await {
    let bytes = sample.payload().to_bytes();
    let image = CompressedImage::decode(bytes.as_ref())?;
}
```

### Rust Queryable

```rust
// NEVER use .complete(true) — blocks wildcard discovery like bubbaloop/**/schema
let queryable = session
    .declare_queryable("bubbaloop/local/my_machine/camera/schema")
    .await?;

while let Ok(query) = queryable.recv_async().await {
    query.reply(query.key_expr(), schema_bytes.clone()).await?;
}
```

### Python Publisher

```python
import zenoh
import json

conf = zenoh.Config()
conf.insert_json5('mode', '"client"')
conf.insert_json5('connect/endpoints', '["tcp/127.0.0.1:7447"]')

session = zenoh.open(conf)
pub = session.declare_publisher('bubbaloop/local/my_machine/sensor/data')
pub.put(payload_bytes)
```

### Python Queryable

```python
def on_query(query):
    # CRITICAL: query.key_expr is a PROPERTY, not a method
    # WRONG: query.key_expr()  <- TypeError crash
    # RIGHT: query.key_expr
    query.reply(query.key_expr, payload_bytes)

queryable = session.declare_queryable('bubbaloop/local/my_machine/sensor/schema', on_query)
```

### TypeScript (Dashboard / Browser)

The dashboard uses `zenoh-ts` over WebSocket:

```typescript
import { Session, Config } from "@zenohprotocol/zenoh-ts";

const session = await Session.open(new Config("ws/127.0.0.1:10001"));

// Subscribe to all camera topics on this machine
const sub = await session.declareSubscriber(
    "bubbaloop/local/my_machine/camera/**",
    (sample) => {
        const bytes = new Uint8Array(sample.payload().buffer);
        const image = CompressedImage.decode(bytes);
    }
);

// Query a node's schema
const replies = await session.get("bubbaloop/local/my_machine/camera/schema");
for await (const reply of replies) {
    const descriptorBytes = new Uint8Array(reply.result().payload().buffer);
}
```

---

## Agent Gateway Protocol

The daemon hosts a multi-agent runtime. All LLM processing is daemon-side. The CLI is a thin Zenoh client — it publishes to inbox and subscribes to the agent's outbox.

### Topics

```
bubbaloop/{scope}/{machine}/agent/inbox                   <- shared inbox (all agents)
bubbaloop/{scope}/{machine}/agent/{agent_id}/outbox       <- per-agent event stream
bubbaloop/{scope}/{machine}/agent/{agent_id}/manifest     <- queryable: agent metadata
```

### Wire Format

Messages are JSON.

**AgentMessage** (client → inbox):
```json
{
    "id": "550e8400-e29b-41d4-a716-446655440000",
    "text": "What nodes are running?",
    "agent": "openclaw"
}
```

`agent` field is optional. If omitted, the daemon routes to the default agent.

**AgentEvent** (daemon → outbox):
```json
{
    "id": "550e8400-e29b-41d4-a716-446655440000",
    "event_type": "Delta",
    "text": "Currently running nodes:"
}
```

`event_type` values:

| Type | Meaning |
|------|---------|
| `Delta` | Streaming token — append to output |
| `Tool` | Agent is calling a tool (tool name in `text`) |
| `ToolResult` | Tool returned a result |
| `Error` | Turn failed |
| `Done` | Turn complete, stream closed |

### Correlation

The `id` field links every `AgentEvent` back to the originating `AgentMessage`. A client that sends multiple messages concurrently uses the `id` to demux the response streams.

### CLI Subscription Order

```
1. Subscribe to outbox FIRST
2. Then publish to inbox

   (avoids missing early Delta events)
```

```
CLI                        Daemon / Agent Runtime
 |                                  |
 |-- subscribe outbox ------------->|
 |                                  |
 |-- publish inbox (AgentMessage) ->|
 |                                  |
 |<--- AgentEvent{Delta} -----------|
 |<--- AgentEvent{Delta} -----------|
 |<--- AgentEvent{Tool} ------------|
 |<--- AgentEvent{ToolResult} ------|
 |<--- AgentEvent{Delta} -----------|
 |<--- AgentEvent{Done} ------------|
```

### Agent Configuration

Agents are defined in `~/.bubbaloop/agents.toml`. Each agent gets its own soul and memory under `~/.bubbaloop/agents/{id}/`.

---

## Node Queryable Patterns

Every well-behaved node serves five standard queryables:

| Resource | Topic suffix | Returns |
|----------|-------------|---------|
| `schema` | `/schema` | FileDescriptorSet bytes (protobuf) |
| `manifest` | `/manifest` | JSON: name, version, topics |
| `health` | `/health` | JSON: status, uptime, stats |
| `config` | `/config` | YAML or JSON config |
| `command` | `/command` | Accepts control commands |

Discovery: query `bubbaloop/**/manifest` to find all nodes on the network.

```rust
// Discover all nodes
let replies = session
    .get("bubbaloop/**/manifest")
    .target(QueryTarget::All)
    .await?;

while let Ok(reply) = replies.recv_async().await {
    if let Ok(sample) = reply.into_result() {
        println!("Found node: {}", sample.key_expr());
    }
}
```

---

## Network Topology

### Local Development

```
+--------------------------------------------------+
|                  Local Machine                   |
|                                                  |
|  +------------+   TCP     +--------------------+ |
|  | camera     |--:7447--->|  zenohd (router)   | |
|  +------------+           |  - TCP  :7447       | |
|  +------------+   TCP     |  - WS   :10001      | |
|  | openmeteo  |--:7447--->|                    | |
|  +------------+           +----------+---------+ |
|  +------------+   TCP                |           |
|  | bubbaloop  |--:7447--->           |           |
|  | (daemon)   |           +----------v---------+ |
|  +------------+           | Dashboard (browser)| |
|                           | http://localhost:  | |
|                           |   5173             | |
|                           +--------------------+ |
+--------------------------------------------------+
```

### Distributed Setup

```
+------------------------------------------------+
|              Edge Device (Robot)               |
|  +------------+         +------------------+  |
|  | camera     |--TCP--->|  zenohd          |  |
|  +------------+  :7447  |  - tcp  :7447    |  |
|  +------------+         |  - ws   :10001   |  |
|  | bubbaloop  |--TCP--->|                  |  |
|  | (daemon)   |         +--------+---------+  |
|  +------------+                  |            |
+----------------------------------+------------+
                                   | TCP :7447
+----------------------------------+------------+
|              Operator Laptop                  |
|                        +------------------+   |
|  +-----------+  WS     | zenohd           |   |
|  | Dashboard |<-:10001-| - connects to    |   |
|  +-----------+         |   robot :7447    |   |
|                        | - ws :10001      |   |
|  +-----------+  TCP    |   (local)        |   |
|  | bubbaloop |--:7447->|                  |   |
|  | (CLI)     |         +------------------+   |
+-------------------------------------------------+
```

All nodes connect as clients to the local zenohd. The daemon discovers remote machines through router-to-router peering.

---

## Quality of Service

| Setting | Options | Default | Notes |
|---------|---------|---------|-------|
| Reliability | `BestEffort` / `Reliable` | `BestEffort` | Use best-effort for streams |
| Congestion | `Drop` / `Block` | `Drop` | Streams drop; commands block |
| Priority | 1 (high) – 7 (low) | 5 | Controls scheduling |
| Express | `true` / `false` | `false` | Bypasses batching, lower latency |

Camera and sensor streams: `BestEffort` + `Drop`. Dropping stale frames is correct.

Agent messages: `Reliable` + `Block`. Commands must not be lost.

---

## WebSocket Bridge

The `zenoh-bridge-remote-api` gives browser clients Zenoh access over WebSocket.

| Port | Protocol | Purpose |
|------|----------|---------|
| 7447 | TCP | Zenoh router (nodes + daemon) |
| 10001 | WebSocket | Browser / dashboard clients |

```json5
{
  mode: "router",
  listen: {
    endpoints: ["tcp/127.0.0.1:7447"],
  },
  plugins: {
    remote_api: {
      websocket_port: 10001,
    },
  },
}
```

Bind to `127.0.0.1`, not `0.0.0.0`. Never expose Zenoh ports directly to the internet.

---

## Next Steps

- [Architecture](../architecture.md) — Layer model, daemon, agent runtime
- [Memory](memory.md) — 3-tier agent memory (short-term, episodic, semantic)
