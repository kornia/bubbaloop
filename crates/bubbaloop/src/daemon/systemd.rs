//! D-Bus communication with systemd user services
//!
//! This module provides native D-Bus communication with systemd,
//! avoiding shell spawning for better performance and reliability.

use std::path::PathBuf;
use thiserror::Error;
use tokio::sync::mpsc;
use zbus::zvariant::OwnedObjectPath;
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

    #[error("Invalid node name: {0}")]
    InvalidNodeName(String),

    #[error("Invalid input for systemd unit: {0}")]
    InvalidInput(String),

    #[error("D-Bus call timed out")]
    Timeout,
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

    /// Subscribe to signals (required before receiving signals)
    fn subscribe(&self) -> zbus::Result<()>;

    /// Unsubscribe from signals
    fn unsubscribe(&self) -> zbus::Result<()>;

    /// Signal: Job removed (job completed/failed/cancelled)
    #[zbus(signal)]
    fn job_removed(
        &self,
        id: u32,
        job: OwnedObjectPath,
        unit: &str,
        result: &str,
    ) -> zbus::Result<()>;

    /// Signal: Unit added
    #[zbus(signal)]
    fn unit_new(&self, id: &str, unit: OwnedObjectPath) -> zbus::Result<()>;

    /// Signal: Unit removed
    #[zbus(signal)]
    fn unit_removed(&self, id: &str, unit: OwnedObjectPath) -> zbus::Result<()>;
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

    /// Get the active state of a unit.
    ///
    /// Includes a 5-second timeout to prevent indefinite hangs if the D-Bus
    /// connection is stalled (e.g. by signal backpressure).
    pub async fn get_active_state(&self, unit_name: &str) -> Result<ActiveState> {
        match tokio::time::timeout(
            std::time::Duration::from_secs(5),
            self.get_active_state_inner(unit_name),
        )
        .await
        {
            Ok(result) => result,
            Err(_) => {
                log::warn!("D-Bus get_active_state timed out for {}", unit_name);
                Err(SystemdError::Timeout)
            }
        }
    }

    async fn get_active_state_inner(&self, unit_name: &str) -> Result<ActiveState> {
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

    /// Check if a unit file is enabled.
    ///
    /// Includes a 5-second timeout to prevent D-Bus stalls.
    pub async fn is_enabled(&self, unit_name: &str) -> Result<bool> {
        match tokio::time::timeout(std::time::Duration::from_secs(5), async {
            let manager = self.manager().await?;
            match manager.get_unit_file_state(unit_name).await {
                Ok(state) => Ok(state == "enabled"),
                Err(_) => Ok(false),
            }
        })
        .await
        {
            Ok(result) => result,
            Err(_) => {
                log::warn!("D-Bus is_enabled timed out for {}", unit_name);
                Ok(false)
            }
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

    /// Stop a unit.
    ///
    /// Includes a 10-second timeout to prevent D-Bus stalls.
    pub async fn stop_unit(&self, unit_name: &str) -> Result<()> {
        match tokio::time::timeout(std::time::Duration::from_secs(10), async {
            let manager = self.manager().await?;
            manager.stop_unit(unit_name, "replace").await.map_err(|e| {
                SystemdError::OperationFailed(format!("Failed to stop {}: {}", unit_name, e))
            })?;
            Ok(())
        })
        .await
        {
            Ok(result) => result,
            Err(_) => {
                log::warn!("D-Bus stop_unit timed out for {}", unit_name);
                Err(SystemdError::Timeout)
            }
        }
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

    /// Subscribe to systemd signals and return a receiver for signal events
    pub async fn subscribe_to_signals(&self) -> Result<mpsc::Receiver<SystemdSignalEvent>> {
        let manager = self.manager().await?;

        // Subscribe to signals first
        manager.subscribe().await?;
        log::info!("Subscribed to systemd signals");

        let (tx, rx) = mpsc::channel(100);

        // Clone connection for the signal listener task
        let connection = self.connection.clone();

        // Spawn signal listener task
        tokio::spawn(async move {
            let manager_proxy = match SystemdManagerProxy::new(&connection).await {
                Ok(proxy) => proxy,
                Err(e) => {
                    log::error!("Failed to create manager proxy for signals: {}", e);
                    return;
                }
            };

            // Spawn separate tasks for each signal type
            let tx_job = tx.clone();
            let mut job_removed = match manager_proxy.receive_job_removed().await {
                Ok(stream) => stream,
                Err(e) => {
                    log::error!("Failed to receive job_removed signals: {}", e);
                    return;
                }
            };

            tokio::spawn(async move {
                use futures::StreamExt;
                while let Some(signal) = job_removed.next().await {
                    if let Ok(args) = signal.args() {
                        let unit = args.unit.to_string();
                        // Only care about bubbaloop services
                        if unit.starts_with("bubbaloop-") {
                            let node_name = extract_node_name(&unit);
                            let event = SystemdSignalEvent::JobRemoved {
                                unit,
                                result: args.result.to_string(),
                                node_name,
                            };
                            if tx_job.send(event).await.is_err() {
                                break;
                            }
                        }
                    }
                }
            });

            let tx_new = tx.clone();
            let mut unit_new = match manager_proxy.receive_unit_new().await {
                Ok(stream) => stream,
                Err(e) => {
                    log::error!("Failed to receive unit_new signals: {}", e);
                    return;
                }
            };

            tokio::spawn(async move {
                use futures::StreamExt;
                while let Some(signal) = unit_new.next().await {
                    if let Ok(args) = signal.args() {
                        let unit = args.id.to_string();
                        if unit.starts_with("bubbaloop-") {
                            let node_name = extract_node_name(&unit);
                            let event = SystemdSignalEvent::UnitNew { unit, node_name };
                            if tx_new.send(event).await.is_err() {
                                break;
                            }
                        }
                    }
                }
            });

            let tx_removed = tx;
            let mut unit_removed = match manager_proxy.receive_unit_removed().await {
                Ok(stream) => stream,
                Err(e) => {
                    log::error!("Failed to receive unit_removed signals: {}", e);
                    return;
                }
            };

            tokio::spawn(async move {
                use futures::StreamExt;
                while let Some(signal) = unit_removed.next().await {
                    if let Ok(args) = signal.args() {
                        let unit = args.id.to_string();
                        if unit.starts_with("bubbaloop-") {
                            let node_name = extract_node_name(&unit);
                            let event = SystemdSignalEvent::UnitRemoved { unit, node_name };
                            if tx_removed.send(event).await.is_err() {
                                break;
                            }
                        }
                    }
                }
            });
        });

        Ok(rx)
    }
}

/// Event from systemd signal
#[derive(Debug, Clone)]
pub enum SystemdSignalEvent {
    /// Job completed/failed/cancelled
    JobRemoved {
        unit: String,
        result: String,
        node_name: Option<String>,
    },
    /// Unit added
    UnitNew {
        unit: String,
        node_name: Option<String>,
    },
    /// Unit removed
    UnitRemoved {
        unit: String,
        node_name: Option<String>,
    },
}

/// Extract node name from service name (e.g., "bubbaloop-rtsp-camera.service" -> "rtsp-camera")
fn extract_node_name(unit: &str) -> Option<String> {
    if unit.starts_with("bubbaloop-") && unit.ends_with(".service") {
        let name = unit.strip_prefix("bubbaloop-")?.strip_suffix(".service")?;
        if name.is_empty() {
            None
        } else {
            Some(name.to_string())
        }
    } else {
        None
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

/// Validate node name for systemd service naming
fn validate_node_name(name: &str) -> Result<()> {
    // Node names should be alphanumeric with hyphens/underscores only
    if !name
        .chars()
        .all(|c| c.is_alphanumeric() || c == '-' || c == '_')
    {
        return Err(SystemdError::InvalidNodeName(format!(
            "'{}' contains invalid characters (only alphanumeric, hyphens, and underscores allowed)",
            name
        )));
    }
    if name.is_empty() || name.len() > 64 {
        return Err(SystemdError::InvalidNodeName(format!(
            "'{}' has invalid length (must be 1-64 characters)",
            name
        )));
    }
    Ok(())
}

/// Sanitize a string for use in systemd unit file Description field
fn sanitize_description(s: &str) -> String {
    // Remove newlines and special characters that could break unit file parsing
    s.chars()
        .filter(|c| !matches!(c, '\n' | '\r' | '[' | ']'))
        .take(200) // Reasonable length limit for descriptions
        .collect()
}

/// Sanitize a path for use in systemd unit files
fn sanitize_path(path: &str) -> Result<String> {
    // Basic validation - paths should not contain newlines or null bytes
    if path.contains('\n') || path.contains('\r') || path.contains('\0') {
        return Err(SystemdError::InvalidInput(format!(
            "Path '{}' contains invalid characters",
            path
        )));
    }
    // Ensure it's a valid UTF-8 path
    Ok(path.to_string())
}

/// Sanitize a command for use in ExecStart
fn sanitize_command(cmd: &str) -> Result<String> {
    // Commands should not contain newlines or null bytes
    if cmd.contains('\n') || cmd.contains('\r') || cmd.contains('\0') {
        return Err(SystemdError::InvalidInput(
            "Command contains invalid characters".to_string(),
        ));
    }
    // Prevent injection of systemd directives by checking for suspicious patterns
    if cmd.contains("[Unit]") || cmd.contains("[Service]") || cmd.contains("[Install]") {
        return Err(SystemdError::InvalidInput(
            "Command contains systemd unit section markers".to_string(),
        ));
    }
    Ok(cmd.to_string())
}

/// Generate a systemd service unit file content
pub fn generate_service_unit(
    node_path: &str,
    name: &str,
    node_type: &str,
    command: Option<&str>,
    depends_on: &[String],
) -> Result<String> {
    // Validate and sanitize inputs
    validate_node_name(name)?;
    let safe_node_path = sanitize_path(node_path)?;
    let safe_name = sanitize_description(name);
    let home = dirs::home_dir().unwrap_or_else(|| PathBuf::from("/home/user"));
    let cargo_path = home.join(".cargo/bin/cargo");
    let pixi_bin_dir = home.join(".pixi/bin");
    let pixi_path = pixi_bin_dir.join("pixi");
    let path_env = format!(
        "PATH={}:{}:/usr/local/bin:/usr/bin:/bin",
        home.join(".cargo/bin").display(),
        pixi_bin_dir.display()
    );

    let (exec_start, environment) = if let Some(cmd) = command {
        let safe_cmd = sanitize_command(cmd)?;
        if safe_cmd.starts_with("cargo ") {
            (
                safe_cmd.replacen("cargo ", &format!("{} ", cargo_path.display()), 1),
                "RUST_LOG=info".to_string(),
            )
        } else if safe_cmd.starts_with("pixi ") {
            // Resolve pixi to absolute path like cargo
            (
                safe_cmd.replacen("pixi ", &format!("{} ", pixi_path.display()), 1),
                "PYTHONUNBUFFERED=1".to_string(),
            )
        } else if safe_cmd.starts_with("python3 ")
            || safe_cmd.starts_with("python ")
            || safe_cmd.starts_with("/")
        {
            // Keep interpreter commands and absolute paths as-is
            // WorkingDirectory will handle relative script paths
            (safe_cmd, "PYTHONUNBUFFERED=1".to_string())
        } else {
            // Resolve relative binary paths to absolute
            let resolved = std::path::Path::new(&safe_node_path).join(&safe_cmd);
            (
                resolved.to_string_lossy().to_string(),
                "RUST_LOG=info".to_string(),
            )
        }
    } else if node_type == "rust" {
        // Point directly to the built binary so starting a cleaned node
        // fails fast instead of silently triggering a full rebuild.
        let binary = std::path::Path::new(&safe_node_path)
            .join("target/release")
            .join(&safe_name);
        (
            binary.to_string_lossy().to_string(),
            "RUST_LOG=info".to_string(),
        )
    } else {
        // Python
        let venv_python = std::path::Path::new(&safe_node_path).join("venv/bin/python");
        (
            format!("{} main.py", venv_python.display()),
            "PYTHONUNBUFFERED=1".to_string(),
        )
    };

    // Validate and generate dependency lines for systemd
    let (after_line, requires_line) = if depends_on.is_empty() {
        ("After=network.target".to_string(), String::new())
    } else {
        // Validate all dependency names
        for dep in depends_on {
            validate_node_name(dep)?;
        }
        let dep_services: Vec<String> =
            depends_on.iter().map(|dep| get_service_name(dep)).collect();
        let deps_str = dep_services.join(" ");
        (
            format!("After=network.target {}", deps_str),
            format!("Requires={}", deps_str),
        )
    };

    // Build the requires line (empty if no dependencies)
    let requires_section = if requires_line.is_empty() {
        String::new()
    } else {
        format!("\n{}", requires_line)
    };

    Ok(format!(
        r#"[Unit]
Description=Bubbaloop Node: {safe_name}
{after_line}{requires_section}

[Service]
Type=simple
WorkingDirectory={safe_node_path}
ExecStart={exec_start}
Restart=on-failure
RestartSec=5
Environment={environment}
Environment={path_env}

# Security hardening (user service compatible)
NoNewPrivileges=true
ProtectSystem=strict
PrivateTmp=true
ProtectKernelTunables=true
# Note: ProtectKernelModules requires capabilities not available in user services
# ProtectKernelModules=true
ProtectControlGroups=true
# Robotics-compatible settings (allow RT scheduling and JIT)
RestrictRealtime=false
MemoryDenyWriteExecute=false

[Install]
WantedBy=default.target
"#
    ))
}

/// Install a service unit file
pub async fn install_service(
    node_path: &str,
    name: &str,
    node_type: &str,
    command: Option<&str>,
    depends_on: &[String],
) -> Result<()> {
    let service_dir = get_systemd_user_dir();
    std::fs::create_dir_all(&service_dir)?;

    let service_path = get_service_path(name);
    let content = generate_service_unit(node_path, name, node_type, command, depends_on)?;
    std::fs::write(&service_path, &content)?;

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

#[cfg(test)]
mod tests {
    use super::*;

    // ActiveState parsing
    #[test]
    fn test_active_state_from_known_values() {
        assert_eq!(ActiveState::from("active"), ActiveState::Active);
        assert_eq!(ActiveState::from("reloading"), ActiveState::Reloading);
        assert_eq!(ActiveState::from("inactive"), ActiveState::Inactive);
        assert_eq!(ActiveState::from("failed"), ActiveState::Failed);
        assert_eq!(ActiveState::from("activating"), ActiveState::Activating);
        assert_eq!(ActiveState::from("deactivating"), ActiveState::Deactivating);
    }

    #[test]
    fn test_active_state_from_unknown() {
        assert_eq!(
            ActiveState::from("unknown-state"),
            ActiveState::Unknown("unknown-state".to_string())
        );
        assert_eq!(
            ActiveState::from("weird"),
            ActiveState::Unknown("weird".to_string())
        );
    }

    // UnitFileState parsing
    #[test]
    fn test_unit_file_state_from_known_values() {
        assert_eq!(UnitFileState::from("enabled"), UnitFileState::Enabled);
        assert_eq!(UnitFileState::from("disabled"), UnitFileState::Disabled);
        assert_eq!(UnitFileState::from("static"), UnitFileState::Static);
        assert_eq!(UnitFileState::from("masked"), UnitFileState::Masked);
        assert_eq!(UnitFileState::from("generated"), UnitFileState::Generated);
        assert_eq!(UnitFileState::from("transient"), UnitFileState::Transient);
    }

    #[test]
    fn test_unit_file_state_from_unknown() {
        assert_eq!(
            UnitFileState::from("not-found"),
            UnitFileState::Unknown("not-found".to_string())
        );
        assert_eq!(
            UnitFileState::from("custom"),
            UnitFileState::Unknown("custom".to_string())
        );
    }

    // extract_node_name
    #[test]
    fn test_extract_node_name_valid() {
        assert_eq!(
            extract_node_name("bubbaloop-rtsp-camera.service"),
            Some("rtsp-camera".to_string())
        );
        assert_eq!(
            extract_node_name("bubbaloop-weather.service"),
            Some("weather".to_string())
        );
        assert_eq!(
            extract_node_name("bubbaloop-my-node.service"),
            Some("my-node".to_string())
        );
    }

    #[test]
    fn test_extract_node_name_no_prefix() {
        assert_eq!(extract_node_name("rtsp-camera.service"), None);
        assert_eq!(extract_node_name("systemd-resolved.service"), None);
    }

    #[test]
    fn test_extract_node_name_no_suffix() {
        assert_eq!(extract_node_name("bubbaloop-rtsp-camera"), None);
        assert_eq!(extract_node_name("bubbaloop-weather.timer"), None);
    }

    #[test]
    fn test_extract_node_name_empty() {
        assert_eq!(extract_node_name(""), None);
        assert_eq!(extract_node_name("bubbaloop-.service"), None);
    }

    // get_service_name
    #[test]
    fn test_get_service_name() {
        assert_eq!(
            get_service_name("rtsp-camera"),
            "bubbaloop-rtsp-camera.service"
        );
        assert_eq!(get_service_name("weather"), "bubbaloop-weather.service");
        assert_eq!(get_service_name("my_node"), "bubbaloop-my_node.service");
    }

    // validate_node_name
    #[test]
    fn test_validate_node_name_valid() {
        assert!(validate_node_name("rtsp-camera").is_ok());
        assert!(validate_node_name("weather").is_ok());
        assert!(validate_node_name("my_node").is_ok());
        assert!(validate_node_name("node123").is_ok());
        assert!(validate_node_name("a-b-c_d").is_ok());
        assert!(validate_node_name("a").is_ok()); // minimum length
        assert!(validate_node_name(&"a".repeat(64)).is_ok()); // maximum length
    }

    #[test]
    fn test_validate_node_name_special_chars() {
        assert!(validate_node_name("node.with.dots").is_err());
        assert!(validate_node_name("node/with/slash").is_err());
        assert!(validate_node_name("node with spaces").is_err());
        assert!(validate_node_name("node@email").is_err());
        assert!(validate_node_name("node$var").is_err());
        assert!(validate_node_name("node!").is_err());
    }

    #[test]
    fn test_validate_node_name_empty() {
        let result = validate_node_name("");
        assert!(result.is_err());
        if let Err(SystemdError::InvalidNodeName(msg)) = result {
            assert!(msg.contains("invalid length"));
        }
    }

    #[test]
    fn test_validate_node_name_too_long() {
        let long_name = "a".repeat(65);
        let result = validate_node_name(&long_name);
        assert!(result.is_err());
        if let Err(SystemdError::InvalidNodeName(msg)) = result {
            assert!(msg.contains("invalid length"));
        }
    }

    #[test]
    fn test_validate_node_name_injection_attempt() {
        assert!(validate_node_name("node;rm-rf").is_err());
        assert!(validate_node_name("node\nmalicious").is_err());
        assert!(validate_node_name("node|cmd").is_err());
    }

    // sanitize_description
    #[test]
    fn test_sanitize_description_removes_newlines() {
        assert_eq!(sanitize_description("line1\nline2"), "line1line2");
        assert_eq!(sanitize_description("line1\r\nline2"), "line1line2");
    }

    #[test]
    fn test_sanitize_description_removes_brackets() {
        assert_eq!(
            sanitize_description("description [with] brackets"),
            "description with brackets"
        );
        assert_eq!(sanitize_description("[Unit]Description"), "UnitDescription");
    }

    #[test]
    fn test_sanitize_description_truncates_long() {
        let long_desc = "a".repeat(300);
        let sanitized = sanitize_description(&long_desc);
        assert_eq!(sanitized.len(), 200);
        assert_eq!(sanitized, "a".repeat(200));
    }

    // sanitize_path
    #[test]
    fn test_sanitize_path_valid() {
        assert!(sanitize_path("/home/user/node").is_ok());
        assert!(sanitize_path("/tmp/test-path_123").is_ok());
        assert!(sanitize_path("./relative/path").is_ok());
    }

    #[test]
    fn test_sanitize_path_rejects_newlines() {
        assert!(sanitize_path("/home/user\n/node").is_err());
        assert!(sanitize_path("/home/user\r\n/node").is_err());
    }

    #[test]
    fn test_sanitize_path_rejects_null() {
        assert!(sanitize_path("/home/user\0/node").is_err());
    }

    // sanitize_command
    #[test]
    fn test_sanitize_command_valid() {
        assert!(sanitize_command("cargo run").is_ok());
        assert!(sanitize_command("python main.py --arg value").is_ok());
        assert!(sanitize_command("/usr/bin/node script.js").is_ok());
    }

    #[test]
    fn test_sanitize_command_rejects_newlines() {
        assert!(sanitize_command("cargo run\nmalicious").is_err());
        assert!(sanitize_command("python\r\nrm -rf /").is_err());
    }

    #[test]
    fn test_sanitize_command_rejects_section_markers() {
        assert!(sanitize_command("cargo run && [Unit]").is_err());
        assert!(sanitize_command("[Service]\nExecStart=evil").is_err());
        assert!(sanitize_command("[Install] WantedBy=multi-user.target").is_err());
    }

    // generate_service_unit
    #[test]
    fn test_generate_service_unit_rust_default() {
        let result = generate_service_unit(
            "/home/user/.bubbaloop/nodes/test-node",
            "test-node",
            "rust",
            None,
            &[],
        );
        assert!(result.is_ok());
        let content = result.unwrap();

        assert!(content.contains("Description=Bubbaloop Node: test-node"));
        assert!(content.contains("After=network.target"));
        assert!(!content.contains("Requires="));
        assert!(content.contains("WorkingDirectory=/home/user/.bubbaloop/nodes/test-node"));
        assert!(content
            .contains("ExecStart=/home/user/.bubbaloop/nodes/test-node/target/release/test-node"));
        assert!(content.contains("Environment=RUST_LOG=info"));
        assert!(content.contains("WantedBy=default.target"));
    }

    #[test]
    fn test_generate_service_unit_python_default() {
        let result = generate_service_unit(
            "/home/user/.bubbaloop/nodes/py-node",
            "py-node",
            "python",
            None,
            &[],
        );
        assert!(result.is_ok());
        let content = result.unwrap();

        assert!(content.contains("Description=Bubbaloop Node: py-node"));
        assert!(content.contains("WorkingDirectory=/home/user/.bubbaloop/nodes/py-node"));
        assert!(content
            .contains("ExecStart=/home/user/.bubbaloop/nodes/py-node/venv/bin/python main.py"));
        assert!(content.contains("Environment=PYTHONUNBUFFERED=1"));
    }

    #[test]
    fn test_generate_service_unit_cargo_command() {
        let result = generate_service_unit(
            "/home/user/.bubbaloop/nodes/dev-node",
            "dev-node",
            "rust",
            Some("cargo run --release"),
            &[],
        );
        assert!(result.is_ok());
        let content = result.unwrap();

        // Should replace "cargo " with absolute path
        assert!(content.contains("/.cargo/bin/cargo run --release"));
        assert!(content.contains("Environment=RUST_LOG=info"));
    }

    #[test]
    fn test_generate_service_unit_pixi_command() {
        let result = generate_service_unit(
            "/home/user/.bubbaloop/nodes/pixi-node",
            "pixi-node",
            "python",
            Some("pixi run start"),
            &[],
        );
        assert!(result.is_ok());
        let content = result.unwrap();

        // Should replace "pixi " with absolute path
        assert!(content.contains("/.pixi/bin/pixi run start"));
        assert!(content.contains("Environment=PYTHONUNBUFFERED=1"));
    }

    #[test]
    fn test_generate_service_unit_with_dependencies() {
        let result = generate_service_unit(
            "/home/user/.bubbaloop/nodes/dependent-node",
            "dependent-node",
            "rust",
            None,
            &["dep1".to_string(), "dep2".to_string()],
        );
        assert!(result.is_ok());
        let content = result.unwrap();

        assert!(
            content.contains("After=network.target bubbaloop-dep1.service bubbaloop-dep2.service")
        );
        assert!(content.contains("Requires=bubbaloop-dep1.service bubbaloop-dep2.service"));
    }

    #[test]
    fn test_generate_service_unit_invalid_name() {
        let result = generate_service_unit(
            "/home/user/.bubbaloop/nodes/bad-node",
            "bad node with spaces",
            "rust",
            None,
            &[],
        );
        assert!(result.is_err());
        if let Err(SystemdError::InvalidNodeName(_)) = result {
            // Expected error type
        } else {
            panic!("Expected InvalidNodeName error");
        }
    }

    #[test]
    fn test_generate_service_unit_invalid_command() {
        let result = generate_service_unit(
            "/home/user/.bubbaloop/nodes/test-node",
            "test-node",
            "rust",
            Some("cargo run\n[Unit]\nDescription=evil"),
            &[],
        );
        assert!(result.is_err());
        if let Err(SystemdError::InvalidInput(_)) = result {
            // Expected error type
        } else {
            panic!("Expected InvalidInput error");
        }
    }

    #[test]
    fn test_generate_service_unit_invalid_dependency_name() {
        let result = generate_service_unit(
            "/home/user/.bubbaloop/nodes/test-node",
            "test-node",
            "rust",
            None,
            &["valid-dep".to_string(), "bad dep".to_string()],
        );
        assert!(result.is_err());
        if let Err(SystemdError::InvalidNodeName(_)) = result {
            // Expected error type
        } else {
            panic!("Expected InvalidNodeName error");
        }
    }

    #[test]
    fn test_generate_service_unit_security_hardening() {
        let result = generate_service_unit(
            "/home/user/.bubbaloop/nodes/secure-node",
            "secure-node",
            "rust",
            None,
            &[],
        );
        assert!(result.is_ok());
        let content = result.unwrap();

        // Check for security hardening directives
        assert!(content.contains("NoNewPrivileges=true"));
        assert!(content.contains("ProtectSystem=strict"));
        assert!(content.contains("PrivateTmp=true"));
        assert!(content.contains("ProtectKernelTunables=true"));
        assert!(content.contains("ProtectControlGroups=true"));

        // Robotics-compatible settings
        assert!(content.contains("RestrictRealtime=false"));
        assert!(content.contains("MemoryDenyWriteExecute=false"));
    }

    #[test]
    fn test_generate_service_unit_restart_policy() {
        let result = generate_service_unit(
            "/home/user/.bubbaloop/nodes/restart-node",
            "restart-node",
            "rust",
            None,
            &[],
        );
        assert!(result.is_ok());
        let content = result.unwrap();

        assert!(content.contains("Restart=on-failure"));
        assert!(content.contains("RestartSec=5"));
    }

    #[test]
    fn test_sanitize_description_preserves_valid_chars() {
        let desc = "My node with numbers 123 and symbols: - _ / @ !";
        let sanitized = sanitize_description(desc);
        // Should keep everything except brackets and newlines
        assert!(sanitized.contains("My node with numbers 123"));
        assert!(sanitized.contains("- _ / @ !"));
    }

    #[test]
    fn test_generate_service_unit_absolute_path_command() {
        let result = generate_service_unit(
            "/home/user/.bubbaloop/nodes/abs-node",
            "abs-node",
            "python",
            Some("/usr/bin/python3 script.py"),
            &[],
        );
        assert!(result.is_ok());
        let content = result.unwrap();

        // Absolute paths should be preserved
        assert!(content.contains("ExecStart=/usr/bin/python3 script.py"));
    }
}
