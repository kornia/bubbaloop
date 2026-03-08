//! Mission model — markdown files backed by SQLite state machine.
//!
//! Missions are the unit of agent work. Each mission is a markdown file
//! in `~/.bubbaloop/agents/{id}/missions/` that gets compiled into a
//! SQLite record with status tracking, expiry, resource locking, and
//! dependency DAG support.

use rusqlite::{params, Connection};
use std::collections::HashMap;
use std::path::Path;

/// Mission lifecycle states.
#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
pub enum MissionStatus {
    Active,
    Paused,
    Cancelled,
    Completed,
    Failed,
}

impl std::fmt::Display for MissionStatus {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            MissionStatus::Active => write!(f, "active"),
            MissionStatus::Paused => write!(f, "paused"),
            MissionStatus::Cancelled => write!(f, "cancelled"),
            MissionStatus::Completed => write!(f, "completed"),
            MissionStatus::Failed => write!(f, "failed"),
        }
    }
}

impl std::str::FromStr for MissionStatus {
    type Err = String;
    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s {
            "active" => Ok(MissionStatus::Active),
            "paused" => Ok(MissionStatus::Paused),
            "cancelled" => Ok(MissionStatus::Cancelled),
            "completed" => Ok(MissionStatus::Completed),
            "failed" => Ok(MissionStatus::Failed),
            _ => Err(format!(
                "Unknown mission status '{}' — must be active, paused, cancelled, completed, or failed",
                s
            )),
        }
    }
}

/// A mission — the unit of agent work.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct Mission {
    pub id: String,
    /// Raw markdown content.
    pub markdown: String,
    pub status: MissionStatus,
    /// Epoch secs, None = no expiry.
    pub expires_at: Option<i64>,
    /// Locked resources (JSON array in DB).
    pub resources: Vec<String>,
    /// Child mission IDs (JSON array in DB).
    pub sub_mission_ids: Vec<String>,
    /// Parent mission IDs (JSON array in DB).
    pub depends_on: Vec<String>,
    /// Epoch secs of last setup turn.
    pub compiled_at: i64,
}

/// SQLite-backed store for missions.
pub struct MissionStore {
    conn: Connection,
}

impl MissionStore {
    /// Open (or create) the mission store at the given path.
    pub fn open(path: &Path) -> anyhow::Result<Self> {
        let conn = crate::daemon::util::open_sqlite(path)?;

        conn.execute_batch(
            "CREATE TABLE IF NOT EXISTS missions (
                id              TEXT PRIMARY KEY,
                markdown        TEXT NOT NULL,
                status          TEXT NOT NULL DEFAULT 'active',
                expires_at      INTEGER,
                resources       TEXT NOT NULL DEFAULT '[]',
                sub_mission_ids TEXT NOT NULL DEFAULT '[]',
                depends_on      TEXT NOT NULL DEFAULT '[]',
                compiled_at     INTEGER NOT NULL
            );",
        )?;

        Ok(Self { conn })
    }

    /// Save (insert or replace) a mission.
    pub fn save_mission(&self, m: &Mission) -> anyhow::Result<()> {
        let resources_json = serde_json::to_string(&m.resources)?;
        let sub_mission_ids_json = serde_json::to_string(&m.sub_mission_ids)?;
        let depends_on_json = serde_json::to_string(&m.depends_on)?;

        self.conn.execute(
            "INSERT OR REPLACE INTO missions \
             (id, markdown, status, expires_at, resources, sub_mission_ids, depends_on, compiled_at) \
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8)",
            params![
                m.id,
                m.markdown,
                m.status.to_string(),
                m.expires_at,
                resources_json,
                sub_mission_ids_json,
                depends_on_json,
                m.compiled_at,
            ],
        )?;
        Ok(())
    }

    /// Get a mission by ID.
    pub fn get_mission(&self, id: &str) -> anyhow::Result<Option<Mission>> {
        let mut stmt = self.conn.prepare(
            "SELECT id, markdown, status, expires_at, resources, sub_mission_ids, depends_on, compiled_at \
             FROM missions WHERE id = ?1",
        )?;
        let mut rows = stmt.query_map(params![id], Self::row_to_mission)?;
        match rows.next() {
            Some(row) => Ok(Some(row??)),
            None => Ok(None),
        }
    }

    /// List all missions.
    pub fn list_missions(&self) -> anyhow::Result<Vec<Mission>> {
        let mut stmt = self.conn.prepare(
            "SELECT id, markdown, status, expires_at, resources, sub_mission_ids, depends_on, compiled_at \
             FROM missions ORDER BY id ASC",
        )?;
        let rows = stmt.query_map([], Self::row_to_mission)?;
        let mut missions = Vec::new();
        for row in rows {
            missions.push(row??);
        }
        Ok(missions)
    }

    /// Update the status of a mission.
    pub fn update_status(&self, id: &str, status: MissionStatus) -> anyhow::Result<()> {
        let rows = self.conn.execute(
            "UPDATE missions SET status = ?1 WHERE id = ?2",
            params![status.to_string(), id],
        )?;
        if rows == 0 {
            anyhow::bail!("Mission '{}' not found", id);
        }
        Ok(())
    }

    /// Find missions that have expired (expires_at in the past and still active).
    pub fn expired_missions(&self) -> anyhow::Result<Vec<Mission>> {
        let mut stmt = self.conn.prepare(
            "SELECT id, markdown, status, expires_at, resources, sub_mission_ids, depends_on, compiled_at \
             FROM missions \
             WHERE expires_at IS NOT NULL AND expires_at < strftime('%s','now') AND status = 'active'",
        )?;
        let rows = stmt.query_map([], Self::row_to_mission)?;
        let mut missions = Vec::new();
        for row in rows {
            missions.push(row??);
        }
        Ok(missions)
    }

    /// List active missions.
    pub fn active_missions(&self) -> anyhow::Result<Vec<Mission>> {
        let mut stmt = self.conn.prepare(
            "SELECT id, markdown, status, expires_at, resources, sub_mission_ids, depends_on, compiled_at \
             FROM missions WHERE status = 'active' ORDER BY id ASC",
        )?;
        let rows = stmt.query_map([], Self::row_to_mission)?;
        let mut missions = Vec::new();
        for row in rows {
            missions.push(row??);
        }
        Ok(missions)
    }

    /// Convert a SQLite row to a Mission, parsing JSON arrays.
    fn row_to_mission(row: &rusqlite::Row<'_>) -> rusqlite::Result<anyhow::Result<Mission>> {
        let id: String = row.get(0)?;
        let markdown: String = row.get(1)?;
        let status_str: String = row.get(2)?;
        let expires_at: Option<i64> = row.get(3)?;
        let resources_json: String = row.get(4)?;
        let sub_mission_ids_json: String = row.get(5)?;
        let depends_on_json: String = row.get(6)?;
        let compiled_at: i64 = row.get(7)?;

        Ok((|| -> anyhow::Result<Mission> {
            let status: MissionStatus =
                status_str.parse().map_err(|e: String| anyhow::anyhow!(e))?;
            let resources: Vec<String> = serde_json::from_str(&resources_json)?;
            let sub_mission_ids: Vec<String> = serde_json::from_str(&sub_mission_ids_json)?;
            let depends_on: Vec<String> = serde_json::from_str(&depends_on_json)?;

            Ok(Mission {
                id,
                markdown,
                status,
                expires_at,
                resources,
                sub_mission_ids,
                depends_on,
                compiled_at,
            })
        })())
    }
}

// ── DAG evaluator ──────────────────────────────────────────────────────

/// Evaluates the mission DAG to find missions that are ready to activate.
/// A mission is "ready" when all its `depends_on` missions are Completed.
/// Called on each daemon heartbeat tick. O(N) per tick using cached topo-sort.
pub struct DagEvaluator;

impl DagEvaluator {
    /// Returns missions that are Active and have all dependencies Completed.
    pub fn ready_missions(all_missions: &[Mission]) -> Vec<&Mission> {
        let completed_ids: std::collections::HashSet<&str> = all_missions
            .iter()
            .filter(|m| m.status == MissionStatus::Completed)
            .map(|m| m.id.as_str())
            .collect();

        all_missions
            .iter()
            .filter(|m| {
                m.status == MissionStatus::Active
                    && m.depends_on
                        .iter()
                        .all(|dep| completed_ids.contains(dep.as_str()))
            })
            .collect()
    }

    /// Returns missions whose expires_at has passed (for expiry sweeping).
    pub fn expired_missions(all_missions: &[Mission]) -> Vec<&Mission> {
        let now = crate::agent::memory::now_epoch_secs() as i64;
        all_missions
            .iter()
            .filter(|m| {
                m.expires_at.map(|exp| exp < now).unwrap_or(false)
                    && m.status == MissionStatus::Active
            })
            .collect()
    }
}

// ── Mission file watcher ────────────────────────────────────────────────

/// Poll a missions directory every 5 seconds for new/changed .md files.
/// Sends the mission ID (filename without .md) through setup_tx when detected.
/// No inotify dependency — simple polling for portability.
pub async fn watch_missions_dir(
    missions_dir: std::path::PathBuf,
    mut shutdown: tokio::sync::watch::Receiver<()>,
    setup_tx: tokio::sync::mpsc::Sender<String>,
) {
    let mut known: HashMap<String, std::time::SystemTime> = HashMap::new();
    let mut interval = tokio::time::interval(std::time::Duration::from_secs(5));

    // Consume the first immediate tick
    interval.tick().await;

    loop {
        tokio::select! {
            _ = interval.tick() => {
                let entries = match std::fs::read_dir(&missions_dir) {
                    Ok(e) => e,
                    Err(_) => continue,
                };

                for entry in entries.flatten() {
                    let path = entry.path();
                    if path.extension().and_then(|e| e.to_str()) != Some("md") {
                        continue;
                    }

                    let id = match path.file_stem().and_then(|s| s.to_str()) {
                        Some(s) => s.to_string(),
                        None => continue,
                    };

                    let modified = match path.metadata().and_then(|m| m.modified()) {
                        Ok(t) => t,
                        Err(_) => continue,
                    };

                    let is_new_or_changed = known
                        .get(&id)
                        .map(|prev| *prev != modified)
                        .unwrap_or(true);

                    if is_new_or_changed {
                        log::info!("[MissionWatcher] detected: {}", id);
                        known.insert(id.clone(), modified);
                        if setup_tx.send(id).await.is_err() {
                            return; // channel closed
                        }
                    }
                }
            }
            _ = shutdown.changed() => {
                log::info!("[MissionWatcher] shutting down");
                return;
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn mission_store_roundtrips() {
        let dir = tempfile::tempdir().unwrap();
        let store = MissionStore::open(&dir.path().join("missions.db")).unwrap();

        let mission = Mission {
            id: "dog-monitor".to_string(),
            markdown: "# Watch the dog\nKeep an eye on the kitchen.".to_string(),
            status: MissionStatus::Active,
            expires_at: None,
            resources: vec![],
            sub_mission_ids: vec![],
            depends_on: vec![],
            compiled_at: 1700000000,
        };

        store.save_mission(&mission).unwrap();
        let loaded = store.get_mission("dog-monitor").unwrap().unwrap();

        assert_eq!(loaded.id, "dog-monitor");
        assert_eq!(loaded.status, MissionStatus::Active);
        assert_eq!(loaded.compiled_at, 1700000000);
        assert!(loaded.markdown.contains("Watch the dog"));
    }

    #[test]
    fn mission_expiry_detection() {
        let dir = tempfile::tempdir().unwrap();
        let store = MissionStore::open(&dir.path().join("missions.db")).unwrap();

        // Expired mission (expires_at in the past)
        let mission = Mission {
            id: "expired-mission".to_string(),
            markdown: "Already done.".to_string(),
            status: MissionStatus::Active,
            expires_at: Some(1), // epoch 1 = way in the past
            resources: vec![],
            sub_mission_ids: vec![],
            depends_on: vec![],
            compiled_at: 1,
        };

        store.save_mission(&mission).unwrap();
        let expired = store.expired_missions().unwrap();

        assert_eq!(expired.len(), 1);
        assert_eq!(expired[0].id, "expired-mission");
    }

    #[test]
    fn mission_status_update() {
        let dir = tempfile::tempdir().unwrap();
        let store = MissionStore::open(&dir.path().join("missions.db")).unwrap();

        let mission = Mission {
            id: "pause-me".to_string(),
            markdown: "Pausable mission.".to_string(),
            status: MissionStatus::Active,
            expires_at: None,
            resources: vec![],
            sub_mission_ids: vec![],
            depends_on: vec![],
            compiled_at: 1700000000,
        };

        store.save_mission(&mission).unwrap();
        store
            .update_status("pause-me", MissionStatus::Paused)
            .unwrap();

        let loaded = store.get_mission("pause-me").unwrap().unwrap();
        assert_eq!(loaded.status, MissionStatus::Paused);
    }

    #[test]
    fn mission_json_array_fields() {
        let dir = tempfile::tempdir().unwrap();
        let store = MissionStore::open(&dir.path().join("missions.db")).unwrap();

        let mission = Mission {
            id: "complex-mission".to_string(),
            markdown: "Multi-resource mission.".to_string(),
            status: MissionStatus::Active,
            expires_at: Some(9999999999),
            resources: vec!["camera-1".to_string(), "speaker-1".to_string()],
            sub_mission_ids: vec!["sub-a".to_string(), "sub-b".to_string()],
            depends_on: vec!["parent-1".to_string()],
            compiled_at: 1700000000,
        };

        store.save_mission(&mission).unwrap();
        let loaded = store.get_mission("complex-mission").unwrap().unwrap();

        assert_eq!(loaded.resources, vec!["camera-1", "speaker-1"]);
        assert_eq!(loaded.sub_mission_ids, vec!["sub-a", "sub-b"]);
        assert_eq!(loaded.depends_on, vec!["parent-1"]);
    }

    #[test]
    fn mission_list_and_active() {
        let dir = tempfile::tempdir().unwrap();
        let store = MissionStore::open(&dir.path().join("missions.db")).unwrap();

        let m1 = Mission {
            id: "a-active".to_string(),
            markdown: "Active.".to_string(),
            status: MissionStatus::Active,
            expires_at: None,
            resources: vec![],
            sub_mission_ids: vec![],
            depends_on: vec![],
            compiled_at: 1,
        };
        let m2 = Mission {
            id: "b-paused".to_string(),
            markdown: "Paused.".to_string(),
            status: MissionStatus::Paused,
            expires_at: None,
            resources: vec![],
            sub_mission_ids: vec![],
            depends_on: vec![],
            compiled_at: 2,
        };

        store.save_mission(&m1).unwrap();
        store.save_mission(&m2).unwrap();

        let all = store.list_missions().unwrap();
        assert_eq!(all.len(), 2);

        let active = store.active_missions().unwrap();
        assert_eq!(active.len(), 1);
        assert_eq!(active[0].id, "a-active");
    }

    #[test]
    fn mission_status_display_and_parse() {
        for (status, label) in [
            (MissionStatus::Active, "active"),
            (MissionStatus::Paused, "paused"),
            (MissionStatus::Cancelled, "cancelled"),
            (MissionStatus::Completed, "completed"),
            (MissionStatus::Failed, "failed"),
        ] {
            assert_eq!(status.to_string(), label);
            assert_eq!(label.parse::<MissionStatus>().unwrap(), status);
        }
        assert!("bogus".parse::<MissionStatus>().is_err());
    }

    /// Helper: create a default Mission for testing.
    fn test_mission(id: &str) -> Mission {
        Mission {
            id: id.to_string(),
            markdown: format!("# {}", id),
            status: MissionStatus::Active,
            expires_at: None,
            resources: vec![],
            sub_mission_ids: vec![],
            depends_on: vec![],
            compiled_at: 1700000000,
        }
    }

    #[test]
    fn sub_mission_activates_when_no_depends_on() {
        let missions = vec![
            Mission {
                id: "root".into(),
                status: MissionStatus::Active,
                depends_on: vec![],
                ..test_mission("root")
            },
            Mission {
                id: "child".into(),
                status: MissionStatus::Active,
                depends_on: vec![],
                ..test_mission("child")
            },
        ];
        let ready = DagEvaluator::ready_missions(&missions);
        assert_eq!(ready.len(), 2);
    }

    #[test]
    fn sub_mission_waits_for_dependency_completion() {
        let missions = vec![
            Mission {
                id: "step-1".into(),
                status: MissionStatus::Active,
                depends_on: vec![],
                ..test_mission("step-1")
            },
            Mission {
                id: "step-2".into(),
                status: MissionStatus::Active,
                depends_on: vec!["step-1".into()],
                ..test_mission("step-2")
            },
        ];
        // step-1 is Active (not Completed) -> step-2 should NOT be in ready
        let ready = DagEvaluator::ready_missions(&missions);
        assert_eq!(ready.len(), 1);
        assert_eq!(ready[0].id, "step-1");
    }

    #[test]
    fn dag_evaluator_returns_ready_when_dependency_completed() {
        let missions = vec![
            Mission {
                id: "dep".into(),
                status: MissionStatus::Completed,
                depends_on: vec![],
                ..test_mission("dep")
            },
            Mission {
                id: "next".into(),
                status: MissionStatus::Active,
                depends_on: vec!["dep".into()],
                ..test_mission("next")
            },
        ];
        let ready = DagEvaluator::ready_missions(&missions);
        assert_eq!(ready.len(), 1);
        assert_eq!(ready[0].id, "next");
    }

    #[test]
    fn dag_evaluator_expired_missions() {
        let missions = vec![
            Mission {
                expires_at: Some(1), // epoch 1 = way in the past
                ..test_mission("old")
            },
            Mission {
                expires_at: Some(9999999999),
                ..test_mission("future")
            },
            Mission {
                expires_at: None,
                ..test_mission("no-expiry")
            },
        ];
        let expired = DagEvaluator::expired_missions(&missions);
        assert_eq!(expired.len(), 1);
        assert_eq!(expired[0].id, "old");
    }

    #[test]
    fn dag_evaluator_expired_skips_non_active() {
        let missions = vec![Mission {
            expires_at: Some(1),
            status: MissionStatus::Completed,
            ..test_mission("done")
        }];
        let expired = DagEvaluator::expired_missions(&missions);
        assert!(expired.is_empty());
    }

    #[test]
    fn mission_update_status_not_found() {
        let dir = tempfile::tempdir().unwrap();
        let store = MissionStore::open(&dir.path().join("missions.db")).unwrap();
        let result = store.update_status("nonexistent", MissionStatus::Paused);
        assert!(result.is_err());
    }

    #[tokio::test]
    async fn mission_watcher_detects_new_file() {
        let dir = tempfile::tempdir().unwrap();
        let (tx, mut rx) = tokio::sync::mpsc::channel(4);
        let (shutdown_tx, shutdown_rx) = tokio::sync::watch::channel(());

        let dir_path = dir.path().to_path_buf();
        tokio::spawn(watch_missions_dir(dir_path.clone(), shutdown_rx, tx));

        // Give watcher time to do first scan (empty)
        tokio::time::sleep(std::time::Duration::from_millis(200)).await;

        // Create a mission file
        std::fs::write(dir_path.join("dog-monitor.md"), "Watch the kitchen.").unwrap();

        // Wait up to 6 seconds for detection (watcher polls every 5s)
        let mission_id = tokio::time::timeout(std::time::Duration::from_secs(6), rx.recv())
            .await
            .expect("timeout")
            .expect("channel closed");

        assert_eq!(mission_id, "dog-monitor");
        drop(shutdown_tx);
    }
}
