//! SQLite memory layer — conversations, sensor events, schedules.
//!
//! Stores agent conversation history, sensor lifecycle events, and
//! scheduled jobs in a single SQLite database at `~/.bubbaloop/memory.db`.

use rusqlite::{params, Connection};
use std::path::Path;

/// Errors from memory operations.
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
    /// Creates all tables if they don't exist. Sets WAL journal mode
    /// and file permissions to 0600 (owner read/write only).
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
    pub fn log_message(&self, role: &str, content: &str, tool_calls: Option<&str>) -> Result<()> {
        let id = uuid::Uuid::new_v4().to_string();
        let timestamp = now_iso8601();
        self.conn.execute(
            "INSERT INTO conversations (id, timestamp, role, content, tool_calls) \
             VALUES (?1, ?2, ?3, ?4, ?5)",
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
            "INSERT INTO sensor_events (id, timestamp, node_name, event_type, details) \
             VALUES (?1, ?2, ?3, ?4, ?5)",
            params![id, timestamp, node_name, event_type, details],
        )?;
        Ok(())
    }

    /// Get recent conversation messages (most recent first).
    pub fn recent_conversations(&self, limit: usize) -> Result<Vec<ConversationRow>> {
        let mut stmt = self.conn.prepare(
            "SELECT id, timestamp, role, content, tool_calls \
             FROM conversations ORDER BY timestamp DESC LIMIT ?1",
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
        rows.collect::<std::result::Result<Vec<_>, _>>()
            .map_err(MemoryError::from)
    }

    /// Get recent sensor events (most recent first).
    pub fn recent_events(&self, limit: usize) -> Result<Vec<SensorEvent>> {
        let mut stmt = self.conn.prepare(
            "SELECT id, timestamp, node_name, event_type, details \
             FROM sensor_events ORDER BY timestamp DESC LIMIT ?1",
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
        rows.collect::<std::result::Result<Vec<_>, _>>()
            .map_err(MemoryError::from)
    }

    /// Get events for a specific node (most recent first).
    pub fn events_for_node(&self, node_name: &str, limit: usize) -> Result<Vec<SensorEvent>> {
        let mut stmt = self.conn.prepare(
            "SELECT id, timestamp, node_name, event_type, details \
             FROM sensor_events WHERE node_name = ?1 \
             ORDER BY timestamp DESC LIMIT ?2",
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
        rows.collect::<std::result::Result<Vec<_>, _>>()
            .map_err(MemoryError::from)
    }

    /// Insert or update a schedule (upsert on name conflict).
    pub fn upsert_schedule(&self, schedule: &Schedule) -> Result<()> {
        self.conn.execute(
            "INSERT INTO schedules (id, name, cron, actions, tier, last_run, next_run, created_by) \
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8) \
             ON CONFLICT(name) DO UPDATE SET \
                cron = excluded.cron, \
                actions = excluded.actions, \
                tier = excluded.tier, \
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

    /// List all schedules, ordered by name.
    pub fn list_schedules(&self) -> Result<Vec<Schedule>> {
        let mut stmt = self.conn.prepare(
            "SELECT id, name, cron, actions, tier, last_run, next_run, created_by \
             FROM schedules ORDER BY name",
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
        rows.collect::<std::result::Result<Vec<_>, _>>()
            .map_err(MemoryError::from)
    }

    /// Update last_run and next_run for a schedule.
    pub fn update_schedule_run(&self, name: &str, last_run: &str, next_run: &str) -> Result<()> {
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

/// ISO 8601 timestamp for the current time (UTC, no chrono dependency).
///
/// Returns a string like `"1709136000"` (Unix epoch seconds), which is
/// monotonically increasing and sufficient for ordering. A proper ISO 8601
/// string (e.g. `"2024-02-28T16:00:00Z"`) would require the `chrono` crate.
fn now_iso8601() -> String {
    let duration = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default();
    format!("{}", duration.as_secs())
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::tempdir;

    /// Helper: create an in-tempdir Memory instance.
    /// Returns (Memory, TempDir) so the tempdir stays alive.
    fn test_memory() -> (Memory, tempfile::TempDir) {
        let dir = tempdir().unwrap();
        let mem = Memory::open(&dir.path().join("test.db")).unwrap();
        (mem, dir)
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
    fn open_twice_idempotent() {
        let dir = tempdir().unwrap();
        let db_path = dir.path().join("test.db");
        Memory::open(&db_path).unwrap();
        Memory::open(&db_path).unwrap(); // should not error
    }

    #[test]
    fn log_and_retrieve_messages() {
        let (mem, _dir) = test_memory();
        mem.log_message("user", "hello", None).unwrap();
        mem.log_message("assistant", "hi there", None).unwrap();
        mem.log_message(
            "assistant",
            "tool result",
            Some(r#"[{"name":"list_nodes"}]"#),
        )
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
        let (mem, _dir) = test_memory();
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
        let (mem, _dir) = test_memory();
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
        let (mem, _dir) = test_memory();
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

        // Update cron via upsert on same name
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
        let (mem, _dir) = test_memory();
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
        let (mem, _dir) = test_memory();
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
    fn conversation_limit() {
        let (mem, _dir) = test_memory();
        for i in 0..10 {
            mem.log_message("user", &format!("msg {}", i), None)
                .unwrap();
        }
        let msgs = mem.recent_conversations(3).unwrap();
        assert_eq!(msgs.len(), 3);
    }

    #[cfg(unix)]
    #[test]
    fn file_permissions_0600() {
        use std::os::unix::fs::PermissionsExt;
        let dir = tempdir().unwrap();
        let db_path = dir.path().join("secure.db");
        Memory::open(&db_path).unwrap();
        let perms = std::fs::metadata(&db_path).unwrap().permissions();
        assert_eq!(perms.mode() & 0o777, 0o600);
    }
}
