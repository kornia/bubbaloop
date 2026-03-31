//! Unified process supervisor: selects systemd or native backend at startup.
//!
//! `Supervisor::detect()` tries to connect to systemd D-Bus. On success it
//! uses `SystemdClient` (Linux with systemd). On failure it falls back to
//! `NativeSupervisor` (development fallback for Docker, macOS, or any
//! environment without D-Bus).
//!
//! All call sites in `NodeManager` use this type exclusively — the systemd
//! module is purely an implementation detail.

use crate::daemon::native_supervisor::NativeSupervisor;
use crate::daemon::systemd::{self, ActiveState, SystemdClient, SystemdError, SystemdSignalEvent};
use tokio::sync::mpsc;

type Result<T> = std::result::Result<T, SystemdError>;

/// The active process-management backend.
pub enum Supervisor {
    /// systemd via D-Bus — used on Linux with systemd (Jetson, RPi, server).
    Systemd(SystemdClient),
    /// Native spawning via tokio::process — development fallback for Docker/macOS.
    Native(NativeSupervisor),
}

impl Supervisor {
    /// Detect the best available backend. Never fails — falls back to Native.
    pub async fn detect() -> Self {
        match SystemdClient::new().await {
            Ok(client) => {
                log::info!("[Supervisor] Using systemd backend");
                Supervisor::Systemd(client)
            }
            Err(err) => {
                log::warn!(
                    "[Supervisor] systemd unavailable ({err}), \
                     using native process supervisor for development fallback \
                     (Docker/macOS, no journalctl/systemd integration)"
                );
                Supervisor::Native(NativeSupervisor::new())
            }
        }
    }

    pub fn is_native(&self) -> bool {
        matches!(self, Supervisor::Native(_))
    }

    // ── Process state ──────────────────────────────────────────────────────

    /// Get the active state of a node by node name (not service name).
    pub async fn get_active_state(&self, node_name: &str) -> Result<ActiveState> {
        match self {
            Supervisor::Systemd(c) => {
                c.get_active_state(&systemd::get_service_name(node_name))
                    .await
            }
            Supervisor::Native(n) => n.get_active_state(node_name).await,
        }
    }

    /// Returns true if the node is configured for autostart.
    pub async fn is_enabled(&self, node_name: &str) -> bool {
        match self {
            Supervisor::Systemd(c) => c
                .is_enabled(&systemd::get_service_name(node_name))
                .await
                .unwrap_or(false),
            Supervisor::Native(n) => n.is_enabled(node_name),
        }
    }

    /// Returns true if a service/config file exists for the node.
    pub fn is_installed(&self, node_name: &str) -> bool {
        match self {
            Supervisor::Systemd(_) => systemd::is_service_installed(node_name),
            Supervisor::Native(_) => NativeSupervisor::is_installed(node_name),
        }
    }

    // ── Lifecycle ──────────────────────────────────────────────────────────

    pub async fn start_unit(&self, node_name: &str) -> Result<()> {
        match self {
            Supervisor::Systemd(c) => {
                c.start_unit(&systemd::get_service_name(node_name)).await
            }
            Supervisor::Native(n) => n.start_unit(node_name).await,
        }
    }

    pub async fn stop_unit(&self, node_name: &str) -> Result<()> {
        match self {
            Supervisor::Systemd(c) => {
                c.stop_unit(&systemd::get_service_name(node_name)).await
            }
            Supervisor::Native(n) => n.stop_unit(node_name).await,
        }
    }

    pub async fn restart_unit(&self, node_name: &str) -> Result<()> {
        match self {
            Supervisor::Systemd(c) => {
                c.restart_unit(&systemd::get_service_name(node_name)).await
            }
            Supervisor::Native(n) => n.restart_unit(node_name).await,
        }
    }

    pub async fn enable_unit(&self, node_name: &str) -> Result<()> {
        match self {
            Supervisor::Systemd(c) => {
                c.enable_unit(&systemd::get_service_name(node_name)).await
            }
            Supervisor::Native(n) => n.enable_unit(node_name),
        }
    }

    pub async fn disable_unit(&self, node_name: &str) -> Result<()> {
        match self {
            Supervisor::Systemd(c) => {
                c.disable_unit(&systemd::get_service_name(node_name)).await
            }
            Supervisor::Native(n) => n.disable_unit(node_name),
        }
    }

    // ── Install / uninstall ────────────────────────────────────────────────

    pub async fn install_service(
        &self,
        node_path: &str,
        node_name: &str,
        node_type: &str,
        command: Option<&str>,
        depends_on: &[String],
    ) -> Result<()> {
        match self {
            Supervisor::Systemd(_) => {
                systemd::install_service(node_path, node_name, node_type, command, depends_on)
                    .await
            }
            Supervisor::Native(n) => {
                n.install_service(node_path, node_name, node_type, command)
            }
        }
    }

    pub async fn uninstall_service(&self, node_name: &str) -> Result<()> {
        match self {
            Supervisor::Systemd(_) => systemd::uninstall_service(node_name).await,
            Supervisor::Native(n) => n.uninstall_service(node_name).await,
        }
    }

    // ── Signals ────────────────────────────────────────────────────────────

    /// Subscribe to lifecycle signals. Returns a receiver of `SystemdSignalEvent`.
    /// On systemd: real D-Bus signals. On native: mpsc events from process watcher tasks.
    pub async fn subscribe_to_signals(
        &self,
    ) -> Result<mpsc::Receiver<SystemdSignalEvent>> {
        match self {
            Supervisor::Systemd(c) => c.subscribe_to_signals().await,
            Supervisor::Native(n) => Ok(n.subscribe_to_signals()),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn unique_name(prefix: &str) -> String {
        format!(
            "{}-{}",
            prefix,
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        )
    }

    #[test]
    fn native_variant_reports_is_native() {
        let sup = Supervisor::Native(NativeSupervisor::new());
        assert!(sup.is_native());
    }

    #[tokio::test]
    async fn native_dispatcher_delegates_full_lifecycle() {
        let sup = Supervisor::Native(NativeSupervisor::new());
        let name = unique_name("sup-dlg");

        // Install through the Supervisor dispatcher
        sup.install_service("/tmp", &name, "rust", Some("sleep 30"), &[])
            .await
            .unwrap();
        assert!(sup.is_installed(&name));

        // Start through dispatcher
        sup.start_unit(&name).await.unwrap();

        // Check state through dispatcher
        let state = sup.get_active_state(&name).await.unwrap();
        assert!(
            matches!(state, ActiveState::Active | ActiveState::Inactive),
            "Expected active or inactive after start, got {:?}",
            state
        );

        // Enable / disable autostart through dispatcher
        sup.enable_unit(&name).await.unwrap();
        assert!(sup.is_enabled(&name).await);
        sup.disable_unit(&name).await.unwrap();
        assert!(!sup.is_enabled(&name).await);

        // Stop + uninstall through dispatcher
        sup.stop_unit(&name).await.unwrap();
        sup.uninstall_service(&name).await.unwrap();
        assert!(!sup.is_installed(&name));
    }
}
