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
use std::path::PathBuf;
use tokio::sync::{broadcast, mpsc};

type Result<T> = std::result::Result<T, SystemdError>;

/// Process configuration stored on disk under `~/.bubbaloop/procs/{name}.json`.
#[derive(Debug, Clone, Serialize, Deserialize)]
struct ProcConfig {
    name: String,
    command: String,
    work_dir: String,
    node_type: String,
    autostart: bool,
}

/// Native process supervisor — manages processes directly without systemd.
pub struct NativeSupervisor {
    /// Broadcast channel for lifecycle events (started, stopped, failed).
    event_tx: broadcast::Sender<SystemdSignalEvent>,
}

impl Default for NativeSupervisor {
    fn default() -> Self {
        Self::new()
    }
}

impl NativeSupervisor {
    pub fn new() -> Self {
        let (event_tx, _) = broadcast::channel(64);
        Self { event_tx }
    }

    // ── Filesystem helpers ─────────────────────────────────────────────────

    fn procs_dir() -> PathBuf {
        dirs::home_dir()
            .unwrap_or_else(|| PathBuf::from("/tmp"))
            .join(".bubbaloop/procs")
    }

    fn config_path(name: &str) -> PathBuf {
        Self::procs_dir().join(format!("{name}.json"))
    }

    fn pid_path(name: &str) -> PathBuf {
        Self::procs_dir().join(format!("{name}.pid"))
    }

    fn read_config(name: &str) -> Option<ProcConfig> {
        let content = std::fs::read_to_string(Self::config_path(name)).ok()?;
        serde_json::from_str(&content).ok()
    }

    fn write_config(config: &ProcConfig) -> Result<()> {
        std::fs::create_dir_all(Self::procs_dir()).map_err(SystemdError::Io)?;
        let content = serde_json::to_string_pretty(config)
            .map_err(|e| SystemdError::OperationFailed(e.to_string()))?;
        std::fs::write(Self::config_path(&config.name), content).map_err(SystemdError::Io)
    }

    fn read_pid(name: &str) -> Option<u32> {
        std::fs::read_to_string(Self::pid_path(name))
            .ok()
            .and_then(|s| s.trim().parse().ok())
    }

    fn write_pid(name: &str, pid: u32) -> Result<()> {
        std::fs::write(Self::pid_path(name), pid.to_string()).map_err(SystemdError::Io)
    }

    fn remove_pid(name: &str) {
        let _ = std::fs::remove_file(Self::pid_path(name));
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
    pub fn install_service(
        &self,
        node_path: &str,
        name: &str,
        node_type: &str,
        command: Option<&str>,
    ) -> Result<()> {
        let cmd = command
            .map(|c| c.to_string())
            .unwrap_or_else(|| format!("./{name}"));

        let config = ProcConfig {
            name: name.to_string(),
            command: cmd,
            work_dir: node_path.to_string(),
            node_type: node_type.to_string(),
            autostart: false,
        };
        Self::write_config(&config)?;

        self.emit(SystemdSignalEvent::UnitNew {
            unit: format!("bubbaloop-{name}.service"),
            node_name: Some(name.to_string()),
        });

        Ok(())
    }

    /// Remove process configuration. Equivalent to systemd `uninstall_service`.
    pub async fn uninstall_service(&self, name: &str) -> Result<()> {
        self.stop_unit(name).await.ok(); // best-effort stop

        let _ = std::fs::remove_file(Self::config_path(name));
        Self::remove_pid(name);

        self.emit(SystemdSignalEvent::UnitRemoved {
            unit: format!("bubbaloop-{name}.service"),
            node_name: Some(name.to_string()),
        });

        Ok(())
    }

    /// Returns true if a config file exists for the node.
    pub fn is_installed(name: &str) -> bool {
        Self::config_path(name).exists()
    }

    /// Start the process described in the node's config file.
    pub async fn start_unit(&self, name: &str) -> Result<()> {
        let config = Self::read_config(name)
            .ok_or_else(|| SystemdError::ServiceNotFound(name.to_string()))?;

        // Already running?
        if let Some(pid) = Self::read_pid(name) {
            if Self::is_pid_alive(pid) {
                return Ok(());
            }
        }

        // Split command into executable + args (simple whitespace split)
        let parts: Vec<&str> = config.command.split_whitespace().collect();
        let (exe, args) = parts
            .split_first()
            .ok_or_else(|| SystemdError::OperationFailed("empty command".to_string()))?;

        let child = tokio::process::Command::new(exe)
            .args(args)
            .current_dir(&config.work_dir)
            .spawn()
            .map_err(|e| SystemdError::OperationFailed(format!("Failed to spawn {name}: {e}")))?;

        let pid = child.id().ok_or_else(|| {
            SystemdError::OperationFailed("could not get PID after spawn".to_string())
        })?;

        Self::write_pid(name, pid)?;

        // Spawn watcher: when process exits, emit JobRemoved
        let name_owned = name.to_string();
        let event_tx = self.event_tx.clone();
        tokio::spawn(async move {
            // Re-own child so we can wait on it
            drop(child); // child is moved into this task

            // Poll until PID disappears (cross-platform)
            loop {
                tokio::time::sleep(std::time::Duration::from_secs(1)).await;
                if !Self::is_pid_alive(pid) {
                    break;
                }
            }

            Self::remove_pid(&name_owned);
            let result = "done"; // we don't know exit code without the handle
            let unit = format!("bubbaloop-{name_owned}.service");
            let _ = event_tx.send(SystemdSignalEvent::JobRemoved {
                unit,
                result: result.to_string(),
                node_name: Some(name_owned),
            });
        });

        self.emit(Self::job_removed_event(name, "done")); // "started" approximation
        log::info!("[NativeSupervisor] Started {name} (pid={pid})");

        Ok(())
    }

    /// Stop the process by sending SIGTERM via `/bin/kill`.
    pub async fn stop_unit(&self, name: &str) -> Result<()> {
        let pid = Self::read_pid(name)
            .filter(|&pid| Self::is_pid_alive(pid))
            .ok_or_else(|| SystemdError::ServiceNotFound(name.to_string()))?;

        // Send SIGTERM — use absolute path per security conventions
        let kill_bin = if std::path::Path::new("/bin/kill").exists() {
            "/bin/kill"
        } else {
            "/usr/bin/kill"
        };

        tokio::process::Command::new(kill_bin)
            .args(["-TERM", &pid.to_string()])
            .status()
            .await
            .map_err(|e| SystemdError::OperationFailed(e.to_string()))?;

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
        }

        Self::remove_pid(name);
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
        match Self::read_pid(name) {
            Some(pid) if Self::is_pid_alive(pid) => Ok(ActiveState::Active),
            Some(_) => {
                // Stale PID file
                Self::remove_pid(name);
                Ok(if Self::is_installed(name) {
                    ActiveState::Inactive
                } else {
                    ActiveState::Unknown("not-found".to_string())
                })
            }
            None => Ok(if Self::is_installed(name) {
                ActiveState::Inactive
            } else {
                ActiveState::Unknown("not-found".to_string())
            }),
        }
    }

    /// Returns the autostart flag from the config file.
    pub fn is_enabled(&self, name: &str) -> bool {
        Self::read_config(name)
            .map(|c| c.autostart)
            .unwrap_or(false)
    }

    /// Enable autostart (persisted to config file).
    pub fn enable_unit(&self, name: &str) -> Result<()> {
        let mut config = Self::read_config(name)
            .ok_or_else(|| SystemdError::ServiceNotFound(name.to_string()))?;
        config.autostart = true;
        Self::write_config(&config)
    }

    /// Disable autostart (persisted to config file).
    pub fn disable_unit(&self, name: &str) -> Result<()> {
        let mut config = Self::read_config(name)
            .ok_or_else(|| SystemdError::ServiceNotFound(name.to_string()))?;
        config.autostart = false;
        Self::write_config(&config)
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

    fn unique_name(prefix: &str) -> String {
        let now = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap_or_else(|_| Duration::from_secs(0))
            .as_nanos();
        format!("{prefix}-{now}")
    }

    fn cleanup_node_files(name: &str) {
        let _ = std::fs::remove_file(NativeSupervisor::config_path(name));
        let _ = std::fs::remove_file(NativeSupervisor::pid_path(name));
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
        let sup = NativeSupervisor::new();
        let name = unique_name("native-cycle");
        cleanup_node_files(&name);

        sup.install_service("/tmp", &name, "rust", Some("sleep 30"))
            .unwrap();
        assert!(NativeSupervisor::is_installed(&name));

        sup.start_unit(&name).await.unwrap();
        wait_for_active_state(&sup, &name, ActiveState::Active, 2_000).await;

        sup.stop_unit(&name).await.unwrap();
        wait_for_active_state(&sup, &name, ActiveState::Inactive, 2_000).await;

        sup.uninstall_service(&name).await.unwrap();
        assert!(!NativeSupervisor::is_installed(&name));

        cleanup_node_files(&name);
    }

    #[tokio::test]
    async fn autostart_enable_disable_persists() {
        let sup = NativeSupervisor::new();
        let name = unique_name("native-autostart");
        cleanup_node_files(&name);

        sup.install_service("/tmp", &name, "rust", Some("sleep 5"))
            .unwrap();

        assert!(!sup.is_enabled(&name));
        sup.enable_unit(&name).unwrap();
        assert!(sup.is_enabled(&name));

        sup.disable_unit(&name).unwrap();
        assert!(!sup.is_enabled(&name));

        sup.uninstall_service(&name).await.unwrap();
        cleanup_node_files(&name);
    }

    #[tokio::test]
    async fn stale_pid_file_is_cleaned_and_state_is_inactive() {
        let sup = NativeSupervisor::new();
        let name = unique_name("native-stale");
        cleanup_node_files(&name);

        sup.install_service("/tmp", &name, "rust", Some("sleep 5"))
            .unwrap();

        std::fs::write(NativeSupervisor::pid_path(&name), "4294967295").unwrap();
        let state = sup.get_active_state(&name).await.unwrap();
        assert_eq!(state, ActiveState::Inactive);
        assert!(!NativeSupervisor::pid_path(&name).exists());

        sup.uninstall_service(&name).await.unwrap();
        cleanup_node_files(&name);
    }

    #[tokio::test]
    async fn expected_errors_for_missing_service_and_empty_command() {
        let sup = NativeSupervisor::new();
        let missing = unique_name("native-missing");
        cleanup_node_files(&missing);

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
        cleanup_node_files(&empty_cmd_name);
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
        let sup = NativeSupervisor::new();
        let name = unique_name("native-signals");
        cleanup_node_files(&name);

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

        cleanup_node_files(&name);
    }
}
