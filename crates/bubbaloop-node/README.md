# Bubbaloop Node SDK

Batteries-included framework for writing bubbaloop nodes. Reduces boilerplate from ~300 to ~50 lines.

> **Parity invariant:** This Rust SDK and `python-sdk/` are peer APIs, not layered — every publisher/subscriber/context method added to one MUST have an equivalent in the other in the same PR (or a linked tracking issue). Names should match where possible. Where `zenoh-python` can't surface a knob that Rust exposes, the Python side collapses it to the simplest equivalent that preserves wire behavior. Example: Rust `publisher_cbor_shm(suffix, slot_count, slot_size)` ↔ Python `publisher_cbor(suffix, local=True)` — same `application/cbor` encoding and `CongestionControl::Block` on the wire; Python slot sizing is implicit because `zenoh-python` doesn't expose `ShmProvider`.

## Quick Start (Rust)

```rust
use bubbaloop_node::{Node, NodeContext};

struct MySensor;

#[async_trait::async_trait]
impl Node for MySensor {
    type Config = MyConfig;
    fn name() -> &'static str { "my-sensor" }
    fn descriptor() -> &'static [u8] { include_bytes!(concat!(env!("OUT_DIR"), "/descriptor.bin")) }

    async fn init(ctx: &NodeContext, config: &MyConfig) -> anyhow::Result<Self> {
        Ok(Self)
    }

    async fn run(self, ctx: NodeContext) -> anyhow::Result<()> {
        // Declared publisher — encoding set once, reused for every put()
        let pub_proto = ctx.publisher_proto::<MyMessage>("sensor/data").await?;
        let pub_json = ctx.publisher_json("sensor/status").await?;

        loop {
            tokio::select! {
                _ = ctx.shutdown_rx.changed() => break,
                _ = tokio::time::sleep(Duration::from_secs(1)) => {
                    pub_proto.put(&my_message).await?;
                    pub_json.put(&serde_json::json!({"state": "running"})).await?;
                }
            }
        }
        Ok(())
    }
}
```

## Publish API

All publishing uses declared publishers for efficiency. Encoding is set once at creation.

| Method | Encoding | Use case |
|--------|----------|----------|
| `ctx.publisher_proto::<T>(suffix)` | `application/protobuf;{type_name}` | Realtime sensor data |
| `ctx.publisher_json(suffix)` | `application/json` | Status, config, metadata |

The protobuf publisher extracts the type name from `MessageTypeName::type_name()` and embeds it in the Zenoh encoding. The dashboard reads this to decode without schema discovery.

## Subscribe API

| Method | Decoding | Use case |
|--------|----------|----------|
| `ctx.subscriber::<T>(suffix)` | Auto-decode protobuf, 256-slot FIFO | Node-to-node typed streams |
| `ctx.subscriber_raw(suffix, local)` | Raw ZBytes, 4-slot FIFO | Video frames, dynamic decode |

Additionally, two raw publisher variants:

| Method | Encoding | Use case |
|--------|----------|----------|
| `ctx.publisher_raw(suffix, local)` | Raw ZBytes (custom encoding) | Video frames, SHM |
| `ctx.publisher_raw_proto::<T>(suffix)` | `application/protobuf;{type_name}` + raw bytes | Proto without encode overhead |

## Python SDK

A pure Python SDK with the same API is available at `python-sdk/`:

```bash
pip install git+https://github.com/kornia/bubbaloop.git#subdirectory=python-sdk
```

```python
from bubbaloop_sdk import NodeContext

async def main():
    ctx = await NodeContext.connect()
    pub = await ctx.publisher_proto("camera/front/compressed", CompressedImage)
    await pub.put(msg)
```

## How It Works

The SDK handles:
- Zenoh client-mode session (routes through zenohd)
- Schema queryable at `bubbaloop/global/{machine_id}/{node_name}/schema`
- Health heartbeat every 5s
- YAML config loading
- SIGINT/SIGTERM graceful shutdown
- Encoding metadata on every publish (Zenoh `Encoding` field)

Nodes implement the `Node` trait and call `run_node::<MyNode>().await`.

## Zenoh Encoding

Every publish sets the Zenoh `Encoding` field:
- Protobuf: `Encoding::APPLICATION_PROTOBUF.with_schema("bubbaloop.camera.v1.CompressedImage")`
- JSON: `Encoding::APPLICATION_JSON`

The dashboard reads `sample.encoding()` to pick the right decoder instantly — no schema discovery race, no polling.
