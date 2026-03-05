//! Telemetry sampler: ring buffer, snapshot collection, and the async sampler loop.

use std::collections::VecDeque;
use std::sync::Arc;
use sysinfo::{Disks, ProcessesToUpdate, System};
use tokio::sync::{watch, RwLock};

use super::types::{
    classify_level, ProcessSnapshot, SamplingConfig, SystemSnapshot, TelemetryConfig,
    TelemetrySnapshot, WatchdogLevel,
};

// ---------------------------------------------------------------------------
// RingBuffer
// ---------------------------------------------------------------------------

/// Fixed-capacity ring buffer for `TelemetrySnapshot`s.
/// Evicts the oldest entry when capacity is exceeded.
pub struct RingBuffer {
    inner: VecDeque<TelemetrySnapshot>,
    capacity: usize,
}

impl RingBuffer {
    /// Create a new ring buffer with the given `capacity`.
    pub fn new(capacity: usize) -> Self {
        Self {
            inner: VecDeque::with_capacity(capacity),
            capacity,
        }
    }

    /// Push a snapshot, evicting the oldest entry if the buffer is full.
    pub fn push(&mut self, snapshot: TelemetrySnapshot) {
        if self.inner.len() == self.capacity {
            self.inner.pop_front();
        }
        self.inner.push_back(snapshot);
    }

    /// Return the most recently pushed snapshot, or `None` if empty.
    pub fn latest(&self) -> Option<&TelemetrySnapshot> {
        self.inner.back()
    }

    /// Iterate over all snapshots from oldest to newest.
    pub fn iter(&self) -> impl Iterator<Item = &TelemetrySnapshot> {
        self.inner.iter()
    }

    /// Current number of snapshots held.
    pub fn len(&self) -> usize {
        self.inner.len()
    }

    /// Returns `true` when no snapshots are held.
    pub fn is_empty(&self) -> bool {
        self.inner.is_empty()
    }
}

// ---------------------------------------------------------------------------
// take_snapshot
// ---------------------------------------------------------------------------

/// Collect a `TelemetrySnapshot` from the given sysinfo handles.
///
/// - Refreshes memory, CPU, and all processes.
/// - Resolves a leading `~` in `disk_path` to the user's home directory.
/// - Finds the disk whose mount point is the longest prefix of the resolved
///   path (best-match), then records its total and available space.
/// - Returns the top 20 processes sorted descending by RSS.
pub fn take_snapshot(sys: &mut System, disks: &mut Disks, disk_path: &str) -> TelemetrySnapshot {
    // Refresh system metrics.
    sys.refresh_memory();
    sys.refresh_cpu_all();
    sys.refresh_processes(ProcessesToUpdate::All, true);

    // Refresh disk list.
    disks.refresh(true);

    let timestamp_ms = chrono::Utc::now().timestamp_millis();

    // Resolve `~` in disk_path.
    let resolved_path: std::path::PathBuf = if disk_path.starts_with('~') {
        if let Some(home) = dirs::home_dir() {
            let stripped = disk_path.trim_start_matches('~').trim_start_matches('/');
            if stripped.is_empty() {
                home
            } else {
                home.join(stripped)
            }
        } else {
            std::path::PathBuf::from(disk_path)
        }
    } else {
        std::path::PathBuf::from(disk_path)
    };

    // Find the disk whose mount point is the longest prefix match of resolved_path.
    let (disk_total, disk_available, matched_path) = disks
        .list()
        .iter()
        .filter_map(|disk| {
            let mount = disk.mount_point();
            if resolved_path.starts_with(mount) {
                let match_len = mount.as_os_str().len();
                Some((
                    disk.total_space(),
                    disk.available_space(),
                    match_len,
                    mount.to_string_lossy().into_owned(),
                ))
            } else {
                None
            }
        })
        .max_by_key(|&(_, _, len, _)| len)
        .map(|(total, avail, _, path)| (total, avail, path))
        .unwrap_or((0, 0, disk_path.to_string()));

    let disk_used = disk_total.saturating_sub(disk_available);

    // Build top-20 process list by RSS (descending).
    let mut processes: Vec<ProcessSnapshot> = sys
        .processes()
        .values()
        .map(|p| ProcessSnapshot {
            pid: p.pid().as_u32(),
            name: p.name().to_string_lossy().into_owned(),
            rss_bytes: p.memory(),
            cpu_percent: p.cpu_usage(),
        })
        .collect();

    processes.sort_unstable_by(|a, b| b.rss_bytes.cmp(&a.rss_bytes));
    processes.truncate(20);

    let system = SystemSnapshot {
        timestamp_ms,
        memory_used_bytes: sys.used_memory(),
        memory_total_bytes: sys.total_memory(),
        memory_available_bytes: sys.available_memory(),
        swap_used_bytes: sys.used_swap(),
        swap_total_bytes: sys.total_swap(),
        cpu_usage_percent: sys.global_cpu_usage(),
        load_average_1m: sysinfo::System::load_average().one,
        disk_used_bytes: disk_used,
        disk_total_bytes: disk_total,
        disk_path: matched_path,
    };

    TelemetrySnapshot { system, processes }
}

// ---------------------------------------------------------------------------
// interval_for_level
// ---------------------------------------------------------------------------

/// Return the sampling interval (seconds) appropriate for the given pressure level.
///
/// - Green → `idle_secs`
/// - Yellow → `elevated_secs`
/// - Orange / Red / Critical → `critical_secs`
pub fn interval_for_level(level: WatchdogLevel, config: &SamplingConfig) -> u64 {
    match level {
        WatchdogLevel::Green => config.idle_secs,
        WatchdogLevel::Yellow => config.elevated_secs,
        WatchdogLevel::Orange | WatchdogLevel::Red | WatchdogLevel::Critical => {
            config.critical_secs
        }
    }
}

// ---------------------------------------------------------------------------
// run_sampler
// ---------------------------------------------------------------------------

/// Async sampler loop.
///
/// - Creates a `System` and `Disks` instance internally.
/// - Does an initial CPU refresh followed by a 500 ms sleep to establish a
///   baseline (sysinfo CPU usage requires two samples to be meaningful).
/// - On each iteration: reads config, takes a snapshot, classifies the
///   memory pressure level, pushes to the ring buffer, sends an alert when
///   the level is Yellow or worse (or when the level changes), and then
///   sleeps for the interval appropriate to the current level.
/// - Exits cleanly when `shutdown_rx` fires.
pub async fn run_sampler(
    config: Arc<RwLock<TelemetryConfig>>,
    ring: Arc<RwLock<RingBuffer>>,
    alert_tx: tokio::sync::mpsc::Sender<(TelemetrySnapshot, WatchdogLevel)>,
    mut shutdown_rx: watch::Receiver<()>,
) {
    let mut sys = System::new_all();
    let mut disks = Disks::new_with_refreshed_list();

    // Initial CPU refresh + baseline sleep.
    sys.refresh_cpu_all();
    tokio::time::sleep(std::time::Duration::from_millis(500)).await;

    let mut prev_level: Option<WatchdogLevel> = None;

    loop {
        // Read config under a short-lived read lock.
        let (disk_path, sampling_cfg, thresholds) = {
            let cfg = config.read().await;
            (
                cfg.monitored_disk_path.clone(),
                cfg.sampling.clone(),
                cfg.thresholds.clone(),
            )
        };

        // Take snapshot (blocking sysinfo calls — acceptable on Tokio tasks for
        // short durations; run in blocking task for stricter latency budgets).
        let snapshot = take_snapshot(&mut sys, &mut disks, &disk_path);

        // Classify memory pressure level.
        let available_pct = snapshot.system.memory_available_percent();
        let level = classify_level(available_pct, &thresholds);

        // Push to ring.
        {
            let mut ring_guard = ring.write().await;
            ring_guard.push(snapshot.clone());
        }

        // Send alert on Yellow+ or on level change.
        let level_changed = prev_level.map(|p| p != level).unwrap_or(false);
        if level >= WatchdogLevel::Yellow || level_changed {
            // Log level transitions.
            if level_changed {
                if let Some(prev) = prev_level {
                    log::info!(
                        "[telemetry] watchdog level changed: {:?} → {:?} (memory available {:.1}%)",
                        prev,
                        level,
                        available_pct
                    );
                }
            }
            // Best-effort send; drop alert if channel is full.
            let _ = alert_tx.try_send((snapshot, level));
        }

        prev_level = Some(level);

        // Determine sleep interval.
        let interval_secs = interval_for_level(level, &sampling_cfg);
        let sleep_duration = std::time::Duration::from_secs(interval_secs);

        // Sleep or shutdown.
        tokio::select! {
            _ = tokio::time::sleep(sleep_duration) => {}
            _ = shutdown_rx.changed() => {
                log::info!("[telemetry] sampler shutting down");
                break;
            }
        }
    }
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    fn make_snapshot(id: u64) -> TelemetrySnapshot {
        TelemetrySnapshot {
            system: SystemSnapshot {
                timestamp_ms: id as i64,
                memory_used_bytes: id,
                memory_total_bytes: 100,
                memory_available_bytes: 100 - id,
                swap_used_bytes: 0,
                swap_total_bytes: 0,
                cpu_usage_percent: 0.0,
                load_average_1m: 0.0,
                disk_used_bytes: 0,
                disk_total_bytes: 0,
                disk_path: "/".to_string(),
            },
            processes: vec![ProcessSnapshot {
                pid: id as u32,
                name: format!("proc{}", id),
                rss_bytes: id,
                cpu_percent: 0.0,
            }],
        }
    }

    #[test]
    fn ring_buffer_push_and_capacity() {
        let mut ring = RingBuffer::new(3);

        // Push 5 snapshots into a capacity-3 buffer.
        for i in 0..5u64 {
            ring.push(make_snapshot(i));
        }

        // Oldest two (0, 1) should have been evicted.
        assert_eq!(ring.len(), 3, "len should be capped at capacity");

        // latest() should be snapshot id=4.
        let latest = ring.latest().expect("should have a latest snapshot");
        assert_eq!(
            latest.system.timestamp_ms, 4,
            "latest should be the last pushed snapshot"
        );

        // Iteration order oldest→newest: ids 2, 3, 4.
        let ids: Vec<i64> = ring.iter().map(|s| s.system.timestamp_ms).collect();
        assert_eq!(ids, vec![2, 3, 4], "iter should yield oldest first");
    }

    #[test]
    fn interval_for_level_values() {
        let cfg = SamplingConfig {
            idle_secs: 30,
            elevated_secs: 10,
            critical_secs: 5,
            ring_capacity: 720,
        };

        assert_eq!(interval_for_level(WatchdogLevel::Green, &cfg), 30);
        assert_eq!(interval_for_level(WatchdogLevel::Yellow, &cfg), 10);
        assert_eq!(interval_for_level(WatchdogLevel::Orange, &cfg), 5);
        assert_eq!(interval_for_level(WatchdogLevel::Red, &cfg), 5);
        assert_eq!(interval_for_level(WatchdogLevel::Critical, &cfg), 5);
    }
}
