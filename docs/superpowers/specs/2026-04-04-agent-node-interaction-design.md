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

1. **Application-agnostic interface first** — the node's interface contract (`subscribes:`, `commands:`) is declared in `node.yaml` as pure metadata, independent of any implementation strategy.
2. **Foundation only** — build what won't be thrown away when the full pipeline/operator system is built. No operators, no `on:` handlers, no condition evaluation.
3. **Future-proof** — the node.yaml contract is what the future event-driven pipeline (operators, `on:` handlers, skill YAML) depends on. SDK implementation details are secondary.

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

- **SDK `latest` queryable** — last-value cache per publisher. Implementation detail, not the interface.
- **SDK `command` queryable helper** — standardized node-side command handling. Implementation detail.
- **`grab_frame` queryable** — camera-specific JPEG endpoint. Node-specific, not universal.
- **Agent `read_node` / `grab_frame` tools** — bubbaloop-specific agent tools. Application-specific.
- **`on:` event handlers** — requires EventBridge, OnHandlerQueue, condition evaluation.
- **Skill YAML `pipeline:` section** — node wiring automation.
- **Built-in operators** (`log-to-sqlite`, `save-image`, `send-telegram`).

---

## Implementation Scope (when ready)

| Piece | Effort | Files |
|---|---|---|
| Add `subscribes:` + `commands:` to existing nodes | Trivial | `node.yaml` in each node in `bubbaloop-nodes-official` |
| Update `node.yaml` JSON schema / validation | Trivial | daemon registry parser |
| Surface `commands:` in agent `discover_capabilities` | Small | `crates/bubbaloop/src/agent/dispatch.rs` |

No SDK changes. No daemon changes. No breaking changes to existing nodes.
