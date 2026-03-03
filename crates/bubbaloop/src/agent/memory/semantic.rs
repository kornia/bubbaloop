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
    /// Next scheduled run time (epoch seconds as string).
    pub next_run_at: String,
    /// The instruction to execute.
    pub prompt_payload: String,
    /// Status: pending, running, completed, failed, failed_requires_approval.
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

        conn.execute_batch(
            "CREATE TABLE IF NOT EXISTS jobs (
                id              TEXT PRIMARY KEY,
                cron_schedule   TEXT,
                next_run_at     TEXT NOT NULL,
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
            CREATE INDEX IF NOT EXISTS idx_proposals_status ON proposals(status);",
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
        let now = super::now_epoch_secs().to_string();
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
    pub fn complete_job(&self, id: &str, next_run_at: Option<&str>) -> super::Result<()> {
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
    /// If retry_count >= max_retries, sets status to `failed_requires_approval`.
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
            // Circuit breaker: park the job
            self.conn.execute(
                "UPDATE jobs SET status = 'failed_requires_approval', retry_count = ?1, last_error = ?2 WHERE id = ?3",
                params![new_count, error, id],
            )?;
            Ok(FailureOutcome::CircuitBroken {
                retry_count: new_count,
            })
        } else {
            // Exponential backoff: 10s * 2^retry_count (20s, 40s, 80s)
            let backoff_secs = 10u64 * 2u64.pow(new_count as u32);
            let next_run = super::now_epoch_secs() + backoff_secs;
            self.conn.execute(
                "UPDATE jobs SET status = 'pending', retry_count = ?1, last_error = ?2, next_run_at = ?3 WHERE id = ?4",
                params![new_count, error, next_run.to_string(), id],
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

    /// Delete completed/failed jobs older than `retention_days`.
    ///
    /// Only prunes jobs with status 'completed' or 'failed'. Pending/running jobs are kept.
    /// Returns the number of deleted jobs.
    pub fn prune_completed_jobs(&self, retention_days: u32) -> super::Result<usize> {
        if retention_days == 0 {
            return Ok(0);
        }
        let cutoff = super::now_epoch_secs().saturating_sub(u64::from(retention_days) * 86400);
        let count = self.conn.execute(
            "DELETE FROM jobs WHERE status IN ('completed', 'failed') AND next_run_at <= ?1",
            params![cutoff.to_string()],
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

/// Outcome of a job failure.
#[derive(Debug)]
pub enum FailureOutcome {
    /// Job will be retried after backoff delay.
    Retrying { retry_count: i32, next_run_at: u64 },
    /// Circuit breaker tripped — needs human approval.
    CircuitBroken { retry_count: i32 },
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
            next_run_at: "0".to_string(), // due immediately
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
            next_run_at: "0".to_string(),
            prompt_payload: "health patrol".to_string(),
            status: "pending".to_string(),
            recurrence: true,
            retry_count: 0,
            last_error: None,
        };
        store.create_job(&job).unwrap();
        store.start_job("job-2").unwrap();

        // Complete with next run
        store.complete_job("job-2", Some("9999999999")).unwrap();

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
            next_run_at: "0".to_string(),
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
            next_run_at: "0".to_string(),
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
            FailureOutcome::CircuitBroken { retry_count } => {
                assert_eq!(retry_count, 3);
            }
            _ => panic!("expected CircuitBroken"),
        }
    }

    #[test]
    fn job_delete() {
        let (store, _dir) = test_store();
        let job = Job {
            id: "job-5".to_string(),
            cron_schedule: None,
            next_run_at: "0".to_string(),
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
                    next_run_at: "0".to_string(),
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
                next_run_at: "0".to_string(),
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
                next_run_at: "0".to_string(),
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
                next_run_at: "0".to_string(),
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
                next_run_at: "0".to_string(),
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
