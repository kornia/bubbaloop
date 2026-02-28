//! SQLite memory layer — conversations, sensor events, schedules.
//!
//! Stores agent conversation history, sensor lifecycle events, and
//! scheduled jobs in a single SQLite database at `~/.bubbaloop/memory.db`.

use chrono::SecondsFormat;
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

/// A skill search result with BM25 rank score.
#[derive(Debug, Clone)]
pub struct SkillSearchResult {
    pub name: String,
    pub driver: String,
    pub body: String,
    pub rank: f64,
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

        // WAL mode for better concurrent read performance.
        // Use query_row because bundled-full enables COLUMN_METADATA which
        // makes pragma_update fail with ExecuteReturnedResults.
        conn.query_row("PRAGMA journal_mode=WAL", [], |_| Ok(()))?;
        // Allow up to 5 s wait when another connection holds the write lock
        conn.query_row("PRAGMA busy_timeout=5000", [], |_| Ok(()))?;

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

        // FTS5 virtual tables are created via query() rather than execute_batch()
        // because SQLite returns SQLITE_ROW for CREATE VIRTUAL TABLE in some
        // configurations (bundled-full enables extra_check which rejects SQLITE_ROW
        // from execute), so we use the query path and discard any rows.
        for fts_ddl in &[
            "CREATE VIRTUAL TABLE IF NOT EXISTS fts_conversations USING fts5(\
                id UNINDEXED, role UNINDEXED, content, timestamp UNINDEXED)",
            "CREATE VIRTUAL TABLE IF NOT EXISTS fts_events USING fts5(\
                id UNINDEXED, node_name, event_type, details, timestamp UNINDEXED)",
            "CREATE VIRTUAL TABLE IF NOT EXISTS fts_skills USING fts5(\
                name, driver UNINDEXED, body)",
        ] {
            conn.prepare(fts_ddl)?.query([])?.next()?;
        }

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
        let timestamp = now_rfc3339();
        self.conn.execute(
            "INSERT INTO conversations (id, timestamp, role, content, tool_calls) \
             VALUES (?1, ?2, ?3, ?4, ?5)",
            params![id, timestamp, role, content, tool_calls],
        )?;
        // Dual-write to FTS5 index for full-text search
        self.conn.execute(
            "INSERT INTO fts_conversations (id, role, content, timestamp) \
             VALUES (?1, ?2, ?3, ?4)",
            params![id, role, content, timestamp],
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
        let timestamp = now_rfc3339();
        self.conn.execute(
            "INSERT INTO sensor_events (id, timestamp, node_name, event_type, details) \
             VALUES (?1, ?2, ?3, ?4, ?5)",
            params![id, timestamp, node_name, event_type, details],
        )?;
        // Dual-write to FTS5 index for full-text search
        self.conn.execute(
            "INSERT INTO fts_events (id, node_name, event_type, details, timestamp) \
             VALUES (?1, ?2, ?3, ?4, ?5)",
            params![id, node_name, event_type, details.unwrap_or(""), timestamp],
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

    /// Delete conversations older than `days` days (both regular and FTS5 tables).
    pub fn prune_old_conversations(&self, days: u32) -> Result<usize> {
        let cutoff = (chrono::Utc::now() - chrono::Duration::days(days as i64))
            .to_rfc3339_opts(SecondsFormat::Secs, true);
        // Delete from FTS5 index first (matching by id via subquery)
        self.conn.execute(
            "DELETE FROM fts_conversations WHERE id IN \
             (SELECT id FROM conversations WHERE timestamp < ?1)",
            params![cutoff],
        )?;
        let count = self.conn.execute(
            "DELETE FROM conversations WHERE timestamp < ?1",
            params![cutoff],
        )?;
        Ok(count)
    }

    /// Delete sensor events older than `days` days (both regular and FTS5 tables).
    pub fn prune_old_events(&self, days: u32) -> Result<usize> {
        let cutoff = (chrono::Utc::now() - chrono::Duration::days(days as i64))
            .to_rfc3339_opts(SecondsFormat::Secs, true);
        // Delete from FTS5 index first (matching by id via subquery)
        self.conn.execute(
            "DELETE FROM fts_events WHERE id IN \
             (SELECT id FROM sensor_events WHERE timestamp < ?1)",
            params![cutoff],
        )?;
        let count = self.conn.execute(
            "DELETE FROM sensor_events WHERE timestamp < ?1",
            params![cutoff],
        )?;
        Ok(count)
    }

    /// Full-text search over conversations using FTS5 BM25 ranking.
    pub fn search_conversations(&self, query: &str, limit: usize) -> Result<Vec<ConversationRow>> {
        if query.is_empty() {
            return Ok(Vec::new());
        }
        let sanitized = sanitize_fts5_query(query);
        if sanitized.is_empty() {
            return Ok(Vec::new());
        }
        let mut stmt = self.conn.prepare(
            "SELECT f.id, f.timestamp, f.role, f.content, c.tool_calls \
             FROM fts_conversations f \
             JOIN conversations c ON c.id = f.id \
             WHERE fts_conversations MATCH ?1 \
             ORDER BY rank \
             LIMIT ?2",
        )?;
        let rows = stmt.query_map(params![sanitized, limit as i64], |row| {
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

    /// Full-text search over sensor events using FTS5 BM25 ranking.
    pub fn search_events(&self, query: &str, limit: usize) -> Result<Vec<SensorEvent>> {
        if query.is_empty() {
            return Ok(Vec::new());
        }
        let sanitized = sanitize_fts5_query(query);
        if sanitized.is_empty() {
            return Ok(Vec::new());
        }
        let mut stmt = self.conn.prepare(
            "SELECT f.id, f.timestamp, f.node_name, f.event_type, f.details \
             FROM fts_events f \
             WHERE fts_events MATCH ?1 \
             ORDER BY rank \
             LIMIT ?2",
        )?;
        let rows = stmt.query_map(params![sanitized, limit as i64], |row| {
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

    /// Full-text search over indexed skills using FTS5 BM25 ranking.
    pub fn search_skills(&self, query: &str, limit: usize) -> Result<Vec<SkillSearchResult>> {
        if query.is_empty() {
            return Ok(Vec::new());
        }
        let sanitized = sanitize_fts5_query(query);
        if sanitized.is_empty() {
            return Ok(Vec::new());
        }
        let mut stmt = self.conn.prepare(
            "SELECT name, driver, body, rank \
             FROM fts_skills \
             WHERE fts_skills MATCH ?1 \
             ORDER BY rank \
             LIMIT ?2",
        )?;
        let rows = stmt.query_map(params![sanitized, limit as i64], |row| {
            Ok(SkillSearchResult {
                name: row.get(0)?,
                driver: row.get(1)?,
                body: row.get(2)?,
                rank: row.get(3)?,
            })
        })?;
        rows.collect::<std::result::Result<Vec<_>, _>>()
            .map_err(MemoryError::from)
    }

    /// Index skill bodies into FTS5 for full-text search.
    ///
    /// Clears existing skill entries and re-indexes from the provided list.
    /// Called at agent startup after loading skills from disk.
    pub fn index_skills(&self, skills: &[crate::skills::SkillConfig]) -> Result<()> {
        self.conn.execute("DELETE FROM fts_skills", [])?;
        let mut stmt = self.conn.prepare(
            "INSERT INTO fts_skills (name, driver, body) VALUES (?1, ?2, ?3)",
        )?;
        for skill in skills {
            if !skill.body.is_empty() {
                stmt.execute(params![skill.name, skill.driver, skill.body])?;
            }
        }
        Ok(())
    }
}

/// Sanitize user input for FTS5 MATCH queries.
///
/// Wraps each word in double quotes to prevent FTS5 operator interpretation
/// (AND, OR, NOT, *, etc.) and joins them with spaces (implicit OR).
fn sanitize_fts5_query(query: &str) -> String {
    query
        .split_whitespace()
        .take(30) // limit to 30 tokens to prevent very long queries
        .map(|word| {
            let escaped = word.replace('"', "\"\"");
            format!("\"{}\"", escaped)
        })
        .collect::<Vec<_>>()
        .join(" OR ")
}

/// RFC 3339 timestamp for the current time (UTC).
///
/// Returns a string like `"2026-02-28T16:00:00Z"` which is both
/// human-readable and sorts lexicographically for correct ORDER BY.
fn now_rfc3339() -> String {
    chrono::Utc::now().to_rfc3339_opts(SecondsFormat::Secs, true)
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

    #[test]
    fn now_rfc3339_returns_valid_format() {
        let ts = now_rfc3339();
        // Must look like "2026-02-28T16:00:00Z"
        assert!(ts.ends_with('Z'), "timestamp should end with Z: {}", ts);
        assert_eq!(ts.len(), 20, "RFC 3339 secs should be 20 chars: {}", ts);
        // Verify chrono can parse it back
        chrono::DateTime::parse_from_rfc3339(&ts).expect("should be valid RFC 3339");
    }

    #[test]
    fn busy_timeout_is_set() {
        let (mem, _dir) = test_memory();
        let timeout: i64 = mem
            .conn
            .pragma_query_value(None, "busy_timeout", |row| row.get(0))
            .unwrap();
        assert_eq!(timeout, 5000);
    }

    #[test]
    fn prune_old_conversations_deletes_old_rows() {
        let (mem, _dir) = test_memory();
        // Insert a row with an old timestamp (epoch string — lexicographically < any RFC 3339 date)
        mem.conn
            .execute(
                "INSERT INTO conversations (id, timestamp, role, content) VALUES (?1, ?2, ?3, ?4)",
                params!["old1", "1709136000", "user", "ancient message"],
            )
            .unwrap();
        // Insert a current-timestamp row via the normal API
        mem.log_message("user", "recent message", None).unwrap();

        let pruned = mem.prune_old_conversations(30).unwrap();
        assert_eq!(pruned, 1, "should prune the old epoch-timestamp row");

        let remaining = mem.recent_conversations(10).unwrap();
        assert_eq!(remaining.len(), 1);
        assert_eq!(remaining[0].content, "recent message");
    }

    #[test]
    fn prune_old_events_deletes_old_rows() {
        let (mem, _dir) = test_memory();
        // Insert a row with an old timestamp
        mem.conn
            .execute(
                "INSERT INTO sensor_events (id, timestamp, node_name, event_type) VALUES (?1, ?2, ?3, ?4)",
                params!["old1", "1709136000", "cam", "started"],
            )
            .unwrap();
        // Insert a current-timestamp row
        mem.log_event("cam", "health_ok", None).unwrap();

        let pruned = mem.prune_old_events(30).unwrap();
        assert_eq!(pruned, 1);

        let remaining = mem.recent_events(10).unwrap();
        assert_eq!(remaining.len(), 1);
        assert_eq!(remaining[0].event_type, "health_ok");
    }

    #[test]
    fn fts_conversations_search() {
        let (mem, _dir) = test_memory();
        mem.log_message("user", "the entrance camera keeps freezing", None).unwrap();
        mem.log_message("user", "what is the weather today", None).unwrap();
        mem.log_message("assistant", "I checked the camera status", None).unwrap();

        let results = mem.search_conversations("camera", 10).unwrap();
        assert_eq!(results.len(), 2, "should match 'camera' in two messages");
        // BM25 rank means more relevant first
        assert!(results[0].content.contains("camera"));
    }

    #[test]
    fn fts_events_search() {
        let (mem, _dir) = test_memory();
        mem.log_event("rtsp-camera", "started", None).unwrap();
        mem.log_event("rtsp-camera", "health_degraded", Some("frame_drop_rate=0.3")).unwrap();
        mem.log_event("openmeteo", "started", None).unwrap();

        let results = mem.search_events("camera", 10).unwrap();
        assert_eq!(results.len(), 2, "should match 'camera' in node_name");

        let results = mem.search_events("frame_drop", 10).unwrap();
        assert_eq!(results.len(), 1, "should match in details");
    }

    #[test]
    fn fts_skills_search() {
        let (mem, _dir) = test_memory();
        let skills = vec![
            crate::skills::SkillConfig {
                name: "entrance-cam".into(),
                driver: "rtsp".into(),
                config: Default::default(),
                schedule: None,
                actions: Vec::new(),
                body: "# Entrance Camera\n\nTapo C200 monitoring the front door.".into(),
            },
            crate::skills::SkillConfig {
                name: "weather-station".into(),
                driver: "http-poll".into(),
                config: Default::default(),
                schedule: None,
                actions: Vec::new(),
                body: "# Weather\n\nOpenMeteo API for temperature and humidity.".into(),
            },
        ];
        mem.index_skills(&skills).unwrap();

        let results = mem.search_skills("camera entrance", 10).unwrap();
        assert_eq!(results.len(), 1);
        assert_eq!(results[0].name, "entrance-cam");

        let results = mem.search_skills("temperature", 10).unwrap();
        assert_eq!(results.len(), 1);
        assert_eq!(results[0].name, "weather-station");
    }

    #[test]
    fn fts_dual_write_consistency() {
        let (mem, _dir) = test_memory();
        mem.log_message("user", "check camera health", None).unwrap();

        // Should be findable via both recency and search
        let recent = mem.recent_conversations(10).unwrap();
        assert_eq!(recent.len(), 1);
        let searched = mem.search_conversations("camera health", 10).unwrap();
        assert_eq!(searched.len(), 1);
        assert_eq!(recent[0].id, searched[0].id);
    }

    #[test]
    fn fts_empty_query_returns_empty() {
        let (mem, _dir) = test_memory();
        mem.log_message("user", "hello world", None).unwrap();

        let results = mem.search_conversations("", 10).unwrap();
        assert!(results.is_empty(), "empty query should return empty results");
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
