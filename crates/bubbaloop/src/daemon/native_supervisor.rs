//! Native process supervisor for development environments without systemd.
//!
//! Manages node processes directly using `tokio::process`, storing process
//! configuration and PIDs under `~/.bubbaloop/procs/`. This backend is used
//! automatically when systemd D-Bus is unavailable, mainly for Docker-based
//! development and future non-systemd experiments.
//!
//! Capabilities vs systemd backend:
//! - start / stop / restart / status  ✅
//! - autostart persisted to disk       ✅
//! - install / uninstall config        ✅
//! - lifecycle signals (mpsc events)   ✅
//! - journalctl logs                   ❌
//!
//! This is intentionally not a production-equivalent replacement for systemd.

use crate::daemon::systemd::{ActiveState, SystemdError, SystemdSignalEvent};
use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};
use tokio::sync::{broadcast, mpsc};

type Result<T> = std::result::Result<T, SystemdError>;

/// Process configuration stored on disk under `<procs_dir>/{name}.json`.
#[derive(Debug, Clone, Serialize, Deserialize)]
struct ProcConfig {
    name: String,
    command: String,
    work_dir: String,
    node_type: String,
    autostart: bool,
    /// Dependency names (informational only — native backend does not enforce ordering).
    #[serde(default)]
    depends_on: Vec<String>,
}

/// Native process supervisor — manages processes directly without systemd.
pub struct NativeSupervisor {
    /// Broadcast channel for lifecycle events (started, stopped, failed).
    event_tx: broadcast::Sender<SystemdSignalEvent>,
    /// Root directory for config, PID, and log files.
    procs_dir: PathBuf,
}

impl Default for NativeSupervisor {
    fn default() -> Self {
        Self::new()
    }
}

impl NativeSupervisor {
    pub fn new() -> Self {
        let procs_dir = dirs::home_dir()
            .unwrap_or_else(|| PathBuf::from("/tmp"))
            .join(".bubbaloop/procs");
        Self::new_with_root(procs_dir)
    }

    /// Create a supervisor that stores all files under `procs_dir`.
    /// Useful for tests that need an isolated directory.
    pub fn new_with_root(procs_dir: PathBuf) -> Self {
        let (event_tx, _) = broadcast::channel(64);
        Self { event_tx, procs_dir }
    }

    // ── Filesystem helpers ─────────────────────────────────────────────────

    fn config_path(&self, name: &str) -> PathBuf {
        self.procs_dir.join(format!("{name}.json"))
    }

    fn pid_path(&self, name: &str) -> PathBuf {
        self.procs_dir.join(format!("{name}.pid"))
    }

    fn stdout_path(&self, name: &str) -> PathBuf {
        self.procs_dir.join(format!("{name}.stdout"))
    }

    fn stderr_path(&self, name: &str) -> PathBuf {
        self.procs_dir.join(format!("{name}.stderr"))
    }

    fn read_config(&self, name: &str) -> Option<ProcConfig> {
        let content = std::fs::read_to_string(self.config_path(name)).ok()?;
        serde_json::from_str(&content).ok()
    }

    fn write_config(&self, config: &ProcConfig) -> Result<()> {
        std::fs::create_dir_all(&self.procs_dir).map_err(SystemdError::Io)?;
        let content = serde_json::to_string_pretty(config)
            .map_err(|e| SystemdError::OperationFailed(e.to_string()))?;
        std::fs::write(self.config_path(&config.name), content).map_err(SystemdError::Io)
    }

    fn read_pid(&self, name: &str) -> Option<u32> {
        std::fs::read_to_string(self.pid_path(name))
            .ok()
            .and_then(|s| s.trim().parse().ok())
    }

    fn write_pid(&self, name: &str, pid: u32) -> Result<()> {
        std::fs::write(self.pid_path(name), pid.to_string()).map_err(SystemdError::Io)
    }

    fn remove_pid(&self, name: &str) {
        let _ = std::fs::remove_file(self.pid_path(name));
    }

    // ── Process liveness check ─────────────────────────────────────────────

    /// Check if a PID is alive. Uses `/proc/{pid}` on Linux, `ps` on macOS.
    fn is_pid_alive(pid: u32) -> bool {
        #[cfg(target_os = "linux")]
        {
            std::path::Path::new(&format!("/proc/{pid}")).exists()
        }
        #[cfg(not(target_os = "linux"))]
        {
            std::process::Command::new("ps")
                .args(["-p", &pid.to_string()])
                .output()
                .map(|o| o.status.success())
                .unwrap_or(false)
        }
    }

    // ── Signal helpers ─────────────────────────────────────────────────────

    fn emit(&self, event: SystemdSignalEvent) {
        // Ignore errors: no subscribers is fine
        let _ = self.event_tx.send(event);
    }

    fn job_removed_event(name: &str, result: &str) -> SystemdSignalEvent {
        let unit = format!("bubbaloop-{name}.service");
        SystemdSignalEvent::JobRemoved {
            unit,
            result: result.to_string(),
            node_name: Some(name.to_string()),
        }
    }

    // ── Public API (mirrors SystemdClient) ────────────────────────────────

    /// Write process configuration. Equivalent to systemd `install_service`.
    ///
    /// When `command` is `None`, a default is derived from `node_type`:
    /// - `rust`   → `<node_path>/target/release/<name>` (or debug if release absent)
    /// - `python` → `<node_path>/venv/bin/python main.py`
    /// - other    → `./<name>`
    ///
    /// `depends_on` is persisted for informational purposes. The native backend
    /// does not enforce ordering — a warning is emitted at start time if non-empty.
    pub fn install_service(
        &self,
        node_path: &str,
        name: &str,
        node_type: &str,
        command: Option<&str>,
        depends_on: &[String],
    ) -> Result<()> {
        let cmd = match command {
            Some(c) => c.to_string(),
            None => match node_type {
                "rust" => {
                    let release = Path::new(node_path).join("target/release").join(name);
                    let debug = Path::new(node_path).join("target/debug").join(name);
                    if release.exists() {
                        release.to_string_lossy().to_string()
                    } else {
                        debug.to_string_lossy().to_string()
                    }
                }
                "python" => {
                    let venv_python = Path::new(node_path).join("venv/bin/python");
                    format!("{} main.py", venv_python.display())
                }
                _ => format!("./{name}"),
            },
        };

        let config = ProcConfig {
            name: name.to_string(),
            command: cmd,
            work_dir: node_path.to_string(),
            node_type: node_type.to_string(),
            autostart: false,
            depends_on: depends_on.to_vec(),
        };
        self.write_config(&config)?;

        self.emit(SystemdSignalEvent::UnitNew {
            unit: format!("bubbaloop-{name}.service"),
            node_name: Some(name.to_string()),
        });

        Ok(())
    }

    /// Remove process configuration. Equivalent to systemd `uninstall_service`.
    pub async fn uninstall_service(&self, name: &str) -> Result<()> {
        self.stop_unit(name).await.ok(); // best-effort stop

        let _ = std::fs::remove_file(self.config_path(name));
        let _ = std::fs::remove_file(self.stdout_path(name));
        let _ = std::fs::remove_file(self.stderr_path(name));
        self.remove_pid(name);

        self.emit(SystemdSignalEvent::UnitRemoved {
            unit: format!("bubbaloop-{name}.service"),
            node_name: Some(name.to_string()),
        });

        Ok(())
    }

    /// Returns true if a config file exists for the node.
    pub fn is_installed(&self, name: &str) -> bool {
        self.config_path(name).exists()
    }

    /// Start the process described in the node's config file.
    pub async fn start_unit(&self, name: &str) -> Result<()> {
        let config = self
            .read_config(name)
            .ok_or_else(|| SystemdError::ServiceNotFound(name.to_string()))?;

        if !config.depends_on.is_empty() {
            log::warn!(
                "[NativeSupervisor] {name} has depends_on {:?} — \
                 ordering is not enforced by the native backend (dev-only fallback)",
                config.depends_on
            );
        }

        // Already running?
        if let Some(pid) = self.read_pid(name) {
            if Self::is_pid_alive(pid) {
                return Ok(());
            }
        }

        // Split command into executable + args (simple whitespace split)
        let parts: Vec<&str> = config.command.split_whitespace().collect();
        let (exe, args) = parts
            .split_first()
            .ok_or_else(|| SystemdError::OperationFailed("empty command".to_string()))?;

        // Redirect stdout/stderr to log files so child output does not
        // pollute the daemon's own logs and users have a basic log trail.
        std::fs::create_dir_all(&self.procs_dir).map_err(SystemdError::Io)?;
        let stdout_file = std::fs::File::create(self.stdout_path(name))
            .map_err(|e| SystemdError::OperationFailed(format!("stdout log: {e}")))?;
        let stderr_file = std::fs::File::create(self.stderr_path(name))
            .map_err(|e| SystemdError::OperationFailed(format!("stderr log: {e}")))?;

        let child = tokio::process::Command::new(exe)
            .args(args)
            .current_dir(&config.work_dir)
            .stdout(stdout_file)
            .stderr(stderr_file)
            .spawn()
            .map_err(|e| SystemdError::OperationFailed(format!("Failed to spawn {name}: {e}")))?;

        let pid = child.id().ok_or_else(|| {
            SystemdError::OperationFailed("could not get PID after spawn".to_string())
        })?;

        self.write_pid(name, pid)?;

        // Spawn a watcher task that calls `child.wait()` to reap the process
        // and collect its exit status. `kill_on_drop` is false by default, so
        // dropping a `tokio::process::Child` without waiting does NOT send SIGKILL —
        // it merely leaks the child. We use `.wait()` here to properly reap the
        // process and get its exit status before emitting lifecycle events.
        let name_owned = name.to_string();
        let event_tx = self.event_tx.clone();
        let pid_path = self.pid_path(name);
        tokio::spawn(async move {
            let mut child = child;
            let status = child.wait().await;

            let result = match &status {
                Ok(s) if s.success() => "done",
                Ok(_) => "failed",
                Err(_) => "failed",
            };

            let _ = std::fs::remove_file(&pid_path);
            let unit = format!("bubbaloop-{name_owned}.service");
            let _ = event_tx.send(SystemdSignalEvent::JobRemoved {
                unit,
                result: result.to_string(),
                node_name: Some(name_owned),
            });
        });

        log::info!("[NativeSupervisor] Started {name} (pid={pid})");

        Ok(())
    }

    /// Stop the process by sending SIGTERM via `/bin/kill`.
    ///
    /// If the node is installed but not running (no PID file or stale PID),
    /// the call is treated as a no-op and returns `Ok(())`. `ServiceNotFound`
    /// is only returned when the node has never been installed.
    ///
    /// **Caveat (dev-only backend):** Between reading the PID file and sending
    /// the signal there is a small window where the PID could be recycled by
    /// the OS and reassigned to an unrelated process. This is inherent to
    /// PID-file management and acceptable for a development-only supervisor.
    pub async fn stop_unit(&self, name: &str) -> Result<()> {
        let pid = self
            .read_pid(name)
            .filter(|&pid| Self::is_pid_alive(pid));

        let Some(pid) = pid else {
            // No live PID — node is stopped or was never started.
            if self.is_installed(name) {
                self.remove_pid(name);
                self.emit(Self::job_removed_event(name, "done"));
                log::info!("[NativeSupervisor] {name} already stopped");
                return Ok(());
            }
            return Err(SystemdError::ServiceNotFound(name.to_string()));
        };

        // Guard against dangerous PIDs (corrupted PID file, init process).
        if pid <= 1 {
            self.remove_pid(name);
            return Err(SystemdError::OperationFailed(format!(
                "refusing to send signal to pid {pid}"
            )));
        }

        let kill_bin = if std::path::Path::new("/bin/kill").exists() {
            "/bin/kill"
        } else {
            "/usr/bin/kill"
        };

        // Send SIGTERM and verify it was delivered.
        let term_status = tokio::process::Command::new(kill_bin)
            .args(["-TERM", &pid.to_string()])
            .status()
            .await
            .map_err(|e| SystemdError::OperationFailed(e.to_string()))?;

        if !term_status.success() {
            return Err(SystemdError::OperationFailed(format!(
                "kill -TERM {pid} failed with exit status {term_status}"
            )));
        }

        // Give it up to 3s to exit gracefully, then SIGKILL
        for _ in 0..6 {
            tokio::time::sleep(std::time::Duration::from_millis(500)).await;
            if !Self::is_pid_alive(pid) {
                break;
            }
        }

        if Self::is_pid_alive(pid) {
            tokio::process::Command::new(kill_bin)
                .args(["-KILL", &pid.to_string()])
                .status()
                .await
                .ok();

            // Give SIGKILL a moment then re-check liveness before claiming success.
            tokio::time::sleep(std::time::Duration::from_millis(300)).await;
            if Self::is_pid_alive(pid) {
                return Err(SystemdError::OperationFailed(format!(
                    "process {pid} survived SIGKILL — node {name} may be in uninterruptible sleep"
                )));
            }
        }

        self.remove_pid(name);
        self.emit(Self::job_removed_event(name, "done"));
        log::info!("[NativeSupervisor] Stopped {name} (pid={pid})");

        Ok(())
    }

    /// Stop then start.
    pub async fn restart_unit(&self, name: &str) -> Result<()> {
        self.stop_unit(name).await.ok(); // ignore "not running"
        self.start_unit(name).await
    }

    /// Returns Active if the PID file exists and the process is alive.
    pub async fn get_active_state(&self, name: &str) -> Result<ActiveState> {
        match self.read_pid(name) {
            Some(pid) if Self::is_pid_alive(pid) => Ok(ActiveState::Active),
            Some(_) => {
                // Stale PID file
                self.remove_pid(name);
                Ok(if self.is_installed(name) {
                    ActiveState::Inactive
                } else {
                    ActiveState::Unknown("not-found".to_string())
                })
            }
            None => Ok(if self.is_installed(name) {
                ActiveState::Inactive
            } else {
                ActiveState::Unknown("not-found".to_string())
            }),
        }
    }

    /// Returns the autostart flag from the config file.
    pub fn is_enabled(&self, name: &str) -> bool {
        self.read_config(name)
            .map(|c| c.autostart)
            .unwrap_or(false)
    }

    /// Enable autostart (persisted to config file).
    pub fn enable_unit(&self, name: &str) -> Result<()> {
        let mut config = self
            .read_config(name)
            .ok_or_else(|| SystemdError::ServiceNotFound(name.to_string()))?;
        config.autostart = true;
        self.write_config(&config)
    }

    /// Disable autostart (persisted to config file).
    pub fn disable_unit(&self, name: &str) -> Result<()> {
        let mut config = self
            .read_config(name)
            .ok_or_else(|| SystemdError::ServiceNotFound(name.to_string()))?;
        config.autostart = false;
        self.write_config(&config)
    }

    /// Enumerate all installed node names (based on `*.json` config files in `procs_dir`).
    pub fn list_installed_names(&self) -> Vec<String> {
        let Ok(entries) = std::fs::read_dir(&self.procs_dir) else {
            return Vec::new();
        };
        entries
            .flatten()
            .filter_map(|e| {
                let p = e.path();
                if p.extension().and_then(|s| s.to_str()) == Some("json") {
                    p.file_stem()
                        .and_then(|s| s.to_str())
                        .map(|s| s.to_string())
                } else {
                    None
                }
            })
            .collect()
    }

    /// Start all installed nodes that have `autostart: true`.
    /// Called by the daemon on startup in native mode to replicate systemd's autostart behavior.
    pub async fn start_autostart_units(&self) -> usize {
        let names = self.list_installed_names();
        let mut started = 0usize;
        for name in names {
            if self.is_enabled(&name) {
                match self.start_unit(&name).await {
                    Ok(()) => {
                        log::info!("[NativeSupervisor] Autostart: started {name}");
                        started += 1;
                    }
                    Err(e) => {
                        log::warn!("[NativeSupervisor] Autostart: failed to start {name}: {e}");
                    }
                }
            }
        }
        started
    }

    /// Returns an mpsc receiver that receives lifecycle events.
    pub fn subscribe_to_signals(&self) -> mpsc::Receiver<SystemdSignalEvent> {
        let (tx, rx) = mpsc::channel(64);
        let mut bcast_rx = self.event_tx.subscribe();
        tokio::spawn(async move {
            while let Ok(event) = bcast_rx.recv().await {
                if tx.send(event).await.is_err() {
                    break;
                }
            }
        });
        rx
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::{Duration, SystemTime, UNIX_EPOCH};
    use tempfile::TempDir;

    fn unique_name(prefix: &str) -> String {
        let now = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap_or_else(|_| Duration::from_secs(0))
            .as_nanos();
        format!("{prefix}-{now}")
    }

    /// Create an isolated supervisor + temp dir. The dir is kept alive for the
    /// duration of the test by returning it alongside the supervisor.
    fn isolated_supervisor() -> (NativeSupervisor, TempDir) {
        let dir = tempfile::tempdir().expect("tempdir");
        let sup = NativeSupervisor::new_with_root(dir.path().to_path_buf());
        (sup, dir)
    }

    async fn wait_for_active_state(
        sup: &NativeSupervisor,
        name: &str,
        expected: ActiveState,
        timeout_ms: u64,
    ) {
        let deadline = tokio::time::Instant::now() + Duration::from_millis(timeout_ms);
        loop {
            let state = sup.get_active_state(name).await.unwrap();
            if state == expected {
                return;
            }
            if tokio::time::Instant::now() >= deadline {
                panic!("timed out waiting for state {expected:?}, got {state:?}");
            }
            tokio::time::sleep(Duration::from_millis(50)).await;
        }
    }

    #[tokio::test]
    async fn install_start_stop_uninstall_cycle() {
        let (sup, _dir) = isolated_supervisor();
        let name = unique_name("native-cycle");

        sup.install_service("/tmp", &name, "rust", Some("sleep 30"))
            .unwrap();
        assert!(sup.is_installed(&name));

        sup.start_unit(&name).await.unwrap();
        wait_for_active_state(&sup, &name, ActiveState::Active, 2_000).await;

        sup.stop_unit(&name).await.unwrap();
        wait_for_active_state(&sup, &name, ActiveState::Inactive, 2_000).await;

        sup.uninstall_service(&name).await.unwrap();
        assert!(!sup.is_installed(&name));
    }

    #[tokio::test]
    async fn stop_unit_is_idempotent_when_installed_but_not_running() {
        let (sup, _dir) = isolated_supervisor();
        let name = unique_name("native-idempotent");

        sup.install_service("/tmp", &name, "rust", Some("sleep 5"))
            .unwrap();

        // Stop without ever starting — should be a no-op, not an error
        sup.stop_unit(&name).await.unwrap();
        let state = sup.get_active_state(&name).await.unwrap();
        assert_eq!(state, ActiveState::Inactive);

        sup.uninstall_service(&name).await.unwrap();
    }

    #[tokio::test]
    async fn autostart_enable_disable_persists() {
        let (sup, _dir) = isolated_supervisor();
        let name = unique_name("native-autostart");

        sup.install_service("/tmp", &name, "rust", Some("sleep 5"))
            .unwrap();

        assert!(!sup.is_enabled(&name));
        sup.enable_unit(&name).unwrap();
        assert!(sup.is_enabled(&name));

        sup.disable_unit(&name).unwrap();
        assert!(!sup.is_enabled(&name));

        sup.uninstall_service(&name).await.unwrap();
    }

    #[tokio::test]
    async fn stale_pid_file_is_cleaned_and_state_is_inactive() {
        let (sup, _dir) = isolated_supervisor();
        let name = unique_name("native-stale");

        sup.install_service("/tmp", &name, "rust", Some("sleep 5"))
            .unwrap();

        std::fs::write(sup.pid_path(&name), "4294967295").unwrap();
        let state = sup.get_active_state(&name).await.unwrap();
        assert_eq!(state, ActiveState::Inactive);
        assert!(!sup.pid_path(&name).exists());

        sup.uninstall_service(&name).await.unwrap();
    }

    #[tokio::test]
    async fn expected_errors_for_missing_service_and_empty_command() {
        let (sup, _dir) = isolated_supervisor();
        let missing = unique_name("native-missing");

        match sup.start_unit(&missing).await {
            Err(SystemdError::ServiceNotFound(s)) => assert_eq!(s, missing),
            other => panic!("expected ServiceNotFound from start_unit, got {other:?}"),
        }

        match sup.stop_unit(&missing).await {
            Err(SystemdError::ServiceNotFound(s)) => assert_eq!(s, missing),
            other => panic!("expected ServiceNotFound from stop_unit, got {other:?}"),
        }

        match sup.enable_unit(&missing) {
            Err(SystemdError::ServiceNotFound(s)) => assert_eq!(s, missing),
            other => panic!("expected ServiceNotFound from enable_unit, got {other:?}"),
        }

        match sup.disable_unit(&missing) {
            Err(SystemdError::ServiceNotFound(s)) => assert_eq!(s, missing),
            other => panic!("expected ServiceNotFound from disable_unit, got {other:?}"),
        }

        let empty_cmd_name = unique_name("native-emptycmd");
        sup.install_service("/tmp", &empty_cmd_name, "rust", Some("   "))
            .unwrap();
        match sup.start_unit(&empty_cmd_name).await {
            Err(SystemdError::OperationFailed(msg)) => {
                assert!(msg.contains("empty command"));
            }
            other => panic!("expected OperationFailed(empty command), got {other:?}"),
        }
        sup.uninstall_service(&empty_cmd_name).await.unwrap();
    }

    #[tokio::test]
    async fn subscribe_to_signals_receives_install_and_uninstall_events() {
        let (sup, _dir) = isolated_supervisor();
        let name = unique_name("native-signals");

        let mut rx = sup.subscribe_to_signals();

        sup.install_service("/tmp", &name, "rust", Some("sleep 5"))
            .unwrap();
        let first = tokio::time::timeout(Duration::from_secs(2), rx.recv())
            .await
            .expect("timed out waiting for UnitNew")
            .expect("channel closed before UnitNew");

        match first {
            SystemdSignalEvent::UnitNew { unit, node_name } => {
                assert_eq!(unit, format!("bubbaloop-{name}.service"));
                assert_eq!(node_name, Some(name.clone()));
            }
            other => panic!("expected UnitNew, got {other:?}"),
        }

        sup.uninstall_service(&name).await.unwrap();
        let second = tokio::time::timeout(Duration::from_secs(2), rx.recv())
            .await
            .expect("timed out waiting for UnitRemoved")
            .expect("channel closed before UnitRemoved");

        match second {
            SystemdSignalEvent::UnitRemoved { unit, node_name } => {
                assert_eq!(unit, format!("bubbaloop-{name}.service"));
                assert_eq!(node_name, Some(name.clone()));
            }
            other => panic!("expected UnitRemoved, got {other:?}"),
        }
    }
}
