//! SQLite cold storage for telemetry snapshots.
//!
//! Batch-flushes ring buffer snapshots periodically and handles retention pruning.

use super::sampler::RingBuffer;
use super::types::{TelemetryConfig, TelemetrySnapshot};
use rusqlite::Connection;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use tokio::sync::RwLock;

/// Initialize the telemetry database, creating the table and index if needed.
pub fn init_db(path: &Path) -> Result<Connection, rusqlite::Error> {
    let conn = Connection::open(path)?;
    conn.execute_batch(
        "CREATE TABLE IF NOT EXISTS system_telemetry (
            timestamp_ms INTEGER NOT NULL,
            snapshot_json TEXT NOT NULL
        );
        CREATE INDEX IF NOT EXISTS idx_telemetry_ts ON system_telemetry(timestamp_ms);",
    )?;
    Ok(conn)
}

/// Insert a batch of snapshots into the database.
///
/// Uses `prepare_cached` for performance. Returns the number of rows inserted.
pub fn insert_batch(
    conn: &Connection,
    snapshots: &[TelemetrySnapshot],
) -> Result<usize, rusqlite::Error> {
    let mut count = 0;
    let mut stmt = conn.prepare_cached(
        "INSERT INTO system_telemetry (timestamp_ms, snapshot_json) VALUES (?1, ?2)",
    )?;
    for snap in snapshots {
        if let Ok(json) = serde_json::to_string(snap) {
            stmt.execute(rusqlite::params![snap.system.timestamp_ms, json])?;
            count += 1;
        }
    }
    Ok(count)
}

/// Delete rows older than `retention_days` days.
///
/// Returns the number of rows deleted.
pub fn prune(conn: &Connection, retention_days: u32) -> Result<usize, rusqlite::Error> {
    let cutoff_ms =
        chrono::Utc::now().timestamp_millis() - (retention_days as i64 * 24 * 60 * 60 * 1000);
    let deleted = conn.execute(
        "DELETE FROM system_telemetry WHERE timestamp_ms < ?1",
        rusqlite::params![cutoff_ms],
    )?;
    Ok(deleted)
}

/// Query snapshots within a time range, returning at most `max_points` results.
///
/// If the result set exceeds `max_points`, evenly-spaced downsampling is applied.
pub fn query_range(
    conn: &Connection,
    from_ms: i64,
    to_ms: i64,
    max_points: usize,
) -> Result<Vec<TelemetrySnapshot>, rusqlite::Error> {
    let mut stmt = conn.prepare_cached(
        "SELECT snapshot_json FROM system_telemetry
         WHERE timestamp_ms >= ?1 AND timestamp_ms <= ?2
         ORDER BY timestamp_ms ASC",
    )?;
    let rows: Vec<TelemetrySnapshot> = stmt
        .query_map(rusqlite::params![from_ms, to_ms], |row| {
            let json: String = row.get(0)?;
            Ok(json)
        })?
        .filter_map(|r| r.ok())
        .filter_map(|json| serde_json::from_str(&json).ok())
        .collect();

    if rows.len() <= max_points {
        return Ok(rows);
    }

    let len = rows.len();
    let step = len / max_points;
    let downsampled: Vec<TelemetrySnapshot> = (0..max_points)
        .map(|i| rows[(i * step).min(len - 1)].clone())
        .collect();

    Ok(downsampled)
}

/// Run the background storage flush loop.
///
/// Wakes every `flush_interval_secs`, collects new snapshots from the ring
/// buffer (skipping already-flushed entries), inserts them into SQLite, and
/// prunes stale rows approximately once per hour (~60 cycles).
///
/// Exits cleanly when `shutdown_rx` fires.
pub async fn run_storage_flusher(
    config: Arc<RwLock<TelemetryConfig>>,
    ring: Arc<RwLock<RingBuffer>>,
    db_path: PathBuf,
    mut shutdown_rx: tokio::sync::watch::Receiver<()>,
) {
    let conn = match init_db(&db_path) {
        Ok(c) => c,
        Err(e) => {
            log::error!("[WATCHDOG] Failed to open telemetry DB: {}", e);
            return;
        }
    };

    let mut last_flushed_ts: i64 = 0;
    let mut prune_counter: u32 = 0;

    loop {
        let flush_interval = config.read().await.storage.flush_interval_secs;

        tokio::select! {
            _ = tokio::time::sleep(std::time::Duration::from_secs(flush_interval)) => {},
            _ = shutdown_rx.changed() => break,
        }

        // Collect new snapshots since last flush — track by timestamp, not count,
        // because the ring buffer evicts old entries after wrapping.
        let snapshots: Vec<TelemetrySnapshot> = {
            let ring_guard = ring.read().await;
            ring_guard
                .iter()
                .filter(|s| s.system.timestamp_ms > last_flushed_ts)
                .cloned()
                .collect()
        };

        if !snapshots.is_empty() {
            if let Some(last) = snapshots.last() {
                let new_ts = last.system.timestamp_ms;
                match insert_batch(&conn, &snapshots) {
                    Ok(n) => {
                        log::debug!("[WATCHDOG] Flushed {} snapshots to telemetry DB", n);
                        last_flushed_ts = new_ts;
                    }
                    Err(e) => log::warn!("[WATCHDOG] Failed to flush telemetry: {}", e),
                }
            }
        }

        prune_counter += 1;
        if prune_counter >= 60 {
            prune_counter = 0;
            let cfg = config.read().await;
            match prune(&conn, cfg.storage.retention_days) {
                Ok(n) if n > 0 => log::info!("[WATCHDOG] Pruned {} old telemetry records", n),
                Ok(_) => {}
                Err(e) => log::warn!("[WATCHDOG] Failed to prune telemetry: {}", e),
            }
        }
    }

    log::info!("[WATCHDOG] Storage flusher stopped");
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use crate::daemon::telemetry::types::{ProcessSnapshot, SystemSnapshot};

    fn test_snapshot(ts: i64) -> TelemetrySnapshot {
        TelemetrySnapshot {
            system: SystemSnapshot {
                timestamp_ms: ts,
                memory_used_bytes: 4_000_000_000,
                memory_total_bytes: 8_000_000_000,
                memory_available_bytes: 4_000_000_000,
                swap_used_bytes: 0,
                swap_total_bytes: 0,
                cpu_usage_percent: 30.0,
                load_average_1m: 1.5,
                disk_used_bytes: 50_000_000_000,
                disk_total_bytes: 64_000_000_000,
                disk_path: "/".to_string(),
            },
            processes: vec![ProcessSnapshot {
                pid: 1234,
                name: "test-node".to_string(),
                rss_bytes: 100_000_000,
                cpu_percent: 15.0,
            }],
        }
    }

    #[test]
    fn init_and_insert() {
        let dir = tempfile::tempdir().unwrap();
        let db_path = dir.path().join("test_telemetry.db");
        let conn = init_db(&db_path).unwrap();

        let snapshots = vec![
            test_snapshot(1000),
            test_snapshot(2000),
            test_snapshot(3000),
        ];
        let count = insert_batch(&conn, &snapshots).unwrap();
        assert_eq!(count, 3);
    }

    #[test]
    fn query_range_returns_all() {
        let dir = tempfile::tempdir().unwrap();
        let db_path = dir.path().join("test_telemetry.db");
        let conn = init_db(&db_path).unwrap();

        let snapshots: Vec<_> = (0..10).map(|i| test_snapshot(i * 1000)).collect();
        insert_batch(&conn, &snapshots).unwrap();

        let results = query_range(&conn, 0, 9000, 100).unwrap();
        assert_eq!(results.len(), 10);
    }

    #[test]
    fn query_range_downsamples() {
        let dir = tempfile::tempdir().unwrap();
        let db_path = dir.path().join("test_telemetry.db");
        let conn = init_db(&db_path).unwrap();

        let snapshots: Vec<_> = (0..100).map(|i| test_snapshot(i * 1000)).collect();
        insert_batch(&conn, &snapshots).unwrap();

        let results = query_range(&conn, 0, 99000, 10).unwrap();
        assert_eq!(results.len(), 10);
    }

    #[test]
    fn prune_removes_old() {
        let dir = tempfile::tempdir().unwrap();
        let db_path = dir.path().join("test_telemetry.db");
        let conn = init_db(&db_path).unwrap();

        // 10 days ago
        let old_ms = chrono::Utc::now().timestamp_millis() - (10_i64 * 24 * 60 * 60 * 1000);
        let recent_ms = chrono::Utc::now().timestamp_millis();
        let snapshots = vec![test_snapshot(old_ms), test_snapshot(recent_ms)];
        insert_batch(&conn, &snapshots).unwrap();

        let deleted = prune(&conn, 7).unwrap();
        assert_eq!(deleted, 1);

        let remaining = query_range(&conn, 0, i64::MAX, 100).unwrap();
        assert_eq!(remaining.len(), 1);
    }
}
