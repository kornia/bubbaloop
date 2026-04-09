---
description: "Bubbaloop 3-tier agent memory system. Short-term RAM, episodic NDJSON with FTS5 search, and long-term SQLite semantic memory for persistent context."
---

# Memory & Mission Engine

The LLM is expensive and slow — everything that doesn't need reasoning runs without it.

Bubbaloop's memory stack gives agents **persistent awareness** of the physical world across conversations, sessions, and reboots.

---

## Overview

```
┌─────────────────────────────────────────────────────┐
│  Tier 0 — World State (live, sensor-driven)          │
│  SQLite world_state table                            │
│  Written by: Context Providers (no LLM, <1ms)       │
│  Read by: injected at top of every agent turn        │
├─────────────────────────────────────────────────────┤
│  Tier 1 — Short-term (RAM)                          │
│  Vec<Message> — current conversation only           │
├─────────────────────────────────────────────────────┤
│  Tier 2 — Episodic (NDJSON + FTS5)                  │
│  daily_logs_YYYY-MM-DD.jsonl — past events          │
│  BM25 search with temporal decay                    │
├─────────────────────────────────────────────────────┤
│  Tier 3 — Semantic (SQLite)                         │
│  Beliefs + jobs + proposals + episodic_meta         │
└─────────────────────────────────────────────────────┘
```

**Key principle:** The agent always knows what's happening right now (Tier 0), can recall past events (Tier 2), and holds durable knowledge about the world (Tier 3 beliefs). The LLM only writes Tiers 1–3. Tier 0 is written by the daemon without LLM involvement.

---

## Tier 0: World State

The agent's real-time awareness of the physical environment.

### What it is

A SQLite table of key/value pairs that gets **prepended to the system prompt** before every LLM turn. The agent sees current sensor readings without any tool calls.

```
[World State]
person.location = "hallway"     (confidence: 0.92, age: 3s)
robot_arm.location = "home"     (confidence: 0.8,  age: 12s)
front_door.status = "closed"    (confidence: 0.99, age: 1s)
```

### Who writes it: Context Providers

Context Providers are daemon background tasks that subscribe to Zenoh topics and write to world state — no LLM involved. Each provider is configured with `configure_context` (Admin):

```
configure_context
  mission_id="security-patrol"
  topic_pattern="bubbaloop/**/vision/detections"
  world_state_key_template="{label}.location"
  value_field="label"
  filter="confidence>0.8"
  min_interval_secs=1
  max_age_secs=30
```

**How it works:**

```
Vision node publishes to Zenoh:
  {"label": "person", "confidence": 0.92, "zone": "hallway"}
          ↓
Context Provider (filter: confidence>0.8) matches
          ↓
Writes: world_state["person.location"] = "hallway"
          ↓ (within milliseconds, no LLM)
Next agent turn sees: "person.location = hallway" in its prompt
```

**Filter syntax:** `field=value AND field2>number`
- Equality: `label=person`
- Numeric: `confidence>0.8`, `temperature<50`
- Combined: `label=dog AND confidence>0.85`

**Key template:** `{field}` substitutes from the payload.
- `{label}.location` → `"person.location"` when `label="person"`
- `arm.{joint_id}.angle` → `"arm.shoulder.angle"` when `joint_id="shoulder"`

### Reading world state

```
list_world_state
→ [
    {"key": "person.location", "value": "hallway", "confidence": 0.92, "updated_at": 1772990123},
    {"key": "robot_arm.location", "value": "home", "confidence": 0.8, "updated_at": 1772990111}
  ]
```

---

## Tier 3: Beliefs

Durable knowledge the agent holds about the world that doesn't come from live sensors — things that are true for hours, days, or permanently.

### Data model

Every belief is a **subject + predicate → value** triple with a confidence score:

| Field | Example | Notes |
|-------|---------|-------|
| `subject` | `"front_door_camera"` | The entity |
| `predicate` | `"is_reliable"` | The property |
| `value` | `"true"` | The current value |
| `confidence` | `0.95` | 0.0–1.0 |
| `source` | `"heartbeat_monitor"` | Optional: how this was formed |
| `confirmation_count` | `3` | Incremented on each re-confirmation |
| `contradiction_count` | `0` | Incremented on each contradiction |
| `first_observed` | `1772990085` | Unix epoch |
| `last_confirmed` | `1772990200` | Updated on re-confirmation |

### Creating and updating beliefs

```
update_belief
  subject="front_door_camera"
  predicate="is_reliable"
  value="true"
  confidence=0.95
  source="heartbeat_monitor"
→ Belief (front_door_camera, is_reliable) updated with confidence 0.95
```

Calling `update_belief` again with the same subject+predicate increments `confirmation_count` and updates `confidence`. Calling it with a different value increments `contradiction_count`.

### Reading beliefs

```
get_belief subject="front_door_camera" predicate="is_reliable"
→ {
    "id": "belief-a3135f9d-...",
    "subject": "front_door_camera",
    "predicate": "is_reliable",
    "value": "true",
    "confidence": 0.95,
    "source": "heartbeat_monitor",
    "first_observed": 1772990085,
    "last_confirmed": 1772990200,
    "confirmation_count": 3,
    "contradiction_count": 0,
    "notes": null
  }

get_belief subject="nonexistent" predicate="nothing"
→ "not found"
```

### Belief decay

A daemon background task (`spawn_belief_decay_task`) periodically reduces `confidence` by the configured rate. A belief confirmed daily by a sensor stays high; a belief not revisited for weeks decays toward 0.

---

## Missions vs Tasks

These two concepts are distinct and complementary:

| | Task (`schedule_task`) | Mission |
|---|---|---|
| Duration | Momentary | Days / weeks / ongoing |
| Granularity | One action at a scheduled time | Many actions over time |
| State | `pending → running → completed/failed` | `active → paused → completed/failed/cancelled` |
| Trigger | Time-based (cron or one-shot) | Always present until completed |
| Safety constraints | None | Per-mission limits attached |
| Storage | `memory.db` `jobs` table | `missions.db` |
| Example | "send status report at 09:00" | "monitor the front entrance 24/7" |

**Relationship:** a mission *can spawn tasks*. For example, the `security-patrol` mission might call `schedule_task` to send an hourly summary report — the mission is the persistent intent, the task is the concrete timed action it produces. The agent wires this relationship itself via tool calls; there is currently no hard foreign key in the DB linking a job to its parent mission.

---

## Missions

Missions are the unit of persistent agent intent — goals that span multiple conversations and survive restarts.

### How missions are created

Drop a markdown file into `~/.bubbaloop/agents/{agent-id}/missions/`. The filename stem becomes the mission ID:

```
~/.bubbaloop/agents/jean-clawd/missions/
├── security-patrol.md      →  mission ID: "security-patrol"
├── dog-monitor.md          →  mission ID: "dog-monitor"
└── maintenance-check.md    →  mission ID: "maintenance-check"
```

The daemon watches this directory (5-second poll) and picks up new and changed files automatically. The file content is stored as-is in SQLite as the mission's markdown.

> **Note:** The file-watcher is implemented and tested. Missions are currently inserted via the MCP platform layer; direct file-drop support is being wired into the agent runtime in the next release.

### Mission lifecycle

```
         ┌─────────┐
    ───►  │ Active  │ ◄──── resume_mission
         └────┬────┘
              │ pause_mission
         ┌────▼────┐
         │ Paused  │
         └────┬────┘
              │ cancel_mission (or auto-expiry)
    ┌─────────┼──────────────────┐
    ▼         ▼                  ▼
Cancelled  Completed           Failed
```

States: `active | paused | cancelled | completed | failed`

### MCP control tools

```
list_missions
→ [
    {"id": "security-patrol", "status": "active",  "compiled_at": 1772990000, "expires_at": null},
    {"id": "dog-monitor",     "status": "paused",  "compiled_at": 1772989000, "expires_at": null}
  ]

pause_mission   mission_id="security-patrol"   →  "Mission security-patrol paused"
resume_mission  mission_id="security-patrol"   →  "Mission security-patrol resumed"
cancel_mission  mission_id="security-patrol"   →  "Mission security-patrol cancelled"

# Unknown mission ID returns a graceful error (not a crash):
pause_mission   mission_id="nonexistent"       →  "Error: mission not found"
```

### Mission DAG (dependencies)

Missions can depend on other missions. The `DagEvaluator` only activates a mission when all its `depends_on` missions are `Completed`:

```
calibrate-arm  ──depends_on──►  run-welding-task
```

Before activating a sub-mission, the daemon makes a **micro-turn** — a single cheap LLM call with no tools and no episodic write — to validate preconditions. This catches obvious mistakes without burning a full agent turn.

---

## Safety: Constraints

Per-mission safety limits that are checked **synchronously and fail-closed** before any actuator command. If the validator errors, the command is denied.

### Registering constraints

```
register_constraint
  mission_id="robot-arm-task"
  constraint_type="workspace"
  params_json='{"x": [-1.0, 1.0], "y": [-1.0, 1.0], "z": [0.0, 2.0]}'
→ "Constraint registered for mission robot-arm-task"
```

### Constraint types

| Type | `params_json` format | Description |
|------|---------------------|-------------|
| `workspace` | `{"x": [min, max], "y": [min, max], "z": [min, max]}` | Axis-aligned bounding box |
| `max_velocity` | `1.5` | Maximum speed in m/s |
| `forbidden_zone` | `{"center": [x, y, z], "radius": r}` | Spherical exclusion zone |
| `max_force` | `50.0` | Maximum force in Newtons |

### Listing constraints

```
list_constraints mission_id="robot-arm-task"
→ [
    {"constraint": {"Workspace": {"x": [-1.0, 1.0], "y": [-1.0, 1.0], "z": [0.0, 2.0]}}},
    {"constraint": {"MaxVelocity": 1.5}}
  ]
```

### What happens on violation

`ConstraintEngine::validate_position_goal()` returns `Allow | Deny | ValidatorError`. On denial, a `CompiledFallback` fires:

| Fallback | What it does |
|----------|-------------|
| `StopActuators` | Sends stop command to all actuators |
| `PauseAllMissions` | Pauses every active mission |
| `AlertAgent` | Spikes agent arousal to trigger a turn |
| `HaltAndWait` | Stops all action until human intervention |

There is deliberately **no "publish arbitrary Zenoh message" fallback** — all actions are pre-enumerated at compile time.

### Resource locking

`ResourceRegistry` ensures two missions cannot simultaneously command the same actuator. Locks are held by a `ResourceGuard` that releases automatically on drop (Rust RAII — no explicit unlock needed).

---

## Reactive Alerts

Fast pre-filter that fires without an LLM call when world state matches a predicate.

```
register_alert
  mission_id="childproof-home"
  predicate="toddler.near_stairs = 'true'"
→ "Alert registered"
```

When the world state entry `toddler.near_stairs` becomes `"true"` (written by a vision context provider), agent arousal spikes immediately — in milliseconds, no LLM token spent. The LLM only wakes up if arousal crosses the agent's threshold.

Per-rule debounce prevents alert storms. Each rule stores its last-fired timestamp as an `AtomicI64`.

---

## The Full Data Flow

```
[Camera node]
      │ publishes frame detections to Zenoh
      ▼
[Context Provider]  (daemon, no LLM, <1ms)
      │ filter: confidence>0.8
      │ key template: {label}.location
      │ writes: world_state["person.location"] = "hallway"
      ▼
[Reactive alert check]  (no LLM, <1ms)
      │ predicate: "person.location = 'front_door'" → arousal spike?
      ▼
[Agent turn triggers]
      │ world state injected into system prompt:
      │   "person.location = front_door  (confidence: 0.94)"
      ▼
[LLM reasons]
      │ decides to act → "move arm to greet position"
      ▼
[Constraint engine]  (no LLM, <1ms)
      │ validate_position_goal([0.5, 0.2, 1.0])
      │ workspace constraint: ALLOW
      ▼
[Actuator executes]
      ▼
[Belief engine]
      │ observation confirms: arm.in_greeting_position = true
      │ update_belief → confirmation_count++
```

The LLM is called once. Everything else — sensor ingestion, alert matching, constraint validation — runs in the daemon without it.

---

## Per-Agent Isolation

Each agent has its own memory directory and database. No shared state:

```
~/.bubbaloop/agents/
├── jean-clawd/
│   ├── soul/identity.md
│   ├── memory/daily_logs_YYYY-MM-DD.jsonl
│   ├── memory.db          ← beliefs, jobs, proposals, world_state
│   └── missions/
│       ├── security-patrol.md
│       └── dog-monitor.md
└── camera-expert/
    ├── soul/
    ├── memory/
    ├── memory.db
    └── missions/
```

---

## MCP Tool Reference

| Tool | Tier | Role | Description |
|------|------|------|-------------|
| `list_world_state` | 0 | Viewer | Current world state snapshot |
| `configure_context` | 0 | Admin | Wire Zenoh topic → world state |
| `update_belief` | 3 | Operator | Create or update a belief |
| `get_belief` | 3 | Viewer | Retrieve a single belief |
| `list_missions` | — | Viewer | List missions with status |
| `pause_mission` | — | Operator | Pause an active mission |
| `resume_mission` | — | Operator | Resume a paused mission |
| `cancel_mission` | — | Admin | Cancel a mission permanently |
| `register_constraint` | — | Admin | Add a safety constraint to a mission |
| `list_constraints` | — | Viewer | List constraints for a mission |
| `register_alert` | — | Admin | Register a reactive alert rule |
| `unregister_alert` | — | Admin | Remove a reactive alert rule |
| `memory_search` | 2 | Operator | BM25 search over episodic logs |
| `memory_forget` | 2 | Admin | Remove entries from episodic memory |
| `schedule_task` | 3 | Operator | Create a one-shot or recurring job (cron). Distinct from missions — tasks are timed actions, missions are persistent goals. |
| `create_proposal` | 3 | Operator | Submit a proposal for human approval |

---

## See Also

- [Architecture](architecture.md) — full layer model and invariants
- [Agent Guide](../agent-guide.md) — configuring agents, Soul, and providers
- [Telemetry Watchdog](../guides/telemetry-watchdog.md) — edge device resource limits
