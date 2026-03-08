//! 3-tier memory model — short-term (RAM), episodic (NDJSON + FTS5), semantic (SQLite).
//!
//! - **Tier 1 (Short-term):** `Vec<Message>` for the active turn, cleared after job completion.
//! - **Tier 2 (Episodic):** NDJSON daily log files + FTS5 dual-write index.
//! - **Tier 3 (Semantic):** SQLite tables for jobs and proposals.

pub mod episodic;
pub mod semantic;

use crate::agent::provider::Message;
use chrono::SecondsFormat;
use std::path::Path;
use std::sync::Arc;

pub use semantic::{Belief, WorldStateEntry};

/// Errors from memory operations.
#[derive(Debug, thiserror::Error)]
pub enum MemoryError {
    #[error("SQLite error: {0}")]
    Sqlite(#[from] rusqlite::Error),
    #[error("IO error: {0}")]
    Io(#[from] std::io::Error),
    #[error("JSON error: {0}")]
    Json(#[from] serde_json::Error),
}

pub type Result<T> = std::result::Result<T, MemoryError>;

/// Backend storage wrapping episodic + semantic tiers.
/// Wrapped in `Arc<tokio::sync::Mutex<MemoryBackend>>` by the runtime
/// to make the `!Send` rusqlite::Connection safely accessible from async code.
pub struct MemoryBackend {
    /// Tier 2: Episodic log (NDJSON + FTS5).
    pub episodic: episodic::EpisodicLog,
    /// Tier 3: Semantic store (SQLite jobs + proposals).
    pub semantic: semantic::SemanticStore,
}

impl MemoryBackend {
    /// Read the full world-state snapshot (delegates to semantic store).
    pub fn world_state_snapshot(&self) -> Result<Vec<WorldStateEntry>> {
        self.semantic.world_state_snapshot()
    }

    /// List all beliefs (delegates to semantic store).
    pub fn list_beliefs(&self) -> Result<Vec<Belief>> {
        self.semantic.list_beliefs()
    }
}

/// Unified memory facade combining all three tiers.
pub struct Memory {
    /// Tier 1: Short-term conversation messages (RAM).
    pub short_term: Vec<Message>,
    /// Tier 2+3: Episodic + semantic backend behind Arc<Mutex>.
    pub backend: Arc<tokio::sync::Mutex<MemoryBackend>>,
}

impl Memory {
    /// Open (or create) the memory system at the given base directory.
    ///
    /// - Episodic logs go to `{base}/memory/` (NDJSON files)
    /// - Semantic DB goes to `{base}/memory.db` (SQLite)
    /// - FTS5 index for episodic search is in the same SQLite DB
    pub fn open(base: &Path) -> Result<Self> {
        let log_dir = base.join("memory");
        let db_path = base.join("memory.db");

        let semantic = semantic::SemanticStore::open(&db_path)?;
        let episodic = episodic::EpisodicLog::new(&log_dir, &db_path)?;

        Ok(Self {
            short_term: Vec::new(),
            backend: Arc::new(tokio::sync::Mutex::new(MemoryBackend {
                episodic,
                semantic,
            })),
        })
    }

    /// Clear short-term memory (after job completion).
    pub fn clear_short_term(&mut self) {
        self.short_term.clear();
    }

    /// Run startup cleanup: prune old episodic logs and completed jobs.
    ///
    /// Called once after `Memory::open()` in the agent main loop.
    pub async fn startup_cleanup(&self, episodic_retention_days: u32) {
        let backend = self.backend.lock().await;
        match backend.episodic.prune_old_logs(episodic_retention_days) {
            Ok(0) => {}
            Ok(n) => log::info!("[Memory] Startup pruned {} old episodic log file(s)", n),
            Err(e) => log::warn!("[Memory] Startup episodic prune failed: {}", e),
        }

        match backend
            .semantic
            .prune_completed_jobs(episodic_retention_days)
        {
            Ok(0) => {}
            Ok(n) => log::info!("[Memory] Startup pruned {} completed/failed job(s)", n),
            Err(e) => log::warn!("[Memory] Startup semantic prune failed: {}", e),
        }
    }
}

/// Current epoch seconds (UTC).
pub fn now_epoch_secs() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs()
}

/// RFC 3339 timestamp for the current time (UTC, second precision).
pub fn now_rfc3339() -> String {
    chrono::Utc::now().to_rfc3339_opts(SecondsFormat::Secs, true)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn open_creates_structures() {
        let dir = tempfile::tempdir().unwrap();
        let mem = Memory::open(dir.path()).unwrap();
        assert!(mem.short_term.is_empty());
        assert!(dir.path().join("memory").exists());
        assert!(dir.path().join("memory.db").exists());
    }

    #[test]
    fn clear_short_term() {
        let dir = tempfile::tempdir().unwrap();
        let mut mem = Memory::open(dir.path()).unwrap();
        mem.short_term.push(Message::user("hello"));
        assert_eq!(mem.short_term.len(), 1);
        mem.clear_short_term();
        assert!(mem.short_term.is_empty());
    }

    #[tokio::test]
    async fn startup_cleanup_runs_without_error() {
        let dir = tempfile::tempdir().unwrap();
        let mem = Memory::open(dir.path()).unwrap();
        // Should not panic or error with empty memory
        mem.startup_cleanup(30).await;
        // 0 retention should also be fine (no-op)
        mem.startup_cleanup(0).await;
    }

    #[test]
    fn now_rfc3339_format() {
        let ts = now_rfc3339();
        assert!(ts.ends_with('Z'));
        assert_eq!(ts.len(), 20);
        chrono::DateTime::parse_from_rfc3339(&ts).expect("valid RFC 3339");
    }

    #[test]
    fn now_epoch_secs_is_reasonable() {
        let now = now_epoch_secs();
        assert!(now > 1_577_836_800); // after 2020
        assert!(now < 4_102_444_800); // before 2100
    }

    #[test]
    fn snapshot_includes_world_state_and_beliefs() {
        let dir = tempfile::tempdir().unwrap();
        let db_path = dir.path().join("test.db");
        let store = semantic::SemanticStore::open(&db_path).unwrap();

        // Insert world state
        store
            .upsert_world_state(
                "cam.status",
                "online",
                0.95,
                Some("topic/cam"),
                Some("rtsp"),
                300,
            )
            .unwrap();

        // Insert belief
        store
            .upsert_belief(
                "b1",
                "camera",
                "is_reliable",
                "true",
                0.8,
                "observation",
                None,
            )
            .unwrap();

        // Verify both appear in snapshots
        let ws = store.world_state_snapshot().unwrap();
        assert_eq!(ws.len(), 1);
        assert_eq!(ws[0].key, "cam.status");

        let beliefs = store.list_beliefs().unwrap();
        assert_eq!(beliefs.len(), 1);
        assert_eq!(beliefs[0].subject, "camera");
    }
}
