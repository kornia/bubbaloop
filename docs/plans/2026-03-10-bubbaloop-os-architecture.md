# Bubbaloop OS Architecture
*Agentic Operating System for Physical AI*

**Date**: 2026-03-10
**Status**: Design — pre-implementation
**Synthesis of**: Four parallel architect reviews (core contract, YAML v3, runtime coordinator, distribution model)

---

## Vision

Bubbaloop OS is the operating system for physical AI devices. It manages sensor pipelines, operator graphs, and AI agent deliberation on edge hardware (NVIDIA Jetson, Raspberry Pi, x86_64).

**The guiding principle**: *nodes are skills, skills are intent declarations, operators are the plumbing*. A user writes 5 lines of YAML and gets a running camera-to-detection pipeline. The agent understands what it does because `intent:` says so in natural language. The daemon runs the pipeline because the driver catalog maps `rtsp` to a compiled binary. The agent wakes only when a derived event (not a raw 30fps stream) crosses a threshold.

**What is broken today**:
- "One binary per sensor type" conflates protocol + codec + format + consumer
- A dedicated `rtsp-camera` binary cannot serve both "1 frame/sec for VLM" and "30fps for dashboard"
- BuiltIn drivers have no manifest — the daemon cannot answer "what does this skill publish?" until it starts
- `on:` is a placeholder (`Vec<serde_yaml::Value>`) with no evaluation engine
- No pipeline concept — skills are flat, not composable

---

## Three-Layer Model

```
┌───────────────────────────────────────────────────────────┐
│  LAYER 3: AGENT (deliberation, seconds)                   │
│  LLM reasoning, tool calls, memory write, alerts          │
│  Wakes on: OnHandlerQueue events (never raw streams)      │
├───────────────────────────────────────────────────────────┤
│  LAYER 2: EVENTS (derived signals, ms-seconds)            │
│  EventBridge filters stream → structured events           │
│  on: handlers evaluated here                              │
├───────────────────────────────────────────────────────────┤
│  LAYER 1: STREAMING (raw sensor data, µs-ms)              │
│  Camera frames, IMU, GPIO, HTTP polls                     │
│  Never reaches agent directly                             │
└───────────────────────────────────────────────────────────┘
```

The three layers are enforced by the runtime — not by convention. A skill cannot accidentally push a 30fps stream into an agent turn.

---

## Part 1: Operator Contract (node.yaml v2)

### 1.1 Operator Roles

Every operator is one of three roles. The role determines what the daemon expects:

| Role | Has inputs | Has outputs | Example |
|------|-----------|------------|---------|
| `source` | No (empty `[]`) | Yes | rtsp-camera, http-poll, gpio |
| `processor` | Yes | Yes | person-detect, resize, threshold |
| `sink` | Yes | No (empty `[]`) | webhook, recorder, log |

### 1.2 node.yaml v2 Format

```yaml
apiVersion: bubbaloop.dev/v1
kind: Operator
metadata:
  name: rtsp-camera
  version: "0.2.0"
  description: RTSP camera capture with hardware H264 decode
  tags: [camera, rtsp, video]

spec:
  role: source
  runtime: binary          # binary | builtin | wasm
  build: pixi run build
  command: ./target/release/rtsp_camera_node

  outputs:
    - name: compressed_frames
      topic_suffix: camera/{instance}/compressed
      schema: bubbaloop.camera.v1.CompressedImage
      tier: event          # deliberation | event | stream
      rate_hz: 30.0

  inputs: []               # Sources have no inputs

  config:
    schema:
      type: object
      required: [url]
      properties:
        url:
          type: string
          description: RTSP stream URL
        fps:
          type: integer
          default: 30

  requires:
    hardware: [network, gpu]
    software: [gstreamer, gst-plugins-base]

  health:
    heartbeat_interval_secs: 5
    startup_timeout_secs: 30
```

### 1.3 Port Tiers and Transport

| Tier | Rate | Transport | Buffer policy |
|------|------|-----------|--------------|
| `deliberation` | < 1 Hz | Zenoh pub/sub | Queue (lossless) |
| `event` | 1–30 Hz | Zenoh pub/sub | Latest (drop old) |
| `stream` | > 30 Hz | Zenoh SHM + pub/sub | Ring buffer size=1 |

**Key rule**: Operators never choose transport. They declare `tier`. The daemon negotiates the actual transport. Zenoh SHM is transparent to both publisher and subscriber.

### 1.4 Processor Declaration

```yaml
spec:
  role: processor
  inputs:
    - name: frames
      topic_suffix: camera/*/compressed     # wildcard: any camera
      schema: bubbaloop.camera.v1.CompressedImage
      tier: event
      required: true                        # don't start until input exists

  outputs:
    - name: detections
      topic_suffix: vision/{instance}/detections
      schema: bubbaloop.vision.v1.Detections
      tier: event
      rate_hz: 10.0

  constraints:
    latency_budget_ms: 100
    stateful: false
    batch_size: 1
```

### 1.5 Sink Declaration

```yaml
spec:
  role: sink
  inputs:
    - name: frames
      topic_suffix: camera/*/compressed
      tier: event
    - name: trigger
      topic_suffix: events/*/record_trigger
      tier: deliberation

  outputs: []

  side_effects:
    - kind: filesystem
      path: /data/recordings/
    - kind: network
      target: http://localhost:9090/upload
```

### 1.6 BuiltIn Driver Static Ports

BuiltIn drivers (no `node.yaml`) declare ports statically in `DRIVER_CATALOG`:

```rust
pub struct DriverEntry {
    pub driver_name: &'static str,
    pub kind: DriverKind,
    pub description: &'static str,
    pub role: OperatorRole,
    pub outputs: &'static [StaticPort],
    pub inputs: &'static [StaticPort],
}

pub struct StaticPort {
    pub name: &'static str,
    pub topic_suffix_template: &'static str,  // "{instance}/data"
    pub encoding: Encoding,                   // Bytes | Json | Protobuf
    pub tier: Tier,
}
```

### 1.7 Backward Compatibility

Existing `node.yaml` (v0) auto-upgrades:
- `publishes[].suffix` → `outputs[].topic_suffix`
- `publishes[].schema_type` → `outputs[].schema`
- `publishes[].rate_hz < 1.0` → `tier: deliberation`, else `tier: event`
- `capabilities: [sensor]` → `role: source`
- `type: rust` → `runtime: binary, language: rust`

Existing nodes continue working unchanged.

---

## Part 2: Skill YAML v3

### 2.1 Progressive Disclosure

v3 uses **progressive disclosure**: simple skills look identical to v2. Complex skills unlock pipeline syntax.

```
Simple (v2-compatible)     Multi-operator pipeline
─────────────────────────  ─────────────────────────────
name: weather              name: entrance-watch
driver: http-poll          version: 3
config:                    operators:
  url: https://...           - id: cam
  interval_secs: 300           driver: rtsp
intent: Monitor weather.       config: { url: rtsp://... }
                             - id: detector
                               driver: person-detect
                               config: { model: yolov8n }
                           links:
                             - from: cam -> detector
                             - from: detector -> dashboard
                           intent: Watch entrance.
```

The coordinator normalizes v2 to v3 internally (never writes to disk).

### 2.2 Full Pipeline Example

```yaml
name: entrance-watch
version: 3

operators:
  - id: cam
    driver: rtsp
    config:
      url: rtsp://192.168.1.10/stream
    outputs:
      stream:
        rate: 30fps
      snapshot:
        rate: 1fps          # decimation hint to driver

  - id: detector
    driver: person-detect
    config:
      model: yolov8n
      confidence: 0.7

  - id: alert
    driver: webhook
    config:
      url: http://localhost:9090/alerts
      method: POST

links:
  - from: cam/stream   -> detector
  - from: cam/snapshot -> dashboard    # reserved sink
  - from: detector     -> alert

intent: Watch the front entrance and alert when a person is detected.
on:
  - trigger: detector/person_detected
    action: notify
    message: "Person detected at entrance"
    cooldown: 5m
```

### 2.3 Cross-Skill References

```yaml
links:
  - from: skill://entrance-watch/cam/snapshot -> analyzer
```

`skill://` URI resolves to the producing skill's Zenoh topic at startup. If the referenced skill is not running, the coordinator logs a warning and retries (does not fail the whole skill).

### 2.4 Script Operators

```yaml
  - id: enrich
    driver: script
    config:
      runtime: python3      # allowlist: python3, node, bash
      path: ./scripts/enrich.py
      # stdin: upstream JSON lines
      # stdout: downstream JSON lines
      # stderr: captured to daemon log
```

`script` is a long-running subprocess transform. `exec` (existing) is a periodic source (command on interval). They are distinct.

### 2.5 Variables

```yaml
vars:
  alert_endpoint: http://localhost:9090

operators:
  - id: alert
    driver: webhook
    config:
      url: ${alert_endpoint}     # ${} syntax, resolved at load time
```

Resolution cascade: operator config → skill `vars:` → `BUBBALOOP_VAR_{NAME}` env → built-in globals (`${machine_id}`, `${scope}`, `${name}`, `${data_dir}`).

### 2.6 Rust Structs

```rust
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct SkillConfig {
    pub name: String,
    #[serde(default)]
    pub version: Option<u32>,
    #[serde(default = "default_true")]
    pub enabled: bool,

    // v2 shorthand (mutually exclusive with operators)
    #[serde(default)]
    pub driver: Option<String>,
    #[serde(default)]
    pub config: HashMap<String, serde_yaml::Value>,

    // v3 pipeline
    #[serde(default)]
    pub operators: Vec<OperatorDef>,
    #[serde(default)]
    pub links: Vec<LinkDef>,
    #[serde(default)]
    pub vars: HashMap<String, String>,

    // agent surface
    #[serde(default)]
    pub intent: String,
    #[serde(default)]
    pub on: Vec<EventHandler>,      // was Vec<serde_yaml::Value>
    #[serde(default)]
    pub schedule: Option<String>,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct OperatorDef {
    pub id: String,
    pub driver: String,
    #[serde(default)]
    pub config: HashMap<String, serde_yaml::Value>,
    #[serde(default)]
    pub outputs: HashMap<String, OutputDef>,
}

#[derive(Debug, Clone, Default, Deserialize, Serialize)]
pub struct OutputDef {
    #[serde(default)]
    pub topic: Option<String>,
    #[serde(default)]
    pub rate: Option<String>,      // "30fps", "1fps", "5s"
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct LinkDef {
    pub from: String,   // "{op_id}[/{output}]" or "skill://{skill}/{op}/{out}"
    pub to: String,     // "{op_id}" or reserved: "dashboard", "log", "null"
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct EventHandler {
    pub trigger: String,              // "{op_id}/{event_name}"
    #[serde(default)]
    pub condition: Option<String>,    // simple predicate: "value > 80"
    pub action: String,               // "notify" | "log" | "webhook" | ...
    #[serde(default)]
    pub message: Option<String>,
    #[serde(default)]
    pub cooldown: Option<String>,     // "5m", "30s"
}
```

### 2.7 Topic Layout

```
bubbaloop/{scope}/{machine_id}/{skill_name}/{operator_id}/{output_name}
```

For v2-normalized single-operator skills (`op_id == skill_name`):
```
bubbaloop/default/jetson01/weather/data          # backward compat
```

For multi-output pipelines:
```
bubbaloop/default/jetson01/entrance-watch/cam/stream     # 30fps
bubbaloop/default/jetson01/entrance-watch/cam/snapshot   # 1fps
bubbaloop/default/jetson01/entrance-watch/detector/data  # detections
```

---

## Part 3: Three-Speed Runtime Coordinator

### 3.1 PipelineCoordinator

```rust
pub struct PipelineCoordinator {
    session: Arc<Session>,
    scope: String,
    machine_id: String,
    shutdown_rx: watch::Receiver<()>,

    // Speed 1 & 2: running operator tasks
    streaming_ops: HashMap<OperatorId, OperatorHandle>,
    event_bridges: HashMap<OperatorId, EventBridgeHandle>,

    // Speed 2→3: queued events for agent deliberation
    on_handler_queue: OnHandlerQueue,

    // Wiring: which operator feeds which
    topology: HashMap<OperatorId, Vec<TopicBinding>>,

    // Unified health monitor
    health: HealthAggregator,
}

pub struct OperatorHandle {
    join_handle: JoinHandle<()>,
    kind: OperatorKind,
    control_tx: mpsc::Sender<OperatorCommand>,  // graceful reconfigure
    last_heartbeat: Arc<AtomicI64>,
}

pub enum OperatorKind {
    BuiltInTask,
    ScriptProcess { pid: u32 },
    BinaryService { unit_name: String },
}
```

**Daemon integration** — new `pipeline_task` alongside existing tasks in `daemon/mod.rs`:

```rust
let pipeline_task = tokio::spawn(pipeline_coordinator::run(
    session.clone(),
    scope.clone(),
    machine_id.clone(),
    skills_dir.clone(),
    shutdown_tx.subscribe(),
));
```

### 3.2 EventBridge Trait (Speed 1 → Speed 2)

```rust
#[allow(async_fn_in_trait)]
pub trait EventBridge: Send + Sync + 'static {
    fn name(&self) -> &'static str;
    async fn init(&mut self, ctx: &EventBridgeContext) -> anyhow::Result<()>;

    /// Called on EVERY sample (30fps / 1kHz). Must be < 1ms. No allocations.
    fn process(&mut self, payload: &[u8], timestamp: u64) -> Option<Vec<u8>>;

    async fn teardown(&mut self) {}
}
```

**Builtin bridges**:

```rust
/// Sample every N frames or M seconds.
pub struct FrameSampler { every_n: u64, every_ms: u64, frame_count: u64, last_emit_ms: u64 }

/// Emit event when numeric field crosses threshold.
pub struct ThresholdMonitor { field: String, threshold: f64, comparison: Comparison, was_above: bool, debounce_ms: u64 }

/// Detect motion from consecutive frames.
pub struct MotionDetector { sensitivity: u8, area_threshold: f64, prev_frame: Option<Vec<u8>>, is_motion: bool }
```

**Runner loop** (inside `PipelineCoordinator`):

```rust
async fn run_event_bridge(mut bridge: Box<dyn EventBridge>, ctx: EventBridgeContext) {
    bridge.init(&ctx).await.unwrap_or_log();
    let subscriber = ctx.session.declare_subscriber(&ctx.input_topic).await.unwrap();
    let publisher = ctx.session.declare_publisher(&ctx.output_topic).await.unwrap();
    loop {
        tokio::select! {
            Ok(sample) = subscriber.recv_async() => {
                let payload = sample.payload().to_bytes();
                let ts = sample.timestamp().map(|t| t.get_time().as_u64()).unwrap_or(0);
                if let Some(event_bytes) = bridge.process(&payload, ts) {
                    publisher.put(event_bytes).await.ok();
                }
            }
            _ = ctx.shutdown_rx.changed() => break,
        }
    }
    bridge.teardown().await;
}
```

### 3.3 OnHandlerQueue (Speed 2 → Speed 3)

```rust
pub struct OnHandlerQueue {
    queues: HashMap<String, AgentEventQueue>,  // keyed by agent_id
    session: Arc<Session>,
    scope: String,
    machine_id: String,
}

struct AgentEventQueue {
    pending: VecDeque<PendingEvent>,
    max_depth: usize,
    agent_busy: AtomicBool,
    coalesce_window_ms: u64,
}

pub struct PendingEvent {
    handler_id: String,
    event_topic: String,
    event_payload: Vec<u8>,
    received_at: Instant,
    priority: u8,
}
```

**Coalescing logic**:
- Agent FREE → immediately dispatch event as agent turn via `agents/{id}/inbox`
- Agent BUSY → enqueue in ring buffer
- Same `handler_id` + within `coalesce_window_ms` → merge into one summary event
- Agent finishes turn (Done event observed) → dequeue next / coalesced batch

Agent sees: `"Person detected (3 occurrences in last 8s, latest: {bbox, confidence})"` — never raw 30fps frames.

**Integration with arousal model** (`agent/heartbeat.rs`): each enqueued event fires `ArousalSource::EventTrigger` (new variant, boost 1.5), tightening the agent's heartbeat interval even before the turn starts.

### 3.4 Full Timing Budget

| Stage | Latency | Rate |
|-------|---------|------|
| RTSP decode → Zenoh publish | ~1ms | 30fps |
| FrameSampler `process()` | <100µs | 30x/s call, 1x/s emit |
| Zenoh transport → detector | ~500µs | 1fps |
| YOLOv8 inference | ~200ms | ~5fps capacity |
| Zenoh → OnHandlerQueue | ~500µs | 0–5 events/s |
| OnHandlerQueue filter + coalesce | <1ms | immediate |
| Agent turn (LLM + tools) | 2–8s | on-demand |
| **Total: sensor → agent action** | **3–10s** | N/A |

Speed 1 is never blocked by Speed 3. The three speeds are isolated by channel buffers with `DropPolicy::Latest`.

### 3.5 Health Contract Per Operator Type

| Operator Type | Health Signal | Timeout | Restart |
|--------------|---------------|---------|---------|
| BuiltIn Task | Zenoh `health_pub.put(b"ok")` every 5s | 15s | `handle.abort()` + re-spawn |
| Script Process | Zenoh health OR process exit | 30s | Kill PID + re-spawn |
| Binary Service | Zenoh health (Node SDK `health.rs`) | 30s | systemd auto-restart |

---

## Part 4: Operator Distribution Model

### 4.1 Four Operator Types

| Type | Execution | Security | Use case |
|------|----------|---------|---------|
| **BuiltIn** | tokio task in daemon | Full trust | http-poll, system, exec, webhook |
| **Script** | subprocess, stdin/stdout JSON | Filesystem + network restricted | Custom transforms, glue code |
| **WASM** | wasmtime sandbox | Capability-based | Portable ML operators |
| **Native binary** | subprocess, own Zenoh session | SHA256 verified | GStreamer, YOLO, heavy compute |

### 4.2 Script Operator Protocol

```
stdin  ← JSON lines (one per upstream sample)
stdout → JSON lines (one per output sample, or empty for suppression)
stderr → captured to daemon log at level INFO
```

Security constraints:
- `runtime:` is an allowlist: `python3`, `node`, `bash` (matches build command allowlist)
- `path:` must be under `~/.bubbaloop/skills/` — no `../` traversal
- Env available: `BUBBALOOP_ZENOH_ENDPOINT`, `BUBBALOOP_SCOPE`, `BUBBALOOP_MACHINE_ID`, `BUBBALOOP_SKILL_NAME`
- No additional network or filesystem grants in v1

### 4.3 WASM Operator Interface

```rust
// WASM guest exports (Extism plugin):
fn process(input: &[u8]) -> Vec<u8>;   // pure transform
fn init(config: &[u8]);                // called once with JSON config
fn name() -> &str;                     // operator name

// Capability grants (in skill YAML):
capabilities:
  - network                 // outbound HTTP
  - filesystem:/data/       // read/write under /data/
  - gpu                     // CUDA/Metal access (future)
```

- WASM operators are platform-universal (run on any arch)
- Stored at `~/.bubbaloop/operators/{sha256}/{name}.wasm`
- Performance: ~3–5x slower than native for pure compute; acceptable for glue/transform
- Recommended for: data formatters, protocol translators, simple ML post-processing
- Not recommended for: heavy CV inference (use native binary)

### 4.4 Skill Hub

The hub is the community registry for skill YAMLs and operator binaries.

**Hub index format** (`~/.bubbaloop/cache/skills_hub.yaml`):

```yaml
version: "1.0"
updated_at: "2026-03-10T00:00:00Z"

operators:
  - name: person-detect
    version: "0.3.1"
    description: YOLOv8 person detection
    author: bubbaloop-team
    categories: [vision, detection]
    platforms:
      - arch: linux/aarch64
        sha256: "abc123..."
        url: "https://hub.bubbaloop.dev/operators/person-detect/0.3.1/linux-aarch64"
        size_bytes: 45_000_000
      - arch: wasm32-wasi
        sha256: "def456..."
        url: "https://hub.bubbaloop.dev/operators/person-detect/0.3.1/wasm"
        size_bytes: 12_000_000
    node_yaml_url: "https://hub.bubbaloop.dev/operators/person-detect/0.3.1/node.yaml"

skills:
  - name: entrance-watch
    version: "1.0.0"
    description: Camera + person detection + webhook alert
    author: community/alice
    operators_required: [person-detect]
    skill_yaml_url: "https://hub.bubbaloop.dev/skills/entrance-watch/1.0.0/skill.yaml"
    # No binary needed if all operators are builtin!
```

**Community skill YAML sharing** (the key insight): Skills that only use builtin drivers need zero downloads. A user shares a `.yaml` file; recipients install it and it just works. This is the "npm scripts" equivalent — shareable, zero-compile, zero-download for the common case.

### 4.5 Content-Addressed Storage

Operators stored by SHA256:

```
~/.bubbaloop/operators/
  abc123.../person-detect          # native binary (pinned by sha256)
  def456.../person-detect.wasm    # WASM variant
  ~/.bubbaloop/operators/current/person-detect -> abc123.../person-detect  # symlink
```

Multiple versions co-exist. Skills pin exact sha256. Rollback: previous version still present. The existing `marketplace.rs` download logic (SHA256 verification) extends to the operator store with the same security model.

### 4.6 Resolution Cascade (updated)

```
bubbaloop up entrance-watch
         │
         ▼
    DriverResolution::resolve(skill)
         │
   ┌─────┴─────────────────────────────┐
   │                                   │
BuiltIn?                          Marketplace?
   │                                   │
   ▼                                   ▼
daemon.StartBuiltIn()         skill_hub.resolve_operator()
(no download, no process)              │
                              ┌────────┴──────────┐
                         local binary?         hub download?
                              │                   │
                              ▼                   ▼
                         register +          download + verify
                         start process       + register + start
```

Users never see "builtin" vs "marketplace" — they see "started" or "error".

---

## Part 5: Migration Path

### Current → v1 (this document)

**Phase 0** (done): `DriverKind { BuiltIn, Marketplace }`, `resolve()`, `SkillRuntime`

**Phase 1** (done): Builtin drivers (http-poll, system, exec, webhook, tcp-listen)

**Phase 2** (done): `StartSkill`/`StopSkill`/`ListSkills` daemon commands

**Phase 3** (done): `bubbaloop up` cascade, `bubbaloop skill drivers`

**What changes in v2 (this document)**:

1. **`SkillConfig` struct** — add `operators`, `links`, `vars` fields; change `on` from `Vec<serde_yaml::Value>` to `Vec<EventHandler>`
2. **`DriverEntry`** — add `StaticPort` outputs/inputs for builtin drivers
3. **`PipelineCoordinator`** — new struct replacing flat `SkillRuntime`, adds `EventBridge` + `OnHandlerQueue`
4. **`node.yaml` parser** — support v2 format with `apiVersion`, `spec.role`, `spec.outputs[]`
5. **Builtin bridges** — `FrameSampler`, `ThresholdMonitor`, `MotionDetector`
6. **Operator store** — content-addressed `~/.bubbaloop/operators/{sha256}/`
7. **Skill hub** — extend `skill_hub.rs` with `operators:` section in hub index

### Existing nodes (rtsp-camera, openmeteo, etc.)

Existing nodes continue to work unchanged. The gaps to close (separately in `bubbaloop-nodes-official`):
- Read `BUBBALOOP_CONFIG_PATH` env var for per-instance config
- rtsp-camera: schema key uses `config.name`, not hardcoded `rtsp-camera`
- Add optional `width`/`height` defaults (224×224)
- Add `node.yaml` v2 `spec.outputs[]` declarations (backward-compat: old format still works)

---

## Part 6: Design Decisions and Trade-offs

| Decision | Rationale | What we lose |
|----------|-----------|-------------|
| Zenoh as universal transport (no Arrow IPC) | One schema system (protobuf), one bus | Cannot zero-copy across language boundaries without Zenoh SHM |
| `tier` drives transport (not explicit selection) | Operators are transport-agnostic | Operators cannot force a specific transport |
| `process()` returns `Vec<u8>` | Simple, testable, no trait generics | One allocation per emitted event |
| Coalescing in `OnHandlerQueue` | Agent sees actionable summaries | Loses individual event detail |
| `links:` is explicit (not implicit array order) | Unambiguous fan-out/fan-in | More verbose for linear pipelines |
| Script operator security: allowlist runtime, path under skills/ | Practical for v1 | No true isolation (no container) |
| WASM: wasmtime + Extism, not custom | Proven, fast startup, capability model | 3–5x perf penalty vs native |
| Rate control on output port (not link) | Driver controls decimation efficiently | Cannot serve two consumers at different rates from one port |
| No pipeline DSL (only YAML + Zenoh wildcards) | Agent resolves high-level intent | Users must write explicit `links:` for non-trivial graphs |

### What NOT to build in v1

- **Pipeline DSL** — agent subsumes this eventually
- **Zenoh SHM optimization** — ship with standard pub/sub first; add SHM when a real 30fps raw frame pipeline exists
- **Schema compatibility enforcement** — warn on mismatch, don't block startup
- **Conditional links** (`from: detector -> alerter when confidence > 0.8`) — use `on:` for now
- **WASM GPU access** — requires custom capabilities, deferred
- **Link-level rate decimation** — per-output rate is sufficient for v1

---

## Part 7: GStreamer Position

GStreamer is **one implementation** of the `rtsp` driver, not a system requirement.

```
Driver abstraction:
  rtsp driver (node.yaml, binary) ─── implements using: gstreamer (default on Jetson)
                                                          v4l2 (alternative)
                                                          ffmpeg (alternative)
                                                          custom (future)
```

The `rtsp-camera` binary today is a GStreamer pipeline in a Rust wrapper. In the new model, it becomes the `rtsp` marketplace driver — `node.yaml v2` declares typed outputs, the daemon treats it as a Source operator. The internal implementation (GStreamer, V4L2, etc.) is invisible to the skill YAML.

For future camera implementations that don't use GStreamer, you write a new binary, publish it to the skill hub as `rtsp@0.2.0`, and the old binary remains available as `rtsp@0.1.x`. Skills pin by operator name + optional version.

---

## Part 8: Agent Interface

### How the agent interacts with the pipeline

The agent never touches raw sensor data. It interacts through:

1. **MCP tools** (existing): `install_node`, `start_node`, `stop_node`, `get_node_list`
2. **New: `run_operator`** — spawn ad-hoc operator for N seconds: `run_operator(driver="person-detect", input="entrance-watch/cam/snapshot", duration_secs=10)`
3. **New: `grab_frame`** — Zenoh query for one message from a topic: `grab_frame(topic="bubbaloop/default/jetson01/entrance-watch/cam/snapshot")`
4. **OnHandlerQueue** — agent wakes when `on:` condition matches; receives coalesced summary

### `grab_frame` MCP tool

This is the key missing piece for agent-vision interaction:

```rust
// In mcp/tools.rs
async fn grab_frame(&self, params: GrabFrameParams) -> ToolResult {
    // Zenoh query with timeout=2s, QueryTarget::BestMatching
    // Returns base64-encoded payload or error
    let replies = self.platform.zenoh_session()
        .get(&params.topic)
        .timeout(Duration::from_secs(2))
        .target(QueryTarget::BestMatching)
        .await?;
    // ...return first reply as base64
}
```

Without `grab_frame`, the agent cannot inspect what a camera is seeing. With it, the agent can: notice an alert → grab a snapshot → analyze visually → update belief → take action.

---

## Part 9: Critical Quick Wins (Dead Code Already Written)

The three-speed runtime coordinator agent discovered **existing dead code** that gives us 80% of Speed 2→3 bridging without new modules:

### 9.1 ReactiveRule + ArousalBoost (NEVER WIRED)

- `daemon/reactive.rs:62` — `evaluate_rules()` is implemented and tested but **never called in production**
- `agent/heartbeat.rs:108` — `add_external_boost()` is implemented but **never called outside tests**
- `daemon/context_provider.rs` — already subscribes Zenoh, writes SQLite world state — but **never notifies the agent**

**Phase A (1–2 days, zero new modules)**:
1. In `agent_loop` (`runtime.rs:651`), add call to `evaluate_rules()` on heartbeat tick branch → `arousal.add_external_boost(boost)`
2. Add a `tokio::sync::Notify` in `spawn_provider()` that fires when world_state changes, waking agent immediately instead of waiting for heartbeat

This alone gives you reactive sensor-to-agent bridging. The `on:` handlers and coalescing are refinements on top.

### 9.2 Implementation Phasing (Both Phase A and Distribution)

**For the runtime coordinator**:

| Phase | Duration | Description | New Deps |
|-------|----------|-------------|---------|
| A | 1–2 days | Wire dead code: `evaluate_rules()` + `Notify` in context_provider | None |
| B | 3–5 days | EventBridge + OnHandlerQueue new modules | None |
| C | 1–2 weeks | PipelineCoordinator + GStreamer integration | `gstreamer` feature-gated |
| D | Future | WASM/Python operators | `wasmtime` feature-gated |

**For the distribution model**:

| Phase | Description | Binary Impact |
|-------|-------------|--------------|
| P0 | Script operator protocol + `script` DriverKind | 0 (Python users can write operators immediately) |
| P1 | Content-addressed operator storage (`~/.bubbaloop/operators/`) | 0 |
| P2 | Hub index v2 (add `operators[]` to `SkillHubIndex`) | 0 |
| P3 | GStreamer pipeline driver | 0 |
| P4 | WASM operator support (feature-gated `wasm-operators`) | +3–5 MB |
| P5 | Ed25519 signature verification | +50 KB (`ed25519-dalek`) |
| P6 | Agent `run_operator` / `stop_operator` MCP tools | 0 |

**P0–P2 can ship together as a single PR.** P3 is independent. P4 deferred until concrete demand. Start with Phase A immediately — it is the fastest path to a reactive agent that wakes on sensor events.

---

## Appendix: Key Files

| File | Phase | Change |
|------|-------|--------|
| `crates/bubbaloop/src/skills/mod.rs` | v2 | Add `OperatorDef`, `LinkDef`, `EventHandler` structs; update `SkillConfig` |
| `crates/bubbaloop/src/skills/resolve.rs` | v2 | Add WASM + script resolution arms |
| `crates/bubbaloop/src/skills/builtin/mod.rs` | v2 | Add `EventBridge` trait, `EventBridgeContext` |
| `crates/bubbaloop/src/skills/builtin/{frame_sampler,threshold,motion}.rs` | v2 | New bridges |
| `crates/bubbaloop/src/skills/runtime.rs` | v2 | Evolve into `PipelineCoordinator` |
| `crates/bubbaloop/src/skills/on_handler.rs` | v2 | New: `OnHandlerQueue`, coalescing |
| `crates/bubbaloop/src/daemon/mod.rs` | v2 | Add `pipeline_task = tokio::spawn(...)` |
| `crates/bubbaloop/src/daemon/node_manager/lifecycle.rs` | v2 | Recognize `OperatorKind::BuiltInTask` |
| `crates/bubbaloop/src/mcp/tools.rs` | v2 | Add `grab_frame`, `run_operator` tools |
| `crates/bubbaloop/src/skill_hub.rs` | v2 | Add `operators:` section to hub index |
| `crates/bubbaloop-node-sdk/src/` | v2 | Expose `SourceNode`/`ProcessorNode`/`SinkNode` traits (optional) |
| `bubbaloop-nodes-official/*/` | v2 | Add `node.yaml v2`, `BUBBALOOP_CONFIG_PATH` support |
