# Bubbaloop MCP Server

Exposes Bubbaloop to AI assistants (Claude, etc.) via [Model Context Protocol (MCP)](https://modelcontextprotocol.io/).

## Architecture

```
┌─────────────────────────────────────────────────────┐
│           Claude / AI Assistant                      │
└──────────────────────┬──────────────────────────────┘
                       │ MCP (JSON-RPC over stdio)
┌──────────────────────▼──────────────────────────────┐
│           bubbaloop-mcp-server                       │
│  - Tool handlers → Zenoh publishers                  │
│  - Resource handlers → Zenoh subscribers             │
└──────────────────────┬──────────────────────────────┘
                       │ Zenoh pub/sub
┌──────────────────────▼──────────────────────────────┐
│           Zenoh Router (tcp/127.0.0.1:7447)         │
└──────────────────────┬──────────────────────────────┘
         ┌─────────────┼─────────────┐
         ▼             ▼             ▼
    [Camera]     [Weather]     [Recorder]
```

## Available Tools

| Tool | Description |
|------|-------------|
| `get_weather` | Get current weather data from Bubbaloop |
| `get_forecast` | Get hourly weather forecast |
| `update_location` | Update weather location (lat, lon) |
| `list_topics` | Debug: list available Zenoh topics |

## Usage

### Build

```bash
pixi run cargo build -p bubbaloop-mcp-server --release
```

### Run Standalone

```bash
# Start Bubbaloop services first
pixi run up

# Run MCP server (in another terminal)
./target/release/bubbaloop-mcp-server --zenoh-endpoint tcp/127.0.0.1:7447
```

### Claude Desktop Integration

Add to `~/.config/claude/claude_desktop_config.json`:

```json
{
  "mcpServers": {
    "bubbaloop": {
      "command": "/path/to/bubbaloop/target/release/bubbaloop-mcp-server",
      "args": ["--zenoh-endpoint", "tcp/127.0.0.1:7447"]
    }
  }
}
```

Then restart Claude Desktop and ask:
- "What's the current weather in Bubbaloop?"
- "Get the weather forecast"
- "Update the weather location to San Francisco"

## Development

The server implements MCP JSON-RPC protocol manually (no SDK dependency) for compatibility with Rust 1.85.

### Testing

Send JSON-RPC requests via stdin:

```bash
echo '{"jsonrpc":"2.0","id":1,"method":"initialize","params":{}}' | ./target/release/bubbaloop-mcp-server
```
