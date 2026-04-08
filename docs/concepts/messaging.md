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

### Client Mode for Nodes

**ALWAYS use `mode: "client"`** when connecting nodes and CLI clients to the zenohd router.

Peer mode does direct P2P — messages never route through the router and are invisible to other clients.

```
Nodes/CLI/Daemon: mode = "client" <- all traffic flows through zenohd
```

!!! note
    Everything uses `"client"` mode — nodes, CLI, and the daemon itself (see `daemon/mod.rs`). Client mode routes all traffic through the Zenoh router, which is required for message visibility across participants.

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

All topics live in two key spaces:

```
bubbaloop/global/{machine_id}/{suffix}   ← network-visible (dashboard, CLI, remote machines)
bubbaloop/local/{machine_id}/{suffix}    ← SHM-only (never crosses WebSocket bridge)
```

| Field | Description | Example |
|-------|-------------|---------|
| `machine_id` | Hostname (hyphens → underscores) | `nvidia_orin00` |
| `suffix` | Node instance name + resource | `tapo_terrace/compressed`, `openmeteo/weather` |

Use **global** for data that the dashboard or other machines should see.
Use **local** for large binary payloads (raw RGBA frames) consumed only on the same machine via SHM zero-copy.

Examples:

| Topic | Description |
|-------|-------------|
| `bubbaloop/global/nvidia_orin00/tapo_terrace/compressed` | Camera frames (protobuf, network-visible) |
| `bubbaloop/local/nvidia_orin00/tapo_terrace/raw` | Camera raw RGBA (SHM-only) |
| `bubbaloop/global/nvidia_orin00/openmeteo/weather` | Current weather reading |
| `bubbaloop/global/nvidia_orin00/tapo_terrace/health` | Camera node heartbeat |
| `bubbaloop/global/nvidia_orin00/tapo_terrace/schema` | FileDescriptorSet bytes |

Hostname sanitization: `nvidia-orin-00` becomes `nvidia_orin_00`. Hyphens in topic paths break wildcard matching.

---

## Message Patterns

### SDK Publishers (recommended)

The Node SDK handles encoding, topic construction, and SHM configuration:

```rust
// Rust — protobuf publisher (sets APPLICATION_PROTOBUF encoding + type name)
let pub_proto = ctx.publisher_proto::<CompressedImage>("compressed").await?;
pub_proto.put(&image).await?;

// JSON publisher (sets APPLICATION_JSON encoding)
let pub_json = ctx.publisher_json("weather").await?;
pub_json.put(&serde_json::json!({"temperature": 22.5})).await?;

// Raw SHM publisher for local-only large payloads
let pub_raw = ctx.publisher_raw("raw", true).await?;
pub_raw.put(zbytes_payload).await?;
```

```python
# Python — protobuf publisher
pub_proto = ctx.publisher_proto("compressed", msg_class=CompressedImage)
pub_proto.put(image)

# JSON publisher
pub_json = ctx.publisher_json("weather")
pub_json.put({"temperature": 22.5})

# Raw SHM publisher for local-only large payloads
pub_raw = ctx.publisher_raw("raw", local=True)
pub_raw.put(raw_bytes)
```

### SDK Subscribers (recommended)

```rust
// Rust — typed subscriber (auto-decodes protobuf)
let sub = ctx.subscriber::<CompressedImage>("tapo_terrace/compressed").await?;
while let Some(image) = sub.recv().await {
    // image is already decoded
}

// Raw subscriber (ZBytes, no decoding — for SHM frames)
let sub_raw = ctx.subscriber_raw("tapo_terrace/raw", true).await?;
while let Some(payload) = sub_raw.recv().await {
    // payload is ZBytes — zero-copy if SHM
}
```

```python
# Python — auto-decoding subscriber (proto, JSON, or bytes based on encoding)
sub = ctx.subscribe("tapo_terrace/raw", local=True)
for msg in sub:   # decoded proto object — no _pb2 imports needed
    tensor = torch.frombuffer(msg.data, dtype=torch.uint8)

# Raw subscriber (bytes, no decoding)
sub_raw = ctx.subscribe_raw("camera/raw", local=True)
for raw_bytes in sub_raw:
    tensor = torch.frombuffer(raw_bytes, dtype=torch.uint8)
```

### Raw Zenoh (low-level)

For cases where you need direct Zenoh access:

```rust
// Rust — raw Zenoh publisher
let publisher = session
    .declare_publisher("bubbaloop/global/my_machine/camera/compressed")
    .await?;
publisher.put(image.encode_to_vec()).await?;
```

```python
# Python — raw Zenoh publisher
pub = session.declare_publisher("bubbaloop/global/my_machine/sensor/data")
pub.put(payload_bytes)
```

### Queryable

```rust
// NEVER use .complete(true) — blocks wildcard discovery like bubbaloop/**/schema
let queryable = session
    .declare_queryable("bubbaloop/global/my_machine/camera/schema")
    .await?;

while let Ok(query) = queryable.recv_async().await {
    query.reply(query.key_expr(), schema_bytes.clone()).await?;
}
```

```python
def on_query(query):
    # CRITICAL: query.key_expr is a PROPERTY, not a method
    # WRONG: query.key_expr()  <- TypeError crash
    # RIGHT: query.key_expr
    query.reply(query.key_expr, payload_bytes)

queryable = session.declare_queryable('bubbaloop/global/my_machine/sensor/schema', on_query)
```

### TypeScript (Dashboard / Browser)

The dashboard uses `zenoh-ts` over WebSocket:

```typescript
import { Session, Config } from "@zenohprotocol/zenoh-ts";

const session = await Session.open(new Config("ws/127.0.0.1:10001"));

// Subscribe to all camera topics on this machine
const sub = await session.declareSubscriber(
    "bubbaloop/global/my_machine/camera/**",
    (sample) => {
        const bytes = new Uint8Array(sample.payload().buffer);
        const image = CompressedImage.decode(bytes);
    }
);

// Query a node's schema
const replies = await session.get("bubbaloop/global/my_machine/camera/schema");
for await (const reply of replies) {
    const descriptorBytes = new Uint8Array(reply.result().payload().buffer);
}
```

---

## Agent Gateway Protocol

The daemon hosts a multi-agent runtime. All LLM processing is daemon-side. The CLI is a thin Zenoh client — it publishes to inbox and subscribes to the agent's outbox.

### Topics

```
bubbaloop/global/{machine}/agent/inbox                   <- shared inbox (all agents)
bubbaloop/global/{machine}/agent/{agent_id}/outbox       <- per-agent event stream
bubbaloop/global/{machine}/agent/{agent_id}/manifest     <- queryable: agent metadata
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
    "type": "delta",
    "text": "Currently running nodes:"
}
```

Note: The field is serialized as `"type"` (not `event_type`) with snake_case values.

`type` values:

| Type | Meaning |
|------|---------|
| `delta` | Streaming token — append to output |
| `tool` | Agent is calling a tool (tool name in `text`) |
| `tool_result` | Tool returned a result |
| `error` | Turn failed |
| `done` | Turn complete, stream closed |

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

Discovery: query `bubbaloop/**/manifest` to find all nodes, or use the SDK:

```rust
// SDK discovery (recommended)
let nodes = bubbaloop_node::discover_nodes(&session, Duration::from_secs(2)).await?;
for node in &nodes {
    println!("{}/{}", node.machine_id, node.node_name);
}

// Raw Zenoh discovery
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

Agent messages: Currently use Zenoh defaults (`BestEffort` + `Drop`). `Reliable` + `Block` is planned but not yet configured.

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

- [Architecture](architecture.md) — Layer model, daemon, agent runtime
- [Memory](memory.md) — 3-tier agent memory (short-term, episodic, semantic)
