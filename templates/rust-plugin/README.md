# {{plugin_name}}

{{description}}

This plugin is built using the Bubbaloop SDK and implements the `BubbleNode` trait for standardized lifecycle management.

## Building

```bash
cargo build --release
```

## Running

The plugin uses standard Bubbaloop CLI arguments:

```bash
# With default config (config.yaml) and endpoint (tcp/localhost:7447)
./target/release/{{plugin_name}}

# With custom config
./target/release/{{plugin_name}} -c my-config.yaml

# Connect to specific Zenoh endpoint
./target/release/{{plugin_name}} -e tcp/192.168.1.100:7447
```

## Configuration

Edit `config.yaml`:

```yaml
# Topic to publish data to
topic: "{{plugin_name}}/data"

# How often to publish data (seconds)
interval_secs: 60
```

## Architecture

This plugin implements the `BubbleNode` trait:

```rust
#[async_trait]
impl BubbleNode for {{plugin_name_pascal}}Node {
    type Config = Config;

    fn metadata() -> BubbleMetadata { ... }
    fn new(ctx: Arc<ZContext>, config: Config) -> Result<Self, NodeError> { ... }
    async fn run(self, shutdown: watch::Receiver<()>) -> Result<(), NodeError> { ... }
}
```

The `run_node::<{{plugin_name_pascal}}Node>()` function handles:
- CLI argument parsing (`-c/--config`, `-e/--endpoint`)
- Logging setup (respects `RUST_LOG` env var)
- YAML config loading
- Zenoh connection via ROS-Z
- Graceful shutdown on Ctrl+C

## Integration with Bubbaloop

This plugin publishes to `{{plugin_name}}/data` topic. To integrate:

1. Copy to plugins directory:
   ```bash
   cp -r . ~/.bubbaloop/plugins/{{plugin_name}}/
   ```

2. Build:
   ```bash
   cd ~/.bubbaloop/plugins/{{plugin_name}}
   cargo build --release
   ```

3. Add to launch file:
   ```yaml
   nodes:
     {{plugin_name}}:
       external: ~/.bubbaloop/plugins/{{plugin_name}}
       binary: {{plugin_name}}
       config: ./config.yaml
   ```

## MCP Integration

This plugin exposes (via `plugin.yaml`):
- **Tool**: `get_{{plugin_name}}_data` - Get current data
- **Resource**: `{{plugin_name}}://data` - Real-time data stream

AI assistants (Claude, etc.) can query this plugin via the Bubbaloop MCP server.

## Development

### Customizing the Plugin

1. **Add configuration options**: Extend the `Config` struct in `main.rs`
2. **Change the data format**: Modify the `PluginData` struct
3. **Add subscriptions**: Use `self.ctx.session().declare_subscriber()` in `run()`
4. **Use protobuf**: Import schemas from `bubbaloop::schemas::*`

### Using Protobuf Messages

For typed messages instead of JSON:

```rust
use bubbaloop::prelude::*;
use prost::Message;

// Create a protobuf message
let weather = CurrentWeather {
    temperature_2m: 22.5,
    // ...
};

// Publish via ROS-Z typed publisher
let publisher = self.ctx.advertise::<CurrentWeather>("/weather/current")?;
publisher.publish(&weather)?;
```
