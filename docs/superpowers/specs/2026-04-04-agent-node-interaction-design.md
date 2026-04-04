# Agent–Node Interaction Foundation

**Date**: 2026-04-04
**Status**: Implemented
**Scope**: Node SDK (Rust + Python), `node.yaml` schema

---

## Problem

Agents cannot read data from running nodes. The existing tools are:

- `query_zenoh` — Zenoh `get` on queryables only (`schema`, `manifest`, `command`). Does not work on pub/sub topics.
- `get_stream_info` — returns topic metadata but no data
- `send_command` — sends JSON to `{name}/command` (ad-hoc, no SDK standard)
- Context providers — background subscribe → world state (good for ambient data, requires upfront config)

There is no way for an agent to:
1. Read the current value of a published topic on demand
2. Discover what commands a node accepts
3. Know what topics a node subscribes to (for pipeline wiring)

### Root cause: pub/sub and queryable are separate Zenoh primitives

A `put()` reaches subscribers. A `get()` reaches queryables. A regular published topic does not respond to `get`. The SDK currently registers queryables only for `health`, `schema`, and `manifest` — not for published data topics.

## Design Goals

1. **Application-agnostic interface first** — the node's interface contract (`subscribes:`, `commands:`) is declared in `node.yaml` as pure metadata, independent of any implementation strategy.
2. **Foundation only** — build what won't be thrown away when the full pipeline/operator system is built. No operators, no `on:` handlers, no condition evaluation.
3. **Future-proof** — the node.yaml contract is what the future event-driven pipeline (operators, `on:` handlers, skill YAML) depends on. SDK implementation details are secondary.

## Motivating Use Case

```
User → agent: "What's the weather right now?"
Agent → discover_capabilities          # finds: openmeteo publishes weather/current
Agent → query_zenoh("bubbaloop/.../openmeteo/weather/current")  # existing tool
SDK   → responds with last put() value
Agent → answers user

Future (not now):
  person-detector node publishes detections
  on: {topic: vision/detections, condition: "person AND confidence>0.85"}
  → agent wakes, sends Telegram alert
```

No new agent tools required — `query_zenoh` already exists. The SDK just needs to register a queryable mirror for each publisher so `get` returns the last value.

---

## SDK: Publisher Queryable Mirror

### Why `get()` on a pub/sub topic returns nothing today

Zenoh has two separate primitives: `put()` reaches **subscribers**; `get()` reaches **queryables**. A regular `Publisher` has no queryable attached. `session.get("weather/current")` returns empty even while the node is publishing to that topic continuously.

Zenoh's own solution is the **storage plugin** — a daemon-side queryable that subscribes to a topic and caches values. But that requires configuring `zenohd`, not something a node controls.

Zenoh 1.1+ also has `AdvancedPublisher` (zenoh-ext) with a built-in cache, but it serves `AdvancedSubscriber` late-joiner recovery via an internal suffix — a generic `session.get("topic")` does NOT reach it.

The correct approach for our use case: **declare a queryable at the same key as each publisher, buffer the last `put()` payload, respond to `get()`**. This is exactly the pattern the SDK already uses for `schema` (a queryable at `{name}/schema` that responds with binary data). Same mechanism, applied to data topics.

### How it works

```
node:    put("bubbaloop/local/jetson/openmeteo/weather/current", payload)
                                    ↓ subscribers receive streaming data (unchanged)
agent:   get("bubbaloop/local/jetson/openmeteo/weather/current")
                                    ↓ SDK queryable responds with last payload
```

Zenoh allows the same key expression to have both a publisher and a queryable — they serve different callers (subscribers vs. queriers) independently.

### Per-publisher, not per-node

Each publisher gets its own queryable and buffer. A node with three publishers (`weather/current`, `weather/hourly`, `weather/daily`) registers three independent queryables. The agent targets the exact topic it needs — not a node-level grab-bag.

### SDK change surface

- **Rust** (`bubbaloop-node`): `JsonPublisher` and `ProtoPublisher` each hold an `Arc<Mutex<Option<(ZBytes, Encoding)>>>`. `put()` updates the buffer. A background `tokio::spawn` task declares the queryable and responds from the buffer on `get()`.
- **Python** (`bubbaloop-sdk`): `JsonPublisher.put()` updates a `threading.Lock`-protected buffer. The queryable runs on the same background thread as the health heartbeat.

Memory cost: one copy of the last payload per publisher. JSON nodes: negligible (<2KB). Camera frames: excluded — camera publishes protobuf H264; agent vision needs JPEG (separate concern, deferred).

### Agent usage — no new tools

`query_zenoh` already does `session.get()` with a 3s timeout and `QueryTarget::BestMatching`. The SDK queryable is registered at node startup (before any agent call). The agent flow becomes:

1. `discover_capabilities` → reads all `publishes:` from manifests → knows every available topic path
2. `query_zenoh("<full_topic_path>")` → SDK queryable responds with last value

---

## Foundation: Node Interface Contract

The application-agnostic foundation is two optional fields added to `node.yaml`. No SDK changes, no implementation — pure interface declaration.

### `subscribes:` — what a node consumes

```yaml
subscribes:
  - suffix: camera/*/compressed
    description: "Input camera frames (H264)"
    encoding: application/x-protobuf
```

A wildcard in `suffix` (e.g. `camera/*/compressed`) means the node can connect to any matching topic. The future pipeline system uses this to wire processor nodes to their inputs automatically.

### `commands:` — what a node accepts at runtime

```yaml
commands:
  - name: set_threshold
    description: "Update confidence threshold at runtime"
    params:
      threshold: float
  - name: restart
    description: "Restart internal inference loop"
    params: {}
```

The agent's `discover_capabilities` tool (which already reads manifests) can surface this. The agent knows what it can send to each node without trial-and-error. The future orchestration system uses this for automated control.

### Why just these two?

`publishes:`, `name`, `type`, `capabilities` already exist. Health and schema queryables already exist. Adding `subscribes:` and `commands:` completes the node's self-description:

| Field | Answers |
|---|---|
| `publishes:` | What does this node produce? |
| `subscribes:` | What does this node consume? **(new)** |
| `commands:` | What can you ask it to do? **(new)** |
| `capabilities:` | What role does it play? (sensor/processor/actuator) |

These are **application-agnostic** — a camera node, a weather node, a person-detector, a robot arm controller all describe themselves the same way. The consuming system (agent, pipeline YAML, dashboard) interprets the contract.

No SDK code change required. The manifest queryable already serves whatever is in `node.yaml`.

---

## Full Example — Person Detector Node

```yaml
name: person-detector
version: "0.1.0"
type: python
description: "Real-time person detection from camera frames"

capabilities:
  - processor

subscribes:
  - suffix: camera/*/compressed
    description: "Input H264 camera frames"
    encoding: application/x-protobuf

publishes:
  - suffix: vision/detections
    description: "Person detections with bounding boxes and confidence"
    encoding: application/json
    rate_hz: varies

commands:
  - name: set_threshold
    description: "Update minimum confidence threshold"
    params:
      threshold: float          # 0.0–1.0, default 0.85
  - name: set_labels
    description: "Filter to specific object labels only"
    params:
      labels: list[str]         # e.g. ["person", "car"]
```

This is everything a pipeline system needs to:
1. Find the node (`name`, `type`)
2. Connect it to a camera (`subscribes:`)
3. Read its output (`publishes:`)
4. Control it at runtime (`commands:`)

---

## What This Enables

Once nodes declare `subscribes:` and `commands:`, the agent (via `discover_capabilities`) and any future pipeline system can:

- **Discover wiring**: find which processor nodes can consume which camera topics, without hardcoding
- **Send commands**: know what parameters each command accepts before calling `send_command`
- **Validate pipelines**: check at skill-load time whether all required node connections are satisfiable

---

## What This Does NOT Include

Intentionally deferred — these build on top of this contract but are separate concerns:

- **SDK `command` queryable helper** — standardized node-side command registration. Implementation detail.
- **`grab_frame` for camera** — camera-specific JPEG endpoint for agent vision. Node-specific.
- **`on:` event handlers** — requires EventBridge, OnHandlerQueue, condition evaluation.
- **Skill YAML `pipeline:` section** — node wiring automation.
- **Built-in operators** (`log-to-sqlite`, `save-image`, `send-telegram`).

---

## Implementation Scope (when ready)

| Piece | Effort | Files |
|---|---|---|
| SDK publisher queryable mirror (Rust) | Small | `crates/bubbaloop-node/src/publisher.rs` |
| SDK publisher queryable mirror (Python) | Small | `python-sdk/bubbaloop_sdk/publisher.py` |
| Add `subscribes:` + `commands:` to existing nodes | Trivial | `node.yaml` in each node in `bubbaloop-nodes-official` |
| Update `node.yaml` validation | Trivial | daemon registry parser |
| Surface `commands:` in `discover_capabilities` | Small | `crates/bubbaloop/src/agent/dispatch.rs` |

No new agent tools. No daemon changes. No breaking changes to existing nodes or topics.
