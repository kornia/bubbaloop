//! Semantic memory — SQLite jobs + proposals tables.
//!
//! Tier 3 of the 3-tier memory model. Handles structured data:
//! - Jobs: scheduled and one-off agent tasks with retry logic
//! - Proposals: human-in-the-loop approval queue

use rusqlite::{params, Connection};
use std::path::Path;

/// A scheduled or one-off agent job.
#[derive(Debug, Clone, serde::Serialize)]
pub struct Job {
    pub id: String,
    /// Optional cron expression for recurring jobs.
    pub cron_schedule: Option<String>,
    /// Next scheduled run time (epoch seconds).
    pub next_run_at: i64,
    /// The instruction to execute.
    pub prompt_payload: String,
    /// Status: pending, running, completed, failed, dead_letter.
    pub status: String,
    /// Whether this is a recurring job.
    pub recurrence: bool,
    /// Consecutive failure count.
    pub retry_count: i32,
    /// Most recent error message.
    pub last_error: Option<String>,
}

/// A proposal for human-in-the-loop approval.
#[derive(Debug, Clone, serde::Serialize)]
pub struct Proposal {
    pub id: String,
    pub timestamp: String,
    /// Skill or action category.
    pub skill: String,
    /// Human-readable description.
    pub description: String,
    /// JSON array of tool calls to execute.
    pub actions: String,
    /// Status: pending, approved, rejected, expired.
    pub status: String,
    /// Who decided: "user", "mcp", "timeout".
    pub decided_by: Option<String>,
    /// When the decision was made.
    pub decided_at: Option<String>,
}

/// SQLite-backed semantic store for jobs and proposals.
pub struct SemanticStore {
    conn: Connection,
}

impl SemanticStore {
    /// Open (or create) the semantic store at the given path.
    pub fn open(path: &Path) -> super::Result<Self> {
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent)?;
        }

        let conn = Connection::open(path)?;

        // WAL mode + busy timeout
        conn.query_row("PRAGMA journal_mode=WAL", [], |_| Ok(()))?;
        conn.query_row("PRAGMA busy_timeout=5000", [], |_| Ok(()))?;

        // Migrate jobs table: if next_run_at is TEXT, drop and recreate with INTEGER.
        // Jobs are transient — dropping is safe.
        let needs_migration: bool = conn
            .query_row(
                "SELECT type FROM pragma_table_info('jobs') WHERE name = 'next_run_at'",
                [],
                |row| row.get::<_, String>(0),
            )
            .map(|col_type| col_type.to_uppercase() != "INTEGER")
            .unwrap_or(false); // false = table doesn't exist yet, no migration needed
        if needs_migration {
            conn.execute_batch("DROP TABLE IF EXISTS jobs;")?;
        }

        conn.execute_batch(
            "CREATE TABLE IF NOT EXISTS jobs (
                id              TEXT PRIMARY KEY,
                cron_schedule   TEXT,
                next_run_at     INTEGER NOT NULL,
                prompt_payload  TEXT NOT NULL,
                status          TEXT DEFAULT 'pending',
                recurrence      BOOLEAN DEFAULT 0,
                retry_count     INTEGER DEFAULT 0,
                last_error      TEXT
            );

            CREATE TABLE IF NOT EXISTS proposals (
                id          TEXT PRIMARY KEY,
                timestamp   TEXT NOT NULL,
                skill       TEXT NOT NULL,
                description TEXT NOT NULL,
                actions     TEXT NOT NULL,
                status      TEXT NOT NULL DEFAULT 'pending',
                decided_by  TEXT,
                decided_at  TEXT
            );
            CREATE INDEX IF NOT EXISTS idx_proposals_status ON proposals(status);

            CREATE TABLE IF NOT EXISTS world_state (
                key           TEXT PRIMARY KEY,
                value         TEXT NOT NULL,
                confidence    REAL NOT NULL DEFAULT 1.0,
                source_topic  TEXT,
                source_node   TEXT,
                last_seen_at  INTEGER NOT NULL,
                max_age_secs  INTEGER NOT NULL DEFAULT 300,
                created_at    INTEGER NOT NULL
            );
            CREATE INDEX IF NOT EXISTS idx_ws_last_seen ON world_state(last_seen_at);

            CREATE TABLE IF NOT EXISTS beliefs (
                id                 TEXT PRIMARY KEY,
                subject            TEXT NOT NULL,
                predicate          TEXT NOT NULL,
                value              TEXT NOT NULL,
                confidence         REAL NOT NULL DEFAULT 1.0,
                source             TEXT NOT NULL,
                first_observed     INTEGER NOT NULL,
                last_confirmed     INTEGER NOT NULL,
                confirmation_count INTEGER NOT NULL DEFAULT 1,
                contradiction_count INTEGER NOT NULL DEFAULT 0,
                notes              TEXT,
                UNIQUE(subject, predicate)
            );",
        )?;

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

    // ── Jobs CRUD ───────────────────────────────────────────────────

    /// Create a new job.
    pub fn create_job(&self, job: &Job) -> super::Result<()> {
        self.conn.execute(
            "INSERT INTO jobs (id, cron_schedule, next_run_at, prompt_payload, status, recurrence, retry_count, last_error) \
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8)",
            params![
                job.id,
                job.cron_schedule,
                job.next_run_at,
                job.prompt_payload,
                job.status,
                job.recurrence,
                job.retry_count,
                job.last_error,
            ],
        )?;
        Ok(())
    }

    /// Get all pending jobs that are due (next_run_at <= now).
    pub fn pending_jobs(&self) -> super::Result<Vec<Job>> {
        let now = super::now_epoch_secs() as i64;
        let mut stmt = self.conn.prepare(
            "SELECT id, cron_schedule, next_run_at, prompt_payload, status, recurrence, retry_count, last_error \
             FROM jobs WHERE status = 'pending' AND next_run_at <= ?1 \
             ORDER BY next_run_at ASC",
        )?;
        let rows = stmt.query_map(params![now], |row| {
            Ok(Job {
                id: row.get(0)?,
                cron_schedule: row.get(1)?,
                next_run_at: row.get(2)?,
                prompt_payload: row.get(3)?,
                status: row.get(4)?,
                recurrence: row.get(5)?,
                retry_count: row.get(6)?,
                last_error: row.get(7)?,
            })
        })?;
        rows.collect::<std::result::Result<Vec<_>, _>>()
            .map_err(super::MemoryError::from)
    }

    /// Mark a job as running.
    pub fn start_job(&self, id: &str) -> super::Result<()> {
        self.conn.execute(
            "UPDATE jobs SET status = 'running' WHERE id = ?1",
            params![id],
        )?;
        Ok(())
    }

    /// Mark a job as completed. Resets retry_count and clears last_error.
    /// If recurring, sets next_run_at and resets to pending.
    pub fn complete_job(&self, id: &str, next_run_at: Option<i64>) -> super::Result<()> {
        match next_run_at {
            Some(next) => {
                self.conn.execute(
                    "UPDATE jobs SET status = 'pending', retry_count = 0, last_error = NULL, next_run_at = ?1 WHERE id = ?2",
                    params![next, id],
                )?;
            }
            None => {
                self.conn.execute(
                    "UPDATE jobs SET status = 'completed', retry_count = 0, last_error = NULL WHERE id = ?1",
                    params![id],
                )?;
            }
        }
        Ok(())
    }

    /// Record a job failure with retry logic.
    ///
    /// If retry_count >= max_retries, sets status to `dead_letter`.
    /// Otherwise, increments retry_count and schedules next attempt with exponential backoff.
    pub fn fail_job(
        &self,
        id: &str,
        error: &str,
        max_retries: u32,
    ) -> super::Result<FailureOutcome> {
        // Get current retry count
        let retry_count: i32 = self.conn.query_row(
            "SELECT retry_count FROM jobs WHERE id = ?1",
            params![id],
            |row| row.get(0),
        )?;

        let new_count = retry_count + 1;

        if new_count as u32 >= max_retries {
            // Dead letter: park the job permanently after exhausting retries
            self.conn.execute(
                "UPDATE jobs SET status = 'dead_letter', retry_count = ?1, last_error = ?2 WHERE id = ?3",
                params![new_count, error, id],
            )?;
            log::warn!(
                "[SemanticStore] Job '{}' dead-lettered after {} retries. Last error: {}",
                id,
                new_count,
                error
            );
            Ok(FailureOutcome::DeadLettered {
                retry_count: new_count,
            })
        } else {
            // Exponential backoff: 30s * 2^retry_count (30s, 60s, 120s)
            let backoff_secs = 30u64 * 2u64.pow(new_count as u32);
            let next_run = super::now_epoch_secs() + backoff_secs;
            self.conn.execute(
                "UPDATE jobs SET status = 'pending', retry_count = ?1, last_error = ?2, next_run_at = ?3 WHERE id = ?4",
                params![new_count, error, next_run as i64, id],
            )?;
            Ok(FailureOutcome::Retrying {
                retry_count: new_count,
                next_run_at: next_run,
            })
        }
    }

    /// Delete a job by ID.
    pub fn delete_job(&self, id: &str) -> super::Result<()> {
        self.conn
            .execute("DELETE FROM jobs WHERE id = ?1", params![id])?;
        Ok(())
    }

    /// List jobs, optionally filtered by status.
    pub fn list_jobs(&self, status: Option<&str>) -> super::Result<Vec<Job>> {
        let (sql, param): (String, Vec<Box<dyn rusqlite::types::ToSql>>) = match status {
            Some(s) => (
                "SELECT id, cron_schedule, next_run_at, prompt_payload, status, recurrence, retry_count, last_error \
                 FROM jobs WHERE status = ?1 ORDER BY next_run_at ASC"
                    .to_string(),
                vec![Box::new(s.to_string())],
            ),
            None => (
                "SELECT id, cron_schedule, next_run_at, prompt_payload, status, recurrence, retry_count, last_error \
                 FROM jobs ORDER BY next_run_at ASC"
                    .to_string(),
                vec![],
            ),
        };

        let mut stmt = self.conn.prepare(&sql)?;
        let rows = stmt.query_map(rusqlite::params_from_iter(param.iter()), |row| {
            Ok(Job {
                id: row.get(0)?,
                cron_schedule: row.get(1)?,
                next_run_at: row.get(2)?,
                prompt_payload: row.get(3)?,
                status: row.get(4)?,
                recurrence: row.get(5)?,
                retry_count: row.get(6)?,
                last_error: row.get(7)?,
            })
        })?;
        rows.collect::<std::result::Result<Vec<_>, _>>()
            .map_err(super::MemoryError::from)
    }

    /// Hard delete a proposal by ID.
    pub fn delete_proposal(&self, id: &str) -> super::Result<()> {
        self.conn
            .execute("DELETE FROM proposals WHERE id = ?1", params![id])?;
        Ok(())
    }

    /// Delete completed/failed/dead-lettered jobs older than `retention_days`.
    ///
    /// Only prunes terminal jobs. Pending/running jobs are kept.
    /// Returns the number of deleted jobs.
    pub fn prune_completed_jobs(&self, retention_days: u32) -> super::Result<usize> {
        if retention_days == 0 {
            return Ok(0);
        }
        let cutoff = super::now_epoch_secs().saturating_sub(u64::from(retention_days) * 86400);
        let count = self.conn.execute(
            "DELETE FROM jobs WHERE status IN ('completed', 'failed', 'dead_letter') AND next_run_at <= ?1",
            params![cutoff as i64],
        )?;
        Ok(count)
    }

    // ── Proposals CRUD ──────────────────────────────────────────────

    /// Create a new proposal.
    pub fn create_proposal(&self, proposal: &Proposal) -> super::Result<()> {
        self.conn.execute(
            "INSERT INTO proposals (id, timestamp, skill, description, actions, status) \
             VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
            params![
                proposal.id,
                proposal.timestamp,
                proposal.skill,
                proposal.description,
                proposal.actions,
                proposal.status,
            ],
        )?;
        Ok(())
    }

    /// List proposals by status.
    pub fn list_proposals(&self, status: Option<&str>) -> super::Result<Vec<Proposal>> {
        let (sql, param): (String, Vec<Box<dyn rusqlite::types::ToSql>>) = match status {
            Some(s) => (
                "SELECT id, timestamp, skill, description, actions, status, decided_by, decided_at \
                 FROM proposals WHERE status = ?1 ORDER BY timestamp DESC"
                    .to_string(),
                vec![Box::new(s.to_string())],
            ),
            None => (
                "SELECT id, timestamp, skill, description, actions, status, decided_by, decided_at \
                 FROM proposals ORDER BY timestamp DESC"
                    .to_string(),
                vec![],
            ),
        };

        let mut stmt = self.conn.prepare(&sql)?;
        let rows = stmt.query_map(rusqlite::params_from_iter(param.iter()), |row| {
            Ok(Proposal {
                id: row.get(0)?,
                timestamp: row.get(1)?,
                skill: row.get(2)?,
                description: row.get(3)?,
                actions: row.get(4)?,
                status: row.get(5)?,
                decided_by: row.get(6)?,
                decided_at: row.get(7)?,
            })
        })?;
        rows.collect::<std::result::Result<Vec<_>, _>>()
            .map_err(super::MemoryError::from)
    }

    /// Approve a proposal.
    pub fn approve_proposal(&self, id: &str, decided_by: &str) -> super::Result<bool> {
        let now = super::now_rfc3339();
        let count = self.conn.execute(
            "UPDATE proposals SET status = 'approved', decided_by = ?1, decided_at = ?2 \
             WHERE id = ?3 AND status = 'pending'",
            params![decided_by, now, id],
        )?;
        Ok(count > 0)
    }

    /// Reject a proposal.
    pub fn reject_proposal(&self, id: &str, decided_by: &str) -> super::Result<bool> {
        let now = super::now_rfc3339();
        let count = self.conn.execute(
            "UPDATE proposals SET status = 'rejected', decided_by = ?1, decided_at = ?2 \
             WHERE id = ?3 AND status = 'pending'",
            params![decided_by, now, id],
        )?;
        Ok(count > 0)
    }

    // ── World State CRUD ──────────────────────────────────────────

    /// Upsert a world-state entry using the current time.
    pub fn upsert_world_state(
        &self,
        key: &str,
        value: &str,
        confidence: f64,
        source_topic: Option<&str>,
        source_node: Option<&str>,
        max_age_secs: i64,
    ) -> super::Result<()> {
        let now = super::now_epoch_secs() as i64;
        self.upsert_world_state_at(
            key,
            value,
            confidence,
            source_topic,
            source_node,
            max_age_secs,
            now,
        )
    }

    /// Upsert a world-state entry with an explicit timestamp (for testing staleness).
    #[allow(clippy::too_many_arguments)]
    pub fn upsert_world_state_at(
        &self,
        key: &str,
        value: &str,
        confidence: f64,
        source_topic: Option<&str>,
        source_node: Option<&str>,
        max_age_secs: i64,
        last_seen_at: i64,
    ) -> super::Result<()> {
        self.conn.execute(
            "INSERT INTO world_state (key, value, confidence, source_topic, source_node, last_seen_at, max_age_secs, created_at)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?6)
             ON CONFLICT(key) DO UPDATE SET
                value = excluded.value,
                confidence = excluded.confidence,
                source_topic = excluded.source_topic,
                source_node = excluded.source_node,
                last_seen_at = excluded.last_seen_at,
                max_age_secs = excluded.max_age_secs",
            params![key, value, confidence, source_topic, source_node, last_seen_at, max_age_secs],
        )?;
        Ok(())
    }

    /// Read the full world-state snapshot. Marks entries stale where `now - last_seen_at > max_age_secs`.
    pub fn world_state_snapshot(&self) -> super::Result<Vec<WorldStateEntry>> {
        let now = super::now_epoch_secs() as i64;
        let mut stmt = self.conn.prepare(
            "SELECT key, value, confidence, source_topic, source_node, last_seen_at, max_age_secs \
             FROM world_state ORDER BY key ASC",
        )?;
        let rows = stmt.query_map([], |row| {
            let last_seen_at: i64 = row.get(5)?;
            let max_age_secs: i64 = row.get(6)?;
            let stale = (now - last_seen_at) > max_age_secs;
            Ok(WorldStateEntry {
                key: row.get(0)?,
                value: row.get(1)?,
                confidence: row.get(2)?,
                source_topic: row.get(3)?,
                source_node: row.get(4)?,
                last_seen_at,
                max_age_secs,
                stale,
            })
        })?;
        rows.collect::<std::result::Result<Vec<_>, _>>()
            .map_err(super::MemoryError::from)
    }

    /// Read the world-state snapshot excluding stale entries.
    ///
    /// Stale entries (`now - last_seen_at > max_age_secs`) are filtered out at query
    /// time. Reactive rule evaluation uses this to avoid firing predicates against
    /// expired data — which otherwise causes the exact runaway-alert loop we hit
    /// on 2026-04-10 when a synthetic motion value never aged out of the table.
    ///
    /// All returned entries have `stale == false` by construction, so callers do
    /// not need to re-check.
    pub fn world_state_snapshot_fresh(&self) -> super::Result<Vec<WorldStateEntry>> {
        let now = super::now_epoch_secs() as i64;
        let mut stmt = self.conn.prepare(
            "SELECT key, value, confidence, source_topic, source_node, last_seen_at, max_age_secs \
             FROM world_state \
             WHERE (?1 - last_seen_at) <= max_age_secs \
             ORDER BY key ASC",
        )?;
        let rows = stmt.query_map(params![now], |row| {
            Ok(WorldStateEntry {
                key: row.get(0)?,
                value: row.get(1)?,
                confidence: row.get(2)?,
                source_topic: row.get(3)?,
                source_node: row.get(4)?,
                last_seen_at: row.get(5)?,
                max_age_secs: row.get(6)?,
                stale: false,
            })
        })?;
        rows.collect::<std::result::Result<Vec<_>, _>>()
            .map_err(super::MemoryError::from)
    }

    /// Delete world-state entries where `now - last_seen_at > max_age_secs`.
    ///
    /// Used by the background sweeper (`daemon::world_state_sweeper`) to bound
    /// table growth. Returns the number of rows evicted. Safe to call concurrently
    /// with writers: SQLite serializes on the WAL.
    pub fn delete_stale_world_state(&self) -> super::Result<usize> {
        let now = super::now_epoch_secs() as i64;
        let n = self.conn.execute(
            "DELETE FROM world_state WHERE (?1 - last_seen_at) > max_age_secs",
            params![now],
        )?;
        Ok(n)
    }

    // ── Beliefs CRUD ─────────────────────────────────────────────

    /// Upsert a belief. On conflict (subject, predicate), updates value/confidence/source
    /// and increments confirmation_count.
    #[allow(clippy::too_many_arguments)]
    pub fn upsert_belief(
        &self,
        id: &str,
        subject: &str,
        predicate: &str,
        value: &str,
        confidence: f64,
        source: &str,
        notes: Option<&str>,
    ) -> super::Result<()> {
        let now = super::now_epoch_secs() as i64;
        self.conn.execute(
            "INSERT INTO beliefs (id, subject, predicate, value, confidence, source, first_observed, last_confirmed, confirmation_count, contradiction_count, notes)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?7, 1, 0, ?8)
             ON CONFLICT(subject, predicate) DO UPDATE SET
                value = excluded.value,
                confidence = excluded.confidence,
                source = excluded.source,
                last_confirmed = excluded.last_confirmed,
                confirmation_count = confirmation_count + 1,
                notes = COALESCE(excluded.notes, notes)",
            params![id, subject, predicate, value, confidence, source, now, notes],
        )?;
        Ok(())
    }

    /// Confirm a belief: bump confidence by +0.05 (capped at 1.0), increment confirmation_count.
    pub fn confirm_belief(&self, subject: &str, predicate: &str) -> super::Result<bool> {
        let now = super::now_epoch_secs() as i64;
        let count = self.conn.execute(
            "UPDATE beliefs SET confidence = MIN(1.0, confidence + 0.05), \
             confirmation_count = confirmation_count + 1, last_confirmed = ?1 \
             WHERE subject = ?2 AND predicate = ?3",
            params![now, subject, predicate],
        )?;
        Ok(count > 0)
    }

    /// Contradict a belief: reduce confidence by -0.15 (floored at 0.0), increment contradiction_count.
    pub fn contradict_belief(&self, subject: &str, predicate: &str) -> super::Result<bool> {
        let count = self.conn.execute(
            "UPDATE beliefs SET confidence = MAX(0.0, confidence - 0.15), \
             contradiction_count = contradiction_count + 1 \
             WHERE subject = ?1 AND predicate = ?2",
            params![subject, predicate],
        )?;
        Ok(count > 0)
    }

    /// Get a single belief by subject + predicate.
    pub fn get_belief(&self, subject: &str, predicate: &str) -> super::Result<Option<Belief>> {
        let mut stmt = self.conn.prepare(
            "SELECT id, subject, predicate, value, confidence, source, first_observed, last_confirmed, \
             confirmation_count, contradiction_count, notes \
             FROM beliefs WHERE subject = ?1 AND predicate = ?2",
        )?;
        let mut rows = stmt.query_map(params![subject, predicate], |row| {
            Ok(Belief {
                id: row.get(0)?,
                subject: row.get(1)?,
                predicate: row.get(2)?,
                value: row.get(3)?,
                confidence: row.get(4)?,
                source: row.get(5)?,
                first_observed: row.get(6)?,
                last_confirmed: row.get(7)?,
                confirmation_count: row.get(8)?,
                contradiction_count: row.get(9)?,
                notes: row.get(10)?,
            })
        })?;
        match rows.next() {
            Some(Ok(b)) => Ok(Some(b)),
            Some(Err(e)) => Err(super::MemoryError::from(e)),
            None => Ok(None),
        }
    }

    /// List all beliefs.
    pub fn list_beliefs(&self) -> super::Result<Vec<Belief>> {
        let mut stmt = self.conn.prepare(
            "SELECT id, subject, predicate, value, confidence, source, first_observed, last_confirmed, \
             confirmation_count, contradiction_count, notes \
             FROM beliefs ORDER BY subject ASC, predicate ASC",
        )?;
        let rows = stmt.query_map([], |row| {
            Ok(Belief {
                id: row.get(0)?,
                subject: row.get(1)?,
                predicate: row.get(2)?,
                value: row.get(3)?,
                confidence: row.get(4)?,
                source: row.get(5)?,
                first_observed: row.get(6)?,
                last_confirmed: row.get(7)?,
                confirmation_count: row.get(8)?,
                contradiction_count: row.get(9)?,
                notes: row.get(10)?,
            })
        })?;
        rows.collect::<std::result::Result<Vec<_>, _>>()
            .map_err(super::MemoryError::from)
    }

    /// Decay all belief confidences by a factor (e.g. 0.95 reduces by 5%).
    /// Returns number of rows updated.
    pub fn decay_beliefs(&self, decay_factor: f64) -> super::Result<u32> {
        let count = self.conn.execute(
            "UPDATE beliefs SET confidence = MAX(0.0, confidence * ?1) WHERE confidence > 0.0",
            params![decay_factor],
        )?;
        Ok(count as u32)
    }

    /// Get a single proposal by ID.
    pub fn get_proposal(&self, id: &str) -> super::Result<Option<Proposal>> {
        let mut stmt = self.conn.prepare(
            "SELECT id, timestamp, skill, description, actions, status, decided_by, decided_at \
             FROM proposals WHERE id = ?1",
        )?;
        let mut rows = stmt.query_map(params![id], |row| {
            Ok(Proposal {
                id: row.get(0)?,
                timestamp: row.get(1)?,
                skill: row.get(2)?,
                description: row.get(3)?,
                actions: row.get(4)?,
                status: row.get(5)?,
                decided_by: row.get(6)?,
                decided_at: row.get(7)?,
            })
        })?;
        match rows.next() {
            Some(Ok(p)) => Ok(Some(p)),
            Some(Err(e)) => Err(super::MemoryError::from(e)),
            None => Ok(None),
        }
    }
}

/// A world-state entry (Tier 0: sensor-grounded state).
#[derive(Debug, Clone, serde::Serialize)]
pub struct WorldStateEntry {
    /// Unique key (e.g. "camera.front_door.status").
    pub key: String,
    /// Current value (JSON or plain text).
    pub value: String,
    /// Confidence in the value (0.0–1.0).
    pub confidence: f64,
    /// Zenoh topic that produced this value.
    pub source_topic: Option<String>,
    /// Node that produced this value.
    pub source_node: Option<String>,
    /// Epoch seconds when this value was last observed.
    pub last_seen_at: i64,
    /// Maximum age in seconds before the entry is considered stale.
    pub max_age_secs: i64,
    /// Whether this entry is stale (computed at read time, not stored).
    pub stale: bool,
}

/// A belief — durable assertion the agent holds about the world.
#[derive(Debug, Clone, serde::Serialize)]
pub struct Belief {
    /// Unique ID.
    pub id: String,
    /// Subject of the belief (e.g. "front_door_camera").
    pub subject: String,
    /// Predicate / relation (e.g. "is_reliable").
    pub predicate: String,
    /// Value (e.g. "true", "mostly", JSON).
    pub value: String,
    /// Confidence (0.0–1.0).
    pub confidence: f64,
    /// How this belief was formed (e.g. "observation", "user_told_me").
    pub source: String,
    /// Epoch seconds when first observed.
    pub first_observed: i64,
    /// Epoch seconds when last confirmed.
    pub last_confirmed: i64,
    /// How many times this belief has been confirmed.
    pub confirmation_count: i32,
    /// How many times this belief has been contradicted.
    pub contradiction_count: i32,
    /// Free-form notes.
    pub notes: Option<String>,
}

/// Outcome of a job failure.
#[derive(Debug)]
pub enum FailureOutcome {
    /// Job will be retried after backoff delay.
    Retrying { retry_count: i32, next_run_at: u64 },
    /// Job permanently dead-lettered after exhausting retries.
    DeadLettered { retry_count: i32 },
}

#[cfg(test)]
mod tests {
    use super::*;

    fn test_store() -> (SemanticStore, tempfile::TempDir) {
        let dir = tempfile::tempdir().unwrap();
        let store = SemanticStore::open(&dir.path().join("test.db")).unwrap();
        (store, dir)
    }

    #[test]
    fn open_creates_tables() {
        let (store, _dir) = test_store();
        // Should not error
        store.pending_jobs().unwrap();
        store.list_proposals(None).unwrap();
    }

    #[test]
    fn open_twice_idempotent() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("test.db");
        SemanticStore::open(&path).unwrap();
        SemanticStore::open(&path).unwrap();
    }

    #[test]
    fn job_lifecycle() {
        let (store, _dir) = test_store();
        let job = Job {
            id: "job-1".to_string(),
            cron_schedule: None,
            next_run_at: 0, // due immediately
            prompt_payload: "check camera health".to_string(),
            status: "pending".to_string(),
            recurrence: false,
            retry_count: 0,
            last_error: None,
        };
        store.create_job(&job).unwrap();

        let pending = store.pending_jobs().unwrap();
        assert_eq!(pending.len(), 1);
        assert_eq!(pending[0].id, "job-1");

        store.start_job("job-1").unwrap();

        // After starting, it shouldn't appear in pending
        let pending = store.pending_jobs().unwrap();
        assert!(pending.is_empty());

        store.complete_job("job-1", None).unwrap();
    }

    #[test]
    fn job_recurring_completion() {
        let (store, _dir) = test_store();
        let job = Job {
            id: "job-2".to_string(),
            cron_schedule: Some("*/15 * * * *".to_string()),
            next_run_at: 0,
            prompt_payload: "health patrol".to_string(),
            status: "pending".to_string(),
            recurrence: true,
            retry_count: 0,
            last_error: None,
        };
        store.create_job(&job).unwrap();
        store.start_job("job-2").unwrap();

        // Complete with next run
        store
            .complete_job("job-2", Some(9_999_999_999_i64))
            .unwrap();

        // Should still exist in DB with pending status and future run time
        // (won't appear in pending_jobs since next_run_at > now)
        let pending = store.pending_jobs().unwrap();
        assert!(pending.is_empty());
    }

    #[test]
    fn job_failure_retry() {
        let (store, _dir) = test_store();
        let job = Job {
            id: "job-3".to_string(),
            cron_schedule: None,
            next_run_at: 0,
            prompt_payload: "failing task".to_string(),
            status: "pending".to_string(),
            recurrence: false,
            retry_count: 0,
            last_error: None,
        };
        store.create_job(&job).unwrap();
        store.start_job("job-3").unwrap();

        // First failure — should retry
        let outcome = store.fail_job("job-3", "connection timeout", 3).unwrap();
        match outcome {
            FailureOutcome::Retrying { retry_count, .. } => {
                assert_eq!(retry_count, 1);
            }
            _ => panic!("expected Retrying"),
        }
    }

    #[test]
    fn job_failure_circuit_breaker() {
        let (store, _dir) = test_store();
        let job = Job {
            id: "job-4".to_string(),
            cron_schedule: None,
            next_run_at: 0,
            prompt_payload: "always fails".to_string(),
            status: "pending".to_string(),
            recurrence: false,
            retry_count: 0,
            last_error: None,
        };
        store.create_job(&job).unwrap();
        store.start_job("job-4").unwrap();

        // Fail 3 times (max_retries = 3)
        store.fail_job("job-4", "error 1", 3).unwrap(); // retry_count = 1
        store.fail_job("job-4", "error 2", 3).unwrap(); // retry_count = 2
        let outcome = store.fail_job("job-4", "error 3", 3).unwrap(); // retry_count = 3

        match outcome {
            FailureOutcome::DeadLettered { retry_count } => {
                assert_eq!(retry_count, 3);
            }
            _ => panic!("expected DeadLettered"),
        }
    }

    #[test]
    fn job_failure_backoff_30s_base() {
        let (store, _dir) = test_store();
        let job = Job {
            id: "job-backoff".to_string(),
            cron_schedule: None,
            next_run_at: 0,
            prompt_payload: "backoff test".to_string(),
            status: "pending".to_string(),
            recurrence: false,
            retry_count: 0,
            last_error: None,
        };
        store.create_job(&job).unwrap();
        store.start_job("job-backoff").unwrap();

        let before = crate::agent::memory::now_epoch_secs();
        let outcome = store.fail_job("job-backoff", "timeout", 3).unwrap();

        match outcome {
            FailureOutcome::Retrying {
                retry_count,
                next_run_at,
            } => {
                assert_eq!(retry_count, 1);
                // 30s * 2^1 = 60s backoff
                let expected_min = before + 60;
                assert!(
                    next_run_at >= expected_min,
                    "backoff should be at least 60s, got {}s from now",
                    next_run_at.saturating_sub(before)
                );
            }
            _ => panic!("expected Retrying"),
        }
    }

    #[test]
    fn job_dead_letter_status_in_db() {
        let (store, _dir) = test_store();
        let job = Job {
            id: "job-dl".to_string(),
            cron_schedule: None,
            next_run_at: 0,
            prompt_payload: "dead letter test".to_string(),
            status: "pending".to_string(),
            recurrence: false,
            retry_count: 0,
            last_error: None,
        };
        store.create_job(&job).unwrap();
        store.start_job("job-dl").unwrap();

        // Exhaust retries (max_retries = 1)
        store.fail_job("job-dl", "fatal error", 1).unwrap();

        // Verify the job has dead_letter status in DB
        let jobs = store.list_jobs(Some("dead_letter")).unwrap();
        assert_eq!(jobs.len(), 1);
        assert_eq!(jobs[0].id, "job-dl");
        assert_eq!(jobs[0].last_error.as_deref(), Some("fatal error"));
    }

    #[test]
    fn job_delete() {
        let (store, _dir) = test_store();
        let job = Job {
            id: "job-5".to_string(),
            cron_schedule: None,
            next_run_at: 0,
            prompt_payload: "temp task".to_string(),
            status: "pending".to_string(),
            recurrence: false,
            retry_count: 0,
            last_error: None,
        };
        store.create_job(&job).unwrap();
        store.delete_job("job-5").unwrap();
        assert!(store.pending_jobs().unwrap().is_empty());
    }

    #[test]
    fn proposal_lifecycle() {
        let (store, _dir) = test_store();
        let proposal = Proposal {
            id: "prop-1".to_string(),
            timestamp: "2026-03-03T10:00:00Z".to_string(),
            skill: "restart_node".to_string(),
            description: "Restart front-door camera (offline for 5 minutes)".to_string(),
            actions: r#"[{"tool":"restart_node","args":{"node_name":"rtsp-camera"}}]"#.to_string(),
            status: "pending".to_string(),
            decided_by: None,
            decided_at: None,
        };
        store.create_proposal(&proposal).unwrap();

        let pending = store.list_proposals(Some("pending")).unwrap();
        assert_eq!(pending.len(), 1);
        assert_eq!(pending[0].skill, "restart_node");

        store.approve_proposal("prop-1", "user").unwrap();

        let pending = store.list_proposals(Some("pending")).unwrap();
        assert!(pending.is_empty());

        let approved = store.list_proposals(Some("approved")).unwrap();
        assert_eq!(approved.len(), 1);
        assert_eq!(approved[0].decided_by.as_deref(), Some("user"));
    }

    #[test]
    fn proposal_reject() {
        let (store, _dir) = test_store();
        let proposal = Proposal {
            id: "prop-2".to_string(),
            timestamp: "2026-03-03T10:00:00Z".to_string(),
            skill: "remove_node".to_string(),
            description: "Remove unused weather node".to_string(),
            actions: r#"[{"tool":"remove_node","args":{"node_name":"openmeteo"}}]"#.to_string(),
            status: "pending".to_string(),
            decided_by: None,
            decided_at: None,
        };
        store.create_proposal(&proposal).unwrap();
        store.reject_proposal("prop-2", "mcp").unwrap();

        let rejected = store.list_proposals(Some("rejected")).unwrap();
        assert_eq!(rejected.len(), 1);
        assert_eq!(rejected[0].decided_by.as_deref(), Some("mcp"));
    }

    #[test]
    fn approve_nonexistent_returns_false() {
        let (store, _dir) = test_store();
        let result = store.approve_proposal("nonexistent", "user").unwrap();
        assert!(!result);
    }

    #[test]
    fn approve_already_approved_returns_false() {
        let (store, _dir) = test_store();
        let proposal = Proposal {
            id: "prop-3".to_string(),
            timestamp: "2026-03-03T10:00:00Z".to_string(),
            skill: "test".to_string(),
            description: "test".to_string(),
            actions: "[]".to_string(),
            status: "pending".to_string(),
            decided_by: None,
            decided_at: None,
        };
        store.create_proposal(&proposal).unwrap();
        assert!(store.approve_proposal("prop-3", "user").unwrap());
        // Second approval should fail (already approved, not pending)
        assert!(!store.approve_proposal("prop-3", "user").unwrap());
    }

    #[test]
    fn get_proposal() {
        let (store, _dir) = test_store();
        let proposal = Proposal {
            id: "prop-4".to_string(),
            timestamp: "2026-03-03T10:00:00Z".to_string(),
            skill: "test_skill".to_string(),
            description: "A test proposal".to_string(),
            actions: "[]".to_string(),
            status: "pending".to_string(),
            decided_by: None,
            decided_at: None,
        };
        store.create_proposal(&proposal).unwrap();

        let found = store.get_proposal("prop-4").unwrap().unwrap();
        assert_eq!(found.skill, "test_skill");

        assert!(store.get_proposal("nonexistent").unwrap().is_none());
    }

    #[test]
    fn list_proposals_all() {
        let (store, _dir) = test_store();
        for i in 0..3 {
            store
                .create_proposal(&Proposal {
                    id: format!("prop-{}", i),
                    timestamp: "2026-03-03T10:00:00Z".to_string(),
                    skill: "test".to_string(),
                    description: format!("proposal {}", i),
                    actions: "[]".to_string(),
                    status: "pending".to_string(),
                    decided_by: None,
                    decided_at: None,
                })
                .unwrap();
        }
        store.approve_proposal("prop-0", "user").unwrap();

        let all = store.list_proposals(None).unwrap();
        assert_eq!(all.len(), 3);
    }

    #[test]
    fn list_jobs_all() {
        let (store, _dir) = test_store();
        for i in 0..3 {
            store
                .create_job(&Job {
                    id: format!("job-{}", i),
                    cron_schedule: None,
                    next_run_at: 0,
                    prompt_payload: format!("task {}", i),
                    status: if i == 0 { "completed" } else { "pending" }.to_string(),
                    recurrence: false,
                    retry_count: 0,
                    last_error: None,
                })
                .unwrap();
        }
        let all = store.list_jobs(None).unwrap();
        assert_eq!(all.len(), 3);
    }

    #[test]
    fn list_jobs_filtered() {
        let (store, _dir) = test_store();
        store
            .create_job(&Job {
                id: "j-1".to_string(),
                cron_schedule: None,
                next_run_at: 0,
                prompt_payload: "done".to_string(),
                status: "completed".to_string(),
                recurrence: false,
                retry_count: 0,
                last_error: None,
            })
            .unwrap();
        store
            .create_job(&Job {
                id: "j-2".to_string(),
                cron_schedule: None,
                next_run_at: 0,
                prompt_payload: "waiting".to_string(),
                status: "pending".to_string(),
                recurrence: false,
                retry_count: 0,
                last_error: None,
            })
            .unwrap();

        let pending = store.list_jobs(Some("pending")).unwrap();
        assert_eq!(pending.len(), 1);
        assert_eq!(pending[0].id, "j-2");

        let completed = store.list_jobs(Some("completed")).unwrap();
        assert_eq!(completed.len(), 1);
        assert_eq!(completed[0].id, "j-1");
    }

    #[test]
    fn delete_proposal_removes_it() {
        let (store, _dir) = test_store();
        store
            .create_proposal(&Proposal {
                id: "dp-1".to_string(),
                timestamp: "2026-03-03T10:00:00Z".to_string(),
                skill: "test".to_string(),
                description: "test".to_string(),
                actions: "[]".to_string(),
                status: "pending".to_string(),
                decided_by: None,
                decided_at: None,
            })
            .unwrap();
        assert!(store.get_proposal("dp-1").unwrap().is_some());
        store.delete_proposal("dp-1").unwrap();
        assert!(store.get_proposal("dp-1").unwrap().is_none());
    }

    #[test]
    fn prune_completed_jobs_skips_pending() {
        let (store, _dir) = test_store();
        // Old completed job
        store
            .create_job(&Job {
                id: "old-done".to_string(),
                cron_schedule: None,
                next_run_at: 0,
                prompt_payload: "done".to_string(),
                status: "completed".to_string(),
                recurrence: false,
                retry_count: 0,
                last_error: None,
            })
            .unwrap();
        // Pending job (should NOT be pruned)
        store
            .create_job(&Job {
                id: "still-pending".to_string(),
                cron_schedule: None,
                next_run_at: 0,
                prompt_payload: "waiting".to_string(),
                status: "pending".to_string(),
                recurrence: false,
                retry_count: 0,
                last_error: None,
            })
            .unwrap();

        let pruned = store.prune_completed_jobs(1).unwrap();
        assert_eq!(pruned, 1);

        let all = store.list_jobs(None).unwrap();
        assert_eq!(all.len(), 1);
        assert_eq!(all[0].id, "still-pending");
    }

    #[test]
    fn world_state_table_exists_and_roundtrips() {
        let (store, _dir) = test_store();
        store
            .upsert_world_state(
                "cam.front.status",
                "online",
                0.95,
                Some("bubbaloop/cam"),
                Some("rtsp-camera"),
                300,
            )
            .unwrap();
        store
            .upsert_world_state("weather.temp", "22.5", 1.0, None, Some("openmeteo"), 600)
            .unwrap();

        let snapshot = store.world_state_snapshot().unwrap();
        assert_eq!(snapshot.len(), 2);

        let cam = snapshot
            .iter()
            .find(|e| e.key == "cam.front.status")
            .unwrap();
        assert_eq!(cam.value, "online");
        assert!((cam.confidence - 0.95).abs() < f64::EPSILON);
        assert_eq!(cam.source_topic.as_deref(), Some("bubbaloop/cam"));
        assert!(!cam.stale);

        // Upsert should update existing key
        store
            .upsert_world_state(
                "cam.front.status",
                "offline",
                0.8,
                Some("bubbaloop/cam"),
                Some("rtsp-camera"),
                300,
            )
            .unwrap();
        let snapshot = store.world_state_snapshot().unwrap();
        let cam = snapshot
            .iter()
            .find(|e| e.key == "cam.front.status")
            .unwrap();
        assert_eq!(cam.value, "offline");
    }

    #[test]
    fn world_state_staleness_flagged() {
        let (store, _dir) = test_store();
        let old_time = super::super::now_epoch_secs() as i64 - 600; // 10 minutes ago
        store
            .upsert_world_state_at("sensor.temp", "25.0", 1.0, None, None, 300, old_time)
            .unwrap();

        let snapshot = store.world_state_snapshot().unwrap();
        assert_eq!(snapshot.len(), 1);
        assert!(
            snapshot[0].stale,
            "entry 600s old with max_age 300s should be stale"
        );
    }

    #[test]
    fn world_state_snapshot_fresh_excludes_stale_entries() {
        let (store, _dir) = test_store();
        let now = super::super::now_epoch_secs() as i64;

        // Fresh: 60s old, max_age 300 → included
        store
            .upsert_world_state_at("sensor.fresh", "42.0", 1.0, None, None, 300, now - 60)
            .unwrap();
        // Stale: 600s old, max_age 300 → excluded
        store
            .upsert_world_state_at("sensor.stale", "99.0", 1.0, None, None, 300, now - 600)
            .unwrap();
        // Fresh with long max_age: 6h old, max_age 86400 → included
        store
            .upsert_world_state_at(
                "sensor.longlived",
                "7.0",
                1.0,
                None,
                None,
                86400,
                now - 21600,
            )
            .unwrap();

        // Control: regular snapshot returns all three (with stale flag on the expired one)
        let all = store.world_state_snapshot().unwrap();
        assert_eq!(all.len(), 3, "baseline snapshot should return all entries");

        // Fresh-only snapshot excludes sensor.stale
        let fresh = store.world_state_snapshot_fresh().unwrap();
        assert_eq!(fresh.len(), 2);
        let keys: Vec<&str> = fresh.iter().map(|e| e.key.as_str()).collect();
        assert!(keys.contains(&"sensor.fresh"));
        assert!(keys.contains(&"sensor.longlived"));
        assert!(!keys.contains(&"sensor.stale"));
        // Fresh entries are never flagged stale
        for entry in &fresh {
            assert!(!entry.stale, "fresh snapshot must not contain stale rows");
        }
    }

    #[test]
    fn delete_stale_world_state_evicts_expired_rows() {
        let (store, _dir) = test_store();
        let now = super::super::now_epoch_secs() as i64;

        store
            .upsert_world_state_at("k.fresh", "1", 1.0, None, None, 300, now - 60)
            .unwrap();
        store
            .upsert_world_state_at("k.expired1", "2", 1.0, None, None, 300, now - 600)
            .unwrap();
        store
            .upsert_world_state_at("k.expired2", "3", 1.0, None, None, 60, now - 120)
            .unwrap();

        let evicted = store.delete_stale_world_state().unwrap();
        assert_eq!(evicted, 2, "should evict both expired rows");

        let remaining = store.world_state_snapshot().unwrap();
        assert_eq!(remaining.len(), 1);
        assert_eq!(remaining[0].key, "k.fresh");
    }

    #[test]
    fn delete_stale_world_state_is_idempotent_on_empty() {
        let (store, _dir) = test_store();
        // No rows at all
        let evicted = store.delete_stale_world_state().unwrap();
        assert_eq!(evicted, 0);
        // Call again — still zero, still fine
        let evicted = store.delete_stale_world_state().unwrap();
        assert_eq!(evicted, 0);
    }

    #[test]
    fn delete_stale_world_state_leaves_all_fresh_rows_intact() {
        let (store, _dir) = test_store();
        let now = super::super::now_epoch_secs() as i64;
        store
            .upsert_world_state_at("k.a", "1", 1.0, None, None, 3600, now - 1)
            .unwrap();
        store
            .upsert_world_state_at("k.b", "2", 1.0, None, None, 3600, now - 1)
            .unwrap();

        let evicted = store.delete_stale_world_state().unwrap();
        assert_eq!(evicted, 0);
        assert_eq!(store.world_state_snapshot().unwrap().len(), 2);
    }

    #[test]
    fn belief_upsert_and_confirm() {
        let (store, _dir) = test_store();
        store
            .upsert_belief(
                "b1",
                "front_camera",
                "is_reliable",
                "true",
                0.8,
                "observation",
                None,
            )
            .unwrap();

        let belief = store
            .get_belief("front_camera", "is_reliable")
            .unwrap()
            .unwrap();
        assert_eq!(belief.value, "true");
        assert!((belief.confidence - 0.8).abs() < f64::EPSILON);
        assert_eq!(belief.confirmation_count, 1);
        assert_eq!(belief.contradiction_count, 0);

        // Confirm bumps confidence by 0.05
        store.confirm_belief("front_camera", "is_reliable").unwrap();
        let belief = store
            .get_belief("front_camera", "is_reliable")
            .unwrap()
            .unwrap();
        assert!((belief.confidence - 0.85).abs() < f64::EPSILON);
        assert_eq!(belief.confirmation_count, 2);

        // Upsert same (subject, predicate) increments confirmation_count
        store
            .upsert_belief(
                "b1-dup",
                "front_camera",
                "is_reliable",
                "still true",
                0.9,
                "re-observation",
                None,
            )
            .unwrap();
        let belief = store
            .get_belief("front_camera", "is_reliable")
            .unwrap()
            .unwrap();
        assert_eq!(belief.confirmation_count, 3);
        assert_eq!(belief.value, "still true");

        // List returns it
        let all = store.list_beliefs().unwrap();
        assert_eq!(all.len(), 1);
    }

    #[test]
    fn belief_contradiction_reduces_confidence() {
        let (store, _dir) = test_store();
        store
            .upsert_belief("b2", "sensor", "is_accurate", "yes", 0.5, "test", None)
            .unwrap();

        store.contradict_belief("sensor", "is_accurate").unwrap();
        let belief = store.get_belief("sensor", "is_accurate").unwrap().unwrap();
        assert!((belief.confidence - 0.35).abs() < f64::EPSILON);
        assert_eq!(belief.contradiction_count, 1);

        // Contradict again
        store.contradict_belief("sensor", "is_accurate").unwrap();
        let belief = store.get_belief("sensor", "is_accurate").unwrap().unwrap();
        assert!((belief.confidence - 0.20).abs() < f64::EPSILON);
        assert_eq!(belief.contradiction_count, 2);

        // Floor at 0.0
        store.contradict_belief("sensor", "is_accurate").unwrap(); // 0.05
        store.contradict_belief("sensor", "is_accurate").unwrap(); // would be -0.10 => 0.0
        let belief = store.get_belief("sensor", "is_accurate").unwrap().unwrap();
        assert!(belief.confidence >= 0.0);
        assert!(belief.confidence < f64::EPSILON);
    }

    #[test]
    fn belief_decay_reduces_confidence() {
        let (store, _dir) = test_store();
        store
            .upsert_belief("d1", "cam", "works", "yes", 1.0, "test", None)
            .unwrap();
        store
            .upsert_belief("d2", "sensor", "accurate", "yes", 0.5, "test", None)
            .unwrap();

        let updated = store.decay_beliefs(0.9).unwrap();
        assert_eq!(updated, 2);

        let b1 = store.get_belief("cam", "works").unwrap().unwrap();
        assert!((b1.confidence - 0.9).abs() < f64::EPSILON);

        let b2 = store.get_belief("sensor", "accurate").unwrap().unwrap();
        assert!((b2.confidence - 0.45).abs() < f64::EPSILON);
    }

    #[cfg(unix)]
    #[test]
    fn file_permissions_0600() {
        use std::os::unix::fs::PermissionsExt;
        let dir = tempfile::tempdir().unwrap();
        let db_path = dir.path().join("secure.db");
        SemanticStore::open(&db_path).unwrap();
        let perms = std::fs::metadata(&db_path).unwrap().permissions();
        assert_eq!(perms.mode() & 0o777, 0o600);
    }
}
