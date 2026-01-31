# LanceDB Recorder - Implementation Plan (Revised)

## Context

### Original Request
Build a LanceDB-based recording system for bubbaloop that stores data in Google Cloud Storage,
encompassing a new recorder node, CLI commands, dashboard record button, and recordings viewer.

### Research Findings
- Existing MCAP recorder at `crates/bubbaloop-nodes/recorder/` provides the structural template
- Standalone node pattern: own `[workspace]`, path dependency on `bubbaloop-schemas`, `argh` CLI, `ros-z` subscriptions
- CLI extends `Command` enum in `crates/bubbaloop/src/bin/bubbaloop.rs` with argh subcommands
- Dashboard panels: `PanelType` union in `storage.ts`, `SortableXxxCard` wrapper, `XxxView` content component
- **Dashboard API pattern clarification:** The daemon has TWO communication paths:
  - **REST-like JSON API** (`zenoh_api.rs`): Used by CLI (`cli/node.rs`) and by some dashboard queries. Endpoints like `bubbaloop/daemon/api/nodes`, `bubbaloop/daemon/api/nodes/{name}/command`. Payloads are JSON strings, responses are JSON strings.
  - **Protobuf pub/sub** (`zenoh_service.rs`): Used by `NodesView.tsx` for the `bubbaloop/daemon/command` endpoint. Uses protobuf `NodeCommand`/`CommandResult` encode/decode.
  - The recorder API will use the **JSON API pattern** from `zenoh_api.rs`, since it is a standalone node (not the daemon) and JSON is simpler for a new queryable service. This is consistent with how the CLI already communicates with the daemon REST API.
- `get_zenoh_session()` in `cli/node.rs` (line 329) is private and needs extraction for reuse
- `NodeManager` uses `tokio::sync::RwLock<HashMap<...>>` for its primary state (line 124 of `node_manager.rs`)

---

## Work Objectives

### Core Objective
Create a production-quality LanceDB recording pipeline where a standalone Rust node subscribes to
Zenoh topics, batches data into LanceDB tables stored in GCS, exposes a queryable API for data
retrieval, and is controllable from both CLI and dashboard.

### Deliverables
1. `crates/bubbaloop-nodes/lancedb-recorder/` -- standalone Rust node
2. `bubbaloop record` CLI subcommand in `crates/bubbaloop/`
3. Recording controls (record button) in the React dashboard
4. Recordings panel in the React dashboard for browsing/playback

### Definition of Done
- `cargo check` passes for the lancedb-recorder crate (including aarch64 validation)
- `pixi run check` passes for the main workspace
- `pixi run clippy` passes with zero warnings
- Unit tests pass for all new modules
- Dashboard builds (`npm run build`) with no TypeScript errors
- Recording round-trip works: start recording via CLI -> data appears in LanceDB -> query via dashboard

---

## Guardrails

### Must Have
- Follow standalone node pattern exactly (own `[workspace]`, `.cargo/config.toml`, `pixi.toml`, `node.yaml`)
- Use `bubbaloop-schemas` path dependency for protobuf types
- Batch inserts (configurable batch size, default 30) for LanceDB write performance
- GCS authentication via `GOOGLE_APPLICATION_CREDENTIALS` env var or service account path
- Zenoh queryable API on the recorder node for session/frame queries
- Health heartbeat every 5s on `bubbaloop/{scope}/{machine_id}/health/lancedb-recorder`
- Start/stop recording via Zenoh command (independent of systemd start/stop)
- Session tracking with unique IDs, start/end times, topic list, frame count
- Input validation on all CLI args and Zenoh command payloads
- Co-located `#[cfg(test)] mod tests` blocks for all Rust modules
- TypeScript types for all dashboard data structures
- Use `tokio::sync::RwLock` for async state (not `std::sync::Mutex`)
- Commit `Cargo.lock` for the standalone crate (reproducible builds)
- Graceful shutdown: on SIGTERM during recording, flush buffers, finalize session, then exit

### Must NOT Have
- Do NOT add lancedb-recorder to the workspace `members` in root `Cargo.toml`
- Do NOT add `bubbaloop-schemas` as a git dependency (path only)
- Do NOT have the dashboard query GCS directly (all data flows through the recorder's Zenoh API)
- Do NOT modify existing MCAP recorder code
- Do NOT introduce new protobuf files in this phase (use JSON over Zenoh for recorder API)
- Do NOT add authentication/authorization to the recorder API (out of scope)
- Do NOT store decoded/decompressed frames (store raw protobuf bytes as blobs)
- Do NOT use `std::sync::Mutex` in async context

---

## Dual Control Path (Architecture Decision)

The lancedb-recorder has two distinct control planes:

| Control | Mechanism | What it does |
|---------|-----------|--------------|
| `bubbaloop node start/stop lancedb-recorder` | systemd (process lifecycle) | Starts/stops the recorder **process** |
| `bubbaloop record start/stop` | Zenoh query to recorder API | Starts/stops **recording** (application-level) |

**On SIGTERM during active recording:**
1. Signal subscription tasks to stop accepting new messages
2. Flush all pending frame/weather buffers to LanceDB
3. Update session record with final `end_time_ns` and counts
4. Then exit cleanly

The systemd unit for this node will include `TimeoutStopSec=120` to allow flush time.

---

## Implementation Phases

### Phase 0: aarch64 Validation (GATE)

**Goal:** Verify that `lancedb = "0.23"` compiles on the Jetson (aarch64). This is a blocking gate before any other work begins.

#### Steps

1. Create a minimal `Cargo.toml` with just `lancedb = "0.23"` and `tokio`
2. Run `cargo check` on the Jetson
3. If it compiles: proceed to Phase 1
4. If it fails: document the error and fall back to `object_store + parquet` directly (skip LanceDB, use `object_store = { version = "...", features = ["gcs"] }` with `parquet` crate). Update all subsequent phases to use parquet files instead of LanceDB tables.
5. After `lancedb` is added, run `cargo tree -d` to find the exact `arrow-*` versions it pulls in transitively. Pin `arrow-schema`, `arrow-array`, `arrow-buffer` in `Cargo.toml` to match those versions exactly.

#### Acceptance Criteria
- [ ] `cargo check` succeeds with `lancedb = "0.23"` on aarch64
- [ ] Arrow crate versions documented and pinned to match lancedb transitive deps

---

### Phase 1: LanceDB Recorder Node Skeleton

**Goal:** Create the standalone crate structure, compile successfully, connect to Zenoh.

#### Files to Create

**`crates/bubbaloop-nodes/lancedb-recorder/.cargo/config.toml`**
```toml
[build]
target-dir = "target"
```

**`crates/bubbaloop-nodes/lancedb-recorder/Cargo.toml`**
```toml
[package]
name = "lancedb_recorder"
version = "0.1.0"
authors = ["Bubbaloop Team"]
license = "Apache-2.0"
edition = "2021"
description = "LanceDB recorder for Zenoh topics with GCS backend"

[dependencies]
argh = "0.1"
ctrlc = "3.4"
env_logger = "0.11"
log = "0.4"
tokio = { version = "1", features = ["full"] }
futures = "0.3"
ros-z = { git = "https://github.com/ZettaScaleLabs/ros-z.git", branch = "main", features = ["protobuf"] }
zenoh = "1.7"
prost = "0.14"
prost-types = "0.14"
serde = { version = "1.0", features = ["derive"] }
serde_json = "1.0"
serde_yaml = "0.9"
flume = "0.12"
thiserror = "2.0"
hostname = "0.4"
uuid = { version = "1", features = ["v4"] }
chrono = { version = "0.4", features = ["serde"] }
lancedb = "0.23"
# Pin arrow-* versions to match lancedb transitive deps.
# Run `cargo tree -d` after adding lancedb to find exact versions.
# The versions below are placeholders -- update after Phase 0 validation.
arrow-schema = "54"
arrow-array = "54"
arrow-buffer = "54"
bubbaloop-schemas = { path = "../../bubbaloop-schemas", features = ["ros-z", "descriptor", "config"] }

[[bin]]
name = "lancedb_recorder"
path = "src/bin/lancedb_recorder.rs"

[workspace]
```

**IMPORTANT:** The arrow-* versions above are placeholders. After Phase 0, check `cargo tree -d`
output and pin to whatever versions lancedb 0.23 pulls in transitively. GCS support is determined
at runtime by the `gs://` URI scheme -- there is no `features = ["gcs"]` flag needed.

**`crates/bubbaloop-nodes/lancedb-recorder/pixi.toml`**
```toml
[project]
name = "lancedb-recorder"
version = "0.1.0"
description = "LanceDB recorder for Zenoh topics with GCS backend"
channels = ["conda-forge"]
platforms = ["linux-64", "linux-aarch64"]

[tasks]
build = { cmd = "cargo build --release --bin lancedb_recorder", env = { CARGO_TARGET_DIR = "./target" } }
run = { cmd = "cargo run --release --bin lancedb_recorder --", env = { CARGO_TARGET_DIR = "./target" } }
clean = { cmd = "cargo clean", env = { CARGO_TARGET_DIR = "./target" } }

[dependencies]
rust = ">=1.75"
pkg-config = ">=0.29"
cmake = ">=3.20"
zlib = ">=1.2"
protobuf = ">=3.20"
openssl = ">=3.0"
```

**`crates/bubbaloop-nodes/lancedb-recorder/node.yaml`**
```yaml
name: lancedb-recorder
version: "0.1.0"
type: rust
description: LanceDB recorder for Zenoh topics with GCS backend
author: Bubbaloop Team
build: pixi run build
command: ./target/release/lancedb_recorder
```

**`crates/bubbaloop-nodes/lancedb-recorder/configs/recorder.yaml`**
```yaml
# Storage configuration
# GCS: use gs://bucket/path URI. Auth via GOOGLE_APPLICATION_CREDENTIALS env var.
# Local: use a filesystem path like /tmp/recordings
storage_uri: "gs://bubbaloop-recordings/data"

# Topics to record
topics:
  - "/camera/entrance/compressed"
  - "/camera/terrace/compressed"
  - "/weather/current"

# Performance tuning
batch_size: 30
flush_interval_secs: 5
```

**`crates/bubbaloop-nodes/lancedb-recorder/src/lib.rs`**
```rust
pub mod config;
pub mod error;
pub mod lancedb_client;
pub mod recorder_node;
pub mod session;
pub mod zenoh_api;
```

#### Module Signatures (Phase 1 stubs)

**`src/error.rs`** -- Error types
```rust
#[derive(Debug, thiserror::Error)]
pub enum RecorderError {
    #[error("LanceDB error: {0}")]
    LanceDb(String),
    #[error("Zenoh error: {0}")]
    Zenoh(String),
    #[error("Config error: {0}")]
    Config(String),
    #[error("Session error: {0}")]
    Session(String),
    #[error("Arrow error: {0}")]
    Arrow(String),
    #[error("IO error: {0}")]
    Io(#[from] std::io::Error),
    #[error("JSON error: {0}")]
    Json(#[from] serde_json::Error),
}

pub type Result<T> = std::result::Result<T, RecorderError>;
```

**`src/config.rs`** -- Configuration parsing
```rust
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RecorderConfig {
    pub storage_uri: String,
    pub topics: Vec<String>,
    #[serde(default = "default_batch_size")]
    pub batch_size: usize,
    #[serde(default = "default_flush_interval")]
    pub flush_interval_secs: u64,
}

fn default_batch_size() -> usize { 30 }
fn default_flush_interval() -> u64 { 5 }

impl RecorderConfig {
    pub fn from_file(path: impl AsRef<std::path::Path>) -> crate::error::Result<Self> { ... }
}
```

**`src/bin/lancedb_recorder.rs`** -- Entry point (mirrors mcap_recorder.rs pattern)
```rust
// Args struct with argh: --config, --storage-uri overrides
// main(): init logging, parse args, load config, create Zenoh context + session,
//         start health heartbeat on bubbaloop/{scope}/{machine_id}/health/lancedb-recorder,
//         create LanceDbClient, create RecorderNode, run
```

#### Acceptance Criteria
- [ ] `cd crates/bubbaloop-nodes/lancedb-recorder && cargo check` passes
- [ ] Config YAML parsing works with unit tests
- [ ] Error types compile and display correctly
- [ ] Binary starts up, connects to Zenoh, publishes health heartbeat
- [ ] `Cargo.lock` is generated and committed

#### Tests
- `config.rs`: test_parse_valid_config, test_parse_minimal_config, test_parse_missing_storage_uri, test_default_batch_size, test_default_flush_interval
- `error.rs`: test_error_display (verify all variants produce meaningful messages)

---

### Phase 2: LanceDB Client and Schema

**Goal:** Implement the LanceDB connection, table creation, and batch insert logic.

#### Files to Create/Modify

**`src/lancedb_client.rs`** -- LanceDB connection and operations

```rust
use arrow_schema::{DataType, Field, Schema};
use arrow_array::RecordBatch;
use lancedb::Connection;

pub struct LanceDbClient {
    connection: Connection,
    frames_table_name: String,
    weather_table_name: String,
    sessions_table_name: String,
}

impl LanceDbClient {
    /// Connect to LanceDB backend.
    /// uri: "gs://bucket/path" for GCS, or local filesystem path.
    /// GCS auth: reads GOOGLE_APPLICATION_CREDENTIALS env var automatically.
    pub async fn connect(uri: &str) -> Result<Self> { ... }

    /// Create tables if they don't exist
    pub async fn ensure_tables(&self) -> Result<()> { ... }

    /// Insert a batch of frame records
    /// Each FrameRecord: { timestamp_ns: i64, topic: String, camera_id: String,
    ///                      format: String, seq_num: u64, frame_data: Vec<u8>,
    ///                      session_id: String }
    pub async fn insert_frames(&self, frames: &[FrameRecord]) -> Result<usize> { ... }

    /// Insert a batch of weather records
    pub async fn insert_weather(&self, records: &[WeatherRecord]) -> Result<usize> { ... }

    /// Create a new session record
    pub async fn create_session(&self, session: &SessionRecord) -> Result<()> { ... }

    /// Update session end_time and frame_count.
    /// Uses table.update() with a filter matching session_id, NOT SQL UPDATE.
    /// LanceDB update API: table.update().only_if("session_id = '{id}'")
    ///     .column("end_time_ns", lit(value))
    ///     .column("frame_count", lit(value))
    ///     .execute().await
    pub async fn update_session(&self, session_id: &str, end_time: i64, frame_count: u64, weather_count: u64) -> Result<()> { ... }

    /// List all sessions
    pub async fn list_sessions(&self) -> Result<Vec<SessionRecord>> { ... }

    /// Query frames for a session with pagination
    pub async fn query_frames(
        &self,
        session_id: &str,
        offset: usize,
        limit: usize,
    ) -> Result<Vec<FrameRecord>> { ... }

    /// Get a single frame by session_id and sequence number
    pub async fn get_frame(&self, session_id: &str, seq_num: u64) -> Result<Option<FrameRecord>> { ... }

    /// Query weather for a session with time range
    pub async fn query_weather(
        &self,
        session_id: &str,
        start_time: Option<i64>,
        end_time: Option<i64>,
    ) -> Result<Vec<WeatherRecord>> { ... }
}

/// Arrow schema for frames table
fn frames_schema() -> Schema {
    Schema::new(vec![
        Field::new("timestamp_ns", DataType::Int64, false),
        Field::new("topic", DataType::Utf8, false),
        Field::new("camera_id", DataType::Utf8, false),
        Field::new("format", DataType::Utf8, false),
        Field::new("seq_num", DataType::UInt64, false),
        Field::new("frame_data", DataType::LargeBinary, false),
        Field::new("session_id", DataType::Utf8, false),
    ])
}

/// Arrow schema for weather table
/// Fields extracted from CurrentWeather protobuf (weather.proto)
fn weather_schema() -> Schema {
    Schema::new(vec![
        Field::new("timestamp_ns", DataType::Int64, false),
        Field::new("temperature_2m", DataType::Float64, true),
        Field::new("relative_humidity_2m", DataType::Float64, true),
        Field::new("apparent_temperature", DataType::Float64, true),
        Field::new("wind_speed_10m", DataType::Float64, true),
        Field::new("wind_direction_10m", DataType::Float64, true),
        Field::new("pressure_msl", DataType::Float64, true),
        Field::new("cloud_cover", DataType::Float64, true),
        Field::new("weather_code", DataType::Int32, true),
        Field::new("is_day", DataType::UInt32, true),
        Field::new("session_id", DataType::Utf8, false),
    ])
}

/// Arrow schema for sessions table
fn sessions_schema() -> Schema {
    Schema::new(vec![
        Field::new("session_id", DataType::Utf8, false),
        Field::new("session_name", DataType::Utf8, true),
        Field::new("start_time_ns", DataType::Int64, false),
        Field::new("end_time_ns", DataType::Int64, true),
        Field::new("topics", DataType::Utf8, false),       // JSON array string
        Field::new("frame_count", DataType::UInt64, false),
        Field::new("weather_count", DataType::UInt64, false),
        Field::new("machine_id", DataType::Utf8, true),
    ])
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FrameRecord { ... }

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WeatherRecord { ... }

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SessionRecord { ... }
```

**Key implementation details for `insert_frames`:**
1. Build `Vec<Arc<dyn Array>>` columns from the `FrameRecord` slice
2. Create `RecordBatch::try_new(Arc::new(frames_schema()), columns)`
3. Use `table.add(RecordBatchIterator::new(vec![Ok(batch)], schema_ref)).execute().await`
4. The `frame_data` column uses `LargeBinaryArray::from_iter_values(data_iter)`

**Weather data extraction from CurrentWeather protobuf:**
```rust
// In the subscription handler for /weather/current:
fn weather_from_proto(msg: &CurrentWeather, session_id: &str) -> WeatherRecord {
    let ts = msg.header.as_ref().map(|h| h.acq_time as i64).unwrap_or(0);
    WeatherRecord {
        timestamp_ns: ts,
        temperature_2m: msg.temperature_2m,
        relative_humidity_2m: msg.relative_humidity_2m,
        apparent_temperature: msg.apparent_temperature,
        wind_speed_10m: msg.wind_speed_10m,
        wind_direction_10m: msg.wind_direction_10m,
        pressure_msl: msg.pressure_msl,
        cloud_cover: msg.cloud_cover,
        weather_code: msg.weather_code as i32,
        is_day: msg.is_day,
        session_id: session_id.to_string(),
    }
}
```

**Key implementation detail for GCS connection:**
```rust
// GCS auth is picked up automatically from GOOGLE_APPLICATION_CREDENTIALS env var.
// For explicit service account, pass as storage option:
let db = lancedb::connect(uri)
    .execute()
    .await
    .map_err(|e| RecorderError::LanceDb(e.to_string()))?;
```

**Table creation with indices:**
```rust
// After creating tables, add scalar indices for query performance
table.create_index(&["timestamp_ns"], lancedb::index::Index::BTree)
    .execute().await?;
table.create_index(&["session_id"], lancedb::index::Index::BTree)
    .execute().await?;
```

#### Acceptance Criteria
- [ ] `LanceDbClient::connect` succeeds with valid GCS credentials
- [ ] Tables created with correct Arrow schemas
- [ ] Batch insert of 30 frame records completes in <2s
- [ ] Query by session_id returns correct results
- [ ] `update_session` uses filter-based row matching (not SQL UPDATE)
- [ ] Graceful error handling when GCS is unreachable

#### Tests
- `lancedb_client.rs`: test_frames_schema_fields, test_weather_schema_fields, test_sessions_schema_fields
- `lancedb_client.rs`: test_frame_record_serialization, test_weather_record_serialization, test_weather_from_proto_extraction
- Integration tests (require GCS credentials, mark with `#[ignore]`): test_connect_and_create_tables, test_insert_and_query_frames

---

### Phase 3: Recording Logic and Session Management

**Goal:** Wire up Zenoh subscriptions to LanceDB batch inserts, with session lifecycle.

#### Files to Create/Modify

**`src/session.rs`** -- Session state machine
```rust
use uuid::Uuid;

#[derive(Debug, Clone, PartialEq)]
pub enum RecordingState {
    Idle,
    Recording { session_id: String, start_time: i64, frame_count: u64, weather_count: u64 },
    Stopping { session_id: String },
}

/// Async-safe recorder state. Uses tokio::sync::RwLock matching the
/// NodeManager pattern in node_manager.rs (line 124).
pub struct RecorderState {
    state: RecordingState,
    machine_id: String,
}

impl RecorderState {
    pub fn new(machine_id: String) -> Self { ... }
    pub fn start_recording(&mut self, session_name: Option<String>, topics: &[String]) -> Result<SessionRecord> { ... }
    pub fn stop_recording(&mut self) -> Result<SessionRecord> { ... }
    pub fn is_recording(&self) -> bool { ... }
    pub fn current_session_id(&self) -> Option<&str> { ... }
    pub fn increment_frame_count(&mut self) { ... }
    pub fn increment_weather_count(&mut self) { ... }
    pub fn state(&self) -> &RecordingState { ... }
}
```

**`src/recorder_node.rs`** -- Main recording logic
```rust
use tokio::sync::RwLock;

pub struct LanceDbRecorderNode {
    node: Arc<ZNode>,
    zenoh_session: Arc<zenoh::Session>,
    topics: Vec<String>,
    lancedb: Arc<LanceDbClient>,
    recorder_state: Arc<RwLock<RecorderState>>,
    batch_size: usize,
    flush_interval: Duration,
}

impl LanceDbRecorderNode {
    pub fn new(
        node: Arc<ZNode>,
        zenoh_session: Arc<zenoh::Session>,
        topics: &[String],
        lancedb: Arc<LanceDbClient>,
        machine_id: &str,
        batch_size: usize,
        flush_interval: Duration,
    ) -> Result<Self> { ... }

    /// Run the node -- sets up command listener and waits for recording commands
    pub async fn run(self, shutdown_tx: tokio::sync::watch::Sender<()>) -> Result<()> {
        // 1. Set up Zenoh queryable for recording commands
        //    Topic: "bubbaloop/recorder/api/command"
        //    Payload: JSON {"command": "start"|"stop"|"status", "session_name": "...", "topics": [...]}
        //
        // 2. Set up Zenoh queryable for data queries (Phase 4)
        //    Topics: "bubbaloop/recorder/api/sessions", etc.
        //
        // 3. Main event loop:
        //    - Listen for commands
        //    - When "start": spawn subscription tasks, begin batching
        //    - When "stop": signal subscription tasks to stop, flush remaining batch, update session
        //    - On SIGTERM (shutdown signal): stop recording if active, flush, finalize, then exit
    }

    /// Start recording -- spawns topic subscription tasks
    async fn start_recording(&self, session_name: Option<String>, topics: Option<Vec<String>>) -> Result<String> {
        // 1. Create session via RecorderState (write lock: .write().await)
        // 2. Insert session record into LanceDB
        // 3. Spawn subscription tasks for each topic
        // 4. Spawn batch flush task (timer-based)
        // 5. Return session_id
    }

    /// Stop recording -- signals tasks, flushes buffers, updates session
    async fn stop_recording(&self) -> Result<()> {
        // 1. Signal subscription tasks to stop
        // 2. Flush remaining frame/weather buffers
        // 3. Update session end_time and counts in LanceDB (filter-based update)
        // 4. Update RecorderState (write lock: .write().await)
    }
}
```

**State access patterns (matching NodeManager):**
```rust
// Status queries use read lock:
let state = self.recorder_state.read().await;
let is_recording = state.is_recording();

// State mutations use write lock:
let mut state = self.recorder_state.write().await;
state.start_recording(name, &topics)?;
```

**Subscription task pattern (per topic):**
```rust
// For each topic, spawn a task that:
// 1. Subscribes to CompressedImage or CurrentWeather via ros-z
// 2. Decodes the protobuf header to extract timestamp, seq, frame_id
// 3. For weather topics: decode CurrentWeather and extract fields via weather_from_proto()
// 4. Sends FrameRecord or WeatherRecord to a flume channel
// 5. A separate batch writer task consumes from the channel
//    and calls insert_frames/insert_weather when batch_size is reached
//    or flush_interval expires (whichever comes first)
```

**Batch writer task:**
```rust
async fn batch_writer_task(
    rx: flume::Receiver<BatchItem>,
    lancedb: Arc<LanceDbClient>,
    recorder_state: Arc<RwLock<RecorderState>>,
    batch_size: usize,
    flush_interval: Duration,
    mut shutdown_rx: watch::Receiver<()>,
) -> Result<()> {
    let mut frame_buffer: Vec<FrameRecord> = Vec::with_capacity(batch_size);
    let mut weather_buffer: Vec<WeatherRecord> = Vec::with_capacity(batch_size);
    let mut flush_timer = tokio::time::interval(flush_interval);

    loop {
        tokio::select! {
            biased;
            _ = shutdown_rx.changed() => {
                // Flush remaining on shutdown
                flush_frames(&lancedb, &mut frame_buffer).await?;
                flush_weather(&lancedb, &mut weather_buffer).await?;
                break;
            }
            _ = flush_timer.tick() => {
                flush_frames(&lancedb, &mut frame_buffer).await?;
                flush_weather(&lancedb, &mut weather_buffer).await?;
            }
            Ok(item) = rx.recv_async() => {
                match item {
                    BatchItem::Frame(record) => {
                        frame_buffer.push(record);
                        if frame_buffer.len() >= batch_size {
                            flush_frames(&lancedb, &mut frame_buffer).await?;
                        }
                    }
                    BatchItem::Weather(record) => {
                        weather_buffer.push(record);
                        if weather_buffer.len() >= batch_size {
                            flush_weather(&lancedb, &mut weather_buffer).await?;
                        }
                    }
                }
                // Increment counts via write lock
                let mut state = recorder_state.write().await;
                state.increment_frame_count(); // or weather
            }
        }
    }
    Ok(())
}
```

#### Acceptance Criteria
- [ ] Start recording command creates session in LanceDB
- [ ] CompressedImage messages are batched and inserted as FrameRecords
- [ ] CurrentWeather messages are decoded and fields extracted into WeatherRecords
- [ ] Stop recording flushes buffers and updates session record (filter-based update)
- [ ] Status command returns current state (idle/recording with session info) via read lock
- [ ] Batch flush triggered by both size threshold and timer
- [ ] SIGTERM during recording: flush + finalize + exit (no data loss)

#### Tests
- `session.rs`: test_start_stop_lifecycle, test_cannot_start_while_recording, test_cannot_stop_while_idle, test_increment_counts, test_session_id_format
- `recorder_node.rs`: test_batch_item_enum, test_frame_record_from_compressed_image, test_weather_record_from_current_weather

---

### Phase 4: Recorder Zenoh Query API

**Goal:** Expose queryable endpoints for session browsing and frame retrieval.

#### Files to Create/Modify

**`src/zenoh_api.rs`** -- Zenoh queryable handlers for data access

**Design decision:** This API uses JSON payloads and JSON responses, consistent with the daemon's
REST-like Zenoh API in `crates/bubbaloop/src/daemon/zenoh_api.rs`. The daemon API uses JSON for
`health`, `nodes`, `nodes/{name}/command`, `nodes/{name}/logs`, etc. The recorder follows the
same pattern. The separate `bubbaloop/daemon/command` endpoint that uses protobuf is part of the
daemon's `zenoh_service.rs` pub/sub path, which is a different communication channel.

```rust
use tokio::sync::RwLock;

pub struct RecorderApiService {
    session: Arc<zenoh::Session>,
    lancedb: Arc<LanceDbClient>,
    recorder_state: Arc<RwLock<RecorderState>>,
    scope: String,
    machine_id: String,
}

// API Key Expressions:
//   bubbaloop/recorder/api/command           -- start/stop/status
//   bubbaloop/recorder/api/sessions          -- list all sessions
//   bubbaloop/recorder/api/sessions/{id}     -- get session details
//   bubbaloop/recorder/api/sessions/{id}/frames?offset=0&limit=20  -- frame metadata (no data)
//   bubbaloop/recorder/api/sessions/{id}/frames/{seq}              -- single frame with data
//   bubbaloop/recorder/api/sessions/{id}/weather                   -- weather time series

impl RecorderApiService {
    pub fn new(
        session: Arc<zenoh::Session>,
        lancedb: Arc<LanceDbClient>,
        recorder_state: Arc<RwLock<RecorderState>>,
        scope: String,
        machine_id: String,
    ) -> Self { ... }

    pub async fn run(self, mut shutdown: watch::Receiver<()>) -> Result<()> {
        // Declare queryable on "bubbaloop/recorder/api/**"
        // Route queries to handlers based on path
    }

    /// Handle: bubbaloop/recorder/api/command
    /// Payload: {"command": "start"|"stop"|"status", "session_name": "...", "topics": [...]}
    /// Response: {"success": true, "state": "recording"|"idle", "session_id": "...", "message": "..."}
    async fn handle_command(&self, query: &Query) -> String {
        // Status queries use read lock:
        //   let state = self.recorder_state.read().await;
        // Start/stop use write lock:
        //   let mut state = self.recorder_state.write().await;
    }

    /// Handle: bubbaloop/recorder/api/sessions
    /// Response: {"sessions": [...], "count": N}
    async fn handle_list_sessions(&self, _query: &Query) -> String { ... }

    /// Handle: bubbaloop/recorder/api/sessions/{id}
    /// Response: SessionRecord as JSON
    async fn handle_get_session(&self, session_id: &str) -> String { ... }

    /// Handle: bubbaloop/recorder/api/sessions/{id}/frames
    /// Query params via payload: {"offset": 0, "limit": 20}
    /// Response: {"frames": [{"timestamp_ns":..., "topic":..., "seq_num":..., "format":...}], "total": N}
    /// NOTE: frame_data NOT included (too large). Client fetches individual frames.
    async fn handle_list_frames(&self, session_id: &str, query: &Query) -> String { ... }

    /// Handle: bubbaloop/recorder/api/sessions/{id}/frames/{seq}
    /// Response: raw frame bytes (binary payload, not JSON)
    async fn handle_get_frame(&self, session_id: &str, seq_num: u64) -> Result<Vec<u8>> { ... }

    /// Handle: bubbaloop/recorder/api/sessions/{id}/weather
    /// Response: {"weather": [...], "count": N}
    async fn handle_get_weather(&self, session_id: &str, query: &Query) -> String { ... }
}
```

**Response format examples:**

Command response:
```json
{"success": true, "state": "recording", "session_id": "abc-123", "message": "Recording started"}
```

Sessions list:
```json
{
  "sessions": [
    {
      "session_id": "abc-123",
      "session_name": "test-run-1",
      "start_time_ns": 1706700000000000000,
      "end_time_ns": 1706703600000000000,
      "topics": ["/camera/entrance/compressed", "/weather/current"],
      "frame_count": 5400,
      "weather_count": 720,
      "machine_id": "jetson-01"
    }
  ],
  "count": 1
}
```

#### Acceptance Criteria
- [ ] `bubbaloop/recorder/api/command` handles start/stop/status with JSON payloads
- [ ] Sessions list returns all recorded sessions from LanceDB
- [ ] Frame list returns metadata without binary data
- [ ] Single frame endpoint returns raw bytes
- [ ] Weather endpoint returns time series data
- [ ] All responses are valid JSON (except raw frame endpoint)
- [ ] Status queries use `.read().await`, mutations use `.write().await`

#### Tests
- `zenoh_api.rs`: test_parse_command_payload, test_command_response_format, test_session_response_format, test_frame_list_response_format

---

### Phase 5: CLI `bubbaloop record` Subcommand

**Goal:** Add recording control commands to the bubbaloop CLI binary. Extract shared `get_zenoh_session()` function.

#### Prerequisite: Extract get_zenoh_session()

The `get_zenoh_session()` function currently lives as a private function in
`crates/bubbaloop/src/cli/node.rs` (line 329). It needs to be shared with the new `record.rs` module.

**Option chosen:** Create `crates/bubbaloop/src/cli/zenoh.rs` with the shared function, then import
it in both `node.rs` and `record.rs`.

**`crates/bubbaloop/src/cli/zenoh.rs`** -- Shared Zenoh session helper
```rust
//! Shared Zenoh session utilities for CLI commands

use thiserror::Error;

#[derive(Debug, Error)]
pub enum ZenohSessionError {
    #[error("Zenoh error: {0}")]
    Zenoh(String),
}

/// Create a Zenoh session configured for CLI use.
/// Connects to local router in client mode with scouting disabled.
pub async fn get_zenoh_session() -> Result<zenoh::Session, ZenohSessionError> {
    let mut config = zenoh::Config::default();

    config
        .insert_json5("mode", "\"client\"")
        .map_err(|e| ZenohSessionError::Zenoh(e.to_string()))?;

    let endpoint = std::env::var("BUBBALOOP_ZENOH_ENDPOINT")
        .unwrap_or_else(|_| "tcp/127.0.0.1:7447".to_string());
    config
        .insert_json5("connect/endpoints", &format!("[\"{}\"]", endpoint))
        .map_err(|e| ZenohSessionError::Zenoh(e.to_string()))?;

    config
        .insert_json5("scouting/multicast/enabled", "false")
        .map_err(|e| ZenohSessionError::Zenoh(e.to_string()))?;
    config
        .insert_json5("scouting/gossip/enabled", "false")
        .map_err(|e| ZenohSessionError::Zenoh(e.to_string()))?;

    zenoh::open(config)
        .await
        .map_err(|e| ZenohSessionError::Zenoh(e.to_string()))
}
```

#### Files to Modify (get_zenoh_session extraction)

**`crates/bubbaloop/src/cli/mod.rs`** -- Add zenoh module

Current contents:
```rust
pub mod debug;
pub mod doctor;
pub mod node;
pub mod status;

pub use debug::{DebugCommand, DebugError};
pub use node::{NodeCommand, NodeError};
```

Change to:
```rust
pub mod debug;
pub mod doctor;
pub mod node;
pub mod record;
pub mod status;
pub mod zenoh;

pub use debug::{DebugCommand, DebugError};
pub use node::{NodeCommand, NodeError};
pub use record::{RecordCommand, RecordError};
```

**`crates/bubbaloop/src/cli/node.rs`** -- Replace private get_zenoh_session with import

Remove the private `async fn get_zenoh_session() -> Result<zenoh::Session>` (lines 329-356).
Replace all call sites with:
```rust
use crate::cli::zenoh::get_zenoh_session;

// In each function that calls get_zenoh_session():
let session = get_zenoh_session()
    .await
    .map_err(|e| NodeError::Zenoh(e.to_string()))?;
```

#### Files to Create

**`crates/bubbaloop/src/cli/record.rs`** -- New CLI module
```rust
use argh::FromArgs;
use serde::{Deserialize, Serialize};
use thiserror::Error;
use zenoh::query::QueryTarget;

use crate::cli::zenoh::get_zenoh_session;

#[derive(Debug, Error)]
pub enum RecordError {
    #[error("Zenoh error: {0}")]
    Zenoh(String),
    #[error("Recorder error: {0}")]
    Recorder(String),
    #[error("JSON error: {0}")]
    Json(#[from] serde_json::Error),
}

pub type Result<T> = std::result::Result<T, RecordError>;

/// Recording commands
#[derive(FromArgs)]
#[argh(subcommand, name = "record")]
pub struct RecordCommand {
    #[argh(subcommand)]
    action: RecordAction,
}

#[derive(FromArgs)]
#[argh(subcommand)]
enum RecordAction {
    Start(StartArgs),
    Stop(StopArgs),
    Status(StatusArgs),
    List(ListArgs),
}

/// Start recording
#[derive(FromArgs)]
#[argh(subcommand, name = "start")]
struct StartArgs {
    /// topics to record (comma-separated)
    #[argh(option, short = 't')]
    topics: Option<String>,

    /// storage URI (overrides recorder config)
    #[argh(option, short = 'b')]
    storage_uri: Option<String>,

    /// session name (default: auto-generated timestamp)
    #[argh(option, short = 'n')]
    session_name: Option<String>,
}

/// Stop recording
#[derive(FromArgs)]
#[argh(subcommand, name = "stop")]
struct StopArgs {}

/// Show recording status
#[derive(FromArgs)]
#[argh(subcommand, name = "status")]
struct StatusArgs {}

/// List recorded sessions
#[derive(FromArgs)]
#[argh(subcommand, name = "list")]
struct ListArgs {
    /// output format: table, json (default: table)
    #[argh(option, short = 'f', default = "String::from(\"table\")")]
    format: String,
}

impl RecordCommand {
    pub async fn run(self) -> Result<()> {
        match self.action {
            RecordAction::Start(args) => start_recording(args).await,
            RecordAction::Stop(_) => stop_recording().await,
            RecordAction::Status(_) => recording_status().await,
            RecordAction::List(args) => list_sessions(args).await,
        }
    }
}

async fn send_recorder_command(payload: serde_json::Value) -> Result<serde_json::Value> {
    let session = get_zenoh_session()
        .await
        .map_err(|e| RecordError::Zenoh(e.to_string()))?;
    let payload_str = serde_json::to_string(&payload)?;

    // Query "bubbaloop/recorder/api/command" with retry (3 attempts, 5s timeout)
    // On timeout: "LanceDB recorder not running. Start it with: bubbaloop node start lancedb-recorder"
    // Parse JSON response
    // Return parsed response
    // session.close() before return
}

async fn start_recording(args: StartArgs) -> Result<()> {
    let mut cmd = serde_json::json!({ "command": "start" });
    if let Some(topics) = args.topics {
        cmd["topics"] = serde_json::json!(topics.split(',').collect::<Vec<_>>());
    }
    if let Some(uri) = args.storage_uri {
        cmd["storage_uri"] = serde_json::json!(uri);
    }
    if let Some(name) = args.session_name {
        cmd["session_name"] = serde_json::json!(name);
    }

    let response = send_recorder_command(cmd).await?;
    if response["success"].as_bool().unwrap_or(false) {
        println!("Recording started");
        println!("  Session: {}", response["session_id"].as_str().unwrap_or("unknown"));
    } else {
        eprintln!("Failed: {}", response["message"].as_str().unwrap_or("unknown error"));
    }
    Ok(())
}

async fn stop_recording() -> Result<()> { ... }
async fn recording_status() -> Result<()> { ... }
async fn list_sessions(args: ListArgs) -> Result<()> { ... }
```

#### Files to Modify (bubbaloop.rs integration)

**`crates/bubbaloop/src/bin/bubbaloop.rs`** -- Register Record subcommand

Current import line (line 22):
```rust
use bubbaloop::cli::{DebugCommand, NodeCommand};
```
Change to:
```rust
use bubbaloop::cli::{DebugCommand, NodeCommand, RecordCommand};
```

Current Command enum (lines 37-44):
```rust
enum Command {
    Tui(TuiArgs),
    Status(StatusArgs),
    Doctor(DoctorArgs),
    Daemon(DaemonArgs),
    Node(NodeCommand),
    Debug(DebugCommand),
}
```
Change to:
```rust
enum Command {
    Tui(TuiArgs),
    Status(StatusArgs),
    Doctor(DoctorArgs),
    Daemon(DaemonArgs),
    Node(NodeCommand),
    Record(RecordCommand),
    Debug(DebugCommand),
}
```

Current match block -- add after the `Command::Node` arm (after line 148):
```rust
        Some(Command::Record(cmd)) => {
            cmd.run()
                .await
                .map_err(|e| Box::new(e) as Box<dyn std::error::Error>)?;
        }
```

Current help text -- add after the `node` help (after line 122):
```rust
            eprintln!("  record    Control recording:");
            eprintln!("              start, stop, status, list");
```

#### Acceptance Criteria
- [ ] `pixi run check` passes with the new subcommand
- [ ] `get_zenoh_session()` extracted to `cli/zenoh.rs` and imported in both `node.rs` and `record.rs`
- [ ] `bubbaloop record start` sends start command to recorder via JSON Zenoh query
- [ ] `bubbaloop record stop` sends stop command
- [ ] `bubbaloop record status` shows current recorder state
- [ ] `bubbaloop record list` displays sessions in table format
- [ ] `bubbaloop record list -f json` outputs JSON
- [ ] Handles recorder-not-running gracefully (timeout message)
- [ ] All existing tests in `cli/node.rs` still pass

#### Tests
- `record.rs`: test_start_args_serialization, test_stop_args_serialization, test_session_list_deserialization, test_table_formatting
- `zenoh.rs`: (no tests needed -- it's a thin config wrapper, tested indirectly)

---

### Phase 5.5: systemd EnvironmentFile Support

**Goal:** Add `EnvironmentFile` directive support to the systemd unit generator so that nodes
like lancedb-recorder can load GCS credentials when running as a service.

#### Rationale
Currently `generate_service_unit()` in `systemd.rs` has no mechanism for custom env vars.
When lancedb-recorder runs as a systemd service, it needs `GOOGLE_APPLICATION_CREDENTIALS`
for GCS access. The solution is to support an optional `EnvironmentFile` directive that loads
from `~/.bubbaloop/env/{node-name}.env`.

#### Files to Modify

**`crates/bubbaloop/src/daemon/systemd.rs`** -- Add EnvironmentFile to generated unit

In `generate_service_unit()`, after the `Environment=` lines, add:
```rust
// Optional per-node environment file for custom env vars (e.g., GCS credentials)
let env_file_path = home.join(format!(".bubbaloop/env/{}.env", name));
let env_file_line = if env_file_path.exists() {
    format!("\nEnvironmentFile={}", env_file_path.display())
} else {
    // Use the - prefix so systemd doesn't fail if file doesn't exist
    format!("\nEnvironmentFile=-{}", env_file_path.display())
};
```

Also, for the lancedb-recorder specifically (detected by checking if the node name is
lancedb-recorder or if a config flag is set), add `TimeoutStopSec=120` to allow buffer flush
on shutdown. The simplest approach: always add `TimeoutStopSec=120` for all nodes (safe default,
systemd's default is 90s anyway).

#### Acceptance Criteria
- [ ] Generated systemd unit includes `EnvironmentFile=-~/.bubbaloop/env/{name}.env`
- [ ] Generated systemd unit includes `TimeoutStopSec=120`
- [ ] Existing tests still pass
- [ ] `pixi run clippy` passes

#### Tests
- `systemd.rs`: test_generate_unit_includes_env_file, test_generate_unit_includes_timeout_stop

---

### Phase 6: Dashboard Record Button

**Goal:** Add a recording control button to the dashboard header area.

#### Files to Create

**`dashboard/src/components/RecordButton.tsx`** -- Recording control component

Uses the JSON Zenoh query pattern (NOT protobuf). This is consistent with the recorder's
REST-like JSON API.

```tsx
import { useState, useEffect, useCallback, useRef } from 'react';
import { useZenohSubscriptionContext } from '../contexts/ZenohSubscriptionContext';
import { Duration } from 'typed-duration';
import { Reply, ReplyError, Sample } from '@eclipse-zenoh/zenoh-ts';

interface RecorderStatus {
  success: boolean;
  state: 'idle' | 'recording';
  session_id?: string;
  message?: string;
}

export function RecordButton() {
  const { getSession } = useZenohSubscriptionContext();
  const [isRecording, setIsRecording] = useState(false);
  const [sessionId, setSessionId] = useState<string | null>(null);
  const [duration, setDuration] = useState(0);
  const [loading, setLoading] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const startTimeRef = useRef<number | null>(null);
  const timerRef = useRef<ReturnType<typeof setInterval> | null>(null);

  // Send JSON command to recorder via Zenoh query
  const sendRecorderCommand = useCallback(async (cmd: object): Promise<RecorderStatus | null> => {
    const session = getSession();
    if (!session) return null;

    try {
      const payload = new TextEncoder().encode(JSON.stringify(cmd));
      const receiver = await session.get('bubbaloop/recorder/api/command', {
        payload: payload,
        timeout: Duration.milliseconds.of(5000),
      });

      if (receiver) {
        for await (const replyItem of receiver) {
          if (replyItem instanceof Reply) {
            const replyResult = replyItem.result();
            if (replyResult instanceof ReplyError) continue;
            const sample = replyResult as Sample;
            const replyPayload = sample.payload().toBytes();
            const text = new TextDecoder().decode(replyPayload);
            return JSON.parse(text) as RecorderStatus;
          }
          break;
        }
      }
    } catch (err) {
      console.warn('[RecordButton] Command failed:', err);
    }
    return null;
  }, [getSession]);

  // Poll status every 3s
  useEffect(() => {
    let mounted = true;
    let pollTimer: ReturnType<typeof setTimeout> | null = null;

    const pollStatus = async () => {
      if (!mounted) return;
      const status = await sendRecorderCommand({ command: 'status' });
      if (status && mounted) {
        setIsRecording(status.state === 'recording');
        setSessionId(status.session_id ?? null);
        setError(null);
      }
      if (mounted) {
        pollTimer = setTimeout(pollStatus, 3000);
      }
    };

    pollStatus();
    return () => {
      mounted = false;
      if (pollTimer) clearTimeout(pollTimer);
    };
  }, [sendRecorderCommand]);

  // Duration timer
  useEffect(() => {
    if (isRecording) {
      if (!startTimeRef.current) startTimeRef.current = Date.now();
      timerRef.current = setInterval(() => {
        setDuration(Math.floor((Date.now() - (startTimeRef.current ?? Date.now())) / 1000));
      }, 1000);
    } else {
      startTimeRef.current = null;
      setDuration(0);
      if (timerRef.current) clearInterval(timerRef.current);
    }
    return () => { if (timerRef.current) clearInterval(timerRef.current); };
  }, [isRecording]);

  const handleClick = async () => {
    setLoading(true);
    setError(null);
    const cmd = isRecording ? { command: 'stop' } : { command: 'start' };
    const result = await sendRecorderCommand(cmd);
    if (result && !result.success) {
      setError(result.message ?? 'Unknown error');
    }
    setLoading(false);
  };

  const formatDuration = (secs: number) => {
    const h = Math.floor(secs / 3600).toString().padStart(2, '0');
    const m = Math.floor((secs % 3600) / 60).toString().padStart(2, '0');
    const s = (secs % 60).toString().padStart(2, '0');
    return `${h}:${m}:${s}`;
  };

  // UI:
  //   Not recording: gray circle button with "REC" label
  //   Recording: red pulsing circle + "REC 00:05:32" duration text
  //   Loading: spinner in place of circle
  //   Error: tooltip with error message
  return ( /* ... JSX ... */ );
}
```

#### Files to Modify

**`dashboard/src/App.tsx`** -- Add RecordButton to header

Insert `<RecordButton />` between the `header-left` div and the `StatusIndicator`:

Current (line 197-201):
```tsx
        <div className="header-left">
          <h1>Bubbaloop</h1>
          <span className="header-subtitle">Dashboard</span>
        </div>
        <StatusIndicator status={status} endpoint={ZENOH_ENDPOINT} onReconnect={reconnect} />
```

Change to:
```tsx
        <div className="header-left">
          <h1>Bubbaloop</h1>
          <span className="header-subtitle">Dashboard</span>
        </div>
        {session && (
          <ZenohSubscriptionProvider session={session}>
            <RecordButton />
          </ZenohSubscriptionProvider>
        )}
        <StatusIndicator status={status} endpoint={ZENOH_ENDPOINT} onReconnect={reconnect} />
```

Note: The `RecordButton` needs to be inside a `ZenohSubscriptionProvider` to access the session.
Since the header renders before the main `ZenohSubscriptionProvider`, we conditionally wrap it.
Alternatively, move the `RecordButton` import and render it inside the existing `ZenohSubscriptionProvider`
block and use CSS absolute positioning in the header. The implementer should choose the cleaner approach.

Add import at top of `App.tsx`:
```tsx
import { RecordButton } from './components/RecordButton';
```

#### Acceptance Criteria
- [ ] Record button visible in dashboard header, between title and status indicator
- [ ] Click starts recording (sends JSON Zenoh command, shows red indicator)
- [ ] Click again stops recording (sends stop command, returns to gray)
- [ ] Duration counter increments while recording
- [ ] Status polling reflects external start/stop (e.g., via CLI)
- [ ] Graceful handling when recorder node not running
- [ ] Uses `TextEncoder` for JSON payloads and `JSON.parse` for responses (not protobuf)

#### Tests
- No Rust tests needed for this phase
- Manual verification: start via button, stop via CLI -- button should update

---

### Phase 7: Dashboard Recordings Panel

**Goal:** New panel type for browsing and playing back recorded data.

#### Dependencies to Add

**`dashboard/package.json`** -- Add typed-duration if not already present

`typed-duration` is already used by `NodesView.tsx` (imported as `import { Duration } from 'typed-duration'`).
Verify it exists in `package.json` dependencies. If not, add:
```json
"typed-duration": "^2.0.0"
```

#### Files to Create

**`dashboard/src/components/RecordingsView.tsx`** -- Main recordings panel component

Uses the same JSON Zenoh query pattern as RecordButton:

```tsx
import { useState, useEffect, useCallback } from 'react';
import { useZenohSubscriptionContext } from '../contexts/ZenohSubscriptionContext';
import { Duration } from 'typed-duration';
import { Reply, ReplyError, Sample } from '@eclipse-zenoh/zenoh-ts';

interface SessionInfo {
  session_id: string;
  session_name: string | null;
  start_time_ns: number;
  end_time_ns: number | null;
  topics: string[];
  frame_count: number;
  weather_count: number;
  machine_id: string | null;
}

interface FrameInfo {
  timestamp_ns: number;
  topic: string;
  camera_id: string;
  format: string;
  seq_num: number;
}

// Helper to send JSON query to recorder API
async function queryRecorderApi(
  session: /* zenoh Session */,
  path: string,
  payload?: object,
): Promise<unknown> {
  const opts: { timeout: Duration; payload?: Uint8Array } = {
    timeout: Duration.milliseconds.of(5000),
  };
  if (payload) {
    opts.payload = new TextEncoder().encode(JSON.stringify(payload));
  }

  const receiver = await session.get(`bubbaloop/recorder/api/${path}`, opts);
  if (receiver) {
    for await (const replyItem of receiver) {
      if (replyItem instanceof Reply) {
        const replyResult = replyItem.result();
        if (replyResult instanceof ReplyError) continue;
        const sample = replyResult as Sample;
        const replyPayload = sample.payload().toBytes();
        const text = new TextDecoder().decode(replyPayload);
        return JSON.parse(text);
      }
      break;
    }
  }
  return null;
}

// State:
//   sessions: SessionInfo[]
//   selectedSession: string | null
//   frames: FrameInfo[]
//   weather: WeatherInfo[]
//   selectedFrame: number | null
//   currentFrameData: Uint8Array | null
//   playbackPosition: number
//   isPlaying: boolean
//
// Sub-components:
//   SessionList     -- left panel: list sessions, click to select
//   FrameTimeline   -- bottom: scrubber bar, play/pause, frame counter
//   FrameViewer     -- center: displays H264 frame (reuse CameraView decoder?)
//   WeatherChart    -- right panel: temperature/humidity line chart
//
// Data flow:
//   1. On mount: queryRecorderApi(session, 'sessions') -> populate SessionList
//   2. On session select: queryRecorderApi(session, `sessions/${id}/frames`, {offset:0, limit:100})
//   3. On frame select: queryRecorderApi(session, `sessions/${id}/frames/${seq}`) -> raw bytes
//   4. On session select: queryRecorderApi(session, `sessions/${id}/weather`) -> chart data
```

**`dashboard/src/components/SortableRecordingsCard.tsx`** -- DnD wrapper
```tsx
// Same pattern as SortableNodesCard.tsx:
// Wraps RecordingsView with useSortable() from @dnd-kit/sortable
```

#### Files to Modify

**`dashboard/src/lib/storage.ts`** -- Add recordings panel type

Change `PanelType` (line 13):
```typescript
export type PanelType = 'camera' | 'json' | 'rawdata' | 'weather' | 'stats' | 'nodes' | 'recordings';
```

Add interface (after `NodesPanelConfig`):
```typescript
export interface RecordingsPanelConfig extends BasePanelConfig {
  type: 'recordings';
}
```

Update `PanelConfig` union (line 46):
```typescript
export type PanelConfig = CameraPanelConfig | JsonPanelConfig | RawDataPanelConfig | WeatherPanelConfig | StatsPanelConfig | NodesPanelConfig | RecordingsPanelConfig;
```

Update `generatePanelId` prefixes (line 151-158):
```typescript
const prefixes: Record<PanelType, string> = {
    camera: 'cam',
    json: 'json',
    rawdata: 'rawdata',
    weather: 'weather',
    stats: 'stats',
    nodes: 'nodes',
    recordings: 'rec',
};
```

**`dashboard/src/components/Dashboard.tsx`** -- Register recordings panel
```tsx
// Add import: import { SortableRecordingsCard } from './SortableRecordingsCard';

// Add to addPanel switch:
case 'recordings':
  newPanel = {
    id: newId,
    name: `Recordings ${count}`,
    topic: '',
    type: 'recordings',
  };
  break;

// Add to panel rendering switch:
case 'recordings':
  return (
    <SortableRecordingsCard
      key={panel.id}
      id={panel.id}
      panelName={panel.name}
      isHidden={isHidden}
      onRemove={() => removePanel(panel.id)}
    />
  );

// Add "Add Recordings" button in both the dropdown menu and empty-state buttons
```

#### Acceptance Criteria
- [ ] "Recordings" panel type appears in Add Panel menu
- [ ] Panel lists recorded sessions from the recorder
- [ ] Selecting a session shows frame metadata and weather data
- [ ] Frame timeline scrubber allows seeking through frames
- [ ] Individual frames can be displayed (if H264 decode works)
- [ ] Weather data displayed as text/simple visualization
- [ ] Dashboard builds with no TypeScript errors (`npm run build`)
- [ ] All Zenoh queries use JSON (TextEncoder/JSON.parse pattern)

#### Tests
- No Rust tests needed for this phase
- Manual verification: create recording via CLI, browse in dashboard panel

---

## Commit Strategy

| Commit | Phase | Message |
|--------|-------|---------|
| 1 | Phase 0 | `test: validate lancedb 0.23 compiles on aarch64` |
| 2 | Phase 1 | `feat: add lancedb-recorder node skeleton with config and error types` |
| 3 | Phase 2 | `feat: implement LanceDB client with Arrow schemas and GCS backend` |
| 4 | Phase 3 | `feat: add recording logic with session management and batch inserts` |
| 5 | Phase 4 | `feat: expose recorder Zenoh queryable API for sessions and frames` |
| 6 | Phase 5 | `feat: add bubbaloop record CLI subcommand and extract shared zenoh helper` |
| 7 | Phase 5.5 | `feat: add EnvironmentFile and TimeoutStopSec to systemd unit generator` |
| 8 | Phase 6 | `feat: add recording control button to dashboard` |
| 9 | Phase 7 | `feat: add recordings panel to dashboard for browsing recorded data` |

---

## Risk Identification

### Risk 1: LanceDB aarch64 Compilation
**Likelihood:** Medium
**Impact:** High -- blocks entire feature
**Description:** The `lancedb` Rust crate may not compile on aarch64 (Jetson).
**Mitigation:** Phase 0 is a blocking gate. If lancedb 0.23 doesn't compile, fall back to
`object_store + parquet` crates directly. This changes the storage layer but preserves
the API surface. Document the fallback path in Phase 0.

### Risk 2: Arrow Version Conflicts
**Likelihood:** Medium
**Impact:** Medium -- requires dependency pinning
**Description:** `lancedb` depends on specific `arrow-*` crate versions. Specifying incompatible
versions in Cargo.toml will cause compilation errors.
**Mitigation:** After adding `lancedb` dependency, run `cargo tree -d` to check for duplicate
arrow versions. Pin `arrow-*` crates to match what `lancedb` uses transitively. If conflicts
arise, omit explicit `arrow-*` dependencies and use re-exports from `lancedb` if available.

### Risk 3: Large Binary Blobs in LanceDB
**Likelihood:** Low
**Impact:** Medium -- performance degradation
**Description:** Storing raw H264 frames (50-200KB each) as LargeBinary might cause slow
queries when not filtering properly, or GCS transfer overhead.
**Mitigation:** Use blob storage class metadata on the `frame_data` column so LanceDB can
optimize storage. Frame list endpoint returns metadata only (no binary data). Individual
frame endpoint fetches one frame at a time. Batch inserts amortize write overhead.

### Risk 4: Dashboard Frame Playback
**Likelihood:** Medium
**Impact:** Low -- degraded UX but functional
**Description:** Playing back recorded H264 frames in the browser requires the same decoder
pipeline as live camera view. Single-frame H264 I-frames might decode, but P-frames won't
without the reference frame chain.
**Mitigation:** Phase 7 frame viewer is best-effort. If H264 decode fails for individual frames,
display frame metadata and hex dump instead. Future improvement: store thumbnail JPEGs alongside
H264 frames, or implement GOP-aware frame fetching.

### Risk 5: Recorder Node Not Running for CLI/Dashboard
**Likelihood:** High (user error scenario)
**Impact:** Low -- expected failure mode
**Description:** CLI and dashboard send Zenoh queries to the recorder. If the recorder node
isn't running, queries time out.
**Mitigation:** All Zenoh queries use 5s timeout with clear error messages:
"LanceDB recorder not running. Start it with: bubbaloop node start lancedb-recorder"

### Risk 6: GCS Credentials in systemd
**Likelihood:** High -- no existing mechanism
**Impact:** Medium -- GCS recording fails when running as service
**Description:** Nodes running as systemd user services don't inherit shell environment variables.
`GOOGLE_APPLICATION_CREDENTIALS` won't be available unless explicitly configured.
**Mitigation:** Phase 5.5 adds `EnvironmentFile` support to the unit generator. Users create
`~/.bubbaloop/env/lancedb-recorder.env` with `GOOGLE_APPLICATION_CREDENTIALS=/path/to/creds.json`.
The systemd unit generator uses `-` prefix so missing file doesn't cause startup failure.

---

## Verification Steps

### Per-Phase Verification

**After Phase 0:**
```bash
# On the Jetson (aarch64):
cd /tmp && cargo init --name lancedb-test
# Add lancedb = "0.23" to Cargo.toml
cargo check
# If fails: document error, plan fallback to object_store + parquet
```

**After Phase 1:**
```bash
cd crates/bubbaloop-nodes/lancedb-recorder && cargo check
cd crates/bubbaloop-nodes/lancedb-recorder && cargo test
```

**After Phase 2:**
```bash
cd crates/bubbaloop-nodes/lancedb-recorder && cargo test
# Integration test (requires GCS credentials):
# GOOGLE_APPLICATION_CREDENTIALS=/path/to/sa.json cargo test -- --ignored
```

**After Phase 3:**
```bash
cd crates/bubbaloop-nodes/lancedb-recorder && cargo test
cd crates/bubbaloop-nodes/lancedb-recorder && cargo clippy -- -D warnings
```

**After Phase 4:**
```bash
cd crates/bubbaloop-nodes/lancedb-recorder && cargo test
cd crates/bubbaloop-nodes/lancedb-recorder && cargo build --release
```

**After Phase 5:**
```bash
pixi run check
pixi run clippy
pixi run test
pixi run fmt
```

**After Phase 5.5:**
```bash
pixi run check
pixi run clippy
pixi run test
```

**After Phase 6-7:**
```bash
cd dashboard && npm run build
pixi run check
pixi run clippy
```

### End-to-End Verification
```bash
# 1. Start infrastructure
zenohd &
zenoh-bridge-remote-api --ws-port 10001 -e tcp/127.0.0.1:7447 &

# 2. Start recorder node
cd crates/bubbaloop-nodes/lancedb-recorder
GOOGLE_APPLICATION_CREDENTIALS=/path/to/sa.json \
  cargo run --release -- --config configs/recorder.yaml

# 3. Start a camera node (provides data to record)
cd crates/bubbaloop-nodes/rtsp-camera
cargo run --release

# 4. Test CLI
bubbaloop record status        # Should show "idle"
bubbaloop record start         # Should show "recording started"
bubbaloop record status        # Should show "recording" with session_id
bubbaloop record stop          # Should show "recording stopped"
bubbaloop record list          # Should show the completed session

# 5. Test dual control path
bubbaloop node start lancedb-recorder   # Start process
bubbaloop record start                  # Start recording
bubbaloop record status                 # recording
bubbaloop node stop lancedb-recorder    # SIGTERM -> flush -> finalize -> exit
bubbaloop record list                   # Session should have end_time set

# 6. Test dashboard
pixi run dashboard             # Open browser, add Recordings panel, verify session appears
```

---

## File Summary

### New Files (18 files)

| File | Phase | Description |
|------|-------|-------------|
| `crates/bubbaloop-nodes/lancedb-recorder/.cargo/config.toml` | 1 | Cargo build config |
| `crates/bubbaloop-nodes/lancedb-recorder/Cargo.toml` | 1 | Crate manifest (lancedb 0.23, no gcs feature flag) |
| `crates/bubbaloop-nodes/lancedb-recorder/Cargo.lock` | 1 | Lock file (committed for reproducible builds) |
| `crates/bubbaloop-nodes/lancedb-recorder/pixi.toml` | 1 | Pixi build config |
| `crates/bubbaloop-nodes/lancedb-recorder/node.yaml` | 1 | Node manifest |
| `crates/bubbaloop-nodes/lancedb-recorder/configs/recorder.yaml` | 1 | Default config |
| `crates/bubbaloop-nodes/lancedb-recorder/src/lib.rs` | 1 | Library root |
| `crates/bubbaloop-nodes/lancedb-recorder/src/error.rs` | 1 | Error types |
| `crates/bubbaloop-nodes/lancedb-recorder/src/config.rs` | 1 | Config parsing |
| `crates/bubbaloop-nodes/lancedb-recorder/src/bin/lancedb_recorder.rs` | 1 | Binary entry point |
| `crates/bubbaloop-nodes/lancedb-recorder/src/lancedb_client.rs` | 2 | LanceDB operations |
| `crates/bubbaloop-nodes/lancedb-recorder/src/session.rs` | 3 | Session state machine (tokio::sync::RwLock) |
| `crates/bubbaloop-nodes/lancedb-recorder/src/recorder_node.rs` | 3 | Recording logic |
| `crates/bubbaloop-nodes/lancedb-recorder/src/zenoh_api.rs` | 4 | Recorder query API (JSON) |
| `crates/bubbaloop/src/cli/zenoh.rs` | 5 | Shared get_zenoh_session() helper |
| `crates/bubbaloop/src/cli/record.rs` | 5 | CLI record subcommand |
| `dashboard/src/components/RecordButton.tsx` | 6 | Record button component |
| `dashboard/src/components/RecordingsView.tsx` | 7 | Recordings panel |
| `dashboard/src/components/SortableRecordingsCard.tsx` | 7 | DnD wrapper |

### Modified Files (7 files)

| File | Phase | Change |
|------|-------|--------|
| `crates/bubbaloop/src/cli/mod.rs` | 5 | Add `pub mod record`, `pub mod zenoh`, and re-exports for `RecordCommand`, `RecordError` |
| `crates/bubbaloop/src/cli/node.rs` | 5 | Remove private `get_zenoh_session()`, import from `cli::zenoh` |
| `crates/bubbaloop/src/bin/bubbaloop.rs` | 5 | Add `use RecordCommand`, `Record(RecordCommand)` variant, match arm, help text |
| `crates/bubbaloop/src/daemon/systemd.rs` | 5.5 | Add `EnvironmentFile` directive and `TimeoutStopSec=120` |
| `dashboard/src/lib/storage.ts` | 7 | Add `'recordings'` to `PanelType`, add `RecordingsPanelConfig`, update prefix map |
| `dashboard/src/components/Dashboard.tsx` | 7 | Add recordings panel to add menu and render switch |
| `dashboard/src/App.tsx` | 6 | Add `<RecordButton />` to header between header-left and StatusIndicator |

---

## Success Criteria

1. **Functional:** Recording round-trip works end-to-end (start via CLI or dashboard, data lands in GCS via LanceDB, browsable in dashboard)
2. **Performance:** Batch inserts of 30 frames complete in <2s; no dropped frames at 30fps for a single camera
3. **Reliability:** Clean start/stop lifecycle; no data loss on graceful shutdown (SIGTERM flushes + finalizes); session records always updated on stop
4. **Ergonomic:** CLI output is clear and actionable; dashboard controls are responsive; error messages guide the user; dual control path (process vs recording) is well-documented
5. **Code quality:** All Rust code passes clippy with zero warnings; all modules have unit tests; dashboard builds cleanly; `tokio::sync::RwLock` for all async state; no `std::sync::Mutex` in async context
6. **aarch64:** Validated to compile on Jetson before any implementation work begins
