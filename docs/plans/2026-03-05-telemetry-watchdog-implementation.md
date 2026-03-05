# Telemetry Watchdog Implementation Plan

> **For Claude:** REQUIRED SUB-SKILL: Use superpowers:executing-plans to implement this plan task-by-task.

**Goal:** Add a cross-platform telemetry watchdog to the daemon that monitors system resources (memory, CPU, disk), kills nodes at critical thresholds, and gives the agent rich telemetry data with hot-reloadable config.

**Architecture:** New `daemon/telemetry/` module with Sampler (adaptive `sysinfo` reads), CircuitBreaker (threshold evaluation + node kill), and Storage (SQLite cold store). Agent gets 3 new dispatch tools + system prompt injection. Config is hot-reloadable via file watcher.

**Tech Stack:** `sysinfo` crate (cross-platform), `rusqlite` (already in deps), `notify` (already in deps for Soul hot-reload), `tokio` watch/mpsc channels.

**Design doc:** `docs/plans/2026-03-05-telemetry-watchdog-design.md`

---

### Task 1: Add `sysinfo` dependency

**Files:**
- Modify: `crates/bubbaloop/Cargo.toml:51-52` (add after `hostname`)
- Modify: `Cargo.toml` (workspace root, add to `[workspace.dependencies]`)

**Step 1: Check workspace root for sysinfo**

Run: `grep -n sysinfo Cargo.toml`
Expected: no matches

**Step 2: Add sysinfo to workspace dependencies**

Add to `Cargo.toml` workspace `[workspace.dependencies]` section:
```toml
sysinfo = "0.33"
```

**Step 3: Add sysinfo to bubbaloop crate**

Add to `crates/bubbaloop/Cargo.toml` after `hostname = "0.4"`:
```toml
sysinfo.workspace = true
```

**Step 4: Verify it compiles**

Run: `cargo check --lib -p bubbaloop 2>&1 | tail -5`
Expected: Compiling sysinfo, then success

**Step 5: Commit**

```bash
git add Cargo.toml crates/bubbaloop/Cargo.toml Cargo.lock
git commit -m "feat: add sysinfo dependency for telemetry watchdog"
```

---

### Task 2: Types module (`daemon/telemetry/types.rs`)

**Files:**
- Create: `crates/bubbaloop/src/daemon/telemetry/types.rs`

**Step 1: Write tests for types**

Create the file with types and inline tests:

```rust
//! Telemetry data types — snapshots, thresholds, and watchdog levels.

use serde::{Deserialize, Serialize};

/// Watchdog alert level based on available memory.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
pub enum WatchdogLevel {
    /// > 40% available — normal operation.
    Green,
    /// 20-40% available — elevated sampling, agent warned.
    Yellow,
    /// 10-20% available — critical sampling, agent gets urgent alert.
    Orange,
    /// < 10% available — circuit breaker kills largest non-essential node.
    Red,
    /// < 5% available — emergency: kill ALL non-essential nodes.
    Critical,
}

/// Point-in-time system resource snapshot.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SystemSnapshot {
    pub timestamp_ms: i64,
    pub memory_used_bytes: u64,
    pub memory_total_bytes: u64,
    pub memory_available_bytes: u64,
    pub swap_used_bytes: u64,
    pub swap_total_bytes: u64,
    pub cpu_usage_percent: f32,
    pub load_average_1m: f64,
    pub disk_used_bytes: u64,
    pub disk_total_bytes: u64,
    pub disk_path: String,
}

/// Per-process resource snapshot.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProcessSnapshot {
    pub pid: u32,
    pub name: String,
    pub rss_bytes: u64,
    pub cpu_percent: f32,
}

/// Combined telemetry snapshot (system + processes).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TelemetrySnapshot {
    pub system: SystemSnapshot,
    pub processes: Vec<ProcessSnapshot>,
}

/// Alert event pushed to the agent inbox.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type")]
pub enum WatchdogAlert {
    ResourceWarning {
        level: WatchdogLevel,
        memory_available_percent: f32,
        cpu_percent: f32,
        disk_free_mb: u64,
        top_processes: Vec<ProcessSnapshot>,
    },
    NodeKilledByWatchdog {
        node_name: String,
        reason: String,
        rss_mb: u64,
        memory_available_percent: f32,
    },
    ResourceRecovered {
        previous_level: WatchdogLevel,
        memory_available_percent: f32,
    },
    TrendAlert {
        metric: String,
        rate_per_hour: f64,
        current_value: f64,
    },
}

impl SystemSnapshot {
    /// Memory available as a percentage (0.0 - 100.0).
    pub fn memory_available_percent(&self) -> f32 {
        if self.memory_total_bytes == 0 {
            return 100.0;
        }
        (self.memory_available_bytes as f64 / self.memory_total_bytes as f64 * 100.0) as f32
    }

    /// Disk free in megabytes.
    pub fn disk_free_mb(&self) -> u64 {
        (self.disk_total_bytes.saturating_sub(self.disk_used_bytes)) / (1024 * 1024)
    }
}

/// Classify a memory-available percentage into a watchdog level.
pub fn classify_level(
    available_percent: f32,
    thresholds: &TelemetryThresholds,
) -> WatchdogLevel {
    let used_percent = 100.0 - available_percent;
    if used_percent >= thresholds.critical_memory_percent as f32 {
        WatchdogLevel::Critical
    } else if used_percent >= thresholds.red_memory_percent as f32 {
        WatchdogLevel::Red
    } else if used_percent >= thresholds.orange_memory_percent as f32 {
        WatchdogLevel::Orange
    } else if used_percent >= thresholds.yellow_memory_percent as f32 {
        WatchdogLevel::Yellow
    } else {
        WatchdogLevel::Green
    }
}

/// Telemetry thresholds (hot-reloadable).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TelemetryThresholds {
    /// Memory used % to trigger Yellow (default: 60).
    #[serde(default = "default_yellow")]
    pub yellow_memory_percent: u8,
    /// Memory used % to trigger Orange (default: 80).
    #[serde(default = "default_orange")]
    pub orange_memory_percent: u8,
    /// Memory used % to trigger Red (default: 90).
    #[serde(default = "default_red")]
    pub red_memory_percent: u8,
    /// Memory used % to trigger Critical (default: 95).
    #[serde(default = "default_critical")]
    pub critical_memory_percent: u8,
    /// CPU % to warn agent (sustained, default: 95).
    #[serde(default = "default_cpu_warn")]
    pub cpu_warn_percent: u8,
    /// Seconds CPU must be above threshold to trigger warn (default: 60).
    #[serde(default = "default_cpu_sustained")]
    pub cpu_warn_sustained_secs: u64,
    /// Disk free MB to warn agent (default: 1024).
    #[serde(default = "default_disk_warn")]
    pub disk_warn_mb: u64,
    /// Disk free MB for critical action (default: 200).
    #[serde(default = "default_disk_critical")]
    pub disk_critical_mb: u64,
}

fn default_yellow() -> u8 { 60 }
fn default_orange() -> u8 { 80 }
fn default_red() -> u8 { 90 }
fn default_critical() -> u8 { 95 }
fn default_cpu_warn() -> u8 { 95 }
fn default_cpu_sustained() -> u64 { 60 }
fn default_disk_warn() -> u64 { 1024 }
fn default_disk_critical() -> u64 { 200 }

impl Default for TelemetryThresholds {
    fn default() -> Self {
        Self {
            yellow_memory_percent: default_yellow(),
            orange_memory_percent: default_orange(),
            red_memory_percent: default_red(),
            critical_memory_percent: default_critical(),
            cpu_warn_percent: default_cpu_warn(),
            cpu_warn_sustained_secs: default_cpu_sustained(),
            disk_warn_mb: default_disk_warn(),
            disk_critical_mb: default_disk_critical(),
        }
    }
}

/// Sampling intervals (hot-reloadable).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SamplingConfig {
    #[serde(default = "default_idle_interval")]
    pub idle_interval_secs: u64,
    #[serde(default = "default_elevated_interval")]
    pub elevated_interval_secs: u64,
    #[serde(default = "default_critical_interval")]
    pub critical_interval_secs: u64,
    #[serde(default = "default_ring_capacity")]
    pub ring_buffer_capacity: usize,
}

fn default_idle_interval() -> u64 { 30 }
fn default_elevated_interval() -> u64 { 10 }
fn default_critical_interval() -> u64 { 5 }
fn default_ring_capacity() -> usize { 720 }

impl Default for SamplingConfig {
    fn default() -> Self {
        Self {
            idle_interval_secs: default_idle_interval(),
            elevated_interval_secs: default_elevated_interval(),
            critical_interval_secs: default_critical_interval(),
            ring_buffer_capacity: default_ring_capacity(),
        }
    }
}

/// Circuit breaker config.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CircuitBreakerConfig {
    #[serde(default = "default_true")]
    pub enabled: bool,
    #[serde(default = "default_cooldown")]
    pub cooldown_secs: u64,
}

fn default_true() -> bool { true }
fn default_cooldown() -> u64 { 30 }

impl Default for CircuitBreakerConfig {
    fn default() -> Self {
        Self {
            enabled: default_true(),
            cooldown_secs: default_cooldown(),
        }
    }
}

/// Storage config.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StorageConfig {
    #[serde(default = "default_flush_interval")]
    pub flush_interval_secs: u64,
    #[serde(default = "default_retention")]
    pub retention_days: u32,
}

fn default_flush_interval() -> u64 { 60 }
fn default_retention() -> u32 { 7 }

impl Default for StorageConfig {
    fn default() -> Self {
        Self {
            flush_interval_secs: default_flush_interval(),
            retention_days: default_retention(),
        }
    }
}

/// Top-level telemetry config (maps to `~/.bubbaloop/telemetry.toml`).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TelemetryConfig {
    #[serde(default = "default_true")]
    pub enabled: bool,
    #[serde(default)]
    pub sampling: SamplingConfig,
    #[serde(default)]
    pub thresholds: TelemetryThresholds,
    #[serde(default)]
    pub circuit_breaker: CircuitBreakerConfig,
    #[serde(default)]
    pub storage: StorageConfig,
    #[serde(default = "default_disk_path")]
    pub monitored_disk_path: String,
}

fn default_disk_path() -> String { "~/.bubbaloop".to_string() }

impl Default for TelemetryConfig {
    fn default() -> Self {
        Self {
            enabled: true,
            sampling: SamplingConfig::default(),
            thresholds: TelemetryThresholds::default(),
            circuit_breaker: CircuitBreakerConfig::default(),
            storage: StorageConfig::default(),
            monitored_disk_path: default_disk_path(),
        }
    }
}

/// Guardrail constants for agent config updates.
pub const MIN_CRITICAL_MEMORY_PERCENT: u8 = 80;
pub const MAX_CRITICAL_MEMORY_PERCENT: u8 = 98;
pub const MIN_SAMPLING_INTERVAL_SECS: u64 = 2;

impl TelemetryConfig {
    /// Load from file or return default config.
    pub fn load_or_default(path: &std::path::Path) -> Self {
        match std::fs::read_to_string(path) {
            Ok(content) => {
                // The TOML file wraps everything under [telemetry]
                #[derive(Deserialize)]
                struct Wrapper {
                    #[serde(default)]
                    telemetry: TelemetryConfig,
                }
                match toml::from_str::<Wrapper>(&content) {
                    Ok(w) => w.telemetry,
                    Err(e) => {
                        log::warn!("Failed to parse telemetry.toml: {}, using defaults", e);
                        Self::default()
                    }
                }
            }
            Err(_) => Self::default(),
        }
    }

    /// Validate and clamp guardrails. Returns list of clamped fields.
    pub fn validate_and_clamp(&mut self) -> Vec<String> {
        let mut clamped = Vec::new();

        if self.thresholds.critical_memory_percent > MAX_CRITICAL_MEMORY_PERCENT {
            self.thresholds.critical_memory_percent = MAX_CRITICAL_MEMORY_PERCENT;
            clamped.push(format!(
                "critical_memory_percent clamped to {}",
                MAX_CRITICAL_MEMORY_PERCENT
            ));
        }
        if self.thresholds.critical_memory_percent < MIN_CRITICAL_MEMORY_PERCENT {
            self.thresholds.critical_memory_percent = MIN_CRITICAL_MEMORY_PERCENT;
            clamped.push(format!(
                "critical_memory_percent clamped to {}",
                MIN_CRITICAL_MEMORY_PERCENT
            ));
        }
        if self.sampling.critical_interval_secs < MIN_SAMPLING_INTERVAL_SECS {
            self.sampling.critical_interval_secs = MIN_SAMPLING_INTERVAL_SECS;
            clamped.push(format!(
                "critical_interval_secs clamped to {}",
                MIN_SAMPLING_INTERVAL_SECS
            ));
        }
        if self.sampling.elevated_interval_secs < MIN_SAMPLING_INTERVAL_SECS {
            self.sampling.elevated_interval_secs = MIN_SAMPLING_INTERVAL_SECS;
            clamped.push("elevated_interval_secs clamped to 2".to_string());
        }
        if self.sampling.idle_interval_secs < MIN_SAMPLING_INTERVAL_SECS {
            self.sampling.idle_interval_secs = MIN_SAMPLING_INTERVAL_SECS;
            clamped.push("idle_interval_secs clamped to 2".to_string());
        }

        clamped
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn classify_green() {
        let t = TelemetryThresholds::default();
        assert_eq!(classify_level(50.0, &t), WatchdogLevel::Green);
    }

    #[test]
    fn classify_yellow() {
        let t = TelemetryThresholds::default();
        // 60% used = 40% available
        assert_eq!(classify_level(40.0, &t), WatchdogLevel::Yellow);
    }

    #[test]
    fn classify_orange() {
        let t = TelemetryThresholds::default();
        // 80% used = 20% available
        assert_eq!(classify_level(20.0, &t), WatchdogLevel::Orange);
    }

    #[test]
    fn classify_red() {
        let t = TelemetryThresholds::default();
        // 90% used = 10% available
        assert_eq!(classify_level(10.0, &t), WatchdogLevel::Red);
    }

    #[test]
    fn classify_critical() {
        let t = TelemetryThresholds::default();
        // 95% used = 5% available
        assert_eq!(classify_level(5.0, &t), WatchdogLevel::Critical);
    }

    #[test]
    fn classify_boundary_exact() {
        let t = TelemetryThresholds::default();
        // Exactly 60% used = 40% available → Yellow (boundary)
        assert_eq!(classify_level(40.0, &t), WatchdogLevel::Yellow);
        // 39.9% available = 60.1% used → Yellow
        assert_eq!(classify_level(39.9, &t), WatchdogLevel::Yellow);
        // 40.1% available = 59.9% used → Green
        assert_eq!(classify_level(40.1, &t), WatchdogLevel::Green);
    }

    #[test]
    fn memory_available_percent_zero_total() {
        let s = SystemSnapshot {
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
            disk_path: String::new(),
        };
        assert_eq!(s.memory_available_percent(), 100.0);
    }

    #[test]
    fn memory_available_percent_normal() {
        let s = SystemSnapshot {
            timestamp_ms: 0,
            memory_used_bytes: 6 * 1024 * 1024 * 1024,
            memory_total_bytes: 8 * 1024 * 1024 * 1024,
            memory_available_bytes: 2 * 1024 * 1024 * 1024,
            swap_used_bytes: 0,
            swap_total_bytes: 0,
            cpu_usage_percent: 0.0,
            load_average_1m: 0.0,
            disk_used_bytes: 0,
            disk_total_bytes: 0,
            disk_path: String::new(),
        };
        assert!((s.memory_available_percent() - 25.0).abs() < 0.1);
    }

    #[test]
    fn disk_free_mb() {
        let s = SystemSnapshot {
            timestamp_ms: 0,
            memory_used_bytes: 0,
            memory_total_bytes: 0,
            memory_available_bytes: 0,
            swap_used_bytes: 0,
            swap_total_bytes: 0,
            cpu_usage_percent: 0.0,
            load_average_1m: 0.0,
            disk_used_bytes: 50 * 1024 * 1024 * 1024,
            disk_total_bytes: 64 * 1024 * 1024 * 1024,
            disk_path: String::new(),
        };
        assert_eq!(s.disk_free_mb(), 14 * 1024); // 14 GB
    }

    #[test]
    fn validate_clamps_critical_too_high() {
        let mut config = TelemetryConfig::default();
        config.thresholds.critical_memory_percent = 99;
        let clamped = config.validate_and_clamp();
        assert_eq!(config.thresholds.critical_memory_percent, 98);
        assert!(!clamped.is_empty());
    }

    #[test]
    fn validate_clamps_critical_too_low() {
        let mut config = TelemetryConfig::default();
        config.thresholds.critical_memory_percent = 50;
        let clamped = config.validate_and_clamp();
        assert_eq!(config.thresholds.critical_memory_percent, 80);
        assert!(!clamped.is_empty());
    }

    #[test]
    fn validate_clamps_sampling_too_fast() {
        let mut config = TelemetryConfig::default();
        config.sampling.critical_interval_secs = 1;
        let clamped = config.validate_and_clamp();
        assert_eq!(config.sampling.critical_interval_secs, 2);
        assert!(!clamped.is_empty());
    }

    #[test]
    fn validate_no_clamps_for_defaults() {
        let mut config = TelemetryConfig::default();
        let clamped = config.validate_and_clamp();
        assert!(clamped.is_empty());
    }

    #[test]
    fn default_config_values() {
        let config = TelemetryConfig::default();
        assert!(config.enabled);
        assert_eq!(config.sampling.idle_interval_secs, 30);
        assert_eq!(config.sampling.elevated_interval_secs, 10);
        assert_eq!(config.sampling.critical_interval_secs, 5);
        assert_eq!(config.sampling.ring_buffer_capacity, 720);
        assert_eq!(config.thresholds.yellow_memory_percent, 60);
        assert_eq!(config.thresholds.red_memory_percent, 90);
        assert_eq!(config.thresholds.critical_memory_percent, 95);
        assert!(config.circuit_breaker.enabled);
        assert_eq!(config.circuit_breaker.cooldown_secs, 30);
        assert_eq!(config.storage.retention_days, 7);
    }

    #[test]
    fn load_config_from_toml() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("telemetry.toml");
        std::fs::write(&path, r#"
[telemetry]
enabled = true

[telemetry.thresholds]
yellow_memory_percent = 50
red_memory_percent = 85

[telemetry.sampling]
idle_interval_secs = 15
"#).unwrap();
        let config = TelemetryConfig::load_or_default(&path);
        assert_eq!(config.thresholds.yellow_memory_percent, 50);
        assert_eq!(config.thresholds.red_memory_percent, 85);
        assert_eq!(config.sampling.idle_interval_secs, 15);
        // Defaults for unspecified fields
        assert_eq!(config.thresholds.critical_memory_percent, 95);
    }

    #[test]
    fn load_config_missing_file_returns_default() {
        let config = TelemetryConfig::load_or_default(std::path::Path::new("/nonexistent"));
        assert!(config.enabled);
        assert_eq!(config.thresholds.yellow_memory_percent, 60);
    }

    #[test]
    fn watchdog_level_ordering() {
        assert!(WatchdogLevel::Green < WatchdogLevel::Yellow);
        assert!(WatchdogLevel::Yellow < WatchdogLevel::Orange);
        assert!(WatchdogLevel::Orange < WatchdogLevel::Red);
        assert!(WatchdogLevel::Red < WatchdogLevel::Critical);
    }
}
```

**Step 2: Run tests**

Run: `cargo test --lib -p bubbaloop telemetry::types 2>&1 | tail -20`
Expected: all tests pass

**Step 3: Commit**

```bash
git add crates/bubbaloop/src/daemon/telemetry/
git commit -m "feat(telemetry): add types module with snapshots, thresholds, config, and watchdog levels"
```

---

### Task 3: Sampler module (`daemon/telemetry/sampler.rs`)

**Files:**
- Create: `crates/bubbaloop/src/daemon/telemetry/sampler.rs`

**Step 1: Write the sampler**

```rust
//! Adaptive system metrics sampler using `sysinfo`.
//!
//! Reads memory, CPU, disk, and per-process stats at an interval
//! determined by the current watchdog level. Feeds a ring buffer.

use super::types::{
    classify_level, ProcessSnapshot, SamplingConfig, SystemSnapshot,
    TelemetryConfig, TelemetrySnapshot, TelemetryThresholds, WatchdogLevel,
};
use std::collections::VecDeque;
use std::path::Path;
use std::sync::Arc;
use sysinfo::{Disks, System};
use tokio::sync::RwLock;

/// Ring buffer of telemetry snapshots.
pub struct RingBuffer {
    buffer: VecDeque<TelemetrySnapshot>,
    capacity: usize,
}

impl RingBuffer {
    pub fn new(capacity: usize) -> Self {
        Self {
            buffer: VecDeque::with_capacity(capacity),
            capacity,
        }
    }

    pub fn push(&mut self, snapshot: TelemetrySnapshot) {
        if self.buffer.len() >= self.capacity {
            self.buffer.pop_front();
        }
        self.buffer.push_back(snapshot);
    }

    pub fn latest(&self) -> Option<&TelemetrySnapshot> {
        self.buffer.back()
    }

    pub fn iter(&self) -> impl Iterator<Item = &TelemetrySnapshot> {
        self.buffer.iter()
    }

    pub fn len(&self) -> usize {
        self.buffer.len()
    }
}

/// Take a single system snapshot.
pub fn take_snapshot(sys: &mut System, disks: &mut Disks, disk_path: &str) -> TelemetrySnapshot {
    sys.refresh_memory();
    sys.refresh_cpu_usage();
    sys.refresh_processes(sysinfo::ProcessesToUpdate::All, true);

    disks.refresh(true);

    let now_ms = chrono::Utc::now().timestamp_millis();

    // Resolve disk path (expand ~)
    let resolved_path = if disk_path.starts_with("~/") {
        dirs::home_dir()
            .map(|h| h.join(&disk_path[2..]))
            .unwrap_or_else(|| std::path::PathBuf::from(disk_path))
    } else {
        std::path::PathBuf::from(disk_path)
    };

    // Find the disk that contains the monitored path
    let (disk_used, disk_total, disk_mount) = find_disk_for_path(disks, &resolved_path);

    let system = SystemSnapshot {
        timestamp_ms: now_ms,
        memory_used_bytes: sys.used_memory(),
        memory_total_bytes: sys.total_memory(),
        memory_available_bytes: sys.available_memory(),
        swap_used_bytes: sys.used_swap(),
        swap_total_bytes: sys.total_swap(),
        cpu_usage_percent: sys.global_cpu_usage(),
        load_average_1m: sysinfo::System::load_average().one,
        disk_used_bytes: disk_used,
        disk_total_bytes: disk_total,
        disk_path: disk_mount,
    };

    // Collect per-process snapshots (top 20 by RSS)
    let mut processes: Vec<ProcessSnapshot> = sys
        .processes()
        .values()
        .map(|p| ProcessSnapshot {
            pid: p.pid().as_u32(),
            name: p.name().to_string_lossy().to_string(),
            rss_bytes: p.memory(),
            cpu_percent: p.cpu_usage(),
        })
        .collect();
    processes.sort_by(|a, b| b.rss_bytes.cmp(&a.rss_bytes));
    processes.truncate(20);

    TelemetrySnapshot { system, processes }
}

/// Find the disk that contains the given path.
fn find_disk_for_path(disks: &Disks, path: &Path) -> (u64, u64, String) {
    let mut best_match: Option<&sysinfo::Disk> = None;
    let mut best_len = 0;

    for disk in disks.list() {
        let mount = disk.mount_point();
        if path.starts_with(mount) {
            let mount_len = mount.as_os_str().len();
            if mount_len > best_len {
                best_len = mount_len;
                best_match = Some(disk);
            }
        }
    }

    match best_match {
        Some(disk) => {
            let total = disk.total_space();
            let available = disk.available_space();
            let used = total.saturating_sub(available);
            (used, total, disk.mount_point().to_string_lossy().to_string())
        }
        None => (0, 0, "unknown".to_string()),
    }
}

/// Determine the sampling interval based on the current watchdog level.
pub fn interval_for_level(level: WatchdogLevel, config: &SamplingConfig) -> u64 {
    match level {
        WatchdogLevel::Green => config.idle_interval_secs,
        WatchdogLevel::Yellow => config.elevated_interval_secs,
        WatchdogLevel::Orange | WatchdogLevel::Red | WatchdogLevel::Critical => {
            config.critical_interval_secs
        }
    }
}

/// Run the sampler loop. Writes snapshots to the shared ring buffer.
///
/// Returns the alert channel receiver for the circuit breaker.
pub async fn run_sampler(
    config: Arc<RwLock<TelemetryConfig>>,
    ring: Arc<RwLock<RingBuffer>>,
    alert_tx: tokio::sync::mpsc::Sender<(TelemetrySnapshot, WatchdogLevel)>,
    mut shutdown_rx: tokio::sync::watch::Receiver<()>,
) {
    let mut sys = System::new();
    let mut disks = Disks::new();

    // Initial refresh to get baseline CPU readings
    sys.refresh_cpu_usage();
    tokio::time::sleep(std::time::Duration::from_millis(500)).await;

    let mut current_level = WatchdogLevel::Green;

    loop {
        let cfg = config.read().await.clone();
        if !cfg.enabled {
            tokio::select! {
                _ = tokio::time::sleep(std::time::Duration::from_secs(cfg.sampling.idle_interval_secs)) => continue,
                _ = shutdown_rx.changed() => break,
            }
        }

        let snapshot = take_snapshot(&mut sys, &mut disks, &cfg.monitored_disk_path);
        let new_level = classify_level(
            snapshot.system.memory_available_percent(),
            &cfg.thresholds,
        );

        // Push to ring buffer
        {
            let mut ring = ring.write().await;
            ring.push(snapshot.clone());
        }

        // Send alert if level is Yellow or above, or if level changed
        if new_level >= WatchdogLevel::Yellow || new_level != current_level {
            let _ = alert_tx.try_send((snapshot.clone(), new_level));
        }

        if new_level != current_level {
            log::info!(
                "[WATCHDOG] Level changed: {:?} -> {:?} (memory available: {:.1}%)",
                current_level,
                new_level,
                snapshot.system.memory_available_percent()
            );
        }
        current_level = new_level;

        let interval = interval_for_level(new_level, &cfg.sampling);
        tokio::select! {
            _ = tokio::time::sleep(std::time::Duration::from_secs(interval)) => {},
            _ = shutdown_rx.changed() => break,
        }
    }

    log::info!("[WATCHDOG] Sampler stopped");
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ring_buffer_push_and_capacity() {
        let mut ring = RingBuffer::new(3);
        assert_eq!(ring.len(), 0);

        for i in 0..5 {
            ring.push(TelemetrySnapshot {
                system: SystemSnapshot {
                    timestamp_ms: i,
                    memory_used_bytes: 0,
                    memory_total_bytes: 100,
                    memory_available_bytes: 50,
                    swap_used_bytes: 0,
                    swap_total_bytes: 0,
                    cpu_usage_percent: 0.0,
                    load_average_1m: 0.0,
                    disk_used_bytes: 0,
                    disk_total_bytes: 0,
                    disk_path: String::new(),
                },
                processes: vec![],
            });
        }
        assert_eq!(ring.len(), 3);
        assert_eq!(ring.latest().unwrap().system.timestamp_ms, 4);
        // Oldest should be 2 (0 and 1 were evicted)
        assert_eq!(ring.iter().next().unwrap().system.timestamp_ms, 2);
    }

    #[test]
    fn interval_for_level_values() {
        let config = SamplingConfig::default();
        assert_eq!(interval_for_level(WatchdogLevel::Green, &config), 30);
        assert_eq!(interval_for_level(WatchdogLevel::Yellow, &config), 10);
        assert_eq!(interval_for_level(WatchdogLevel::Orange, &config), 5);
        assert_eq!(interval_for_level(WatchdogLevel::Red, &config), 5);
        assert_eq!(interval_for_level(WatchdogLevel::Critical, &config), 5);
    }
}
```

**Step 2: Run tests**

Run: `cargo test --lib -p bubbaloop telemetry::sampler 2>&1 | tail -10`
Expected: all tests pass

**Step 3: Commit**

```bash
git add crates/bubbaloop/src/daemon/telemetry/sampler.rs
git commit -m "feat(telemetry): add adaptive sampler with ring buffer and sysinfo integration"
```

---

### Task 4: Storage module (`daemon/telemetry/storage.rs`)

**Files:**
- Create: `crates/bubbaloop/src/daemon/telemetry/storage.rs`

**Step 1: Write SQLite cold storage**

```rust
//! SQLite cold storage for telemetry snapshots.
//!
//! Batch-flushes ring buffer snapshots periodically and handles retention pruning.

use super::sampler::RingBuffer;
use super::types::{StorageConfig, TelemetryConfig, TelemetrySnapshot};
use rusqlite::Connection;
use std::path::Path;
use std::sync::Arc;
use tokio::sync::RwLock;

/// Initialize the telemetry database.
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
pub fn insert_batch(conn: &Connection, snapshots: &[TelemetrySnapshot]) -> Result<usize, rusqlite::Error> {
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

/// Prune snapshots older than retention_days.
pub fn prune(conn: &Connection, retention_days: u32) -> Result<usize, rusqlite::Error> {
    let cutoff_ms = chrono::Utc::now().timestamp_millis()
        - (retention_days as i64 * 24 * 60 * 60 * 1000);
    let deleted = conn.execute(
        "DELETE FROM system_telemetry WHERE timestamp_ms < ?1",
        rusqlite::params![cutoff_ms],
    )?;
    Ok(deleted)
}

/// Query snapshots within a time range, returning downsampled results.
/// Returns at most `max_points` evenly spaced entries.
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

    // Downsample: pick evenly spaced points
    let step = rows.len() as f64 / max_points as f64;
    let downsampled: Vec<TelemetrySnapshot> = (0..max_points)
        .map(|i| {
            let idx = (i as f64 * step) as usize;
            rows[idx.min(rows.len() - 1)].clone()
        })
        .collect();

    Ok(downsampled)
}

/// Run the storage flush loop.
pub async fn run_storage_flusher(
    config: Arc<RwLock<TelemetryConfig>>,
    ring: Arc<RwLock<RingBuffer>>,
    db_path: std::path::PathBuf,
    mut shutdown_rx: tokio::sync::watch::Receiver<()>,
) {
    let conn = match init_db(&db_path) {
        Ok(c) => c,
        Err(e) => {
            log::error!("[WATCHDOG] Failed to open telemetry DB: {}", e);
            return;
        }
    };

    let mut last_flush_count: usize = 0;
    let mut prune_counter: u32 = 0;

    loop {
        let cfg = config.read().await.clone();
        let flush_interval = cfg.storage.flush_interval_secs;

        tokio::select! {
            _ = tokio::time::sleep(std::time::Duration::from_secs(flush_interval)) => {},
            _ = shutdown_rx.changed() => break,
        }

        // Collect new snapshots from ring buffer since last flush
        let snapshots: Vec<TelemetrySnapshot> = {
            let ring = ring.read().await;
            ring.iter()
                .skip(last_flush_count)
                .cloned()
                .collect()
        };

        if !snapshots.is_empty() {
            match insert_batch(&conn, &snapshots) {
                Ok(n) => {
                    log::debug!("[WATCHDOG] Flushed {} snapshots to telemetry DB", n);
                    let ring = ring.read().await;
                    last_flush_count = ring.len();
                }
                Err(e) => log::warn!("[WATCHDOG] Failed to flush telemetry: {}", e),
            }
        }

        // Prune old data roughly once per hour (60 flush cycles at 60s interval)
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

        let snapshots = vec![test_snapshot(1000), test_snapshot(2000), test_snapshot(3000)];
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

        // Insert one very old snapshot and one recent
        let old_ms = chrono::Utc::now().timestamp_millis() - (10 * 24 * 60 * 60 * 1000); // 10 days ago
        let recent_ms = chrono::Utc::now().timestamp_millis();
        let snapshots = vec![test_snapshot(old_ms), test_snapshot(recent_ms)];
        insert_batch(&conn, &snapshots).unwrap();

        let deleted = prune(&conn, 7).unwrap();
        assert_eq!(deleted, 1);

        let remaining = query_range(&conn, 0, i64::MAX, 100).unwrap();
        assert_eq!(remaining.len(), 1);
    }
}
```

**Step 2: Run tests**

Run: `cargo test --lib -p bubbaloop telemetry::storage 2>&1 | tail -10`
Expected: all tests pass

**Step 3: Commit**

```bash
git add crates/bubbaloop/src/daemon/telemetry/storage.rs
git commit -m "feat(telemetry): add SQLite cold storage with batch flush and retention pruning"
```

---

### Task 5: Circuit breaker module (`daemon/telemetry/circuit_breaker.rs`)

**Files:**
- Create: `crates/bubbaloop/src/daemon/telemetry/circuit_breaker.rs`

**Step 1: Write circuit breaker**

This module receives alerts from the sampler and decides whether to kill nodes.

```rust
//! Circuit breaker — kills nodes when memory reaches critical thresholds.
//!
//! Operates independently from the agent (no LLM in the critical path).
//! After killing a node, notifies the agent via alert channel.

use super::types::{
    CircuitBreakerConfig, TelemetryConfig, TelemetrySnapshot, TelemetryThresholds,
    WatchdogAlert, WatchdogLevel,
};
use crate::daemon::NodeManager;
use std::sync::Arc;
use std::time::Instant;
use tokio::sync::RwLock;

/// Run the circuit breaker loop.
///
/// Receives (snapshot, level) from the sampler's alert channel and acts.
pub async fn run_circuit_breaker(
    config: Arc<RwLock<TelemetryConfig>>,
    node_manager: Arc<NodeManager>,
    mut alert_rx: tokio::sync::mpsc::Receiver<(TelemetrySnapshot, WatchdogLevel)>,
    watchdog_alert_tx: tokio::sync::broadcast::Sender<WatchdogAlert>,
    mut shutdown_rx: tokio::sync::watch::Receiver<()>,
) {
    let mut last_kill_time: Option<Instant> = None;
    let mut previous_level = WatchdogLevel::Green;
    let mut cpu_above_threshold_since: Option<Instant> = None;

    loop {
        tokio::select! {
            Some((snapshot, level)) = alert_rx.recv() => {
                let cfg = config.read().await.clone();

                if !cfg.circuit_breaker.enabled {
                    continue;
                }

                // Check for recovery
                if level == WatchdogLevel::Green && previous_level >= WatchdogLevel::Yellow {
                    let _ = watchdog_alert_tx.send(WatchdogAlert::ResourceRecovered {
                        previous_level,
                        memory_available_percent: snapshot.system.memory_available_percent(),
                    });
                }

                // CPU sustained check
                if snapshot.system.cpu_usage_percent > cfg.thresholds.cpu_warn_percent as f32 {
                    match cpu_above_threshold_since {
                        None => cpu_above_threshold_since = Some(Instant::now()),
                        Some(since) if since.elapsed().as_secs() >= cfg.thresholds.cpu_warn_sustained_secs => {
                            let _ = watchdog_alert_tx.send(WatchdogAlert::ResourceWarning {
                                level,
                                memory_available_percent: snapshot.system.memory_available_percent(),
                                cpu_percent: snapshot.system.cpu_usage_percent,
                                disk_free_mb: snapshot.system.disk_free_mb(),
                                top_processes: snapshot.processes.iter().take(5).cloned().collect(),
                            });
                            cpu_above_threshold_since = None; // Reset after warning
                        }
                        _ => {}
                    }
                } else {
                    cpu_above_threshold_since = None;
                }

                // Disk check
                let disk_free_mb = snapshot.system.disk_free_mb();
                if disk_free_mb < cfg.thresholds.disk_warn_mb {
                    let _ = watchdog_alert_tx.send(WatchdogAlert::ResourceWarning {
                        level,
                        memory_available_percent: snapshot.system.memory_available_percent(),
                        cpu_percent: snapshot.system.cpu_usage_percent,
                        disk_free_mb,
                        top_processes: snapshot.processes.iter().take(5).cloned().collect(),
                    });
                }

                // Memory threshold actions
                match level {
                    WatchdogLevel::Yellow | WatchdogLevel::Orange => {
                        let _ = watchdog_alert_tx.send(WatchdogAlert::ResourceWarning {
                            level,
                            memory_available_percent: snapshot.system.memory_available_percent(),
                            cpu_percent: snapshot.system.cpu_usage_percent,
                            disk_free_mb: snapshot.system.disk_free_mb(),
                            top_processes: snapshot.processes.iter().take(5).cloned().collect(),
                        });
                    }
                    WatchdogLevel::Red => {
                        // Kill largest non-essential node (with cooldown)
                        if should_kill(&last_kill_time, cfg.circuit_breaker.cooldown_secs) {
                            if let Some((name, rss)) = find_kill_candidate(&node_manager, &snapshot, false).await {
                                log::warn!(
                                    "[WATCHDOG] RED: Stopping node '{}' (RSS {}MB, memory available {:.1}%)",
                                    name, rss / (1024 * 1024), snapshot.system.memory_available_percent()
                                );
                                kill_node(&node_manager, &name, &watchdog_alert_tx, &snapshot, rss).await;
                                last_kill_time = Some(Instant::now());
                            }
                        }
                    }
                    WatchdogLevel::Critical => {
                        // Kill ALL non-essential nodes
                        log::error!(
                            "[WATCHDOG] CRITICAL: Memory at {:.1}% available — stopping all non-essential nodes",
                            snapshot.system.memory_available_percent()
                        );
                        kill_all_non_essential(&node_manager, &watchdog_alert_tx, &snapshot).await;
                        last_kill_time = Some(Instant::now());
                    }
                    WatchdogLevel::Green => {}
                }

                previous_level = level;
            }
            _ = shutdown_rx.changed() => break,
        }
    }

    log::info!("[WATCHDOG] Circuit breaker stopped");
}

fn should_kill(last_kill: &Option<Instant>, cooldown_secs: u64) -> bool {
    match last_kill {
        None => true,
        Some(t) => t.elapsed().as_secs() >= cooldown_secs,
    }
}

/// Find the largest running node by RSS. If `include_essential` is false,
/// skip nodes marked as essential in their manifest.
async fn find_kill_candidate(
    node_manager: &NodeManager,
    snapshot: &TelemetrySnapshot,
    include_essential: bool,
) -> Option<(String, u64)> {
    let node_list = node_manager.get_node_list().await;
    let running_names: Vec<String> = node_list
        .nodes
        .iter()
        .filter(|n| n.status == "Running" || n.status == "running")
        .map(|n| n.name.clone())
        .collect();

    // Match running nodes to process snapshots by name
    let mut candidates: Vec<(String, u64)> = Vec::new();
    for proc in &snapshot.processes {
        for node_name in &running_names {
            // Match process name to node name (process names often contain the node name)
            if proc.name.contains(node_name.as_str()) || node_name.contains(proc.name.as_str()) {
                if !include_essential {
                    // TODO: Check node manifest for `essential: true` field
                    // For now, treat all nodes as non-essential
                }
                candidates.push((node_name.clone(), proc.rss_bytes));
            }
        }
    }

    // Sort by RSS descending, return largest
    candidates.sort_by(|a, b| b.1.cmp(&a.1));
    candidates.into_iter().next()
}

async fn kill_node(
    node_manager: &NodeManager,
    name: &str,
    alert_tx: &tokio::sync::broadcast::Sender<WatchdogAlert>,
    snapshot: &TelemetrySnapshot,
    rss_bytes: u64,
) {
    match node_manager.stop_node(name).await {
        Ok(_) => {
            log::info!("[WATCHDOG] Successfully stopped node '{}'", name);
            let _ = alert_tx.send(WatchdogAlert::NodeKilledByWatchdog {
                node_name: name.to_string(),
                reason: format!(
                    "Memory pressure: {:.1}% available",
                    snapshot.system.memory_available_percent()
                ),
                rss_mb: rss_bytes / (1024 * 1024),
                memory_available_percent: snapshot.system.memory_available_percent(),
            });
        }
        Err(e) => {
            log::error!("[WATCHDOG] Failed to stop node '{}': {}", name, e);
        }
    }
}

async fn kill_all_non_essential(
    node_manager: &NodeManager,
    alert_tx: &tokio::sync::broadcast::Sender<WatchdogAlert>,
    snapshot: &TelemetrySnapshot,
) {
    let node_list = node_manager.get_node_list().await;
    for node in &node_list.nodes {
        if node.status == "Running" || node.status == "running" {
            // TODO: Check essential flag in manifest
            kill_node(node_manager, &node.name, alert_tx, snapshot, 0).await;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn should_kill_no_previous() {
        assert!(should_kill(&None, 30));
    }

    #[test]
    fn should_kill_after_cooldown() {
        let past = Instant::now() - std::time::Duration::from_secs(31);
        assert!(should_kill(&Some(past), 30));
    }

    #[test]
    fn should_not_kill_during_cooldown() {
        let recent = Instant::now();
        assert!(!should_kill(&Some(recent), 30));
    }
}
```

**Step 2: Run tests**

Run: `cargo test --lib -p bubbaloop telemetry::circuit_breaker 2>&1 | tail -10`
Expected: all tests pass

**Step 3: Commit**

```bash
git add crates/bubbaloop/src/daemon/telemetry/circuit_breaker.rs
git commit -m "feat(telemetry): add circuit breaker with cooldown and node kill logic"
```

---

### Task 6: Telemetry service module (`daemon/telemetry/mod.rs`)

**Files:**
- Create: `crates/bubbaloop/src/daemon/telemetry/mod.rs`
- Modify: `crates/bubbaloop/src/daemon/mod.rs:15-18` (add `pub mod telemetry;`)

**Step 1: Write the TelemetryService**

```rust
//! Telemetry watchdog — cross-platform resource monitoring for the daemon.
//!
//! Spawns three background tasks:
//! 1. Sampler — adaptive sysinfo reads, feeds ring buffer
//! 2. Circuit breaker — threshold evaluation, node kill
//! 3. Storage flusher — SQLite cold storage

pub mod circuit_breaker;
pub mod sampler;
pub mod storage;
pub mod types;

use crate::daemon::NodeManager;
use sampler::RingBuffer;
use std::path::PathBuf;
use std::sync::Arc;
use tokio::sync::{broadcast, RwLock};
use types::{TelemetryConfig, TelemetrySnapshot, WatchdogAlert, WatchdogLevel};

/// Capacity for the alert channel (sampler → circuit breaker).
const ALERT_CHANNEL_CAPACITY: usize = 64;

/// Capacity for watchdog alert broadcast (circuit breaker → agent).
const WATCHDOG_BROADCAST_CAPACITY: usize = 32;

/// The telemetry service — owns shared state and spawns background tasks.
pub struct TelemetryService {
    config: Arc<RwLock<TelemetryConfig>>,
    ring: Arc<RwLock<RingBuffer>>,
    watchdog_alert_tx: broadcast::Sender<WatchdogAlert>,
    db_path: PathBuf,
}

impl TelemetryService {
    /// Create and start the telemetry service.
    pub async fn start(
        node_manager: Arc<NodeManager>,
        shutdown_rx: tokio::sync::watch::Receiver<()>,
    ) -> Self {
        let home = crate::daemon::registry::get_bubbaloop_home();
        let config_path = home.join("telemetry.toml");
        let db_path = home.join("telemetry.db");

        let mut config = TelemetryConfig::load_or_default(&config_path);
        let clamped = config.validate_and_clamp();
        for msg in &clamped {
            log::warn!("[WATCHDOG] Config guardrail: {}", msg);
        }

        let config = Arc::new(RwLock::new(config));
        let ring = Arc::new(RwLock::new(RingBuffer::new(
            config.read().await.sampling.ring_buffer_capacity,
        )));
        let (alert_tx, alert_rx) = tokio::sync::mpsc::channel(ALERT_CHANNEL_CAPACITY);
        let (watchdog_alert_tx, _) = broadcast::channel(WATCHDOG_BROADCAST_CAPACITY);

        // Spawn sampler
        tokio::spawn(sampler::run_sampler(
            config.clone(),
            ring.clone(),
            alert_tx,
            shutdown_rx.clone(),
        ));

        // Spawn circuit breaker
        tokio::spawn(circuit_breaker::run_circuit_breaker(
            config.clone(),
            node_manager,
            alert_rx,
            watchdog_alert_tx.clone(),
            shutdown_rx.clone(),
        ));

        // Spawn storage flusher
        tokio::spawn(storage::run_storage_flusher(
            config.clone(),
            ring.clone(),
            db_path.clone(),
            shutdown_rx.clone(),
        ));

        // Spawn config hot-reload watcher
        {
            let config = config.clone();
            let config_path = config_path.clone();
            let mut shutdown = shutdown_rx.clone();
            tokio::spawn(async move {
                Self::watch_config(config, config_path, &mut shutdown).await;
            });
        }

        log::info!(
            "[WATCHDOG] Telemetry service started (db={}, config={})",
            db_path.display(),
            config_path.display()
        );

        Self {
            config,
            ring,
            watchdog_alert_tx,
            db_path,
        }
    }

    /// Get the current snapshot (for agent tools).
    pub async fn current_snapshot(&self) -> Option<TelemetrySnapshot> {
        let ring = self.ring.read().await;
        ring.latest().cloned()
    }

    /// Get the current watchdog level.
    pub async fn current_level(&self) -> WatchdogLevel {
        let ring = self.ring.read().await;
        match ring.latest() {
            Some(snap) => {
                let cfg = self.config.read().await;
                types::classify_level(
                    snap.system.memory_available_percent(),
                    &cfg.thresholds,
                )
            }
            None => WatchdogLevel::Green,
        }
    }

    /// Query historical telemetry (for agent tools).
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

    /// Subscribe to watchdog alerts (for agent runtime).
    pub fn subscribe_alerts(&self) -> broadcast::Receiver<WatchdogAlert> {
        self.watchdog_alert_tx.subscribe()
    }

    /// Update config from agent tool (partial update, validates guardrails).
    pub async fn update_config(&self, updates: serde_json::Value) -> Result<Vec<String>, String> {
        let mut cfg = self.config.read().await.clone();

        // Apply partial updates
        if let Some(v) = updates.get("yellow_memory_percent").and_then(|v| v.as_u64()) {
            cfg.thresholds.yellow_memory_percent = v as u8;
        }
        if let Some(v) = updates.get("orange_memory_percent").and_then(|v| v.as_u64()) {
            cfg.thresholds.orange_memory_percent = v as u8;
        }
        if let Some(v) = updates.get("red_memory_percent").and_then(|v| v.as_u64()) {
            cfg.thresholds.red_memory_percent = v as u8;
        }
        if let Some(v) = updates.get("critical_memory_percent").and_then(|v| v.as_u64()) {
            cfg.thresholds.critical_memory_percent = v as u8;
        }
        if let Some(v) = updates.get("cooldown_secs").and_then(|v| v.as_u64()) {
            cfg.circuit_breaker.cooldown_secs = v;
        }
        if let Some(v) = updates.get("idle_interval_secs").and_then(|v| v.as_u64()) {
            cfg.sampling.idle_interval_secs = v;
        }
        if let Some(v) = updates.get("elevated_interval_secs").and_then(|v| v.as_u64()) {
            cfg.sampling.elevated_interval_secs = v;
        }
        if let Some(v) = updates.get("critical_interval_secs").and_then(|v| v.as_u64()) {
            cfg.sampling.critical_interval_secs = v;
        }
        if let Some(v) = updates.get("circuit_breaker_enabled").and_then(|v| v.as_bool()) {
            cfg.circuit_breaker.enabled = v;
        }

        let clamped = cfg.validate_and_clamp();

        // Write to file for persistence
        let home = crate::daemon::registry::get_bubbaloop_home();
        let config_path = home.join("telemetry.toml");

        // Wrap in [telemetry] section
        #[derive(serde::Serialize)]
        struct Wrapper<'a> {
            telemetry: &'a TelemetryConfig,
        }
        let toml_str = toml::to_string_pretty(&Wrapper { telemetry: &cfg })
            .map_err(|e| format!("Failed to serialize config: {}", e))?;
        std::fs::write(&config_path, toml_str)
            .map_err(|e| format!("Failed to write config: {}", e))?;

        // Update in-memory config (file watcher will also pick this up)
        *self.config.write().await = cfg;

        log::info!("[WATCHDOG] Config updated by agent. Clamped: {:?}", clamped);

        Ok(clamped)
    }

    /// Watch for config file changes and hot-reload.
    async fn watch_config(
        config: Arc<RwLock<TelemetryConfig>>,
        config_path: PathBuf,
        shutdown_rx: &mut tokio::sync::watch::Receiver<()>,
    ) {
        use notify::{Event, EventKind, RecursiveMode, Watcher};

        let (tx, mut rx) = tokio::sync::mpsc::channel::<()>(1);

        let mut watcher = match notify::recommended_watcher(move |res: Result<Event, _>| {
            if let Ok(event) = res {
                if matches!(
                    event.kind,
                    EventKind::Modify(_) | EventKind::Create(_)
                ) {
                    let _ = tx.try_send(());
                }
            }
        }) {
            Ok(w) => w,
            Err(e) => {
                log::warn!("[WATCHDOG] Failed to create config watcher: {}", e);
                return;
            }
        };

        if let Some(parent) = config_path.parent() {
            if let Err(e) = watcher.watch(parent, RecursiveMode::NonRecursive) {
                log::warn!("[WATCHDOG] Failed to watch config directory: {}", e);
                return;
            }
        }

        loop {
            tokio::select! {
                Some(()) = rx.recv() => {
                    // Debounce: wait 200ms for writes to settle
                    tokio::time::sleep(std::time::Duration::from_millis(200)).await;

                    let mut new_config = TelemetryConfig::load_or_default(&config_path);
                    let clamped = new_config.validate_and_clamp();
                    for msg in &clamped {
                        log::warn!("[WATCHDOG] Config guardrail on reload: {}", msg);
                    }
                    *config.write().await = new_config;
                    log::info!("[WATCHDOG] Config hot-reloaded from {}", config_path.display());
                }
                _ = shutdown_rx.changed() => break,
            }
        }
    }

    /// Build a summary string for the agent system prompt.
    pub async fn prompt_summary(&self) -> Option<String> {
        let ring = self.ring.read().await;
        let snap = ring.latest()?;
        let cfg = self.config.read().await;
        let level = types::classify_level(
            snap.system.memory_available_percent(),
            &cfg.thresholds,
        );

        Some(format!(
            "## System Resources\nMemory: {:.0}% used ({:?}) | CPU: {:.0}% | Disk: {} free\n",
            100.0 - snap.system.memory_available_percent(),
            level,
            snap.system.cpu_usage_percent,
            format_bytes(snap.system.disk_total_bytes.saturating_sub(snap.system.disk_used_bytes)),
        ))
    }
}

fn format_bytes(bytes: u64) -> String {
    if bytes >= 1024 * 1024 * 1024 {
        format!("{:.1}GB", bytes as f64 / (1024.0 * 1024.0 * 1024.0))
    } else {
        format!("{}MB", bytes / (1024 * 1024))
    }
}
```

**Step 2: Register the module in daemon/mod.rs**

Add `pub mod telemetry;` after line 18 in `crates/bubbaloop/src/daemon/mod.rs`.

**Step 3: Verify it compiles**

Run: `cargo check --lib -p bubbaloop 2>&1 | tail -5`
Expected: success (may have warnings about unused code — that's OK, we'll wire it up next)

**Step 4: Commit**

```bash
git add crates/bubbaloop/src/daemon/telemetry/mod.rs crates/bubbaloop/src/daemon/mod.rs
git commit -m "feat(telemetry): add TelemetryService with hot-reload, alert broadcast, and prompt summary"
```

---

### Task 7: Wire telemetry into daemon startup

**Files:**
- Modify: `crates/bubbaloop/src/daemon/mod.rs:380-526` (the `run()` function)

**Step 1: Add telemetry service start**

After the health monitor start (around line 447) and before the MCP server start (around line 450), add:

```rust
// Start telemetry watchdog
log::info!("Starting telemetry watchdog...");
let telemetry_service = Arc::new(
    telemetry::TelemetryService::start(
        node_manager.clone(),
        shutdown_rx.clone(),
    ).await
);
```

Then update the log output at line 507 to include:
```rust
log::info!("  Telemetry watchdog: active");
```

**Step 2: Verify it compiles and runs**

Run: `cargo check --lib -p bubbaloop 2>&1 | tail -5`
Expected: success

**Step 3: Commit**

```bash
git add crates/bubbaloop/src/daemon/mod.rs
git commit -m "feat(telemetry): wire TelemetryService into daemon startup"
```

---

### Task 8: Add agent dispatch tools

**Files:**
- Modify: `crates/bubbaloop/src/agent/dispatch.rs`

**Step 1: Add TelemetryService to Dispatcher**

Add a new field to the `Dispatcher` struct (around line 35-42):

```rust
telemetry: Option<Arc<crate::daemon::telemetry::TelemetryService>>,
```

Update the constructors to accept and store it. Add a setter method:

```rust
pub fn with_telemetry(mut self, telemetry: Arc<crate::daemon::telemetry::TelemetryService>) -> Self {
    self.telemetry = Some(telemetry);
    self
}
```

**Step 2: Add 3 tool definitions to `tool_definitions()`**

Add at the end of the `vec![]` in `tool_definitions()` (before the closing `]`):

```rust
ToolDefinition {
    name: "get_system_telemetry".to_string(),
    description: "Get current system resource telemetry: memory, CPU, disk usage, \
        watchdog alert level, and top processes by memory consumption.".to_string(),
    input_schema: empty_object.clone(),
},
ToolDefinition {
    name: "get_telemetry_history".to_string(),
    description: "Query historical system telemetry. Returns downsampled time series \
        with trend analysis. Use to detect memory leaks or resource degradation.".to_string(),
    input_schema: json!({
        "type": "object",
        "properties": {
            "duration_minutes": {
                "type": "integer",
                "description": "How many minutes of history to return (default: 60)"
            }
        },
        "required": []
    }),
},
ToolDefinition {
    name: "update_telemetry_config".to_string(),
    description: "Update telemetry watchdog thresholds at runtime. Only provided \
        fields are changed. Guardrails prevent unsafe values. Use to adapt monitoring \
        based on workload (e.g., relax thresholds for heavy ML inference, tighten for \
        leak detection).".to_string(),
    input_schema: json!({
        "type": "object",
        "properties": {
            "yellow_memory_percent": { "type": "integer", "description": "Memory used % to trigger Yellow warning (default: 60)" },
            "orange_memory_percent": { "type": "integer", "description": "Memory used % to trigger Orange warning (default: 80)" },
            "red_memory_percent": { "type": "integer", "description": "Memory used % to trigger Red (auto-kill, default: 90)" },
            "critical_memory_percent": { "type": "integer", "description": "Memory used % to trigger Critical (kill all, default: 95, min: 80, max: 98)" },
            "cooldown_secs": { "type": "integer", "description": "Seconds between node kills (default: 30)" },
            "idle_interval_secs": { "type": "integer", "description": "Sampling interval when Green (default: 30, min: 2)" },
            "elevated_interval_secs": { "type": "integer", "description": "Sampling interval when Yellow (default: 10, min: 2)" },
            "critical_interval_secs": { "type": "integer", "description": "Sampling interval when Orange+ (default: 5, min: 2)" },
            "circuit_breaker_enabled": { "type": "boolean", "description": "Enable/disable automatic node killing (default: true)" }
        },
        "required": []
    }),
},
```

**Step 3: Add handler methods and call_tool match arms**

Add 3 handler methods:

```rust
async fn handle_get_system_telemetry(&self) -> ToolResult {
    let telemetry = match &self.telemetry {
        Some(t) => t,
        None => return ToolResult::error("Telemetry service not available".to_string()),
    };
    match telemetry.current_snapshot().await {
        Some(snap) => {
            let level = telemetry.current_level().await;
            let result = serde_json::json!({
                "memory_used_percent": 100.0 - snap.system.memory_available_percent(),
                "memory_available_mb": snap.system.memory_available_bytes / (1024 * 1024),
                "memory_total_mb": snap.system.memory_total_bytes / (1024 * 1024),
                "cpu_usage_percent": snap.system.cpu_usage_percent,
                "load_average_1m": snap.system.load_average_1m,
                "disk_free_gb": snap.system.disk_free_mb() as f64 / 1024.0,
                "disk_total_gb": snap.system.disk_total_bytes as f64 / (1024.0 * 1024.0 * 1024.0),
                "swap_used_mb": snap.system.swap_used_bytes / (1024 * 1024),
                "watchdog_level": format!("{:?}", level),
                "top_processes": snap.processes.iter().take(10).map(|p| {
                    serde_json::json!({
                        "name": p.name,
                        "pid": p.pid,
                        "rss_mb": p.rss_bytes / (1024 * 1024),
                        "cpu_percent": p.cpu_percent,
                    })
                }).collect::<Vec<_>>(),
            });
            ToolResult::success(serde_json::to_string_pretty(&result).unwrap_or_default())
        }
        None => ToolResult::error("No telemetry data available yet".to_string()),
    }
}

async fn handle_get_telemetry_history(&self, input: &serde_json::Value) -> ToolResult {
    let telemetry = match &self.telemetry {
        Some(t) => t,
        None => return ToolResult::error("Telemetry service not available".to_string()),
    };
    let duration = input.get("duration_minutes")
        .and_then(|v| v.as_u64())
        .unwrap_or(60);
    match telemetry.query_history(duration, 60) {
        Ok(snapshots) if snapshots.is_empty() => {
            ToolResult::success("No telemetry history available yet.".to_string())
        }
        Ok(snapshots) => {
            let samples: Vec<_> = snapshots.iter().map(|s| {
                serde_json::json!({
                    "timestamp_ms": s.system.timestamp_ms,
                    "memory_used_percent": 100.0 - s.system.memory_available_percent(),
                    "cpu_percent": s.system.cpu_usage_percent,
                    "disk_free_gb": s.system.disk_free_mb() as f64 / 1024.0,
                })
            }).collect();

            // Calculate trend (memory change per hour)
            let trend = if snapshots.len() >= 2 {
                let first = &snapshots[0];
                let last = &snapshots[snapshots.len() - 1];
                let dt_hours = (last.system.timestamp_ms - first.system.timestamp_ms) as f64
                    / (1000.0 * 3600.0);
                if dt_hours > 0.01 {
                    let mem_delta = (100.0 - last.system.memory_available_percent())
                        - (100.0 - first.system.memory_available_percent());
                    Some(mem_delta as f64 / dt_hours)
                } else {
                    None
                }
            } else {
                None
            };

            let result = serde_json::json!({
                "duration_minutes": duration,
                "sample_count": samples.len(),
                "samples": samples,
                "memory_trend_per_hour": trend,
                "trend_description": match trend {
                    Some(r) if r > 2.0 => "memory_rising_fast",
                    Some(r) if r > 0.5 => "memory_rising",
                    Some(r) if r < -0.5 => "memory_falling",
                    _ => "stable",
                },
            });
            ToolResult::success(serde_json::to_string_pretty(&result).unwrap_or_default())
        }
        Err(e) => ToolResult::error(format!("Failed to query history: {}", e)),
    }
}

async fn handle_update_telemetry_config(&self, input: &serde_json::Value) -> ToolResult {
    let telemetry = match &self.telemetry {
        Some(t) => t,
        None => return ToolResult::error("Telemetry service not available".to_string()),
    };
    match telemetry.update_config(input.clone()).await {
        Ok(clamped) => {
            log::info!("[MCP] tool=update_telemetry_config clamped={:?}", clamped);
            let msg = if clamped.is_empty() {
                "Telemetry config updated successfully.".to_string()
            } else {
                format!(
                    "Telemetry config updated with guardrails applied: {}",
                    clamped.join(", ")
                )
            };
            ToolResult::success(msg)
        }
        Err(e) => ToolResult::error(format!("Failed to update config: {}", e)),
    }
}
```

Add to the `call_tool` match block (before the `_ =>` arm):

```rust
"get_system_telemetry" => self.handle_get_system_telemetry().await,
"get_telemetry_history" => self.handle_get_telemetry_history(input).await,
"update_telemetry_config" => self.handle_update_telemetry_config(input).await,
```

**Step 4: Update RBAC in dispatch_security.rs**

Add the new tools to the appropriate tier:
- `get_system_telemetry` → Viewer
- `get_telemetry_history` → Viewer
- `update_telemetry_config` → Admin

**Step 5: Update the tool count test**

The test at line ~1205 checks `tool_definitions_count`. Update the expected count from 34 to 37.

**Step 6: Run tests**

Run: `cargo test --lib -p bubbaloop dispatch 2>&1 | tail -10`
Expected: all tests pass

**Step 7: Commit**

```bash
git add crates/bubbaloop/src/agent/dispatch.rs crates/bubbaloop/src/agent/dispatch_security.rs
git commit -m "feat(telemetry): add 3 agent dispatch tools (get_system_telemetry, get_telemetry_history, update_telemetry_config)"
```

---

### Task 9: Agent system prompt injection

**Files:**
- Modify: `crates/bubbaloop/src/agent/prompt.rs`

**Step 1: Add resource_summary parameter to `build_system_prompt()`**

Add `resource_summary: Option<&str>` parameter after `recovered_context`.

After the Tools section (around line 92), inject:

```rust
// System resource summary (from telemetry watchdog)
if let Some(summary) = resource_summary {
    parts.push(summary.to_string());
}
```

**Step 2: Update all call sites**

Search for `build_system_prompt` calls in `runtime.rs` and pass the telemetry summary. For the call site, get the summary from `TelemetryService::prompt_summary()`.

**Step 3: Update all tests**

All existing tests call `build_system_prompt` — add `None` as the new parameter to each.

Add a new test:
```rust
#[test]
fn prompt_with_resource_summary() {
    let soul = Soul::default();
    let prompt = build_system_prompt(&soul, "", &[], &[], None, None, Some("## System Resources\nMemory: 62% used (Yellow)"));
    assert!(prompt.contains("System Resources"));
    assert!(prompt.contains("62% used"));
}
```

**Step 4: Run tests**

Run: `cargo test --lib -p bubbaloop prompt 2>&1 | tail -10`
Expected: all tests pass

**Step 5: Commit**

```bash
git add crates/bubbaloop/src/agent/prompt.rs crates/bubbaloop/src/agent/runtime.rs
git commit -m "feat(telemetry): inject resource summary into agent system prompt"
```

---

### Task 10: Wire TelemetryService into agent runtime

**Files:**
- Modify: `crates/bubbaloop/src/agent/runtime.rs`
- Modify: `crates/bubbaloop/src/daemon/mod.rs`

**Step 1: Pass TelemetryService to agent runtime**

Update `run_agent_runtime()` to accept `Option<Arc<TelemetryService>>` and pass it through to the Dispatcher constructor.

**Step 2: Pass telemetry_service from daemon `run()`**

In the agent_task spawn, pass the `telemetry_service.clone()`.

**Step 3: Subscribe to watchdog alerts in agent loop**

In the agent loop `tokio::select!`, add a branch for watchdog alerts:
```rust
Ok(alert) = watchdog_alert_rx.recv() => {
    // Convert alert to an agent inbox message
    let alert_text = serde_json::to_string_pretty(&alert).unwrap_or_default();
    let msg = AgentMessage {
        text: format!("[WATCHDOG ALERT] {}", alert_text),
        ..
    };
    // Push to agent inbox
}
```

**Step 4: Verify it compiles**

Run: `cargo check --lib -p bubbaloop 2>&1 | tail -5`
Expected: success

**Step 5: Commit**

```bash
git add crates/bubbaloop/src/agent/runtime.rs crates/bubbaloop/src/daemon/mod.rs
git commit -m "feat(telemetry): wire TelemetryService into agent runtime with alert subscription"
```

---

### Task 11: Full integration test

**Files:**
- No new files — test the whole system

**Step 1: Run all unit tests**

Run: `cargo test --lib -p bubbaloop 2>&1 | tail -20`
Expected: all tests pass, zero failures

**Step 2: Run clippy**

Run: `pixi run clippy 2>&1 | tail -20`
Expected: zero warnings

**Step 3: Run fmt**

Run: `pixi run fmt`

**Step 4: Final commit**

```bash
git add -A
git commit -m "chore(telemetry): fix any clippy/fmt issues from telemetry feature"
```

---

## Summary

| Task | What | Files | Tests |
|------|------|-------|-------|
| 1 | Add sysinfo dep | Cargo.toml (2) | compile check |
| 2 | Types module | telemetry/types.rs | 15 unit tests |
| 3 | Sampler | telemetry/sampler.rs | 2 unit tests |
| 4 | Storage | telemetry/storage.rs | 4 unit tests |
| 5 | Circuit breaker | telemetry/circuit_breaker.rs | 3 unit tests |
| 6 | TelemetryService | telemetry/mod.rs + daemon/mod.rs | compile check |
| 7 | Daemon wiring | daemon/mod.rs | compile check |
| 8 | Agent tools | dispatch.rs + dispatch_security.rs | updated count test |
| 9 | Prompt injection | prompt.rs + runtime.rs | 1 new test + updated existing |
| 10 | Runtime wiring | runtime.rs + daemon/mod.rs | compile check |
| 11 | Integration | — | full test + clippy + fmt |

**Total new files:** 5 (types.rs, sampler.rs, storage.rs, circuit_breaker.rs, telemetry/mod.rs)
**Modified files:** 5 (daemon/mod.rs, dispatch.rs, dispatch_security.rs, prompt.rs, runtime.rs)
**New dependency:** 1 (`sysinfo`)
**New tests:** ~25 unit tests
