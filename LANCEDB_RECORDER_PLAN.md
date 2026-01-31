# Bubbaloop Storage Service - Implementation Plan

## Overview

A **generic storage service** for bubbaloop that records ANY Zenoh message to LanceDB
backed by GCP Cloud Storage. Unlike a type-specific recorder, the storage service is
**message-type agnostic** -- it stores raw protobuf bytes alongside extracted metadata,
so users writing custom nodes with their own message types can use it without modification.

### Why Generic?

Users will write their own nodes with custom protobuf message types. The storage service
should not need to know about `CompressedImage` or `CurrentWeather` specifically. Instead:

1. **All bubbaloop data messages embed a `Header` as protobuf field 1** -- this is a
   convention enforced across the ecosystem. The Header contains `acq_time`, `pub_time`,
   `sequence`, `frame_id`, `machine_id`, `scope`.

2. **Raw bytes are stored as blobs** -- the storage service subscribes to topics, extracts
   the Header from field 1 (works for ANY message following the convention), and stores
   the complete raw protobuf bytes alongside the metadata.

3. **Schema descriptors stored per-topic** -- using `bubbaloop-schemas`'s descriptor system,
   the service records the protobuf `FileDescriptorSet` for each message type. This enables
   downstream consumers (dashboard, analysis tools) to decode the raw bytes later.

4. **Topic-pattern subscriptions** -- users configure topic patterns (e.g., `/camera/**`,
   `/weather/**`, `my-custom-node/**`) rather than specific message types.

### Architecture

```
┌──────────────────────────────────────────────────────────────┐
│                    Storage Service                           │
│                                                              │
│  ┌──────────┐    ┌──────────────┐    ┌──────────────────┐   │
│  │  Zenoh   │───>│ Header       │───>│  Batch Writer    │   │
│  │ Subscriber│   │ Extractor    │    │  (LanceDB)       │   │
│  │ (topics) │    │ (field 1)    │    │                  │   │
│  └──────────┘    └──────────────┘    └────────┬─────────┘   │
│                                               │             │
│  ┌──────────┐    ┌──────────────┐             │             │
│  │  Zenoh   │<───│ Query API    │<────────────┘             │
│  │ Queryable│    │ (JSON)       │                           │
│  └──────────┘    └──────────────┘                           │
│                                                              │
│  Schema Registry: topic -> (type_name, descriptor_bytes)    │
└──────────────────────────────────────────────────────────────┘
```

---

## Core Design: Message-Type Agnostic Storage

### Single `messages` Table

Instead of separate `frames` and `weather` tables, one unified table:

```
messages table (Arrow schema):
  ├── timestamp_ns    : Int64        (from Header.acq_time)
  ├── pub_time_ns     : Int64        (from Header.pub_time)
  ├── sequence        : UInt32       (from Header.sequence)
  ├── topic           : Utf8         (Zenoh key expression)
  ├── frame_id        : Utf8         (from Header.frame_id)
  ├── machine_id      : Utf8         (from Header.machine_id)
  ├── message_type    : Utf8         (e.g., "bubbaloop.camera.v1.CompressedImage")
  ├── data_size       : UInt64       (byte length, for queries without loading blob)
  ├── raw_data        : LargeBinary  (complete protobuf bytes, blob storage)
  └── session_id      : Utf8         (links to sessions table)
```

### `sessions` Table

```
sessions table (Arrow schema):
  ├── session_id      : Utf8
  ├── session_name    : Utf8 (nullable)
  ├── start_time_ns   : Int64
  ├── end_time_ns     : Int64 (nullable, set on stop)
  ├── topics          : Utf8         (JSON array of subscribed topic patterns)
  ├── message_count   : UInt64
  ├── machine_id      : Utf8 (nullable)
  └── status          : Utf8         ("recording", "completed", "interrupted")
```

### `schemas` Table (Schema Registry)

```
schemas table (Arrow schema):
  ├── topic           : Utf8         (topic pattern or exact topic)
  ├── message_type    : Utf8         (fully qualified protobuf name)
  ├── descriptor      : LargeBinary  (FileDescriptorSet bytes)
  ├── first_seen_ns   : Int64
  └── session_id      : Utf8
```

This enables any consumer to decode stored raw bytes later:
```rust
// Consumer retrieves schema:
let descriptor_bytes = schemas_table.query("topic = '/camera/entrance/compressed'");
let descriptor_set = FileDescriptorSet::decode(descriptor_bytes)?;
// Then decode any message from that topic
let msg = DynamicMessage::decode(descriptor_set, "bubbaloop.camera.v1.CompressedImage", raw_data)?;
```

### Generic Header Extraction

The Header is always protobuf field 1 in data messages. We can extract it without knowing
the outer message type:

```rust
/// Extract Header from ANY protobuf message where Header is field 1.
/// Uses raw protobuf wire format: field 1 with wire type 2 (length-delimited).
/// Falls back gracefully if the message doesn't have a Header.
fn extract_header(raw_bytes: &[u8]) -> Option<Header> {
    // Approach 1: Direct decode attempt
    // Since Header is field 1 and all other fields have different numbers,
    // we can try decoding the raw bytes AS a wrapper struct with just field 1:
    #[derive(prost::Message)]
    struct HeaderWrapper {
        #[prost(message, optional, tag = "1")]
        header: Option<Header>,
    }
    HeaderWrapper::decode(raw_bytes).ok()?.header
}
```

This works because protobuf ignores unknown fields during decode. A `HeaderWrapper` with
only field 1 will successfully decode from any message that has a Header at field 1,
silently ignoring all other fields.

### Message Type Detection

For known types (from `bubbaloop-schemas`), the type is determined by the ros-z topic
encoding. For unknown/user types, the storage service can:

1. **Discover via ros-z topic metadata** -- ros-z encodes the type name in the topic key
   expression (e.g., `0/camera%entrance%compressed/bubbaloop.camera.v1.CompressedImage`)
2. **Accept registration via config** -- users map topics to type names in `storage.yaml`
3. **Fall back to "unknown"** -- store raw bytes with `message_type = "unknown"`, still
   extract Header if present

### User Node Integration

A user writing a custom node just needs to:

1. **Follow the Header convention** -- embed `Header` as field 1 in their protobuf message
2. **Add their topic to the storage config** -- or use a wildcard pattern
3. **Optionally register their schema** -- for typed decoding in the dashboard

Example user proto:
```protobuf
syntax = "proto3";
package mycompany.lidar.v1;
import "bubbaloop/header.proto";

message PointCloud {
    bubbaloop.header.v1.Header header = 1;  // Convention: Header is field 1
    uint32 num_points = 2;
    bytes points_data = 3;
    string frame_id = 4;
}
```

The storage service records this automatically -- no code changes needed.

---

## Deliverables

1. `crates/bubbaloop-nodes/storage/` -- standalone Rust node (renamed from lancedb-recorder)
2. `bubbaloop record` CLI subcommand in `crates/bubbaloop/`
3. Recording controls (record button) in the React dashboard
4. Recordings panel in the React dashboard for browsing/playback

## Service Naming

| Old (plan v1) | New (generic) |
|---------------|---------------|
| `lancedb-recorder` | `storage` |
| `lancedb_recorder` (binary) | `storage_node` (binary) |
| `bubbaloop/recorder/api/**` | `bubbaloop/storage/api/**` |
| `crates/bubbaloop-nodes/lancedb-recorder/` | `crates/bubbaloop-nodes/storage/` |

---

## Configuration

### `configs/storage.yaml`

```yaml
# Backend: gs://bucket/path for GCS, or local filesystem path
storage_uri: "gs://bubbaloop-recordings/data"

# Topic patterns to subscribe to (supports wildcards)
# The service subscribes to ALL matching topics
topics:
  - "/camera/**"          # All camera topics
  - "/weather/**"         # All weather topics
  - "my-sensor/**"        # User's custom node topics

# Optional: explicit topic-to-type mapping for schema registration
# If omitted, types are auto-detected from ros-z topic encoding
schema_hints:
  "/camera/*/compressed": "bubbaloop.camera.v1.CompressedImage"
  "/weather/current": "bubbaloop.weather.v1.CurrentWeather"
  # Users add their own:
  # "my-sensor/lidar": "mycompany.lidar.v1.PointCloud"

# Performance
batch_size: 30
flush_interval_secs: 5
```

Key design choice: `topics` uses **patterns with wildcards** so users don't need to
enumerate every specific topic. `schema_hints` is optional -- the service works without
it, but having it enables typed queries in the dashboard.

---

## Guardrails

### Must Have
- Store raw protobuf bytes as blobs (never decode to type-specific fields)
- Extract Header from field 1 generically (works for any message with a Header)
- Support wildcard topic patterns (`/camera/**`, `my-sensor/**`)
- Schema registry: store FileDescriptorSet per topic for later decoding
- All the existing guardrails from plan v1 (standalone node, RwLock, Cargo.lock, etc.)

### Must NOT Have
- Do NOT have type-specific tables (no `frames` table, no `weather` table)
- Do NOT require knowledge of specific message types at compile time
- Do NOT require users to modify the storage node code to add new message types
- Do NOT decode message payloads beyond Header extraction

---

## Dual Control Path (unchanged from v1)

| Control | Mechanism | What it does |
|---------|-----------|--------------|
| `bubbaloop node start/stop storage` | systemd (process lifecycle) | Starts/stops the **process** |
| `bubbaloop record start/stop` | Zenoh query to storage API | Starts/stops **recording** |

---

## Implementation Phases

### Phase 0: aarch64 Validation (GATE)

Unchanged from v1 -- verify `lancedb = "0.23"` compiles on Jetson.

### Phase 1: Storage Node Skeleton

**Goal:** Standalone crate at `crates/bubbaloop-nodes/storage/`.

Files:
```
crates/bubbaloop-nodes/storage/
├── .cargo/config.toml
├── Cargo.toml          # lancedb, zenoh, ros-z, prost, bubbaloop-schemas
├── Cargo.lock
├── pixi.toml
├── node.yaml           # name: storage
├── configs/storage.yaml
└── src/
    ├── lib.rs
    ├── error.rs
    ├── config.rs       # StorageConfig with topic patterns + schema_hints
    ├── header.rs       # Generic Header extraction (HeaderWrapper trick)
    ├── bin/storage_node.rs
    ├── lancedb_client.rs
    ├── session.rs
    ├── recorder_node.rs
    └── zenoh_api.rs
```

**Key new module: `header.rs`**
```rust
use bubbaloop_schemas::Header;

/// Wrapper for extracting Header from any protobuf message where Header is field 1.
#[derive(prost::Message)]
struct HeaderWrapper {
    #[prost(message, optional, tag = "1")]
    pub header: Option<Header>,
}

/// Extract Header metadata from raw protobuf bytes.
/// Returns None if the message doesn't have a Header at field 1.
pub fn extract_header(raw_bytes: &[u8]) -> Option<Header> {
    use prost::Message;
    HeaderWrapper::decode(raw_bytes).ok()?.header
}

/// Message metadata extracted from the Header + Zenoh sample.
#[derive(Debug, Clone)]
pub struct MessageMeta {
    pub timestamp_ns: i64,      // acq_time
    pub pub_time_ns: i64,       // pub_time
    pub sequence: u32,
    pub frame_id: String,
    pub machine_id: String,
    pub topic: String,
    pub message_type: String,   // from ros-z or schema_hints or "unknown"
    pub data_size: u64,
}
```

### Phase 2: LanceDB Client (Generic Schema)

**Goal:** Single `messages` table + `sessions` table + `schemas` table.

```rust
pub struct StorageClient {
    connection: lancedb::Connection,
}

impl StorageClient {
    pub async fn connect(uri: &str) -> Result<Self>;
    pub async fn ensure_tables(&self) -> Result<()>;

    // Generic message storage
    pub async fn insert_messages(&self, messages: &[StoredMessage]) -> Result<usize>;

    // Session management
    pub async fn create_session(&self, session: &SessionRecord) -> Result<()>;
    pub async fn update_session(&self, id: &str, end_time: i64, count: u64, status: &str) -> Result<()>;
    pub async fn list_sessions(&self) -> Result<Vec<SessionRecord>>;

    // Schema registry
    pub async fn register_schema(&self, topic: &str, type_name: &str,
                                  descriptor: &[u8], session_id: &str) -> Result<()>;

    // Queries
    pub async fn query_messages(&self, session_id: &str, topic: Option<&str>,
                                 offset: usize, limit: usize) -> Result<Vec<MessageMeta>>;
    pub async fn get_message(&self, session_id: &str, topic: &str, sequence: u32) -> Result<Option<Vec<u8>>>;
    pub async fn get_schema(&self, topic: &str) -> Result<Option<(String, Vec<u8>)>>;
}

#[derive(Debug, Clone)]
pub struct StoredMessage {
    pub meta: MessageMeta,
    pub raw_data: Vec<u8>,      // Complete protobuf bytes
    pub session_id: String,
}
```

### Phase 3: Recording Logic

**Goal:** Generic subscription + Header extraction + batch insert.

```rust
// The recorder subscribes to topics as RAW bytes (not typed):
// Uses zenoh subscriber directly (not ros-z typed subscription)
// so it doesn't need to know message types at compile time.

let subscriber = session.declare_subscriber(topic_pattern).await?;

// For each sample:
loop {
    let sample = subscriber.recv_async().await?;
    let topic = sample.key_expr().as_str().to_string();
    let raw_bytes = sample.payload().to_bytes().to_vec();

    // Extract Header generically
    let header = extract_header(&raw_bytes);

    // Detect message type from ros-z encoding or schema_hints
    let message_type = detect_message_type(&topic, &config.schema_hints);

    // Register schema if first time seeing this topic
    if !seen_topics.contains(&topic) {
        if let Some(descriptor) = get_descriptor_for_topic(&topic, &message_type) {
            client.register_schema(&topic, &message_type, &descriptor, &session_id).await?;
        }
        seen_topics.insert(topic.clone());
    }

    let stored = StoredMessage {
        meta: MessageMeta {
            timestamp_ns: header.as_ref().map(|h| h.acq_time as i64).unwrap_or_else(now_ns),
            pub_time_ns: header.as_ref().map(|h| h.pub_time as i64).unwrap_or(0),
            sequence: header.as_ref().map(|h| h.sequence).unwrap_or(0),
            frame_id: header.as_ref().map(|h| h.frame_id.clone()).unwrap_or_default(),
            machine_id: header.as_ref().map(|h| h.machine_id.clone()).unwrap_or_default(),
            topic,
            message_type,
            data_size: raw_bytes.len() as u64,
        },
        raw_data: raw_bytes,
        session_id: session_id.clone(),
    };

    tx.send(stored)?;
}
```

**Key difference from v1:** Uses raw Zenoh subscription, not ros-z typed `create_sub::<CompressedImage>()`.
This means ANY message can be stored, regardless of type.

### Phase 4: Storage Zenoh Query API

```
bubbaloop/storage/api/command                           -- start/stop/status
bubbaloop/storage/api/sessions                          -- list all sessions
bubbaloop/storage/api/sessions/{id}                     -- session details
bubbaloop/storage/api/sessions/{id}/messages             -- message metadata (no blobs)
bubbaloop/storage/api/sessions/{id}/messages/{topic}/{seq} -- single message raw bytes
bubbaloop/storage/api/sessions/{id}/topics               -- list topics in session + counts
bubbaloop/storage/api/schemas/{topic}                    -- get schema for a topic
```

Changed from v1: no `/frames` or `/weather` endpoints. Instead generic `/messages`
with topic filtering. `/schemas` endpoint enables dashboard to decode any stored message.

### Phase 5: CLI `bubbaloop record` Subcommand

Unchanged from v1, except references use `storage` naming:
- Queries `bubbaloop/storage/api/command`
- `bubbaloop record list` shows sessions with topic counts (not frame/weather counts)

### Phase 5.5: systemd EnvironmentFile Support

Unchanged from v1.

### Phase 6: Dashboard Record Button

Unchanged from v1, except Zenoh topic is `bubbaloop/storage/api/command`.

### Phase 7: Dashboard Recordings Panel

**Changed from v1:** The recordings panel is now generic:

1. **Session browser** -- list sessions with time range, message count, topic list
2. **Topic filter** -- select a topic within a session to browse messages
3. **Message timeline** -- scrubber showing messages over time for selected topic
4. **Raw message viewer** -- displays raw bytes, hex dump, or decoded JSON if schema available
5. **Schema-aware decoding** -- fetches schema from `/schemas/{topic}`, uses `protobufjs`
   to decode raw bytes into readable JSON in the browser
6. **Per-topic stats** -- message count, avg size, time range per topic

For camera topics specifically, the dashboard can detect `format: "h264"` in the decoded
message and attempt frame display. But this is a best-effort enhancement, not a requirement.

---

## How Users Extend This

### Scenario: User creates a LiDAR node

1. User defines `lidar.proto` with Header at field 1
2. User's node publishes to `/lidar/front/pointcloud` via ros-z
3. User adds to `storage.yaml`:
   ```yaml
   topics:
     - "/lidar/**"
   schema_hints:
     "/lidar/*/pointcloud": "mycompany.lidar.v1.PointCloud"
   ```
4. Storage service automatically records all LiDAR messages
5. Dashboard shows LiDAR data in the recordings panel as decoded JSON

### Scenario: User has a message WITHOUT a Header

The storage service still works -- it falls back to:
- `timestamp_ns` = current wall clock time
- `sequence` = 0
- `frame_id`, `machine_id` = empty
- Raw bytes are stored intact

The data is still queryable by topic and time range, just without sub-message metadata.

---

## Risk Additions (beyond v1)

### Risk 7: Raw Zenoh Subscription vs ros-z
**Likelihood:** Low
**Impact:** Medium
**Description:** Using raw Zenoh `declare_subscriber` instead of ros-z typed subscriptions
means we don't get automatic protobuf decoding. The ros-z encoding adds topic metadata
(domain ID, type info) that we need to parse manually.
**Mitigation:** Use Zenoh's key expression pattern matching which works on the raw topic
string. For type detection, parse the ros-z topic encoding format or rely on `schema_hints`.

### Risk 8: Header Extraction from Non-Conforming Messages
**Likelihood:** Medium (user error)
**Impact:** Low
**Description:** If a user's message doesn't have Header at field 1, or has a different
message at field 1, the `HeaderWrapper` decode may extract garbage or fail.
**Mitigation:** Validate extracted header fields (e.g., `acq_time` should be a reasonable
nanosecond timestamp, not zero or far-future). Fall back to wall clock time on failure.

---

## File Summary

### New Files (18 files)

| File | Phase | Description |
|------|-------|-------------|
| `crates/bubbaloop-nodes/storage/.cargo/config.toml` | 1 | Cargo build config |
| `crates/bubbaloop-nodes/storage/Cargo.toml` | 1 | Crate manifest |
| `crates/bubbaloop-nodes/storage/Cargo.lock` | 1 | Lock file |
| `crates/bubbaloop-nodes/storage/pixi.toml` | 1 | Pixi build config |
| `crates/bubbaloop-nodes/storage/node.yaml` | 1 | Node manifest (name: storage) |
| `crates/bubbaloop-nodes/storage/configs/storage.yaml` | 1 | Default config |
| `crates/bubbaloop-nodes/storage/src/lib.rs` | 1 | Library root |
| `crates/bubbaloop-nodes/storage/src/error.rs` | 1 | Error types |
| `crates/bubbaloop-nodes/storage/src/config.rs` | 1 | Config with topic patterns + schema_hints |
| `crates/bubbaloop-nodes/storage/src/header.rs` | 1 | Generic Header extraction |
| `crates/bubbaloop-nodes/storage/src/bin/storage_node.rs` | 1 | Binary entry point |
| `crates/bubbaloop-nodes/storage/src/lancedb_client.rs` | 2 | LanceDB operations (generic schema) |
| `crates/bubbaloop-nodes/storage/src/session.rs` | 3 | Session state machine |
| `crates/bubbaloop-nodes/storage/src/recorder_node.rs` | 3 | Generic recording logic |
| `crates/bubbaloop-nodes/storage/src/zenoh_api.rs` | 4 | Query API (JSON) |
| `crates/bubbaloop/src/cli/zenoh.rs` | 5 | Shared get_zenoh_session() |
| `crates/bubbaloop/src/cli/record.rs` | 5 | CLI record subcommand |
| `dashboard/src/components/RecordButton.tsx` | 6 | Record button |
| `dashboard/src/components/RecordingsView.tsx` | 7 | Recordings panel |
| `dashboard/src/components/SortableRecordingsCard.tsx` | 7 | DnD wrapper |

### Modified Files (7 files)

| File | Phase | Change |
|------|-------|--------|
| `crates/bubbaloop/src/cli/mod.rs` | 5 | Add record + zenoh modules |
| `crates/bubbaloop/src/cli/node.rs` | 5 | Extract get_zenoh_session() |
| `crates/bubbaloop/src/bin/bubbaloop.rs` | 5 | Add Record command |
| `crates/bubbaloop/src/daemon/systemd.rs` | 5.5 | EnvironmentFile + TimeoutStopSec |
| `dashboard/src/lib/storage.ts` | 7 | Add recordings panel type |
| `dashboard/src/components/Dashboard.tsx` | 7 | Register recordings panel |
| `dashboard/src/App.tsx` | 6 | Add RecordButton to header |

---

## Commit Strategy

| Commit | Phase | Message |
|--------|-------|---------|
| 1 | Phase 0 | `test: validate lancedb 0.23 compiles on aarch64` |
| 2 | Phase 1 | `feat: add storage node skeleton with generic header extraction` |
| 3 | Phase 2 | `feat: implement generic LanceDB client with messages/sessions/schemas tables` |
| 4 | Phase 3 | `feat: add recording logic with raw Zenoh subscription and batch inserts` |
| 5 | Phase 4 | `feat: expose storage Zenoh queryable API for sessions and messages` |
| 6 | Phase 5 | `feat: add bubbaloop record CLI subcommand and extract shared zenoh helper` |
| 7 | Phase 5.5 | `feat: add EnvironmentFile and TimeoutStopSec to systemd unit generator` |
| 8 | Phase 6 | `feat: add recording control button to dashboard` |
| 9 | Phase 7 | `feat: add recordings panel with schema-aware message decoding` |

---

## Success Criteria

1. **Generic:** Store ANY protobuf message without code changes to the storage node
2. **Functional:** Round-trip works: record via CLI/dashboard, browse in dashboard
3. **Extensible:** Users add topics to config, storage handles the rest
4. **Schema-aware:** Dashboard can decode stored messages using registered schemas
5. **Performance:** Batch inserts of 30 messages in <2s; no drops at 30fps single camera
6. **Reliable:** SIGTERM flushes + finalizes; sessions always have end_time on stop
7. **aarch64:** Validated on Jetson before implementation
