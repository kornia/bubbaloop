# Dashboard Schema & Decode Pipeline Redesign

**Date**: 2026-04-02
**Status**: Approved
**Scope**: Node SDK publish API, Python PyO3 bindings, dashboard decode pipeline, daemon JSON migration

## Problem

The dashboard cannot reliably decode Zenoh messages because:

1. **No format signal** — samples carry no metadata about whether they're protobuf, JSON, or raw binary. The dashboard must guess.
2. **Schema race condition** — protobuf decoding requires a FileDescriptorSet, fetched via Zenoh queryable. Messages arrive before schemas load, causing hex/raw display.
3. **Polling-based discovery** — the dashboard polls for schemas every 10-30s, missing late-starting nodes and wasting cycles on known nodes.
4. **Protobuf everywhere** — the daemon uses protobuf for low-frequency request/reply messages (NodeList, CommandResult), requiring a core schema queryable that doesn't exist.

## Design Principles

- **Node author picks the format** — protobuf for realtime performance, JSON for quick prototyping
- **The SDK makes both paths equally easy** — no extra ceremony for either choice
- **The dashboard decodes anything** — reads Zenoh encoding metadata, picks the right decoder
- **Backward compatible** — old nodes without encoding hit the existing sniff fallback

## Architecture

### Zenoh Encoding as Primary Signal

Zenoh 1.0+ carries an `Encoding` field on every sample (like HTTP `Content-Type`). Nodes set this on publish; the dashboard reads it to pick the decoder. No sniffing needed for well-behaved nodes.

Key encodings:

| Encoding | Schema suffix | Dashboard action |
|---|---|---|
| `APPLICATION_PROTOBUF` | `bubbaloop.camera.v1.CompressedImage` | Fetch FileDescriptorSet from `{node}/schema`, decode |
| `APPLICATION_JSON` | (none) | `JSON.parse()`, done |
| `APPLICATION_OCTET_STREAM` | (none) | Route to specialized decoder (H.264, etc.) |
| Missing/unknown | (none) | Sniff fallback: JSON -> proto brute force -> text -> hex |

### Component Responsibilities

**Node SDK** — sets encoding automatically on every publish. Node authors never think about it.

**Dashboard** — reads `sample.encoding()` as primary signal. SchemaRegistry becomes a cache for fetched FileDescriptorSets, not a startup prerequisite.

**Daemon** — publishes all messages as JSON. No protobuf, no schema queryable, no FileDescriptorSet to serve.

**Nodes** — serve their own FileDescriptorSet at `{node}/schema` queryable (already implemented in SDK). Only needed for protobuf nodes.

## Node SDK Publish API

### Declared Publishers

All publishing uses declared publishers (not `session.put()`), reusing the Zenoh publisher for efficiency. The encoding is set once at publisher creation time.

### Rust API

```rust
// Protobuf publisher — encoding set once at declaration
let camera_pub = ctx
    .publisher_proto::<CompressedImage>("camera/front/compressed")
    .await?;

loop {
    let msg = capture_frame();
    camera_pub.put(&msg).await?;  // encode_to_vec() + publish, publisher reused
}

// JSON publisher
let weather_pub = ctx.publisher_json("weather/current").await?;
weather_pub.put(&serde_json::json!({"temperature": 22.5})).await?;
```

Type name for protobuf comes from the existing `MessageTypeName` trait. Internally:

```rust
// publisher_proto sets encoding once:
session.declare_publisher(topic)
    .encoding(Encoding::APPLICATION_PROTOBUF.with_schema(T::type_name()))
    .await
```

### Python API (via PyO3)

```python
from bubbaloop_sdk import NodeContext

async def run():
    ctx = await NodeContext.connect()

    # Protobuf — type name from msg.DESCRIPTOR.full_name, extracted once
    camera_pub = await ctx.publisher_proto("camera/front/compressed", CompressedImage)

    while not ctx.is_shutdown():
        msg = capture_frame()
        await camera_pub.put(msg)

    # JSON — no schema needed
    weather_pub = await ctx.publisher_json("weather/current")
    await weather_pub.put({"temperature": 22.5})
```

### Typed Subscribers

Two tiers: typed for node-to-node, raw for dashboard-style dynamic decoding.

```rust
// Typed — node-to-node, knows the type at compile time
let sub = ctx.subscriber::<CurrentWeather>("weather/+/current").await?;

loop {
    tokio::select! {
        Some(msg) = sub.recv() => {
            println!("temp: {}", msg.temperature);
        }
        _ = ctx.shutdown_rx.changed() => break,
    }
}

// Untyped — dashboard-style, dynamic decoding
let sub = ctx.subscriber_raw("bubbaloop/**").await?;
// sample.encoding() tells you format, sample.payload() gives raw bytes
```

```python
# Python typed subscriber — async iterator
async for msg in ctx.subscriber("weather/+/current", CurrentWeather):
    print(f"temp: {msg.temperature}")
```

The typed subscriber reads the incoming sample's `Encoding` to decide how to decode:

| Sample encoding | Behavior |
|---|---|
| `APPLICATION_PROTOBUF` + schema | Decode with prost (Rust) / protobuf (Python) |
| `APPLICATION_JSON` | Deserialize into target type via serde / dict |
| Missing/unknown | Try proto -> JSON -> return raw bytes |

## PyO3 Bindings

### Stack

```
Python asyncio -> PyO3 (pyo3-async-runtimes) -> Rust SDK -> Rust zenoh
```

No dependency on zenoh-python. The Rust zenoh session is used directly, giving Python nodes the same performance as Rust nodes for I/O.

### Async Support

All publish/subscribe methods are async, using `pyo3_async_runtimes::tokio::future_into_py()` to convert Rust futures into Python awaitables.

### GIL Handling

The GIL is held only for argument extraction (microseconds). All encoding, serialization, and network I/O runs GIL-free on the tokio runtime.

```
|-- GIL held (us) --|-------- GIL released (ms) --------|
   extract args         serialize -> zenoh put -> network
```

For `publish_proto`:
1. **GIL held**: extract `msg.DESCRIPTOR.full_name` (string) and `msg.SerializeToString()` (bytes) — done once at publisher creation for type name, per-message for serialization
2. **GIL released**: Zenoh `publisher.put(bytes)` — all network I/O

For `publish_json`:
1. **GIL held**: extract Python dict
2. **GIL released**: `serde_json` serialization + Zenoh publish

Multiple Python nodes in the same process can publish concurrently without blocking each other.

## Dashboard Decode Pipeline

### New Flow

```
sample arrives
    |
    +-- read sample.encoding()
    |
    +-- APPLICATION_JSON -----------> JSON.parse(), done
    |
    +-- APPLICATION_PROTOBUF
    |   +-- .schema() suffix? ------> type name (e.g. "bubbaloop.camera.v1.CompressedImage")
    |       +-- type in SchemaRegistry cache? -> decode immediately
    |       +-- type NOT in cache? -----------> fetch {node}/schema queryable (async)
    |                                            -> cache FileDescriptorSet
    |                                            -> decode this sample once schema arrives
    |                                            (sample held in component state, not dropped)
    |
    +-- APPLICATION_OCTET_STREAM ---> route to specialized decoder (H.264, etc.)
    |
    +-- missing/unknown encoding ---> sniff fallback (JSON -> proto brute force -> text -> hex)
```

### What Changes

**Removed:**
- Proactive 10s/30s re-discovery polling loop
- `useSchemaReady()` gating pattern (encoding on sample is the signal)
- Brute-force `tryDecodeAny()` for samples that have encoding set

**Kept:**
- SchemaRegistry class (as a FileDescriptorSet cache, not a prerequisite)
- `*/schema` queryable fetching (triggered on-demand by first unknown protobuf type)
- Sniff fallback for legacy nodes without encoding
- Built-in decoders for specialized formats (H.264)

**Added:**
- `sample.encoding()` read as primary decode signal
- On-demand schema fetch triggered by first sample with unknown protobuf type

## Daemon JSON Migration

The daemon switches all gateway messages from protobuf to JSON:

| Endpoint | Current format | New format |
|---|---|---|
| Manifest queryable | JSON | JSON (no change) |
| Node list queryable | Protobuf | JSON |
| Command queryable | Protobuf | JSON |
| Command results | Protobuf | JSON |
| Events pub/sub | JSON | JSON (no change) |

All daemon messages use `Encoding::APPLICATION_JSON`. The dashboard decodes them with `JSON.parse()` — no schema needed.

This eliminates:
- Core `DESCRIPTOR` serving from the daemon
- The `daemon/api/schemas` queryable concept
- Dashboard dependency on core schema loading at startup

The `DESCRIPTOR` constant and `bubbaloop-schemas` crate remain for the Node SDK (nodes that publish protobuf still need them for schema queryables).

## Backward Compatibility & Migration

### Old nodes (no encoding set on publish)

- Dashboard hits sniff fallback: try JSON -> proto brute force -> text -> hex
- `*/schema` queryable still serves FileDescriptorSet for these nodes
- SchemaRegistry cache still works for the proto brute-force path
- No breakage — same behavior as today

### Migration path for existing nodes

1. Update Node SDK dependency
2. Switch from `session.put()` to `ctx.publisher_proto()` / `ctx.publisher_json()`
3. No proto schema changes needed — just how they publish

### Dashboard rollout

- **Phase 1**: Read `sample.encoding()`, use it when available, fall back to current behavior
- **Phase 2**: Remove polling loop and `useSchemaReady()` gating once all nodes are updated

Both phases are backward compatible. No breaking changes at any point.

## Success Criteria

1. Dashboard decodes JSON messages on first sample without any schema fetch
2. Dashboard decodes protobuf messages on first sample (after one-time schema fetch per node type)
3. Old nodes without encoding still work via sniff fallback
4. Rust and Python nodes use the same SDK API surface via PyO3
5. GIL is never held during network I/O in Python nodes
6. Daemon gateway messages are pure JSON — no protobuf encoding in daemon
7. No polling loops in the dashboard for schema discovery
