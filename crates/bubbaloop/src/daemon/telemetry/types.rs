//! Telemetry watchdog types
//!
//! Pure data types for system telemetry: snapshots, alerts, thresholds,
//! and configuration. No sysinfo calls or async here.

use serde::{Deserialize, Serialize};
use std::path::Path;

// ---------------------------------------------------------------------------
// Guardrail constants
// ---------------------------------------------------------------------------

/// Minimum allowed critical threshold (used percent). Prevents accidental
/// hair-trigger kills at very low thresholds.
pub const MIN_CRITICAL: u8 = 80;

/// Maximum allowed critical threshold (used percent). Prevents accidentally
/// disabling the watchdog at 100%.
pub const MAX_CRITICAL: u8 = 98;

/// Minimum sampling interval in seconds. Prevents runaway polling.
pub const MIN_SAMPLING_SECS: u64 = 2;

// ---------------------------------------------------------------------------
// WatchdogLevel
// ---------------------------------------------------------------------------

/// Resource pressure level, ordered from least to most severe.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum WatchdogLevel {
    Green,
    Yellow,
    Orange,
    Red,
    Critical,
}

// ---------------------------------------------------------------------------
// SystemSnapshot
// ---------------------------------------------------------------------------

/// Point-in-time snapshot of system-level resource metrics.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SystemSnapshot {
    /// Wall-clock timestamp (milliseconds since Unix epoch).
    pub timestamp_ms: i64,

    // Memory
    pub memory_used_bytes: u64,
    pub memory_total_bytes: u64,
    pub memory_available_bytes: u64,

    // Swap
    pub swap_used_bytes: u64,
    pub swap_total_bytes: u64,

    // CPU
    pub cpu_usage_percent: f32,

    // Load average
    pub load_average_1m: f64,

    // Disk
    pub disk_used_bytes: u64,
    pub disk_total_bytes: u64,
    pub disk_path: String,
}

impl SystemSnapshot {
    /// Available memory as a percentage of total (0–100).
    /// Returns 100.0 when total is 0 to avoid division-by-zero panics.
    pub fn memory_available_percent(&self) -> f64 {
        if self.memory_total_bytes == 0 {
            return 100.0;
        }
        (self.memory_available_bytes as f64 / self.memory_total_bytes as f64) * 100.0
    }

    /// Free disk space in megabytes.
    pub fn disk_free_mb(&self) -> u64 {
        let free_bytes = self.disk_total_bytes.saturating_sub(self.disk_used_bytes);
        free_bytes / (1024 * 1024)
    }
}

// ---------------------------------------------------------------------------
// ProcessSnapshot
// ---------------------------------------------------------------------------

/// Point-in-time snapshot of a single process's resource usage.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProcessSnapshot {
    pub pid: u32,
    pub name: String,
    /// Resident Set Size in bytes.
    pub rss_bytes: u64,
    pub cpu_percent: f32,
}

// ---------------------------------------------------------------------------
// TelemetrySnapshot
// ---------------------------------------------------------------------------

/// Combined system + process snapshot collected in a single pass.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TelemetrySnapshot {
    pub system: SystemSnapshot,
    pub processes: Vec<ProcessSnapshot>,
}

// ---------------------------------------------------------------------------
// WatchdogAlert
// ---------------------------------------------------------------------------

/// Structured alert emitted by the watchdog when notable events occur.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum WatchdogAlert {
    /// A resource exceeded a warning threshold.
    ResourceWarning {
        level: WatchdogLevel,
        resource: String,
        value: f64,
        threshold: f64,
    },
    /// A node process was killed by the watchdog due to resource pressure.
    NodeKilledByWatchdog {
        node_name: String,
        pid: u32,
        reason: String,
    },
    /// A previously stressed resource has recovered below warning level.
    ResourceRecovered { resource: String, value: f64 },
    /// A sustained trend was detected (e.g., memory growing continuously).
    TrendAlert {
        resource: String,
        description: String,
    },
}

// ---------------------------------------------------------------------------
// TelemetryThresholds
// ---------------------------------------------------------------------------

/// Threshold configuration for the telemetry watchdog.
///
/// All `*_pct` fields represent **used** percentage (0–100).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TelemetryThresholds {
    /// Used % at which level transitions to Yellow.
    #[serde(default = "default_yellow")]
    pub yellow_pct: u8,

    /// Used % at which level transitions to Orange.
    #[serde(default = "default_orange")]
    pub orange_pct: u8,

    /// Used % at which level transitions to Red.
    #[serde(default = "default_red")]
    pub red_pct: u8,

    /// Used % at which level transitions to Critical.
    #[serde(default = "default_critical")]
    pub critical_pct: u8,

    /// CPU usage % that triggers a CPU warning.
    #[serde(default = "default_cpu_warn")]
    pub cpu_warn_pct: u8,

    /// Seconds of sustained elevated CPU before a trend alert fires.
    #[serde(default = "default_cpu_sustained_secs")]
    pub cpu_sustained_secs: u64,

    /// Free disk below this many MB → warning.
    #[serde(default = "default_disk_warn_mb")]
    pub disk_warn_mb: u64,

    /// Free disk below this many MB → critical.
    #[serde(default = "default_disk_critical_mb")]
    pub disk_critical_mb: u64,
}

fn default_yellow() -> u8 {
    60
}
fn default_orange() -> u8 {
    80
}
fn default_red() -> u8 {
    90
}
fn default_critical() -> u8 {
    95
}
fn default_cpu_warn() -> u8 {
    95
}
fn default_cpu_sustained_secs() -> u64 {
    60
}
fn default_disk_warn_mb() -> u64 {
    1024
}
fn default_disk_critical_mb() -> u64 {
    200
}

impl Default for TelemetryThresholds {
    fn default() -> Self {
        Self {
            yellow_pct: default_yellow(),
            orange_pct: default_orange(),
            red_pct: default_red(),
            critical_pct: default_critical(),
            cpu_warn_pct: default_cpu_warn(),
            cpu_sustained_secs: default_cpu_sustained_secs(),
            disk_warn_mb: default_disk_warn_mb(),
            disk_critical_mb: default_disk_critical_mb(),
        }
    }
}

// ---------------------------------------------------------------------------
// classify_level
// ---------------------------------------------------------------------------

/// Classify a resource pressure level from an **available** percent and a
/// threshold config (which stores **used** percents).
///
/// The conversion is: `used_pct = 100 - available_pct`.
pub fn classify_level(available_percent: f64, thresholds: &TelemetryThresholds) -> WatchdogLevel {
    let used = 100.0 - available_percent;

    if used >= thresholds.critical_pct as f64 {
        WatchdogLevel::Critical
    } else if used >= thresholds.red_pct as f64 {
        WatchdogLevel::Red
    } else if used >= thresholds.orange_pct as f64 {
        WatchdogLevel::Orange
    } else if used >= thresholds.yellow_pct as f64 {
        WatchdogLevel::Yellow
    } else {
        WatchdogLevel::Green
    }
}

// ---------------------------------------------------------------------------
// SamplingConfig
// ---------------------------------------------------------------------------

/// How frequently the watchdog samples system metrics, by pressure level.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SamplingConfig {
    /// Seconds between samples when pressure is Green/Yellow.
    #[serde(default = "default_idle_secs")]
    pub idle_secs: u64,

    /// Seconds between samples when pressure is Orange/Red.
    #[serde(default = "default_elevated_secs")]
    pub elevated_secs: u64,

    /// Seconds between samples when pressure is Critical.
    #[serde(default = "default_critical_secs")]
    pub critical_secs: u64,

    /// Number of snapshots kept in the in-memory ring buffer.
    #[serde(default = "default_ring_capacity")]
    pub ring_capacity: usize,
}

fn default_idle_secs() -> u64 {
    30
}
fn default_elevated_secs() -> u64 {
    10
}
fn default_critical_secs() -> u64 {
    5
}
fn default_ring_capacity() -> usize {
    720
}

impl Default for SamplingConfig {
    fn default() -> Self {
        Self {
            idle_secs: default_idle_secs(),
            elevated_secs: default_elevated_secs(),
            critical_secs: default_critical_secs(),
            ring_capacity: default_ring_capacity(),
        }
    }
}

// ---------------------------------------------------------------------------
// CircuitBreakerConfig
// ---------------------------------------------------------------------------

/// Configuration for the watchdog's circuit breaker (prevents kill storms).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CircuitBreakerConfig {
    /// Whether the circuit breaker is active.
    #[serde(default = "default_cb_enabled")]
    pub enabled: bool,

    /// Seconds the breaker stays open after tripping before attempting reset.
    #[serde(default = "default_cb_cooldown_secs")]
    pub cooldown_secs: u64,
}

fn default_cb_enabled() -> bool {
    true
}
fn default_cb_cooldown_secs() -> u64 {
    30
}

impl Default for CircuitBreakerConfig {
    fn default() -> Self {
        Self {
            enabled: default_cb_enabled(),
            cooldown_secs: default_cb_cooldown_secs(),
        }
    }
}

// ---------------------------------------------------------------------------
// StorageConfig
// ---------------------------------------------------------------------------

/// Configuration for persisting telemetry snapshots to disk.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StorageConfig {
    /// Seconds between flushing the in-memory ring to disk.
    #[serde(default = "default_flush_interval_secs")]
    pub flush_interval_secs: u64,

    /// Days of historical telemetry to retain on disk.
    #[serde(default = "default_retention_days")]
    pub retention_days: u32,
}

fn default_flush_interval_secs() -> u64 {
    60
}
fn default_retention_days() -> u32 {
    7
}

impl Default for StorageConfig {
    fn default() -> Self {
        Self {
            flush_interval_secs: default_flush_interval_secs(),
            retention_days: default_retention_days(),
        }
    }
}

// ---------------------------------------------------------------------------
// TelemetryConfig
// ---------------------------------------------------------------------------

/// Top-level telemetry watchdog configuration (lives under `[telemetry]` in
/// the TOML file).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TelemetryConfig {
    /// Master switch — set to false to disable the watchdog entirely.
    #[serde(default = "default_enabled")]
    pub enabled: bool,

    #[serde(default)]
    pub sampling: SamplingConfig,

    #[serde(default)]
    pub thresholds: TelemetryThresholds,

    #[serde(default)]
    pub circuit_breaker: CircuitBreakerConfig,

    #[serde(default)]
    pub storage: StorageConfig,

    /// Filesystem path to monitor for disk space (e.g., `/` or `/data`).
    #[serde(default = "default_monitored_disk_path")]
    pub monitored_disk_path: String,
}

fn default_enabled() -> bool {
    true
}
fn default_monitored_disk_path() -> String {
    "/".to_string()
}

impl Default for TelemetryConfig {
    fn default() -> Self {
        Self {
            enabled: default_enabled(),
            sampling: SamplingConfig::default(),
            thresholds: TelemetryThresholds::default(),
            circuit_breaker: CircuitBreakerConfig::default(),
            storage: StorageConfig::default(),
            monitored_disk_path: default_monitored_disk_path(),
        }
    }
}

/// Wrapper for TOML deserialization — the config file wraps fields under
/// `[telemetry]`.
#[derive(Debug, Deserialize)]
struct TelemetryConfigFile {
    #[serde(default)]
    telemetry: TelemetryConfig,
}

impl TelemetryConfig {
    /// Load config from `path`, falling back to [`TelemetryConfig::default`]
    /// if the file is missing or unreadable.
    pub fn load_or_default(path: &Path) -> Self {
        match std::fs::read_to_string(path) {
            Ok(contents) => match toml::from_str::<TelemetryConfigFile>(&contents) {
                Ok(file) => {
                    let mut cfg = file.telemetry;
                    let _ = cfg.validate_and_clamp();
                    cfg
                }
                Err(e) => {
                    log::warn!(
                        "Failed to parse telemetry config at {}: {}. Using defaults.",
                        path.display(),
                        e
                    );
                    Self::default()
                }
            },
            Err(_) => Self::default(),
        }
    }

    /// Enforce guardrail constraints on threshold and sampling values,
    /// clamping any out-of-range fields in place.
    ///
    /// Returns a list of human-readable descriptions of every field that was
    /// clamped.
    pub fn validate_and_clamp(&mut self) -> Vec<String> {
        let mut clamped = Vec::new();

        // critical_pct
        if self.thresholds.critical_pct > MAX_CRITICAL {
            clamped.push(format!(
                "thresholds.critical_pct clamped from {} to {}",
                self.thresholds.critical_pct, MAX_CRITICAL
            ));
            self.thresholds.critical_pct = MAX_CRITICAL;
        }
        if self.thresholds.critical_pct < MIN_CRITICAL {
            clamped.push(format!(
                "thresholds.critical_pct clamped from {} to {}",
                self.thresholds.critical_pct, MIN_CRITICAL
            ));
            self.thresholds.critical_pct = MIN_CRITICAL;
        }

        // sampling intervals
        if self.sampling.idle_secs < MIN_SAMPLING_SECS {
            clamped.push(format!(
                "sampling.idle_secs clamped from {} to {}",
                self.sampling.idle_secs, MIN_SAMPLING_SECS
            ));
            self.sampling.idle_secs = MIN_SAMPLING_SECS;
        }
        if self.sampling.elevated_secs < MIN_SAMPLING_SECS {
            clamped.push(format!(
                "sampling.elevated_secs clamped from {} to {}",
                self.sampling.elevated_secs, MIN_SAMPLING_SECS
            ));
            self.sampling.elevated_secs = MIN_SAMPLING_SECS;
        }
        if self.sampling.critical_secs < MIN_SAMPLING_SECS {
            clamped.push(format!(
                "sampling.critical_secs clamped from {} to {}",
                self.sampling.critical_secs, MIN_SAMPLING_SECS
            ));
            self.sampling.critical_secs = MIN_SAMPLING_SECS;
        }

        clamped
    }
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::tempdir;

    fn default_thresholds() -> TelemetryThresholds {
        TelemetryThresholds::default()
    }

    // --- classify_level tests ---

    #[test]
    fn classify_green() {
        let t = default_thresholds(); // yellow=60 → used>=60 → avail<=40
                                      // available = 50% → used = 50% < 60 → Green
        assert_eq!(classify_level(50.0, &t), WatchdogLevel::Green);
    }

    #[test]
    fn classify_yellow() {
        let t = default_thresholds(); // yellow=60
                                      // available = 39% → used = 61% ≥ 60, < 80 → Yellow
        assert_eq!(classify_level(39.0, &t), WatchdogLevel::Yellow);
    }

    #[test]
    fn classify_orange() {
        let t = default_thresholds(); // orange=80
                                      // available = 15% → used = 85% ≥ 80, < 90 → Orange
        assert_eq!(classify_level(15.0, &t), WatchdogLevel::Orange);
    }

    #[test]
    fn classify_red() {
        let t = default_thresholds(); // red=90
                                      // available = 8% → used = 92% ≥ 90, < 95 → Red
        assert_eq!(classify_level(8.0, &t), WatchdogLevel::Red);
    }

    #[test]
    fn classify_critical() {
        let t = default_thresholds(); // critical=95
                                      // available = 3% → used = 97% ≥ 95 → Critical
        assert_eq!(classify_level(3.0, &t), WatchdogLevel::Critical);
    }

    #[test]
    fn classify_boundary_exact() {
        let t = default_thresholds(); // yellow=60
                                      // available=40.0 → used=60.0 → exactly at yellow threshold → Yellow
        assert_eq!(classify_level(40.0, &t), WatchdogLevel::Yellow);
        // available=40.1 → used=59.9 → below yellow → Green
        assert_eq!(classify_level(40.1, &t), WatchdogLevel::Green);
        // available=39.9 → used=60.1 → above yellow → Yellow
        assert_eq!(classify_level(39.9, &t), WatchdogLevel::Yellow);
    }

    // --- SystemSnapshot helpers ---

    #[test]
    fn memory_available_percent_zero_total() {
        let snap = SystemSnapshot {
            timestamp_ms: 0,
            memory_used_bytes: 0,
            memory_total_bytes: 0,
            memory_available_bytes: 0,
            swap_used_bytes: 0,
            swap_total_bytes: 0,
            cpu_usage_percent: 0.0,
            load_average_1m: 0.0,
            disk_used_bytes: 0,
            disk_total_bytes: 0,
            disk_path: "/".to_string(),
        };
        assert_eq!(snap.memory_available_percent(), 100.0);
    }

    #[test]
    fn memory_available_percent_normal() {
        // 2 GB available out of 8 GB total → 25%
        let two_gb: u64 = 2 * 1024 * 1024 * 1024;
        let eight_gb: u64 = 8 * 1024 * 1024 * 1024;
        let snap = SystemSnapshot {
            timestamp_ms: 0,
            memory_used_bytes: eight_gb - two_gb,
            memory_total_bytes: eight_gb,
            memory_available_bytes: two_gb,
            swap_used_bytes: 0,
            swap_total_bytes: 0,
            cpu_usage_percent: 0.0,
            load_average_1m: 0.0,
            disk_used_bytes: 0,
            disk_total_bytes: 0,
            disk_path: "/".to_string(),
        };
        let pct = snap.memory_available_percent();
        assert!((pct - 25.0).abs() < 0.001, "expected ~25%, got {}", pct);
    }

    #[test]
    fn disk_free_mb() {
        // 14 GB free
        let gb: u64 = 1024 * 1024 * 1024;
        let snap = SystemSnapshot {
            timestamp_ms: 0,
            memory_used_bytes: 0,
            memory_total_bytes: 0,
            memory_available_bytes: 0,
            swap_used_bytes: 0,
            swap_total_bytes: 0,
            cpu_usage_percent: 0.0,
            load_average_1m: 0.0,
            disk_used_bytes: 2 * gb,
            disk_total_bytes: 16 * gb,
            disk_path: "/".to_string(),
        };
        assert_eq!(snap.disk_free_mb(), 14 * 1024); // 14 * 1024 MB
    }

    // --- TelemetryConfig::validate_and_clamp ---

    #[test]
    fn validate_clamps_critical_too_high() {
        let mut cfg = TelemetryConfig::default();
        cfg.thresholds.critical_pct = 99; // above MAX_CRITICAL (98)
        let clamped = cfg.validate_and_clamp();
        assert_eq!(cfg.thresholds.critical_pct, MAX_CRITICAL);
        assert!(!clamped.is_empty());
    }

    #[test]
    fn validate_clamps_critical_too_low() {
        let mut cfg = TelemetryConfig::default();
        cfg.thresholds.critical_pct = 50; // below MIN_CRITICAL (80)
        let clamped = cfg.validate_and_clamp();
        assert_eq!(cfg.thresholds.critical_pct, MIN_CRITICAL);
        assert!(!clamped.is_empty());
    }

    #[test]
    fn validate_clamps_sampling_too_fast() {
        let mut cfg = TelemetryConfig::default();
        cfg.sampling.critical_secs = 1; // below MIN_SAMPLING_SECS (2)
        let clamped = cfg.validate_and_clamp();
        assert_eq!(cfg.sampling.critical_secs, MIN_SAMPLING_SECS);
        assert!(!clamped.is_empty());
    }

    #[test]
    fn validate_no_clamps_for_defaults() {
        let mut cfg = TelemetryConfig::default();
        let clamped = cfg.validate_and_clamp();
        assert!(clamped.is_empty(), "defaults should require no clamping");
    }

    #[test]
    fn default_config_values() {
        let cfg = TelemetryConfig::default();
        assert!(cfg.enabled);
        assert_eq!(cfg.thresholds.yellow_pct, 60);
        assert_eq!(cfg.thresholds.orange_pct, 80);
        assert_eq!(cfg.thresholds.red_pct, 90);
        assert_eq!(cfg.thresholds.critical_pct, 95);
        assert_eq!(cfg.thresholds.cpu_warn_pct, 95);
        assert_eq!(cfg.thresholds.cpu_sustained_secs, 60);
        assert_eq!(cfg.thresholds.disk_warn_mb, 1024);
        assert_eq!(cfg.thresholds.disk_critical_mb, 200);
        assert_eq!(cfg.sampling.idle_secs, 30);
        assert_eq!(cfg.sampling.elevated_secs, 10);
        assert_eq!(cfg.sampling.critical_secs, 5);
        assert_eq!(cfg.sampling.ring_capacity, 720);
        assert!(cfg.circuit_breaker.enabled);
        assert_eq!(cfg.circuit_breaker.cooldown_secs, 30);
        assert_eq!(cfg.storage.flush_interval_secs, 60);
        assert_eq!(cfg.storage.retention_days, 7);
        assert_eq!(cfg.monitored_disk_path, "/");
    }

    #[test]
    fn load_config_from_toml() {
        let dir = tempdir().unwrap();
        let path = dir.path().join("telemetry.toml");

        std::fs::write(
            &path,
            r#"
[telemetry]
enabled = true
monitored_disk_path = "/data"

[telemetry.thresholds]
yellow_pct = 70
critical_pct = 95

[telemetry.sampling]
idle_secs = 60
"#,
        )
        .unwrap();

        let cfg = TelemetryConfig::load_or_default(&path);
        assert!(cfg.enabled);
        assert_eq!(cfg.monitored_disk_path, "/data");
        assert_eq!(cfg.thresholds.yellow_pct, 70);
        assert_eq!(cfg.thresholds.critical_pct, 95);
        assert_eq!(cfg.sampling.idle_secs, 60);
        // Fields not specified in TOML fall back to defaults.
        assert_eq!(cfg.thresholds.orange_pct, 80);
    }

    #[test]
    fn load_config_missing_file_returns_default() {
        let dir = tempdir().unwrap();
        let path = dir.path().join("nonexistent.toml");
        let cfg = TelemetryConfig::load_or_default(&path);
        // Should silently return defaults.
        assert!(cfg.enabled);
        assert_eq!(cfg.thresholds.critical_pct, 95);
    }

    #[test]
    fn watchdog_level_ordering() {
        assert!(WatchdogLevel::Green < WatchdogLevel::Yellow);
        assert!(WatchdogLevel::Yellow < WatchdogLevel::Orange);
        assert!(WatchdogLevel::Orange < WatchdogLevel::Red);
        assert!(WatchdogLevel::Red < WatchdogLevel::Critical);
    }
}
