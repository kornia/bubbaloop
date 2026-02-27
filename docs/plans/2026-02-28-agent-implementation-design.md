# Agent Implementation Design (Phases 2-4)

**Date:** 2026-02-28
**Status:** Approved, ready for implementation
**Builds on:** `2026-02-27-hardware-ai-agent-design.md`

---

## Scope

Implement all three remaining phases in sequence:
- **Phase 2**: Agent Loop (Claude API + internal MCP tool dispatch)
- **Phase 3**: SQLite Memory (conversations, sensor events, schedules)
- **Phase 4**: Scheduling (Tier 1 offline cron + Tier 2 LLM-driven)

## Module Structure

```
crates/bubbaloop/src/
├── agent/
│   ├── mod.rs          # AgentConfig, run_agent(), system prompt builder
│   ├── claude.rs       # Claude API client (raw reqwest, tool_use)
│   ├── dispatch.rs     # Internal MCP tool dispatch (no HTTP)
│   ├── memory.rs       # SQLite: conversations, sensor_events, schedules
│   └── scheduler.rs    # Cron executor (Tier 1 + Tier 2)
├── cli/
│   └── agent.rs        # `bubbaloop agent` CLI command (terminal chat)
```

## Phase 2: Agent Loop

### claude.rs — Claude API Client

Raw reqwest to `https://api.anthropic.com/v1/messages`:
- API key from `ANTHROPIC_API_KEY` env var (never persisted)
- Model: `claude-sonnet-4-20250514` (configurable)
- Tool definitions: generated from the 23 MCP tool schemas
- Multi-turn loop: user → Claude → tool_use → tool results → Claude → response
- Streaming not required for v1 (simplifies implementation)

Key types:
```rust
pub struct ClaudeClient { client: reqwest::Client, api_key: String, model: String }
pub struct Message { role: String, content: Vec<ContentBlock> }
pub enum ContentBlock { Text { text: String }, ToolUse { id, name, input }, ToolResult { tool_use_id, content } }
```

### dispatch.rs — Internal MCP Tool Dispatch

Calls MCP tools without HTTP round-trip:
- Creates `BubbaLoopMcpServer<DaemonPlatform>` instance
- Maps Claude tool_use `name` + `input` → `CallToolRequestParams`
- Calls `server.call_tool(params)` directly
- Returns `CallToolResult` text content back to Claude as `ToolResult`

### agent/mod.rs — Orchestrator

```rust
pub async fn run_agent(config: AgentConfig, session: Session, node_manager: NodeManager) {
    // 1. Build system prompt (sensor inventory, schedules)
    // 2. Read user input (stdin readline)
    // 3. Send to Claude with tools
    // 4. If tool_use: dispatch internally, append results, loop
    // 5. If text: display response, log to memory
    // 6. Repeat
}
```

System prompt includes live data on every turn:
- Sensor inventory from `list_nodes()`
- Active schedules from SQLite (Phase 3)
- Recent events summary (Phase 3)

### cli/agent.rs — CLI Command

```rust
#[derive(FromArgs)]
#[argh(subcommand, name = "agent")]
pub struct AgentCommand {
    #[argh(option, short = 'm')]
    pub model: Option<String>,  // default: claude-sonnet-4-20250514
}
```

Wired into `bin/bubbaloop.rs` Command enum. Requires Zenoh session + NodeManager (like daemon/mcp commands).

## Phase 3: SQLite Memory

### memory.rs — Database Layer

`rusqlite` with bundled SQLite3. DB at `~/.bubbaloop/memory.db` (0600 permissions).

Schema (from approved design doc):
```sql
CREATE TABLE conversations (
    id TEXT PRIMARY KEY, timestamp TEXT NOT NULL,
    role TEXT NOT NULL, content TEXT NOT NULL, tool_calls TEXT
);
CREATE TABLE sensor_events (
    id TEXT PRIMARY KEY, timestamp TEXT NOT NULL,
    node_name TEXT NOT NULL, event_type TEXT NOT NULL, details TEXT
);
CREATE TABLE schedules (
    id TEXT PRIMARY KEY, name TEXT NOT NULL UNIQUE,
    cron TEXT NOT NULL, actions TEXT NOT NULL,
    tier INTEGER NOT NULL DEFAULT 1,
    last_run TEXT, next_run TEXT, created_by TEXT NOT NULL
);
```

Key interface:
```rust
pub struct Memory { conn: rusqlite::Connection }
impl Memory {
    pub fn open(path: &Path) -> Result<Self>;       // creates tables if missing
    pub fn log_message(&self, role, content, tool_calls) -> Result<()>;
    pub fn log_event(&self, node_name, event_type, details) -> Result<()>;
    pub fn recent_events(&self, limit: usize) -> Result<Vec<SensorEvent>>;
    pub fn recent_conversations(&self, limit: usize) -> Result<Vec<ConversationRow>>;
    pub fn upsert_schedule(&self, schedule: &Schedule) -> Result<()>;
    pub fn list_schedules(&self) -> Result<Vec<Schedule>>;
    pub fn update_last_run(&self, id: &str, last_run: &str, next_run: &str) -> Result<()>;
}
```

### Event Hook

In `NodeManager::emit_event()`, add an optional `Memory` subscriber:
```rust
// Existing: broadcast to watchers
let _ = self.event_tx.send(event.clone());
// New: persist to SQLite (non-blocking, log-and-continue on error)
if let Some(memory) = &self.memory {
    if let Err(e) = memory.log_event(&event.node_name, &event.event_type, &details) {
        log::warn!("Failed to write event to memory: {}", e);
    }
}
```

## Phase 4: Scheduling

### scheduler.rs — Cron Executor

Background tokio task that checks `schedules` table every 60 seconds.

```rust
pub struct Scheduler { memory: Arc<Memory>, platform: Arc<dyn PlatformOperations> }
impl Scheduler {
    pub async fn run(&self, mut shutdown: watch::Receiver<()>);
    fn execute_tier1(&self, schedule: &Schedule) -> Result<()>;    // no LLM
    async fn execute_tier2(&self, schedule: &Schedule, claude: &ClaudeClient) -> Result<()>;
}
```

**Tier 1 built-in actions** (closed set, no arbitrary code):
- `check_all_health` — query health for all running nodes
- `restart` — restart unhealthy nodes
- `capture_frame` — send capture command to camera node
- `start_node` / `stop_node` — node lifecycle
- `send_command` — generic command dispatch
- `log_event` / `store_event` — write to sensor_events
- `notify` — print to terminal / notification queue

**Tier 2**: Invokes Claude API with schedule context. Rate-limited (configurable max calls/day, default 100).

### Skill Integration

`bubbaloop up` already loads skills with `schedule` and `actions` fields. At daemon startup, skills with schedules register as Tier 1 jobs in the `schedules` table (created_by = "yaml").

## New Dependencies

| Crate | Purpose | Binary Impact |
|-------|---------|---------------|
| `rusqlite` (bundled) | Embedded SQLite | +1-2 MB |
| `cron` | Cron expression parsing | ~50 KB |

`reqwest` already in dep tree via zenoh. Target binary: ~12-13 MB.

## Implementation Order

1. Phase 2a: `claude.rs` + `dispatch.rs` — get a working chat loop
2. Phase 2b: `cli/agent.rs` + `agent/mod.rs` — wire into binary, test end-to-end
3. Phase 3a: `memory.rs` — SQLite schema + CRUD operations
4. Phase 3b: Hook memory into agent loop + daemon events
5. Phase 4a: `scheduler.rs` — Tier 1 cron executor with built-in actions
6. Phase 4b: Tier 2 LLM scheduling + skill YAML integration
7. Update ROADMAP.md checkboxes
