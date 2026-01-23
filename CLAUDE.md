# Bubbaloop

Physical AI camera streaming platform built on Zenoh/ROS-Z.

## Quick Start

```bash
# Build everything
pixi run cargo build --workspace

# Run dashboard
cd dashboard && pixi run npm run dev

# Run camera node
pixi run cargo run -p bubbaloop-plugins --bin cameras_node -- -c configs/cameras.yaml
```

## Architecture

```
crates/
├── bubbaloop/           # Core library: schemas + plugin SDK
├── bubbaloop-plugins/   # All official nodes (camera, weather, etc.)
└── bubbaloop_launch/    # Launch system orchestrator

dashboard/               # React dashboard (Vite + TypeScript)
launch/                  # YAML launch configurations
protos/                  # Protobuf schema definitions
```

## Plugin Development

Create a new Bubble by implementing `BubbleNode`:

```rust
use bubbaloop::prelude::*;

#[derive(Deserialize)]
struct MyConfig {
    topic: String,
}

struct MyNode {
    ctx: Arc<ZContext>,
    config: MyConfig,
}

#[async_trait]
impl BubbleNode for MyNode {
    type Config = MyConfig;

    fn metadata() -> BubbleMetadata {
        BubbleMetadata {
            name: "my-node",
            version: "0.1.0",
            description: "Example node",
            topics_published: &["/my/topic"],
            topics_subscribed: &[],
        }
    }

    fn new(ctx: Arc<ZContext>, config: Self::Config) -> Result<Self, NodeError> {
        Ok(Self { ctx, config })
    }

    async fn run(self, mut shutdown: watch::Receiver<()>) -> Result<(), NodeError> {
        loop {
            tokio::select! {
                _ = shutdown.changed() => break,
                // Your logic here
            }
        }
        Ok(())
    }
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    run_node::<MyNode>().await
}
```

## Key Dependencies

- **Zenoh** - Pub/sub messaging
- **ros-z** - ROS 2 compatibility layer
- **prost** - Protobuf serialization
- **GStreamer** - H264 camera capture

## Commands

```bash
# Build
pixi run cargo build --workspace

# Test
pixi run cargo test --workspace

# Run specific node
pixi run cargo run -p bubbaloop-plugins --bin cameras_node -- -c config.yaml

# Run with launch file
pixi run cargo run -p bubbaloop_launch -- launch/default.launch.yaml

# Dashboard dev server
cd dashboard && pixi run npm run dev
```
