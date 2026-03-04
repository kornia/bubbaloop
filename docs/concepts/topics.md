# Topics

Bubbaloop uses vanilla Zenoh key expressions for all message routing.

## Topic Naming

### Format

Topics follow a hierarchical naming pattern:

```
bubbaloop/{scope}/{machine_id}/{node_name}/{resource}
```

| Segment | Description | Example |
|---------|-------------|---------|
| `scope` | Deployment environment | `local` |
| `machine_id` | Unique machine identifier (hostname-based) | `nvidia_orin00` |
| `node_name` | Node instance name | `rtsp-camera`, `openmeteo` |
| `resource` | Data type or service | `schema`, `manifest`, `health`, `config`, `command` |

### Standard Resources

Every self-describing node serves these queryables:

| Resource | Encoding | Description |
|----------|----------|-------------|
| `schema` | Protobuf (binary) | FileDescriptorSet for data messages |
| `manifest` | JSON | Capabilities, topics, commands, hardware requirements |
| `health` | JSON | Status, uptime, last heartbeat |
| `config` | JSON | Current configuration |
| `command` | JSON request/response | Imperative actions |

### Data Topics

Nodes publish data on custom sub-topics:

| Topic | Description |
|-------|-------------|
| `bubbaloop/local/nvidia_orin00/rtsp-camera/frame` | Camera compressed frames |
| `bubbaloop/local/nvidia_orin00/openmeteo/status` | Current weather conditions |
| `bubbaloop/local/nvidia_orin00/telemetry/status` | System telemetry |

### Agent Topics

The agent runtime uses dedicated topics for multi-agent messaging:

| Topic | Direction | Description |
|-------|-----------|-------------|
| `bubbaloop/{scope}/{machine}/agent/inbox` | CLI → Daemon | Shared intake for all agent messages |
| `bubbaloop/{scope}/{machine}/agent/{agent_id}/outbox` | Daemon → CLI | Per-agent streamed responses |
| `bubbaloop/{scope}/{machine}/agent/{agent_id}/manifest` | Queryable | Agent capabilities and model info |

## Topic Discovery

### Via CLI

```bash
# List all active Zenoh topics
bubbaloop debug topics

# Subscribe to a specific topic
bubbaloop debug subscribe "bubbaloop/local/nvidia_orin00/rtsp-camera/**"

# Query a specific key expression
bubbaloop debug query "bubbaloop/local/nvidia_orin00/openmeteo/status"

# List running agents
bubbaloop agent list
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
| `bubbaloop/**` | All bubbaloop topics |
| `bubbaloop/local/nvidia_orin00/**` | All topics on one machine |
| `bubbaloop/**/schema` | All node schemas |
| `bubbaloop/**/manifest` | All node manifests |
| `bubbaloop/local/*/agent/*/outbox` | All agent outbox streams |

## Topic Conventions

### Naming Guidelines

1. Use underscores (not hyphens) in topic path segments
2. Hostnames with hyphens are sanitized: `jetson-orin` → `jetson_orin`
3. Be descriptive but concise
4. Include node name and resource type

### Publishing Custom Topics

```rust
// Rust example using the Node SDK
let topic = ctx.topic("status");  // → bubbaloop/{scope}/{machine_id}/{node_name}/status
let publisher = ctx.session.declare_publisher(&topic).await?;
publisher.put(encoded_bytes).await?;
```

```python
# Python example
topic = f"bubbaloop/{scope}/{machine_id}/{node_name}/status"
session.put(topic, payload)
```

## Next Steps

- [API Reference](../api/index.md) — Message type definitions
- [Agent Guide](../agent-guide.md) — Multi-agent messaging details
- [Architecture](architecture.md) — System design overview
