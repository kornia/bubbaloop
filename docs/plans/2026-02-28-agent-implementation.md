# Agent Implementation Plan (Phases 2-4)

> **For Claude:** REQUIRED SUB-SKILL: Use superpowers:executing-plans to implement this plan task-by-task.

**Goal:** Add `bubbaloop agent` — a terminal chat interface that connects Claude to the 24 existing MCP tools, backed by SQLite memory and an offline-capable scheduler.

**Architecture:** Agent layer sits ON TOP of existing MCP server. `dispatch.rs` creates a `BubbaLoopMcpServer<DaemonPlatform>` and calls `call_tool()` directly (no HTTP). Claude API via raw `reqwest`. SQLite at `~/.bubbaloop/memory.db`. Scheduler checks cron expressions every 60s in a background tokio task.

**Tech Stack:** `reqwest` (Claude API), `rusqlite` (bundled SQLite3), `cron` crate (expression parsing), existing `rmcp`/`argh`/`tokio`/`serde_json`.

---

## Task 1: Add Dependencies

**Files:**
- Modify: `/home/nvidia/bubbaloop/Cargo.toml` (workspace deps)
- Modify: `/home/nvidia/bubbaloop/crates/bubbaloop/Cargo.toml` (crate deps)

**Step 1: Add workspace dependencies**

In `/home/nvidia/bubbaloop/Cargo.toml`, add after the existing `anyhow = "1"` line:

```toml
# HTTP client (Claude API)
reqwest = { version = "0.12", default-features = false, features = ["rustls-tls", "json"] }

# Embedded database (memory layer)
rusqlite = { version = "0.33", features = ["bundled"] }

# Cron expression parsing (scheduler)
cron = "0.15"
```

**Step 2: Add crate dependencies**

In `/home/nvidia/bubbaloop/crates/bubbaloop/Cargo.toml`, add in `[dependencies]`:

```toml
reqwest.workspace = true
rusqlite.workspace = true
cron.workspace = true
```

**Step 3: Verify compilation**

Run: `pixi run check`
Expected: Compiles with new deps (may take a while first time to download/compile rusqlite bundled)

**Step 4: Commit**

```bash
git add Cargo.toml crates/bubbaloop/Cargo.toml Cargo.lock
git commit -m "chore: add reqwest, rusqlite, cron dependencies for agent"
```

---

## Task 2: SQLite Memory Module

**Files:**
- Create: `crates/bubbaloop/src/agent/mod.rs`
- Create: `crates/bubbaloop/src/agent/memory.rs`
- Modify: `crates/bubbaloop/src/lib.rs` (add `pub mod agent;`)

**Step 1: Create agent module root**

Create `crates/bubbaloop/src/agent/mod.rs`:

```rust
//! Agent layer — Claude API chat, SQLite memory, scheduling.

pub mod memory;
```

**Step 2: Register agent module in lib.rs**

In `crates/bubbaloop/src/lib.rs`, add after `pub mod skills;`:

```rust
/// Agent layer: Claude API chat, SQLite memory, scheduling
pub mod agent;
```

**Step 3: Write memory tests first**

Create `crates/bubbaloop/src/agent/memory.rs` with the test module:

```rust
//! SQLite memory layer — conversations, sensor events, schedules.
//!
//! Stores agent conversation history, sensor lifecycle events, and
//! scheduled jobs in a single SQLite database at `~/.bubbaloop/memory.db`.

use rusqlite::{params, Connection};
use std::path::Path;

#[derive(Debug, thiserror::Error)]
pub enum MemoryError {
    #[error("SQLite error: {0}")]
    Sqlite(#[from] rusqlite::Error),
    #[error("IO error: {0}")]
    Io(#[from] std::io::Error),
}

pub type Result<T> = std::result::Result<T, MemoryError>;

/// A conversation message row.
#[derive(Debug, Clone)]
pub struct ConversationRow {
    pub id: String,
    pub timestamp: String,
    pub role: String,
    pub content: String,
    pub tool_calls: Option<String>,
}

/// A sensor event row.
#[derive(Debug, Clone)]
pub struct SensorEvent {
    pub id: String,
    pub timestamp: String,
    pub node_name: String,
    pub event_type: String,
    pub details: Option<String>,
}

/// A scheduled job row.
#[derive(Debug, Clone)]
pub struct Schedule {
    pub id: String,
    pub name: String,
    pub cron: String,
    pub actions: String,
    pub tier: i32,
    pub last_run: Option<String>,
    pub next_run: Option<String>,
    pub created_by: String,
}

/// SQLite-backed memory store.
pub struct Memory {
    conn: Connection,
}

impl Memory {
    /// Open (or create) the memory database at the given path.
    ///
    /// Creates all tables if they don't exist. Sets file permissions to 0600.
    pub fn open(path: &Path) -> Result<Self> {
        // Ensure parent directory exists
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent)?;
        }

        let conn = Connection::open(path)?;

        // WAL mode for better concurrent read performance
        conn.pragma_update(None, "journal_mode", "WAL")?;

        conn.execute_batch(
            "CREATE TABLE IF NOT EXISTS conversations (
                id          TEXT PRIMARY KEY,
                timestamp   TEXT NOT NULL,
                role        TEXT NOT NULL,
                content     TEXT NOT NULL,
                tool_calls  TEXT
            );
            CREATE INDEX IF NOT EXISTS idx_conv_ts ON conversations(timestamp);

            CREATE TABLE IF NOT EXISTS sensor_events (
                id          TEXT PRIMARY KEY,
                timestamp   TEXT NOT NULL,
                node_name   TEXT NOT NULL,
                event_type  TEXT NOT NULL,
                details     TEXT
            );
            CREATE INDEX IF NOT EXISTS idx_events_node_ts ON sensor_events(node_name, timestamp);

            CREATE TABLE IF NOT EXISTS schedules (
                id          TEXT PRIMARY KEY,
                name        TEXT NOT NULL UNIQUE,
                cron        TEXT NOT NULL,
                actions     TEXT NOT NULL,
                tier        INTEGER NOT NULL DEFAULT 1,
                last_run    TEXT,
                next_run    TEXT,
                created_by  TEXT NOT NULL
            );",
        )?;

        // Set file permissions to 0600 (owner read/write only)
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            if path.exists() {
                let perms = std::fs::Permissions::from_mode(0o600);
                std::fs::set_permissions(path, perms)?;
            }
        }

        Ok(Self { conn })
    }

    /// Log a conversation message (user, assistant, or tool).
    pub fn log_message(
        &self,
        role: &str,
        content: &str,
        tool_calls: Option<&str>,
    ) -> Result<()> {
        let id = uuid::Uuid::new_v4().to_string();
        let timestamp = now_iso8601();
        self.conn.execute(
            "INSERT INTO conversations (id, timestamp, role, content, tool_calls) VALUES (?1, ?2, ?3, ?4, ?5)",
            params![id, timestamp, role, content, tool_calls],
        )?;
        Ok(())
    }

    /// Log a sensor event (health change, started, stopped, etc.).
    pub fn log_event(
        &self,
        node_name: &str,
        event_type: &str,
        details: Option<&str>,
    ) -> Result<()> {
        let id = uuid::Uuid::new_v4().to_string();
        let timestamp = now_iso8601();
        self.conn.execute(
            "INSERT INTO sensor_events (id, timestamp, node_name, event_type, details) VALUES (?1, ?2, ?3, ?4, ?5)",
            params![id, timestamp, node_name, event_type, details],
        )?;
        Ok(())
    }

    /// Get recent conversation messages (most recent first).
    pub fn recent_conversations(&self, limit: usize) -> Result<Vec<ConversationRow>> {
        let mut stmt = self.conn.prepare(
            "SELECT id, timestamp, role, content, tool_calls FROM conversations ORDER BY timestamp DESC LIMIT ?1",
        )?;
        let rows = stmt.query_map(params![limit as i64], |row| {
            Ok(ConversationRow {
                id: row.get(0)?,
                timestamp: row.get(1)?,
                role: row.get(2)?,
                content: row.get(3)?,
                tool_calls: row.get(4)?,
            })
        })?;
        rows.collect::<std::result::Result<Vec<_>, _>>().map_err(MemoryError::from)
    }

    /// Get recent sensor events (most recent first).
    pub fn recent_events(&self, limit: usize) -> Result<Vec<SensorEvent>> {
        let mut stmt = self.conn.prepare(
            "SELECT id, timestamp, node_name, event_type, details FROM sensor_events ORDER BY timestamp DESC LIMIT ?1",
        )?;
        let rows = stmt.query_map(params![limit as i64], |row| {
            Ok(SensorEvent {
                id: row.get(0)?,
                timestamp: row.get(1)?,
                node_name: row.get(2)?,
                event_type: row.get(3)?,
                details: row.get(4)?,
            })
        })?;
        rows.collect::<std::result::Result<Vec<_>, _>>().map_err(MemoryError::from)
    }

    /// Get events for a specific node.
    pub fn events_for_node(
        &self,
        node_name: &str,
        limit: usize,
    ) -> Result<Vec<SensorEvent>> {
        let mut stmt = self.conn.prepare(
            "SELECT id, timestamp, node_name, event_type, details FROM sensor_events WHERE node_name = ?1 ORDER BY timestamp DESC LIMIT ?2",
        )?;
        let rows = stmt.query_map(params![node_name, limit as i64], |row| {
            Ok(SensorEvent {
                id: row.get(0)?,
                timestamp: row.get(1)?,
                node_name: row.get(2)?,
                event_type: row.get(3)?,
                details: row.get(4)?,
            })
        })?;
        rows.collect::<std::result::Result<Vec<_>, _>>().map_err(MemoryError::from)
    }

    /// Insert or update a schedule.
    pub fn upsert_schedule(&self, schedule: &Schedule) -> Result<()> {
        self.conn.execute(
            "INSERT INTO schedules (id, name, cron, actions, tier, last_run, next_run, created_by)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8)
             ON CONFLICT(name) DO UPDATE SET
                cron = excluded.cron,
                actions = excluded.actions,
                tier = excluded.tier,
                next_run = excluded.next_run",
            params![
                schedule.id,
                schedule.name,
                schedule.cron,
                schedule.actions,
                schedule.tier,
                schedule.last_run,
                schedule.next_run,
                schedule.created_by,
            ],
        )?;
        Ok(())
    }

    /// List all schedules.
    pub fn list_schedules(&self) -> Result<Vec<Schedule>> {
        let mut stmt = self.conn.prepare(
            "SELECT id, name, cron, actions, tier, last_run, next_run, created_by FROM schedules ORDER BY name",
        )?;
        let rows = stmt.query_map([], |row| {
            Ok(Schedule {
                id: row.get(0)?,
                name: row.get(1)?,
                cron: row.get(2)?,
                actions: row.get(3)?,
                tier: row.get(4)?,
                last_run: row.get(5)?,
                next_run: row.get(6)?,
                created_by: row.get(7)?,
            })
        })?;
        rows.collect::<std::result::Result<Vec<_>, _>>().map_err(MemoryError::from)
    }

    /// Update last_run and next_run for a schedule.
    pub fn update_schedule_run(
        &self,
        name: &str,
        last_run: &str,
        next_run: &str,
    ) -> Result<()> {
        self.conn.execute(
            "UPDATE schedules SET last_run = ?1, next_run = ?2 WHERE name = ?3",
            params![last_run, next_run, name],
        )?;
        Ok(())
    }

    /// Delete a schedule by name.
    pub fn delete_schedule(&self, name: &str) -> Result<()> {
        self.conn
            .execute("DELETE FROM schedules WHERE name = ?1", params![name])?;
        Ok(())
    }
}

/// ISO 8601 timestamp for the current time.
fn now_iso8601() -> String {
    // Use std::time to avoid chrono dependency
    let duration = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default();
    let secs = duration.as_secs();
    // Simple UTC format: seconds since epoch as ISO-ish string
    // For proper ISO 8601 we'd need chrono, but this is sufficient for ordering
    format!("{}", secs)
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::tempdir;

    fn test_memory() -> Memory {
        let dir = tempdir().unwrap();
        Memory::open(&dir.path().join("test.db")).unwrap()
    }

    #[test]
    fn open_creates_tables() {
        let dir = tempdir().unwrap();
        let db_path = dir.path().join("test.db");
        let mem = Memory::open(&db_path).unwrap();
        // Verify tables exist by running a query on each
        mem.recent_conversations(1).unwrap();
        mem.recent_events(1).unwrap();
        mem.list_schedules().unwrap();
    }

    #[test]
    fn open_twice_is_idempotent() {
        let dir = tempdir().unwrap();
        let db_path = dir.path().join("test.db");
        Memory::open(&db_path).unwrap();
        Memory::open(&db_path).unwrap(); // should not error
    }

    #[test]
    fn log_and_retrieve_messages() {
        let mem = test_memory();
        mem.log_message("user", "hello", None).unwrap();
        mem.log_message("assistant", "hi there", None).unwrap();
        mem.log_message("assistant", "tool result", Some(r#"[{"name":"list_nodes"}]"#))
            .unwrap();

        let msgs = mem.recent_conversations(10).unwrap();
        assert_eq!(msgs.len(), 3);
        // Most recent first
        assert_eq!(msgs[0].role, "assistant");
        assert!(msgs[0].tool_calls.is_some());
        assert_eq!(msgs[2].role, "user");
    }

    #[test]
    fn log_and_retrieve_events() {
        let mem = test_memory();
        mem.log_event("rtsp-camera", "started", None).unwrap();
        mem.log_event("rtsp-camera", "health_ok", Some(r#"{"uptime":120}"#))
            .unwrap();
        mem.log_event("system-telemetry", "started", None).unwrap();

        let all = mem.recent_events(10).unwrap();
        assert_eq!(all.len(), 3);

        let cam = mem.events_for_node("rtsp-camera", 10).unwrap();
        assert_eq!(cam.len(), 2);
    }

    #[test]
    fn upsert_and_list_schedules() {
        let mem = test_memory();
        let sched = Schedule {
            id: "s1".into(),
            name: "health-patrol".into(),
            cron: "*/15 * * * *".into(),
            actions: r#"["check_all_health"]"#.into(),
            tier: 1,
            last_run: None,
            next_run: Some("1709136000".into()),
            created_by: "yaml".into(),
        };
        mem.upsert_schedule(&sched).unwrap();

        let list = mem.list_schedules().unwrap();
        assert_eq!(list.len(), 1);
        assert_eq!(list[0].name, "health-patrol");
        assert_eq!(list[0].tier, 1);
    }

    #[test]
    fn upsert_schedule_updates_existing() {
        let mem = test_memory();
        let sched1 = Schedule {
            id: "s1".into(),
            name: "patrol".into(),
            cron: "*/15 * * * *".into(),
            actions: r#"["check_all_health"]"#.into(),
            tier: 1,
            last_run: None,
            next_run: None,
            created_by: "yaml".into(),
        };
        mem.upsert_schedule(&sched1).unwrap();

        // Update cron
        let sched2 = Schedule {
            id: "s2".into(),
            name: "patrol".into(),
            cron: "*/5 * * * *".into(),
            actions: r#"["check_all_health","restart"]"#.into(),
            tier: 1,
            last_run: None,
            next_run: None,
            created_by: "yaml".into(),
        };
        mem.upsert_schedule(&sched2).unwrap();

        let list = mem.list_schedules().unwrap();
        assert_eq!(list.len(), 1);
        assert_eq!(list[0].cron, "*/5 * * * *");
    }

    #[test]
    fn update_schedule_run() {
        let mem = test_memory();
        let sched = Schedule {
            id: "s1".into(),
            name: "patrol".into(),
            cron: "*/15 * * * *".into(),
            actions: "[]".into(),
            tier: 1,
            last_run: None,
            next_run: None,
            created_by: "yaml".into(),
        };
        mem.upsert_schedule(&sched).unwrap();
        mem.update_schedule_run("patrol", "1709136000", "1709136900")
            .unwrap();

        let list = mem.list_schedules().unwrap();
        assert_eq!(list[0].last_run.as_deref(), Some("1709136000"));
        assert_eq!(list[0].next_run.as_deref(), Some("1709136900"));
    }

    #[test]
    fn delete_schedule() {
        let mem = test_memory();
        let sched = Schedule {
            id: "s1".into(),
            name: "patrol".into(),
            cron: "*/15 * * * *".into(),
            actions: "[]".into(),
            tier: 1,
            last_run: None,
            next_run: None,
            created_by: "yaml".into(),
        };
        mem.upsert_schedule(&sched).unwrap();
        mem.delete_schedule("patrol").unwrap();

        let list = mem.list_schedules().unwrap();
        assert!(list.is_empty());
    }

    #[test]
    fn conversation_limit_works() {
        let mem = test_memory();
        for i in 0..10 {
            mem.log_message("user", &format!("msg {}", i), None)
                .unwrap();
        }
        let msgs = mem.recent_conversations(3).unwrap();
        assert_eq!(msgs.len(), 3);
    }

    #[cfg(unix)]
    #[test]
    fn file_permissions_are_0600() {
        use std::os::unix::fs::PermissionsExt;
        let dir = tempdir().unwrap();
        let db_path = dir.path().join("secure.db");
        Memory::open(&db_path).unwrap();
        let perms = std::fs::metadata(&db_path).unwrap().permissions();
        assert_eq!(perms.mode() & 0o777, 0o600);
    }
}
```

**Step 4: Run tests**

Run: `pixi run test`
Expected: All existing tests pass + new memory tests pass

**Step 5: Commit**

```bash
git add crates/bubbaloop/src/agent/ crates/bubbaloop/src/lib.rs
git commit -m "feat: add SQLite memory module with conversations, events, schedules"
```

---

## Task 3: Claude API Client

**Files:**
- Create: `crates/bubbaloop/src/agent/claude.rs`
- Modify: `crates/bubbaloop/src/agent/mod.rs`

**Step 1: Write Claude client tests**

Create `crates/bubbaloop/src/agent/claude.rs`:

```rust
//! Claude API client — raw reqwest to the Messages API with tool_use support.
//!
//! Does NOT use an SDK. Sends JSON to `https://api.anthropic.com/v1/messages`
//! and parses the response, including tool_use content blocks.

use serde::{Deserialize, Serialize};

const API_URL: &str = "https://api.anthropic.com/v1/messages";
const API_VERSION: &str = "2023-06-01";
const DEFAULT_MODEL: &str = "claude-sonnet-4-20250514";
const MAX_TOKENS: u32 = 4096;

#[derive(Debug, thiserror::Error)]
pub enum ClaudeError {
    #[error("HTTP error: {0}")]
    Http(#[from] reqwest::Error),
    #[error("API error ({status}): {message}")]
    Api { status: u16, message: String },
    #[error("Missing ANTHROPIC_API_KEY environment variable")]
    MissingApiKey,
    #[error("Unexpected response format: {0}")]
    Format(String),
}

pub type Result<T> = std::result::Result<T, ClaudeError>;

/// A content block in a Claude message.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type")]
pub enum ContentBlock {
    #[serde(rename = "text")]
    Text { text: String },
    #[serde(rename = "tool_use")]
    ToolUse {
        id: String,
        name: String,
        input: serde_json::Value,
    },
    #[serde(rename = "tool_result")]
    ToolResult {
        tool_use_id: String,
        content: String,
        #[serde(skip_serializing_if = "Option::is_none")]
        is_error: Option<bool>,
    },
}

/// A message in the conversation.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Message {
    pub role: String,
    pub content: Vec<ContentBlock>,
}

impl Message {
    pub fn user(text: &str) -> Self {
        Self {
            role: "user".into(),
            content: vec![ContentBlock::Text {
                text: text.to_string(),
            }],
        }
    }

    pub fn tool_results(results: Vec<ContentBlock>) -> Self {
        Self {
            role: "user".into(),
            content: results,
        }
    }

    /// Extract all text content from the message.
    pub fn text(&self) -> String {
        self.content
            .iter()
            .filter_map(|b| match b {
                ContentBlock::Text { text } => Some(text.as_str()),
                _ => None,
            })
            .collect::<Vec<_>>()
            .join("")
    }

    /// Extract all tool_use blocks from the message.
    pub fn tool_uses(&self) -> Vec<(&str, &str, &serde_json::Value)> {
        self.content
            .iter()
            .filter_map(|b| match b {
                ContentBlock::ToolUse { id, name, input } => {
                    Some((id.as_str(), name.as_str(), input))
                }
                _ => None,
            })
            .collect()
    }
}

/// A tool definition for the Claude API.
#[derive(Debug, Clone, Serialize)]
pub struct ToolDefinition {
    pub name: String,
    pub description: String,
    pub input_schema: serde_json::Value,
}

/// Claude API request body.
#[derive(Debug, Serialize)]
struct ApiRequest {
    model: String,
    max_tokens: u32,
    system: String,
    messages: Vec<Message>,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    tools: Vec<ToolDefinition>,
}

/// Claude API response body.
#[derive(Debug, Deserialize)]
pub struct ApiResponse {
    pub id: String,
    pub content: Vec<ContentBlock>,
    pub stop_reason: Option<String>,
    pub usage: Usage,
}

#[derive(Debug, Deserialize)]
pub struct Usage {
    pub input_tokens: u32,
    pub output_tokens: u32,
}

/// Claude API client.
pub struct ClaudeClient {
    client: reqwest::Client,
    api_key: String,
    model: String,
}

impl ClaudeClient {
    /// Create a new client. Reads `ANTHROPIC_API_KEY` from environment.
    pub fn from_env(model: Option<&str>) -> Result<Self> {
        let api_key =
            std::env::var("ANTHROPIC_API_KEY").map_err(|_| ClaudeError::MissingApiKey)?;
        Ok(Self {
            client: reqwest::Client::new(),
            api_key,
            model: model.unwrap_or(DEFAULT_MODEL).to_string(),
        })
    }

    /// Send a messages request to Claude.
    pub async fn send(
        &self,
        system: &str,
        messages: &[Message],
        tools: &[ToolDefinition],
    ) -> Result<ApiResponse> {
        let body = ApiRequest {
            model: self.model.clone(),
            max_tokens: MAX_TOKENS,
            system: system.to_string(),
            messages: messages.to_vec(),
            tools: tools.to_vec(),
        };

        let resp = self
            .client
            .post(API_URL)
            .header("x-api-key", &self.api_key)
            .header("anthropic-version", API_VERSION)
            .header("content-type", "application/json")
            .json(&body)
            .send()
            .await?;

        let status = resp.status().as_u16();
        if status != 200 {
            let text = resp.text().await.unwrap_or_default();
            return Err(ClaudeError::Api {
                status,
                message: text,
            });
        }

        let response: ApiResponse = resp.json().await?;
        Ok(response)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn message_user_constructor() {
        let msg = Message::user("hello");
        assert_eq!(msg.role, "user");
        assert_eq!(msg.text(), "hello");
    }

    #[test]
    fn message_text_concatenation() {
        let msg = Message {
            role: "assistant".into(),
            content: vec![
                ContentBlock::Text {
                    text: "Hello ".into(),
                },
                ContentBlock::Text {
                    text: "world".into(),
                },
            ],
        };
        assert_eq!(msg.text(), "Hello world");
    }

    #[test]
    fn message_tool_uses_extraction() {
        let msg = Message {
            role: "assistant".into(),
            content: vec![
                ContentBlock::Text {
                    text: "Let me check".into(),
                },
                ContentBlock::ToolUse {
                    id: "tu_1".into(),
                    name: "list_nodes".into(),
                    input: serde_json::json!({}),
                },
                ContentBlock::ToolUse {
                    id: "tu_2".into(),
                    name: "start_node".into(),
                    input: serde_json::json!({"node_name": "cam1"}),
                },
            ],
        };
        let uses = msg.tool_uses();
        assert_eq!(uses.len(), 2);
        assert_eq!(uses[0].1, "list_nodes");
        assert_eq!(uses[1].1, "start_node");
    }

    #[test]
    fn tool_result_message() {
        let msg = Message::tool_results(vec![ContentBlock::ToolResult {
            tool_use_id: "tu_1".into(),
            content: "2 nodes running".into(),
            is_error: None,
        }]);
        assert_eq!(msg.role, "user");
        assert_eq!(msg.content.len(), 1);
    }

    #[test]
    fn content_block_serde_text() {
        let block = ContentBlock::Text {
            text: "hello".into(),
        };
        let json = serde_json::to_string(&block).unwrap();
        assert!(json.contains(r#""type":"text""#));
        let parsed: ContentBlock = serde_json::from_str(&json).unwrap();
        match parsed {
            ContentBlock::Text { text } => assert_eq!(text, "hello"),
            _ => panic!("expected Text"),
        }
    }

    #[test]
    fn content_block_serde_tool_use() {
        let json = r#"{"type":"tool_use","id":"tu_1","name":"list_nodes","input":{}}"#;
        let block: ContentBlock = serde_json::from_str(json).unwrap();
        match block {
            ContentBlock::ToolUse { id, name, input } => {
                assert_eq!(id, "tu_1");
                assert_eq!(name, "list_nodes");
                assert!(input.is_object());
            }
            _ => panic!("expected ToolUse"),
        }
    }

    #[test]
    fn content_block_serde_tool_result() {
        let block = ContentBlock::ToolResult {
            tool_use_id: "tu_1".into(),
            content: "result".into(),
            is_error: Some(true),
        };
        let json = serde_json::to_string(&block).unwrap();
        assert!(json.contains("tool_result"));
        assert!(json.contains("is_error"));
    }

    #[test]
    fn api_response_deserialization() {
        let json = r#"{
            "id": "msg_123",
            "content": [{"type": "text", "text": "Hello!"}],
            "stop_reason": "end_turn",
            "usage": {"input_tokens": 10, "output_tokens": 5}
        }"#;
        let resp: ApiResponse = serde_json::from_str(json).unwrap();
        assert_eq!(resp.id, "msg_123");
        assert_eq!(resp.content.len(), 1);
        assert_eq!(resp.usage.input_tokens, 10);
    }

    #[test]
    fn api_response_with_tool_use() {
        let json = r#"{
            "id": "msg_456",
            "content": [
                {"type": "text", "text": "Let me check."},
                {"type": "tool_use", "id": "tu_1", "name": "list_nodes", "input": {}}
            ],
            "stop_reason": "tool_use",
            "usage": {"input_tokens": 50, "output_tokens": 30}
        }"#;
        let resp: ApiResponse = serde_json::from_str(json).unwrap();
        assert_eq!(resp.stop_reason.as_deref(), Some("tool_use"));
        assert_eq!(resp.content.len(), 2);
    }

    #[test]
    fn client_from_env_missing_key() {
        // Temporarily remove the key if set
        let original = std::env::var("ANTHROPIC_API_KEY").ok();
        std::env::remove_var("ANTHROPIC_API_KEY");
        let result = ClaudeClient::from_env(None);
        assert!(matches!(result, Err(ClaudeError::MissingApiKey)));
        // Restore
        if let Some(key) = original {
            std::env::set_var("ANTHROPIC_API_KEY", key);
        }
    }

    #[test]
    fn tool_definition_serialization() {
        let tool = ToolDefinition {
            name: "list_nodes".into(),
            description: "List all registered nodes".into(),
            input_schema: serde_json::json!({
                "type": "object",
                "properties": {},
                "required": []
            }),
        };
        let json = serde_json::to_string(&tool).unwrap();
        assert!(json.contains("list_nodes"));
        assert!(json.contains("input_schema"));
    }
}
```

**Step 2: Add module to agent/mod.rs**

Update `crates/bubbaloop/src/agent/mod.rs`:

```rust
//! Agent layer — Claude API chat, SQLite memory, scheduling.

pub mod claude;
pub mod memory;
```

**Step 3: Run tests**

Run: `pixi run test`
Expected: All tests pass

**Step 4: Commit**

```bash
git add crates/bubbaloop/src/agent/claude.rs crates/bubbaloop/src/agent/mod.rs
git commit -m "feat: add Claude API client with tool_use support"
```

---

## Task 4: Internal MCP Tool Dispatch

**Files:**
- Create: `crates/bubbaloop/src/agent/dispatch.rs`
- Modify: `crates/bubbaloop/src/agent/mod.rs`

**Step 1: Write dispatch module**

Create `crates/bubbaloop/src/agent/dispatch.rs`:

```rust
//! Internal MCP tool dispatch — calls MCP tools without HTTP round-trip.
//!
//! Creates a `BubbaLoopMcpServer<DaemonPlatform>` and invokes `call_tool()`
//! directly. Generates Claude-compatible tool definitions from the MCP tool
//! router's schema metadata.

use std::sync::Arc;

use crate::agent::claude::{ContentBlock, ToolDefinition};
use crate::mcp::platform::{DaemonPlatform, NodeInfo, PlatformOperations};
use crate::mcp::BubbaLoopMcpServer;

#[derive(Debug, thiserror::Error)]
pub enum DispatchError {
    #[error("Tool call failed: {0}")]
    ToolError(String),
    #[error("MCP error: {0}")]
    Mcp(String),
}

pub type Result<T> = std::result::Result<T, DispatchError>;

/// Internal tool dispatcher — calls MCP tools in-process.
pub struct Dispatcher {
    server: BubbaLoopMcpServer<DaemonPlatform>,
}

impl Dispatcher {
    /// Create a dispatcher backed by a real daemon platform.
    pub fn new(
        node_manager: Arc<crate::daemon::node_manager::NodeManager>,
        session: Arc<zenoh::Session>,
    ) -> Self {
        let scope =
            std::env::var("BUBBALOOP_SCOPE").unwrap_or_else(|_| "local".to_string());
        let machine_id = crate::daemon::util::get_machine_id();

        let platform = Arc::new(DaemonPlatform {
            node_manager,
            session,
            scope: scope.clone(),
            machine_id: machine_id.clone(),
        });

        let server = BubbaLoopMcpServer::new(platform, None, scope, machine_id);
        Self { server }
    }

    /// Generate Claude-compatible tool definitions from the MCP tool router.
    ///
    /// This reads the tool schemas registered via `#[tool(...)]` macros and
    /// converts them to the format Claude expects in the `tools` array.
    pub fn tool_definitions(&self) -> Vec<ToolDefinition> {
        let tools = self.server.list_tool_definitions();
        tools
            .into_iter()
            .map(|t| ToolDefinition {
                name: t.name.to_string(),
                description: t.description.unwrap_or_default().to_string(),
                input_schema: t.input_schema,
            })
            .collect()
    }

    /// Dispatch a single tool call and return the result as a ContentBlock.
    ///
    /// Takes the tool name and JSON input from Claude's `tool_use` block,
    /// calls the MCP tool internally, and returns a `ToolResult` block.
    pub async fn call_tool(
        &self,
        tool_use_id: &str,
        name: &str,
        input: &serde_json::Value,
    ) -> ContentBlock {
        let params = rmcp::model::CallToolRequestParams {
            name: name.into(),
            arguments: Some(
                input
                    .as_object()
                    .cloned()
                    .unwrap_or_default()
                    .into_iter()
                    .collect(),
            ),
            meta: None,
        };

        // Create a minimal request context for the tool router
        let context = rmcp::service::RequestContext::default();

        match self.server.call_tool(params, context).await {
            Ok(result) => {
                let text = result
                    .content
                    .iter()
                    .filter_map(|c| match c {
                        rmcp::model::Content::Text(t) => Some(t.text.as_str()),
                        _ => None,
                    })
                    .collect::<Vec<_>>()
                    .join("\n");

                ContentBlock::ToolResult {
                    tool_use_id: tool_use_id.to_string(),
                    content: text,
                    is_error: if result.is_error.unwrap_or(false) {
                        Some(true)
                    } else {
                        None
                    },
                }
            }
            Err(e) => ContentBlock::ToolResult {
                tool_use_id: tool_use_id.to_string(),
                content: format!("Error: {}", e.message),
                is_error: Some(true),
            },
        }
    }

    /// Get current node inventory for system prompt.
    pub async fn get_node_inventory(&self) -> String {
        // Call list_nodes via platform directly for efficiency
        let input = serde_json::json!({});
        let result = self.call_tool("_inventory", "list_nodes", &input).await;
        match result {
            ContentBlock::ToolResult { content, .. } => content,
            _ => "No sensors found.".to_string(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn tool_definition_has_required_fields() {
        let def = ToolDefinition {
            name: "list_nodes".into(),
            description: "List all nodes".into(),
            input_schema: serde_json::json!({"type": "object", "properties": {}}),
        };
        let json = serde_json::to_value(&def).unwrap();
        assert!(json.get("name").is_some());
        assert!(json.get("description").is_some());
        assert!(json.get("input_schema").is_some());
    }
}
```

**Note:** The `Dispatcher` depends on `BubbaLoopMcpServer` having a public `list_tool_definitions()` method and `ServerHandler::call_tool()`. We need to expose these. Add a helper method to the MCP server.

**Step 2: Expose tool listing on BubbaLoopMcpServer**

In `crates/bubbaloop/src/mcp/mod.rs`, add this method to the `impl<P: PlatformOperations> BubbaLoopMcpServer<P>` block (near `fn new()`):

```rust
    /// Get all tool definitions for external use (e.g., Claude API tool schemas).
    pub fn list_tool_definitions(&self) -> Vec<rmcp::model::Tool> {
        self.tool_router.list_all()
    }
```

**Step 3: Add module to agent/mod.rs**

```rust
//! Agent layer — Claude API chat, SQLite memory, scheduling.

pub mod claude;
pub mod dispatch;
pub mod memory;
```

**Step 4: Run check (may need adjustments for rmcp types)**

Run: `pixi run check`

The `rmcp::service::RequestContext::default()` and `ServerHandler::call_tool()` signatures may need adjustment based on the actual rmcp API. If `RequestContext` doesn't implement `Default`, create it differently — check `rmcp` docs. The key pattern is: we're calling the same `call_tool` that the HTTP/stdio transports call.

If `call_tool` requires a real context that can't be easily mocked, an alternative approach is to call the platform methods directly (bypassing the MCP server layer):

```rust
// Fallback: call platform directly instead of through MCP server
pub async fn call_tool_direct(
    &self,
    name: &str,
    input: &serde_json::Value,
) -> Result<String> {
    match name {
        "list_nodes" => {
            let nodes = self.platform.list_nodes().await
                .map_err(|e| DispatchError::ToolError(e.to_string()))?;
            Ok(serde_json::to_string_pretty(&nodes)
                .unwrap_or_else(|_| "[]".to_string()))
        }
        "start_node" => {
            let node_name = input["node_name"].as_str()
                .ok_or_else(|| DispatchError::ToolError("missing node_name".into()))?;
            self.platform.execute_command(node_name, NodeCommand::Start).await
                .map_err(|e| DispatchError::ToolError(e.to_string()))
        }
        // ... map all 24 tools
        _ => Err(DispatchError::ToolError(format!("unknown tool: {}", name))),
    }
}
```

Choose whichever approach compiles. The tool_router approach is cleaner but depends on rmcp internals.

**Step 5: Run tests**

Run: `pixi run test`
Expected: All tests pass

**Step 6: Commit**

```bash
git add crates/bubbaloop/src/agent/dispatch.rs crates/bubbaloop/src/agent/mod.rs crates/bubbaloop/src/mcp/mod.rs
git commit -m "feat: add internal MCP tool dispatch for agent"
```

---

## Task 5: Agent Orchestrator and CLI Command

**Files:**
- Create: `crates/bubbaloop/src/agent/prompt.rs`
- Modify: `crates/bubbaloop/src/agent/mod.rs`
- Create: `crates/bubbaloop/src/cli/agent.rs`
- Modify: `crates/bubbaloop/src/cli/mod.rs`
- Modify: `crates/bubbaloop/src/bin/bubbaloop.rs`

**Step 1: Create system prompt builder**

Create `crates/bubbaloop/src/agent/prompt.rs`:

```rust
//! System prompt builder — injects live sensor inventory and schedules.

use crate::agent::memory::{Memory, Schedule, SensorEvent};

/// Build the system prompt with live sensor data.
pub fn build_system_prompt(
    node_inventory: &str,
    schedules: &[Schedule],
    recent_events: &[SensorEvent],
) -> String {
    let mut prompt = String::from(
        "You are Bubbaloop, an AI agent that controls physical sensors and hardware. \
         You have MCP tools to discover, install, start, stop, and monitor sensor nodes.\n\n\
         Be concise. When a user asks to do something, use the appropriate tool. \
         When creating scheduled jobs, prefer Tier 1 (offline, no LLM) for simple \
         health checks and restarts. Use Tier 2 (LLM-powered) only for tasks that \
         need reasoning.\n",
    );

    // Sensor inventory
    prompt.push_str("\n## Current Sensors\n\n");
    if node_inventory.is_empty() || node_inventory == "[]" {
        prompt.push_str("No sensors installed yet. Use install_node to add sensors.\n");
    } else {
        prompt.push_str(node_inventory);
        prompt.push('\n');
    }

    // Schedules
    if !schedules.is_empty() {
        prompt.push_str("\n## Active Schedules\n\n");
        for s in schedules {
            prompt.push_str(&format!(
                "- {} (Tier {}, cron: {})\n",
                s.name, s.tier, s.cron
            ));
        }
    }

    // Recent events
    if !recent_events.is_empty() {
        prompt.push_str("\n## Recent Events\n\n");
        for e in recent_events.iter().take(10) {
            prompt.push_str(&format!(
                "- [{}] {} {}\n",
                e.timestamp, e.node_name, e.event_type
            ));
        }
    }

    prompt
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn prompt_with_empty_inventory() {
        let prompt = build_system_prompt("", &[], &[]);
        assert!(prompt.contains("No sensors installed"));
        assert!(prompt.contains("Bubbaloop"));
    }

    #[test]
    fn prompt_with_nodes() {
        let prompt = build_system_prompt(
            "rtsp-camera: Running, Healthy\nsystem-telemetry: Running, Healthy",
            &[],
            &[],
        );
        assert!(prompt.contains("rtsp-camera"));
        assert!(prompt.contains("system-telemetry"));
    }

    #[test]
    fn prompt_with_schedules() {
        let schedules = vec![Schedule {
            id: "s1".into(),
            name: "health-patrol".into(),
            cron: "*/15 * * * *".into(),
            actions: "[]".into(),
            tier: 1,
            last_run: None,
            next_run: None,
            created_by: "yaml".into(),
        }];
        let prompt = build_system_prompt("", &schedules, &[]);
        assert!(prompt.contains("health-patrol"));
        assert!(prompt.contains("Tier 1"));
    }

    #[test]
    fn prompt_with_events() {
        let events = vec![SensorEvent {
            id: "e1".into(),
            timestamp: "1709136000".into(),
            node_name: "cam1".into(),
            event_type: "health_ok".into(),
            details: None,
        }];
        let prompt = build_system_prompt("", &[], &events);
        assert!(prompt.contains("cam1"));
        assert!(prompt.contains("health_ok"));
    }
}
```

**Step 2: Create the run_agent orchestrator**

Update `crates/bubbaloop/src/agent/mod.rs`:

```rust
//! Agent layer — Claude API chat, SQLite memory, scheduling.

pub mod claude;
pub mod dispatch;
pub mod memory;
pub mod prompt;

use std::io::{self, BufRead, Write};
use std::sync::Arc;

use claude::{ClaudeClient, ContentBlock, Message};
use dispatch::Dispatcher;
use memory::Memory;

/// Configuration for the agent.
pub struct AgentConfig {
    pub model: Option<String>,
}

/// Run the interactive agent loop.
///
/// Connects to Claude API, dispatches MCP tools internally, persists
/// conversations and events to SQLite.
pub async fn run_agent(
    config: AgentConfig,
    session: Arc<zenoh::Session>,
    node_manager: Arc<crate::daemon::node_manager::NodeManager>,
) -> anyhow::Result<()> {
    // Initialize components
    let client = ClaudeClient::from_env(config.model.as_deref())?;
    let dispatcher = Dispatcher::new(node_manager, session);
    let tools = dispatcher.tool_definitions();

    let db_path = crate::daemon::registry::get_bubbaloop_home().join("memory.db");
    let mem = Memory::open(&db_path)?;

    let mut messages: Vec<Message> = Vec::new();

    println!("Bubbaloop Agent (type 'quit' to exit)");
    println!("Connected to Claude. {} MCP tools available.\n", tools.len());

    let stdin = io::stdin();
    let mut stdout = io::stdout();

    loop {
        // Prompt
        print!("> ");
        stdout.flush()?;

        // Read user input
        let mut input = String::new();
        if stdin.lock().read_line(&mut input)? == 0 {
            break; // EOF
        }
        let input = input.trim();
        if input.is_empty() {
            continue;
        }
        if input == "quit" || input == "exit" {
            break;
        }

        // Log user message
        mem.log_message("user", input, None).ok();
        messages.push(Message::user(input));

        // Build system prompt with live data
        let inventory = dispatcher.get_node_inventory().await;
        let schedules = mem.list_schedules().unwrap_or_default();
        let events = mem.recent_events(10).unwrap_or_default();
        let system = prompt::build_system_prompt(&inventory, &schedules, &events);

        // Conversation loop (handles multi-step tool use)
        loop {
            let response = client.send(&system, &messages, &tools).await?;

            // Check for tool use
            let assistant_msg = Message {
                role: "assistant".into(),
                content: response.content,
            };

            let tool_calls = assistant_msg.tool_uses();
            let has_tools = !tool_calls.is_empty();

            // Print text portions
            let text = assistant_msg.text();
            if !text.is_empty() {
                println!("\n{}\n", text);
            }

            messages.push(assistant_msg.clone());

            if has_tools {
                // Dispatch each tool call
                let mut results = Vec::new();
                for (id, name, input) in &tool_calls {
                    log::info!("[agent] tool call: {} ({})", name, id);
                    let result = dispatcher.call_tool(id, name, input).await;
                    results.push(result);
                }

                // Log tool calls
                let tool_json = serde_json::to_string(&tool_calls).ok();
                mem.log_message("assistant", &text, tool_json.as_deref()).ok();

                messages.push(Message::tool_results(results));
            } else {
                // Final response — log and break inner loop
                mem.log_message("assistant", &text, None).ok();
                break;
            }

            // If stop_reason is not "tool_use", break
            if response.stop_reason.as_deref() != Some("tool_use") {
                break;
            }
        }
    }

    println!("Goodbye.");
    Ok(())
}
```

**Step 3: Create CLI command**

Create `crates/bubbaloop/src/cli/agent.rs`:

```rust
//! `bubbaloop agent` — interactive AI chat for hardware control.

use argh::FromArgs;

/// Start the interactive AI agent for hardware control
#[derive(FromArgs, PartialEq, Debug)]
#[argh(subcommand, name = "agent")]
pub struct AgentCommand {
    /// Claude model to use (default: claude-sonnet-4-20250514)
    #[argh(option, short = 'm')]
    pub model: Option<String>,

    /// zenoh endpoint to connect to (default: auto-discover)
    #[argh(option, short = 'z')]
    pub zenoh_endpoint: Option<String>,
}
```

**Step 4: Wire into CLI**

In `crates/bubbaloop/src/cli/mod.rs`, add:

```rust
pub mod agent;
```

And add re-export:

```rust
pub use agent::AgentCommand;
```

**Step 5: Wire into binary**

In `crates/bubbaloop/src/bin/bubbaloop.rs`:

1. Add import:
```rust
use bubbaloop::cli::{DebugCommand, MarketplaceCommand, NodeCommand, UpCommand, AgentCommand};
```

2. Add variant to `Command` enum:
```rust
    Agent(AgentCommand),
```

3. Add help text in the `None` arm:
```rust
            eprintln!("  agent     Chat with your hardware via Claude AI:");
            eprintln!("              -m, --model <model>: Claude model");
            eprintln!("              -z, --zenoh-endpoint <endpoint>: Zenoh endpoint");
```

4. Add dispatch arm (after the `Up` arm):
```rust
        Some(Command::Agent(cmd)) => {
            // Re-initialize logging for agent (info level)
            drop(
                env_logger::Builder::from_env(env_logger::Env::default().default_filter_or("info"))
                    .try_init(),
            );

            // Connect to Zenoh
            log::info!("Connecting to Zenoh...");
            let session = bubbaloop::daemon::create_session(cmd.zenoh_endpoint.as_deref())
                .await
                .map_err(|e| e as Box<dyn std::error::Error>)?;

            // Create node manager
            log::info!("Initializing node manager...");
            let node_manager = bubbaloop::daemon::NodeManager::new().await?;

            // Run agent
            bubbaloop::agent::run_agent(
                bubbaloop::agent::AgentConfig {
                    model: cmd.model,
                },
                session,
                node_manager,
            )
            .await?;
        }
```

**Step 6: Run check**

Run: `pixi run check`
Expected: Compiles (may need minor adjustments to type imports)

**Step 7: Run tests**

Run: `pixi run test`
Expected: All tests pass

**Step 8: Commit**

```bash
git add crates/bubbaloop/src/agent/ crates/bubbaloop/src/cli/agent.rs crates/bubbaloop/src/cli/mod.rs crates/bubbaloop/src/bin/bubbaloop.rs
git commit -m "feat: add bubbaloop agent command with Claude chat loop"
```

---

## Task 6: Scheduler

**Files:**
- Create: `crates/bubbaloop/src/agent/scheduler.rs`
- Modify: `crates/bubbaloop/src/agent/mod.rs`

**Step 1: Write scheduler with Tier 1 built-in actions**

Create `crates/bubbaloop/src/agent/scheduler.rs`:

```rust
//! Cron scheduler — Tier 1 (offline, no LLM) + Tier 2 (LLM-driven).
//!
//! Runs as a background tokio task. Every 60 seconds, checks the `schedules`
//! table for jobs whose `next_run` has passed, executes them, and updates
//! `last_run` / `next_run`.

use std::sync::Arc;

use crate::agent::memory::{Memory, Schedule};
use crate::mcp::platform::{NodeCommand, PlatformOperations};

#[derive(Debug, thiserror::Error)]
pub enum SchedulerError {
    #[error("Memory error: {0}")]
    Memory(#[from] crate::agent::memory::MemoryError),
    #[error("Cron parse error: {0}")]
    CronParse(String),
    #[error("Action error: {0}")]
    Action(String),
}

pub type Result<T> = std::result::Result<T, SchedulerError>;

/// Built-in Tier 1 actions (closed set, no arbitrary code execution).
#[derive(Debug, Clone, serde::Deserialize)]
#[serde(tag = "action")]
pub enum Tier1Action {
    #[serde(rename = "check_all_health")]
    CheckAllHealth,
    #[serde(rename = "restart")]
    Restart,
    #[serde(rename = "start_node")]
    StartNode { node: String },
    #[serde(rename = "stop_node")]
    StopNode { node: String },
    #[serde(rename = "send_command")]
    SendCommand {
        node: String,
        command: String,
        #[serde(default)]
        params: serde_json::Value,
    },
    #[serde(rename = "capture_frame")]
    CaptureFrame {
        node: String,
        #[serde(default)]
        params: serde_json::Value,
    },
    #[serde(rename = "log_event")]
    LogEvent { message: String },
    #[serde(rename = "notify")]
    Notify { message: String },
}

/// Parse a cron expression and compute the next run time from now.
pub fn next_run_after(cron_expr: &str, after_epoch_secs: u64) -> Result<u64> {
    use cron::Schedule as CronSchedule;
    use std::str::FromStr;

    let schedule = CronSchedule::from_str(cron_expr)
        .map_err(|e| SchedulerError::CronParse(format!("{}: {}", cron_expr, e)))?;

    let after = chrono_from_epoch(after_epoch_secs);
    let next = schedule
        .after(&after)
        .next()
        .ok_or_else(|| SchedulerError::CronParse("no next occurrence".into()))?;

    Ok(next.timestamp() as u64)
}

/// Convert epoch seconds to a chrono DateTime for cron evaluation.
fn chrono_from_epoch(secs: u64) -> chrono::DateTime<chrono::Utc> {
    chrono::DateTime::from_timestamp(secs as i64, 0).unwrap_or_default()
}

/// Get the current epoch seconds.
fn now_epoch() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs()
}

/// Execute a single Tier 1 action against the platform.
pub async fn execute_tier1_action<P: PlatformOperations>(
    action: &Tier1Action,
    platform: &P,
    memory: &Memory,
) -> Result<()> {
    match action {
        Tier1Action::CheckAllHealth => {
            let nodes = platform
                .list_nodes()
                .await
                .map_err(|e| SchedulerError::Action(e.to_string()))?;
            for node in &nodes {
                log::info!("[scheduler] health: {} = {}", node.name, node.health);
            }
            Ok(())
        }
        Tier1Action::Restart => {
            // Restart unhealthy nodes
            let nodes = platform
                .list_nodes()
                .await
                .map_err(|e| SchedulerError::Action(e.to_string()))?;
            for node in &nodes {
                if node.health != "Healthy" && node.status == "Running" {
                    log::info!("[scheduler] restarting unhealthy node: {}", node.name);
                    platform
                        .execute_command(&node.name, NodeCommand::Restart)
                        .await
                        .map_err(|e| SchedulerError::Action(e.to_string()))?;
                    memory
                        .log_event(&node.name, "scheduled_restart", None)
                        .ok();
                }
            }
            Ok(())
        }
        Tier1Action::StartNode { node } => {
            platform
                .execute_command(node, NodeCommand::Start)
                .await
                .map_err(|e| SchedulerError::Action(e.to_string()))?;
            Ok(())
        }
        Tier1Action::StopNode { node } => {
            platform
                .execute_command(node, NodeCommand::Stop)
                .await
                .map_err(|e| SchedulerError::Action(e.to_string()))?;
            Ok(())
        }
        Tier1Action::SendCommand {
            node,
            command,
            params,
        } => {
            let payload = serde_json::json!({
                "command": command,
                "params": params,
            });
            let key = format!(
                "bubbaloop/*/*/{}/command",
                node
            );
            platform
                .send_zenoh_query(&key, serde_json::to_vec(&payload).unwrap_or_default())
                .await
                .map_err(|e| SchedulerError::Action(e.to_string()))?;
            Ok(())
        }
        Tier1Action::CaptureFrame { node, params } => {
            let payload = serde_json::json!({
                "command": "capture_frame",
                "params": params,
            });
            let key = format!(
                "bubbaloop/*/*/{}/command",
                node
            );
            platform
                .send_zenoh_query(&key, serde_json::to_vec(&payload).unwrap_or_default())
                .await
                .map_err(|e| SchedulerError::Action(e.to_string()))?;
            Ok(())
        }
        Tier1Action::LogEvent { message } => {
            memory
                .log_event("scheduler", "log_event", Some(message))
                .map_err(|e| SchedulerError::Action(e.to_string()))?;
            Ok(())
        }
        Tier1Action::Notify { message } => {
            println!("[bubbaloop] {}", message);
            Ok(())
        }
    }
}

/// Run the scheduler loop. Checks every 60 seconds for due jobs.
pub async fn run_scheduler<P: PlatformOperations>(
    memory: Arc<Memory>,
    platform: Arc<P>,
    mut shutdown: tokio::sync::watch::Receiver<()>,
) {
    log::info!("[scheduler] started");

    loop {
        tokio::select! {
            _ = shutdown.changed() => {
                log::info!("[scheduler] shutting down");
                break;
            }
            _ = tokio::time::sleep(std::time::Duration::from_secs(60)) => {
                if let Err(e) = tick(&memory, &*platform).await {
                    log::warn!("[scheduler] tick error: {}", e);
                }
            }
        }
    }
}

/// Single scheduler tick — check and execute due jobs.
async fn tick<P: PlatformOperations>(memory: &Memory, platform: &P) -> Result<()> {
    let schedules = memory.list_schedules()?;
    let now = now_epoch();

    for schedule in &schedules {
        // Only process Tier 1 schedules in the background loop
        if schedule.tier != 1 {
            continue;
        }

        // Check if due
        let next_run = schedule
            .next_run
            .as_deref()
            .and_then(|s| s.parse::<u64>().ok())
            .unwrap_or(0);

        if next_run > now {
            continue; // Not due yet
        }

        log::info!("[scheduler] executing: {}", schedule.name);

        // Parse actions JSON
        let actions: Vec<Tier1Action> = match serde_json::from_str(&schedule.actions) {
            Ok(a) => a,
            Err(e) => {
                log::warn!(
                    "[scheduler] failed to parse actions for '{}': {}",
                    schedule.name,
                    e
                );
                continue;
            }
        };

        // Execute each action
        for action in &actions {
            if let Err(e) = execute_tier1_action(action, platform, memory).await {
                log::warn!(
                    "[scheduler] action error in '{}': {}",
                    schedule.name,
                    e
                );
            }
        }

        // Compute next run and update
        let last_run = format!("{}", now);
        match next_run_after(&schedule.cron, now) {
            Ok(next) => {
                let next_str = format!("{}", next);
                memory
                    .update_schedule_run(&schedule.name, &last_run, &next_str)
                    .ok();
            }
            Err(e) => {
                log::warn!(
                    "[scheduler] failed to compute next run for '{}': {}",
                    schedule.name,
                    e
                );
            }
        }
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_tier1_actions() {
        let json = r#"[
            {"action": "check_all_health"},
            {"action": "restart"},
            {"action": "log_event", "message": "health check done"}
        ]"#;
        let actions: Vec<Tier1Action> = serde_json::from_str(json).unwrap();
        assert_eq!(actions.len(), 3);
    }

    #[test]
    fn parse_start_node_action() {
        let json = r#"{"action": "start_node", "node": "rtsp-camera"}"#;
        let action: Tier1Action = serde_json::from_str(json).unwrap();
        match action {
            Tier1Action::StartNode { node } => assert_eq!(node, "rtsp-camera"),
            _ => panic!("expected StartNode"),
        }
    }

    #[test]
    fn parse_send_command_action() {
        let json =
            r#"{"action": "send_command", "node": "cam1", "command": "capture_frame", "params": {"resolution": "1080p"}}"#;
        let action: Tier1Action = serde_json::from_str(json).unwrap();
        match action {
            Tier1Action::SendCommand {
                node,
                command,
                params,
            } => {
                assert_eq!(node, "cam1");
                assert_eq!(command, "capture_frame");
                assert!(params.get("resolution").is_some());
            }
            _ => panic!("expected SendCommand"),
        }
    }

    #[test]
    fn parse_unknown_action_fails() {
        let json = r#"{"action": "drop_database"}"#;
        let result: std::result::Result<Tier1Action, _> = serde_json::from_str(json);
        assert!(result.is_err());
    }
}
```

**Note:** The `cron` crate may pull in `chrono` as a transitive dependency. If so, that's fine — it's lightweight. If `cron` 0.15 doesn't use chrono, adjust the `chrono_from_epoch` function accordingly. Check `cron` crate docs for the exact API.

**Step 2: Add chrono as dependency if needed**

If the `cron` crate requires chrono, add to workspace deps:

```toml
chrono = { version = "0.4", default-features = false, features = ["std"] }
```

And in `crates/bubbaloop/Cargo.toml`:
```toml
chrono.workspace = true
```

**Step 3: Update agent/mod.rs**

Add `pub mod scheduler;` to the module declarations.

**Step 4: Run check + tests**

Run: `pixi run check && pixi run test`

**Step 5: Commit**

```bash
git add crates/bubbaloop/src/agent/scheduler.rs crates/bubbaloop/src/agent/mod.rs
git commit -m "feat: add Tier 1 cron scheduler with built-in actions"
```

---

## Task 7: Integration — Wire Scheduler into Agent + Skill Loader

**Files:**
- Modify: `crates/bubbaloop/src/agent/mod.rs` (start scheduler on agent launch)
- Modify: `crates/bubbaloop/src/cli/up.rs` (register skill schedules in SQLite)

**Step 1: Start scheduler alongside agent**

In `agent/mod.rs`, update `run_agent()` to spawn the scheduler:

After initializing the `Dispatcher`, add:

```rust
    // Start scheduler in background
    let (shutdown_tx, shutdown_rx) = tokio::sync::watch::channel(());
    let sched_memory = Arc::new(Memory::open(&db_path)?);
    let sched_platform = Arc::new(DaemonPlatform { /* same fields */ });
    tokio::spawn(scheduler::run_scheduler(
        sched_memory,
        sched_platform,
        shutdown_rx,
    ));
```

And drop `shutdown_tx` when the agent loop ends (implicit on function return).

**Step 2: Register skill schedules at startup**

In `crates/bubbaloop/src/cli/up.rs`, after processing skills, register any that have `schedule` + `actions` fields into the SQLite `schedules` table.

Add to `UpCommand::run()` after the skill processing loop:

```rust
        // 6. Register skill schedules in memory DB
        let db_path = get_bubbaloop_home().join("memory.db");
        if let Ok(mem) = crate::agent::memory::Memory::open(&db_path) {
            for skill in &skill_configs {
                if let Some(ref schedule_expr) = skill.schedule {
                    if !skill.actions.is_empty() {
                        let actions_json = serde_json::to_string(&skill.actions)
                            .unwrap_or_else(|_| "[]".to_string());
                        let sched = crate::agent::memory::Schedule {
                            id: uuid::Uuid::new_v4().to_string(),
                            name: skill.name.clone(),
                            cron: schedule_expr.clone(),
                            actions: actions_json,
                            tier: 1,
                            last_run: None,
                            next_run: None,
                            created_by: "yaml".to_string(),
                        };
                        if self.dry_run {
                            println!("  [dry-run] Would register schedule: {} ({})", skill.name, schedule_expr);
                        } else if let Err(e) = mem.upsert_schedule(&sched) {
                            println!("  [warn] Failed to register schedule: {}", e);
                        } else {
                            println!("  Registered schedule: {} ({})", skill.name, schedule_expr);
                        }
                    }
                }
            }
        }
```

**Step 3: Run check + tests**

Run: `pixi run check && pixi run test`

**Step 4: Commit**

```bash
git add crates/bubbaloop/src/agent/mod.rs crates/bubbaloop/src/cli/up.rs
git commit -m "feat: wire scheduler into agent and register skill schedules"
```

---

## Task 8: Clippy, Tests, Update ROADMAP

**Files:**
- Modify: `ROADMAP.md` (check Phase 1-4 boxes)
- All agent files (clippy fixes)

**Step 1: Run full verification**

```bash
pixi run fmt
pixi run clippy
pixi run test
```

Fix any warnings or test failures.

**Step 2: Update ROADMAP.md checkboxes**

Change Phase 1 items from `[ ]` to `[x]`:
```markdown
- [x] `~/.bubbaloop/skills/*.yaml` loader
- [x] Driver registry: map `driver: rtsp` → marketplace node `rtsp-camera`
- [x] Auto-install: download precompiled binary if driver not present
- [x] Config injection: YAML config → node env vars / config.yaml
- [x] `bubbaloop up` command
- [x] Built-in driver catalog (v1)
```

Change Phase 2 items from `[ ]` to `[x]`:
```markdown
- [x] `bubbaloop agent` CLI command
- [x] Claude API integration via `reqwest` (tool_use for MCP tools)
- [x] Internal MCP tool dispatch (call tools without HTTP round-trip)
- [x] System prompt injection: sensor inventory, node status, active schedules
- [x] Multi-turn conversation loop with tool use
```

Phase 3:
```markdown
- [x] SQLite via `rusqlite` (static libsqlite3) at `~/.bubbaloop/memory.db`
- [x] `conversations` table with timestamp indexing
- [x] `sensor_events` table: health changes, crashes, alerts
- [x] `schedules` table: active jobs + execution history
- [x] Context injection: recent events included in agent system prompt
```

Phase 4:
```markdown
- [x] Tier 1 cron executor with built-in action set (offline, no LLM)
- [x] YAML `schedule:` + `actions:` syntax in skill files
```

Leave unchecked:
```markdown
- [ ] Tier 2 conversational schedules stored in SQLite
- [ ] Rate limiting: configurable max LLM calls/day
- [ ] `bubbaloop jobs` CLI: list, pause, resume, delete
- [ ] Execution history logged in SQLite
- [ ] HTTP chat endpoint for future dashboard integration
- [ ] Daemon event hook: write sensor events as they happen (no polling)
```

**Step 3: Commit**

```bash
git add -A
git commit -m "feat: complete Phases 2-4 — agent loop, SQLite memory, scheduler"
```

**Step 4: Push**

```bash
git push
```
