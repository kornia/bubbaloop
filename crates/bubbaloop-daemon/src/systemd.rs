//! D-Bus communication with systemd user services
//!
//! This module provides native D-Bus communication with systemd,
//! avoiding shell spawning for better performance and reliability.

use std::path::PathBuf;
use thiserror::Error;
use zbus::{proxy, Connection};

#[derive(Error, Debug)]
pub enum SystemdError {
    #[error("D-Bus connection error: {0}")]
    Connection(#[from] zbus::Error),

    #[error("Service not found: {0}")]
    ServiceNotFound(String),

    #[error("Operation failed: {0}")]
    OperationFailed(String),

    #[error("IO error: {0}")]
    Io(#[from] std::io::Error),
}

pub type Result<T> = std::result::Result<T, SystemdError>;

/// Active state of a systemd unit
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ActiveState {
    Active,
    Reloading,
    Inactive,
    Failed,
    Activating,
    Deactivating,
    Unknown(String),
}

impl From<&str> for ActiveState {
    fn from(s: &str) -> Self {
        match s {
            "active" => ActiveState::Active,
            "reloading" => ActiveState::Reloading,
            "inactive" => ActiveState::Inactive,
            "failed" => ActiveState::Failed,
            "activating" => ActiveState::Activating,
            "deactivating" => ActiveState::Deactivating,
            other => ActiveState::Unknown(other.to_string()),
        }
    }
}

/// Unit file state (enabled/disabled)
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum UnitFileState {
    Enabled,
    Disabled,
    Static,
    Masked,
    Generated,
    Transient,
    Unknown(String),
}

impl From<&str> for UnitFileState {
    fn from(s: &str) -> Self {
        match s {
            "enabled" => UnitFileState::Enabled,
            "disabled" => UnitFileState::Disabled,
            "static" => UnitFileState::Static,
            "masked" => UnitFileState::Masked,
            "generated" => UnitFileState::Generated,
            "transient" => UnitFileState::Transient,
            other => UnitFileState::Unknown(other.to_string()),
        }
    }
}

/// D-Bus proxy for systemd manager interface
#[proxy(
    interface = "org.freedesktop.systemd1.Manager",
    default_service = "org.freedesktop.systemd1",
    default_path = "/org/freedesktop/systemd1"
)]
trait SystemdManager {
    /// Start a unit
    fn start_unit(&self, name: &str, mode: &str) -> zbus::Result<zbus::zvariant::OwnedObjectPath>;

    /// Stop a unit
    fn stop_unit(&self, name: &str, mode: &str) -> zbus::Result<zbus::zvariant::OwnedObjectPath>;

    /// Restart a unit
    fn restart_unit(&self, name: &str, mode: &str)
        -> zbus::Result<zbus::zvariant::OwnedObjectPath>;

    /// Reload systemd daemon
    fn reload(&self) -> zbus::Result<()>;

    /// Get unit object path
    fn get_unit(&self, name: &str) -> zbus::Result<zbus::zvariant::OwnedObjectPath>;

    /// Load unit (returns path even if unit doesn't exist yet)
    fn load_unit(&self, name: &str) -> zbus::Result<zbus::zvariant::OwnedObjectPath>;

    /// Enable unit files
    fn enable_unit_files(
        &self,
        files: &[&str],
        runtime: bool,
        force: bool,
    ) -> zbus::Result<(bool, Vec<(String, String, String)>)>;

    /// Disable unit files
    fn disable_unit_files(
        &self,
        files: &[&str],
        runtime: bool,
    ) -> zbus::Result<Vec<(String, String, String)>>;

    /// Get unit file state
    fn get_unit_file_state(&self, name: &str) -> zbus::Result<String>;
}

/// D-Bus proxy for systemd unit interface
#[proxy(
    interface = "org.freedesktop.systemd1.Unit",
    default_service = "org.freedesktop.systemd1"
)]
trait SystemdUnit {
    /// Active state property
    #[zbus(property)]
    fn active_state(&self) -> zbus::Result<String>;

    /// Sub state property
    #[zbus(property)]
    fn sub_state(&self) -> zbus::Result<String>;

    /// Unit file state property
    #[zbus(property)]
    fn unit_file_state(&self) -> zbus::Result<String>;

    /// Description property
    #[zbus(property)]
    fn description(&self) -> zbus::Result<String>;

    /// Load state property
    #[zbus(property)]
    fn load_state(&self) -> zbus::Result<String>;
}

/// Systemd client for managing user services
pub struct SystemdClient {
    connection: Connection,
}

impl SystemdClient {
    /// Create a new systemd client connected to the user session bus
    pub async fn new() -> Result<Self> {
        let connection = Connection::session().await?;
        Ok(Self { connection })
    }

    /// Get the systemd manager proxy
    async fn manager(&self) -> Result<SystemdManagerProxy<'_>> {
        Ok(SystemdManagerProxy::new(&self.connection).await?)
    }

    /// Get the active state of a unit
    pub async fn get_active_state(&self, unit_name: &str) -> Result<ActiveState> {
        let manager = self.manager().await?;

        // Try to load the unit - this works even if unit doesn't exist
        match manager.load_unit(unit_name).await {
            Ok(path) => {
                let unit = SystemdUnitProxy::builder(&self.connection)
                    .path(path)?
                    .build()
                    .await?;

                let load_state = unit.load_state().await?;
                if load_state == "not-found" {
                    return Ok(ActiveState::Unknown("not-found".to_string()));
                }

                let state = unit.active_state().await?;
                Ok(ActiveState::from(state.as_str()))
            }
            Err(_) => Ok(ActiveState::Unknown("not-found".to_string())),
        }
    }

    /// Check if a unit file is enabled
    pub async fn is_enabled(&self, unit_name: &str) -> Result<bool> {
        let manager = self.manager().await?;

        match manager.get_unit_file_state(unit_name).await {
            Ok(state) => Ok(state == "enabled"),
            Err(_) => Ok(false),
        }
    }

    /// Get the unit file state
    pub async fn get_unit_file_state(&self, unit_name: &str) -> Result<UnitFileState> {
        let manager = self.manager().await?;

        match manager.get_unit_file_state(unit_name).await {
            Ok(state) => Ok(UnitFileState::from(state.as_str())),
            Err(_) => Ok(UnitFileState::Unknown("not-found".to_string())),
        }
    }

    /// Start a unit
    pub async fn start_unit(&self, unit_name: &str) -> Result<()> {
        let manager = self.manager().await?;
        manager
            .start_unit(unit_name, "replace")
            .await
            .map_err(|e| {
                SystemdError::OperationFailed(format!("Failed to start {}: {}", unit_name, e))
            })?;
        Ok(())
    }

    /// Stop a unit
    pub async fn stop_unit(&self, unit_name: &str) -> Result<()> {
        let manager = self.manager().await?;
        manager.stop_unit(unit_name, "replace").await.map_err(|e| {
            SystemdError::OperationFailed(format!("Failed to stop {}: {}", unit_name, e))
        })?;
        Ok(())
    }

    /// Restart a unit
    pub async fn restart_unit(&self, unit_name: &str) -> Result<()> {
        let manager = self.manager().await?;
        manager
            .restart_unit(unit_name, "replace")
            .await
            .map_err(|e| {
                SystemdError::OperationFailed(format!("Failed to restart {}: {}", unit_name, e))
            })?;
        Ok(())
    }

    /// Enable a unit for autostart
    pub async fn enable_unit(&self, unit_name: &str) -> Result<()> {
        let manager = self.manager().await?;
        manager
            .enable_unit_files(&[unit_name], false, false)
            .await
            .map_err(|e| {
                SystemdError::OperationFailed(format!("Failed to enable {}: {}", unit_name, e))
            })?;
        Ok(())
    }

    /// Disable a unit from autostart
    pub async fn disable_unit(&self, unit_name: &str) -> Result<()> {
        let manager = self.manager().await?;
        manager
            .disable_unit_files(&[unit_name], false)
            .await
            .map_err(|e| {
                SystemdError::OperationFailed(format!("Failed to disable {}: {}", unit_name, e))
            })?;
        Ok(())
    }

    /// Reload systemd daemon (after adding/removing unit files)
    pub async fn daemon_reload(&self) -> Result<()> {
        let manager = self.manager().await?;
        manager.reload().await?;
        Ok(())
    }
}

/// Get the systemd user directory path
pub fn get_systemd_user_dir() -> PathBuf {
    dirs::home_dir()
        .unwrap_or_else(|| PathBuf::from("/tmp"))
        .join(".config/systemd/user")
}

/// Get the service name for a node
pub fn get_service_name(node_name: &str) -> String {
    format!("bubbaloop-{}.service", node_name)
}

/// Get the full service file path
pub fn get_service_path(node_name: &str) -> PathBuf {
    get_systemd_user_dir().join(get_service_name(node_name))
}

/// Generate a systemd service unit file content
pub fn generate_service_unit(
    node_path: &str,
    name: &str,
    node_type: &str,
    command: Option<&str>,
) -> String {
    let home = dirs::home_dir().unwrap_or_else(|| PathBuf::from("/home/user"));
    let cargo_path = home.join(".cargo/bin/cargo");
    let pixi_bin = home.join(".pixi/bin");
    let path_env = format!(
        "PATH={}:{}:/usr/local/bin:/usr/bin:/bin",
        home.join(".cargo/bin").display(),
        pixi_bin.display()
    );

    let (exec_start, environment) = if let Some(cmd) = command {
        if cmd.starts_with("cargo ") {
            (
                cmd.replacen("cargo ", &format!("{} ", cargo_path.display()), 1),
                "RUST_LOG=info".to_string(),
            )
        } else {
            // Resolve relative paths to absolute
            let resolved = std::path::Path::new(node_path).join(cmd);
            (
                resolved.to_string_lossy().to_string(),
                "RUST_LOG=info".to_string(),
            )
        }
    } else if node_type == "rust" {
        (
            format!("{} run --release", cargo_path.display()),
            "RUST_LOG=info".to_string(),
        )
    } else {
        // Python
        let venv_python = std::path::Path::new(node_path).join("venv/bin/python");
        (
            format!("{} main.py", venv_python.display()),
            "PYTHONUNBUFFERED=1".to_string(),
        )
    };

    format!(
        r#"[Unit]
Description=Bubbaloop Node: {name}
After=network.target

[Service]
Type=simple
WorkingDirectory={node_path}
ExecStart={exec_start}
Restart=on-failure
RestartSec=5
Environment={environment}
Environment={path_env}

[Install]
WantedBy=default.target
"#
    )
}

/// Install a service unit file
pub async fn install_service(
    node_path: &str,
    name: &str,
    node_type: &str,
    command: Option<&str>,
) -> Result<()> {
    let service_dir = get_systemd_user_dir();
    std::fs::create_dir_all(&service_dir)?;

    let service_path = get_service_path(name);
    let content = generate_service_unit(node_path, name, node_type, command);
    std::fs::write(&service_path, content)?;

    // Reload systemd to pick up the new unit
    let client = SystemdClient::new().await?;
    client.daemon_reload().await?;

    Ok(())
}

/// Uninstall a service unit file
pub async fn uninstall_service(name: &str) -> Result<()> {
    let client = SystemdClient::new().await?;
    let service_name = get_service_name(name);

    // Stop and disable first (ignore errors)
    let _ = client.stop_unit(&service_name).await;
    let _ = client.disable_unit(&service_name).await;

    // Remove the unit file
    let service_path = get_service_path(name);
    if service_path.exists() {
        std::fs::remove_file(&service_path)?;
    }

    // Reload daemon
    client.daemon_reload().await?;

    Ok(())
}

/// Check if a service is installed (unit file exists)
pub fn is_service_installed(name: &str) -> bool {
    get_service_path(name).exists()
}
