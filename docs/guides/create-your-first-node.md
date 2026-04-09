---
description: "Create your first Bubbaloop node. Step-by-step Rust tutorial using the Node SDK to build a self-describing sensor node in ~50 lines of code."
---

# Create Your First Node

This guide walks you through creating a custom Bubbaloop node that publishes data via Zenoh.

## Prerequisites

- [Pixi](https://pixi.sh) installed
- Rust toolchain (installed via pixi)
- `zenohd` running locally
- `bubbaloop` CLI built and in your PATH

## Where to Create Your Node

**Every node is its own Git repository**, separate from the main bubbaloop repo. Create it anywhere on your filesystem:

```bash
# Pick any directory — your home, a projects folder, etc.
mkdir ~/my-nodes && cd ~/my-nodes
```

Your node will depend on `bubbaloop-node-sdk` and `bubbaloop-schemas` via Git (not local paths). This means you can develop, build, and publish independently.

## Step 1: Scaffold

```bash
bubbaloop node init my-sensor --type rust -d "My custom sensor node"
cd my-sensor
```

This creates:

```
my-sensor/
  Cargo.toml         # Depends on bubbaloop-node + bubbaloop-node-build + bubbaloop-schemas
  node.yaml          # Node manifest
  pixi.toml          # Build environment
  config.yaml        # Runtime configuration (publish_topic, rate_hz)
  build.rs           # Proto compilation (one-liner via bubbaloop-node-build)
  src/
    main.rs          # Node trait impl + run_node() (edit this)
```

## Step 2: Implement Your Logic

Edit `src/main.rs`. The generated scaffold uses the Node SDK — you only implement `init()` and `run()`:

```rust
use bubbaloop_node::{Node, NodeContext, JsonPublisher};

#[async_trait::async_trait]
impl Node for MySensorNode {
    type Config = Config;
    fn name() -> &'static str { "my-sensor" }
    fn descriptor() -> &'static [u8] { DESCRIPTOR }

    async fn init(ctx: &NodeContext, config: &Config) -> anyhow::Result<Self> {
        // SDK handles topic construction and encoding
        let publisher = ctx.publisher_json(&config.publish_topic).await?;
        Ok(Self { publisher, rate_hz: config.rate_hz })
    }

    async fn run(self, ctx: NodeContext) -> anyhow::Result<()> {
        let mut shutdown_rx = ctx.shutdown_rx.clone();
        let mut tick = tokio::time::interval(std::time::Duration::from_secs_f64(1.0 / self.rate_hz));
        loop {
            tokio::select! {
                biased;
                _ = shutdown_rx.changed() => break,
                _ = tick.tick() => {
                    // YOUR SENSOR LOGIC HERE
                    self.publisher.put(&reading).await.ok();
                }
            }
        }
        Ok(())
    }
}
```

The SDK automatically handles: Zenoh session, health heartbeat, schema queryable, config loading, signal handling, and logging. You focus on your sensor logic.

Topics use two key spaces — `global` for network-visible data, `local` for SHM-only:

```
bubbaloop/global/{machine_id}/{suffix}   ← visible to dashboard, CLI, remote machines
bubbaloop/local/{machine_id}/{suffix}    ← SHM zero-copy, same-machine only
```

## Step 3: Build

```bash
pixi run build
# or
cargo build --release
```

## Step 4: Test Locally

```bash
# Terminal 1: Start zenoh router
zenohd

# Terminal 2: Run your node
pixi run run
# or
./target/release/my-sensor_node -c config.yaml
```

Verify output with:

```bash
bubbaloop debug subscribe "my-sensor/output"
```

## Step 5: Register with Daemon

```bash
bubbaloop node add .
bubbaloop node build my-sensor
bubbaloop node install my-sensor
bubbaloop node start my-sensor
bubbaloop node logs my-sensor -f
```

## Step 6: Publish

Push your node to GitHub so others can install it:

```bash
git init && git add -A && git commit -m "Initial node"
gh repo create my-sensor --public --push --source .
```

Others install with:

```bash
bubbaloop node add your-username/my-sensor --build --install
```

## Available Schemas

The `bubbaloop-schemas` crate provides these protobuf types:

| Type | Module | Feature |
|------|--------|---------|
| `Header` | root | (default) |
| `CompressedImage`, `RawImage` | `camera` | (default) |
| `CurrentWeather`, `HourlyForecast`, `DailyForecast` | `weather` | (default) |
| `SystemMetrics` | `system_telemetry` | (default) |
| `NetworkStatus` | `network_monitor` | (default) |
| `TopicsConfig` | `config` | `config` |

## Next Steps

- The SDK handles health heartbeats automatically — no manual setup needed
- Add custom protobuf messages in `protos/` and reference them in `run()`
- Add node-specific config fields to your `Config` struct and `config.yaml`
- See [Node Development Guide](../node-development.md) for the full SDK reference
- See [Node Marketplace](node-marketplace.md) for publishing and discovery
