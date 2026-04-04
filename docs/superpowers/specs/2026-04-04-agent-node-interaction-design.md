# Agent–Node Interaction Foundation

**Date**: 2026-04-04
**Status**: Proposal — not implemented
**Scope**: Node SDK (Rust + Python), `node.yaml` schema, `rtsp-camera` node, agent dispatch tools

---

## Problem

Agents cannot interact with running nodes in any structured way today. The existing tools are:

- `query_zenoh` — raw Zenoh `get` on queryables (admin-only, works only for nodes that expose a queryable like `schema` or `manifest`)
- `get_stream_info` — returns topic metadata but no data
- `send_command` — sends JSON to a node's `command` queryable (ad-hoc, no SDK standard)
- Context providers — daemon background tasks that subscribe continuously and write to world state (good for ambient data, requires upfront configuration)

There is no way for an agent to:
1. Pull the latest sample from a node on demand
2. Grab a frame from a camera for vision reasoning
3. Discover what commands a node accepts
4. Know what topics a node subscribes to (for pipeline wiring)

## Design Goals

1. **Foundation only** — build what won't be thrown away when the full pipeline/operator system is built. No operators, no `on:` handlers, no condition evaluation.
2. **SDK-level contract** — every node gets the interface automatically, like health heartbeat and schema queryable today.
3. **Agent-useful today** — agent can pull data from any running node with a single tool call.
4. **Future-proof** — the pieces below are exactly what the future event-driven pipeline (operators, `on:` handlers, skill YAML) will depend on.

## Motivating Use Case

```
User → agent: "What's the weather right now?"
Agent → read_node("openmeteo") → latest weather JSON → answers user

User → agent: "What does the entrance camera see?"
Agent → grab_frame("tapo-entrance") → JPEG base64 → passes to vision model → answers user

Future (not now):
  person-detector node publishes detections
  on: {topic: vision/detections, condition: "person AND confidence>0.85"}
  → agent wakes, sends Telegram alert, logs to DB
```

The two agent tools (`read_node`, `grab_frame`) are all that's needed now. The future system builds on the same SDK queryables.

---

## Foundation Pieces

### 1. `node.yaml` schema extension

Add two optional fields. Additive, no breaking changes. The manifest queryable already serves `node.yaml`; the agent already reads manifests.

```yaml
# Existing fields unchanged.

subscribes:                          # NEW — optional
  - suffix: camera/*/compressed
    description: "Input camera frames (H264)"
    encoding: application/x-protobuf

commands:                            # NEW — optional
  - name: set_threshold
    description: "Update confidence threshold at runtime"
    params:
      threshold: float
```

`subscribes:` gives the future pipeline system the metadata to wire processor nodes to their inputs without human configuration.

`commands:` gives the agent (and future orchestration) a machine-readable list of what a node accepts, with parameter types. The agent's existing `discover_capabilities` tool can surface this.

No SDK code change required for this piece — it's schema documentation. The manifest queryable already serves whatever is in `node.yaml`.

---

### 2. SDK `latest` queryable

**Both Rust (`bubbaloop-node`) and Python (`bubbaloop-sdk`) SDKs.**

On every `put()` call, the SDK buffers the raw payload bytes (last value only). At startup, the SDK registers a `{instance_name}/latest` queryable that responds with the buffered payload and its encoding.

This is the Zenoh storage pattern implemented locally inside the node — no storage plugin required.

```
bubbaloop/{scope}/{machine}/{name}/latest   ← new queryable (auto-registered by SDK)
```

Response format: same bytes and encoding as the original `put()`. If the node publishes JSON, `latest` returns JSON. If the node publishes protobuf, `latest` returns protobuf bytes.

**Why this is load-bearing for the future:**
The `on:` handler system will subscribe to `{name}/latest` changes to detect when new data arrives. The `read_node` agent tool queries it. Both depend on this queryable existing.

**SDK change surface:**
- Rust: `JsonPublisher::put()` and `ProtoPublisher::put()` update an `Arc<Mutex<Option<(ZBytes, Encoding)>>>`. A `tokio::spawn` task runs the queryable loop.
- Python: `JsonPublisher.put()` updates a `threading.Lock`-protected buffer. The queryable is declared in the same background thread that handles health heartbeats.

Memory cost: one copy of the last payload per publisher. For JSON nodes this is negligible (weather/telemetry payloads are <2KB). For camera nodes, frames are NOT buffered via this path (see `grab_frame` below).

---

### 3. SDK `command` queryable helper

**Both Rust and Python SDKs.**

Today, `send_command` (agent tool) sends JSON to `{name}/command`, but nodes implement this ad-hoc. The SDK should provide a standard registration helper so the interface is consistent.

```rust
// Rust SDK — node author writes:
ctx.register_command_handler(|cmd: &str, params: Value| async move {
    match cmd {
        "set_threshold" => { /* ... */ Ok("threshold updated") }
        _ => Err(anyhow!("unknown command: {}", cmd))
    }
}).await?;
```

```python
# Python SDK — node author writes:
def on_command(cmd: str, params: dict) -> str:
    if cmd == "set_threshold":
        ...
        return "threshold updated"
    raise ValueError(f"unknown command: {cmd}")

ctx.register_command_handler(on_command)
```

The SDK registers the `{name}/command` queryable, handles JSON deserializaton of `{command, params}`, calls the handler, and serializes the response. The node author only writes the match logic.

This formalizes what already half-exists. The `send_command` agent tool already sends to `{name}/command` — this makes the node side consistent.

---

### 4. `grab_frame` queryable in `rtsp-camera`

The camera node (`rtsp-camera`, Rust) adds one new queryable: `{name}/frame`.

On query, it takes the latest decoded frame from its internal frame buffer (already maintained for H264 encoding), re-encodes it as JPEG at a configurable quality (default 80%), and replies with:

```json
{
  "frame_b64": "<base64-encoded JPEG>",
  "timestamp_ms": 1743200000000,
  "width": 1280,
  "height": 720,
  "node_name": "tapo_entrance"
}
```

**Why not buffer via `latest` queryable?**
Camera frames are large (~50–200KB JPEG) and the agent needs a JPEG, not a raw H264 frame. This is camera-specific logic that the generic `latest` buffer can't provide. Camera frames should NOT flow through `latest`.

**Why JPEG not raw?**
The agent passes this to a vision model (Claude, GPT-4V). These APIs accept JPEG/PNG, not H264 or raw YUV.

**Config addition to `configs/*.yaml`:**
```yaml
frame_queryable:
  enabled: true
  jpeg_quality: 80          # 1–100
```

Disabled by default to avoid unexpected behavior on existing deployments.

---

### 5. Agent tools: `read_node` and `grab_frame`

Two new tools in `agent/dispatch.rs`. Both are Operator-tier (not Admin) since they only read data.

**`read_node(node_name)`**

Queries `bubbaloop/{scope}/{machine}/{node_name}/latest`. Decodes response:
- If encoding is `application/json` → returns the JSON text directly
- If encoding is `application/x-protobuf` → returns `"<N bytes protobuf — use get_node_schema to decode>"`
- If no response → returns `"Node is running but has not published any data yet"`

```json
{ "node_name": "openmeteo" }
```

**`grab_frame(node_name)`**

Queries `bubbaloop/{scope}/{machine}/{node_name}/frame`. Returns the JSON payload including `frame_b64`. The agent can extract `frame_b64` and pass it to the vision model.

```json
{ "node_name": "tapo_entrance" }
```

Both tools use the existing `platform.send_zenoh_query()` path with a 3-second timeout.

---

## What This Enables Today

| User intent | Agent tool | Node queryable |
|---|---|---|
| "What's the current weather?" | `read_node("openmeteo")` | `openmeteo/latest` |
| "What's CPU usage?" | `read_node("system-telemetry")` | `system-telemetry/latest` |
| "Are all network checks passing?" | `read_node("network-monitor")` | `network-monitor/latest` |
| "What does the entrance camera see?" | `grab_frame("tapo-entrance")` | `tapo-entrance/frame` |
| "Adjust person-detector threshold" | `send_command("person-detector", "set_threshold", {threshold: 0.9})` | `person-detector/command` |

---

## What This Does NOT Include

The following are intentionally deferred:

- **`on:` event handlers** — requires EventBridge, OnHandlerQueue, condition evaluation. These build on `latest` queryable but are not part of this spec.
- **Skill YAML `pipeline:` section** — node wiring. Builds on `subscribes:` in `node.yaml` but not implemented here.
- **Built-in operators** (`log-to-sqlite`, `save-image`, `send-telegram`) — deferred.
- **Detection node** (`person-detector`) — a future processor node. The SDK contract defined here is what it will use.
- **`read_node` for protobuf nodes** — protobuf response is surfaced as-is today. Full decode requires schema registry integration (future).

---

## Implementation Scope (when ready to implement)

| Piece | Effort | Files |
|---|---|---|
| `node.yaml` schema extension | Trivial | `node.yaml` in each node |
| SDK `latest` queryable (Rust) | Small | `crates/bubbaloop-node/src/publisher.rs`, new `latest.rs` |
| SDK `latest` queryable (Python) | Small | `python-sdk/bubbaloop_sdk/publisher.py` |
| SDK `command` helper (Rust) | Small | `crates/bubbaloop-node/src/context.rs` |
| SDK `command` helper (Python) | Small | `python-sdk/bubbaloop_sdk/context.py` |
| `grab_frame` queryable (rtsp-camera) | Medium | `rtsp-camera/src/node.rs` |
| Agent tools `read_node` + `grab_frame` | Small | `crates/bubbaloop/src/agent/dispatch.rs` |

No changes to daemon, dashboard, MCP server, or existing node behavior.
