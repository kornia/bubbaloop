# Bubbaloop Node Development Guide

Create custom nodes for Bubbaloop in Rust or Python. Nodes connect to the Zenoh pub/sub network to publish sensor data, process messages, or integrate with external services.

## Quick Start (Recommended: Node SDK)

The Node SDK handles all boilerplate. You write ~50 lines of business logic:

```bash
# Scaffold
bubbaloop node init my-sensor --type rust -d "My custom sensor"
cd my-sensor

# Edit src/main.rs — implement Node trait (init + run)
# Build and register
pixi run build
bubbaloop node add .
bubbaloop node start my-sensor
```

See the [Skillet Development Guide](skillet-development.md#node-sdk-recommended) for the full SDK reference.

### Manual Setup (Advanced)

For cases where you need full control over the Zenoh lifecycle:

---

## Rust Node (Manual Setup)

### Directory Structure

```
my-node/
├── Cargo.toml
├── node.yaml        # Node manifest
├── configs/         # Configuration files
└── src/
    └── main.rs
```

### Step 1: Cargo.toml

```toml
[package]
name = "my-node"
version = "0.1.0"
edition = "2021"

[[bin]]
name = "my_node"
path = "src/main.rs"

[dependencies]
# Bubbaloop SDK - provides BubbleNode trait
bubbaloop = { path = "../../bubbaloop" }

# Required
tokio = { version = "1", features = ["full"] }
serde = { version = "1", features = ["derive"] }
serde_json = "1"
async-trait = "0.1"
log = "0.4"
```

### Step 2: src/main.rs

```rust
//! my-node - A Bubbaloop node

use bubbaloop::prelude::*;
use serde::Serialize;
use std::time::Duration;

// ============================================================================
// CONFIGURATION - Edit this to add config options
// ============================================================================

#[derive(Debug, Deserialize)]
pub struct Config {
    #[serde(default = "default_topic")]
    pub topic: String,

    #[serde(default = "default_interval")]
    pub interval_secs: u64,

    // Add your config fields here:
    // pub api_key: String,
    // pub sensor_address: String,
}

fn default_topic() -> String { "my-node/data".to_string() }
fn default_interval() -> u64 { 60 }

// ============================================================================
// DATA FORMAT - Edit this to define your message structure
// ============================================================================

#[derive(Debug, Serialize)]
struct NodeData {
    value: f64,
    timestamp: u64,
    // Add your fields here:
    // temperature: f64,
    // humidity: f64,
    // status: String,
}

// ============================================================================
// NODE IMPLEMENTATION - Edit run() to add your logic
// ============================================================================

pub struct MyNode {
    ctx: Arc<ZContext>,
    config: Config,
}

#[async_trait]
impl BubbleNode for MyPluginNode {
    type Config = Config;

    fn metadata() -> BubbleMetadata {
        BubbleMetadata {
            name: "my-plugin",
            version: "0.1.0",
            description: "My custom plugin",
            topics_published: &["my-plugin/data"],
            topics_subscribed: &[],
        }
    }

    fn new(ctx: Arc<ZContext>, config: Self::Config) -> Result<Self, NodeError> {
        info!("Initializing my-plugin");
        Ok(Self { ctx, config })
    }

    async fn run(self, mut shutdown: watch::Receiver<()>) -> Result<(), NodeError> {
        // Get Zenoh session for pub/sub
        let session = self.ctx.session();

        // Create publisher
        let publisher = session
            .declare_publisher(&self.config.topic)
            .await
            .map_err(|e| NodeError::Zenoh(e.to_string()))?;

        info!("Publishing to: {}", self.config.topic);

        let interval = Duration::from_secs(self.config.interval_secs);

        loop {
            tokio::select! {
                // Graceful shutdown
                _ = shutdown.changed() => {
                    info!("Shutdown signal received");
                    break;
                }

                // Your main logic
                _ = tokio::time::sleep(interval) => {
                    // ========================================
                    // EDIT HERE: Your plugin logic
                    // ========================================
                    let data = PluginData {
                        value: 42.0,
                        timestamp: std::time::SystemTime::now()
                            .duration_since(std::time::UNIX_EPOCH)
                            .unwrap()
                            .as_secs(),
                    };

                    // Publish as JSON
                    let json = serde_json::to_string(&data)
                        .map_err(|e| NodeError::Runtime(e.to_string()))?;

                    publisher.put(json.as_bytes()).await
                        .map_err(|e| NodeError::Zenoh(e.to_string()))?;

                    debug!("Published: {:?}", data);
                }
            }
        }

        Ok(())
    }
}

// ============================================================================
// MAIN - No changes needed
// ============================================================================

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    run_node::<MyPluginNode>().await
}
```

### Step 3: config.yaml

```yaml
# Topic to publish to
topic: "my-plugin/data"

# Publish interval in seconds
interval_secs: 60

# Add your config here:
# api_key: "your-key"
# sensor_address: "/dev/ttyUSB0"
```

### Step 4: Build & Run

```bash
cargo build --release
./target/release/my-plugin -c config.yaml -e tcp/localhost:7447
```

---

## Python Plugin (Manual Setup)

### Directory Structure

```
my-plugin/
├── main.py
├── config.yaml
├── requirements.txt
└── plugin.yaml      # Optional: for MCP/discovery
```

### Step 1: requirements.txt

```
eclipse-zenoh>=1.0.0
pyyaml>=6.0
```

### Step 2: main.py

```python
#!/usr/bin/env python3
"""my-plugin - A Bubbaloop plugin"""

import argparse
import json
import logging
import signal
import threading
import time
from dataclasses import dataclass
from pathlib import Path
from typing import Any, Dict

import yaml
import zenoh

logging.basicConfig(
    level=logging.INFO,
    format="%(asctime)s - %(name)s - %(levelname)s - %(message)s",
)
logger = logging.getLogger("my-plugin")


# ============================================================================
# CONFIGURATION - Edit this to add config options
# ============================================================================

@dataclass
class Config:
    topic: str = "my-plugin/data"
    interval_secs: int = 60
    # Add your config fields here:
    # api_key: str = ""
    # sensor_address: str = ""

    @classmethod
    def from_dict(cls, data: Dict[str, Any]) -> "Config":
        return cls(
            topic=data.get("topic", cls.topic),
            interval_secs=data.get("interval_secs", cls.interval_secs),
            # api_key=data.get("api_key", cls.api_key),
        )


# ============================================================================
# PLUGIN NODE - Edit run() to add your logic
# ============================================================================

class MyPluginNode:
    """My custom plugin"""

    def __init__(self, session: zenoh.Session, config_dict: Dict[str, Any]):
        self.session = session
        self.config = Config.from_dict(config_dict)
        logger.info(f"Initializing my-plugin")
        logger.info(f"  Topic: {self.config.topic}")

    def run(self, shutdown_event: threading.Event) -> None:
        """Main loop - runs until shutdown signal."""
        publisher = self.session.declare_publisher(self.config.topic)
        logger.info(f"Publishing to: {self.config.topic}")

        counter = 0
        while not shutdown_event.is_set():
            try:
                # ========================================
                # EDIT HERE: Your plugin logic
                # ========================================
                data = {
                    "value": 42.0 + (counter * 0.1),
                    "timestamp": int(time.time()),
                }

                # Publish as JSON
                publisher.put(json.dumps(data))
                logger.debug(f"Published: {data}")
                counter += 1

                # Wait for interval or shutdown
                shutdown_event.wait(timeout=self.config.interval_secs)

            except Exception as e:
                logger.error(f"Error: {e}")
                shutdown_event.wait(timeout=1)

        logger.info("my-plugin stopped")


# ============================================================================
# MAIN - No changes needed
# ============================================================================

def load_config(path: str) -> Dict[str, Any]:
    """Load config from YAML file."""
    p = Path(path)
    if p.exists():
        with open(p) as f:
            return yaml.safe_load(f) or {}
    logger.warning(f"Config not found: {path}, using defaults")
    return {}


def main():
    parser = argparse.ArgumentParser(description="my-plugin")
    parser.add_argument("-c", "--config", default="config.yaml")
    parser.add_argument("-e", "--endpoint", default="tcp/localhost:7447")
    args = parser.parse_args()

    config = load_config(args.config)

    # Setup Zenoh
    zenoh_config = zenoh.Config()
    zenoh_config.insert_json5("connect/endpoints", json.dumps([args.endpoint]))

    # Setup shutdown
    shutdown_event = threading.Event()
    signal.signal(signal.SIGINT, lambda *_: shutdown_event.set())
    signal.signal(signal.SIGTERM, lambda *_: shutdown_event.set())

    logger.info(f"Connecting to Zenoh at: {args.endpoint}")

    with zenoh.open(zenoh_config) as session:
        logger.info("Connected to Zenoh")
        node = MyPluginNode(session, config)
        node.run(shutdown_event)


if __name__ == "__main__":
    main()
```

### Step 3: config.yaml

```yaml
# Topic to publish to
topic: "my-plugin/data"

# Publish interval in seconds
interval_secs: 60

# Add your config here:
# api_key: "your-key"
```

### Step 4: Run

```bash
pip install -r requirements.txt
python main.py -c config.yaml -e tcp/localhost:7447
```

---

## Common Patterns

### Subscribe to Topics

**Rust:**
```rust
async fn run(self, mut shutdown: watch::Receiver<()>) -> Result<(), NodeError> {
    let session = self.ctx.session();

    // Subscribe
    let subscriber = session
        .declare_subscriber("other/topic")
        .await
        .map_err(|e| NodeError::Zenoh(e.to_string()))?;

    loop {
        tokio::select! {
            _ = shutdown.changed() => break,

            // Handle incoming messages
            sample = subscriber.recv_async() => {
                if let Ok(sample) = sample {
                    let payload = sample.payload().to_bytes();
                    let data: serde_json::Value = serde_json::from_slice(&payload)?;
                    info!("Received: {:?}", data);
                }
            }
        }
    }
    Ok(())
}
```

**Python:**
```python
def run(self, shutdown_event):
    subscriber = self.session.declare_subscriber("other/topic")

    while not shutdown_event.is_set():
        sample = subscriber.try_recv()
        if sample:
            data = json.loads(sample.payload.to_bytes())
            logger.info(f"Received: {data}")

        shutdown_event.wait(timeout=0.1)
```

### Use Protobuf Messages

**Rust:**
```rust
use bubbaloop::schemas::CurrentWeather;
use prost::Message;

// Publish protobuf
let weather = CurrentWeather {
    temperature_2m: 22.5,
    ..Default::default()
};
let bytes = weather.encode_to_vec();
publisher.put(bytes).await?;

// Subscribe and decode
let payload = sample.payload().to_bytes();
let weather = CurrentWeather::decode(payload.as_ref())?;
```

**Python:**
```python
# Generate Python protobuf code first:
# protoc --python_out=. weather.proto

from weather_pb2 import CurrentWeather

# Publish
weather = CurrentWeather(temperature_2m=22.5)
publisher.put(weather.SerializeToString())

# Receive
weather = CurrentWeather()
weather.ParseFromString(sample.payload.to_bytes())
```

### Add Multiple Publishers

```rust
async fn run(self, mut shutdown: watch::Receiver<()>) -> Result<(), NodeError> {
    let session = self.ctx.session();

    let temp_pub = session.declare_publisher("sensors/temperature").await?;
    let humidity_pub = session.declare_publisher("sensors/humidity").await?;
    let status_pub = session.declare_publisher("sensors/status").await?;

    loop {
        tokio::select! {
            _ = shutdown.changed() => break,
            _ = tokio::time::sleep(interval) => {
                temp_pub.put(json!({"value": 22.5}).to_string()).await?;
                humidity_pub.put(json!({"value": 65.0}).to_string()).await?;
                status_pub.put(json!({"online": true}).to_string()).await?;
            }
        }
    }
    Ok(())
}
```

---

## CLI Reference

Both Rust and Python plugins use the same CLI flags:

| Flag | Description | Default |
|------|-------------|---------|
| `-c, --config` | Path to config YAML | `config.yaml` |
| `-e, --endpoint` | Zenoh endpoint | `tcp/localhost:7447` |

```bash
# Examples
./my-plugin                                    # Use defaults
./my-plugin -c /etc/my-plugin.yaml            # Custom config
./my-plugin -e tcp/192.168.1.100:7447         # Remote Zenoh
./my-plugin -c prod.yaml -e tcp/server:7447   # Both
```

---

## Plugin Manifest (plugin.yaml)

Optional file for MCP integration and plugin discovery:

```yaml
name: "my-plugin"
version: "0.1.0"
author: "Your Name"
description: "What this plugin does"

type: rust  # or "python"
entry: target/release/my-plugin  # or "main.py"

topics:
  publishes:
    - pattern: "my-plugin/data"
      message_type: "my.plugin.v1.Data"
  subscribes:
    - pattern: "config/my-plugin"

# Expose to AI assistants via MCP
mcp:
  tools:
    - name: "get_my_plugin_data"
      description: "Get current data"
  resources:
    - uri_pattern: "my-plugin://data"
      description: "Real-time data stream"
```

---

## Integration with Bubbaloop

### Option 1: Run Standalone

```bash
# Start Bubbaloop
pixi run up

# Run your plugin (separate terminal)
./my-plugin -e tcp/localhost:7447
```

### Option 2: Add to Launch File

```yaml
# launch/my-config.launch.yaml
nodes:
  my_plugin:
    external: ~/.bubbaloop/plugins/my-plugin
    binary: my-plugin           # For Rust
    # type: python              # For Python
    # entry: main.py
    config: ./config.yaml
```

---

## Troubleshooting

### Connection Issues

```bash
# Check Zenoh is running
nc -zv localhost 7447

# Enable debug logging
RUST_LOG=debug ./my-plugin
```

### Common Errors

| Error | Solution |
|-------|----------|
| "Connection refused" | Start Bubbaloop first: `pixi run up` |
| "Config not found" | Check config path: `-c /path/to/config.yaml` |
| "Topic not found" | Topics are created on first publish, no pre-registration needed |

---

## Examples

See `templates/rust-plugin/` and `templates/python-plugin/` for complete working examples.

---

## Security Best Practices

When developing nodes, follow these guidelines to keep your deployment secure:

### Input Validation

- **Config values**: Always validate configuration values after loading from YAML. Check bounds on numeric fields (e.g., intervals >= 1s), validate string patterns for topics and endpoints.
- **Zenoh topics**: Restrict topic names to alphanumeric characters, `/`, `-`, `_`, and `.`. Never allow arbitrary strings as topic names.
- **Endpoints**: Validate Zenoh endpoint format before connecting. Reject strings containing shell metacharacters (`&`, `|`, `;`, `$`, backticks, newlines).

### Example: Python Config Validation

```python
import re

def validate(self):
    if self.interval_secs < 1 or self.interval_secs > 86400:
        logger.warning("Invalid interval_secs, using default")
        self.interval_secs = 60
    if not re.match(r'^[a-zA-Z0-9/_.\-]+$', self.topic):
        logger.warning("Invalid topic name, using default")
        self.topic = "my-node/data"
```

### Example: Rust Config Validation

```rust
if config.rate_hz <= 0.0 || config.rate_hz > 100.0 {
    log::warn!("Invalid rate_hz, using default");
    config.rate_hz = 1.0;
}
let valid_topic = config.publish_topic.chars().all(|c| {
    c.is_alphanumeric() || c == '/' || c == '-' || c == '_' || c == '.'
});
if !valid_topic {
    log::warn!("Invalid topic, using default");
    config.publish_topic = "my-node/output".to_string();
}
```

### Network Security

- Use `tcp/localhost:7447` for single-machine deployments.
- For distributed setups, configure TLS endpoints in Zenoh.
- Never expose Zenoh ports to untrusted networks without authentication.

### systemd Hardening

Node services installed via the daemon include security directives (`NoNewPrivileges=true`, `ProtectSystem=strict`, etc.). For additional hardening, consider restricting network access with `PrivateNetwork=true` for monitoring-only nodes.
