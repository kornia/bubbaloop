---
description: "Canonical CBOR envelope used by Bubbaloop nodes on the data plane. Outer transport header, inner schema body, and the rules consumers (recorders, replayers, dashboards) follow."
---

# Wire Format

Every payload published on the data plane is a **CBOR-encoded envelope**. The envelope splits transport metadata (delivery, ordering, source) from schema metadata (sensor timestamps, frame IDs, codec). This separation is what lets the same MCAP recorder, dashboard subscriber, or replayer node consume frames from any sensor without baking in per-sensor decoders.

For the topic-key layout (`bubbaloop/{global,local}/{machine}/{instance}/{suffix}`) and Zenoh routing, see [Messaging → Topic Convention](messaging.md#topic-convention). This document focuses on what's *inside* a payload at any of those keys.

---

## Why an Envelope at All?

Raw CBOR-of-payload would force every consumer to know every sensor's schema. Instead the envelope answers two questions before you decode:

1. **Transport** — where did this come from, in what order, when? (`header`)
2. **Schema** — what does the body look like? (`header.schema_uri`, plus body-level header)

The recorder needs the transport header to write MCAP indices. The dashboard needs the schema URI to pick a decoder. Neither needs the inner sensor payload to operate.

---

## The Envelope

```
{
  "header": {
    "schema_uri":     "bubbaloop://schemas/sensor/CompressedImage/v1",
    "source_instance": "tapo_terrace_camera",
    "ts_ns":          1714752000000000000,
    "monotonic_seq":  4711
  },
  "body": {
    ...schema-specific fields, see below...
  }
}
```

### `header` — transport metadata

| Field | Type | Required | Description |
|-------|------|----------|-------------|
| `schema_uri` | string | yes | Stable URI naming the body's schema and version. Used by consumers to dispatch to a decoder. |
| `source_instance` | string | yes | The publishing instance's *config name* (the YAML's top-level `name`). Often differs from the registered node name — see [Topic Convention](messaging.md#topic-convention). |
| `ts_ns` | uint64 | yes | Wall-clock publish time, nanoseconds since UNIX epoch. Recorders use this for the MCAP `publish_time` index. |
| `monotonic_seq` | uint64 | yes | Per-instance monotonic counter, starts at 0 on process start. Drop detection: if seq jumps, frames were dropped on the wire. |

### `body` — schema-specific

The shape of `body` is determined entirely by `schema_uri`. The recorder writes `body` verbatim into the MCAP message's data field; it does not parse it.

For sensor frames the convention (not enforced) is to nest a sensor-domain header inside `body.header` so the inner schema is self-describing once unwrapped:

```
"body": {
  "header": {
    "acq_time":   1714751999987000000,
    "pub_time":   1714752000000000000,
    "sequence":   4711,
    "frame_id":   "tapo_terrace_camera",
    "machine_id": "nvidia_orin00"
  },
  "format": "h264",
  "data":   <bytes — H264 NAL units>
}
```

| Inner header field | Description |
|--------------------|-------------|
| `acq_time` | Sensor capture time (camera shutter, sensor read) — what you want for fusion / synchronization |
| `pub_time` | Time the publisher serialized this frame — `header.ts_ns` mirrors this |
| `sequence` | Sensor's own sequence (RTSP packet number, etc.) — independent from `header.monotonic_seq` |
| `frame_id` | Coordinate frame name (TF-style) |
| `machine_id` | Where the data was acquired |

The body header is for the **schema's domain**. The outer header is for **the bus**. They overlap (both have a timestamp and a sequence) on purpose: the recorder uses the outer pair without parsing the schema, and the application logic uses the inner pair without caring about how it got delivered.

---

## Schema URI Format

```
bubbaloop://schemas/{family}/{name}/{version}
```

| Component | Example | Notes |
|-----------|---------|-------|
| `family` | `sensor`, `event`, `command` | Top-level category |
| `name` | `CompressedImage`, `WeatherReading` | One per schema; PascalCase |
| `version` | `v1`, `v2` | Bump for breaking changes; consumers can dispatch by version |

Consumers should treat unknown URIs as opaque payload — log and forward, do not fail.

---

## Encoding

- **CBOR** ([RFC 8949](https://www.rfc-editor.org/rfc/rfc8949)) — compact, self-describing, no schema needed to parse.
- Maps use string keys (no integer-key shortcuts) so debug tools can dump frames without a code book.
- Byte strings (`data: <bytes>`) use the CBOR byte-string type, not base64-of-string.

The Zenoh `encoding` field is set to `application/cbor` so subscribers know what to do without inspecting the payload.

---

## Two Wire Formats for Commands

Commands sent to a node's `command` queryable use **JSON** (not CBOR), and accept two envelope shapes for backwards compatibility with older daemons:

```jsonc
// Flat (bubbaloop daemon ≥ PR #80) — preferred
{ "command": "start_recording", "topic_patterns": ["**/compressed"] }

// Nested (older daemons / direct callers) — still accepted
{ "command": "start_recording", "params": { "topic_patterns": ["**/compressed"] } }
```

Nodes should accept both; the bubbaloop SDK does this transparently. Commands are JSON because they are written by humans (slash commands, MCP tools) and round-trip through tools that don't speak CBOR. Sensor data is CBOR because it is high-volume and binary.

---

## Recipes

### Decoding a frame in Python

```python
import cbor2

def decode_envelope(payload: bytes):
    env = cbor2.loads(payload)
    return env["header"]["schema_uri"], env["header"]["ts_ns"], env["body"]

uri, ts_ns, body = decode_envelope(zenoh_sample.payload.to_bytes())
if uri.endswith("/CompressedImage/v1"):
    h264_bytes = body["data"]
    # feed to PyAV / ffmpeg
```

### Detecting dropped frames

```python
last_seq = -1
def on_sample(sample):
    global last_seq
    env = cbor2.loads(bytes(sample.payload))
    seq = env["header"]["monotonic_seq"]
    if last_seq >= 0 and seq != last_seq + 1:
        log.warning("dropped %d frames", seq - last_seq - 1)
    last_seq = seq
```

### Recording verbatim (mcap-recorder)

The recorder writes the **full envelope** as the MCAP message data. Replay tools later re-parse the same envelope, so no information is lost — the wire format is its own archive format.

---

## Next Steps

- [Messaging](messaging.md) — Zenoh topics and SDK publishers/subscribers
- [Topics](topics.md) — Per-node topic catalog
- [Dataflow](dataflow.md) — How the dataflow tool discovers live edges
