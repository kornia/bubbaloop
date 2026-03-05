//! Auto-fix actions for doctor diagnostics
//!
//! Contains the `FixAction` enum and execution logic for automatically
//! resolving common configuration and service issues.

use anyhow::{anyhow, Result};
use std::time::Duration;

use crate::cli::system_utils::is_process_running;
use crate::daemon::systemd::SystemdClient;

/// Actions that can be automatically fixed
#[derive(Debug, Clone)]
pub enum FixAction {
    StartZenohd,
    StartDaemonService,
    RestartDaemonService,
    InstallAndStartDaemon,
    StartBridgeService,
    CreateZenohConfig,
    CreateMarketplaceSources,
}

impl FixAction {
    pub fn description(&self) -> &'static str {
        match self {
            FixAction::StartZenohd => "Start zenohd router",
            FixAction::StartDaemonService => "Start bubbaloop-daemon service",
            FixAction::RestartDaemonService => "Restart bubbaloop-daemon service",
            FixAction::InstallAndStartDaemon => "Install and start bubbaloop-daemon service",
            FixAction::StartBridgeService => "Start zenoh-bridge service",
            FixAction::CreateZenohConfig => "Create Zenoh config file",
            FixAction::CreateMarketplaceSources => {
                "Create marketplace sources with official registry"
            }
        }
    }

    pub async fn execute(&self) -> Result<String> {
        match self {
            FixAction::StartZenohd => {
                // Start zenohd in background via std::process (not tokio) so it
                // outlives this process as an orphan adopted by init.
                let child = std::process::Command::new("zenohd")
                    .stdout(std::process::Stdio::null())
                    .stderr(std::process::Stdio::null())
                    .stdin(std::process::Stdio::null())
                    .spawn()?;

                let pid = child.id();
                // Drop the Child handle without waiting — the OS reparents to init
                drop(child);

                // Wait a moment for it to start
                tokio::time::sleep(Duration::from_millis(500)).await;

                // Verify it started
                if is_process_running("zenohd").await {
                    Ok(format!("zenohd started (pid={})", pid))
                } else {
                    Err(anyhow!("Failed to start zenohd"))
                }
            }
            FixAction::StartDaemonService => {
                let systemd = SystemdClient::new()
                    .await
                    .map_err(|e| anyhow!("D-Bus connect failed: {}", e))?;
                systemd
                    .start_unit("bubbaloop-daemon.service")
                    .await
                    .map_err(|e| anyhow!("Failed to start: {}", e))?;
                Ok("bubbaloop-daemon service started".to_string())
            }
            FixAction::RestartDaemonService => {
                let systemd = SystemdClient::new()
                    .await
                    .map_err(|e| anyhow!("D-Bus connect failed: {}", e))?;
                systemd
                    .restart_unit("bubbaloop-daemon.service")
                    .await
                    .map_err(|e| anyhow!("Failed to restart: {}", e))?;
                Ok("bubbaloop-daemon service restarted".to_string())
            }
            FixAction::InstallAndStartDaemon => {
                // Install systemd service file, then start it
                // This mirrors what `bubbaloop daemon start` does
                let exe = std::env::current_exe()?;
                let home = dirs::home_dir().ok_or_else(|| anyhow!("HOME not set"))?;
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

                // Reload systemd and start via D-Bus
                let systemd = SystemdClient::new()
                    .await
                    .map_err(|e| anyhow!("D-Bus connect failed: {}", e))?;
                if let Err(e) = systemd.daemon_reload().await {
                    log::warn!("systemd daemon-reload failed: {}", e);
                }
                systemd
                    .start_unit("bubbaloop-daemon.service")
                    .await
                    .map_err(|e| anyhow!("Service installed but failed to start: {}", e))?;
                Ok(format!(
                    "Installed {} and started service",
                    service_path.display()
                ))
            }
            FixAction::StartBridgeService => {
                let systemd = SystemdClient::new()
                    .await
                    .map_err(|e| anyhow!("D-Bus connect failed: {}", e))?;
                systemd
                    .start_unit("bubbaloop-bridge.service")
                    .await
                    .map_err(|e| anyhow!("Failed to start: {}", e))?;
                Ok("zenoh-bridge service started".to_string())
            }
            FixAction::CreateZenohConfig => {
                let home = dirs::home_dir().ok_or_else(|| anyhow!("HOME not set"))?;
                let zenoh_dir = home.join(".bubbaloop/zenoh");
                std::fs::create_dir_all(&zenoh_dir)?;

                let config_path = zenoh_dir.join("zenohd.json5");
                let config_content = r#"{
  mode: "router",
  listen: {
    endpoints: ["tcp/127.0.0.1:7447"]
  },
  scouting: {
    multicast: {
      enabled: false
    },
    gossip: {
      enabled: false
    }
  }
}"#;
                std::fs::write(&config_path, config_content)?;
                Ok(format!("Created {}", config_path.display()))
            }
            FixAction::CreateMarketplaceSources => {
                let home = dirs::home_dir().ok_or_else(|| anyhow!("HOME not set"))?;
                let sources_path = home.join(".bubbaloop/sources.json");

                let sources_content = r#"{
  "sources": [
    {
      "name": "Official Nodes",
      "path": "kornia/bubbaloop-nodes-official",
      "type": "builtin",
      "enabled": true
    }
  ]
}"#;
                std::fs::write(&sources_path, sources_content)?;
                Ok(format!(
                    "Created {} with official nodes registry",
                    sources_path.display()
                ))
            }
        }
    }
}
