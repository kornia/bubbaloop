# Skillet Development Guide

> A "skillet" is a self-describing sensor or actuator capability in bubbaloop. Historically called "nodes", skillets are the core building blocks of the platform.

## What is a Skillet?

A skillet is an autonomous process that:
- Connects to the Zenoh data plane
- Publishes sensor data (protobuf-encoded)
- Serves a manifest describing its capabilities
- Responds to commands from AI agents via MCP
- Reports health via periodic heartbeats
- Manages its own lifecycle (start, stop, restart)

Skillets run as systemd user services managed by the bubbaloop daemon. They can run on any machine — the daemon scopes all topics by `scope` and `machine_id` for multi-machine deployments.

> **Recommended:** Use the [Node SDK](#node-sdk-recommended) to create Rust skillets with ~50 lines of code. The manual approach below is for advanced use cases or Python nodes.

## Anatomy of a Skillet

### Required Components

1. **node.yaml** — Marketplace metadata (name, version, type, description, author, build, command, capabilities, publishes, requires)
2. **config.yaml** — Runtime instance parameters (publish_topic, rate_hz, node-specific fields)
3. **protos/** — Protobuf schema definitions for messages
4. **build system** — pixi.toml with build/run tasks
5. **health heartbeat** — Periodic publish to `bubbaloop/{scope}/{machine}/health/{name}`
6. **schema queryable** — Serves FileDescriptorSet at `{prefix}/schema`
7. **signal handling** — Graceful shutdown on SIGINT/SIGTERM

### Zenoh Topics

Every skillet operates within a scoped topic hierarchy:

```
bubbaloop/{scope}/{machine_id}/{node_name}/schema      → FileDescriptorSet bytes
bubbaloop/{scope}/{machine_id}/{node_name}/manifest    → JSON manifest
bubbaloop/{scope}/{machine_id}/{node_name}/health      → "ok" | error details
bubbaloop/{scope}/{machine_id}/{node_name}/config      → JSON config (GET/SET)
bubbaloop/{scope}/{machine_id}/{node_name}/command     → JSON command interface

bubbaloop/{scope}/{machine_id}/{publish_topic}         → Protobuf sensor data
bubbaloop/{scope}/{machine_id}/health/{node_name}      → Periodic heartbeat
```

**Environment variables:**
- `BUBBALOOP_SCOPE` (default: `"local"`) — Deployment context (site, fleet, etc.)
- `BUBBALOOP_MACHINE_ID` (default: hostname) — Machine identifier

**Topic naming rules:**
- Only specify the suffix in `config.yaml`: `publish_topic: my-node/data`
- Validate against `^[a-zA-Z0-9/_\-\.]+$` — reject anything else
- Reserved tokens: `health`, `daemon`, `camera`, `fleet`, `coordination`, `_global`

### Manifest Format

The manifest is a JSON document served via Zenoh queryable that describes the skillet's capabilities:

```json
{
  "name": "network-monitor",
  "version": "0.2.0",
  "type": "python",
  "description": "Network connectivity monitor",
  "capabilities": ["sensor"],
  "publishes": [
    {
      "suffix": "network-monitor/status",
      "schema_type": "bubbaloop.network_monitor.NetworkStatus",
      "rate_hz": 0.1,
      "description": "Network health status"
    }
  ],
  "commands": [
    {
      "name": "ping_host",
      "description": "Ping a specific host",
      "parameters": {"host": "string", "count": "integer"}
    }
  ],
  "requires": {
    "hardware": ["network"]
  }
}
```

### Schema Contract (Protobuf Skillets)

Every skillet that publishes protobuf messages **MUST** serve its FileDescriptorSet via a Zenoh queryable. This enables runtime schema discovery for dashboards, AI agents, and cross-node type checking.

**Requirements:**

1. **Declare schema queryable** at `{node-name}/schema` (relative to topic prefix):
   ```rust
   // Rust: NEVER use .complete(true) — blocks wildcard discovery
   let schema_queryable = session
       .declare_queryable(format!("{}/schema", topic_prefix))
       .await?;

   // Python: NEVER use complete=True
   queryable = session.declare_queryable(f"{topic_prefix}/schema")
   ```

2. **Serve FileDescriptorSet bytes** (not JSON):
   ```rust
   // Rust: include compiled descriptor
   pub const DESCRIPTOR: &[u8] = include_bytes!(concat!(env!("OUT_DIR"), "/descriptor.bin"));
   query.reply(query.key_expr().clone(), DESCRIPTOR).await?;

   // Python: reply with query.key_expr property (NOT method)
   with open("descriptor.bin", "rb") as f:
       descriptor_bytes = f.read()
   query.reply(query.key_expr, descriptor_bytes)  # key_expr is a PROPERTY
   ```

3. **Compile descriptor.bin** via `build.rs` (Rust) or protoc (Python):
   ```rust
   // build.rs
   prost_build::Config::new()
       .file_descriptor_set_path(out_dir.join("descriptor.bin"))
       .compile_protos(&["protos/my_node.proto", "protos/header.proto"], &["protos/"])?;
   ```

4. **Include all .proto files** the skillet uses (including `header.proto` from bubbaloop-schemas):
   - Copy header.proto into node's `protos/` directory
   - Reference it in your message definitions: `import "header.proto";`

**Why this matters:**

- Dashboard auto-discovers all schemas via wildcard query `bubbaloop/**/schema`
- AI agents can introspect message types without reading source code
- Cross-node type safety: verify sender/receiver compatibility at runtime
- Version detection: dashboard can warn about schema mismatches

**Common mistakes** (caught by `./scripts/validate.sh`):

- Using `.complete(true)` in Rust queryables
- Using `complete=True` in Python queryables
- Using `query.key_expr()` as method in Python (it's a property)
- Serving JSON instead of raw FileDescriptorSet bytes
- Missing `header.proto` in FileDescriptorSet

## Creating a Rust Skillet

### Step 1: Scaffold

Use the bubbaloop CLI to generate boilerplate:

```bash
bubbaloop node init my-sensor --template sensor --output ./my-sensor
cd my-sensor
```

This creates:
```
my-sensor/
  Cargo.toml          # Dependencies: zenoh, prost, bubbaloop-schemas
  build.rs            # Proto compilation
  config.yaml         # Runtime instance params
  node.yaml           # Marketplace metadata
  pixi.toml           # Build/run tasks
  protos/
    header.proto      # Shared Header contract
    my_sensor.proto   # Node-specific messages
  src/
    main.rs           # Entry point
    proto.rs          # include! generated types
```

### Step 2: Define your proto

Edit `protos/my_sensor.proto`:

```protobuf
syntax = "proto3";

package bubbaloop.my_sensor.v1;

import "header.proto";

message SensorReading {
    bubbaloop.header.v1.Header header = 1;

    double temperature = 2;
    double humidity = 3;
    double pressure = 4;
    uint32 sequence = 5;
}
```

### Step 3: Implement the main loop

Example from `openmeteo` skillet:

```rust
// src/main.rs
use anyhow::Result;
use argh::FromArgs;
use prost::Message;
use std::sync::Arc;
use tokio::time::{interval, Duration};

mod proto {
    include!(concat!(env!("OUT_DIR"), "/bubbaloop.my_sensor.v1.rs"));
}

/// FileDescriptorSet for this node's protobuf schemas
const DESCRIPTOR: &[u8] = include_bytes!(concat!(env!("OUT_DIR"), "/descriptor.bin"));

#[derive(FromArgs)]
/// My sensor data publisher for Zenoh
struct Args {
    /// path to the configuration file
    #[argh(option, short = 'c')]
    config: Option<String>,

    /// zenoh router endpoint to connect to
    /// Default: tcp/127.0.0.1:7447 (local zenohd router)
    #[argh(option, short = 'e', default = "String::from(\"tcp/127.0.0.1:7447\")")]
    endpoint: String,
}

#[tokio::main]
async fn main() -> Result<()> {
    // Initialize logging
    let env = env_logger::Env::default().default_filter_or("info");
    env_logger::init_from_env(env);

    let args: Args = argh::from_env();

    // Load config (from YAML, see Step 4)
    let config = load_config(&args.config)?;

    // Create shutdown channel
    let shutdown_tx = tokio::sync::watch::Sender::new(());

    // Set up Ctrl+C handler
    {
        let shutdown_tx = shutdown_tx.clone();
        ctrlc::set_handler(move || {
            log::info!("Received Ctrl+C, shutting down gracefully...");
            let _ = shutdown_tx.send(());
        })
        .expect("Error setting Ctrl+C handler");
    }

    // Read scope/machine env vars
    let scope = std::env::var("BUBBALOOP_SCOPE")
        .unwrap_or_else(|_| "local".to_string());
    let machine_id = std::env::var("BUBBALOOP_MACHINE_ID")
        .unwrap_or_else(|_| {
            hostname::get()
                .map(|h| h.to_string_lossy().to_string())
                .unwrap_or_else(|_| "unknown".to_string())
        });
    log::info!("Scope: {}, Machine ID: {}", scope, machine_id);

    // Initialize Zenoh session in client mode
    let endpoint = std::env::var("ZENOH_ENDPOINT").unwrap_or(args.endpoint);
    log::info!("Connecting to Zenoh at: {}", endpoint);
    let mut zenoh_config = zenoh::Config::default();
    zenoh_config.insert_json5("mode", r#""client""#).unwrap();
    zenoh_config
        .insert_json5("connect/endpoints", &format!(r#"["{}"]"#, endpoint))
        .unwrap();
    let zenoh_session = Arc::new(zenoh::open(zenoh_config).await?);

    // Declare schema queryable (NO .complete(true)!)
    let schema_key = format!(
        "bubbaloop/{}/{}/my-sensor/schema",
        scope, machine_id
    );
    let _schema_queryable = zenoh_session
        .declare_queryable(&schema_key)
        .callback({
            let descriptor = DESCRIPTOR.to_vec();
            move |query| {
                let _ = query.reply(&query.key_expr().clone(), descriptor.as_slice());
            }
        })
        .await?;
    log::info!("Schema queryable: {}", schema_key);

    // Declare data publisher
    let data_topic = format!(
        "bubbaloop/{}/{}/{}",
        scope, machine_id, config.publish_topic
    );
    let publisher = zenoh_session.declare_publisher(&data_topic).await?;
    log::info!("Publishing to: {}", data_topic);

    // Health heartbeat publisher
    let health_topic = format!(
        "bubbaloop/{}/{}/health/my-sensor",
        scope, machine_id
    );
    let health_pub = zenoh_session.declare_publisher(&health_topic).await?;

    // Spawn health heartbeat task (5s interval)
    let health_task = tokio::spawn({
        let shutdown_rx = shutdown_tx.subscribe();
        async move {
            let mut ticker = interval(Duration::from_secs(5));
            let mut shutdown_rx = shutdown_rx;
            loop {
                tokio::select! {
                    biased;
                    _ = shutdown_rx.changed() => break,
                    _ = ticker.tick() => {
                        let _ = health_pub.put(b"alive").await;
                    }
                }
            }
        }
    });

    // Main sensing loop
    let mut shutdown_rx = shutdown_tx.subscribe();
    let mut tick = interval(Duration::from_secs_f64(1.0 / config.rate_hz));
    let mut seq: u32 = 0;

    loop {
        tokio::select! {
            biased;
            _ = shutdown_rx.changed() => break,
            _ = tick.tick() => {
                let now_ns = std::time::SystemTime::now()
                    .duration_since(std::time::UNIX_EPOCH)?
                    .as_nanos() as u64;

                let reading = proto::SensorReading {
                    header: Some(bubbaloop_schemas::header::v1::Header {
                        acq_time: now_ns,
                        pub_time: now_ns,
                        sequence: seq,
                        frame_id: "my-sensor".to_string(),
                        machine_id: machine_id.clone(),
                        scope: scope.clone(),
                    }),
                    temperature: read_temperature(),
                    humidity: read_humidity(),
                    pressure: read_pressure(),
                    sequence: seq,
                };

                publisher.put(reading.encode_to_vec()).await.ok();
                seq = seq.wrapping_add(1);
            }
        }
    }

    // Clean shutdown
    health_task.abort();
    log::info!("my-sensor node shut down, exiting");
    Ok(())
}

fn read_temperature() -> f64 { 22.5 } // Replace with real sensor
fn read_humidity() -> f64 { 45.0 }
fn read_pressure() -> f64 { 1013.25 }
```

### Step 4: Configure build.rs

```rust
// build.rs
fn main() -> Result<(), Box<dyn std::error::Error>> {
    let protos_dir = std::path::Path::new("protos");
    if !protos_dir.exists() {
        return Ok(());
    }

    let proto_files: Vec<_> = std::fs::read_dir(protos_dir)?
        .filter_map(|e| e.ok())
        .map(|e| e.path())
        .filter(|p| p.extension().is_some_and(|ext| ext == "proto"))
        .collect();

    if proto_files.is_empty() {
        return Ok(());
    }

    let out_dir = std::path::PathBuf::from(std::env::var("OUT_DIR")?);

    let proto_strs: Vec<_> = proto_files.iter().filter_map(|p| p.to_str()).collect();
    prost_build::Config::new()
        .extern_path(".bubbaloop.header.v1", "::bubbaloop_schemas::header::v1")
        .type_attribute(".", "#[derive(serde::Serialize, serde::Deserialize)]")
        .file_descriptor_set_path(out_dir.join("descriptor.bin"))
        .compile_protos(&proto_strs, &["protos/"])?;

    for f in &proto_files {
        println!("cargo:rerun-if-changed={}", f.display());
    }
    println!("cargo:rerun-if-changed=protos");
    Ok(())
}
```

### Step 5: Add Cargo.toml dependencies

```toml
[package]
name = "my-sensor"
version = "0.1.0"
edition = "2021"
description = "My sensor data publisher for Zenoh"

[dependencies]
anyhow = "1.0"
argh = "0.1"
ctrlc = "3.4"
env_logger = "0.11"
log = "0.4"
tokio = { version = "1", features = ["full"] }
zenoh = "1.7"
prost = "0.14"
prost-types = "0.14"
serde = { version = "1.0", features = ["derive"] }
serde_yaml = "0.9"
hostname = "0.4"
bubbaloop-schemas = { git = "https://github.com/kornia/bubbaloop.git", branch = "main" }

[build-dependencies]
prost-build = "0.14"
```

### Step 6: Test locally

```bash
# Build
pixi run build

# Run (connects to local zenohd)
pixi run run -c config.yaml

# In another terminal: verify health heartbeat
z_sub -k "bubbaloop/local/*/health/my-sensor"

# Verify data publishing
z_sub -k "bubbaloop/local/*/my-sensor/*"
```

### Step 7: Install and run via daemon

```bash
# Register with daemon
bubbaloop node add .

# Start as systemd service
bubbaloop node start my-sensor

# View logs
bubbaloop node logs my-sensor -f

# Check status
bubbaloop node list

# Stop
bubbaloop node stop my-sensor
```

## Creating a Python Skillet

Python skillets follow the same contract but use `eclipse-zenoh` and `protobuf` for serialization.

### Step 1: Scaffold

```bash
bubbaloop node init my-sensor --template python --output ./my-sensor
cd my-sensor
```

This creates:
```
my-sensor/
  main.py             # Entry point
  build_proto.py      # Proto compilation script
  config.yaml         # Runtime instance params
  node.yaml           # Marketplace metadata
  pixi.toml           # Build/run tasks
  protos/
    header.proto      # Shared Header contract
    my_sensor.proto   # Node-specific messages
```

### Step 2: Define your proto

Same as Rust (Step 2 above).

### Step 3: Implement the main loop

Example from `network-monitor` skillet:

```python
#!/usr/bin/env python3
"""my-sensor node - Temperature/humidity sensor publisher"""

import argparse
import json
import logging
import signal
import socket
import sys
import time
from datetime import datetime, timezone
from pathlib import Path

import yaml
import zenoh

# Configure logging
logging.basicConfig(
    level=logging.INFO,
    format="%(asctime)s [%(levelname)s] %(message)s",
    datefmt="%Y-%m-%d %H:%M:%S",
)
logger = logging.getLogger(__name__)

# Import generated protobuf modules
# Run `python build_proto.py` first to generate these
try:
    import header_pb2
    import my_sensor_pb2
except ImportError:
    logger.error(
        "Protobuf modules not found. Run 'python build_proto.py' first."
    )
    sys.exit(1)


class MySensorNode:
    """Temperature/humidity sensor publisher."""

    def __init__(self, config_path: Path, endpoint: str | None = None):
        # Load configuration
        if config_path.exists():
            with open(config_path) as f:
                self.config = yaml.safe_load(f)
        else:
            logger.warning(f"Config file not found: {config_path}, using defaults")
            self.config = {
                "publish_topic": "my-sensor/data",
                "rate_hz": 1.0,
            }

        # Resolve scope and machine_id from env vars
        import os
        self.scope = os.environ.get("BUBBALOOP_SCOPE", "local")
        self.machine_id = os.environ.get(
            "BUBBALOOP_MACHINE_ID", socket.gethostname()
        )

        # Setup zenoh
        zenoh_config = zenoh.Config()
        if endpoint:
            zenoh_config.insert_json5("connect/endpoints", json.dumps([endpoint]))

        self.session = zenoh.open(zenoh_config)
        logger.info("Connected to zenoh")

        # Build scoped topic: bubbaloop/{scope}/{machine_id}/{publish_topic}
        topic_suffix = self.config["publish_topic"]
        self.full_topic = f"bubbaloop/{self.scope}/{self.machine_id}/{topic_suffix}"

        # Setup publishers
        self.publisher = self.session.declare_publisher(self.full_topic)
        logger.info(f"Publishing to: {self.full_topic}")

        self.health_publisher = self.session.declare_publisher(
            f"bubbaloop/{self.scope}/{self.machine_id}/health/my-sensor"
        )

        # Declare schema queryable (NO complete=True!)
        descriptor_path = Path(__file__).parent / "descriptor.bin"
        if descriptor_path.exists():
            self.descriptor_bytes = descriptor_path.read_bytes()
            schema_key = f"bubbaloop/{self.scope}/{self.machine_id}/my-sensor/schema"
            self.schema_queryable = self.session.declare_queryable(
                schema_key,
                lambda query: query.reply(query.key_expr, self.descriptor_bytes),
            )
            logger.info(f"Schema queryable: {schema_key}")
        else:
            self.descriptor_bytes = None
            self.schema_queryable = None
            logger.warning("descriptor.bin not found, schema queryable not available")

        self.hostname = socket.gethostname()
        self.running = True
        self.sequence = 0

    def process(self) -> bytes:
        """Read sensor and return serialized SensorReading."""
        now_ns = int(datetime.now(timezone.utc).timestamp() * 1e9)

        reading = my_sensor_pb2.SensorReading()
        reading.header.CopyFrom(
            header_pb2.Header(
                acq_time=now_ns,
                pub_time=now_ns,
                sequence=self.sequence,
                frame_id="my-sensor",
                machine_id=self.machine_id,
                scope=self.scope,
            )
        )
        reading.temperature = read_temperature()
        reading.humidity = read_humidity()
        reading.pressure = read_pressure()
        reading.sequence = self.sequence

        return reading.SerializeToString()

    def run(self):
        """Run the node main loop."""
        interval = 1.0 / self.config.get("rate_hz", 1.0)
        logger.info(
            f"my-sensor node started (rate: {self.config.get('rate_hz', 1.0)} Hz)"
        )

        while self.running:
            output = self.process()
            self.publisher.put(output)

            # Health heartbeat
            self.health_publisher.put(b"alive")

            if self.sequence % 10 == 0:
                logger.debug(f"Published reading seq={self.sequence}")

            self.sequence += 1
            time.sleep(interval)

        logger.info("my-sensor node stopped")

    def stop(self):
        """Stop the node."""
        self.running = False

    def close(self):
        """Clean up resources."""
        self.publisher.undeclare()
        self.health_publisher.undeclare()
        if self.schema_queryable is not None:
            self.schema_queryable.undeclare()
        self.session.close()


def read_temperature() -> float:
    return 22.5  # Replace with real sensor


def read_humidity() -> float:
    return 45.0


def read_pressure() -> float:
    return 1013.25


def main():
    parser = argparse.ArgumentParser(
        description="Temperature/humidity sensor publisher"
    )
    parser.add_argument(
        "-c",
        "--config",
        type=Path,
        default=Path("config.yaml"),
        help="Path to configuration file",
    )
    parser.add_argument(
        "-e",
        "--endpoint",
        type=str,
        default="tcp/127.0.0.1:7447",
        help="Zenoh endpoint to connect to (default: tcp/127.0.0.1:7447)",
    )
    args = parser.parse_args()

    node = MySensorNode(args.config, args.endpoint)

    # Setup signal handlers
    def signal_handler(signum, frame):
        logger.info("Shutdown signal received")
        node.stop()

    signal.signal(signal.SIGINT, signal_handler)
    signal.signal(signal.SIGTERM, signal_handler)

    try:
        node.run()
    finally:
        node.close()


if __name__ == "__main__":
    main()
```

### Step 4: Build proto compiler script

```python
# build_proto.py
#!/usr/bin/env python3
"""Compile protobuf schemas for network-monitor node."""

import os
import subprocess
import sys
from pathlib import Path

def main():
    # Directories
    script_dir = Path(__file__).parent
    protos_dir = script_dir / "protos"

    if not protos_dir.exists():
        print("No protos/ directory found, skipping compilation")
        return 0

    # Find all .proto files
    proto_files = list(protos_dir.glob("*.proto"))
    if not proto_files:
        print("No .proto files found in protos/")
        return 0

    print(f"Compiling {len(proto_files)} proto files...")

    # Run protoc
    cmd = [
        "protoc",
        f"--proto_path={protos_dir}",
        f"--python_out={script_dir}",
        f"--descriptor_set_out={script_dir / 'descriptor.bin'}",
        "--include_imports",
        "--include_source_info",
    ]
    cmd.extend(str(f) for f in proto_files)

    try:
        subprocess.run(cmd, check=True)
        print("Proto compilation successful")
        return 0
    except subprocess.CalledProcessError as e:
        print(f"Proto compilation failed: {e}", file=sys.stderr)
        return 1
    except FileNotFoundError:
        print("protoc not found. Install: pip install grpcio-tools", file=sys.stderr)
        return 1

if __name__ == "__main__":
    sys.exit(main())
```

### Step 5: Test locally

```bash
# Compile protos
python build_proto.py

# Run
python main.py -c config.yaml

# In another terminal: verify
z_sub -k "bubbaloop/local/*/health/my-sensor"
z_sub -k "bubbaloop/local/*/my-sensor/*"
```

### Step 6: Install and run via daemon

Same as Rust (Step 7 above).

## node.yaml Format

The `node.yaml` file is the marketplace metadata that describes your skillet:

```yaml
name: my-sensor
version: "0.1.0"
type: rust  # or "python"
description: Temperature/humidity sensor publisher
author: Your Name
build: pixi run build  # Rust: compiles binary; Python: compiles protos
command: ./target/release/my_sensor_node  # Rust binary path
# OR for Python:
# command: pixi run run  # Runs main.py via pixi

# Skill capabilities
capabilities:
  - sensor

# Topics this node publishes
publishes:
  - suffix: my-sensor/data
    description: "Temperature and humidity readings"
    schema_type: "bubbaloop.my_sensor.v1.SensorReading"
    rate_hz: 1.0

# Hardware/software requirements
requires:
  hardware:
    - network
```

**Required fields:**
- `name` — 1-64 chars, `[a-zA-Z0-9_-]`, matches directory name
- `version` — Semantic version (e.g., `"0.1.0"`)
- `type` — `"rust"` or `"python"`
- `description` — Human-readable description
- `author` — Your name or team
- `build` — Command to build the skillet
- `command` — Command to run the skillet

**Optional fields:**
- `capabilities` — List of skill types: `sensor`, `actuator`, `processor`, `gateway`
- `publishes` — List of topics with suffix, description, schema_type, rate_hz
- `requires` — Hardware/software dependencies (hardware: network, camera, gpio, etc.)

## Best Practices

### 1. Always use Zenoh client mode (not peer)

Peer mode does not route through the zenohd router, breaking topic discovery.

```rust
// Rust
let mut zenoh_config = zenoh::Config::default();
zenoh_config.insert_json5("mode", r#""client""#).unwrap();
```

```python
# Python
zenoh_config = zenoh.Config()
# Default is client mode, no explicit config needed
```

### 2. Never use .complete(true) on queryables

This breaks wildcard queries like `bubbaloop/**/schema` that the dashboard uses for discovery.

```rust
// ❌ WRONG
let queryable = session
    .declare_queryable("my-sensor/schema")
    .complete(true)  // BREAKS DISCOVERY
    .await?;

// ✅ CORRECT
let queryable = session
    .declare_queryable("my-sensor/schema")
    .await?;
```

```python
# ❌ WRONG
queryable = session.declare_queryable("my-sensor/schema", complete=True)

# ✅ CORRECT
queryable = session.declare_queryable("my-sensor/schema")
```

### 3. Include header.proto in your FileDescriptorSet

The `Header` message is the standard metadata envelope for all skillet messages:

```protobuf
message Header {
    uint64 acq_time = 1;    // Nanoseconds since Unix epoch (acquisition time)
    uint64 pub_time = 2;    // Nanoseconds since Unix epoch (publish time)
    uint32 sequence = 3;    // Monotonic sequence number
    string frame_id = 4;    // Frame/sensor identifier
    string machine_id = 5;  // Machine identifier
    string scope = 6;       // Deployment scope
}
```

Every skillet message should include a `Header` as the first field:

```protobuf
message SensorReading {
    bubbaloop.header.v1.Header header = 1;
    double temperature = 2;
    // ... other fields
}
```

### 4. Keep config in YAML, loaded at startup

Never hardcode configuration. Use `config.yaml` for all runtime parameters:

```yaml
# config.yaml
publish_topic: my-sensor/data
rate_hz: 1.0
sensor_port: /dev/ttyUSB0
calibration_offset: 2.5
```

Load at startup and validate all fields (bounds checking, required fields, format validation).

### 5. Graceful shutdown on SIGTERM

The daemon sends SIGTERM when stopping a skillet. Always handle it gracefully:

```rust
// Rust: tokio::sync::watch channel pattern
let shutdown_tx = tokio::sync::watch::Sender::new(());
ctrlc::set_handler(move || {
    let _ = shutdown_tx.send(());
})?;

// In main loop:
tokio::select! {
    _ = shutdown_rx.changed() => break,
    // ... other branches
}
```

```python
# Python: signal handlers
import signal

def signal_handler(signum, frame):
    node.stop()

signal.signal(signal.SIGINT, signal_handler)
signal.signal(signal.SIGTERM, signal_handler)
```

### 6. Publish health heartbeats every 5 seconds

The daemon marks a skillet unhealthy if no heartbeat arrives for 30 seconds. Publish every 5 seconds for safety margin:

```rust
// Rust: spawn background task
let health_task = tokio::spawn({
    let shutdown_rx = shutdown_tx.subscribe();
    async move {
        let mut ticker = interval(Duration::from_secs(5));
        loop {
            tokio::select! {
                _ = shutdown_rx.changed() => break,
                _ = ticker.tick() => {
                    let _ = health_pub.put(b"alive").await;
                }
            }
        }
    }
});
```

```python
# Python: inline in main loop
while self.running:
    # ... do work
    self.health_publisher.put(b"alive")
    time.sleep(interval)
```

### 7. Validate all config inputs

**Always** validate configuration fields to prevent runtime errors:

```rust
// Rust example
if !(0.01..=1000.0).contains(&config.rate_hz) {
    anyhow::bail!("rate_hz {} out of range (0.01-1000.0)", config.rate_hz);
}

if !TOPIC_RE.is_match(&config.publish_topic) {
    anyhow::bail!("publish_topic contains invalid characters");
}
```

```python
# Python example
import re
TOPIC_RE = re.compile(r"^[a-zA-Z0-9/_\-\.]+$")

topic = self.config.get("publish_topic", "")
if not TOPIC_RE.match(topic):
    raise ValueError(
        f"publish_topic '{topic}' contains invalid characters "
        f"(must match [a-zA-Z0-9/_\\-\\.]+)"
    )

rate_hz = self.config.get("rate_hz", 1.0)
if not (0.01 <= rate_hz <= 1000.0):
    raise ValueError(f"rate_hz {rate_hz} out of range (0.01-1000.0)")
```

### 8. Security checklist

- Validate topic names: `^[a-zA-Z0-9/_\-\.]+$`
- Enforce bounds checking on numeric config values
- Never bind to `0.0.0.0` — use `localhost` only
- Never store secrets in `config.yaml` — use environment variables
- Validate external endpoints (URL format, TLS certificates, timeout enforcement)

## Node SDK (Recommended)

The `bubbaloop-node-sdk` crate reduces Rust skillet boilerplate from ~300 lines to ~50 lines. It provides:

- Automatic Zenoh session creation (client mode, enforced)
- Automatic health heartbeat publishing (5s interval)
- Automatic schema queryable registration
- Automatic config file loading (YAML deserialization)
- Automatic signal handling (SIGTERM/SIGINT)
- Automatic scope/machine_id resolution
- Automatic logging initialization

**With the SDK, a complete skillet will look like this:**

```rust
use bubbaloop_node_sdk::{Node, NodeContext};
use anyhow::Result;

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
        let publisher = ctx.session.declare_publisher(&topic).await?;
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
                        // ... populate fields
                    };
                    self.publisher.put(reading.encode_to_vec()).await.ok();
                    seq = seq.wrapping_add(1);
                }
            }
        }
        Ok(())
    }
}

#[tokio::main]
async fn main() -> Result<()> {
    bubbaloop_node_sdk::run_node::<MySensor>().await
}
```

### SDK Quick Start

```bash
# 1. Scaffold a new node
bubbaloop node init my-sensor --type rust -d "My custom sensor"

# 2. Edit src/main.rs — implement init() and run()
#    The SDK handles everything else automatically

# 3. Build and register
pixi run build
bubbaloop node add .
bubbaloop node start my-sensor
```

### What the SDK Handles Automatically

| Component | Lines saved | What it does |
|-----------|-------------|--------------|
| Zenoh session | ~15 lines | Client mode, endpoint resolution, scouting disabled |
| Health heartbeat | ~15 lines | 5s interval publish to health topic |
| Schema queryable | ~12 lines | Serves FileDescriptorSet, no `.complete(true)` |
| Config loading | ~15 lines | YAML deserialization with clear errors |
| Signal handling | ~8 lines | SIGINT/SIGTERM via watch channel |
| CLI arguments | ~15 lines | `-c config.yaml -e endpoint` |
| Scope resolution | ~6 lines | BUBBALOOP_SCOPE + BUBBALOOP_MACHINE_ID |
| **Total saved** | **~86 lines** | Per node, automatically correct |

### Cargo.toml for SDK-Based Nodes

```toml
[dependencies]
bubbaloop-node-sdk = { git = "https://github.com/kornia/bubbaloop.git", branch = "main" }
bubbaloop-schemas = { git = "https://github.com/kornia/bubbaloop.git", branch = "main" }
prost = "0.14"
serde = { version = "1.0", features = ["derive"] }

[build-dependencies]
prost-build = "0.14"

[workspace]
```

**Status:** Shipped. See `crates/bubbaloop-node-sdk/` in the bubbaloop repo.

**Migration path:** Existing skillets continue to work unchanged. New Rust skillets should use the SDK. The SDK is a standalone crate (not in workspace), depended on via git, following the same pattern as `bubbaloop-schemas`.

## Complete Skillet Checklist

Before submitting a new skillet, verify ALL items:

### Structure
- [ ] `node.yaml` exists with: name, version, type, description, author, build, command
- [ ] `node.yaml` has `capabilities` field (sensor/actuator/processor/gateway)
- [ ] `node.yaml` has `publishes` field with topic suffix, description, schema_type, rate
- [ ] `node.yaml` has `requires` field for hardware/software dependencies
- [ ] `config.yaml` exists with: publish_topic, rate_hz, and node-specific fields
- [ ] `pixi.toml` exists with: build and run tasks
- [ ] `protos/` directory with `header.proto` and node-specific `.proto` files

### Communication
- [ ] Publishes data via Zenoh to scoped topic: `bubbaloop/{scope}/{machine}/{node-name}/{resource}`
- [ ] `config.yaml` specifies topic suffix only (no `bubbaloop/{scope}/{machine}/` prefix)
- [ ] Uses protobuf serialization for all data messages
- [ ] Publishes health heartbeat to `bubbaloop/{scope}/{machine}/health/{name}` (vanilla zenoh, not protobuf)
- [ ] Heartbeat interval <= 10 seconds (recommended: 5 seconds)
- [ ] Declares schema queryable at `{prefix}/schema` (serves FileDescriptorSet bytes)
- [ ] Schema queryable does NOT use `.complete(true)` (Rust) or `complete=True` (Python)

### Security
- [ ] Topic names validated: `^[a-zA-Z0-9/_\-\.]+$`
- [ ] Config numeric values have bounds checking
- [ ] External endpoints validated (URL format, TLS)
- [ ] No binding to 0.0.0.0
- [ ] No multicast or gossip scouting enabled
- [ ] No secrets in config.yaml
- [ ] Handles SIGINT/SIGTERM gracefully

### Code
- [ ] Rust: uses vanilla zenoh with prost for data pub/sub
- [ ] Python: uses `eclipse-zenoh` + `protobuf`, compiles protos via `build_proto.py`
- [ ] Accepts CLI flags: `-c config.yaml -e tcp/localhost:7447`
- [ ] Uses `Header` message pattern (acq_time, pub_time, sequence, frame_id, machine_id, scope)
- [ ] Reads `BUBBALOOP_SCOPE` env var (default: `local`) and `BUBBALOOP_MACHINE_ID` env var (default: hostname)

### Testing
- [ ] Rust: config validation has unit tests (`#[cfg(test)]` module)
- [ ] Rust: `cargo test` passes
- [ ] Python: config loading/validation has tests
- [ ] Manual integration test: verify health heartbeat with `z_sub`
- [ ] Manual integration test: verify data publishing with `z_sub`
- [ ] End-to-end test: register with daemon, verify `bubbaloop node list` shows HEALTHY

## Reference Skillets

**Rust examples:**
- `openmeteo/` — Weather data publisher (HTTP API, auto-discovery, complex config)
- `rtsp-camera/` — RTSP video stream publisher (hardware access, compression, unit tests)

**Python examples:**
- `network-monitor/` — Network connectivity checks (HTTP, DNS, ping)

**Best practices reference:**
- `rtsp-camera/` is the compliance reference — only skillet with full validation tests
- `openmeteo/` demonstrates config defaults and graceful degradation
- `network-monitor/` demonstrates Python patterns (protobuf compilation, signal handling)

## Getting Help

- **Architecture overview:** `/home/nvidia/bubbaloop/ARCHITECTURE.md` (lines 85-220: Node Contract)
- **Official skillets repo:** https://github.com/kornia/bubbaloop-nodes-official
- **Node SDK design:** `/home/nvidia/bubbaloop/docs/plans/2026-02-24-node-sdk-design.md`
- **Bubbaloop CLI reference:** `bubbaloop node --help`
- **Zenoh documentation:** https://zenoh.io/docs/
