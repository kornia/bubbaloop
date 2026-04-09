---
description: "Bubbaloop topic key expressions. Global and local key spaces, topic naming conventions, and how nodes and agents discover each other via Zenoh."
---

# Topics

Bubbaloop uses vanilla Zenoh key expressions for all message routing.

## Topic Naming

### Key Spaces

Topics live in two key spaces:

| Key space | Pattern | Purpose |
|-----------|---------|---------|
| **Global** | `bubbaloop/global/{machine_id}/{suffix}` | Network-visible — subscribed by the dashboard, other machines, and CLI |
| **Local** | `bubbaloop/local/{machine_id}/{suffix}` | SHM-only — never crosses the WebSocket bridge. For large binary payloads (e.g. raw RGBA frames) consumed on the same machine |

The Node SDK chooses the key space for you:

```rust
// Rust
ctx.topic("camera/compressed")        // → bubbaloop/global/{machine_id}/camera/compressed
ctx.local_topic("camera/raw")         // → bubbaloop/local/{machine_id}/camera/raw
```

```python
# Python
ctx.topic("camera/compressed")        # → bubbaloop/global/{machine_id}/camera/compressed
ctx.local_topic("camera/raw")         # → bubbaloop/local/{machine_id}/camera/raw
```

| Segment | Description | Example |
|---------|-------------|---------|
| `machine_id` | Unique machine identifier (hostname-based) | `nvidia_orin00` |
| `node_name` | Node instance name (from config `name` field) | `tapo_terrace`, `openmeteo` |
| `resource` | Data type or service | `schema`, `health`, `compressed`, `raw` |

### Standard Resources

Every self-describing node serves these queryables:

| Resource | Encoding | Description |
|----------|----------|-------------|
| `schema` | Protobuf (binary) | FileDescriptorSet for data messages |
| `manifest` | JSON | Capabilities, topics, commands, hardware requirements |
| `health` | Plain text (`"ok"`) | Heartbeat every 5s |
| `config` | JSON | Current configuration |
| `command` | JSON request/response | Imperative actions |

### Data Topics

Nodes publish data on custom sub-topics:

| Topic | Description |
|-------|-------------|
| `bubbaloop/global/nvidia_orin00/tapo_terrace/compressed` | Camera compressed frames (network-visible) |
| `bubbaloop/local/nvidia_orin00/tapo_terrace/raw` | Camera raw RGBA frames (SHM-only) |
| `bubbaloop/global/nvidia_orin00/openmeteo/weather` | Current weather conditions |
| `bubbaloop/global/nvidia_orin00/telemetry/status` | System telemetry |

### Agent Topics

The agent runtime uses dedicated topics for multi-agent messaging:

| Topic | Direction | Description |
|-------|-----------|-------------|
| `bubbaloop/global/{machine}/agent/inbox` | CLI → Daemon | Shared intake for all agent messages |
| `bubbaloop/global/{machine}/agent/{agent_id}/outbox` | Daemon → CLI | Per-agent streamed responses |
| `bubbaloop/global/{machine}/agent/{agent_id}/manifest` | Queryable | Agent capabilities and model info |

## Topic Discovery

### Via CLI

```bash
# List all active Zenoh topics
bubbaloop debug topics

# Subscribe to a specific topic
bubbaloop debug subscribe "bubbaloop/global/nvidia_orin00/tapo_terrace/**"

# Query a specific key expression
bubbaloop debug query "bubbaloop/global/nvidia_orin00/openmeteo/weather"

# List running agents
bubbaloop agent list
```

### Via SDK

```rust
// Rust — discover all nodes on the network
let nodes = bubbaloop_node::discover_nodes(&session, Duration::from_secs(2)).await?;
for node in &nodes {
    println!("{} on {}", node.node_name, node.machine_id);
}
```

```python
# Python — discover all nodes on the network
from bubbaloop_sdk import discover_nodes
nodes = discover_nodes(ctx.session, timeout=2.0)
for node in nodes:
    print(f"{node.node_name} on {node.machine_id}")
```

### Via MCP

```
discover_nodes          # Query all manifests on bubbaloop/**/manifest
list_nodes              # List registered nodes with status
get_stream_info         # Get Zenoh topic for a node's data stream
```

### Via Dashboard

The dashboard automatically discovers topics by subscribing to `bubbaloop/**`.

## Wildcard Subscriptions

Zenoh supports wildcard subscriptions for monitoring multiple topics:

| Pattern | Matches |
|---------|---------|
| `bubbaloop/**` | All bubbaloop topics (global + local) |
| `bubbaloop/global/nvidia_orin00/**` | All global topics on one machine |
| `bubbaloop/local/nvidia_orin00/**` | All SHM-only topics on one machine |
| `bubbaloop/**/schema` | All node schemas |
| `bubbaloop/**/health` | All node health heartbeats |
| `bubbaloop/global/*/agent/*/outbox` | All agent outbox streams |

## Topic Conventions

### Naming Guidelines

1. Use underscores (not hyphens) in topic path segments
2. Hostnames with hyphens are sanitized: `jetson-orin` → `jetson_orin`
3. Be descriptive but concise
4. Include node name and resource type

### Publishing Custom Topics

```rust
// Rust — use SDK publishers (handles encoding automatically)
let pub_json = ctx.publisher_json("status").await?;
pub_json.put(&serde_json::json!({"ok": true})).await?;

let pub_proto = ctx.publisher_proto::<MyMessage>("data").await?;
pub_proto.put(&msg).await?;

// SHM raw publisher for large payloads (local only)
let pub_raw = ctx.publisher_raw("raw", true).await?;
pub_raw.put(zbytes_payload).await?;
```

```python
# Python — use SDK publishers
pub_json = ctx.publisher_json("status")
pub_json.put({"ok": True})

pub_proto = ctx.publisher_proto("data", msg_class=MyMessage)
pub_proto.put(msg)

# SHM raw publisher for large payloads (local only)
pub_raw = ctx.publisher_raw("raw", local=True)
pub_raw.put(raw_bytes)
```

## Next Steps

- [API Reference](../api/index.md) — Message type definitions
- [Agent Guide](../agent-guide.md) — Multi-agent messaging details
- [Architecture](architecture.md) — System design overview
