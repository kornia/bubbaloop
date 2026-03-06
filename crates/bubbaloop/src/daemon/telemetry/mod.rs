//! Telemetry watchdog service.
//!
//! Provides adaptive system metric sampling, in-memory ring buffer, SQLite
//! cold storage, and a circuit-breaker that can kill runaway nodes under
//! memory pressure.
//!
//! # Usage
//!
//! ```text
//! let svc = TelemetryService::start(node_manager, shutdown_rx).await;
//! let snapshot = svc.current_snapshot().await;
//! let mut alerts = svc.subscribe_alerts();
//! ```

pub mod circuit_breaker;
pub mod sampler;
pub mod storage;
pub mod types;

use crate::daemon::registry::get_bubbaloop_home;
use crate::daemon::NodeManager;
use sampler::RingBuffer;
use std::path::PathBuf;
use std::sync::Arc;
use tokio::sync::{broadcast, watch, RwLock};
use types::{classify_level, TelemetryConfig, TelemetrySnapshot, WatchdogAlert, WatchdogLevel};

/// Capacity of the broadcast channel for watchdog alerts.
const ALERT_BROADCAST_CAPACITY: usize = 64;

/// Capacity of the internal mpsc channel from sampler → circuit-breaker.
const SAMPLER_CHANNEL_CAPACITY: usize = 32;

// ---------------------------------------------------------------------------
// TelemetryService
// ---------------------------------------------------------------------------

/// High-level handle to the telemetry watchdog subsystem.
///
/// Owns the shared config, ring buffer, alert broadcast channel and the
/// path to the SQLite database.  All long-running work runs in spawned tasks.
pub struct TelemetryService {
    config: Arc<RwLock<TelemetryConfig>>,
    ring: Arc<RwLock<RingBuffer>>,
    watchdog_alert_tx: broadcast::Sender<WatchdogAlert>,
    db_path: PathBuf,
}

impl TelemetryService {
    // -----------------------------------------------------------------------
    // start
    // -----------------------------------------------------------------------

    /// Load config, validate, and spawn the four background tasks:
    /// sampler, circuit-breaker, storage flusher, and config watcher.
    ///
    /// Returns immediately; all work runs in background Tokio tasks.
    pub async fn start(node_manager: Arc<NodeManager>, shutdown_rx: watch::Receiver<()>) -> Self {
        let home = get_bubbaloop_home();
        let config_path = home.join("telemetry.toml");
        let db_path = home.join("telemetry.db");

        // Ensure the home directory exists.
        if let Err(e) = std::fs::create_dir_all(&home) {
            log::warn!("[WATCHDOG] Could not create bubbaloop home dir: {}", e);
        }

        // Load and validate config.
        let mut config = TelemetryConfig::load_or_default(&config_path);
        let clamped = config.validate_and_clamp();
        for msg in &clamped {
            log::warn!("[WATCHDOG] Config clamped: {}", msg);
        }

        let ring_capacity = config.sampling.ring_capacity;
        let config = Arc::new(RwLock::new(config));
        let ring = Arc::new(RwLock::new(RingBuffer::new(ring_capacity)));
        let (watchdog_alert_tx, _) = broadcast::channel(ALERT_BROADCAST_CAPACITY);
        let (sampler_tx, sampler_rx) = tokio::sync::mpsc::channel(SAMPLER_CHANNEL_CAPACITY);

        // --- Task 1: sampler ---
        {
            let cfg = config.clone();
            let r = ring.clone();
            let tx = sampler_tx.clone();
            let sd = shutdown_rx.clone();
            tokio::spawn(async move {
                sampler::run_sampler(cfg, r, tx, sd).await;
            });
        }

        // --- Task 2: circuit-breaker ---
        {
            let cfg = config.clone();
            let nm = node_manager.clone();
            let alert_tx = watchdog_alert_tx.clone();
            let sd = shutdown_rx.clone();
            tokio::spawn(async move {
                circuit_breaker::run_circuit_breaker(cfg, nm, sampler_rx, alert_tx, sd).await;
            });
        }

        // --- Task 3: storage flusher ---
        {
            let cfg = config.clone();
            let r = ring.clone();
            let path = db_path.clone();
            let sd = shutdown_rx.clone();
            tokio::spawn(async move {
                storage::run_storage_flusher(cfg, r, path, sd).await;
            });
        }

        // --- Task 4: config file watcher ---
        {
            let cfg = config.clone();
            let sd = shutdown_rx.clone();
            tokio::spawn(async move {
                watch_config(cfg, config_path, sd).await;
            });
        }

        log::info!(
            "[WATCHDOG] TelemetryService started (db={})",
            db_path.display()
        );

        Self {
            config,
            ring,
            watchdog_alert_tx,
            db_path,
        }
    }

    // -----------------------------------------------------------------------
    // current_snapshot
    // -----------------------------------------------------------------------

    /// Return the most recent snapshot from the in-memory ring buffer.
    pub async fn current_snapshot(&self) -> Option<TelemetrySnapshot> {
        self.ring.read().await.latest().cloned()
    }

    // -----------------------------------------------------------------------
    // current_level
    // -----------------------------------------------------------------------

    /// Classify the current memory pressure level from the latest snapshot.
    ///
    /// Returns `Green` when no snapshot is available yet.
    pub async fn current_level(&self) -> WatchdogLevel {
        let snapshot = self.current_snapshot().await;
        match snapshot {
            None => WatchdogLevel::Green,
            Some(s) => {
                let thresholds = self.config.read().await.thresholds.clone();
                let avail = s.system.memory_available_percent();
                classify_level(avail, &thresholds)
            }
        }
    }

    // -----------------------------------------------------------------------
    // query_history
    // -----------------------------------------------------------------------

    /// Query stored telemetry history from SQLite.
    ///
    /// Returns up to `max_points` snapshots covering the last
    /// `duration_minutes` minutes, downsampled if necessary.
    ///
    /// Runs synchronously on the calling thread (storage I/O is fast enough
    /// for on-demand queries).
    pub fn query_history(
        &self,
        duration_minutes: u64,
        max_points: usize,
    ) -> Result<Vec<TelemetrySnapshot>, String> {
        let conn = storage::init_db(&self.db_path).map_err(|e| e.to_string())?;
        let now_ms = chrono::Utc::now().timestamp_millis();
        let from_ms = now_ms - (duration_minutes as i64 * 60 * 1000);
        storage::query_range(&conn, from_ms, now_ms, max_points).map_err(|e| e.to_string())
    }

    // -----------------------------------------------------------------------
    // subscribe_alerts
    // -----------------------------------------------------------------------

    /// Subscribe to the broadcast stream of `WatchdogAlert`s.
    pub fn subscribe_alerts(&self) -> broadcast::Receiver<WatchdogAlert> {
        self.watchdog_alert_tx.subscribe()
    }

    // -----------------------------------------------------------------------
    // update_config
    // -----------------------------------------------------------------------

    /// Apply a partial JSON update to the telemetry config, validate, persist,
    /// and hot-reload the in-memory config.
    ///
    /// Returns the list of fields that were clamped by validation guardrails.
    /// Emits an audit log entry via `log::info!`.
    pub async fn update_config(&self, updates: serde_json::Value) -> Result<Vec<String>, String> {
        // Serialise current config to JSON, merge the updates, deserialise back.
        let current_json = {
            let cfg = self.config.read().await;
            serde_json::to_value(&*cfg).map_err(|e| e.to_string())?
        };

        let merged = merge_json(current_json, updates);
        let mut new_config: TelemetryConfig =
            serde_json::from_value(merged).map_err(|e| e.to_string())?;

        let clamped = new_config.validate_and_clamp();
        for msg in &clamped {
            log::warn!("[WATCHDOG] update_config clamped: {}", msg);
        }

        // Persist to disk as TOML.
        let home = get_bubbaloop_home();
        let config_path = home.join("telemetry.toml");
        let toml_str = build_toml_string(&new_config).map_err(|e| e.to_string())?;
        std::fs::write(&config_path, toml_str).map_err(|e| e.to_string())?;

        log::info!(
            "[WATCHDOG] [audit] telemetry config updated — {} fields clamped",
            clamped.len()
        );

        // Hot-reload in-memory.
        *self.config.write().await = new_config;

        Ok(clamped)
    }

    // -----------------------------------------------------------------------
    // prompt_summary
    // -----------------------------------------------------------------------

    /// Return a one-line resource status string suitable for agent prompts.
    ///
    /// Example: `"mem:72% cpu:34% disk_free:4321MB level:Yellow"`
    ///
    /// Returns `None` when no snapshot is available yet.
    pub async fn prompt_summary(&self) -> Option<String> {
        let snapshot = self.current_snapshot().await?;
        let thresholds = self.config.read().await.thresholds.clone();
        let avail = snapshot.system.memory_available_percent();
        let mem_used = 100.0 - avail;
        let cpu = snapshot.system.cpu_usage_percent;
        let disk_free = snapshot.system.disk_free_mb();
        let level = classify_level(avail, &thresholds);

        Some(format!(
            "mem:{:.0}% cpu:{:.0}% disk_free:{}MB level:{:?}",
            mem_used, cpu, disk_free, level
        ))
    }
}

// ---------------------------------------------------------------------------
// watch_config  (private)
// ---------------------------------------------------------------------------

/// Watch the config file for changes, debounce 200 ms, and hot-reload
/// the in-memory `TelemetryConfig` when a change is detected.
async fn watch_config(
    config: Arc<RwLock<TelemetryConfig>>,
    config_path: PathBuf,
    shutdown_rx: watch::Receiver<()>,
) {
    use notify::{Event, RecursiveMode, Watcher};
    use std::sync::mpsc as std_mpsc;

    let (tx, rx) = std_mpsc::channel::<notify::Result<Event>>();

    let mut watcher = match notify::recommended_watcher(tx) {
        Ok(w) => w,
        Err(e) => {
            log::warn!("[WATCHDOG] Could not create config watcher: {}", e);
            return;
        }
    };

    if let Err(e) = watcher.watch(&config_path, RecursiveMode::NonRecursive) {
        // File may not exist yet — that's fine; the watcher will be idle.
        log::debug!(
            "[WATCHDOG] Config watcher not watching {} ({})",
            config_path.display(),
            e
        );
    }

    log::debug!(
        "[WATCHDOG] Config watcher started for {}",
        config_path.display()
    );

    loop {
        // Poll the std channel with a short timeout to remain cancellable.
        let event = {
            let timeout = std::time::Duration::from_millis(200);
            rx.recv_timeout(timeout)
        };

        // Check for shutdown signal (non-blocking).
        if shutdown_rx.has_changed().unwrap_or(false) {
            log::debug!("[WATCHDOG] Config watcher shutting down");
            break;
        }

        match event {
            Ok(Ok(_)) => {
                // Debounce: drain any additional events that arrived quickly.
                std::thread::sleep(std::time::Duration::from_millis(200));
                while rx.try_recv().is_ok() {}

                let mut new_cfg = TelemetryConfig::load_or_default(&config_path);
                let clamped = new_cfg.validate_and_clamp();
                for msg in &clamped {
                    log::warn!("[WATCHDOG] Reloaded config clamped: {}", msg);
                }
                *config.write().await = new_cfg;
                log::info!(
                    "[WATCHDOG] Config hot-reloaded from {}",
                    config_path.display()
                );
            }
            Ok(Err(e)) => {
                log::warn!("[WATCHDOG] Config watcher error: {}", e);
            }
            Err(std_mpsc::RecvTimeoutError::Timeout) => {
                // Normal timeout — loop and re-check shutdown.
            }
            Err(std_mpsc::RecvTimeoutError::Disconnected) => {
                log::warn!("[WATCHDOG] Config watcher channel disconnected");
                break;
            }
        }
    }
}

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

/// Recursively merge `updates` into `base` (JSON objects only at the top level).
fn merge_json(mut base: serde_json::Value, updates: serde_json::Value) -> serde_json::Value {
    if let (Some(base_obj), Some(upd_obj)) = (base.as_object_mut(), updates.as_object()) {
        for (k, v) in upd_obj {
            let entry = base_obj.entry(k.clone()).or_insert(serde_json::Value::Null);
            if entry.is_object() && v.is_object() {
                *entry = merge_json(entry.clone(), v.clone());
            } else {
                *entry = v.clone();
            }
        }
        base
    } else {
        updates
    }
}

/// Serialise a `TelemetryConfig` to a TOML string wrapped under `[telemetry]`.
fn build_toml_string(cfg: &TelemetryConfig) -> Result<String, toml::ser::Error> {
    use serde::Serialize;

    #[derive(Serialize)]
    struct TelemetryConfigFile<'a> {
        telemetry: &'a TelemetryConfig,
    }

    toml::to_string_pretty(&TelemetryConfigFile { telemetry: cfg })
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn merge_json_basic() {
        let base = serde_json::json!({ "a": 1, "b": { "c": 2, "d": 3 } });
        let updates = serde_json::json!({ "b": { "c": 99 } });
        let merged = merge_json(base, updates);
        assert_eq!(merged["a"], 1);
        assert_eq!(merged["b"]["c"], 99);
        assert_eq!(merged["b"]["d"], 3);
    }

    #[test]
    fn build_toml_round_trip() {
        let cfg = TelemetryConfig::default();
        let s = build_toml_string(&cfg).expect("serialise");
        assert!(s.contains("[telemetry]"));
        assert!(s.contains("enabled = true"));
    }

    #[test]
    fn prompt_summary_format() {
        // Build a fake snapshot and verify the format string manually.
        use types::{ProcessSnapshot, SystemSnapshot, TelemetrySnapshot};
        let snap = TelemetrySnapshot {
            system: SystemSnapshot {
                timestamp_ms: 0,
                memory_used_bytes: 6 * 1024 * 1024 * 1024,
                memory_total_bytes: 8 * 1024 * 1024 * 1024,
                memory_available_bytes: 2 * 1024 * 1024 * 1024,
                swap_used_bytes: 0,
                swap_total_bytes: 0,
                cpu_usage_percent: 34.5,
                load_average_1m: 1.0,
                disk_used_bytes: 10 * 1024 * 1024 * 1024,
                disk_total_bytes: 16 * 1024 * 1024 * 1024,
                disk_path: "/".to_string(),
            },
            processes: vec![ProcessSnapshot {
                pid: 1,
                name: "test".to_string(),
                rss_bytes: 1024,
                cpu_percent: 1.0,
            }],
        };

        // 2 GB available of 8 GB total → 25% available → 75% used → Yellow (60 ≤ 75 < 80)
        let thresholds = types::TelemetryThresholds::default();
        let avail = snap.system.memory_available_percent();
        let level = classify_level(avail, &thresholds);
        assert_eq!(level, WatchdogLevel::Yellow);

        let disk_free = snap.system.disk_free_mb();
        assert_eq!(disk_free, 6 * 1024); // 6 GB in MB
    }
}
