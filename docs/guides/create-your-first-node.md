# Create Your First Node

This guide walks you through creating a custom Bubbaloop node that publishes data via Zenoh.

## Prerequisites

- [Pixi](https://pixi.sh) installed
- Rust toolchain (installed via pixi)
- `zenohd` running locally
- `bubbaloop` CLI built and in your PATH

## Step 1: Scaffold

```bash
bubbaloop node init my-sensor --type rust -d "My custom sensor node"
cd my-sensor
```

This creates:

```
my-sensor/
  Cargo.toml         # Standalone crate with bubbaloop-schemas dependency
  node.yaml          # Node manifest
  pixi.toml          # Build environment
  .cargo/config.toml # Local target directory
  config.yaml        # Runtime configuration
  src/
    main.rs          # Entry point with CLI args and shutdown handling
    node.rs          # Node logic (edit this)
```

## Step 2: Implement Your Logic

Edit `src/node.rs`. The generated scaffold provides a basic pub/sub structure.

To use Bubbaloop protobuf types (e.g., for publishing camera-compatible messages):

```rust
use bubbaloop_schemas::Header;
use prost::Message;

fn create_header(sequence: u64, node_name: &str) -> Header {
    Header {
        acq_time: std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_nanos() as u64)
            .unwrap_or(0),
        pub_time: 0,
        sequence,
        frame_id: node_name.to_string(),
        ..Default::default()
    }
}
```

Nodes publish using vanilla Zenoh topics with the standard format:

```
bubbaloop/{scope}/{machine_id}/{node_name}/{resource}
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

- Add health heartbeats (publish to `bubbaloop/{scope}/{machine_id}/health/{node-name}`)
- Add configuration via `config.yaml`
- Add service dependencies in `node.yaml` with `depends_on`
- See [Node Marketplace](node-marketplace.md) for publishing to the community
