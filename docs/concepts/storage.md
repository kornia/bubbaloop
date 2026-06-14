---
description: "Fleet recording in Bubbaloop. MCAP chunks + manifest, content-addressed upload to S3-compatible backends, reconcile, and replay — driven from the CLI and MCP."
---

# Storage

Fleet recording → MCAP chunks + manifest → content-addressed upload to an
S3-compatible backend → reconcile → replay.

Storage is the third pillar of Bubbaloop, alongside the **node runtime** and the
**agent runtime**. Where nodes produce live data and agents reason over it,
storage *captures* the fleet's Zenoh traffic to disk, ships it to the cloud, and
plays it back — with byte-identical payloads and original timing.

For the on-disk formats, integrity flow, sync queue, and reconcile diff-matrix,
see [Storage Internals](storage-internals.md).

---

## The Model

A **Recording** is a self-describing directory. There is no separate index, no
SQLite, no metadata service — `manifest.json` *is* the source of truth.

```
~/.bubbaloop/recordings/{name}/
├── manifest.json                      ← the source of truth (schema, chunks, sha256, upload state)
└── chunks/
    ├── chunk-000000-a1b2c3d4.mcap     ← MCAP, with the chunk's sha256 prefix in the filename
    └── chunk-000001-e5f6a7b8.mcap
```

| Piece | What it is |
|-------|-----------|
| **manifest.json** | Recording metadata: machine_id, time bounds, topic selection, per-channel stats, and one entry per chunk (name, size, full SHA-256, upload state). Forward-compatible (`schema_version`, unknown fields preserved). |
| **chunks/** | The recorded data, written as MCAP files. Defaults match rosbag2 verbatim: **786432-byte chunks, `zstd` compression, CRC on.** Every message keeps **both** `publish_time` (the Zenoh sample timestamp) and `log_time` (the recorder's clock at receipt). |
| **SHA-256 per chunk** | Computed streaming during the write, embedded in the filename, stored full in the manifest, and (for cloud backends) sent as `x-amz-checksum-sha256`. Verified again on download. Integrity is mandatory, not configurable. |

### Content-addressed object keys

Uploads are deterministic and idempotent. The object key embeds the machine,
recording, chunk index, and SHA-256 prefix:

```
{bucket}/{machine_id}/{recording_name}/manifest.json
{bucket}/{machine_id}/{recording_name}/chunks/chunk-{idx:06d}-{sha256_prefix8}.mcap
```

Same content → same key, so a re-upload is a no-op (caught by a `HEAD` check).
Changed content → different key, so nothing is ever silently overwritten. The
`{machine_id}/` prefix also makes a **bucket the sharing unit**: point two
machines at the same bucket and `storage list --remote` is fleet-wide.

### Backends

A `StorageBackend` trait abstracts the object store. Two implementations ship:

| Backend | `--backend` value | Notes |
|---------|-------------------|-------|
| **Local FS** | `local` | An on-disk object store under `--local-path`. Runs the whole flow without any cloud credentials — ideal for trying things out and for tests. |
| **S3-compatible** | `r2`, `s3`, `gcs`, `minio` | Cloudflare R2 / AWS / GCS / MinIO via `aws-sdk-s3`. Built with the `s3` cargo feature. |

### Profiles

A **profile** is a small committable YAML file at
`~/.bubbaloop/profiles/{name}.yaml` that captures *what to record and how*: the
topic selection, MCAP/compression knobs, and the capture mode. Commit them to a
repo and a new teammate gets "the record button" the team uses.

```yaml
name: demo
description: "Camera + IMU for outdoor demos"
topics:                                  # additive include patterns
  - 'bubbaloop/global/*/cam_*/compressed'
regex: '^bubbaloop/global/.+/imu/.+'     # additive regex include
exclude:                                 # subtractive
  - '**/health'
include_local: false                     # default: skip SHM-only bubbaloop/local/**
chunk_size_bytes: 786432
compression: zstd
mode: streaming                          # streaming | ring_buffer
```

Topic selection **composes additively**: the final set is
`(topics ∪ regex_match) - exclude`. The two capture modes:

- **`streaming`** — record everything matching the selection until you stop. The default.
- **`ring_buffer`** — keep only a sliding window (by time and a 256 MiB byte cap) in memory; `flush` seals the current window as a brand-new finalized recording and keeps going. This is the pre-trigger / "capture the last 60 seconds" mode that rosbag2 lacks. See [Storage Internals → Ring buffer](storage-internals.md#ring-buffer-mode).

### Secrets

Cloud credentials live **only** in `~/.bubbaloop/secrets.toml` (chmod `0600`),
never in `config.toml`, never in a profile, never in logs.

```toml
[storage.r2]
access_key_id = "..."
secret_access_key = "..."
```

`storage configure --access-key-id … --secret-access-key …` writes this file for
you with the right permissions. The loader wraps secrets in zeroize-on-drop,
`Debug`-redacted types so they can't leak through logging.

---

## End-to-End Walkthrough (local backend, no cloud)

This runs the full lifecycle against the `local` backend, so no R2 account or
credentials are needed. Recording itself is driven by the `mcap-recorder`
node (see the note below); every other verb — `list`, `info`, `upload`,
`download`, `reconcile`, `rm`, `replay`, `profile`, `configure`, `topics` — works
today against any backend, including `local`.

```bash
# 1. Point storage at an on-disk object store (no credentials needed).
bubbaloop storage configure --backend local --local-path /tmp/objstore

# 2. See what's live on the fleet before you record.
bubbaloop storage topics

# 3. Record. Recording-start is driven by the mcap-recorder node
#    (a separate marketplace node — see the note below). It writes the
#    recording into ~/.bubbaloop/recordings/{name}/.

# 4. List local recordings (NAME, MACHINE, MODE, CHUNKS, SYNC, STATUS, STARTED).
bubbaloop storage list

# 5. Inspect one recording's manifest (pretty-printed JSON).
bubbaloop storage info <name>

# 6. Upload its chunks to the configured backend. Idempotent — already-uploaded
#    chunks are skipped by SHA-256 match.
bubbaloop storage upload <name>

# 7. Reconcile (heal any local/remote divergence) and replay back into Zenoh.
bubbaloop storage reconcile <name>
bubbaloop storage replay <name> --rate 1.0
```

`replay` re-publishes the recorded samples into Zenoh with their original
encoding and timing — `--rate` scales speed, `--loop` repeats, `--start-time` /
`--end-time` trim, `--topics` / `--exclude` filter, and `--remap from:=to`
re-keys. Press Ctrl-C to stop.

!!! note "The recorder is a separate marketplace node"
    Honest scoping: **recording-start is driven by the `mcap-recorder` node**, a
    sink node from the marketplace (in `bubbaloop-nodes-official/sinks/`, distinct
    from the official nodes that ship in this repo). It sits idle until it
    receives a `start` command, then writes MCAP chunks + a manifest into
    `~/.bubbaloop/recordings/`. Once a recording exists on disk, every other
    storage verb operates on it directly — none of them need the recorder
    running. The local backend means you can exercise `list → info → upload →
    reconcile → replay` end-to-end without ever touching the cloud.

### `list` output

`storage list` scans `~/.bubbaloop/recordings/*/manifest.json` and prints a
table. `SYNC` is `synced` once every chunk has an `uploaded_at_ns`, else `local`;
`STATUS` is `complete` once the manifest is closed, `open` while it's still being
written, or `corrupt` if reconcile flagged missing chunks.

```
NAME                         MACHINE        MODE         CHUNKS  SYNC     STATUS    STARTED
outdoor_test_3               jetson_alpha   streaming      12/12 synced   complete  2026-06-15 09:13:20
indoor_imu_only              jetson_beta    streaming       0/3  local    open      2026-06-15 09:40:02
```

Add `--remote` to list recordings in the configured backend instead (it lists
objects under the bucket prefix and reads each remote `manifest.json`).

---

## Agents Drive the Same Operations (MCP)

Storage follows Bubbaloop's MCP-everywhere rule: **every CLI verb has a paired
`storage_*` MCP tool**, so an agent can do anything a human can at the terminal.
A handful of MCP-only tools exist for state inspection humans get from the
dashboard or process introspection.

| Group | Tools |
|-------|-------|
| **Read (viewer)** | `storage_config_get`, `storage_list`, `storage_info`, `storage_profile_list`, `storage_profile_show` |
| **Day-to-day (operator)** | `storage_upload`, `storage_download`, `storage_reconcile`, `storage_profile_create`, `storage_profile_rm` |
| **Config / destructive (admin)** | `storage_configure`, `storage_rm` (`remote` delete is admin even though local-only `rm` is operator-equivalent) |
| **Topic discovery (viewer)** | `storage_topics_list` — queries the fleet's `bubbaloop/**/manifest` queryables over Zenoh |
| **Replay (operator)** | `storage_replay_start` / `storage_replay_stop` / `storage_replay_status` — replay is long-running, so it gets a `_start`/`_stop`/`_status` triad (MCP calls have no Ctrl-C) |

The RBAC tiers mirror the CLI's blast radius: **read = viewer, day-to-day =
operator, config and remote-delete = admin.** See
[Storage Internals → Agent operations](storage-internals.md#agent-operations)
for the MCP-only tools and why they exist.

---

## Topology and Limits

The recorder runs as **one node, anywhere, in `client` mode**. Because every
Bubbaloop node already connects as a client to `zenohd`, the recorder sees the
entire `bubbaloop/global/**` namespace — it's the centralized pattern (one
process per fleet), like rosbag2 and the Foxglove Agent. Edge devices do **not**
need cloud credentials; only the host running sync does.

This centralized model is validated for small-to-medium fleets (≲10 nodes). It
breaks down when sustained topic bandwidth exceeds ~50% of the router's link or
the recorder pegs ~80% of one CPU core. The documented escape hatch is a v2
federated topology (per-instance recorders that store-and-forward) — the object
key layout (`{machine_id}/{recording_name}/`) already accommodates it, so no wire
format changes are needed to get there.

---

## Next Steps

- [Storage Internals](storage-internals.md) — manifest schema, integrity flow, sync queue, reconcile diff-matrix, ring buffer, replay
- [Messaging](messaging.md) — the Zenoh pub/sub and queryable layer storage records and replays onto
- [Architecture](architecture.md) — layer model, daemon, the three pillars
