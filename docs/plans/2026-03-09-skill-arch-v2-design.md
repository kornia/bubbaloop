# Skill Architecture v2.0 — Design Synthesis

> Synthesized from design session (March 2026).
> Original plan: `/home/edgar/Downloads/Bubbaloop-Node-Architecture.docx`
> Status: **Pre-implementation review — share before any code is written.**

---

## Core Thesis

> "80% of apps die. Only sensor/hardware apps survive. Nodes are the product."
> — Peter Steinberger / OpenClaw

Bubbaloop's skill system should reflect this. A **skill is an intent declaration**, not a process config. The daemon is plumbing. The agent is the interface.

---

## The Mental Model

```
What the user writes:   YAML intent ("watch the entrance, alert after 11pm")
What the agent does:    Resolves intent → wires pipeline
What the daemon does:   Runs drivers, manages lifecycle
What the node author writes:  Driver implementation (hardware I/O)
```

These four concerns are **independently owned and independently reusable**.

---

## Three Tiers — Unchanged from v1

| Tier | What you write | How it runs | Who it's for |
|------|---------------|-------------|--------------|
| **1 — YAML Skill** | 5-line YAML + known driver | Built-in tokio task OR marketplace binary | 91% of users |
| **2 — Marketplace** | Same YAML, unknown/complex driver | Downloads precompiled binary, runs as process | Plug-and-play hardware |
| **3 — Custom Node** | Rust with SDK traits | `cargo build` + process | Node authors |

The tier is **resolved automatically** by the daemon. Users never select a tier — they just write YAML.

---

## The Skill YAML Format — Revised (Intent-First)

The `body` and `on` fields are **promoted to first-class**. They are what makes a skill an OpenClaw-style capability, not just a process launcher.

```yaml
name: entrance-watch
driver: rtsp
config:
  url: rtsp://192.168.1.10/stream
  fps: 15

# Intent — what the agent should do with this data
intent: |
  Monitor the front entrance.
  Alert me if a person appears between 11pm and 6am.
  Save a 30-second clip on any motion event.

# Declarative event handlers (agent-executed)
on:
  - trigger: person_detected
    between: "23:00-06:00"
    action: notify
  - trigger: motion
    action: save_clip

# Optional cron schedule for agent actions
schedule: "0 8 * * *"
actions:
  - summarize_overnight_activity
```

**Minimal form (still valid — Tier 1 in 4 lines):**

```yaml
name: doorbell-cam
driver: http-poll
config:
  url: http://192.168.1.20/snapshot.jpg
  interval_secs: 1
```

---

## Driver Resolution Cascade

When `bubbaloop up` runs, for each active skill:

```
1. Is the driver BuiltIn?
   → spawn tokio task inside daemon, no download

2. Is the marketplace binary already installed locally?
   → register + start (existing systemd path)

3. Otherwise:
   → download from marketplace, then register + start
```

This cascade is **invisible to the user**. The output of `bubbaloop up` says "started", not "started as builtin" or "started as process". Implementation detail, not UX.

---

## Two Reusable Layers

### Layer 1 — Skill YAML (User-level reusability)

Skill files live in `~/.bubbaloop/skills/*.yaml`. They are:
- Version-controlled (just files)
- Shareable — copy/paste, git repo, future skill hub
- Deployable anywhere — just change the `config` values

A community member shares `security-watch.yaml`. You drop it in your skills dir, update the RTSP URL, run `bubbaloop up`. Done.

### Layer 2 — Driver Binary (Developer-level reusability)

Driver implementations are separate from skill files:

| Driver kind | Location | Maintained by | Distribution |
|-------------|----------|--------------|--------------|
| `BuiltIn` (http-poll, system, exec, webhook…) | `crates/bubbaloop/src/skills/builtin/` | Bubbaloop core team | Ships inside daemon binary |
| `Marketplace` (rtsp, v4l2, serial, gpio…) | `bubbaloop-nodes-official` repo | Kornia / community | SHA256-verified binary download |
| `Custom` (Tier 3) | User's own repo | Node author | GitHub / marketplace contribution |

**Contract between layers**: the `driver:` name in the catalog + the `config:` keys the driver expects. That's it. Skill authors don't know how drivers work. Driver authors don't know how skills are configured.

---

## SDK Node Roles — SourceNode / SinkNode / ProcessorNode

> **These are SDK-only concerns. The core daemon never inspects node roles.**

The original plan used `#[sensor]` / `#[actuator]` / `#[processor]`. These are domain-specific (robotics terms) and break down quickly:
- A webhook receiver isn't "sensing" anything
- A database writer isn't an "actuator" in any meaningful sense
- A PTZ camera is both sensor and actuator

Replaced with universal dataflow terms:

```rust
// Produces data on an interval or event
pub trait SourceNode: Send + Sync {
    async fn produce(&self, ctx: &NodeContext) -> Result<Value>;
    fn interval(&self) -> Duration { Duration::from_secs(60) }
}

// Consumes data from a topic
pub trait SinkNode: Send + Sync {
    async fn consume(&self, ctx: &NodeContext, data: Value) -> Result<()>;
}

// Subscribes, transforms, publishes
pub trait ProcessorNode: Send + Sync {
    async fn transform(&self, ctx: &NodeContext, input: Value) -> Result<Value>;
}
```

**No proc macros in v1.** Traits with default impls + generic runner functions achieve the 25-line goal with less complexity and better error messages:

```rust
// Generates: select! loop, health heartbeat, shutdown, topic wiring
pub async fn run_source<T: SourceNode + Default>() { ... }
pub async fn run_sink<T: SinkNode + Default>() { ... }
pub async fn run_processor<T: ProcessorNode + Default>() { ... }
```

Proc macros are deferred until Tier 3 node authors exist and can give feedback on what they actually need. Don't design an API for zero users.

---

## The Agent's Role — Intent Resolution

The agent (jean-clawd) reads the `intent` and `on` fields of each active skill. This is where OpenClaw's philosophy converges with Bubbaloop:

- **Phase 1 (now)**: Agent reads `intent` as context — can answer questions about what's running
- **Phase 2 (near)**: Agent executes `on` handlers — monitors topic data and fires declared actions
- **Phase 3 (future)**: Agent autonomously wires pipelines — "I need a processor between this source and that sink, let me find one in the marketplace"

The skill YAML talks to the agent. The daemon handles the plumbing. These are orthogonal responsibilities.

---

## The Skill Hub (Future — Not Phase 0-5)

OpenClaw has `openclaw/clawhub` — 5,400+ community skills. Bubbaloop needs the equivalent for sensor/actuator skills.

A skill hub entry is just a YAML file. No binary. No compilation. Anyone can contribute:

```yaml
# community-skills/weather-station.yaml
name: weather-station
driver: http-poll
config:
  url: https://api.open-meteo.com/v1/forecast?latitude=YOUR_LAT&longitude=YOUR_LON
  interval_secs: 300
intent: Track local weather. Alert if temperature drops below 5°C overnight.
on:
  - trigger: temperature_below
    threshold: 5
    between: "22:00-08:00"
    action: notify
```

Deploy in 30 seconds. The `http-poll` built-in handles execution. No binary download needed.

---

## What Changes vs. the Original Plan

| Original | Revised | Why |
|----------|---------|-----|
| `#[sensor]` / `#[actuator]` / `#[processor]` macros | `SourceNode` / `SinkNode` / `ProcessorNode` traits | Universal dataflow terms; macros deferred |
| `bubbaloop-node-sdk-macros` crate (Phase 3) | Deferred post-v1 | No Tier 3 users yet to validate the API |
| `[builtin]` tag in `bubbaloop status` | Hidden from user | Implementation detail, not UX |
| Single-node skill YAML | Intent-first skill YAML (`intent:`, `on:` promoted) | Skill talks to agent, not just to daemon |
| Driver resolution exposed in CLI output | Internal cascade only | User sees "started", not "started as builtin" |

---

## What Stays the Same

- Three-tier mental model (YAML → Marketplace → Custom Rust)
- `DriverKind::BuiltIn` vs `DriverKind::Marketplace` enum in catalog
- `DriverResolution` cascade in `skills/resolve.rs`
- `skills/builtin/` directory — BuiltIn drivers as tokio tasks sharing daemon session
- `SkillRuntime` in daemon, spawned alongside MCP and agent tasks
- NodeManager integration — `CachedNodeKind::BuiltInSkill` vs `SystemdManaged`
- CLI `bubbaloop up` refactor around cascade
- CLI `bubbaloop skill drivers` table
- All existing marketplace / systemd / registry behavior (zero regression)

---

## Revised Phase Sequence

| Phase | What | Duration | Validates |
|-------|------|----------|-----------|
| **0** | `DriverKind` enum + `resolve.rs` cascade | 2 days | Foundation |
| **1** | `skills/builtin/` runtime + `http-poll` + `system` + daemon spawn | 4 days | Does YAML → running task work? |
| **2** | NodeManager integration — `CachedNodeKind` | 3 days | Does `bubbaloop status` see builtin skills? |
| **3** | `bubbaloop up` refactor + `skill drivers` CLI | 2 days | Is the UX right? |
| **4** | Remaining built-in drivers + hot-reload | 4 days | Full driver catalog |
| **5** | `SourceNode`/`SinkNode`/`ProcessorNode` traits in SDK | 3 days | Tier 3 SDK (ship when first node author appears) |
| **6** | Agent `intent` / `on` execution | TBD | Full OpenClaw integration |

Phases 0–4 are the v1 target. Phase 5 when first Tier 3 node author asks for it. Phase 6 is the convergence with the agent roadmap.

---

## Concrete Example: Camera Across All Tiers

The camera case is the clearest illustration of where each tier applies — and of a real seam in the design.

| Camera type | Tier | Driver | Why |
|-------------|------|--------|-----|
| RTSP / ONVIF / NVR | 2 | `rtsp` (Marketplace) | Needs gstreamer — must be a subprocess |
| USB webcam (v4l2) | 2 | `v4l2` (Marketplace) | Needs v4l2 native bindings — subprocess |
| HTTP snapshot / MJPEG | 1 | `http-poll` (BuiltIn) | Pure HTTP — tokio task is fine |
| Custom capture pipeline | 3 | — | Custom Rust `SourceNode` |

**Tier 1 YAML (HTTP snapshot — runs inside daemon, zero download):**
```yaml
name: doorbell-cam
driver: http-poll
config:
  url: http://192.168.1.20/snapshot.jpg
  interval_secs: 1
```

**Tier 2 YAML (RTSP — downloads rtsp-camera binary, runs as systemd service):**
```yaml
name: entrance-cam
driver: rtsp
config:
  url: rtsp://192.168.1.10/stream
  fps: 15
```

**Same user experience.** The tier is resolved invisibly.

Key insight: **built-in drivers are best for pure network I/O** (HTTP, TCP, webhooks, cron). Anything requiring native libraries (video codecs, hardware drivers) will always be Marketplace regardless of tier. This is not a limitation — it's the correct architecture boundary.

---

## Proto Schema: node_type vs node_role

`NodeState` in `daemon.proto` has `node_type: string` (field 8), currently carrying `"rust"` / `"python"` / `"builtin"`.

This conflates two orthogonal concepts:
- **Runtime kind** — how it's managed (`"rust"`, `"python"`, `"builtin"`)
- **Dataflow role** — what it does in the pipeline (`"source"`, `"sink"`, `"processor"`)

**Decision needed before Phase 2 implementation:**

| Option | Approach | Cost |
|--------|----------|------|
| A | Overload `node_type`: `"builtin-source"`, `"builtin-sink"` | Zero schema change, messy long-term |
| B | Add `node_role: string` field (field 19) | Clean, requires schema rebuild + descriptor.bin recompile |
| C | Keep `node_type` for runtime kind, ignore role in proto for now | Simplest — role lives in SDK only, surfaces later |

**Recommendation**: Option C for v1. The daemon doesn't need to know roles to run tasks. Role is an SDK-level concept for node authors and a future marketplace search concern. Revisit when the skill hub needs it.

---

## The Agentic Trap Warning

> "I see too many people discover how powerful agents are — then try to make them even more powerful — and tumble down the rabbit hole."
> — Peter Steinberger

Applied to this design: **do not let the agent own the driver execution path**. The agent resolves intent and wires declarative `on:` handlers. It does not spawn processes, manage systemd units, or download binaries. That is the daemon's job.

The `intent:` and `on:` fields give the agent a well-scoped surface. Keep it that way.

---

## Key Invariants to Preserve

- **Skill YAML is the only user-facing artifact.** No user should ever need to know whether their skill runs as a tokio task or a systemd service.
- **Driver authors and skill authors are decoupled.** The only contract is driver name + config keys.
- **The agent reads intent. The daemon runs plumbing.** These are separate concerns, never mixed.
- **Built-in drivers are generic.** They should work for any HTTP endpoint, any TCP port, any cron job — not be purpose-built for a specific use case.
- **Zero regression on existing systemd/marketplace path.** All existing nodes continue to work unchanged.
