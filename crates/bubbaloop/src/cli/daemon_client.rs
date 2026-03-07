//! Zenoh-based CLI client for daemon communication.
//!
//! Publishes commands to the daemon's command topic and listens for events.
//! Mirrors the agent_client pattern: subscribe-before-publish, correlation ID
//! filtering, timeout handling, and auto-start.

use crate::daemon::gateway::{
    self, DaemonCommand, DaemonCommandType, DaemonEvent, DaemonEventType, DaemonManifest,
};
use std::sync::Arc;
use std::time::Duration;
use zenoh::Session;

/// Timeout waiting for daemon response.
const RESPONSE_TIMEOUT: Duration = Duration::from_secs(10);

/// Max time to wait for daemon to become ready after auto-start.
const DAEMON_STARTUP_TIMEOUT: Duration = Duration::from_secs(15);

/// Error type for daemon client operations.
#[derive(Debug, thiserror::Error)]
pub enum DaemonClientError {
    #[error("Daemon not reachable. Is it running?")]
    NotReachable,
    #[error("Request failed: {0}")]
    Request(String),
    #[error("Timeout waiting for daemon response")]
    Timeout,
    #[error("Daemon error: {0}")]
    DaemonError(String),
}

pub type Result<T> = std::result::Result<T, DaemonClientError>;

/// Zenoh-based client for daemon communication.
pub struct DaemonClient {
    session: Arc<Session>,
    scope: String,
    machine_id: String,
}

impl DaemonClient {
    /// Create a new Zenoh-based daemon client with an existing session.
    pub fn new(session: Arc<Session>) -> Self {
        let scope = std::env::var("BUBBALOOP_SCOPE").unwrap_or_else(|_| "local".to_string());
        let machine_id = crate::daemon::util::get_machine_id();
        Self {
            session,
            scope,
            machine_id,
        }
    }

    /// Create a daemon client by connecting to Zenoh automatically.
    /// Convenience for CLI commands that don't already have a session.
    pub async fn connect() -> Result<Self> {
        let session = crate::agent::create_agent_session(None)
            .await
            .map_err(|e| DaemonClientError::Request(format!("Zenoh connect failed: {}", e)))?;
        Ok(Self::new(session))
    }

    /// Check if the daemon is running by querying its manifest.
    /// Uses 1s timeout with 3 retries per Zenoh query convention.
    pub async fn is_running(&self) -> bool {
        let pattern = gateway::manifest_topic(&self.scope, &self.machine_id);
        for _ in 0..3 {
            if let Ok(replies) = self
                .session
                .get(&pattern)
                .target(zenoh::query::QueryTarget::BestMatching)
                .timeout(Duration::from_secs(1))
                .await
            {
                if replies.recv_async().await.is_ok() {
                    return true;
                }
            }
        }
        false
    }

    /// Auto-start the daemon if not running.
    pub async fn ensure_running(&self) -> Result<()> {
        if self.is_running().await {
            return Ok(());
        }

        eprintln!("Daemon not running — starting it automatically...");

        let exe = std::env::current_exe()
            .map_err(|e| DaemonClientError::Request(format!("Cannot find binary path: {}", e)))?;

        let log_path = crate::daemon::registry::get_bubbaloop_home().join("daemon.log");
        let log_file = std::fs::OpenOptions::new()
            .create(true)
            .append(true)
            .open(&log_path)
            .map_err(|e| {
                DaemonClientError::Request(format!("Cannot open {}: {}", log_path.display(), e))
            })?;
        let log_stderr = log_file
            .try_clone()
            .map_err(|e| DaemonClientError::Request(format!("Cannot clone log handle: {}", e)))?;

        let mut child = std::process::Command::new(&exe)
            .args(["daemon", "run"])
            .env("RUST_LOG", "info")
            .stdout(log_file)
            .stderr(log_stderr)
            .stdin(std::process::Stdio::null())
            .spawn()
            .map_err(|e| DaemonClientError::Request(format!("Failed to start daemon: {}", e)))?;

        let start = std::time::Instant::now();
        let poll_interval = Duration::from_millis(500);

        loop {
            if start.elapsed() > DAEMON_STARTUP_TIMEOUT {
                child.kill().ok();
                return Err(DaemonClientError::Request(format!(
                    "Daemon did not become ready within {}s. Check {}",
                    DAEMON_STARTUP_TIMEOUT.as_secs(),
                    log_path.display()
                )));
            }

            if let Ok(Some(status)) = child.try_wait() {
                return Err(DaemonClientError::Request(format!(
                    "Daemon exited unexpectedly (status: {}). Check {}",
                    status,
                    log_path.display()
                )));
            }

            if self.is_running().await {
                eprintln!(
                    "Daemon started (pid={}, log={})",
                    child.id(),
                    log_path.display()
                );
                return Ok(());
            }

            tokio::time::sleep(poll_interval).await;
        }
    }

    /// Send a command and wait for the response events.
    /// Returns the result text from the first Result event.
    pub async fn send(&self, command: DaemonCommandType) -> Result<String> {
        let correlation_id = uuid::Uuid::new_v4().to_string();

        // Subscribe to events BEFORE publishing (avoid missing early events)
        let evt_topic = gateway::events_topic(&self.scope, &self.machine_id);
        let subscriber = self
            .session
            .declare_subscriber(&evt_topic)
            .await
            .map_err(|e| DaemonClientError::Request(format!("Failed to subscribe: {}", e)))?;

        // Publish command
        let cmd_topic = gateway::command_topic(&self.scope, &self.machine_id);
        let cmd = DaemonCommand {
            id: correlation_id.clone(),
            command,
        };
        let payload = serde_json::to_vec(&cmd)
            .map_err(|e| DaemonClientError::Request(format!("Serialize error: {}", e)))?;
        self.session
            .put(&cmd_topic, payload)
            .await
            .map_err(|e| DaemonClientError::Request(format!("Failed to publish: {}", e)))?;

        // Collect response events, filtering by correlation ID
        let mut result_text = String::new();
        let mut got_first = false;

        loop {
            let timeout = if got_first {
                Duration::from_secs(30)
            } else {
                RESPONSE_TIMEOUT
            };

            tokio::select! {
                result = subscriber.recv_async() => {
                    match result {
                        Ok(sample) => {
                            let bytes = sample.payload().to_bytes();
                            if let Ok(event) = serde_json::from_slice::<DaemonEvent>(&bytes) {
                                if event.id != correlation_id {
                                    continue;
                                }
                                got_first = true;
                                match event.event_type {
                                    DaemonEventType::Result => {
                                        if let Some(text) = &event.text {
                                            result_text = text.clone();
                                        }
                                    }
                                    DaemonEventType::Error => {
                                        let msg = event.text.unwrap_or_else(|| "unknown error".to_string());
                                        return Err(DaemonClientError::DaemonError(msg));
                                    }
                                    DaemonEventType::Notification => {
                                        // Notifications are informational, continue waiting
                                    }
                                    DaemonEventType::Done => {
                                        return Ok(result_text);
                                    }
                                }
                            }
                        }
                        Err(e) => {
                            return Err(DaemonClientError::Request(format!("Subscription error: {}", e)));
                        }
                    }
                }
                _ = tokio::time::sleep(timeout) => {
                    if !got_first {
                        return Err(DaemonClientError::Timeout);
                    }
                    return Err(DaemonClientError::Request("Response timed out during streaming".into()));
                }
            }
        }
    }

    /// Query daemon health (manifest).
    /// Uses 1s timeout with 3 retries per Zenoh query convention.
    pub async fn health(&self) -> Result<DaemonManifest> {
        let pattern = gateway::manifest_topic(&self.scope, &self.machine_id);
        for _ in 0..3 {
            match self
                .session
                .get(&pattern)
                .target(zenoh::query::QueryTarget::BestMatching)
                .timeout(Duration::from_secs(1))
                .await
            {
                Ok(replies) => match replies.recv_async().await {
                    Ok(reply) => {
                        if let Ok(sample) = reply.into_result() {
                            let bytes = sample.payload().to_bytes();
                            return serde_json::from_slice::<DaemonManifest>(&bytes).map_err(|e| {
                                DaemonClientError::Request(format!("Invalid manifest: {}", e))
                            });
                        }
                    }
                    Err(_) => continue,
                },
                Err(_) => continue,
            }
        }
        Err(DaemonClientError::NotReachable)
    }

    /// Send a node command (start, stop, restart, etc.) and return the result message.
    pub async fn send_node_command(&self, name: &str, command: &str) -> Result<String> {
        let cmd_type = match command {
            "start" => DaemonCommandType::StartNode {
                name: name.to_string(),
            },
            "stop" => DaemonCommandType::StopNode {
                name: name.to_string(),
            },
            "restart" => DaemonCommandType::RestartNode {
                name: name.to_string(),
            },
            "logs" | "get_logs" | "get-logs" => DaemonCommandType::GetLogs {
                name: name.to_string(),
            },
            "install" => DaemonCommandType::InstallService {
                name: name.to_string(),
            },
            "build" => DaemonCommandType::BuildNode {
                name: name.to_string(),
            },
            "uninstall" => DaemonCommandType::UninstallNode {
                name: name.to_string(),
            },
            "clean" => DaemonCommandType::CleanNode {
                name: name.to_string(),
            },
            "enable_autostart" => DaemonCommandType::EnableAutostart {
                name: name.to_string(),
            },
            "disable_autostart" => DaemonCommandType::DisableAutostart {
                name: name.to_string(),
            },
            _ => {
                return Err(DaemonClientError::Request(format!(
                    "Unknown command: {}",
                    command
                )));
            }
        };
        self.send(cmd_type).await
    }

    /// Add a node from source path.
    pub async fn add_node(
        &self,
        source: &str,
        name: Option<&str>,
        config: Option<&str>,
    ) -> Result<String> {
        self.send(DaemonCommandType::InstallNode {
            source: source.to_string(),
            name: name.map(|s| s.to_string()),
            config: config.map(|s| s.to_string()),
        })
        .await
    }

    /// List nodes via the daemon gateway.
    pub async fn list_nodes(&self) -> Result<String> {
        self.send(DaemonCommandType::ListNodes).await
    }

    /// Remove a node by name.
    pub async fn remove_node(&self, name: &str) -> Result<String> {
        self.send(DaemonCommandType::RemoveNode {
            name: name.to_string(),
        })
        .await
    }
}

/// Run the daemon status command: query manifest and print a summary.
pub async fn run_daemon_status(
    session: Arc<Session>,
) -> std::result::Result<(), Box<dyn std::error::Error>> {
    let client = DaemonClient::new(session);

    match client.health().await {
        Ok(manifest) => {
            println!("Daemon Status");
            println!("=============");
            println!("  Version:    {}", manifest.version);
            println!("  Machine:    {}", manifest.machine_id);
            println!("  Uptime:     {}s", manifest.uptime_secs);
            println!("  Nodes:      {}", manifest.node_count);
            println!("  Agents:     {}", manifest.agent_count);
            println!("  MCP port:   {}", manifest.mcp_port);
        }
        Err(DaemonClientError::NotReachable) => {
            println!("Daemon is not running.");
            println!("Start it with: bubbaloop daemon start");
        }
        Err(e) => {
            eprintln!("Error: {}", e);
        }
    }

    Ok(())
}

/// Run the daemon stop command: send Shutdown via gateway.
pub async fn run_daemon_stop(
    session: Arc<Session>,
) -> std::result::Result<(), Box<dyn std::error::Error>> {
    let client = DaemonClient::new(session);

    if !client.is_running().await {
        println!("Daemon is not running.");
        return Ok(());
    }

    match client.send(DaemonCommandType::Shutdown).await {
        Ok(_) => println!("Daemon shutdown initiated."),
        Err(e) => eprintln!("Error: {}", e),
    }

    Ok(())
}

/// Run the daemon logs command: journalctl follow.
pub fn run_daemon_logs() -> std::result::Result<(), Box<dyn std::error::Error>> {
    let status = std::process::Command::new("journalctl")
        .args([
            "--user",
            "-u",
            "bubbaloop-daemon.service",
            "-f",
            "--no-pager",
        ])
        .status()?;

    if !status.success() {
        eprintln!("journalctl failed. Is the daemon installed as a systemd service?");
    }

    Ok(())
}

/// Run the daemon start command: install systemd service + start.
pub async fn run_daemon_start() -> std::result::Result<(), Box<dyn std::error::Error>> {
    // Generate systemd service file
    let home = dirs::home_dir().ok_or("HOME not set")?;
    let exe = std::env::current_exe()?;
    let service_dir = home.join(".config/systemd/user");
    std::fs::create_dir_all(&service_dir)?;

    let service_content = format!(
        "[Unit]\n\
         Description=Bubbaloop Daemon\n\
         After=network.target\n\
         \n\
         [Service]\n\
         ExecStart={} daemon run\n\
         Restart=on-failure\n\
         RestartSec=5\n\
         Environment=RUST_LOG=info\n\
         \n\
         [Install]\n\
         WantedBy=default.target\n",
        exe.display()
    );

    let service_path = service_dir.join("bubbaloop-daemon.service");
    std::fs::write(&service_path, service_content)?;
    println!("Installed service: {}", service_path.display());

    // Reload and start via zbus (D-Bus) — never spawn systemctl as subprocess
    let systemd = crate::daemon::systemd::SystemdClient::new().await.map_err(|e| {
        format!("Failed to connect to systemd user session via D-Bus: {}. Is the user session active?", e)
    })?;
    if let Err(e) = systemd.daemon_reload().await {
        eprintln!("Warning: systemd daemon-reload failed: {}", e);
    }
    match systemd.start_unit("bubbaloop-daemon.service").await {
        Ok(()) => println!("Daemon started as systemd service."),
        Err(e) => eprintln!(
            "Failed to start daemon service: {}. Check: systemctl --user status bubbaloop-daemon",
            e
        ),
    }

    Ok(())
}

/// Run the daemon fix command: auto-fix issues.
pub async fn run_daemon_fix(
    session: Arc<Session>,
) -> std::result::Result<(), Box<dyn std::error::Error>> {
    let client = DaemonClient::new(session);

    if !client.is_running().await {
        println!("Daemon is not running. Starting it...");
        run_daemon_start().await?;
        return Ok(());
    }

    match client.health().await {
        Ok(manifest) => {
            println!(
                "Daemon is healthy (v{}, uptime={}s, nodes={})",
                manifest.version, manifest.uptime_secs, manifest.node_count
            );
        }
        Err(e) => {
            println!("Daemon health check failed: {}", e);
            println!("Restarting daemon...");
            let _ = client.send(DaemonCommandType::Shutdown).await;
            tokio::time::sleep(Duration::from_secs(2)).await;
            run_daemon_start().await?;
        }
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn daemon_client_error_display() {
        let err = DaemonClientError::NotReachable;
        assert!(err.to_string().contains("not reachable"));
    }

    #[test]
    fn daemon_client_error_timeout() {
        let err = DaemonClientError::Timeout;
        assert!(err.to_string().contains("Timeout"));
    }

    #[test]
    fn daemon_client_error_daemon_error() {
        let err = DaemonClientError::DaemonError("node not found".to_string());
        assert!(err.to_string().contains("node not found"));
    }
}
