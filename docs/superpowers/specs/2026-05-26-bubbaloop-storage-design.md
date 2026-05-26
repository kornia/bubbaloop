# Bubbaloop Storage — Design Spec

**Status:** Approved — ready for implementation
**Date:** 2026-05-26
**Author:** edgar.riba@gmail.com (with Claude)
**Path:** `docs/superpowers/specs/2026-05-26-bubbaloop-storage-design.md`

---

## 1. Context

The Bubbaloop team has been recording fleet data using the out-of-tree `recorder` node (`bubbaloop-nodes-internal/recorder/`), shipping `.mcap` files to GCS via the separate `uploader/` node. That setup works for a single Jetson but ran into four real pains as the fleet grew:

1. **Topic visibility / config** — per-machine recorders required per-instance `topic_patterns`; maintaining N configs was a chore.
2. **No unified recording** — N machines produced N independent `.mcap` files that had to be correlated offline.
3. **Operational friction** — "record now" wasn't a single button; users had to know which machine the recorder was on.
4. **Cloud auth sprawl** — configuring credentials on every Jetson individually was untenable.

The tactical workaround was to put the recorder in the dashboard (browser aggregates fleet samples via WebSocket bridge, backend stores). That solves visibility but introduces fragility — requires browser open, no scheduled/event-triggered recording, awkward upload path.

This spec proposes **`bubbaloop storage` as a first-class subsystem** alongside `bubbaloop node` and `bubbaloop agent`, with a fleet-aware recorder topology and Cloudflare R2 as the convenience cloud backend. The design is informed by a comparative pass over rosbag2, Foxglove Data Platform, LeRobot, PX4/Auterion/Skydio fleet log management, AWS Greengrass Stream Manager, Kafka tiered storage, and OpenTelemetry Collector — adopting their proven conventions and avoiding the friction points their users complain about.

The intended outcome: one user runs `bubbaloop storage record start`, captures fleet-wide data into a single `.mcap` set, sees it auto-upload to R2, and teammates run `bubbaloop storage list --remote` from their own machines to discover and download it. Replay works offline against any downloaded recording. Recordings are self-describing (`manifest.json`), integrity-verified (SHA-256 per chunk), and reconcilable (local manifest is the source of truth; `reconcile` heals partial-upload failures).

---

## 2. Goals / Non-goals

### Goals (v1)
- One recorder per fleet that sees all machines via the existing Zenoh router (`mode: "client"`).
- `bubbaloop storage` CLI subsystem: `configure | topics | profile | record (start/stop/flush) | list | info | upload | download | replay | reconcile | rm`.
- MCP tools mirroring the CLI for agent-controlled recording.
- Dashboard "Recordings" tab: list (local + remote), start/stop, download. Topic picker UI driven by manifest queryable.
- Saved profiles in `~/.bubbaloop/profiles/*.yaml` — committable, shareable, reviewable. Field names borrowed from OTel Collector (`sending_queue`, `retry_on_failure`).
- `StorageBackend` trait with `LocalFs` and `S3Compat` (R2/AWS/GCS/MinIO via `aws-sdk-s3`).
- **Integrity:** SHA-256 per finalized chunk, embedded in manifest, transmitted on upload via `x-amz-checksum-sha256`, verified on download.
- **Self-describing recordings:** `manifest.json` is the only source of truth (no separate SQLite index). `storage list` scans the recordings directory.
- **Ring-buffer mode:** `record start --window 60s --trigger on-event|manual` keeps a sliding window in memory + disk spillover; `record flush` seals the current window as a finalized recording. This is rosbag2's most-requested missing feature.
- Replay re-publishes into Zenoh with byte-identical payloads and original timing; both `publish_time` (Zenoh sample) and `log_time` (recorder clock) preserved in MCAP.
- Auto-upload of finalized chunks, configurable retention, deterministic object keys for idempotent re-uploads.
- `storage reconcile <name>` heals partial-upload failures by diffing local manifest against R2 listing.

### Non-goals (deferred)
- In-browser MCAP playback / scrubber (Phase 2 — own spec).
- Cloudflare Worker for presigned URLs / per-machine bearer tokens (Phase 2 when team-level auth is needed; v1 uses shared R2 credentials).
- Event-triggered auto-recording wired to agent arousal / alerts (Phase 3).
- Dataset tagging, full-text search, retention tiers (Phase 3).
- `storage publish` to HuggingFace Hub for ML datasets (Phase 3; **verb is reserved** in v1 — do not squat).
- Per-machine "edge recorder" mode for disconnected operation (deferred; centralized recorder covers all current pains; v2 escape hatch documented).
- Cloudflare Stream integration (likely never — R2 + progressive MP4 / H264 in MCAP is enough).
- `storage import <file.mcap>` for sideloading external recordings (defer; small but adds CLI surface).
- Network-policy uploads (`upload_on: [wifi, ethernet]`) (defer; only matters once tethered/4G robots exist).

---

## 3. Architecture

### 3.1 Topology

```
         ┌──────────────┐  ┌──────────────┐
Jetson A │ camera nodes │  │ Jetson B     │   (sources, peers of router via client mode)
         └──────┬───────┘  └──────┬───────┘
                │                  │
                └────────┬─────────┘
                         ▼
                   ┌───────────┐
                   │  zenohd   │       (existing router)
                   └─────┬─────┘
                         │
       ┌─────────────────┼──────────────────┐
       ▼                 ▼                  ▼
  ┌─────────┐     ┌─────────────┐    ┌────────────┐
  │Dashboard│     │mcap-recorder│    │ other      │
  │ browser │     │ (one per    │    │ daemons    │
  │(control │     │  fleet,     │    │            │
  │surface) │     │  client     │    │            │
  └─────────┘     │  mode)      │    └────────────┘
                  └──────┬──────┘
                         │
                         ▼
              ~/.bubbaloop/recordings/{name}/
                 ├── manifest.json
                 └── chunks/
                     ├── chunk-000000-a1b2c3d4.mcap   (sha256 prefix in filename)
                     └── chunk-000001-e5f6a7b8.mcap
                         │
                         ▼ (storage::sync, background)
              ┌─────────────────────┐
              │   Cloudflare R2     │
              │  {bucket}/          │
              │   {machine_id}/     │
              │    {recording_name}/│
              └─────────────────────┘
```

**Key topology decisions (and the prior art that validates them):**

- The recorder is **one node, anywhere in `client` mode**. Because all Bubbaloop nodes already use client mode against zenohd, the recorder sees the entire `bubbaloop/global/**` namespace. This is the **centralized** pattern — aligned with rosbag2 (single process) and Foxglove Agent (one process per host), distinct from drone-fleet federated uploads (which exist only because drones have no shared bus). Validated for small-to-medium fleets (≤10 nodes).
- The recorder runs on whichever host is convenient — most teams will pick the dev laptop or bridge machine. Edge devices do **not** need R2 credentials.
- The dashboard is demoted to a **control surface**: it tells the recorder what to record and lists results. It does not aggregate samples. Browser-side recording is deprecated with a one-release migration window.
- The `uploader` node is **removed entirely** and folded into core (`storage::sync`). One less process, one less node.yaml, one less marketplace entry. The uploader was never Zenoh-bound work; it doesn't need to be a node.

**Centralized → federated escape hatch (documented, not implemented):**

When sustained aggregate topic bandwidth exceeds ~50% of the router's link OR the recorder's CPU exceeds ~80% on one core, the centralized model breaks. The v2 fallback is per-instance local recorders writing to `bubbaloop/local/{instance}/recordings/...` then store-and-forward to R2 via the same uploader. No design or schema changes are needed for v1 to enable this later; the object key layout (`{machine_id}/{recording_name}/`) already accommodates per-machine recorders. Document this in `docs/concepts/storage.md` so users know the limits.

### 3.2 Module structure

```
crates/bubbaloop/src/storage/
├── mod.rs              # subsystem entry, public types
├── backend.rs          # trait StorageBackend { put, get, list, delete, head }
├── backend/local.rs    # LocalFs (~/.bubbaloop/recordings/)
├── backend/s3_compat.rs # aws-sdk-s3 — R2/AWS/GCS/MinIO, SHA-256 checksums
├── recording.rs        # Recording struct + manifest.json schema
├── manifest.rs         # load/save/validate manifest.json
├── profile.rs          # load/save/list ~/.bubbaloop/profiles/*.yaml
├── discover.rs         # query bubbaloop/**/manifest, group by machine/node
├── secrets.rs          # ~/.bubbaloop/secrets.toml (chmod 0600, zeroize-on-drop)
├── integrity.rs        # SHA-256 helpers (streaming digest, verify)
├── ring_buffer.rs      # circular MCAP writer for --window mode
├── sync.rs             # background uploader (replaces uploader/ node)
├── reconcile.rs        # diff local manifest vs R2 listing, re-upload missing
└── replay.rs           # read .mcap → re-publish into Zenoh (preserves both timestamps)

crates/bubbaloop/src/cli/storage/
├── mod.rs              # argh subcommand tree
├── configure.rs        # interactive wizard
├── topics.rs           # discover and list live topics
├── profile.rs          # profile create/list/show/rm
├── record.rs           # record start/stop/flush
├── list.rs / info.rs / upload.rs / download.rs / replay.rs / reconcile.rs / rm.rs

crates/bubbaloop/src/mcp/storage_tools.rs
                        # MCP tools mirroring CLI
```

**Notable absence: no `index.rs`, no SQLite.** The recordings directory is the database. `manifest.json` per recording is the source of truth. Listing scans `~/.bubbaloop/recordings/*/manifest.json` (fast for <10k recordings; if a user ever hits that scale, a v2 index is trivial to add without breaking the wire format). This follows LeRobot's self-describing dataset pattern — no separate metadata service, no migration risk, no "DB out of sync with disk" bugs.

### 3.3 Marketplace node: `mcap-recorder`

#### 3.3.1 Provenance and contract

- Pulled from `bubbaloop-nodes-internal/recorder/` into `bubbaloop-nodes-official/sinks/mcap-recorder/`.
- Declares `role: sink` in `config.yaml` (today both recorder and uploader incorrectly claim `processor`).
- `node.yaml v2` declares **zero outputs** (it's a terminal sink) and one **control input** (the command queryable below). The dataflow graph (`bubbaloop dataflow`) shows it as a leaf with an arrow from every recorded topic.
- Long-lived idle node. Autostart on install (`node_autostart_enable`). Sits waiting on its command queryable until `record start` arrives — zero subscriptions, zero CPU at rest.

#### 3.3.2 Command envelope (canonical `{command, params}`)

**Replaces** today's flat-tagged `{command, topic_patterns, ...}` with the project-standard envelope. Served as a Zenoh queryable at:

```
bubbaloop/global/{machine_id}/{instance_name}/command
```

Schema (CBOR-encoded, mirrors §6.2 MCP tool params 1:1):

```rust
// Wire type
#[derive(Serialize, Deserialize)]
struct RecorderCommand {
    command: CommandKind,           // "start" | "stop" | "flush" | "status"
    params: CommandParams,
}

#[serde(tag = "command", rename_all = "snake_case")]
enum CommandParams {
    Start {
        name: String,                                // session name (caller assigns, never auto-generated by the recorder)
        topics: Vec<String>,                         // resolved literal patterns (storage::sync resolves regex/profile client-side)
        exclude: Vec<String>,
        mode: SessionMode,                           // Streaming | RingBuffer { window_secs }
        duration_secs: Option<u64>,
        chunk_size_bytes: u32,
        compression: CompressionKind,
        compression_level: CompressionLevel,
        chunk_crc: bool,
        profile_name: Option<String>,                // for manifest provenance only — recorder doesn't load profile YAML
        profile_sha256: Option<String>,              // same
    },
    Stop { name: Option<String> },                   // None = stop all
    Flush { name: String },                          // ring-buffer only; required
    Status { name: Option<String> },                 // None = all sessions
}
```

**Why caller-assigned names:** the recorder must never be the source of truth for the recording name. The CLI/MCP layer auto-generates `rec_{ISO}_{machine_id}` if the user didn't pass `--name`, then passes the resolved name to the recorder. This keeps the recorder stateless about naming policy and makes the wire protocol idempotent (same `name` twice → second call gets `AlreadyExists`).

**Reply** is the same `CommandResponse` for all commands:

```rust
struct CommandResponse {
    ok: bool,
    session: Option<SessionState>,                   // populated on success of start/status/flush
    sealed_recording: Option<String>,                // populated on success of flush (the new finalized recording's name)
    error: Option<RecorderError>,                    // populated on ok=false
}

#[serde(tag = "kind", rename_all = "snake_case")]
enum RecorderError {
    AlreadyExists { name: String },                  // start with conflicting name
    NotFound { name: String },                       // stop/flush of unknown session
    WrongMode { name: String, expected: String, actual: String },  // flush on a streaming session
    SchemaUnavailable { topic: String, after_ms: u64 },  // §3.3.4
    DiskQuotaExceeded { used_bytes: u64, limit_bytes: u64 },
    Internal { detail: String },                     // anything else; full chain logged
}
```

All errors propagate verbatim back through `storage::sync` to the CLI/MCP caller — agents see the same typed error structure, never a stringly-typed wrap.

#### 3.3.3 Session state machine

Each session is one of three states; transitions are atomic (held under one `Mutex<HashMap<String, Session>>` write):

```
                    start                          stop
                  ────────►  Streaming  ────────────────────►  (removed)
                                  ▲
                                  │  flush (rejected: WrongMode)
                                  │
                                  │
                    start                          stop
                  ────────►  RingBuffer  ────────────────────►  (removed)
                                  ▲ │
                                  │ │  flush
                                  └─┘  (seals chunks, starts fresh window)

                    (any session)         start with same name
                                  ──────────────────────────►  AlreadyExists
```

**Invariants:**
- A session is never observable in a half-constructed state. The session struct, MCAP writer, and topic subscriptions are all created in `start`'s critical section; subscribers don't start consuming until the session is inserted into the map.
- `stop` is graceful: drains the per-session sample channel (bounded), finalizes the active chunk, writes the manifest, then removes the session. Hard-kill semantics live in process shutdown, not `stop`.
- `flush` on a streaming session returns `WrongMode` — flush is meaningless without a window.
- Naming collisions are detected on `start` before any I/O. `AlreadyExists` is returned synchronously; the caller (with auto-generated names) sees this as a bug in name generation, not a race.

#### 3.3.4 Schema fetch ordering

The recorder needs each subscribed topic's protobuf schema (FileDescriptorSet) at session start so MCAP files render in Foxglove without external schema lookup.

**Sequence on `start`:**
1. Resolve final topic list (caller has already done regex expansion).
2. For each topic, derive the publishing node's instance from the topic key (`bubbaloop/global/{machine}/{instance}/{suffix}` → `{machine}/{instance}/schema`).
3. **Parallel `bubbaloop/**/schema` query** (with `QueryTarget::BestMatching`, 2s timeout — matches Python SDK convention).
4. Build per-topic schema cache in memory (`HashMap<topic, FileDescriptorProto>`).
5. Only after all schemas resolve, install subscriptions and write MCAP `Schema` + `Channel` records.
6. Topics whose schema query times out → return `SchemaUnavailable { topic, after_ms: 2000 }`. Caller decides: drop the topic or fail the start. Default policy: **fail the start** (loud failure beats silent missing channels). Override with `--allow-missing-schemas` (CLI) / `allow_missing_schemas: true` (MCP `start` param), which falls back to `message_encoding: "raw"` for that channel and logs `warn!`.

**New topics appearing mid-session:** ignored. The session's subscription set is fixed at start. A topic that starts publishing after `record start` is **not** captured even if it matches the original patterns. This matches rosbag2's behavior and avoids "what if the schema changed?" semantics. Users who want late-binding capture restart the session.

**Schema drift mid-session:** if a publisher's schema queryable starts returning a different FileDescriptorSet during a session, the recorder does **not** rewrite the MCAP `Schema` record. Bytes are still captured. A `warn!` is logged with the topic and old/new schema hashes. This is acceptable because (a) drift mid-recording is itself a bug worth investigating, (b) Foxglove tolerates schema mismatch by showing raw bytes.

#### 3.3.5 Ring-buffer memory model

Ring-buffer mode is the v1 differentiator and the most technically subtle piece. Specification:

- **In-memory ring** per session: bounded by `ring_max_bytes = 256 MB` default (configurable in profile / `record start` param). Holds the most recent samples in arrival order. Older samples are evicted FIFO until total bytes ≤ `ring_max_bytes`.
- **No disk spillover in v1.** The earlier draft mentioned "memory + disk spillover" — dropped. Spillover would mean writing samples to disk, then potentially rewriting on `flush` — doubles disk I/O and complicates retention. v1 keeps the ring purely in RAM; users who want a long window pre-trigger size their RAM budget accordingly. Documented in `docs/concepts/storage-internals.md` with a sizing table (e.g. 1 Mbps × 60s = ~7.5 MB; 100 Mbps × 60s = ~750 MB).
- **`flush` semantics:** atomically (a) snapshot the in-memory ring's current contents, (b) write them to a brand-new finalized recording directory under `~/.bubbaloop/recordings/{name}-flush-{ISO}/` with its own `manifest.json`, (c) reset the ring's window-start anchor (but keep retained samples — they may belong to the next window too if window > flush cadence). The sealed recording's name is returned to the caller for immediate `upload` / `info`.
- **Window vs. byte cap:** both apply. The ring evicts when *either* (samples older than `window_secs`) *or* (total bytes > `ring_max_bytes`) is exceeded. The byte cap is the OOM safety net.
- **Concurrent `flush` calls** for the same session are serialized via the session's mutex — second call blocks until first completes. Different sessions flush concurrently.

#### 3.3.6 Chunk filenames and integrity

- Writes chunks under `~/.bubbaloop/recordings/{name}/chunks/`.
- **Chunk filename format:** `chunk-{idx:06d}-{sha256_prefix8}.mcap` — deterministic key with embedded integrity hash prefix.
- Uses existing `ChunkedMcapWriter` from `bubbaloop-nodes-internal/recorder/src/mcap_writer.rs` (`.mcap.active` → `.mcap` on finalize). Proven code, just relocated, extended to:
  1. Compute SHA-256 streaming during chunk write (no second pass).
  2. Embed the first 8 hex chars in the filename at finalize-time rename.
  3. Append the chunk entry to `manifest.json` (with full 64-char hash, size, log_time bounds) atomically — manifest is written via `write_tmp_then_rename` so partial-write crashes leave the previous manifest intact.

#### 3.3.7 MCAP wire defaults

- **Defaults match rosbag2 verbatim:** `chunk_size: 786_432`, `compression: zstd`, `compression_level: fast`, `chunk_crc: true`. Years of ROS 2 validation.
- **Dual timestamps:** every message preserves both `publish_time` (from the Zenoh sample / message header) AND `log_time` (recorder local clock at receipt). MCAP supports both natively.
- **Channel metadata** per §4.5: `zenoh.encoding`, `zenoh.topic`, optional `bubbaloop.schema_name`.

#### 3.3.8 Status queryable

```
bubbaloop/global/{machine_id}/{instance_name}/status
```

CBOR reply:

```rust
struct RecorderStatus {
    recorder_version: String,
    uptime_secs: u64,
    disk_used_bytes: u64,
    disk_quota_bytes: u64,
    sessions: Vec<SessionState>,
}

struct SessionState {
    name: String,
    machine_id: String,
    mode: SessionMode,                   // Streaming | RingBuffer { window_secs, ring_used_bytes, ring_max_bytes }
    started_at_ns: u64,
    duration_target_secs: Option<u64>,
    bytes_written: u64,
    messages_recorded: u64,
    chunks_finalized: u32,
    topics: Vec<String>,                 // resolved at start
    last_sample_at_ns: Option<u64>,      // for liveness — None means subscribed but no samples yet
    profile_name: Option<String>,
}
```

Backs both `storage_record_sessions` MCP tool and the dashboard's live progress bars. Queried at most 1 Hz from the dashboard (debounced); agents pull on demand.

#### 3.3.9 Crash resilience

If the recorder process dies (OOM, SIGKILL, segfault) mid-session:

- The active `chunk.mcap.active` file is incomplete (no MCAP footer). `ChunkedMcapWriter` detects this on next startup by scanning the recordings directory for `*.mcap.active` files and **discards** them (no salvage attempt — partial chunks lack the index and would corrupt downstream replay). The discard is logged `warn!`.
- Previously finalized chunks (and their manifest entries) are intact.
- The session itself is **not** auto-resumed. The recorder starts up idle; users restart the session if needed. Auto-resume would require persisting session config to disk, which adds complexity not justified by current pain points.
- The `manifest.json` is "open" (no `ended_at_ns`). `storage list` detects this state and shows the recording as `interrupted`. A separate `storage repair <name>` verb is deferred to Phase 2; for now users either `rm` the partial recording or accept the missing trailing chunks.

This crash model is deliberately simple: **no journal, no resume, no recovery dance.** The cost of complexity in this layer exceeds the cost of restarting a recording.

### 3.4 Sync algorithm (`storage::sync`)

The sync subsystem replaces the standalone `uploader/` node. It runs as a single background task inside the daemon (started by `daemon/mod.rs::start_storage_sync()` if `[storage].auto_upload = true`).

#### 3.4.1 Trigger model

Sync is **chunk-finalize-driven**, not poll-driven. Three trigger sources:

1. **Chunk finalize event** — when `ChunkedMcapWriter` renames `.mcap.active` → `.mcap`, it emits a `ChunkFinalized { recording_name, chunk_idx, path, sha256 }` message on an in-process MPSC channel that `storage::sync` consumes. Zero-latency upload start; no FS polling.
2. **Manifest finalize event** — when `record stop` writes `ended_at_ns` to manifest.json, sync re-uploads the manifest one last time (manifests are uploaded after every chunk for crash-resilient remote state; see §3.4.4).
3. **Startup scan** — on daemon startup, sync walks `~/.bubbaloop/recordings/*/manifest.json` and enqueues any chunk where `uploaded_at_ns IS NULL`. This catches anything the daemon missed while down. It's a safety net, not the primary mechanism.

#### 3.4.2 Queue and concurrency

```rust
struct SyncQueue {
    pending: Arc<Mutex<VecDeque<UploadJob>>>,  // bounded, defaults to 1000 jobs
    in_flight: Arc<Semaphore>,                  // max_concurrent_uploads, default 3
    backoff: Arc<RwLock<BackoffState>>,         // shared across all jobs after a failure
}

struct UploadJob {
    recording_name: String,
    chunk_idx: u32,
    local_path: PathBuf,
    sha256: [u8; 32],
    object_key: String,
    enqueued_at_ns: u64,
    attempt: u32,
}
```

- **Concurrency cap** = 3 parallel uploads (configurable per-profile via `sending_queue.queue_size` is **batches in flight**, but the spec collapses that to "concurrent uploads" since each upload is one chunk in v1). At 768 KiB chunks and a typical 10 MB/s uplink, three parallel uploads saturate the link without thrashing.
- **Queue cap** = 1000 jobs. If the queue is full, new finalize events block the recorder's finalize path for up to 5s, then return `BackpressureSyncQueue` (rare — only happens with sustained sync outage). This backpressure choice means a stuck R2 eventually stops recording rather than silently filling disk.
- **No file-backed queue in v1.** The profile YAML mentions `sending_queue.storage_dir` for forward compat with OTel semantics, but v1 reconstructs the queue from manifest scan on startup (see §3.4.1 trigger 3). A file-backed queue would need its own corruption story; the manifest-scan approach is simpler and equally crash-resilient. Documented in `docs/concepts/storage-internals.md`.

#### 3.4.3 Retry / backoff schedule

Backoff is **shared across all jobs**, not per-job. Rationale: if one upload fails because R2 is down, all subsequent uploads will fail too; per-job backoff would burn the queue trying jobs that have no chance. Shared backoff makes the queue pause as a unit.

```
attempt 1: immediate
attempt 2: 5s     (initial_interval)
attempt 3: 10s
attempt 4: 20s
attempt 5: 40s
attempt 6: 60s    (max_interval clamp)
attempt 7+: 60s repeated
```

- Driven by profile YAML's `retry_on_failure.initial_interval` (5s default), `max_interval` (60s default), `max_elapsed_time` (5m default). After `max_elapsed_time`, the job goes to a **dead-letter list** in `~/.bubbaloop/storage/dead_letter.json` and is logged `error!`. `storage reconcile` picks it back up.
- **Backoff reset** happens on the first successful upload after a failure (any job's success resets the shared backoff to attempt 1 for everyone).
- **Retryable vs terminal errors:**
  - Retryable: network timeouts, 5xx responses, R2 throttling (429), DNS failures.
  - Terminal: `Forbidden` (auth bad — go to dead-letter immediately, don't retry forever on credentials that won't change), `BadDigest` (local file corruption — go to dead-letter, requires `reconcile`), `NoSuchBucket` (config wrong — daemon emits a high-severity log and pauses sync entirely until reconfigured).

#### 3.4.4 Upload sequence per chunk

```
1. Acquire sync_queue.in_flight permit (semaphore wait)
2. SHA-256 of local file MUST match manifest.sha256 (re-verified, not trusted)
   → mismatch = local corruption, mark BadDigest, dead-letter, log error!
3. HEAD object at deterministic key
   → 200 + matching SHA-256 → already uploaded, update manifest with uploaded_at_ns from response, done
   → 200 + mismatch → log warn! (someone else uploaded a different chunk to our key — possible bucket collision), dead-letter for human review
   → 404 → proceed to PUT
4. PUT object with x-amz-checksum-sha256: base64(sha256)
   → 200 → update manifest.chunks[i].uploaded_at_ns + remote_etag, write_tmp_then_rename
   → 400 BadDigest → local corruption, dead-letter
   → 403 → terminal, dead-letter, pause sync
   → 5xx / network → retry with shared backoff
5. Upload updated manifest.json (one PUT per chunk-completion event; manifest is small)
6. (optional) If retention_days_local elapsed AND all chunks uploaded → local cleanup task
```

**Manifest re-uploads** are deduplicated by a 500 ms coalescing window: bursts of chunk-finalizes (10 chunks in 100 ms) produce one manifest re-upload, not 10. The manifest is the same regardless of which chunk triggered the re-upload, so this is purely an optimization.

#### 3.4.5 Local manifest mutation during upload

The recorder writes `manifest.json`. Sync also writes `manifest.json` (to record `uploaded_at_ns`). Both writes happen on the daemon process so there's no cross-process race, but they must be serialized.

**Locking:** `storage::manifest` exposes `ManifestHandle` with an internal `tokio::sync::Mutex`. Both the recorder's "append chunk entry" path and sync's "mark chunk uploaded" path acquire the same handle. Lock granularity is per-recording (one handle per recording name).

**Atomicity:** every manifest write is `write_tmp_then_rename` on a sibling `manifest.json.tmp` file. fsync the tmp, then `rename` it over the target. Crash mid-write leaves either the old or the new manifest, never a half-written one.

#### 3.4.6 R2 outage handling

- **Brief outage** (<5m): jobs queue, shared backoff stretches retries up to 60s, queue drains when R2 recovers. No user-visible impact unless queue fills (then recording backpressures per §3.4.2).
- **Sustained outage** (>5m, max_elapsed_time exceeded): individual jobs hit dead-letter. Recording continues, chunks accumulate locally. `storage list` shows them as `local` (uploaded_chunk_count < chunk_count). Dashboard surfaces the dead-letter count as a banner.
- **Recovery:** when R2 comes back, the next manual `storage upload <name>` OR `storage reconcile <name>` flushes the dead-letter list. Auto-retry of dead-letter is **not** automatic — terminal failures got there for a reason; surfacing them to a human is intentional.
- **Daemon restart during outage:** dead-letter list persists in `~/.bubbaloop/storage/dead_letter.json` (JSON array, `write_tmp_then_rename`); manifest scan on startup re-enqueues `uploaded_at_ns IS NULL` chunks; the union of those two restores full queue state.

### 3.5 Reconcile algorithm (`storage::reconcile`)

Reconcile is the user-facing healing primitive. `storage reconcile <name>` diffs three sources of truth (local manifest, local chunk files, R2 objects) and converges them.

#### 3.5.1 Inputs

For recording `<name>`:
1. **Local manifest** at `~/.bubbaloop/recordings/{name}/manifest.json` — declares which chunks should exist + their SHA-256.
2. **Local chunk files** at `~/.bubbaloop/recordings/{name}/chunks/*.mcap` — what's actually on disk.
3. **R2 listing** via `ListObjectsV2` with `prefix={machine_id}/{name}/` — what's actually in R2.

#### 3.5.2 Diff matrix

For each chunk index 0..N in the manifest:

| Manifest | Local file | R2 object | SHA-256 match | Action |
|---|---|---|---|---|
| ✓ | ✓ | ✓ | all match | mark `uploaded_at_ns` from R2 if manifest had null; no-op otherwise |
| ✓ | ✓ | ✗ | local matches manifest | **upload** with checksum header; update manifest |
| ✓ | ✓ | ✓ | R2 mismatches | R2 has a stale/different chunk for our key — **re-upload** local (deterministic key means overwrite is safe); log warn! |
| ✓ | ✓ | ✓ | local mismatches manifest | **local file corruption** — error, do not upload, surface for human review |
| ✓ | ✗ | ✓ | R2 matches manifest | local was deleted (retention sweep) — **redownload** if `--restore` flag, else mark and continue |
| ✓ | ✗ | ✗ | n/a | chunk is gone in both places — manifest is broken, mark recording `corrupt`, refuse to upload partial |
| ✗ | ✓ | ? | n/a | **orphan local file** — log warn! with hint, do not touch (might be from an interrupted session); user runs `storage rm-orphans <name>` if intentional |
| ✗ | ✗ | ✓ | n/a | **orphan R2 object** — log warn! with the object key; future `storage gc --remote` will clean these; do not auto-delete (might belong to a different session that just lost its manifest) |

#### 3.5.3 Execution order

1. Read local manifest. Refuse to reconcile if manifest doesn't exist (no recovery here — that's a `storage repair` job, deferred).
2. Walk local chunks directory, build `HashMap<chunk_idx, path>`.
3. `ListObjectsV2` with the recording prefix, build `HashMap<chunk_idx, ObjectMeta>` (filename embeds idx and sha prefix per §3.3.6).
4. For each chunk in manifest, apply diff matrix row.
5. Single pass — no multi-round convergence needed because reconcile holds an exclusive lock on the recording (no concurrent sync).
6. Re-upload the manifest.json itself at the end so the post-reconcile state is captured remotely.
7. Return `ReconcileReport { chunks_diffed, chunks_uploaded, chunks_redownloaded, chunks_verified_ok, orphans_local, orphans_remote, errors }`.

**Locking:** acquires the same `ManifestHandle` mutex as the recorder + sync (§3.4.5). Concurrent `record start`/`record stop` against the same recording block until reconcile completes. Active recording sessions are blocked from reconciliation entirely (returns `RecordingActive { name }`) — only stop-and-reconcile is supported in v1; live reconcile against an active recording is deferred.

#### 3.5.4 Reconcile-driven cleanup is opt-in

Reconcile **never deletes** anything (local or remote) in v1. It only:
- Uploads missing chunks
- Re-uploads mismatched chunks
- Re-downloads chunks if `--restore` flag is passed
- Reports orphans

Deletion is a separate concern handled by:
- `storage rm <name>` — explicit user delete
- `storage rm <name> --remote` — explicit remote delete (admin tier)
- Auto-cleanup of uploaded local recordings driven by `retention_days_local` (already part of sync, not reconcile)
- Future `storage gc --remote` for orphan remote objects (deferred)

This split keeps reconcile **idempotent and safe to re-run** — running it five times in a row produces the same result as running it once, never destroys data.

---

## 4. Data model

### 4.1 `~/.bubbaloop/config.toml`

```toml
[storage]
backend = "r2"                                    # r2 | s3 | gcs | minio | local
endpoint = "https://{account}.r2.cloudflarestorage.com"
region = "auto"
bucket = "kornia-recordings"                      # bucket = sharing unit
auto_upload = true                                 # background sync of finalized chunks
retention_days_local = 7                           # auto-delete local after successful upload + N days
disk_quota_gb = 50                                 # soft cap on local recordings dir
recorder_instance = "mcap_recorder"                # which recorder instance to command (if multiple)

[storage.defaults]
chunk_size_bytes = 786_432                         # rosbag2 default
compression = "zstd"
compression_level = "fast"
chunk_crc = true
```

### 4.2 `~/.bubbaloop/secrets.toml` (chmod 0600)

```toml
[storage.r2]
access_key_id = "..."
secret_access_key = "..."
```

Secrets never appear in `config.toml`, `node.yaml`, profile YAML, or logs. `storage::secrets` returns opaque `Secret<String>` wrappers (zeroize-on-drop, `Debug` redacted). Clippy lint enforces no `format!("{}", secret)`.

### 4.3 Profile (`~/.bubbaloop/profiles/{name}.yaml`)

Field names borrow from OpenTelemetry Collector for forward compatibility with v2 multi-pipeline mode. Top-level `pipelines:` key is **reserved** — v1 profiles are implicitly a single pipeline.

```yaml
name: demo
description: "Camera + IMU for outdoor demos"

# Topic selection — additive composition (fixes rosbag2's mutual-exclusion friction)
topics:                                         # additive: include these literal patterns
  - 'bubbaloop/global/*/cam_*/compressed'
regex: '^bubbaloop/global/.+/imu/.+'            # additive: include matching regex
exclude:                                        # subtractive: remove these from the union
  - '**/health'
  - '**/manifest'
include_local: false                            # default: exclude bubbaloop/local/** (SHM-only)

# MCAP writer config
chunk_size_bytes: 786_432
compression: zstd
compression_level: fast
chunk_crc: true

# Session config
mode: streaming                                  # streaming | ring_buffer
window_secs: null                                # required if mode=ring_buffer
trigger: null                                    # required if mode=ring_buffer: on-event | manual

# Upload behavior — OTel-style names
sending_queue:
  enabled: true
  queue_size: 1000                              # batches in flight before backpressure
  storage_dir: ~/.bubbaloop/storage/queue       # file-backed for crash resilience

retry_on_failure:
  enabled: true
  initial_interval: 5s
  max_interval: 60s
  max_elapsed_time: 5m

# Reserved for v2 (must not appear in v1 profiles):
# pipelines:
#   - name: pipeline-a
#     topics: [...]
#     exporters: [...]
```

Profiles are tiny YAML files. Teams commit them to a repo (or share via marketplace later). New teammates `git clone` and instantly have "the record button" the team uses.

#### 4.3.1 Validator rules (v1)

The profile loader (`storage::profile::load`) **rejects** any of:
- Unknown top-level keys EXCEPT `pipelines:` (which is reserved and triggers `ProfileRequiresV2 { found_pipelines: true }` — see §4.3.2). Other unknowns are typos and should fail loud, not be silently ignored.
- Unknown nested keys under `sending_queue:` or `retry_on_failure:`. Same reasoning.
- `mode: ring_buffer` without `window_secs` set → `MissingField { field: "window_secs", required_when: "mode=ring_buffer" }`.
- `mode: ring_buffer` without `trigger` set → `MissingField { field: "trigger", required_when: "mode=ring_buffer" }`.
- `trigger: on-event` in v1 (the trigger pipeline is Phase 3; v1 only accepts `manual`) → `NotYetImplemented { feature: "on-event trigger", available_in: "Phase 3" }`.
- Empty `topics: []` AND empty `regex: null` AND no implicit `--all` → `EmptySelection`. Profiles without a selector are useless and should fail at load time, not at `record start`.
- Conflicting selection: `include_local: true` without any pattern matching `bubbaloop/local/**` → warning, not error (user might want local + global wildcards).
- `chunk_size_bytes` outside [4 KiB, 16 MiB] range → `OutOfRange`. The rosbag2 default (768 KiB) is well-validated; extreme values produce broken MCAP files.

#### 4.3.2 v1 → v2 migration path

v2 adds **multi-pipeline** mode: one profile can declare multiple independent record-then-export pipelines (e.g., "high-rate cameras → R2 bucket A" + "low-rate telemetry → R2 bucket B + local archive"). The reserved `pipelines:` key is the entry point.

**v1 profile** (implicit single pipeline):
```yaml
name: demo
topics: ['bubbaloop/global/*/cam_*/compressed']
chunk_size_bytes: 786_432
sending_queue: { enabled: true, queue_size: 1000 }
```

**v2 profile** (explicit multi-pipeline):
```yaml
schema_version: 2
name: demo
pipelines:
  - id: cameras
    topics: ['bubbaloop/global/*/cam_*/compressed']
    chunk_size_bytes: 786_432
    sending_queue: { enabled: true, queue_size: 1000 }
    exporters: [r2_primary]
  - id: telemetry
    topics: ['bubbaloop/global/*/imu/*']
    chunk_size_bytes: 65_536
    exporters: [r2_primary, local_archive]
exporters:
  r2_primary: { backend: r2, bucket: kornia-recordings }
  local_archive: { backend: local, path: /data/archive }
```

**Migration is one-shot, not auto-bridging:**
- v1 profiles continue to work in v2 — the loader detects `pipelines:` absent and wraps the top-level into an implicit single pipeline `{id: "default"}` with the global `[storage]` backend as the implicit exporter.
- v2 profiles do **not** work in v1 — the loader detects `pipelines:` present and returns `ProfileRequiresV2` (user upgrade message says: "this profile requires bubbaloop ≥0.X.0").
- v1 users opting in to v2 features run `bubbaloop storage profile migrate <name>` — a one-shot transform that writes the v2-shaped YAML alongside the original (`{name}-v2.yaml`) for the user to review-and-replace. No in-place rewrite (preserves user's git history of the profile).
- The v2 spec is its own document (separate from this one); reserving the `pipelines:` key in v1 is the only thing v1 owes v2.

#### 4.3.3 `profile_sha256` stability

`profile_sha256` (stored in manifest for provenance) is computed over the **parsed-and-canonicalized** profile, not the raw bytes. The canonicalization:
1. Parse YAML to `serde_yaml::Value`.
2. Recursively sort map keys alphabetically.
3. Normalize integer/float representations (e.g., `786_432` and `786432` both become `786432`).
4. Strip comments and trailing whitespace.
5. Re-serialize to JSON (deterministic).
6. SHA-256 over the JSON bytes.

This means whitespace-only edits, comment changes, key reorderings, and `786_432` vs `786432` formatting do **not** change `profile_sha256`. Semantic edits (changing a topic pattern, flipping a boolean, adding a key) do change it. The manifest's `profile_sha256` is durable across cosmetic profile rewrites — recordings made with "the same profile" can be grouped by hash even after the team prettified the YAML.

### 4.4 Recording manifest (`manifest.json` at recording root — source of truth)

```json
{
  "schema_version": 1,
  "name": "outdoor_test_3",
  "machine_id": "jetson_alpha",
  "fleet_router": "zenoh://10.0.0.1:7447",
  "started_at_ns": 1748275200000000000,
  "ended_at_ns": 1748275300000000000,
  "duration_ns": 100000000000,
  "size_bytes": 4567890123,
  "mode": "streaming",
  "window_secs": null,
  "trigger": null,
  "selection": {
    "topics": ["bubbaloop/global/*/cam_*/compressed"],
    "regex": null,
    "exclude": ["**/health"],
    "include_local": false
  },
  "channels": [
    {
      "topic": "bubbaloop/global/jetson_alpha/cam_front/compressed",
      "channel_id": 0,
      "message_count": 12345,
      "message_encoding": "cbor",
      "zenoh_encoding": "application/cbor",
      "schema_name": "bubbaloop.camera.v1.CompressedImage",
      "publish_time_first_ns": 1748275200123000000,
      "publish_time_last_ns": 1748275299987000000
    }
  ],
  "chunks": [
    {
      "name": "chunk-000000-a1b2c3d4.mcap",
      "index": 0,
      "size_bytes": 786432,
      "sha256": "a1b2c3d4e5f6a7b8c9d0e1f2a3b4c5d6e7f8a9b0c1d2e3f4a5b6c7d8e9f0a1b2",
      "log_time_first_ns": 1748275200000000000,
      "log_time_last_ns": 1748275260000000000,
      "uploaded_at_ns": 1748275305000000000,
      "remote_etag": "\"a1b2c3d4e5f6...\""
    }
  ],
  "profile_name": "demo",
  "profile_sha256": "abc123def456...",
  "recorder_version": "0.1.0",
  "tags": []
}
```

**Sync status is per-chunk** (`uploaded_at_ns` is `null` if not yet uploaded). The recording is fully uploaded when every chunk has `uploaded_at_ns`. No separate "status" enum at the top level — derive from chunks. This makes partial uploads visible and reconcilable without ambiguity.

**Lifecycle status is derived from `ended_at_ns`:**
- `ended_at_ns` set + `duration_ns` set → `complete` (normal stop or duration-triggered)
- `ended_at_ns` null + recorder still running session with same name → `recording` (live session)
- `ended_at_ns` null + recorder *not* running that session → `interrupted` (crash per §3.3.9 — `storage list` shows this state; `storage rm` removes it; future `storage repair` may salvage)

`storage list` derives all three states client-side from the manifest + the recorder's `status` queryable. No persisted state flag needed.

### 4.5 MCAP channel metadata (replay fidelity)

Each MCAP channel record carries a `metadata` map. We store the original Zenoh encoding so replay is byte-identical:

```
channel.metadata = {
  "zenoh.encoding": "application/cbor",         // or application/protobuf, image/jpeg, etc.
  "zenoh.topic": "bubbaloop/global/jetson_alpha/cam_front/compressed",
  "bubbaloop.schema_name": "bubbaloop.camera.v1.CompressedImage"   // optional
}
```

MCAP `message_encoding` is set to `"cbor"` / `"protobuf"` / `"raw"` to match what Foxglove Studio expects. `zenoh.encoding` is the Bubbaloop-specific extension used by `replay`.

Each MCAP `MessageHeader` preserves **both** `publish_time` (from the Zenoh sample timestamp / message header) and `log_time` (recorder local clock at receipt). MCAP supports both fields natively. Replay defaults to `publish_time` for fidelity; users can `--use-log-time` if they need recorder-local ordering for debugging.

---

## 5. CLI surface

```
bubbaloop storage configure                       # interactive wizard
bubbaloop storage topics [--machine X] [--grep PATTERN]
                                                  # list live topics via manifest queryable

bubbaloop storage profile create <name> [...]
bubbaloop storage profile list
bubbaloop storage profile show <name>
bubbaloop storage profile rm <name>

# Topic selection composes additively (fixes rosbag2 friction)
bubbaloop storage record start
  [--topics PAT[,PAT...]]                         # additive includes
  [--regex REGEX]                                 # additive regex include
  [--exclude PAT[,PAT...]]                        # subtractive
  [--all]                                         # all bubbaloop/global/** (default if no other selector)
  [--include-local]                               # also include bubbaloop/local/**
  [--profile NAME]                                # load defaults from profile, CLI flags override
  [--name NAME]                                   # session name (auto-generated if omitted)
  [--duration DURATION]                           # auto-stop after this long
  [--window DURATION]                             # ring-buffer: keep sliding window
  [--trigger on-event|manual]                     # required with --window

bubbaloop storage record stop [--name NAME]       # finalize all sessions or just one
bubbaloop storage record flush [--name NAME]      # ring-buffer mode: seal current window
                                                  #   as a new finalized recording, keep going

bubbaloop storage list [--remote] [--machine X] [--tag X] [--since DATE]
bubbaloop storage info <name>                     # dumps manifest.json (pretty-printed)
bubbaloop storage upload <name>                   # to R2 (operational/debug); idempotent — skips
                                                  #   chunks already uploaded by sha256 match
bubbaloop storage download <name> [--to PATH]     # pulls + verifies sha256 per chunk
bubbaloop storage replay <name>
  [--rate N] [--loop]
  [--start-time T] [--end-time T]                 # ros2 bag-style
  [--topics PAT] [--exclude PAT]                  # filter what's republished
  [--remap from:=to]                              # topic remapping
  [--use-log-time]                                # default: publish_time

bubbaloop storage reconcile <name>                # diff local manifest vs R2 listing,
                                                  #   re-upload missing chunks, verify sha256
  [--restore]                                     # also re-download chunks present in R2 but
                                                  #   missing locally (e.g., after retention sweep)
bubbaloop storage rm <name> [--remote]

# RESERVED — not implemented in v1, do not squat:
# bubbaloop storage publish <name> --target hf://org/dataset      (Phase 3: HF Hub)
# bubbaloop storage import <file.mcap>                            (deferred)
```

**Smart defaults:**

- `record start` with no selector → `--all` (all `bubbaloop/global/**`), default-excludes `**/health` and `**/manifest`.
- `--include-local` is **opt-in** because `bubbaloop/local/**` is SHM-only and recording it implies cross-process copy that hurts performance.
- Auto-generated name: `rec_2026-05-26T12:34:56_jetson_alpha`.
- `--duration 5m` → auto-stop after duration. Without it, runs until explicit `record stop`.
- `--window 60s --trigger on-event` (Phase 3 integration) → recording sits idle; an external event (agent alert, MCP call) triggers a `flush` to seal the last 60s as a recording.
- `--window 60s --trigger manual` → user calls `record flush` manually whenever they want to capture the last 60s.

**Replay** mirrors `ros2 bag play`: `--rate`, `--loop`, `--start-time`/`--end-time` for trim, `--topics`/`--exclude` to filter what's republished, `--remap from:=to` for topic remapping. Default = original rate, no loop, full file, publish_time-ordered.

---

## 6. MCP surface

Tools added in `crates/bubbaloop/src/mcp/storage_tools.rs`.

### 6.1 Completeness invariant (load-bearing rule)

> **Every CLI verb in §5 MUST have at least one paired MCP tool.** Agents are first-class operators of this subsystem — anything a human can do at the terminal, an agent can do through MCP. New CLI verbs added in future PRs MUST ship with the matching MCP tool in the same diff; CI enforces this with a smoke test that loads the CLI subcommand tree and asserts each leaf has a registered MCP handler whose name matches `storage_{verb_path}`.

Some MCP tools have **no CLI equivalent** — they exist because agents need state inspection that humans get from process introspection (e.g., enumerating active recording sessions). These are flagged "(MCP-only)" in the table.

### 6.2 CLI ↔ MCP mapping (authoritative)

| CLI verb | MCP tool | RBAC tier | Notes |
|---|---|---|---|
| `storage configure` | `storage_configure(backend, endpoint?, region?, bucket, access_key_id, secret_access_key, auto_upload?, retention_days_local?, disk_quota_gb?)` | **admin** | MCP variant is non-interactive — takes all params explicitly. Secrets written to `~/.bubbaloop/secrets.toml` (chmod 0600). The interactive CLI wizard collects the same fields and calls the same handler. |
| — (MCP-only) | `storage_config_get()` | viewer | Returns current `[storage]` config block (secrets redacted). Lets agents check what's configured before suggesting changes. No CLI equivalent — humans `cat ~/.bubbaloop/config.toml`. |
| `storage topics` | `storage_topics_list(machine?, grep?)` | viewer | Queries `bubbaloop/**/manifest` and groups by `machine_id → node_instance → topic`. Includes liveness (`ever_fired`, `still_live`) per topic. |
| `storage profile create` | `storage_profile_create(name, description?, topics?, regex?, exclude?, include_local?, chunk_size_bytes?, compression?, compression_level?, chunk_crc?, mode?, window_secs?, trigger?, sending_queue?, retry_on_failure?)` | operator | Writes `~/.bubbaloop/profiles/{name}.yaml`. Validates the YAML against the v1 schema before saving. |
| `storage profile list` | `storage_profile_list()` | viewer | Returns array of `{name, description, modified_at_ns, profile_sha256}`. |
| `storage profile show` | `storage_profile_show(name)` | viewer | Returns full profile YAML (parsed to JSON for MCP transport). |
| `storage profile rm` | `storage_profile_rm(name)` | operator | Deletes `~/.bubbaloop/profiles/{name}.yaml`. Refuses if any active session references the profile. |
| `storage record start` | `storage_record_start(topics?, regex?, exclude?, all?, include_local?, profile?, name?, duration?, window?, trigger?)` | operator | Returns `{session_name, recorder_machine, started_at_ns, mode}`. If `name` omitted, returns the auto-generated `rec_{ISO}_{machine_id}`. Fails fast if name collides with an active session. |
| `storage record stop` | `storage_record_stop(name?)` | operator | Returns finalized chunk count + total bytes + manifest path. If `name` omitted, stops all sessions and returns an array. |
| `storage record flush` | `storage_record_flush(name?)` | operator | Ring-buffer only. Seals current window as a finalized recording, starts a fresh window. Returns sealed recording's name + manifest path. |
| — (MCP-only) | `storage_record_sessions(machine?)` | viewer | Lists **active** recording sessions across the fleet. Returns `[{session_name, machine_id, mode, started_at_ns, bytes, messages, chunks_finalized, window_secs?, profile?}]`. Backed by the recorder's `{instance}/status` queryable. Critical for agents — without it, agents can't see what's already recording before calling `record start`. CLI equivalent is "look at dashboard" or `bubbaloop dataflow` — humans have visual context, agents don't. |
| `storage list` | `storage_list(remote?, machine?, tag?, since?, limit?)` | viewer | Returns array of recording summaries (`name, machine_id, duration_ns, size_bytes, chunk_count, uploaded_chunk_count, started_at_ns, tags`). `limit` is MCP-only (default 100) to bound token use. |
| `storage info` | `storage_info(name)` | viewer | Returns full parsed `manifest.json`. Per-chunk upload status included. |
| `storage upload` | `storage_upload(name)` | operator | Idempotent. Returns `{chunks_uploaded, chunks_skipped, bytes_transferred}`. Streams progress events via MCP notifications if the client supports them; otherwise returns final summary only. |
| `storage download` | `storage_download(name, to?)` | operator | Verifies SHA-256 per chunk on receipt. Refuses to write corrupted files. Returns local path + `{chunks_verified, bytes_received}`. |
| `storage replay` | `storage_replay_start(name, rate?, loop?, start_time?, end_time?, topics?, exclude?, remap?, use_log_time?)` | operator | **Renamed `_start`** because replay is long-running. Returns `{replay_session_id, started_at_ns, estimated_duration_ns}`. |
| — (MCP-only, replay control) | `storage_replay_stop(replay_session_id)` | operator | Stops an active replay started via `storage_replay_start`. Needed because MCP calls don't have a SIGINT handle the way `Ctrl+C` on a CLI does. |
| — (MCP-only, replay control) | `storage_replay_status(replay_session_id?)` | viewer | Returns active replay sessions with progress (`elapsed_ns`, `messages_replayed`, `topics_active`). With no arg, lists all replays. |
| `storage reconcile` | `storage_reconcile(name, restore?)` | operator | Returns `{chunks_diffed, chunks_uploaded, chunks_redownloaded, chunks_verified_ok, orphans_local, orphans_remote, errors}` per §3.5.3. `restore=true` re-downloads chunks present in R2 but missing locally. |
| `storage rm` | `storage_rm(name, remote?)` | **admin** | Remote delete is destructive — admin tier even though local-only `rm` is operator-equivalent. The single tool covers both; RBAC tier is set to the highest action it can perform. |

### 6.3 Reserved / deferred (MUST NOT register in v1)

| Reserved CLI verb | Reserved MCP tool | Status |
|---|---|---|
| `storage publish` | `storage_publish_*` | Phase 3 (HF Hub). **Do not register a stub** — registering an unimplemented MCP tool is worse than absence (agents will try to call it). |
| `storage import` | `storage_import` | Deferred. Same rule — no stub. |

### 6.4 Convention compliance

- All tool handlers include `log::info!("[MCP] tool=storage_..., params=...")` audit lines per project convention (CLAUDE.md MCP section).
- All tools follow `Parameters<T>` with `schemars::JsonSchema` per the rmcp 0.15 pattern used in `mcp/tools.rs`.
- Tier mapping is registered in `mcp/rbac.rs` so unknown-defaults-to-Admin doesn't silently lock out viewers from read-only tools.
- Each tool's handler dispatches through `PlatformOperations` so the integration test suite can swap in `MockPlatform` (per existing `mcp/platform.rs` convention).
- The `MCP-only` tools (`storage_config_get`, `storage_record_sessions`, `storage_replay_stop`, `storage_replay_status`) are documented in `docs/concepts/storage-internals.md` under "Agent operations" so it's clear why they exist without CLI counterparts.

---

## 7. Dashboard surface

New "Recordings" tab in the dashboard (`dashboard/src/views/RecordingsView.tsx`):

```
┌─ Recordings ──────────────────────────────────────────────────────┐
│ [+ New Recording]                              [Filter: all  ▾]   │
│                                                                   │
│  ● outdoor_test_3       100s  4.5GB  jetson_alpha  ✓uploaded  ⬇  │
│  ● indoor_imu_only       30s  120MB  jetson_beta   ⬆ local    ⬆  │
│  ○ daily_health_check    24h  1.2GB  jetson_alpha  ◐ 3/15 chunks │
│                                                                   │
└───────────────────────────────────────────────────────────────────┘
```

### 7.1 Recording row state machine

Each row in the list is in exactly one of these states; transitions are driven by `storage_list` + `storage_record_sessions` polled at 2 Hz (debounced, single combined request):

| State | Visual | Source of truth | Transitions |
|---|---|---|---|
| `recording` | ● (animated dot) + chunk counter ticking | recorder `status` queryable shows session name match | → `stopping` (user clicks stop) |
| `stopping` | spinner over the dot | local: user clicked stop, awaiting recorder ack | → `local` on `record stop` reply |
| `local` | ⬆ (up-arrow icon, dim) | manifest exists, some `uploaded_at_ns` null | → `syncing` when sync picks up chunks; → `uploaded` when all chunks have `uploaded_at_ns` |
| `syncing` | ◐ + `N/M chunks` progress | derived from per-chunk `uploaded_at_ns` count | → `uploaded` on completion; → `degraded` if dead-letter count > 0 |
| `uploaded` | ✓ (green check) | all chunks have `uploaded_at_ns` | terminal until deleted |
| `degraded` | ⚠ (yellow) + dead-letter count | sync dead-letter list contains entries for this recording | → `syncing` on successful retry; sticky otherwise |
| `interrupted` | ⚠ (red) + "crashed" hint | manifest `ended_at_ns` null + recorder not running this session | → `rm` (user deletes) or → (future) `repair` |
| `corrupt` | ⛔ (red) + "manifest broken" | reconcile reported missing chunks in both local and remote | → `rm` only |
| `remote-only` | ☁ (cloud icon) | listed in R2, no local manifest | → `local` after `download`; can `replay` from remote in future |

States are computed client-side, not stored. The dashboard never invents a state the daemon doesn't observe.

### 7.2 Error surfaces

Each error has a dedicated user-facing affordance. **No silent failures.**

| Daemon error | Banner / toast | User action available |
|---|---|---|
| R2 unreachable (5xx, network) | Yellow banner above list: "R2 unreachable — N recordings pending upload" + retry timer | Wait (auto-retry) or click "Pause sync" |
| R2 auth failed (403) | Red banner: "R2 credentials rejected — sync paused" | "Reconfigure" → opens config modal (calls `storage_configure`) |
| Disk quota exceeded | Red banner: "Local disk quota exceeded — recording paused" + N GB used / M GB cap | "Increase quota" (config), "Delete uploaded recordings" (bulk rm), "Force-resume" |
| Recorder crashed | Toast: "Recording '{name}' interrupted at {time}" + row state flips to `interrupted` | "Show details" (open recording row), "Delete" |
| Schema fetch timeout on `record start` | Modal blocks start: "Topics missing schema: A, B, C" + checkbox "Allow missing schemas" | Cancel or proceed with `allow_missing_schemas: true` |
| Reconcile found orphans | Inline row warning: "K local orphans, L remote orphans" | "View report" (opens reconcile report drawer); orphan cleanup is a separate explicit action |
| Dead-letter accumulation | Sticky banner: "N uploads failed permanently — review needed" | "Show dead-letter" (opens drawer with per-chunk reason); "Retry all" / "Discard" |

### 7.3 Permissions (RBAC reflected in UI)

The dashboard reads the operator's tier from `auth/me` (existing endpoint) and **disables** controls the tier can't use:

- **Viewer**: list view, info drawer, download. Start/stop/flush/upload/reconcile/rm buttons hidden or disabled with tooltip "requires operator tier".
- **Operator**: everything viewer has + start/stop/flush/upload/download/reconcile. Remote `rm` button shown but disabled with tooltip "requires admin tier".
- **Admin**: everything, including remote `rm` (with a confirm-the-name-typed modal — same pattern as GitHub repo delete).

The UI never sends a request the tier won't pass; the daemon also enforces (defense in depth — UI may be stale or bypassed).

### 7.4 New Recording modal flow

```
[+ New Recording] → modal opens
  ├─ Topic Picker (live tree, ever_fired/still_live badges, multi-select)
  ├─ Mode selector: Streaming | Ring buffer
  │    └─ Ring buffer: window duration + trigger (manual only in v1)
  ├─ Advanced (collapsed): chunk size, compression, exclude patterns
  ├─ Profile dropdown: "Load profile..." pre-fills above
  ├─ Optional name (auto-generated preview shown)
  ├─ Optional duration (auto-stop)
  └─ [Cancel] [Save as profile...] [Start Recording]
```

- Topic picker reuses the existing `bubbaloop/**/manifest` discovery infrastructure (no new wire format).
- "Save as profile" opens a sub-modal (name + description), calls `storage_profile_create`, then either starts the recording or closes.
- "Start Recording" calls `storage_record_start`. On success, the new recording row appears immediately in `recording` state. On failure, the modal shows the typed error and stays open for retry.

### 7.5 Live progress

For rows in `recording` state, the dashboard shows live counters (bytes, message count, chunks finalized, last-sample age) updated at 2 Hz from `storage_record_sessions`. For rows in `syncing` state, the chunk upload progress (`N/M`) updates whenever `storage_list` reflects a new `uploaded_at_ns`. Both update via the same 2 Hz combined poll — no separate subscription.

### 7.6 Implementation notes

- List view backed by `storage_list` + `storage_record_sessions` MCP calls (local + remote + live sessions in one combined view).
- Topic picker queries `bubbaloop/**/manifest` via the WebSocket bridge — reuses existing infrastructure.
- Download button streams `.mcap` bytes via `storage_download` MCP call piped through the daemon HTTP API.
- **No in-browser MCAP playback in v1.** Downloaded files open in Foxglove Studio / Lichtblick. Browser playback is its own Phase 2 spec.

Old browser-side recorder code (`dashboard/src/recording/` or equivalent) ships in deprecation mode for one release with a banner pointing users to the new tab, then is removed.

---

## 8. Cloudflare R2 / S3-compatible specifics

- Use `aws-sdk-s3 = "1"` (already a dep in the existing `uploader/` node).
- `Config::builder()` with: `endpoint_url(R2_ENDPOINT)`, `region(Region::new("auto"))`, explicit `Credentials::new(key, secret, None, None, "R2")`. **Never** rely on the default AWS credential chain.
- Bucket-style URLs are R2's documented default; do not set `force_path_style(true)` unless a specific bucket pattern requires it.
- **Per-chunk integrity:** every chunk upload passes `x-amz-checksum-sha256` (base64 of the SHA-256 computed during finalize). R2 validates server-side; mismatch returns `BadDigest`. The SHA-256 is also stored in the manifest. `storage download` verifies on receipt and refuses to write a corrupted file.
- **Object key format** (deterministic, idempotent):
  ```
  {bucket}/{machine_id}/{recording_name}/manifest.json
  {bucket}/{machine_id}/{recording_name}/chunks/chunk-{idx:06d}-{sha256_prefix8}.mcap
  ```
  The SHA-256 prefix in the filename means re-uploads are no-ops by HEAD check (same key, same content); changed content → different key → no silent overwrite. Borrowed from Kafka tiered storage segment naming.
- **Upload strategy:** single PUT for chunks <100 MB (most cases — chunk_size_bytes default is 768 KiB so a single chunk is well below). Multipart only if a future feature merges multi-chunk bundles. Avoiding multipart sidesteps the "ghost incomplete uploads" cost problem entirely.
- **Multipart safety (if ever used):** ship an R2 lifecycle rule that aborts incomplete multipart uploads after 24 hours.
- **Reconcile:** `storage reconcile <name>` does:
  1. Read local `manifest.json`.
  2. `ListObjectsV2` with `prefix={machine_id}/{recording_name}/chunks/`.
  3. For each chunk in manifest, HEAD the expected object key; if missing or SHA-256 mismatch, re-upload.
  4. Update `uploaded_at_ns` in manifest on success.
- Listing: `ListObjectsV2` with `prefix={machine_id}/` (per-machine) or no prefix (all). For each `manifest.json` discovered, GET it and aggregate metadata client-side. For large fleets, a v2 optimization would cache manifests in a Worker — not in v1.
- Reads (download) use simple `GetObject` with shared credentials; v1 does **not** issue presigned URLs (Phase 2 when per-machine bearer tokens + Worker are added).

---

## 9. What changes from today

| Today | After this spec |
|---|---|
| `bubbaloop-nodes-internal/recorder/` (out-of-tree, role: processor, flat-tagged commands, GCS-only, no SHA-256) | `bubbaloop-nodes-official/sinks/mcap-recorder/` (marketplace, `role: sink`, `{command, params}` envelope, backend-agnostic, SHA-256 per chunk, rosbag2 defaults, dual timestamp preservation, multi-session + ring-buffer support) |
| `bubbaloop-nodes-internal/uploader/` (separate node, polls dir, S3 PUT) | **Removed.** Functionality moves to `crates/bubbaloop/src/storage/sync.rs` with `x-amz-checksum-sha256`, deterministic keys, idempotent re-uploads. |
| Per-machine `topic_patterns` config | One fleet recorder with default = all-global-topics; composable `--topics + --regex + --exclude` overrides. |
| Browser-aggregates-and-stores recorder | Dashboard "Recordings" tab is a control surface. Browser-side recording deprecated one release, then removed. |
| No team sharing | Bucket = sharing unit. Same bucket → shared recordings. `bubbaloop storage list --remote` is fleet-wide. |
| No CLI verbs | `bubbaloop storage configure/topics/profile/record/list/info/upload/download/replay/reconcile/rm`. |
| No MCP for recording | Full MCP surface so agents can record/replay/upload/reconcile programmatically. |
| `role: sink` taxonomy slot has no occupants | `mcap-recorder` is the first real sink. |
| No integrity story | SHA-256 per chunk: in filename, in manifest, in `x-amz-checksum-sha256` header, verified on download, used by reconcile. |
| No ring-buffer / pre-trigger recording | `--window` + `--trigger` modes. Differentiates Bubbaloop from rosbag2. |

---

## 10. Phasing

### Phase 1 (this spec, MVP)
- `crates/bubbaloop/src/storage/` module — full module tree from §3.2.
- CLI subcommand tree — all 12 verbs from §5.
- MCP tools — full mirror from §6.
- `mcap-recorder` marketplace cleanup: move in-tree-of-nodes-official, fix envelope, `role: sink`, multi-session, ring-buffer, SHA-256, rosbag2 defaults, channel metadata, dual timestamp preservation.
- Dashboard "Recordings" tab (list + start/stop with topic picker + download). Browser recorder deprecation banner.
- `bubbaloop storage configure` interactive wizard.
- Documentation: `docs/concepts/storage.md` (5-command quickstart) + `docs/concepts/storage-internals.md` (manifest schema, integrity, reconcile flow).

### Phase 2 (separate spec)
- In-browser MCAP playback with scrubber, video pipeline, channel timeline, device-row visual borrowed from Foxglove.
- Cloudflare Worker for per-machine bearer tokens + minted presigned PUT/GET URLs (Skydio/Auterion auth pattern).
- Dashboard download via signed URL (browser → R2 direct, bypassing daemon).
- `storage import <file.mcap>` for sideloading external recordings.

### Phase 3 (separate spec)
- Event-triggered auto-recording (hook ring-buffer `--trigger on-event` into agent arousal / alerts).
- `storage publish <name> --target hf://org/dataset` for HF Hub dataset publishing (LeRobot-style).
- Dataset tagging, search, retention tiers.
- Per-machine "edge recorder" mode for disconnected operation; federated topology when centralized hits its limits.

### Explicitly out of scope (likely never)
- Cloudflare Stream integration.
- Bespoke per-fleet auth scheme (Phase 2 Worker is enough for any team that needs it).

---

## 11. Committed decisions (no further questions)

- **Recorder lifecycle:** long-lived idle node, autostart on install, accepts commands. Not spawned per session.
- **Multi-session:** recorder supports N parallel named sessions. `record stop --name foo` finalizes one.
- **Recording naming:** auto-generate `rec_{ISO timestamp}_{machine_id}` if `--name` not given.
- **Replay timing:** default = original rate using `publish_time`; flags = `--rate`, `--loop`, `--start-time`, `--end-time`, `--topics`/`--exclude` filters, `--remap from:=to`, `--use-log-time`.
- **Local retention:** `retention_days_local = 7` default, `disk_quota_gb = 50` soft cap, auto-cleanup of uploaded recordings only.
- **Replay fidelity:** MCAP channel metadata carries `zenoh.encoding`, `zenoh.topic`, optional `bubbaloop.schema_name`. Replay re-publishes bytes-identical with original encoding.
- **No `fleet_id` concept:** bucket is the sharing unit.
- **No SQLite index:** `manifest.json` per recording is the source of truth. Directory scan is the list operation.
- **Integrity is mandatory:** SHA-256 per chunk, in filename + manifest + `x-amz-checksum-sha256`. Not optional, not configurable.
- **MCAP defaults are rosbag2's defaults:** chunk_size=786 432, compression=zstd, level=fast, CRC=on. Overridable per-profile but the recipe is proven.
- **Topic selection composes additively:** `--topics + --regex + --exclude`. Default = `--all` (`bubbaloop/global/**`), `--include-local` is opt-in.
- **`storage publish` verb is reserved**, not implemented in v1 — Phase 3 HF Hub.
- **MCP-everywhere invariant:** every CLI verb ships a paired MCP tool in the same PR. Reserved/deferred verbs do **not** get stub MCP tools. CI enforces the pairing (see §14). Replay-style long-running operations get a `_start`/`_stop`/`_status` triad on the MCP side even though the CLI is single-verb. MCP-only tools (no CLI equivalent) are allowed for state inspection agents need but humans get from process introspection (`storage_config_get`, `storage_record_sessions`, `storage_replay_status`).

---

## 12. Critical files

### New files
- `crates/bubbaloop/src/storage/{mod,backend,recording,manifest,profile,discover,secrets,integrity,ring_buffer,sync,reconcile,replay}.rs`
- `crates/bubbaloop/src/storage/backend/{local,s3_compat}.rs`
- `crates/bubbaloop/src/cli/storage/{mod,configure,topics,profile,record,list,info,upload,download,replay,reconcile,rm}.rs`
- `crates/bubbaloop/src/mcp/storage_tools.rs`
- `dashboard/src/views/RecordingsView.tsx` (+ subcomponents: TopicPicker, RecordingList, NewRecordingModal)
- `docs/concepts/storage.md`, `docs/concepts/storage-internals.md`
- `bubbaloop-nodes-official/sinks/mcap-recorder/` (relocated from `bubbaloop-nodes-internal/recorder/`, refactored — see §3.3)

### Modified files
- `crates/bubbaloop/src/lib.rs` / `cli/mod.rs` — register `storage` subcommand
- `crates/bubbaloop/src/mcp/mod.rs` — register storage tools
- `crates/bubbaloop/Cargo.toml` — add `aws-sdk-s3 = "1"`, `mcap = "0.24"`, `sha2` (for SHA-256), `base64` (for checksum header)
- `crates/bubbaloop/src/daemon/mod.rs` — start `storage::sync` background task
- `dashboard/src/App.tsx` — register recordings route
- `ARCHITECTURE.md` — add Storage as third pillar in the layer model
- `ROADMAP.md` — check off Storage phases

### Deleted files
- `bubbaloop-nodes-internal/uploader/` (entire directory; functionality moves to core)

---

## 13. Reused existing pieces

- `ChunkedMcapWriter` from `bubbaloop-nodes-internal/recorder/src/mcap_writer.rs` — relocate, extend to compute SHA-256 streaming and embed prefix in filename. Keep the proven chunking + rename-on-finalize logic.
- `RawSubscriber` (`crates/bubbaloop-node/src/subscriber.rs:54-111`) — recorder uses raw bytes; the wire-format doc commits to "envelope verbatim."
- `manifest` queryable (`crates/bubbaloop-node/src/manifest.rs`) — already published by every node; storage's `discover.rs` queries `bubbaloop/**/manifest` for topic discovery.
- `S3Uploader::upload_and_delete()` from `bubbaloop-nodes-internal/uploader/src/s3_client.rs` — port the retry / verify logic into `storage::sync`, extend with checksum header.
- `aws-sdk-s3` config builder from the existing uploader — just swap the endpoint to R2.

---

## 14. Verification plan

### Unit tests (in-crate)
- `storage::backend::s3_compat` — mock S3 server (testcontainers + minio image), verify put/get/list/delete round-trips, `x-amz-checksum-sha256` set on upload, verified on get.
- `storage::backend::local` — `tempfile::tempdir()`, verify path layout matches manifest expectations.
- `storage::integrity` — SHA-256 streaming digest equals one-shot digest; verify with known test vectors.
- `storage::profile` — load/save/validate YAML including OTel-style nested blocks; reject unknown top-level keys (forward-compat for reserved `pipelines:`).
- `storage::discover` — mock Zenoh manifest replies, verify grouping by machine/node.
- `storage::ring_buffer` — verify sliding window drops samples older than `--window`, `flush()` seals current state as a chunk and resets window cleanly.
- `storage::replay` — read a fixture `.mcap`, verify re-published timing within tolerance, verify both `publish_time` and `log_time` round-trip.
- `storage::sync` — simulate chunk finalization, verify upload + SHA-256 header + manifest update + local cleanup. Inject upload failure mid-batch and verify retry.
- `storage::reconcile` — local manifest with 5 chunks, R2 has 3, verify reconcile re-uploads the missing 2 and updates manifest.
- `storage::manifest` — verify schema_version forward-compat (unknown fields preserved on round-trip).

### Integration tests (`cargo test --features test-harness --test integration_storage`)
- Spin up the daemon with `MockPlatform` and a `LocalFs` backend.
- `storage_record_start` → publish synthetic samples on test topics → `storage_record_stop` → verify chunks exist, are valid MCAP, manifest has SHA-256 per chunk, both timestamps present.
- `storage_list`, `storage_info`, `storage_download`, `storage_replay`, `storage_reconcile`, `storage_rm` round-trips.
- Ring-buffer flow: `--window 5s --trigger manual` → publish samples over 10s → `record flush` → verify recorded chunk contains only last 5s.
- Composable topic selection: `--topics A --regex B --exclude C` → verify final topic set is `(A ∪ regex_match(B)) - C`.
- Corruption test: write a chunk, manually flip a byte, run `storage download` against it → verify SHA-256 mismatch is detected and download fails loud.

### CLI ↔ MCP pairing enforcement (new test, blocks merge)
- `tests/integration_storage_mcp_parity.rs` — walks the `argh` CLI subcommand tree for `bubbaloop storage`, collects every leaf verb (`configure`, `topics`, `profile create`, …, `rm`), and asserts each one has a registered MCP handler whose name matches `storage_{verb_path}` (with `_start`/`_stop`/`_status` triad expansion for long-running ops listed in §6.2). Fails compilation-loud if a new CLI verb is added without a paired MCP tool, or if the MCP `_start`/`_stop`/`_status` triad is incomplete. Reserved verbs (`publish`, `import`) are skipped via an allowlist that the test prints — adding a new verb to that allowlist requires PR review.
- The same test asserts that every MCP-only tool (`storage_config_get`, `storage_record_sessions`, `storage_replay_status`, `storage_replay_stop`) is documented in `docs/concepts/storage-internals.md` under "Agent operations" via a simple grep on the tool name. Keeps the docs honest.

### End-to-end (manual + CI smoke)
- `pixi run check && pixi run clippy && pixi run test` — must pass with zero warnings.
- `pixi run fmt-check` (CI enforces this).
- Real R2 smoke test (CI secret): `bubbaloop storage configure` (scripted) → `record start --duration 10s --topics '**/test/**'` → `record stop` → `list --remote` → `download` → `replay` → verify byte-identical bytes and timestamps.
- Reconcile smoke: record + auto-upload, manually delete one chunk from R2 console, `storage reconcile <name>` → verify chunk re-uploaded.
- Dashboard: open Recordings tab, trigger a recording from the UI, verify it appears in the list with correct per-chunk upload progress, download it, open in Foxglove Studio, verify all channels render with correct timestamps.
- Migration smoke: install the new `mcap-recorder` next to the old out-of-tree one, verify both run (different instance names), then remove the old one cleanly.

### Documentation verification
- `docs/concepts/storage.md` walks a new user from `bubbaloop install` to "first recording uploaded to R2 + downloaded + replayed" in ≤7 commands.
- `docs/concepts/storage-internals.md` documents the manifest schema, chunk filename format, integrity flow, reconcile algorithm.
- `ARCHITECTURE.md` reflects the three-pillar model.
- `ROADMAP.md` Storage section has Phase 1 checked off; Phase 2/3 sections list the deferred items.

---

## 15. Risks and mitigations

| Risk | Mitigation |
|---|---|
| R2 multipart upload quirks | Avoid multipart for v1 (single PUT per chunk; chunks are <1 MB by default). |
| Recorder backlog → dropped samples | `RawSubscriber` already uses 4-slot FIFO with explicit drop semantics — document this is video-grade ("recent frames matter more than complete history"). Ring-buffer mode is the structural answer for "I really want recent samples not historical." |
| Disk fills before upload finishes | `disk_quota_gb` soft cap; daemon refuses new `record start` if quota exceeded, surfaces error in CLI/dashboard. |
| Browser-recorder users break on upgrade | One-release deprecation window with in-dashboard banner + migration doc. |
| MCAP channels missing schemas → Foxglove can't render | Recorder fetches schemas via `bubbaloop/**/schema` queryable at session start, embeds in MCAP. Falls back to "raw" encoding if no schema. |
| Multi-session naming collisions | Auto-generated names include timestamp + machine_id; user-specified names rejected if active. |
| Credentials accidentally logged | `secrets.rs` returns opaque `Secret<String>` wrappers (zeroize-on-drop), `Debug` impls redacted; clippy lint added. |
| SHA-256 computation cost on hot path | Use `sha2`'s SIMD-accelerated impl; benchmark shows <1% CPU at 100 MB/s on Jetson Orin. Acceptable. |
| Partial upload leaves orphan chunks in R2 | Deterministic object keys + `reconcile` command + (optional) R2 lifecycle rule for incomplete multipart. Orphans are visible as objects without manifest reference; a future `storage gc` command could clean them. |
| Centralized recorder hits bandwidth ceiling | Documented breaking point (50% router link / 80% CPU) + v2 federated escape hatch in architecture section. No design lock-in. |
| Schema version drift in `manifest.json` | `schema_version: 1` field; loader preserves unknown fields on round-trip so future writers don't lose data. |

---

## 16. Prior art (for the intern — verify these decisions against the sources)

The design adopts conventions that converged across mature frameworks. The intern should skim the linked docs before implementing the corresponding piece, to catch any subtleties this spec didn't capture.

| Bubbaloop decision | Source / validation |
|---|---|
| MCAP defaults (chunk 786 432 bytes, zstd fast, CRC on) | [rosbag2_storage_mcap defaults](https://github.com/ros2/rosbag2/blob/rolling/rosbag2_storage_mcap/README.md) |
| Replay flags (`--rate`, `--loop`, `--start-offset`, `--remap`) | [ROS 2 Rolling rosbag2 tutorial](https://docs.ros.org/en/rolling/Tutorials/Beginner-CLI-Tools/Recording-And-Playing-Back-Data/Recording-And-Playing-Back-Data.html) |
| Composable topic selection (fixes rosbag2 friction) | [rosbag2 #1620](https://github.com/ros2/rosbag2/issues/1620) — the long-standing complaint we're addressing |
| SHA-256 on upload + checksum header | [Foxglove Agent v1.3.0 changelog](https://docs.foxglove.dev/changelog/agent/v1.3.0), [S3 additional checksums tutorial](https://docs.aws.amazon.com/AmazonS3/latest/userguide/tutorial-s3-mpu-additional-checksums.html) |
| Self-describing recordings, no separate index DB | [LeRobotDataset v3](https://huggingface.co/blog/lerobot-datasets-v3) |
| Deterministic object keys with content hash | [Kafka tiered storage segment naming](https://kafka.apache.org/41/operations/tiered-storage/) |
| Centralized vs federated recorder topology | [RTI Recording Service blog](https://www.rti.com/blog/ros2-dds-play-it-again) |
| Per-machine bearer tokens + minted presigned URLs (Phase 2) | [Skydio Cloud telemetry upload](https://support.skydio.com/hc/en-us/articles/4416083654043), [Auterion Suite docs](https://docs.auterion.com/vehicle-operation/auterion-suite-fleet-management) |
| Profile YAML field names (`sending_queue`, `retry_on_failure`) | [OpenTelemetry Collector exporterhelper README](https://github.com/open-telemetry/opentelemetry-collector/blob/main/exporter/exporterhelper/README.md) |
| Agent-watches-directory upload pattern | [Foxglove Agent docs](https://docs.foxglove.dev/docs/agent) |
| `storage publish` reserved for HF Hub (Phase 3) | [LeRobot `push_to_hub`](https://huggingface.co/docs/lerobot/il_robots) |
| Ring-buffer mode (rosbag2 gap, our differentiator) | rosbag2 lacks this; commonly-requested. Implementation: circular MCAP writer with `flush()` that seals current buffer. |

---

## 17. Post-approval workflow

After plan-mode exit:

1. Move this doc to `docs/superpowers/specs/2026-05-26-bubbaloop-storage-design.md`. Commit on a new branch (`feat/bubbaloop-storage-spec`).
2. Hand the spec to the intern.
3. Intern uses `superpowers:writing-plans` skill to produce the detailed implementation plan (file-by-file changes, test order, PR boundaries).
4. Intern executes via `superpowers:executing-plans` skill (or `subagent-driven-development` for parallelizable chunks).

The implementation plan should split into at least 4 PRs to keep reviews tractable:

- **PR1:** `storage::backend` + `storage::manifest` + `storage::profile` + `storage::secrets` + `storage::integrity` (pure logic, no Zenoh, full unit coverage including corruption tests).
- **PR2:** `storage::discover` + `storage::sync` + `storage::reconcile` + CLI `configure/topics/profile/list/info/upload/download/reconcile/rm` (no recording yet; uses fixture .mcap files for upload/download/reconcile tests; smoke against real R2 in CI).
- **PR3:** `storage::replay` + `storage::ring_buffer` + CLI `replay` + `mcap-recorder` marketplace cleanup (relocate, fix envelope, multi-session, ring-buffer, SHA-256, rosbag2 defaults, channel metadata, dual timestamps).
- **PR4:** CLI `record start/stop/flush` + MCP tools + dashboard Recordings tab + browser-recorder deprecation banner + documentation.

Each PR is independently shippable and reviewable.
