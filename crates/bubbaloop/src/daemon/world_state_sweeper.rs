//! Background eviction sweeper for the agent world-state table.
//!
//! Without this, stale context-provider values sit in the `world_state`
//! table forever — even after `max_age_secs` has elapsed — because the
//! table is only written on ingestion, never swept. On 2026-04-10 a single
//! synthetic motion value from a one-shot Zenoh publish continued firing
//! reactive rules for ~17 hours, pinning the Jetson's CPU until it was
//! manually deleted.
//!
//! This sweeper runs on a fixed cadence (default 30s) and deletes any
//! row where `now - last_seen_at > max_age_secs`. Runs per-agent because
//! each agent has its own `memory.db`.
//!
//! Defence-in-depth: `SemanticStore::world_state_snapshot_fresh()` also
//! filters stale rows at read time so reactive evaluation never sees them
//! even between sweeps. The sweeper bounds storage growth; the read-time
//! filter bounds behaviour. Both matter.

use crate::agent::memory::semantic::SemanticStore;
use std::path::PathBuf;
use std::time::Duration;

/// How often the sweeper runs. Short enough that a stuck value can't
/// drive sustained behaviour, long enough that the write load is negligible.
pub const SWEEP_INTERVAL: Duration = Duration::from_secs(30);

/// Spawn a background task that periodically evicts stale world-state rows.
///
/// Opens its own `SemanticStore` connection — SQLite WAL mode allows
/// concurrent readers/writers from multiple connections on the same file,
/// so this does not conflict with the agent loop or context providers.
///
/// Shutdown-aware: breaks out of the loop when `shutdown` fires.
pub fn spawn_world_state_sweeper(
    agent_id: String,
    db_path: PathBuf,
    mut shutdown: tokio::sync::watch::Receiver<()>,
) -> tokio::task::JoinHandle<()> {
    tokio::spawn(async move {
        let store = match tokio::task::block_in_place(|| SemanticStore::open(&db_path)) {
            Ok(s) => s,
            Err(e) => {
                log::error!(
                    "[WorldStateSweeper:{}] failed to open store: {}",
                    agent_id,
                    e
                );
                return;
            }
        };
        log::info!(
            "[WorldStateSweeper:{}] running every {:?}",
            agent_id,
            SWEEP_INTERVAL
        );
        let mut interval = tokio::time::interval(SWEEP_INTERVAL);
        // Skip the immediate first tick so startup doesn't race with
        // context provider population.
        interval.tick().await;

        loop {
            tokio::select! {
                _ = shutdown.changed() => {
                    log::info!("[WorldStateSweeper:{}] shutting down", agent_id);
                    break;
                }
                _ = interval.tick() => {
                    match tokio::task::block_in_place(|| store.delete_stale_world_state()) {
                        Ok(0) => log::debug!("[WorldStateSweeper:{}] 0 rows evicted", agent_id),
                        Ok(n) => log::info!(
                            "[WorldStateSweeper:{}] evicted {} stale world_state row(s)",
                            agent_id,
                            n
                        ),
                        Err(e) => log::warn!(
                            "[WorldStateSweeper:{}] eviction failed: {}",
                            agent_id,
                            e
                        ),
                    }
                }
            }
        }
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::agent::memory::now_epoch_secs;

    #[tokio::test(flavor = "multi_thread")]
    async fn sweeper_evicts_stale_rows_on_tick() {
        let dir = tempfile::tempdir().unwrap();
        let db_path = dir.path().join("memory.db");

        // Seed with one fresh and one stale row
        {
            let store = SemanticStore::open(&db_path).unwrap();
            let now = now_epoch_secs() as i64;
            store
                .upsert_world_state_at("k.fresh", "1", 1.0, None, None, 300, now - 10)
                .unwrap();
            store
                .upsert_world_state_at("k.stale", "2", 1.0, None, None, 60, now - 600)
                .unwrap();
            assert_eq!(store.world_state_snapshot().unwrap().len(), 2);
        }

        // Run the eviction directly (rather than waiting 30s for the real sweeper)
        // This exercises the same code path.
        let store = SemanticStore::open(&db_path).unwrap();
        let evicted = store.delete_stale_world_state().unwrap();
        assert_eq!(evicted, 1);

        let remaining = store.world_state_snapshot().unwrap();
        assert_eq!(remaining.len(), 1);
        assert_eq!(remaining[0].key, "k.fresh");
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn sweeper_shuts_down_on_signal() {
        let dir = tempfile::tempdir().unwrap();
        let db_path = dir.path().join("memory.db");
        // Need the table to exist before spawning, else the sweeper fails to open
        let _ = SemanticStore::open(&db_path).unwrap();

        let (tx, rx) = tokio::sync::watch::channel(());
        let handle = spawn_world_state_sweeper("test-agent".to_string(), db_path, rx);

        // Signal shutdown immediately
        drop(tx);

        // Task should exit within a short window (the first interval.tick() is skipped,
        // so the task is parked in tokio::select! waiting on either the next tick or shutdown).
        tokio::time::timeout(Duration::from_secs(2), handle)
            .await
            .expect("sweeper did not exit within 2s of shutdown")
            .expect("sweeper task panicked");
    }
}
