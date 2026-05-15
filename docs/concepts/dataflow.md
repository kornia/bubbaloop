# Dataflow: envelopes, manifests, and live DAG

This page documents the three pieces the SDK adds so humans and agents can
answer *"what is wired to what, and is it actually flowing?"* without grepping
configs.

All three are wired automatically by the SDK. Node authors do **not** declare
them manually.

---

## 1. Provenance envelope

Every `publisher_json` / `publisher_cbor` message is wrapped:

```jsonc
{
  "header": {
    "schema_uri":      "bubbaloop://tapo_terrace_embedder/embeddings@v1",
    "source_instance": "tapo_terrace_embedder",
    "monotonic_seq":   42,
    "ts_ns":           1776137130135213277
  },
  "body": { /* your payload */ }
}
```

- **`schema_uri`** defaults to `bubbaloop://{instance}/{suffix}@v{schema_version}`.
  Override with `publisher_*_with_schema()` (Rust) or the `schema_uri=` kwarg
  (Python). Passing an empty string is respected — the SDK will not re-derive.
- **`monotonic_seq`** is per-publisher, incremented on each `put()`. Useful for
  detecting drops.
- **`ts_ns`** is the publisher's monotonic nanosecond clock at `put()` time.

Subscribers (`ctx.subscribe(...)`) auto-unwrap — your callback receives an
`Envelope` with `.body` and `.header` attributes. Legacy unenveloped publishers
still work: `getattr(env, "body", env)` is the compatibility idiom.

**Raw publishers (`publisher_raw`) are *not* enveloped** — they're meant for
high-rate binary frames (SHM video) where per-message overhead matters. Use
envelopes for JSON/CBOR; raw for bulk bytes.

---

## 2. Manifest queryable

Every node served by the SDK automatically exposes:

```
bubbaloop/global/{machine_id}/{instance_name}/manifest
```

as a CBOR queryable. Its payload is populated from the **actual** `publisher_*`
and `subscribe()` calls the node made — no `node.yaml` grepping, no drift.

```jsonc
{
  "schema_version": 2,
  "instance":       "tapo_terrace_motion",
  "role":           "processor",          // source | processor | sink | unknown
  "node_kind":      "python",
  "machine_id":     "nvidia_orin00",
  "started_at_ns":  1776137130135213277,
  "inputs":  [{"topic": "…/tapo_terrace_embedder/embeddings",
               "declared_at_ns": 1776…,
               "ever_fired": true,
               "still_live": true}],
  "outputs": [{"topic": "…/tapo_terrace_motion/events",
               "declared_at_ns": 1776…,
               "ever_fired": false,
               "still_live": true}]
}
```

### Liveness tracking

Each I/O entry has three flags the SDK maintains via a `ManifestHook`:

| Flag            | Meaning                                                            |
|-----------------|--------------------------------------------------------------------|
| `declared_at_ns`| When the publisher/subscriber was created.                         |
| `ever_fired`    | Set once on first `put()` / first received sample (atomic fast path). |
| `still_live`    | `true` while the publisher/subscriber is alive; flipped on `Drop`/`undeclare`. |

This is what prevents *ghost entries*: a node that creates a subscriber and
then drops it won't leave a stale input in the manifest.

### Default edge inference

A topic is considered a real edge when `still_live && ever_fired`. The SDK
exports that as the default filter for `bubbaloop dataflow`.

---

## 3. `bubbaloop dataflow` CLI + MCP tool

```bash
# Human-readable summary (groups by role, highlights orphans)
bubbaloop dataflow

# Full graph as JSON (for scripts, CI, or piping to jq)
bubbaloop dataflow --json

# Include I/O that was declared but never fired (catches broken wiring fast)
bubbaloop dataflow --include-declared-but-unused

# Custom endpoint / timeout
bubbaloop dataflow -z tcp/127.0.0.1:7447 --timeout-secs 3
```

Under the hood it's a single Zenoh query:

```
get bubbaloop/**/manifest
```

…merged into a graph: `{nodes, edges, orphan_inputs, unconsumed_outputs}`.

The **MCP tool** `dataflow` (Viewer tier) returns the same structure for
agents. Params:

```jsonc
{ "include_declared_but_unused": false }   // default
```

### Output fields

- **`nodes`** — one per responding instance. Includes `role`, `machine_id`,
  `node_kind`, `started_at_ns`.
- **`edges`** — `{from, to, topic}` where `from` is the producer's instance
  and `to` is the subscriber's instance. Computed by matching live publisher
  outputs against live subscriber inputs.
- **`orphan_inputs`** — subscriptions with no matching producer. *"This node
  expects data nobody is sending."*
- **`unconsumed_outputs`** — publishers with no matching subscriber. *"This
  node is publishing into the void."*

### Use this when you'd otherwise

- `systemctl status` — only tells you the process is alive, not that it's
  receiving frames.
- Dashboard — shows raw topic traffic but can't tell you *which* node is
  consuming what.
- `grep input_topic: configs/*.yaml` — lies the moment a config drifts from
  deployed reality.

---

## Topic key spaces (recap)

```
bubbaloop/global/{machine_id}/{instance_name}/{suffix}   # network-visible
bubbaloop/local/{machine_id}/{instance_name}/{suffix}    # SHM-only
```

Publishers auto-scope under `{instance_name}` — `ctx.publisher_json("data")`
becomes `…/{instance}/data`. Subscribers are **absolute** — pass the upstream
node's full suffix including its instance: `ctx.subscribe("tapo_terrace/raw")`.
The asymmetry is intentional: publishers own their outputs, subscribers
consume *someone else's*.

See [`CLAUDE.md` → Topic naming (auto-scoped)](../../CLAUDE.md) for the
short-form rules.

---

## How the pieces fit

```
node code                     SDK runtime                    zenoh
─────────                     ───────────                    ─────
ctx.publisher_json("x")  ───▶ register in manifest ─────────▶ (no traffic yet)
pub.put(payload)         ───▶ wrap in {header,body}  ───────▶ bubbaloop/global/…/x
                              mark ever_fired=true
                              increment monotonic_seq

ctx.subscribe(abs_topic) ───▶ register in manifest ─────────▶ (subscribe)
for env in sub:          ◀── unwrap envelope   ◀────────────  sample arrives
                              mark ever_fired=true

(agent or user)               ManifestQueryable
bubbaloop dataflow       ───▶ reply with current  ◀────────── get bubbaloop/**/manifest
                              {inputs, outputs, liveness}
```

Three layers, one contract. No manual manifests, no drift.
