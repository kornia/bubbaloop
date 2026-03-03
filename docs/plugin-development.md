# Bubbaloop Node Development Guide

Create custom nodes for Bubbaloop in Rust or Python. Nodes connect to the Zenoh pub/sub network to publish sensor data, process messages, or integrate with external services.

## Quick Start (Recommended: Node SDK)

The Node SDK handles all boilerplate. You write ~50 lines of business logic:

```bash
# Scaffold a new node (works from any directory - nodes are standalone repos)
bubbaloop node init my-sensor --type rust -d "My custom sensor"
cd my-sensor

# Edit src/main.rs — implement Node trait (init + run)
# Build and register with bubbaloop
pixi run build
bubbaloop node add .
bubbaloop node start my-sensor
```

See the [Skillet Development Guide](skillet-development.md#node-sdk-recommended) for the full SDK reference.

### Manual Setup (Advanced)

For full control over the Zenoh lifecycle, see the [Skillet Development Guide](skillet-development.md) which documents the raw Zenoh + argh + ctrlc pattern used by production nodes.

---

## Rust Node (Manual Setup)

### Directory Structure

```
my-node/
├── Cargo.toml
├── node.yaml        # Node manifest
├── config.yaml      # Runtime configuration
├── build.rs         # Proto compilation
├── protos/          # Protobuf definitions
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
bubbaloop-schemas = { git = "https://github.com/kornia/bubbaloop.git", branch = "main" }
zenoh = "1.7"
tokio = { version = "1", features = ["full"] }
serde = { version = "1.0", features = ["derive"] }
serde_yaml = "0.9"
prost = "0.14"
argh = "0.1"
log = "0.4"
env_logger = "0.11"
ctrlc = "3.4"
anyhow = "1"
hostname = "0.4"

[build-dependencies]
prost-build = "0.14"

[workspace]
```

### Step 2: src/main.rs

For the manual implementation pattern, see the [Skillet Development Guide](skillet-development.md#manual-implementation-raw-zenoh) which shows the complete pattern:
- Zenoh client session creation
- argh CLI parsing
- ctrlc signal handling
- Publish/subscribe loops with proper shutdown

The skillet guide includes working examples with health heartbeats, schema queryables, and protobuf encoding.

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

For common patterns including subscribing to topics, using protobuf messages, health heartbeats, and multiple publishers, see the [Skillet Development Guide](skillet-development.md#common-patterns) which documents:

- **Subscribe to topics**: Raw Zenoh subscriber patterns with proper shutdown handling
- **Protobuf encoding/decoding**: Using `bubbaloop-schemas` with prost
- **Health heartbeats**: Publishing periodic health status
- **Schema queryables**: Serving FileDescriptorSet for dynamic protobuf decoding
- **Multiple publishers**: Managing multiple Zenoh publishers in one node

All examples use the actual Zenoh API (not phantom types) and follow the argh + ctrlc pattern used by production nodes

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

Nodes are developed as **standalone repositories** outside the bubbaloop tree. After building, register them with bubbaloop:

### Option 1: Run Standalone

```bash
# Start Bubbaloop
pixi run up

# Run your node from its own directory (separate terminal)
cd /path/to/my-node
./target/release/my-node -e tcp/localhost:7447
```

### Option 2: Register with Bubbaloop

```bash
# From your node's directory
bubbaloop node add .
bubbaloop node start my-node
```

### Option 3: Add to Launch File

```yaml
# launch/my-config.launch.yaml
nodes:
  my_plugin:
    external: /path/to/my-node
    binary: my-node           # For Rust
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
