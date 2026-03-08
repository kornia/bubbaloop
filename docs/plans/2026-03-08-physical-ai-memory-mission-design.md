# Physical AI Memory & Mission Architecture
<!-- Design document — 2026-03-08. Serves as foundation for systems paper. -->

## Abstract

We present a unified agent architecture for physical AI systems — home IoT, mobile robotics, and
software automation — running on a single 12–13 MB binary on resource-constrained edge hardware
(NVIDIA Jetson, Raspberry Pi). The central contribution is a **LLM-compiled reactive mission DAG**
that replaces behavior trees as the planning substrate, combined with a four-tier sensor-grounded
memory model and a daemon-enforced safety layer that provides deterministic guarantees without
LLM involvement. The user interface is a markdown file. The rest is autonomous.

---

## 1. Motivation

### 1.1 The Steinberger Principle

> *"Perhaps only apps that rely on specific hardware sensors will survive."*

80% of software applications will be replaced by AI agents. The surviving 20% interface with
physical reality — sensors, actuators, hardware. The core thesis of this work: **the agent loop
must be a first-class citizen of the sensor runtime**, not an afterthought bolted on top.

### 1.2 The Three Use Cases

This design targets three fundamentally different deployment scenarios that share a common
underlying architecture:

**Home IoT (dog/toddler monitoring)**
A camera watches a living space. A vision node processes frames and publishes semantic
detections. An agent maintains a behavioral profile over time ("the dog eats at 8am and 6pm"),
detects anomalies ("toddler near stairs"), and sends alerts. Token cost must be minimal — the
system runs unattended for months.

**Robot Brain (VLM → VLA coordination)**
A Vision Language Model runs as a sensor node, publishing scene descriptions to a message bus.
A robot state node publishes joint positions and torques. An agent operates at the goal level
(~1 Hz), decomposing natural language instructions into sub-goals and commanding a Vision
Language Action model that executes at the control level (50 Hz). The LLM never enters the
tight control loop.

**Software Automation (PR review bot)**
A GitHub node publishes pull request events. An agent reviews diffs, checks CI status,
validates conventions, and posts structured review comments. No sensors, no real-time
constraint — same architecture, different context providers.

### 1.3 The Gap in Existing Systems

| System | Limitation |
|--------|------------|
| ROS2 + LLM wrappers | No memory, no planning, LLM in control loop |
| Behavior trees | Deterministic but rigid, can't handle novel situations |
| LLM agents (AutoGen, CrewAI) | No sensor grounding, assumes shared memory on one machine |
| MemGPT / mem0 | Conversation memory only, no physical state, no real-time |
| RT-2 / OpenVLA | VLM→action, no persistent memory, no edge deployment |

No existing system combines: sensor-grounded memory, LLM-compiled mission planning,
daemon-enforced safety, and edge deployment in a single binary.

---

## 2. Architecture Overview

### 2.1 The Common Shape

All three use cases share the same loop:

```
Sensor Nodes → Processing Nodes → Zenoh topics
                                        ↓
                          Daemon: Context Providers
                          (world state, no LLM, continuous)
                                        ↓
                            Agent Turn (LLM, event-driven)
                           ↙                        ↘
                   Act (MCP tools)          Remember (episodic + beliefs)
                        ↓
              VLA / notifications / actuators / comments
```

**Key invariant**: The LLM never enters the tight control loop. Sensor data flows through
processing nodes (vision, IMU fusion, etc.) which publish semantic results to Zenoh. The daemon
maintains a structured world state from these results. The agent reasons over the world state,
not raw sensor data.

### 2.2 Layer Model

```
┌──────────────────────────────────────────────────────────────┐
│  BUBBALOOP  (single binary, ~12–13 MB)                        │
│                                                               │
│  ┌─────────────────────────────────────────────────────────┐  │
│  │  Agent Runtime                                           │  │
│  │  Identity (soul) | Mission DAG | Context Assembly       │  │
│  └────────────────────────┬────────────────────────────────┘  │
│  ┌────────────────────────┴────────────────────────────────┐  │
│  │  4-Tier Memory                                           │  │
│  │  Tier 0: World State (live, daemon-written)             │  │
│  │  Tier 1: Short-term (RAM, per turn)                     │  │
│  │  Tier 2: Episodic (NDJSON + FTS5, causal chains)        │  │
│  │  Tier 3: Semantic (SQLite: jobs, proposals, beliefs)    │  │
│  └────────────────────────┬────────────────────────────────┘  │
│  ┌────────────────────────┴────────────────────────────────┐  │
│  │  Mission Runtime                                         │  │
│  │  Mission DAG | Constraint Engine | Resource Locks       │  │
│  └────────────────────────┬────────────────────────────────┘  │
│  ┌────────────────────────┴────────────────────────────────┐  │
│  │  Context Providers (daemon background tasks)            │  │
│  │  ZenohContextProvider | JobProvider | EventProvider     │  │
│  └────────────────────────┬────────────────────────────────┘  │
│  ┌────────────────────────┴────────────────────────────────┐  │
│  │  Reactive Pre-filter                                     │  │
│  │  Rule engine (threshold, pattern) → arousal spikes      │  │
│  └────────────────────────┬────────────────────────────────┘  │
│  ┌────────────────────────┴────────────────────────────────┐  │
│  │  Zenoh Data Plane (zero-copy, real-time pub/sub)        │  │
│  └────────┬──────────────┬───────────────┬────────────────┘  │
└───────────┼──────────────┼───────────────┼────────────────────┘
            │              │               │
       ┌────┴───┐    ┌──────┴──┐    ┌─────┴────┐
       │ Camera │    │ VLM Node│    │ VLA Node │
       └────────┘    └─────────┘    └──────────┘
                  Sensor/processing nodes (external)
```

### 2.3 Timescale Separation

The architecture is built around strict timescale separation — each layer operates at the
timescale appropriate to its function:

```
<1ms    Firmware / VLA          hard limits, emergency stop, motor control
<1ms    Daemon constraint engine workspace validation, resource locks
<10ms   Reactive pre-filter     threshold rules, pattern matching → arousal spike
~500ms  Local LLM (Ollama)      fallback re-planning when cloud unavailable
~2s     Cloud LLM (Claude)      full reasoning, mission compilation, re-planning
minutes Mission DAG             goal decomposition, multi-step planning
```

No layer crosses into the domain below it. The LLM never operates at <500ms. The firmware
never waits for the LLM.

---

## 3. Identity vs Mission: Separation of Concerns

### 3.1 The Distinction

A critical design choice: agent identity and agent mission are separate files with different
semantics, different change frequencies, and different compilation steps.

**Identity** (`~/.bubbaloop/agents/{id}/soul/identity.md`):
- Who the agent *is* — personality, name, values, communication style
- Loaded into every agent turn as system prompt context
- Hot-reloaded on file change — no restart, no reconfiguration
- Written once, rarely changed
- Example: *"I'm a calm, observant home assistant. I care about family safety.
  I speak concisely and only alert when something genuinely needs attention."*

**Mission** (`~/.bubbaloop/agents/{id}/missions/{name}.md`):
- What the agent *does* — sensors to watch, rules, schedules, alerts, goals
- Compiled into executable configuration (context providers, jobs, alerts) on setup turn
- Mission change triggers setup re-run — rewires context providers and jobs
- Written by end users, evolves over time
- Example: *"Watch the kitchen camera. Alert if dog hasn't eaten by 6pm."*

### 3.2 Mission Filesystem Layout

```
~/.bubbaloop/agents/{id}/
  soul/
    identity.md          ← loaded every turn (personality)
  missions/
    dog-monitor.md       ← active
    toddler-safety.md    ← active
    morning-brief.md     ← active
    front-door-night.md  ← active (expires 07:00)
    vet-visit.md         ← paused (manual)
  missions.db            ← compiled mission state (SQLite)
  memory.db              ← agent memory (SQLite)
  memory/                ← episodic NDJSON logs
```

### 3.3 Mission Lifecycle

```
User writes mission.md
       ↓
Daemon detects file (inotify / polling)
       ↓
Setup turn fires (LLM reads mission file)
       ↓
Agent calls: configure_context, schedule_task, register_alert, register_constraint
       ↓
Daemon stores compiled config in missions.db
       ↓
Context providers start, jobs scheduled, alerts armed
       ↓
Agent runs autonomously
       ↓                              ↓
mission.md changes             mission expires / completes
       ↓                              ↓
Setup re-run (that mission only)  Daemon disables, archives
```

---

## 4. The Mission Model

### 4.1 Mission Schema

Each mission compiles from markdown to a structured record:

```rust
struct Mission {
    id: String,                        // derived from filename
    markdown: String,                  // source of truth, user-editable
    status: MissionStatus,             // active | paused | completed | expired | failed
    trigger: MissionTrigger,           // when to activate
    expires_at: Option<i64>,           // hard timeout (epoch secs)
    duration_secs: Option<u64>,        // relative TTL from activation
    completes_when: Option<String>,    // world state predicate
    on_failure: FailureAction,         // stop | retry | alert | spawn
    resources: Vec<String>,            // exclusive locks (e.g. "robot_arm")
    constraints: Vec<Constraint>,      // safety bounds (workspace, velocity)
    sub_missions: Vec<String>,         // ordered DAG children (mission IDs)
    depends_on: Vec<String>,           // DAG parents (must complete first)
    compiled_at: i64,                  // when this was last compiled
    context_providers: Vec<ProviderConfig>,
    jobs: Vec<JobConfig>,
    alerts: Vec<AlertConfig>,
}

enum MissionTrigger {
    Immediate,
    Cron(String),                      // cron expression
    WorldState(String),                // predicate on world state
    DependsOn(Vec<String>),            // all named missions must complete
    Time(i64),                         // specific epoch seconds
}

enum FailureAction {
    Stop,                              // halt all sub-missions
    Retry { max_attempts: u32 },
    Alert { message: String },
    Spawn { mission_id: String },      // activate a fallback mission
}
```

### 4.2 Temporal Missions

Missions can be time-bounded without any LLM involvement after compilation:

```markdown
# Front Door Watch
Watch the front door camera for the next 24 hours.
Alert if anyone unknown arrives or if the door opens after 10pm.
expires: 24h
```

The daemon tracks `expires_at` and auto-disables the mission when time is up. No agent turn
needed for expiry — it is a daemon-side operation evaluated on every heartbeat tick.

### 4.3 Mission DAG

Goals decompose into directed acyclic graphs of sub-missions. The agent compiles the DAG; the
daemon evaluates it:

```markdown
# Navigate A to B
Take the robot from the charging dock to the kitchen counter.

Safety: stop immediately if any person detected within 1.5m.
Timeout: 5 minutes.
resources: [robot_arm, robot_base]
```

Agent compiles to:

```
navigate-dock-to-kitchen (parent, expires: 5min)
  ├── sub-001: navigate to hallway
  │     trigger: immediate
  │     completes_when: robot.position ≈ hallway_waypoint (±0.1m)
  │     expires: 90s
  │     on_failure: stop → alert_agent
  ├── sub-002: navigate to kitchen
  │     trigger: depends_on [sub-001]
  │     completes_when: robot.position ≈ kitchen_counter (±0.1m)
  │     expires: 90s
  │     on_failure: stop → alert_agent
  └── safety-monitor (parallel, always active)
        trigger: immediate
        completes_when: parent completes
        reactive_rule: person.distance < 1.5m → stop_vla immediately
```

### 4.4 Mission Relations

| Relation | Mechanism | LLM involvement |
|----------|-----------|-----------------|
| Sequential (A then B) | `depends_on: [A]` | Compilation only |
| Parallel | No dependency | Compilation only |
| Conditional (if X then Y) | `trigger: WorldState(predicate)` | Compilation only |
| Mutually exclusive | `resources: [shared_resource]` | Compilation only |
| Suspend/resume | `on_failure: Spawn("fallback")` | Compilation only |
| Time-bounded | `expires_at` or `duration_secs` | Compilation only |

After the setup turn, **zero LLM involvement** in DAG evaluation. The daemon ticks through the
DAG on every heartbeat, evaluating predicates against world state.

### 4.5 Why Not Behavior Trees

Behavior trees solve the same problems as the mission DAG but with fundamental limitations:

**BT limitations:**
- Rigid schema — novel situations require hand-coding new nodes
- No natural language interface — users can't write BTs
- No memory integration — BTs don't ground decisions in historical context
- No graceful degradation — BTs have no notion of "try local LLM if cloud is down"

**Mission DAG advantages:**
- Compiled from natural language — user writes markdown
- Intelligence at planning time (LLM), determinism at execution time (daemon)
- Grounded in world state — decisions based on sensor evidence
- Failure handling is first-class — `on_failure` is a first-class field
- Re-planning is natural — mission fails → agent turn → new sub-missions created

The key insight: **BTs are a handwritten planner. LLMs are a better planner.** The BT is
redundant when you have a reasoning system that can compile the same structure from natural
language, grounded in live sensor state.

---

## 5. Four-Tier Memory Model

### 5.1 Overview

The existing three-tier memory (short-term, episodic, semantic) is extended with a new
foundational tier:

| Tier | Name | Storage | Writer | Reader | Purpose |
|------|------|---------|--------|--------|---------|
| 0 | World State | SQLite (RAM-cached) | Daemon (continuous) | Agent (per turn) | Current physical reality |
| 1 | Short-term | RAM Vec\<Message\> | Agent | Agent | Active turn context |
| 2 | Episodic | NDJSON + FTS5 | Agent | Agent | Conversation + event log |
| 3 | Semantic | SQLite | Agent + Daemon | Agent | Jobs, proposals, beliefs |

### 5.2 Tier 0: World State

The world state is a continuously-updated structured snapshot of physical reality. It is written
by daemon background tasks (context providers), never by the LLM. The LLM reads it.

**Schema:**

```sql
CREATE TABLE world_state (
    key          TEXT PRIMARY KEY,      -- e.g. "dog.location", "robot.position"
    value        TEXT NOT NULL,         -- JSON-encoded value
    confidence   REAL DEFAULT 1.0,      -- 0.0-1.0, decays over time
    source_topic TEXT,                  -- Zenoh topic that produced this value
    source_node  TEXT,                  -- node name (e.g. "vision-detector")
    last_seen_at INTEGER NOT NULL,      -- epoch seconds
    max_age_secs INTEGER DEFAULT 300,   -- staleness threshold
    mission_id   TEXT,                  -- which mission owns this key (nullable)
    created_at   INTEGER NOT NULL
);

CREATE INDEX idx_world_state_stale
    ON world_state(last_seen_at)
    WHERE last_seen_at < (strftime('%s','now') - max_age_secs);
```

**Staleness handling:** Every entry has `max_age_secs`. On each agent turn, the daemon marks
entries where `now - last_seen_at > max_age_secs` as stale. The agent prompt includes:

```
WORLD STATE (2026-03-08 14:23:01 UTC):
  dog.location   = "kitchen"   [confidence: 0.97, 2min ago]
  dog.last_fed   = "08:00"     [confidence: 0.91, 6h ago]
  ⚠️ robot.position = [0.3, 0.1] [STALE: 45s, max_age: 10s]
```

**Confidence decay:** Physical observations decay in confidence over time. The decay rate is
configurable per key and per mission. A dog location observation decays to 0.5 confidence
after 30 minutes (dog could have moved). A door open/close event decays immediately (binary
state, must be refreshed). This prevents the agent from acting on stale beliefs with high
confidence.

### 5.3 Tier 2: Episodic — Causal Chains

The existing episodic log stores flat `{timestamp, role, content}` entries. This is extended
with causal chain metadata:

```rust
struct LogEntry {
    id: String,                        // UUID
    timestamp: String,                 // RFC 3339
    role: String,                      // user | assistant | tool | system | event
    content: String,                   // text content
    // Causal chain fields (new)
    cause_id: Option<String>,          // UUID of the event that triggered this
    effect_ids: Vec<String>,           // UUIDs of resulting entries
    world_state_snapshot: Option<String>, // JSON snapshot at time of entry
    mission_id: Option<String>,        // which mission was active
    salience: f32,                     // 0.0-1.0 importance score
    // Existing fields
    job_id: Option<String>,
    flush: Option<bool>,
}
```

**Causal chain query:** When the agent wants to understand a past event, FTS5 search returns
not just the matching entry but the full causal chain:

```
Query: "motor overheat"
Result:
  [2026-03-08 09:14] EVENT:  motor.temperature = 78°C (threshold: 75°C)
  [2026-03-08 09:14] AGENT:  Detected motor overheat → reducing speed to 30%
                               cause_id: motor_temp_event
  [2026-03-08 09:16] EVENT:  motor.temperature = 71°C (resolved)
                               cause_id: speed_reduction_action
  [2026-03-08 09:16] AGENT:  Motor temperature nominal. Restoring speed.
```

This gives the agent a complete picture of what happened and why — not just isolated log lines.

### 5.4 Tier 3: Semantic — Beliefs

The semantic store (currently `jobs` + `proposals` tables) is extended with a `beliefs` table:

```sql
CREATE TABLE beliefs (
    id              TEXT PRIMARY KEY,
    subject         TEXT NOT NULL,         -- e.g. "dog", "motor_1"
    predicate       TEXT NOT NULL,         -- e.g. "eats_at", "temperature_limit"
    value           TEXT NOT NULL,         -- e.g. "08:00,18:00", "75"
    confidence      REAL DEFAULT 1.0,
    source          TEXT NOT NULL,         -- "sensor", "agent", "user"
    first_observed  INTEGER NOT NULL,      -- epoch seconds
    last_confirmed  INTEGER NOT NULL,      -- epoch seconds
    confirmation_count INTEGER DEFAULT 1,
    contradiction_count INTEGER DEFAULT 0,
    notes           TEXT                   -- agent's reasoning when belief was formed
);
```

**Belief update loop:** The daemon's context provider, when writing to world state, also
checks the beliefs table. If a new observation is consistent with an existing belief,
`confirmation_count` increments and confidence increases. If it contradicts (dog seen at bowl
at 6pm, but belief says "dog no longer eats at 6pm"), `contradiction_count` increments and
confidence decreases. When confidence falls below a threshold, the agent gets a turn to
evaluate and potentially retract the belief.

**Belief injection into prompts:**

```
ACTIVE BELIEFS:
  dog eats_at "08:00,18:00"  [confidence: 0.87, 142 confirmations, source: sensor]
  dog location_preference "kitchen"  [confidence: 0.91, 89 confirmations]
  motor_1 temperature_limit "75°C"   [confidence: 0.99, user-defined]
  ⚠️ dog eats_at "18:00"  [WEAKENING: confidence 0.23, 8 contradictions in last 14 days]
```

### 5.5 Salience-Weighted Forgetting

Not all episodic log entries are equally important. Physical events (overcurrent, fall
detected, person at door) have high salience and should be retained longer. Conversational
exchanges (user asked for status, agent responded) have low salience and decay faster.

**Salience scoring (heuristic, no LLM):**

| Event type | Base salience | Retention |
|------------|---------------|-----------|
| Safety event (overcurrent, fall) | 1.0 | 90 days |
| Mission failure | 0.9 | 30 days |
| Anomaly detected | 0.8 | 14 days |
| Successful task completion | 0.6 | 7 days |
| Status query / response | 0.2 | 1 day |
| Heartbeat check (no change) | 0.0 | 6 hours |

Salience is computed by the daemon at write time based on event type — no LLM call. The
existing `prune_old_logs` mechanism is extended to prune based on `salience × age` rather
than age alone.

---

## 6. Context Providers

### 6.1 The Provider Model

A context provider is a daemon background task that watches a Zenoh topic and maintains a
slice of the world state. Providers are registered by the agent via the `configure_context`
tool and persisted to SQLite — they survive daemon restarts.

**Provider configuration (stored in missions.db):**

```rust
struct ProviderConfig {
    id: String,
    mission_id: String,
    topic_pattern: String,             // Zenoh key expression (may include **)
    world_state_key_template: String,  // e.g. "{label}.location"
    value_field: String,               // JSON path into published message
    min_interval_secs: u32,            // rate limit (decimation)
    max_age_secs: u32,                 // staleness threshold
    confidence_field: Option<String>,  // JSON path for confidence value
    filter: Option<String>,            // e.g. "label=dog AND confidence>0.85"
    token_budget: u32,                 // max tokens this provider contributes to prompt
}
```

**Provider lifecycle:**

```
configure_context tool call
        ↓
Daemon stores ProviderConfig in missions.db
        ↓
Background task spawned (tokio task)
        ↓
Zenoh subscriber declared on topic_pattern
        ↓
On each sample:
  1. Apply filter (e.g. confidence > 0.85)
  2. Check min_interval_secs (rate limit)
  3. Extract value via value_field JSON path
  4. Write to world_state table
  5. Update belief table if consistent/contradictory
```

### 6.2 Schema Validation at Registration

When `configure_context` is called with a `value_field`, the daemon immediately queries the
node's schema queryable (`{node}/schema`) and validates that the field path exists in the
published message schema. If the field does not exist:

```
Error: configure_context failed — field "dog.location" not found in
schema for topic "bubbaloop/home/jetson/vision/detections".
Available fields: label (string), confidence (float), bbox (BoundingBox),
timestamp (uint64).
Suggestion: use "label" for subject and "bbox.center" for location.
```

This prevents silent failures where the world state key is never populated.

### 6.3 Token Budget Enforcement

Each provider has a `token_budget`. The total Tier 0 snapshot injected into the agent prompt
is hard-capped. If multiple providers compete for budget:

1. Safety-critical keys (flagged at registration) always included
2. Most recently updated keys prioritized
3. Stale keys (above `max_age_secs`) summarized rather than listed
4. Lowest-salience keys truncated if budget exceeded

Example budget allocation (500 token total cap):
- World state snapshot: 200 tokens
- Active beliefs: 100 tokens
- Stale warnings: 50 tokens
- Active mission summary: 100 tokens
- Available for conversation: remaining

### 6.4 Built-in Providers

Beyond the configurable `ZenohContextProvider`, two providers are always active:

**JobProvider** — injects pending jobs and upcoming schedule:
```
SCHEDULED JOBS:
  morning-brief: next run 08:00 (6h 23min)
  dog-feed-check: next run 18:00 (2h 41min)
```

**TelemetryProvider** — injects resource state (already built, telemetry watchdog):
```
SYSTEM: CPU 34% | RAM 2.1/8.0 GB | Temp 52°C | Disk 14/64 GB
```

---

## 7. The Reactive Pre-filter

### 7.1 Problem

The agent heartbeat is slow by design (60s at rest). But physical events need faster response:
a toddler near stairs shouldn't wait 60 seconds for the agent to notice.

### 7.2 Solution

A daemon-side rule engine evaluates world state predicates continuously. No LLM involved.
When a rule fires, it spikes the agent's arousal, shrinking the heartbeat interval to as low
as 5 seconds. The rule engine never *acts* — it only escalates.

**Rule configuration:**

```rust
struct ReactiveRule {
    id: String,
    mission_id: String,
    predicate: String,            // world state predicate
    debounce_secs: u32,          // prevent flapping (fire at most once per N secs)
    arousal_boost: f64,           // how much to spike arousal (1.0-5.0)
    description: String,          // injected into next agent turn: "Rule fired: ..."
}
```

**Example rules compiled from mission:**

```markdown
# Toddler Safety
Alert if toddler is near the stairs.
```

Compiled to:
```rust
ReactiveRule {
    predicate: "toddler.near_stairs == true AND toddler.confidence > 0.8",
    debounce_secs: 30,
    arousal_boost: 4.0,
    description: "Toddler detected near stairs",
}
```

The reactive layer is not a replacement for the agent — it ensures the agent *wakes up fast
enough* to handle the situation.

### 7.3 Separation of Concerns

| Layer | Responsibility | Timescale |
|-------|---------------|-----------|
| Reactive pre-filter | Detect, escalate | <10ms |
| Agent (LLM) | Reason, decide, act | 1-5s |
| Mission DAG | Track goals, check completion | per heartbeat |

The pre-filter never calls tools, never modifies state, never sends notifications. It only
adjusts the heartbeat interval and prepends a description to the next agent turn's context.

---

## 8. Safety Architecture

### 8.1 Principle: Safety Cannot Depend on the LLM

For safety-critical applications, the LLM is too slow, too stochastic, and too dependent on
external services. Safety guarantees must be enforced by the daemon, independent of whether
the LLM is available.

### 8.2 Constraint Engine

Missions declare safety constraints at compilation time. The daemon enforces them before any
tool call output reaches a node:

```rust
enum Constraint {
    Workspace {
        x: (f64, f64),
        y: (f64, f64),
        z: (f64, f64),
    },
    MaxVelocity(f64),
    ForbiddenZone {
        center: [f64; 3],
        radius: f64,
    },
    MaxForce(f64),
    ResourceLock { resource: String },
    Custom { predicate: String },    // evaluated against world state
}
```

When an agent attempts to publish a goal to the VLA node via Zenoh, the daemon intercepts the
message and validates it against all active constraints for that mission. Violations are
rejected before reaching hardware:

```
CONSTRAINT VIOLATION: Agent goal rejected.
  Goal: navigate_to([0.95, 0.0, 0.5])
  Violation: workspace.x limit [−0.8, 0.8] exceeded (0.95 > 0.8)
  Mission: navigate-dock-to-kitchen
  Action: goal rejected, agent notified, mission paused
```

### 8.3 Resource Locking

Missions that control shared physical resources declare exclusive locks. The daemon enforces
mutual exclusion — a second mission cannot command the same resource while the first holds
the lock:

```markdown
# Pick and Place
resources: [robot_arm, robot_base]
```

If a second mission tries to acquire `robot_arm` while `navigate-a-to-b` holds it:

```
RESOURCE CONFLICT: Mission "cleanup-task" cannot acquire lock "robot_arm".
  Current holder: navigate-a-to-b (expires: 3min 22s or on completion)
  Action: cleanup-task queued (will activate when lock released)
```

### 8.4 Compiled Fallback Actions

Every mission's `on_failure` field is compiled to daemon-executable actions — no LLM call
during failure:

```rust
enum CompiledFallback {
    StopVla,                      // publish stop command immediately
    PauseAllMissions,             // suspend mission DAG
    AlertAgent,                   // fire arousal spike for agent turn
    PublishZenoh { topic, payload }, // send custom Zenoh message
    Sequence(Vec<CompiledFallback>), // execute in order
}
```

Failure response is deterministic and instant. The LLM re-plans asynchronously.

### 8.5 Graceful Degradation Ladder

```
Level 4: Cloud LLM (Claude)       full reasoning, mission compilation
         ↓ if unavailable
Level 3: Local LLM (Ollama)       fallback re-planning, ~500ms (already built)
         ↓ if unavailable
Level 2: Compiled fallback actions pause missions, send alerts, stop actuators
         ↓ always active
Level 1: Daemon constraint engine  workspace bounds, resource locks, rate limits
         ↓ always active
Level 0: Firmware / VLA           current limiting, emergency stop (hardware)
```

Each level is independent. A cloud outage falls back to Ollama. An Ollama failure falls back
to compiled actions. Nothing stops the constraint engine — it runs regardless.

---

## 9. The Setup Turn

### 9.1 How It Works

When a new mission file is detected (or an existing one changes), the daemon triggers a
"setup turn" — a special agent turn that reads the mission markdown and calls configuration
tools. Setup turns do not use conversation history and do not contribute to episodic memory.

**Setup turn system prompt:**

```
You are configuring yourself based on a new mission file.
Read the mission carefully and call the appropriate tools to wire up
your context providers, schedules, alerts, and constraints.

Available tools: configure_context, schedule_task, register_alert,
register_constraint, install_node, list_nodes.

Current node inventory:
  - rtsp-camera (running, topic: bubbaloop/home/jetson/rtsp-camera/*)
  - vision-detector (running, topic: bubbaloop/home/jetson/vision/*)

Mission file: dog-monitor.md
---
Watch the kitchen camera. Alert if dog hasn't eaten by 6pm.
```

The agent reads the mission, discovers available nodes via `list_nodes`, checks their schemas,
and calls the configuration tools. The setup turn is the only LLM involvement in mission
wiring — everything after is daemon-evaluated.

### 9.2 Setup Turn for Node Discovery

The agent can call `install_node` during setup if a required node is not running. The full
sequence for a new mission:

```
1. Daemon detects dog-monitor.md
2. Setup turn fires
3. Agent calls list_nodes() → sees rtsp-camera is running
4. Agent calls install_node("vision-detector") if not present
5. Agent queries vision-detector schema → sees "label", "confidence", "bbox" fields
6. Agent calls configure_context({topic: "**/vision/detections", filter: "label=dog", ...})
7. Agent calls register_alert({condition: "dog.last_fed > 8h", message: "..."})
8. Agent calls schedule_task({cron: "0 18 * * *", prompt: "Has the dog eaten today?"})
9. Setup turn complete — daemon stores config, starts providers
```

User wrote one markdown file. System is now fully configured.

### 9.3 Mission Invalidation and Re-compilation

When `mission.md` changes on disk, only that mission is re-compiled. Other missions continue
running. The re-compilation:

1. Pauses the mission and its sub-missions
2. Tears down existing context providers for that mission
3. Runs a new setup turn
4. Restores active state if the mission was active

---

## 10. Federated Agents

### 10.1 Problem

Current multi-agent systems (AutoGen, CrewAI) assume shared memory on one machine. Physical
AI deployments have agents on different hardware nodes (different Jetsons, different
locations) that need to coordinate without a central server.

### 10.2 Zenoh as Federation Bus

Bubbaloop agents communicate through Zenoh's built-in routing. Each daemon publishes its
world state snapshot on a well-known Zenoh topic:

```
bubbaloop/{scope}/{machine_id}/agent/{agent_id}/world_state
```

Other agents subscribe to these topics and merge them into their local world view. No
central coordinator. No shared database. The federation is implicit in the topic tree.

### 10.3 World State Gossip Protocol

Each agent publishes a compact world state diff (not full snapshot) on a configurable interval:

```json
{
  "agent_id": "kitchen-watcher",
  "machine_id": "jetson-kitchen",
  "timestamp": 1741449600,
  "updates": [
    {"key": "dog.location", "value": "kitchen", "confidence": 0.97},
    {"key": "dog.last_fed", "value": "08:00", "confidence": 0.91}
  ]
}
```

Remote agents merge these updates into their local world state with a `remote:` prefix:

```
WORLD STATE:
  dog.location = "kitchen"        [local, 2min ago]
  remote:jetson-hall.dog.location = "hallway"  [remote:jetson-hall, 5min ago]
```

### 10.4 Quorum Memory

For safety-critical observations, a single agent's observation may not be sufficient.
Quorum memory requires N independent agents to confirm before a belief is accepted into
shared semantic memory:

```rust
struct QuorumConfig {
    key_pattern: String,           // which world state keys require quorum
    required_agents: u32,          // min confirmations (e.g. 2)
    agreement_window_secs: u32,    // all confirmations must arrive within this window
    quorum_action: QuorumAction,   // what happens when quorum is reached
}

enum QuorumAction {
    UpdateBelief,                  // write to beliefs table with high confidence
    FireAlert,                     // immediate notification
    TriggerMission(String),        // activate a mission
}
```

Example: `toddler.near_stairs` requires confirmation from 2 cameras before alert fires.
Prevents false positives from a single camera's misdetection.

### 10.5 Role-Based Topic Scoping

Each agent role subscribes only to topics relevant to its mission. A kitchen watcher agent
subscribes to `bubbaloop/home/*/vision/detections` but not to
`bubbaloop/home/*/robot/state`. This keeps the world state bounded regardless of fleet size.

The total world state for any single agent scales with its role scope, not with fleet size.

---

## 11. Micro-turns for Plan Validation

### 11.1 Problem

Mission DAGs compiled at setup time may become invalid as the world changes. A navigation
goal compiled for an obstacle-free path may be invalid 2 hours later when an obstacle appears.

### 11.2 Micro-turns

Before activating each sub-mission in a DAG, the daemon triggers a "micro-turn" — a minimal
LLM call with only world state + sub-mission description. No conversation history. No episodic
recall. No job list. Budget: ~200 tokens in, ~50 tokens out.

```
Micro-turn prompt:
  Current world state: { robot.position: [0.1, 0.0], obstacle_detected: true,
                         obstacle_position: [0.3, 0.0] }
  About to execute: navigate to [0.3, 0.0]
  Is this sub-mission still valid? Reply: VALID or INVALID [reason]
```

If `INVALID`: sub-mission is skipped, parent mission's `on_failure` fires, agent turn queued
for re-planning. If `VALID`: sub-mission activates. Cost: ~0.001 cents per sub-mission
activation. Prevents executing stale plans without full re-planning cost.

---

## 12. New MCP Tools

The following new tools are required to support this architecture. All follow existing
conventions (validate input, audit log, RBAC gating):

| Tool | RBAC | Description |
|------|------|-------------|
| `configure_context` | Operator | Register a Zenoh context provider with world state key mapping |
| `register_alert` | Operator | Arm a reactive alert rule (predicate → notification) |
| `register_constraint` | Admin | Add a safety constraint to an active mission |
| `list_world_state` | Viewer | Read current world state snapshot |
| `get_belief` | Viewer | Query the beliefs table for a subject/predicate |
| `update_belief` | Operator | Manually assert or retract a belief |
| `list_missions` | Viewer | List all missions and their status |
| `pause_mission` | Operator | Pause a mission (preserves compiled config) |
| `resume_mission` | Operator | Resume a paused mission |
| `cancel_mission` | Operator | Cancel and archive a mission |
| `acquire_resource` | Operator | Manually acquire a resource lock |
| `release_resource` | Operator | Release a resource lock |

---

## 13. Binary Size Budget

A core constraint: the full system must remain deployable as a ~12–13 MB binary.

| Component | Estimated addition | Notes |
|-----------|-------------------|-------|
| World state table | ~0 KB | New SQLite table in existing memory.db |
| Belief table | ~0 KB | New SQLite table |
| Causal chain fields | ~0 KB | New columns in existing FTS5 schema |
| Context provider runtime | ~50 KB | Zenoh subscriber management, Rust |
| Reactive rule engine | ~30 KB | Predicate evaluator, Rust |
| Constraint engine | ~40 KB | Workspace/velocity validation, Rust |
| Mission DAG runtime | ~60 KB | DAG traversal, lock management |
| New MCP tools (~12) | ~20 KB | Thin handler functions |
| **Total addition** | **~200 KB** | |
| **Projected binary** | **~13.2 MB** | Within budget |

No new crate dependencies required. Constraint evaluation uses existing SQLite queries.
Predicate parsing uses a minimal hand-written evaluator (no parser combinator crate).

---

## 14. Open Problems

### 14.1 Predicate Language

The `completes_when`, reactive rule predicates, and constraint expressions all need a common
predicate language. Options:
- **SQL WHERE clause** over the world_state table (familiar, existing SQLite)
- **Simple expression language** (hand-written, ~30KB, no dependencies)
- **Lua scripting** (flexible, ~200KB addition, sandboxable)

Recommendation: SQL WHERE clause for v1 (zero new code, SQLite already linked).

### 14.2 Belief Conflict Resolution

When two agents in a federated setup have contradictory beliefs about the same subject, the
resolution strategy is unclear. Options: last-writer-wins, highest-confidence-wins,
quorum-required. This requires empirical study across use cases.

### 14.3 Micro-turn Latency for Fast Missions

For robot missions with many sub-missions (20+ steps), micro-turn validation could add
significant latency. A local LLM (Ollama) could be used for micro-turns while cloud LLM
handles full re-planning. The ModelProvider trait already supports this.

### 14.4 Mission Markdown Schema

The agent must reliably extract structured fields (expires, resources, constraints) from
freeform markdown. This works well for simple missions but may fail on complex ones.
A lightweight markdown front-matter standard (YAML block at top of mission file) could
make extraction deterministic without requiring an LLM call for parsing.

---

## 15. Summary

This design introduces four primary contributions:

1. **Identity/Mission separation** — the user interface is markdown. Identity shapes
   personality, mission shapes behavior. Missions compile to executable daemon configuration.

2. **LLM-compiled reactive mission DAG** — a replacement for behavior trees where the LLM
   provides intelligence at planning time and the daemon provides determinism at execution time.
   No BT formalism, no hand-coded nodes, no rigid schemas.

3. **Four-tier sensor-grounded memory** — Tier 0 (live world state), causal chain episodic
   memory, belief update loop, and salience-weighted forgetting. Memory is confirmed or
   contradicted by sensor evidence, not just conversation history.

4. **Daemon-enforced safety layer** — constraint engine, resource locking, compiled fallback
   actions, and graceful degradation ladder. Safety guarantees that hold regardless of LLM
   availability.

Together these enable a unified agent architecture deployable on a 12 MB binary that serves
home IoT, mobile robotics, and software automation from a single codebase, with a user
interface that is a markdown file.
