# OpenClaw Agent Rewrite Design

**Date:** 2026-03-03
**Status:** Draft (iterating)
**Scope:** Complete agent rewrite — Soul, Memory, Provider, Heartbeat, Jobs, Proposals

## Motivation

The current agent (~3,400 lines across 6 modules) was built incrementally across Phases 2-4. It works, but carries architectural debt: SQLite handles everything (conversations, events, schedules, FTS5), there's no model abstraction (hardcoded Claude), and no hot-reloadable identity.

This rewrite adopts the **Pico/Zero architecture pattern**: local-first, type-safe, file-system-as-identity. The key novelty is an **adaptive heartbeat** inspired by the human autonomic nervous system.

## Architecture Overview

```
┌─────────────────────────────────────────────────────────────┐
│  bubbaloop agent (OpenClaw rewrite)                          │
│                                                              │
│  ┌──────────────┐  ┌──────────────┐  ┌──────────────────┐  │
│  │  Soul         │  │  Heartbeat   │  │  REPL            │  │
│  │  (identity.md │  │  (adaptive   │  │  (user input)    │  │
│  │  + caps.toml) │  │   60s→5s)    │  │                  │  │
│  │  [hot-reload] │  └──────┬───────┘  └───────┬──────────┘  │
│  └──────┬───────┘         │                   │             │
│         │           tokio::select!            │             │
│         │                 │                   │             │
│         ▼           ┌─────▼───────────────────▼──┐          │
│   Arc<RwLock<Soul>> │     Agent Turn              │          │
│                     │                             │          │
│                     │  1. Read Soul (system prompt)│          │
│                     │  2. Collect state (nodes)    │◄─ Zenoh │
│                     │  3. Call LLM (provider trait) │          │
│                     │  4. Execute tools / propose  │          │
│                     │  5. Log to episodic memory   │          │
│                     └──────┬──────────────────────┘          │
│                            │                                  │
│  ┌─────────────────────────▼──────────────────────────────┐  │
│  │  Memory (3-tier)                                        │  │
│  │                                                          │
│  │  RAM: Vec<Message>     │ Short-term (one turn)          │  │
│  │  NDJSON: daily_logs    │ Episodic (append-only)         │  │
│  │  SQLite: jobs+proposals│ Semantic (stub for sqlite-vec) │  │
│  └─────────────────────────────────────────────────────────┘  │
│                                                              │
│  ┌──────────────────────────────────────────────────────────┐│
│  │  Provider Trait                                           ││
│  │  ┌─────────────────┐  ┌──────────────────┐              ││
│  │  │  ClaudeProvider  │  │  OllamaProvider  │              ││
│  │  │  (reqwest, dual  │  │  (stub, no auth) │              ││
│  │  │   auth: key+OAuth)│  │                  │              ││
│  │  └─────────────────┘  └──────────────────┘              ││
│  └──────────────────────────────────────────────────────────┘│
└──────────────────────────────────────────────────────────────┘
```

## 1. The Soul (Hot-Swappable Identity)

### Files

```
~/.bubbaloop/soul/
├── identity.md        # Base system prompt
└── capabilities.toml  # Model config, heartbeat tuning, approval mode
```

### Struct

```rust
pub struct Soul {
    pub identity: String,
    pub capabilities: Capabilities,
}

#[derive(Deserialize)]
pub struct Capabilities {
    pub model_name: String,            // "claude-sonnet-4-20250514"
    pub max_turns: usize,              // 15
    pub allow_internet: bool,          // true
    // Heartbeat tuning
    pub heartbeat_base_interval: u64,  // 60 seconds
    pub heartbeat_min_interval: u64,   // 5 seconds
    pub heartbeat_decay_factor: f64,   // 0.7
    // Approval
    pub default_approval_mode: String, // "auto" | "propose"
}
```

### Hot-Reload

- Background `tokio::task` uses `notify` crate to watch `~/.bubbaloop/soul/`
- On file change: re-read both files, update `Arc<RwLock<Soul>>`
- Agent loop reads Soul at start of each turn (no lock during LLM calls)
- Fallback: `include_str!` compiled default identity.md if files don't exist

## 2. Memory Model (3-Tier)

### Tier 1: Short-Term (RAM)

- `Vec<Message>` for the active agent turn
- Cleared after job completion or conversation end
- Contains: system prompt, user input, assistant responses, tool results

### Tier 2: Episodic (NDJSON File)

- **Location:** `~/.bubbaloop/memory/daily_logs_YYYY-MM-DD.jsonl`
- **Format:** Newline-delimited JSON, append-only
- **Rotation:** New file per day

```json
{"timestamp":"2026-03-03T10:15:00Z","role":"assistant","content":"Restarted front-door camera","job_id":"abc-123"}
{"timestamp":"2026-03-03T10:15:01Z","role":"tool","content":"{\"result\":\"ok\"}","job_id":"abc-123"}
```

**Why NDJSON over SQLite for episodic memory:**
- Human-readable (grep, tail -f, cat)
- Append-only (no corruption risk from concurrent writes)
- No schema migrations
- Trivial backup (cp the file)
- Natural daily rotation prevents unbounded growth

### Tier 3: Semantic (SQLite)

- **Location:** `~/.bubbaloop/memory.db`
- **Tables:** `jobs` + `proposals` (see schemas below)
- **Future:** `embeddings` table when sqlite-vec is integrated
- **Not used for conversations or events** — that's the episodic layer's job

### SQLite Schema

```sql
CREATE TABLE jobs (
    id TEXT PRIMARY KEY,
    cron_schedule TEXT,           -- e.g., "0 8 * * *" or NULL for one-off
    next_run_at DATETIME NOT NULL,
    prompt_payload TEXT NOT NULL, -- The instruction to execute
    status TEXT DEFAULT 'pending',-- pending, running, completed, failed
    recurrence BOOLEAN DEFAULT 0
);

CREATE TABLE proposals (
    id TEXT PRIMARY KEY,
    timestamp TEXT NOT NULL,
    skill TEXT NOT NULL,
    description TEXT NOT NULL,
    actions TEXT NOT NULL,        -- JSON array of tool calls
    status TEXT NOT NULL DEFAULT 'pending', -- pending, approved, rejected, expired
    decided_by TEXT,              -- "user" | "mcp" | "timeout"
    decided_at TEXT
);
CREATE INDEX idx_proposals_status ON proposals(status);
```

## 3. ModelProvider Trait

```rust
#[async_trait]
pub trait ModelProvider: Send + Sync {
    async fn generate(
        &self,
        messages: &[Message],
        tools: &[ToolDefinition],
    ) -> Result<ModelResponse, ProviderError>;
}

pub enum ModelResponse {
    Text(String),
    ToolCalls(Vec<ToolCall>),
}
```

### ClaudeProvider

- Raw `reqwest` to `api.anthropic.com/v1/messages`
- Dual auth: env var (`ANTHROPIC_API_KEY`) → OAuth (`sk-ant-oat01-*`) → API key file
- OAuth uses Claude CLI identity headers (user-agent, anthropic-beta)
- Carries forward all logic from existing `claude.rs` (596 lines)

### OllamaProvider (stub)

- HTTP to `localhost:11434/api/chat`
- No authentication
- Implements `ModelProvider` trait with `todo!()` body initially
- Ready for future local LLM integration

## 4. Adaptive Heartbeat (Autonomic Nervous System)

### The Novel Pattern

Unlike fixed-interval agent loops (OpenClaw: 5min, ZeroClaw: configurable), bubbaloop uses an **adaptive heartbeat** inspired by biological autonomic regulation:

```
Resting state:  ~60s interval (nothing happening)
Aroused state:  ~5s interval (events detected, user active)
Recovery:       Exponential decay back to resting (0.7x per calm beat)
```

### Algorithm

```
arousal = 0.0  (starts at rest)

each beat:
  interval = max(BASE_INTERVAL / (1.0 + arousal), MIN_INTERVAL)
  sleep(interval)

  new_events = collect_state()

  if new_events > 0:
    arousal += new_events  // spike proportional to activity
  else:
    arousal *= DECAY_FACTOR  // decay toward rest (0.7)

  if arousal < 0.01:
    arousal = 0.0  // snap to rest (avoid floating point drift)
```

### Parameters (configurable in capabilities.toml)

| Parameter | Default | Description |
|-----------|---------|-------------|
| `heartbeat_base_interval` | 60 | Resting interval in seconds |
| `heartbeat_min_interval` | 5 | Minimum interval (max arousal) |
| `heartbeat_decay_factor` | 0.7 | Decay per calm beat (0.0-1.0) |

### Arousal Sources

| Source | Arousal Boost | Rationale |
|--------|---------------|-----------|
| Node health change | +2.0 | Node going unhealthy is important |
| Node crash/restart | +3.0 | Critical event |
| User REPL input | +2.0 | Human interaction = stay alert |
| Pending job fires | +1.0 | Scheduled work |
| No events | ×0.7 | Decay toward rest |

### Cost Impact

- **Resting (quiet system):** 1 beat/min × 90% "nothing changed" ≈ 1,440 beats/day → ~$0.03/day (Haiku)
- **Active (events happening):** 12 beats/min during bursts, but bursts are short-lived
- **vs. fixed 5-min:** 288 beats/day regardless → less responsive, similar cost

## 5. Main Event Loop

```rust
// main.rs — three concurrent loops
tokio::select! {
    _ = soul_watcher(soul.clone())               => {},  // notify file watcher
    _ = heartbeat_loop(soul, db, episodic, ...)  => {},  // adaptive heartbeat + REPL
    _ = signal::ctrl_c()                         => {},  // graceful shutdown
}
```

### heartbeat_loop

The heartbeat loop uses `tokio::select!` to race between the adaptive timer and REPL input:

```
loop {
    interval = base / (1.0 + arousal)

    select! {
        _ = sleep(interval) => {
            state = collect_state()
            jobs = db.pending_jobs()

            update_arousal(state.event_count)

            if state.has_changes || !jobs.is_empty() {
                run_agent_job(state)
            }
        }

        input = repl.recv() => {
            arousal += 2.0
            run_agent_job(user_input)
        }
    }
}
```

### run_agent_job (State Machine)

```
1. Hydrate:
   - Read Soul (system prompt + capabilities)
   - Build context (job payload OR user input + node state)

2. Cycle (max_turns from capabilities, default 15):
   - Call provider.generate(messages, tools)
   - Append response to episodic log
   - If ToolCall → execute → append result → continue
   - If Text → print → break

3. Finalize:
   - Update job status in SQLite (if scheduled job)
   - Calculate next_run if recurring
```

## 6. Module Structure

```
crates/bubbaloop/src/agent/
├── mod.rs           # Agent orchestrator: adaptive heartbeat + REPL + job poller
├── soul.rs          # Soul struct + notify hot-reload (Arc<RwLock<Soul>>)
├── memory/
│   ├── mod.rs       # Memory facade (short-term + episodic + semantic)
│   ├── episodic.rs  # NDJSON append-only log
│   └── semantic.rs  # SQLite stub for future embeddings
├── provider/
│   ├── mod.rs       # ModelProvider trait definition
│   ├── claude.rs    # ClaudeProvider (reqwest, dual auth)
│   └── ollama.rs    # OllamaProvider (stub)
├── scheduler.rs     # Job poller (SQLite jobs table) + Tier 1 actions
├── heartbeat.rs     # Adaptive heartbeat: arousal + decay + state collection
├── dispatch.rs      # Internal MCP tool dispatch (carry forward, adapt)
└── prompt.rs        # System prompt builder (soul + state injection)
```

### New Dependencies

| Crate | Purpose | Size Impact |
|-------|---------|-------------|
| `notify` | File system watcher for Soul hot-reload | ~50KB |

All other deps (`rusqlite`, `reqwest`, `cron`, `chrono`, `uuid`, `serde`, `tokio`) already present.

### Removed Dependencies

- FTS5 virtual tables (no longer needed — episodic is NDJSON)
- `rusqlite` features can be slimmed (no `bundled-full`, just `bundled`)

## 7. Human-in-the-Loop Approval

Two modes per skill:

| Mode | Behavior | Use For |
|------|----------|---------|
| `auto` | Execute immediately, log | Health checks, restarting crashed nodes |
| `propose` | Save proposal, wait for approval | Ambiguous situations, destructive actions |

### Proposal Flow

1. Agent reasons → proposes action
2. Proposal saved to `proposals` table
3. Notification: terminal (REPL) or MCP (external agent)
4. User approves → execute; rejects → discard
5. Timeout → configurable (auto-execute or discard)

### MCP Tools for Proposals

| Tool | RBAC | Description |
|------|------|-------------|
| `list_proposals` | Viewer | List pending proposals |
| `approve_proposal` | Operator | Approve a proposal |
| `reject_proposal` | Operator | Reject a proposal |

## 8. Migration Strategy

This is a **full replacement**, not a refactor:

1. Delete existing `agent/` module (mod.rs, claude.rs, dispatch.rs, memory.rs, scheduler.rs, prompt.rs)
2. Build new module structure from scratch per this design
3. `dispatch.rs` logic carries forward (24 MCP tool definitions are stable)
4. `claude.rs` auth logic carries forward into `provider/claude.rs`
5. SQLite schema changes: drop conversations/sensor_events/schedules/FTS5 tables, add jobs/proposals
6. NDJSON episodic log is new (no migration from SQLite conversations)

### Breaking Changes

- `~/.bubbaloop/memory.db` schema changes (existing data not migrated)
- `~/.bubbaloop/soul/` directory is new (created on first run with defaults)
- `~/.bubbaloop/memory/` directory is new (NDJSON logs)

## 9. Open Questions (for team iteration)

1. **NDJSON retention policy:** How many days of logs to keep? Auto-prune after 30 days?
2. **Arousal cap:** Should arousal have a maximum value (e.g., 20.0) to prevent interval from staying at MIN forever during sustained events?
3. **Model routing:** Haiku for "nothing changed" beats, Sonnet for reasoning — implement now or later?
4. **Episodic → Semantic promotion:** Should high-value episodic entries (errors, decisions) be promoted to SQLite for queryability?
5. **Soul versioning:** Should capabilities.toml have a schema version to handle future field additions gracefully?

## References

- [OpenClaw Agent Loop](https://docs.openclaw.ai/concepts/agent-loop) — heartbeat scheduler pattern
- [ZeroClaw Architecture](https://zeroclaw.net/) — Rust trait-based autonomous agent
- [Pico Framework](https://github.com/nichochar/pico-agent) — file-as-identity, NDJSON memory
- Existing heartbeat design: `docs/plans/2026-03-02-heartbeat-agent-loop-design.md`
- Existing agent implementation design: `docs/plans/2026-02-28-agent-implementation-design.md`
