# Authoring a bubbaloop node

A bubbaloop node is a **standalone process** that speaks the node contract over Zenoh. The daemon manages its lifecycle (systemd) and exposes it to MCP clients via the `node_*` tools — but the node itself is independent and can be written in any language with a Zenoh client.

## The node contract — three queryables, n data topics

```
bubbaloop/{scope}/{machine}/{node}/
├── manifest    ← queryable, CBOR-encoded — capabilities, commands, topics
├── command     ← queryable, JSON in/out — RPC handler
├── health      ← publisher, periodic empty heartbeat
└── <data>      ← any number of topics; you choose schema/encoding
```

If you publish those, the daemon discovers you, MCP clients can drive you via `node_command_send`, and `dataflow_graph` shows you in the live DAG. **That is the entire contract.**

## SDK quick path (recommended)

The bubbaloop-sdk (Rust) and `bubbaloop_node` (Python) packages handle Zenoh session, manifest publish, heartbeat, command queryable boilerplate. You write only business logic.

### Rust

```bash
bubbaloop node init my-sensor --type rust
cd my-sensor
# Edit src/main.rs
cargo build --release
bubbaloop node add .
bubbaloop node install my-sensor   # creates systemd unit
bubbaloop node start my-sensor     # or node_start via MCP plugin
```

Typical `src/main.rs` (~50 lines for a sensor):

```rust
use bubbaloop_node::{run_node, Node, NodeContext};
use std::time::Duration;

#[derive(Default)]
struct MySensor;

#[async_trait::async_trait]
impl Node for MySensor {
    fn name(&self) -> &str { "my-sensor" }

    async fn init(&mut self, _ctx: &NodeContext) -> anyhow::Result<()> {
        // open hardware, allocate buffers
        Ok(())
    }

    async fn run(&mut self, ctx: &NodeContext) -> anyhow::Result<()> {
        let publisher = ctx.session.declare_publisher("my-sensor/value").await?;
        let mut interval = tokio::time::interval(Duration::from_secs(1));
        loop {
            tokio::select! {
                _ = ctx.shutdown_rx.changed() => return Ok(()),
                _ = interval.tick() => {
                    let payload = serde_cbor::to_vec(&Reading { v: read_sensor() })?;
                    publisher.put(payload).await?;
                }
            }
        }
    }
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    run_node(MySensor::default()).await
}
```

### Python

```bash
bubbaloop node init my-sensor --type python
cd my-sensor
pixi install
# Edit recorder/node.py — see bubbaloop-nodes-internal/recorder-py for a clean reference
bubbaloop node add .
bubbaloop node install my-sensor
bubbaloop node start my-sensor
```

The Python SDK is opinionated: implement a `Node` class with `__init__(ctx, config)` and `run()`, the SDK calls them with the right context. See `bubbaloop-nodes-internal/recorder-py/recorder/node.py:60` for a reference.

## Command-driven nodes

If your node accepts commands (actuator / configurable processor), declare them in `node.yaml` and dispatch in your command queryable handler.

### Wire format — important

Commands arrive as JSON with this exact envelope:

```json
{"command": "verb_noun", "params": {"key1": "value", "key2": 42}}
```

Your handler MUST parse the `{command, params}` shape — not flat. Reference handler:

```rust
#[derive(serde::Deserialize)]
struct Envelope {
    command: String,
    #[serde(default)]
    params: serde_json::Value,
}

async fn handle_command(payload: &[u8]) -> Vec<u8> {
    let env: Envelope = match serde_json::from_slice(payload) {
        Ok(e) => e,
        Err(e) => return error_response(format!("invalid command envelope: {e}")),
    };
    match env.command.as_str() {
        "start_recording" => start_recording(env.params).await,
        "stop_recording"  => stop_recording().await,
        "get_status"      => get_status().await,
        other => error_response(format!("unknown command: {other}")),
    }
}
```

Python equivalent:

```python
def handle_command(payload: bytes) -> bytes:
    env = json.loads(payload)
    cmd = env["command"]
    params = env.get("params", {})
    handlers = {
        "start_recording": lambda p: start_recording(**p),
        "stop_recording":  lambda _: stop_recording(),
        "get_status":      lambda _: get_status(),
    }
    h = handlers.get(cmd)
    if not h:
        return json.dumps({"status": "error", "code": "E_UNKNOWN_CMD", "message": f"unknown: {cmd}"}).encode()
    try:
        return json.dumps({"status": "ok", **h(params)}).encode()
    except Exception as e:
        return json.dumps({"status": "error", "code": "E_HANDLER", "message": str(e)}).encode()
```

### Common pitfall

The Rust `recorder` in `bubbaloop-nodes-internal/recorder` uses `#[serde(tag = "command")]` on a flat enum, which expects `{command, topic_patterns, output_dir}` (no params nesting). It works only when called directly via Zenoh, not via `node_command_send` from MCP. The Python `recorder-py` sidesteps the issue by being config-driven (no command queryable). **For a brand-new node, match `{command, params}` and you'll be compatible with both paths.** This is alignment-notes gap #11 territory.

## Naming convention

Match GitHub MCP / OBS MCP: `verb_noun` after the resource. For a recorder:

| Command | Purpose |
|---|---|
| `start_recording` | Begin a new session |
| `stop_recording` | End the active session |
| `pause_recording` | Pause if recording |
| `resume_recording` | Resume from pause |
| `get_recording_status` | Returns `{recording: bool, ...}` |
| `set_recording_format` | Reconfigure (codec, bitrate) |
| `set_recording_path` | Change output path |

Smaller open models (Llama 3.1 8B, Qwen 2.5 7B) pick the right tool 13–15% more often with this convention than verb-only names — see the Klavis benchmark cited in alignment-notes.

## Heartbeat

Publish to `bubbaloop/{scope}/{machine}/{node}/health` every 5 seconds with an empty payload. The SDK does this automatically; if you implement raw, set up a `tokio::time::interval` and `publisher.put(&[])`.

## Multi-instance nodes

If your binary supports multiple configurations (one rtsp-camera binary feeding multiple TAPOs):

```bash
bubbaloop node instance rtsp-camera terrace --config configs/terrace.yaml
bubbaloop node instance rtsp-camera entrance --config configs/entrance.yaml
```

Both share the binary; each gets its own systemd unit, manifest topic, and config. The instance name (`tapo-terrace`) becomes the `instance_name` in the manifest and the prefix on every topic.

## Testing

### Unit / integration in your repo

Standard cargo test / pytest patterns. The SDK exports a `MockNodeContext` for offline testing of the Node trait without a real Zenoh session.

### Live test against the daemon

```bash
# Daemon up (in another terminal)
bubbaloop daemon run

# Start your node
bubbaloop node start my-sensor

# Inspect via plugin (if loaded)
bubbaloop node list
bubbaloop node logs my-sensor

# Drive a command via Zenoh CLI — fastest, no MCP overhead
echo '{"command":"start_recording","params":{"output_path":"/tmp/x.mp4"}}' \
  | zenoh-query -k "bubbaloop/global/$(hostname)/my-sensor/command" --payload-stdin
```

Or via the MCP plugin from a Claude Code session that has it loaded:

```
@bubbaloop-operator send "get_status" command to my-sensor
```

## Two-plane discipline

- **MCP** = control plane (low-rate, ≤1 req/sec sustained, request/response)
- **Zenoh** = data plane (high-rate, 1000s msg/sec, pub/sub)

Your node publishes data on Zenoh topics, accepts commands via Zenoh queryable. **MCP never sees your data.** This is bubbaloop's most important architectural rule — violating it manifests as MCP rate-limit errors and broken downstream agents.

If you find yourself wanting to surface bulk data via MCP (e.g., return frames from a `get_frame` command), step back: the right pattern is a Zenoh topic + `node_stream_info` returning the topic name to the caller.

## See also

- [`agents/bubbaloop-node-author.md`](../agents/bubbaloop-node-author.md) — interactive subagent that walks through this guide
- [`commands/node-new.md`](../commands/node-new.md) — `/bubbaloop:node-new <name>` shortcut
- [`docs/alignment-notes.md`](alignment-notes.md) — open gaps + design rationale
- `bubbaloop-nodes-internal/recorder-py/` — clean reference Python implementation
- `bubbaloop-nodes-official/rtsp-camera/` — clean reference Rust implementation (multi-instance)
- `bubbaloop` repo `docs/plugin-development.md` — older but more detailed for advanced cases
