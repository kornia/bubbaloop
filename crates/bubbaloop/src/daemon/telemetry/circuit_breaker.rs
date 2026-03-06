//! Telemetry watchdog circuit breaker.
//!
//! Receives classified `(TelemetrySnapshot, WatchdogLevel)` alerts from the
//! sampler and decides whether to emit warnings, kill individual nodes, or
//! kill all non-essential nodes.  A cooldown timer prevents rapid successive
//! kills ("kill storms").

use std::sync::Arc;
use std::time::Instant;
use tokio::sync::{broadcast, mpsc, watch, RwLock};

use crate::daemon::telemetry::types::{
    TelemetryConfig, TelemetrySnapshot, WatchdogAlert, WatchdogLevel,
};
use crate::daemon::NodeManager;
use crate::schemas::daemon::v1::NodeStatus;

// ---------------------------------------------------------------------------
// Public entry point
// ---------------------------------------------------------------------------

/// Run the circuit-breaker loop.
///
/// Receives `(snapshot, level)` pairs from the sampler and reacts according to
/// the current `TelemetryConfig`:
///
/// * **Green** (recovered from Yellow+) → emit `ResourceRecovered`
/// * **Yellow / Orange** → emit `ResourceWarning` (top-5 processes by RSS)
/// * **Red** → if cooldown has expired, kill the single largest running node
/// * **Critical** → kill every running node immediately
///
/// CPU sustained-high and disk-low warnings are also emitted here.
pub async fn run_circuit_breaker(
    config: Arc<RwLock<TelemetryConfig>>,
    node_manager: Arc<NodeManager>,
    mut alert_rx: mpsc::Receiver<(TelemetrySnapshot, WatchdogLevel)>,
    watchdog_alert_tx: broadcast::Sender<WatchdogAlert>,
    mut shutdown_rx: watch::Receiver<()>,
) {
    let mut last_kill_time: Option<Instant> = None;
    let mut previous_level = WatchdogLevel::Green;
    let mut cpu_above_threshold_since: Option<Instant> = None;

    loop {
        tokio::select! {
            // A new snapshot+level arrived from the sampler.
            maybe_msg = alert_rx.recv() => {
                let (snapshot, level) = match maybe_msg {
                    Some(v) => v,
                    None => {
                        log::info!("[circuit_breaker] alert channel closed, exiting");
                        break;
                    }
                };

                // Read config once per tick (cheap clone of Arc-protected value).
                let cfg = config.read().await.clone();

                if !cfg.circuit_breaker.enabled {
                    previous_level = level;
                    continue;
                }

                // --- Recovery detection ---
                if previous_level >= WatchdogLevel::Yellow && level == WatchdogLevel::Green {
                    let avail_pct = snapshot.system.memory_available_percent();
                    log::info!(
                        "[circuit_breaker] memory pressure recovered: available={:.1}%",
                        avail_pct
                    );
                    let _ = watchdog_alert_tx.send(WatchdogAlert::ResourceRecovered {
                        resource: "memory".to_string(),
                        value: avail_pct,
                    });
                }

                // --- CPU sustained high ---
                let cpu = snapshot.system.cpu_usage_percent as f64;
                if cpu >= cfg.thresholds.cpu_warn_pct as f64 {
                    let since = cpu_above_threshold_since.get_or_insert_with(Instant::now);
                    let elapsed = since.elapsed().as_secs();
                    if elapsed >= cfg.thresholds.cpu_sustained_secs {
                        log::warn!(
                            "[circuit_breaker] CPU sustained high: {:.1}% for {}s",
                            cpu,
                            elapsed
                        );
                        let _ = watchdog_alert_tx.send(WatchdogAlert::TrendAlert {
                            resource: "cpu".to_string(),
                            description: format!(
                                "CPU at {:.1}% for {} seconds (threshold {}s)",
                                cpu, elapsed, cfg.thresholds.cpu_sustained_secs
                            ),
                        });
                        // Reset so we don't spam the alert every tick.
                        cpu_above_threshold_since = None;
                    }
                } else {
                    // CPU fell below the threshold — reset the timer.
                    cpu_above_threshold_since = None;
                }

                // --- Disk low ---
                let disk_free = snapshot.system.disk_free_mb();
                if disk_free < cfg.thresholds.disk_warn_mb {
                    log::warn!(
                        "[circuit_breaker] low disk: {} MB free (warn threshold {} MB)",
                        disk_free,
                        cfg.thresholds.disk_warn_mb
                    );
                    let _ = watchdog_alert_tx.send(WatchdogAlert::ResourceWarning {
                        level,
                        resource: "disk".to_string(),
                        value: disk_free as f64,
                        threshold: cfg.thresholds.disk_warn_mb as f64,
                    });
                }

                // --- Level-based actions ---
                match level {
                    WatchdogLevel::Green => {
                        // Nothing to do beyond recovery detection above.
                    }

                    WatchdogLevel::Yellow | WatchdogLevel::Orange => {
                        let avail_pct = snapshot.system.memory_available_percent();
                        log::warn!(
                            "[circuit_breaker] memory pressure {:?}: available={:.1}%",
                            level,
                            avail_pct
                        );

                        // Report top-5 processes by RSS.
                        let top5 = top_processes_by_rss(&snapshot, 5);
                        let top5_desc = top5
                            .iter()
                            .map(|p| format!("{}({}MB)", p.name, p.rss_bytes / (1024 * 1024)))
                            .collect::<Vec<_>>()
                            .join(", ");

                        let _ = watchdog_alert_tx.send(WatchdogAlert::ResourceWarning {
                            level,
                            resource: "memory".to_string(),
                            value: avail_pct,
                            threshold: (100.0 - cfg.thresholds.yellow_pct as f64),
                        });

                        if !top5_desc.is_empty() {
                            log::info!("[circuit_breaker] top processes: {}", top5_desc);
                        }
                    }

                    WatchdogLevel::Red => {
                        let cooldown = cfg.circuit_breaker.cooldown_secs;
                        if should_kill(&last_kill_time, cooldown) {
                            if let Some((name, rss)) =
                                find_kill_candidate(&node_manager, &snapshot, false).await
                            {
                                kill_node(
                                    &node_manager,
                                    &name,
                                    &watchdog_alert_tx,
                                    &snapshot,
                                    rss,
                                )
                                .await;
                                last_kill_time = Some(Instant::now());
                            } else {
                                log::warn!(
                                    "[circuit_breaker] Red level but no kill candidate found"
                                );
                            }
                        } else {
                            log::debug!(
                                "[circuit_breaker] Red level — skipping kill (cooldown active)"
                            );
                        }
                    }

                    WatchdogLevel::Critical => {
                        log::error!("[circuit_breaker] CRITICAL memory pressure — killing all non-essential nodes");
                        kill_all_non_essential(&node_manager, &watchdog_alert_tx, &snapshot).await;
                        last_kill_time = Some(Instant::now());
                    }
                }

                previous_level = level;
            }

            // Graceful shutdown signal.
            _ = shutdown_rx.changed() => {
                log::info!("[circuit_breaker] shutdown signal received, exiting");
                break;
            }
        }
    }
}

// ---------------------------------------------------------------------------
// Helper: should_kill
// ---------------------------------------------------------------------------

/// Returns `true` if no kill has occurred yet, or if the last kill happened
/// more than `cooldown_secs` seconds ago.
fn should_kill(last_kill: &Option<Instant>, cooldown_secs: u64) -> bool {
    match last_kill {
        None => true,
        Some(t) => t.elapsed().as_secs() > cooldown_secs,
    }
}

// ---------------------------------------------------------------------------
// Helper: find_kill_candidate
// ---------------------------------------------------------------------------

/// Find the running node with the largest RSS by matching node names against
/// the process snapshots.
///
/// `include_essential` is reserved for future allow/deny-list support; pass
/// `false` to exclude nothing extra (all running nodes are candidates).
///
/// Returns `(node_name, rss_bytes)` of the best candidate, or `None`.
async fn find_kill_candidate(
    node_manager: &Arc<NodeManager>,
    snapshot: &TelemetrySnapshot,
    _include_essential: bool,
) -> Option<(String, u64)> {
    // Collect running node names from the cached state.
    let node_list = node_manager.get_node_list().await;
    let running_names: Vec<String> = node_list
        .nodes
        .iter()
        .filter(|n| n.status == NodeStatus::Running as i32)
        .map(|n| n.name.clone())
        .collect();

    if running_names.is_empty() {
        return None;
    }

    // For each running node, find its best-matching process snapshot.
    // We do a substring match: process name contains node name or vice-versa.
    let mut candidates: Vec<(String, u64)> = running_names
        .iter()
        .filter_map(|node_name| {
            // Find the process snapshot with the highest RSS that matches this
            // node name (case-insensitive substring).
            let name_lower = node_name.to_lowercase();
            let best = snapshot
                .processes
                .iter()
                .filter(|p| {
                    let proc_lower = p.name.to_lowercase();
                    proc_lower.contains(&name_lower) || name_lower.contains(&proc_lower)
                })
                .max_by_key(|p| p.rss_bytes);

            best.map(|p| (node_name.clone(), p.rss_bytes))
        })
        .collect();

    // Sort descending by RSS and take the top candidate.
    candidates.sort_by(|a, b| b.1.cmp(&a.1));
    candidates.into_iter().next()
}

// ---------------------------------------------------------------------------
// Helper: kill_node
// ---------------------------------------------------------------------------

/// Stop a single node, log the action, and broadcast a `NodeKilledByWatchdog`
/// alert.  Errors from `stop_node` are logged but do not propagate.
async fn kill_node(
    node_manager: &Arc<NodeManager>,
    name: &str,
    alert_tx: &broadcast::Sender<WatchdogAlert>,
    snapshot: &TelemetrySnapshot,
    rss_bytes: u64,
) {
    log::warn!(
        "[circuit_breaker] killing node '{}' (RSS {} MB)",
        name,
        rss_bytes / (1024 * 1024)
    );

    // Try to find the PID of the process we matched (best-effort).
    let pid = {
        let name_lower = name.to_lowercase();
        snapshot
            .processes
            .iter()
            .filter(|p| {
                let proc_lower = p.name.to_lowercase();
                proc_lower.contains(&name_lower) || name_lower.contains(&proc_lower)
            })
            .max_by_key(|p| p.rss_bytes)
            .map(|p| p.pid)
            .unwrap_or(0)
    };

    match node_manager.stop_node(name).await {
        Ok(_) => {
            log::info!("[circuit_breaker] successfully stopped '{}'", name);
        }
        Err(e) => {
            log::error!("[circuit_breaker] failed to stop '{}': {}", name, e);
        }
    }

    let _ = alert_tx.send(WatchdogAlert::NodeKilledByWatchdog {
        node_name: name.to_string(),
        pid,
        reason: format!(
            "watchdog Red level kill — RSS {} MB",
            rss_bytes / (1024 * 1024)
        ),
    });
}

// ---------------------------------------------------------------------------
// Helper: kill_all_non_essential
// ---------------------------------------------------------------------------

/// Stop every running node and emit a `NodeKilledByWatchdog` alert for each.
async fn kill_all_non_essential(
    node_manager: &Arc<NodeManager>,
    alert_tx: &broadcast::Sender<WatchdogAlert>,
    snapshot: &TelemetrySnapshot,
) {
    let node_list = node_manager.get_node_list().await;
    let running: Vec<String> = node_list
        .nodes
        .iter()
        .filter(|n| n.status == NodeStatus::Running as i32)
        .map(|n| n.name.clone())
        .collect();

    for name in running {
        // Best-effort PID lookup.
        let name_lower = name.to_lowercase();
        let pid = snapshot
            .processes
            .iter()
            .filter(|p| {
                let proc_lower = p.name.to_lowercase();
                proc_lower.contains(&name_lower) || name_lower.contains(&proc_lower)
            })
            .max_by_key(|p| p.rss_bytes)
            .map(|p| p.pid)
            .unwrap_or(0);

        match node_manager.stop_node(&name).await {
            Ok(_) => {
                log::info!("[circuit_breaker] Critical: stopped '{}'", name);
            }
            Err(e) => {
                log::error!(
                    "[circuit_breaker] Critical: failed to stop '{}': {}",
                    name,
                    e
                );
            }
        }

        let _ = alert_tx.send(WatchdogAlert::NodeKilledByWatchdog {
            node_name: name.clone(),
            pid,
            reason: "watchdog Critical level — killing all nodes".to_string(),
        });
    }
}

// ---------------------------------------------------------------------------
// Private helper: top_processes_by_rss
// ---------------------------------------------------------------------------

/// Return up to `n` process snapshots sorted by RSS descending.
fn top_processes_by_rss(
    snapshot: &TelemetrySnapshot,
    n: usize,
) -> Vec<&crate::daemon::telemetry::types::ProcessSnapshot> {
    let mut procs: Vec<_> = snapshot.processes.iter().collect();
    procs.sort_by(|a, b| b.rss_bytes.cmp(&a.rss_bytes));
    procs.truncate(n);
    procs
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn should_kill_no_previous() {
        // With no previous kill the breaker should always allow a kill.
        assert!(should_kill(&None, 30));
    }

    #[test]
    fn should_kill_after_cooldown() {
        // 31 seconds ago, with a 30-second cooldown → should be allowed.
        let past = Instant::now() - std::time::Duration::from_secs(31);
        assert!(should_kill(&Some(past), 30));
    }

    #[test]
    fn should_not_kill_during_cooldown() {
        // Just now (0 seconds elapsed), with a 30-second cooldown → blocked.
        let now = Instant::now();
        assert!(!should_kill(&Some(now), 30));
    }
}
