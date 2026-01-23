# {{plugin_name}}

{{description}}

This plugin follows the BubbleNode pattern from the Bubbaloop Rust SDK, providing standardized lifecycle management.

## Installation

```bash
# Create virtual environment (recommended)
python -m venv venv
source venv/bin/activate

# Install dependencies
pip install -r requirements.txt
```

## Running

The plugin uses standard Bubbaloop CLI arguments (matching the Rust SDK):

```bash
# With default config (config.yaml) and endpoint (tcp/localhost:7447)
python main.py

# With custom config
python main.py -c my-config.yaml

# Connect to specific Zenoh endpoint
python main.py -e tcp/192.168.1.100:7447
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

This plugin implements the `BubbleNode` pattern (Python equivalent of Rust trait):

```python
class {{plugin_name_pascal}}Node(BubbleNode):
    @staticmethod
    def metadata() -> BubbleMetadata: ...

    def __init__(self, session: zenoh.Session, config: Dict): ...

    def run(self, shutdown_event) -> None: ...
```

The `run_node({{plugin_name_pascal}}Node)` function handles:
- CLI argument parsing (`-c/--config`, `-e/--endpoint`)
- Logging setup
- YAML config loading
- Zenoh connection
- Graceful shutdown on Ctrl+C / SIGTERM

## Integration with Bubbaloop

This plugin publishes to `{{plugin_name}}/data` topic. To integrate:

1. Copy to plugins directory:
   ```bash
   cp -r . ~/.bubbaloop/plugins/{{plugin_name}}/
   ```

2. Install dependencies:
   ```bash
   cd ~/.bubbaloop/plugins/{{plugin_name}}
   pip install -r requirements.txt
   ```

3. Add to launch file:
   ```yaml
   nodes:
     {{plugin_name}}:
       external: ~/.bubbaloop/plugins/{{plugin_name}}
       type: python
       entry: main.py
       config: ./config.yaml
   ```

## MCP Integration

This plugin exposes (via `plugin.yaml`):
- **Tool**: `get_{{plugin_name}}_data` - Get current data
- **Resource**: `{{plugin_name}}://data` - Real-time data stream

AI assistants (Claude, etc.) can query this plugin via the Bubbaloop MCP server.

## Development

### Customizing the Plugin

1. **Add configuration options**: Extend the `Config` dataclass
2. **Change the data format**: Modify the data dict in `run()`
3. **Add subscriptions**: Use `self.session.declare_subscriber()` in `run()`

### Using Protobuf Messages

For typed messages, define your `.proto` file and generate Python code:

```bash
protoc --python_out=. your_messages.proto
```

Then import and use:

```python
from your_messages_pb2 import YourMessage

msg = YourMessage(value=42.0, timestamp=int(time.time()))
publisher.put(msg.SerializeToString())
```

### Comparison with Rust SDK

| Rust SDK | Python Equivalent |
|----------|-------------------|
| `BubbleNode` trait | `BubbleNode` ABC |
| `impl BubbleNode for T` | `class T(BubbleNode)` |
| `fn metadata() -> BubbleMetadata` | `@staticmethod def metadata()` |
| `fn new(ctx, config) -> Result<Self>` | `def __init__(self, session, config)` |
| `async fn run(self, shutdown)` | `def run(self, shutdown_event)` |
| `run_node::<T>().await` | `run_node(T)` |
