---
description: "How Bubbaloop storage works under the hood: the manifest schema, streaming SHA-256 integrity, the sync queue driver, the reconcile diff-matrix, ring-buffer memory, and the replay planner."
---

# Storage Internals

The mechanics behind [Storage](storage.md): the on-disk manifest, the integrity
flow, the background sync queue, the reconcile diff-matrix, ring-buffer memory,
and replay. Read [Storage](storage.md) first for the model and a walkthrough.

---

## The Manifest

`manifest.json` at a recording's root is the **only** source of truth. There is
no separate index or database — `storage list` scans
`~/.bubbaloop/recordings/*/manifest.json`.

```json
{
  "schema_version": 1,
  "name": "outdoor_test_3",
  "machine_id": "jetson_alpha",
  "started_at_ns": 1748275200000000000,
  "ended_at_ns": 1748275300000000000,
  "mode": "streaming",
  "selection": { "topics": ["bubbaloop/global/*/cam_*/compressed"], "exclude": ["**/health"], "include_local": false },
  "channels": [
    { "topic": "...cam_front/compressed", "channel_id": 0, "message_count": 12345,
      "message_encoding": "cbor", "zenoh_encoding": "application/cbor" }
  ],
  "chunks": [
    { "name": "chunk-000000-a1b2c3d4.mcap", "index": 0, "size_bytes": 786432,
      "sha256": "a1b2c3d4…(64 hex)…", "log_time_first_ns": 1748275200000000000,
      "uploaded_at_ns": 1748275305000000000, "remote_etag": "\"a1b2…\"" }
  ],
  "profile_name": "demo",
  "profile_sha256": "abc123…",
  "tags": []
}
```

**Forward-compatible by design.** The loader carries a `schema_version` and
preserves unknown fields on round-trip via an `extra` passthrough on the
`Recording` and `Chunk` types — a future writer's fields survive an older reader
loading and re-saving the manifest, so no data is lost across versions.

**Derived state, not stored flags:**

- **Sync status is per-chunk.** A chunk is uploaded iff its `uploaded_at_ns` is
  set; the recording is fully synced iff *every* chunk is. There is no top-level
  status enum — partial uploads are visible and reconcilable without ambiguity.
- **Lifecycle status derives from `ended_at_ns`.** Set → `complete`; null while
  the recorder is running that session → `recording`; null while it is *not* →
  `interrupted` (a crash). `storage list` computes all of this client-side.

Every manifest write is `write_tmp_then_rename` (write a sibling `.tmp`, fsync,
rename over the target). A crash mid-write leaves either the old or the new
manifest intact, never a half-written one. The recorder's "append a chunk entry"
path and sync's "mark a chunk uploaded" path share one per-recording
`ManifestHandle` mutex, so the two writers on the daemon never race.

---

## Integrity: Streaming SHA-256, Verified Both Ways

Integrity is mandatory and end-to-end. The SHA-256 of each chunk is computed
**streaming during the write** (no second pass), then used at every boundary:

1. **At finalize** — the digest is embedded in the chunk filename
   (`chunk-{idx:06d}-{sha256_prefix8}.mcap`) and written full (64 hex chars) into
   the manifest.
2. **On upload** — the local file's SHA-256 is **re-computed and re-verified**
   against the manifest (never trusted blindly); a mismatch is local corruption
   (`BadDigest`) and dead-letters the job. For cloud backends the digest is sent
   as `x-amz-checksum-sha256` so the server validates it too.
3. **On download** — every chunk is verified on receipt against the manifest's
   SHA-256; a mismatch fails loud and the corrupted file is **never written** to
   disk.

The streaming digest is proven equal to a one-shot digest against known test
vectors.

### Path-traversal validation

Recording names and profile names are validated before they ever touch the
filesystem or a remote prefix — a name like `../../etc` can't reach
`remove_dir_all`, a download path, or an object key. On download, the on-disk
chunk filename is **derived** from `(index, sha256)` via the canonical
`chunk-{idx:06d}-{sha256_prefix8}.mcap` form rather than trusting the manifest's
`name` field as a path segment, removing all doubt about what gets written.

---

## The Sync Queue Driver

Background sync replaces the old standalone `uploader` node. It runs as a single
task inside the daemon when `[storage].auto_upload = true`, and is
**chunk-finalize-driven**, not poll-driven: when the MCAP writer renames
`.mcap.active` → `.mcap` it emits a `ChunkFinalized` event on an in-process
channel that sync consumes — zero-latency, no filesystem polling. A startup scan
of the manifests (enqueueing any chunk with `uploaded_at_ns` still null) is the
safety net for anything missed while the daemon was down.

**Bounded FIFO queue.** A `VecDeque<UploadJob>` (default cap 1000) with a
semaphore bounding concurrency (default 3 parallel uploads). If the queue fills,
the recorder's finalize path back-pressures rather than silently filling disk.

**Shared backoff — the whole queue pauses as a unit.** Backoff is *not* per-job.
If one upload fails because the backend is down, every subsequent upload will
fail too, so burning the queue retrying doomed jobs is pointless. Instead a
single shared `BackoffState` pauses the queue and stretches retries
`5s → 10s → 20s → 40s → 60s` (clamped). The **first successful upload after a
failure resets the shared backoff to attempt 1 for everyone.**

**Manifest re-upload dedup.** The manifest is re-uploaded after each
chunk-completion (so remote state is crash-resilient), but a 500 ms coalescing
window collapses a burst of chunk-finalizes into a single manifest re-upload —
the manifest is identical regardless of which chunk triggered it.

**Retryable vs terminal errors:**

| Class | Errors | Handling |
|-------|--------|----------|
| Retryable | network timeouts, 5xx, throttling (429), DNS | retry under the shared backoff |
| Terminal | `Forbidden` (bad auth), `BadDigest` (local corruption) | → dead-letter immediately |
| `NoSuchBucket` | config is wrong | **pause sync entirely until reconfigured** |

After `max_elapsed_time` (5m default), a job moves to a **dead-letter list**
persisted at `~/.bubbaloop/storage/dead_letter.json` (`write_tmp_then_rename`).
Dead-letter retry is **not** automatic — terminal failures got there for a
reason, and surfacing them to a human is intentional. The next manual
`storage upload` or `storage reconcile` flushes the list. On daemon restart, the
union of the persisted dead-letter list and the startup manifest scan restores
full queue state.

> **Why no file-backed queue in v1.** The profile's `sending_queue.storage_dir`
> field exists for forward-compat with OTel Collector semantics, but v1
> reconstructs the queue from the manifest scan on startup rather than persisting
> the queue itself. A file-backed queue would need its own corruption story; the
> manifest-scan approach is simpler and equally crash-resilient.

### Idempotent upload: HEAD before PUT

Each chunk uploads via a HEAD-before-PUT sequence that makes re-uploads no-ops
and never silently overwrites:

```
1. Re-verify local SHA-256 against the manifest   → mismatch ⇒ BadDigest, dead-letter
2. HEAD the deterministic object key
     200 + matching sha256  ⇒ already uploaded; record uploaded_at_ns; done
     200 + mismatched sha256 ⇒ someone else's content at our key; warn + dead-letter for review
     404                     ⇒ proceed to PUT
3. PUT with x-amz-checksum-sha256
     200       ⇒ record uploaded_at_ns + remote_etag (write_tmp_then_rename)
     400/Digest⇒ local corruption ⇒ dead-letter
     403       ⇒ terminal ⇒ dead-letter, pause sync
     5xx/net   ⇒ retry under shared backoff
4. Re-upload the (small) manifest.json
```

The CLI `storage upload` verb runs this same `upload_one` per chunk, mutating the
manifest in memory and persisting it once at the end — and only when something
actually changed, so an all-failed or all-conflict run never clobbers newer
remote manifest state.

---

## Reconcile: The Diff-Matrix

`storage reconcile <name>` is the user-facing healing primitive. It diffs **three
sources of truth** and converges them: the local manifest, the local chunk files,
and the remote objects (listed via `ListObjectsV2` under the recording's prefix).
It holds the recording's `ManifestHandle` mutex (so no concurrent sync), runs a
single pass, and re-uploads the manifest at the end.

For each chunk index in the manifest:

| Manifest | Local file | Remote | SHA-256 | Action |
|:---:|:---:|:---:|---|---|
| ✓ | ✓ | ✓ | all match | mark `uploaded_at_ns` if it was null; else no-op |
| ✓ | ✓ | ✗ | local matches manifest | **upload** the missing chunk |
| ✓ | ✓ | ✓ | remote mismatches | **re-upload** local (deterministic key ⇒ safe overwrite); warn |
| ✓ | ✓ | ✓ | local mismatches manifest | **local corruption** — error, do not upload |
| ✓ | ✗ | ✓ | remote matches manifest | local deleted (retention sweep) — **re-download** if `--restore`, else mark and continue |
| ✓ | ✗ | ✗ | — | gone everywhere — manifest broken, mark recording `corrupt` |
| ✗ | ✓ | ? | — | **orphan local file** — warn, do not touch |
| ✗ | ✗ | ✓ | — | **orphan remote object** — warn, do not auto-delete |

**Reconcile never deletes anything** (local or remote). It only uploads missing
chunks, re-uploads mismatched ones, re-downloads (with `--restore`), and reports
orphans. That keeps it **idempotent and safe to re-run** — running it five times
produces the same result as once, and never destroys data. Deletion is a separate
explicit concern (`storage rm`, retention-driven local cleanup of uploaded
recordings, and a future `storage gc --remote` for orphans). The CLI returns a
`ReconcileReport` — `chunks_diffed / uploaded / reuploaded / redownloaded /
verified_ok / marked_uploaded`, plus orphan counts and per-chunk errors.

---

## Ring-Buffer Mode

Ring-buffer mode is the pre-trigger differentiator — "keep the last N seconds, let
me seal it when something interesting happens." The window is held **purely in
RAM** (no disk spillover in v1, which would double I/O and complicate retention),
bounded by **two caps that both apply**:

- a **time window** (`window_secs`), and
- a **256 MiB byte cap** (`ring_max_bytes`, the OOM safety net).

Samples are evicted **FIFO** whenever *either* cap is exceeded. Sizing is the
user's responsibility — budget RAM accordingly:

| Stream rate | 60 s window | Notes |
|-------------|-------------|-------|
| 1 Mbps | ~7.5 MB | telemetry, IMU |
| 100 Mbps | ~750 MB | exceeds the 256 MiB cap — the byte cap evicts first |

`flush` atomically snapshots the current ring contents, writes them to a
**brand-new finalized recording directory** (`{name}-flush-{ISO}/`) with its own
manifest, and resets the window-start anchor while keeping still-in-window
samples (they may belong to the next window too). The sealed recording's name is
returned so you can immediately `info` / `upload` it. Concurrent flushes of the
same session serialize via the session mutex; different sessions flush
concurrently.

---

## Replay

Replay reads a recording's MCAP chunks and re-publishes them into Zenoh with
their **original encoding and timing**. The design separates a **pure planner**
from a **sink**:

- The **planner** is pure and testable: given the recorded messages and
  `ReplayOptions` (`rate`, `loop`, `start_time_ns` / `end_time_ns` trim,
  `topics` / `exclude` filters, `remap` rules, `use_log_time`), it decides what to
  publish and when. `--start-time` / `--end-time` are offsets in seconds from the
  recording's first message on the selected timeline; the planner trims against
  the recorded timestamps. Timing defaults to `publish_time`; `--use-log-time`
  schedules by the recorder's `log_time` instead.
- The **`ReplaySink` trait** is the output boundary. The CLI's `ZenohReplaySink`
  re-publishes onto a live Zenoh session, restoring each message's recorded
  encoding (`zenoh.encoding` from the MCAP channel metadata, per the wire-format
  contract) so payloads are byte-identical. Tests swap in a fake sink.

Crucially the sink publishes with **`CongestionControl::Block`** (not Zenoh's
put-default `Drop`): a slow link or subscriber back-pressures the replay instead
of silently dropping samples. Because the planner already paces publishes,
blocking only ever stretches timing — it never loses data, which is what makes
replay faithful. Ctrl-C cancels (and is the only way to stop `--loop`). v1
buffers the whole recording into memory and warns above 1 GiB.

---

## Agent Operations

Storage is MCP-everywhere: every CLI verb has a paired `storage_*` tool. A few
tools are **MCP-only** — they exist because agents need state inspection that
humans get from the dashboard or process introspection, and they have no CLI
counterpart:

| MCP-only tool | Why it exists |
|---------------|---------------|
| `storage_config_get` | Lets an agent read the current `[storage]` config (secrets redacted) before suggesting changes. A human just reads `~/.bubbaloop/config.toml`. |
| `storage_record_sessions` | Lists **active** recording sessions across the fleet (backed by the recorder's `status` queryable). Without it an agent can't see what's already recording before starting another. A human looks at the dashboard. |
| `storage_replay_start` / `storage_replay_stop` / `storage_replay_status` | Replay is long-running; MCP calls have no Ctrl-C, so the single CLI `replay` verb expands to a `_start`/`_stop`/`_status` triad on the MCP side. |

RBAC mirrors blast radius: **read = viewer** (`storage_config_get`,
`storage_list`, `storage_info`, `storage_profile_list`, `storage_profile_show`,
`storage_topics_list`, `storage_replay_status`); **day-to-day = operator**
(`storage_upload`, `storage_download`, `storage_reconcile`, `storage_replay_*`,
`storage_profile_create`/`_rm`); **config / destructive = admin**
(`storage_configure`, `storage_rm`).

---

## Next Steps

- [Storage](storage.md) — the model, profiles, secrets, and the end-to-end walkthrough
- [Wire Format](wire-format.md) — the provenance envelope replay preserves byte-for-byte
- [Messaging](messaging.md) — Zenoh pub/sub, queryables, and congestion control
