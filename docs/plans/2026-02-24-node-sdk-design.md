# Node SDK Design Document

**Date:** 2026-02-24
**Status:** Draft
**Authors:** Edgar Riba + Claude

## Goal

The `bubbaloop-node-sdk` crate provides a batteries-included framework for writing bubbaloop nodes in Rust. Today, every node reimplements ~300 lines of identical scaffolding: Zenoh session creation, health heartbeat publishing, schema queryable registration, config file loading, signal handling, and graceful shutdown. The Node SDK extracts all of that into a single crate so node authors write ~50 lines of business logic -- a `Node` trait implementation and a one-line `main()`.

## Crate Structure

```
bubbaloop-node-sdk/
  Cargo.toml
  src/
    lib.rs          # Public API: Node trait, run_node(), re-exports
    config.rs       # NodeConfig derive macro support, YAML loading, validation
    zenoh.rs        # Zenoh session creation (client mode), endpoint resolution
    health.rs       # Health heartbeat publisher (5s interval)
    schema.rs       # Schema queryable (protobuf descriptor serving)
    shutdown.rs     # Signal handling (SIGTERM/SIGINT), watch channel
    context.rs      # NodeContext: session, scope, machine_id, shutdown_rx
```

### Cargo.toml Dependencies

```toml
[package]
name = "bubbaloop-node-sdk"
version = "0.1.0"
edition = "2021"

[dependencies]
zenoh = "1"
tokio = { version = "1", features = ["full"] }
serde = { version = "1", features = ["derive"] }
serde_yaml = "0.9"
log = "0.4"
env_logger = "0.11"
argh = "0.1"
ctrlc = "3"
anyhow = "1"
thiserror = "2"
hostname = "0.4"
bubbaloop-schemas = { git = "https://github.com/kornia/bubbaloop.git", branch = "main" }
```

This crate is **standalone** -- it is NOT added to the bubbaloop cargo workspace (same pattern as `bubbaloop-schemas`).

## What the SDK Handles (Boilerplate the User Does Not Write)

### 1. Zenoh Session Creation

The SDK opens a Zenoh session in **client mode** connecting to a configurable endpoint. Resolution order:

1. `ZENOH_ENDPOINT` environment variable (highest priority)
2. `-e` / `--endpoint` CLI flag
3. Default: `tcp/127.0.0.1:7447`

Client mode is mandatory -- peer mode does not route through the zenohd router, which breaks topic discovery. The SDK enforces this; node authors cannot override it.

```rust
// Internal SDK implementation (not exposed to users)
let mut zenoh_config = zenoh::Config::default();
zenoh_config.insert_json5("mode", r#""client""#).unwrap();
zenoh_config
    .insert_json5("connect/endpoints", &format!(r#"["{}"]"#, endpoint))
    .unwrap();
let session = Arc::new(zenoh::open(zenoh_config).await?);
```

### 2. Health Heartbeat Publishing

Every node must publish a heartbeat at least every 10 seconds (daemon timeout is 30s). The SDK spawns a background task that publishes `"ok"` to `bubbaloop/{scope}/{machine_id}/health/{node_name}` every 5 seconds. Node authors never interact with this.

### 3. Schema Registration via Queryable

The SDK declares a queryable on `bubbaloop/{scope}/{machine_id}/{node_name}/schema` that responds with the node's protobuf `FileDescriptorSet` bytes. The descriptor is provided by the node author as `include_bytes!` at compile time and passed to the SDK via `Node::descriptor()`.

The queryable does NOT use `.complete(true)` -- this would block wildcard queries like `bubbaloop/**/schema` that the dashboard uses for discovery.

### 4. Config File Loading (YAML)

The SDK loads a YAML config file from the path provided via `-c` / `--config` CLI flag. If no flag is given, it looks for `config.yaml` in the current directory. The config struct is defined by the node author using `#[derive(Deserialize)]` and the SDK deserializes it, returning a clear error if parsing fails.

### 5. Signal Handling (SIGTERM / SIGINT)

The SDK sets up a `ctrlc` handler that sends on a `tokio::sync::watch` channel. The shutdown receiver is passed to the node via `NodeContext`. The health heartbeat task also listens on this channel and stops when shutdown is signaled.

### 6. Graceful Shutdown

On shutdown signal:
1. The SDK calls `Node::shutdown()` on the user's implementation
2. Health heartbeat task stops
3. Schema queryable is dropped
4. Zenoh session closes
5. Process exits cleanly

### 7. Scope and Machine ID Resolution

The SDK reads `BUBBALOOP_SCOPE` (default: `"local"`) and `BUBBALOOP_MACHINE_ID` (default: hostname) from environment variables. These are exposed to the node via `NodeContext`.

### 8. Logging Initialization

The SDK initializes `env_logger` with a default filter of `"info"`. Nodes use `log::info!()`, `log::warn!()`, etc. directly -- no tracing.

### 9. CLI Argument Parsing

The SDK provides built-in `argh` arguments:
- `-c` / `--config`: path to config file
- `-e` / `--endpoint`: Zenoh endpoint

Node authors can extend this with additional arguments if needed, but the common flags are handled automatically.

## What Node Authors Write (~50 Lines)

### 1. `Node` Trait Implementation

```rust
#[async_trait]
pub trait Node: Send + Sync + 'static {
    /// Node-specific configuration type (deserialized from YAML)
    type Config: serde::de::DeserializeOwned + Send + Sync + 'static;

    /// Human-readable node name used for topic construction and health reporting.
    /// Must match the `name` field in `node.yaml`.
    fn name() -> &'static str;

    /// Protobuf FileDescriptorSet bytes for schema registration.
    /// Typically: `include_bytes!(concat!(env!("OUT_DIR"), "/descriptor.bin"))`
    fn descriptor() -> &'static [u8];

    /// Called once after Zenoh session is established and config is loaded.
    /// Use this to create publishers, subscribers, and any node-specific state.
    async fn init(ctx: &NodeContext, config: &Self::Config) -> anyhow::Result<Self>
    where
        Self: Sized;

    /// Main loop. Called after init(). Must respect ctx.shutdown_rx for graceful exit.
    /// When the shutdown signal fires, this method should return Ok(()).
    async fn run(self, ctx: NodeContext) -> anyhow::Result<()>;
}
```

### 2. Config Struct

Node authors define a plain struct with `#[derive(Deserialize)]`:

```rust
#[derive(Debug, Clone, serde::Deserialize)]
pub struct MyConfig {
    pub publish_topic: String,
    pub rate_hz: f64,
    // ... node-specific fields
}
```

No derive macro is needed beyond `Deserialize`. The SDK handles loading and error reporting.

### 3. `main()` Entry Point

```rust
#[tokio::main]
async fn main() -> anyhow::Result<()> {
    bubbaloop_node_sdk::run_node::<MyNode>().await
}
```

## `NodeContext` -- What the SDK Provides to Nodes

```rust
pub struct NodeContext {
    /// Shared Zenoh session (Arc-wrapped, safe to clone)
    pub session: Arc<zenoh::Session>,

    /// Deployment scope (from BUBBALOOP_SCOPE env, default: "local")
    pub scope: String,

    /// Machine identifier (from BUBBALOOP_MACHINE_ID env, default: hostname)
    pub machine_id: String,

    /// Shutdown signal receiver -- select! on this in your main loop
    pub shutdown_rx: tokio::sync::watch::Receiver<()>,
}

impl NodeContext {
    /// Helper to build a fully-qualified scoped topic:
    /// `bubbaloop/{scope}/{machine_id}/{suffix}`
    pub fn topic(&self, suffix: &str) -> String {
        format!("bubbaloop/{}/{}/{}", self.scope, self.machine_id, suffix)
    }
}
```

## `run_node` Implementation (SDK Internal)

```rust
pub async fn run_node<N: Node>() -> anyhow::Result<()> {
    // 1. Init logging
    env_logger::Builder::from_env(
        env_logger::Env::default().default_filter_or("info")
    ).init();

    // 2. Parse CLI args (built-in: -c config, -e endpoint)
    let args: SdkArgs = argh::from_env();

    // 3. Load config
    let config: N::Config = load_config(&args.config)?;

    // 4. Resolve scope + machine_id
    let scope = std::env::var("BUBBALOOP_SCOPE")
        .unwrap_or_else(|_| "local".to_string());
    let machine_id = std::env::var("BUBBALOOP_MACHINE_ID")
        .unwrap_or_else(|_| hostname::get()
            .map(|h| h.to_string_lossy().to_string())
            .unwrap_or_else(|_| "unknown".to_string()));

    // 5. Create shutdown channel
    let shutdown_tx = tokio::sync::watch::Sender::new(());
    {
        let tx = shutdown_tx.clone();
        ctrlc::set_handler(move || {
            log::info!("Shutdown signal received");
            let _ = tx.send(());
        })?;
    }

    // 6. Open Zenoh session (client mode, enforced)
    let session = open_zenoh_session(&args.endpoint).await?;

    // 7. Declare schema queryable
    let _schema = declare_schema_queryable(
        &session, &scope, &machine_id, N::name(), N::descriptor()
    ).await?;

    // 8. Spawn health heartbeat task
    let _health = spawn_health_heartbeat(
        session.clone(), &scope, &machine_id, N::name(),
        shutdown_tx.subscribe()
    ).await?;

    // 9. Build context
    let ctx = NodeContext {
        session: session.clone(),
        scope,
        machine_id,
        shutdown_rx: shutdown_tx.subscribe(),
    };

    // 10. Init node
    let node = N::init(&ctx, &config).await?;
    log::info!("{} node initialized", N::name());

    // 11. Run node (blocks until shutdown)
    node.run(ctx).await?;

    log::info!("{} node shut down", N::name());
    Ok(())
}
```

## Example: Minimal Sensor Node with the SDK

This is what a complete, production-ready sensor node looks like using the SDK:

```rust
// src/main.rs (~45 lines)

use anyhow::Result;
use bubbaloop_node_sdk::{Node, NodeContext};
use prost::Message;
use std::sync::Arc;
use tokio::time::{interval, Duration};

mod proto {
    include!(concat!(env!("OUT_DIR"), "/my_sensor.rs"));
}

const DESCRIPTOR: &[u8] = include_bytes!(concat!(env!("OUT_DIR"), "/descriptor.bin"));

#[derive(Debug, Clone, serde::Deserialize)]
pub struct Config {
    pub publish_topic: String,
    pub rate_hz: f64,
}

pub struct MySensor {
    publisher: zenoh::pubsub::Publisher<'static>,
    rate_hz: f64,
}

#[async_trait::async_trait]
impl Node for MySensor {
    type Config = Config;

    fn name() -> &'static str { "my-sensor" }
    fn descriptor() -> &'static [u8] { DESCRIPTOR }

    async fn init(ctx: &NodeContext, config: &Config) -> Result<Self> {
        let topic = ctx.topic(&config.publish_topic);
        let publisher = ctx.session.declare_publisher(&topic).await
            .map_err(|e| anyhow::anyhow!("Publisher error: {e}"))?;
        Ok(Self { publisher, rate_hz: config.rate_hz })
    }

    async fn run(self, ctx: NodeContext) -> Result<()> {
        let mut shutdown_rx = ctx.shutdown_rx.clone();
        let mut tick = interval(Duration::from_secs_f64(1.0 / self.rate_hz));
        let mut seq: u32 = 0;

        loop {
            tokio::select! {
                biased;
                _ = shutdown_rx.changed() => break,
                _ = tick.tick() => {
                    let reading = proto::SensorReading {
                        value: read_sensor(),
                        sequence: seq,
                    };
                    self.publisher.put(reading.encode_to_vec()).await.ok();
                    seq = seq.wrapping_add(1);
                }
            }
        }
        Ok(())
    }
}

fn read_sensor() -> f64 { 42.0 } // Replace with real sensor logic

#[tokio::main]
async fn main() -> Result<()> {
    bubbaloop_node_sdk::run_node::<MySensor>().await
}
```

### Comparison: Before vs After

| Aspect | Without SDK (~300 lines) | With SDK (~50 lines) |
|--------|-------------------------|---------------------|
| Zenoh session setup | 12 lines (config, mode, endpoint, open) | 0 lines (SDK handles it) |
| Health heartbeat | 15 lines (publisher, interval, select branch) | 0 lines (SDK background task) |
| Schema queryable | 12 lines (key, declare, callback) | 1 line (`fn descriptor()`) |
| Config loading | 15 lines (file read, parse, defaults, errors) | 1 line (`type Config = MyConfig`) |
| Signal handling | 8 lines (ctrlc, channel, handler) | 0 lines (SDK handles it) |
| CLI arg parsing | 15 lines (struct, defaults, from_env) | 0 lines (SDK provides -c, -e) |
| Scope/machine resolution | 6 lines (env vars, hostname fallback) | 0 lines (SDK provides via ctx) |
| Logging init | 3 lines | 0 lines (SDK handles it) |
| **Total scaffolding** | **~86 lines** (repeated per node) | **2 lines** (trait + main) |
| **Business logic** | Same | Same |

## Template System

The existing `bubbaloop node init --template rust` scaffolding (in `crates/bubbaloop/src/templates.rs`) generates nodes with full boilerplate. With the SDK, the templates should be updated to generate SDK-based nodes instead.

### Proposed Template: `sensor`

```bash
bubbaloop node init my-sensor -t rust -d "My sensor node"
```

Generates a working node scaffold that uses the SDK:

```
my-sensor/
  Cargo.toml          # depends on bubbaloop-node-sdk, prost, bubbaloop-schemas
  build.rs            # standard proto compilation
  config.yaml         # publish_topic + rate_hz
  node.yaml           # manifest for daemon registration
  pixi.toml           # build/run tasks
  protos/
    header.proto      # copied from canonical source
    my_sensor.proto   # node-specific proto with Header import
  src/
    main.rs           # ~45 lines using the SDK
    proto.rs           # include! generated types
```

### Future Templates

Additional templates can be added for common patterns:

| Template | Use Case | Extra Scaffolding |
|----------|----------|-------------------|
| `sensor` | Periodic data publisher | Tick interval, single publisher |
| `subscriber` | Data consumer / processor | Subscriber, message decode loop |
| `bridge` | Subscribe + transform + republish | Subscriber + publisher pair |
| `service` | Request/reply queryable | Zenoh queryable handler |

Templates are stored in `~/.bubbaloop/templates/` or embedded in the binary. The `sensor` template is the default.

## Crate Layout Summary

```
bubbaloop-node-sdk/
  Cargo.toml
  src/
    lib.rs            # Node trait, run_node(), NodeContext, re-exports
    config.rs         # load_config<C: DeserializeOwned>(path) -> Result<C>
    zenoh.rs          # open_zenoh_session(endpoint) -> Result<Arc<Session>>
    health.rs         # spawn_health_heartbeat(...) -> JoinHandle
    schema.rs         # declare_schema_queryable(...) -> Result<Queryable>
    shutdown.rs       # setup_shutdown() -> (Sender, Receiver)
    context.rs        # NodeContext struct + topic() helper
```

### `lib.rs` Public API Surface

```rust
// The entire public API of the crate:

pub trait Node { ... }
pub struct NodeContext { ... }
pub async fn run_node<N: Node>() -> anyhow::Result<()>;

// Re-exports for convenience (so nodes don't need to add these deps):
pub use zenoh;
pub use prost;
pub use tokio;
pub use anyhow;
pub use log;
```

## Design Decisions

### Why a Trait, Not a Macro

A `#[derive(Node)]` macro was considered but rejected. The `Node` trait is simpler to understand, debug, and document. Proc macros add compile-time cost and obscure errors. The trait approach also makes it easy to test nodes by constructing a `NodeContext` manually.

### Why Not in the Workspace

Following the same pattern as `bubbaloop-schemas`, the SDK crate lives outside the cargo workspace. Nodes depend on it via git:

```toml
bubbaloop-node-sdk = { git = "https://github.com/kornia/bubbaloop.git", branch = "main" }
```

This keeps the SDK decoupled from the daemon's release cycle and avoids circular dependencies.

### Why async_trait

The `Node` trait uses `#[async_trait]` because Rust's native async trait support still has limitations with trait objects and `Send` bounds. When the ecosystem matures, this can be replaced with native async traits.

### Why Not Support Python

The SDK is Rust-only. Python nodes (like `network-monitor`) have a different ecosystem (no trait system, different package management). A Python SDK would be a separate `bubbaloop-node-sdk-py` package using a similar pattern but with Python idioms (base class, decorators). That is out of scope for this design.

## Open Questions

1. **Should the SDK re-export `async_trait`?** Leaning yes, to reduce boilerplate `Cargo.toml` entries for node authors.
2. **Should `NodeContext` include a pre-built `Header` factory?** A `ctx.header(frame_id, sequence)` helper could further reduce per-message boilerplate.
3. **Should the SDK validate config beyond deserialization?** A `Validate` trait with `fn validate(&self) -> Result<()>` could be optionally implemented by config structs.
4. **Should `run_node` accept custom CLI args?** An extended variant like `run_node_with_args::<MyNode, MyArgs>()` could support nodes that need extra flags beyond `-c` and `-e`.
