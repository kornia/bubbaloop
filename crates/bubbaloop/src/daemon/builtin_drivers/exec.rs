//! Exec driver — run a shell command on a fixed interval, publish stdout to Zenoh.

use super::{spawn_health_loop, BuiltinDriver, DriverConfig, DriverError, Result};
use std::time::Duration;

pub struct ExecDriver;

/// Validate that a command does not contain shell metacharacters.
fn validate_command(cmd: &str) -> Result<()> {
    let forbidden = ['|', ';', '&', '$', '`', '(', ')', '{', '}', '<', '>'];
    for c in forbidden {
        if cmd.contains(c) {
            return Err(DriverError::ConfigError(format!(
                "Command contains forbidden shell metacharacter '{}'",
                c
            )));
        }
    }
    Ok(())
}

#[async_trait::async_trait]
impl BuiltinDriver for ExecDriver {
    fn name(&self) -> &'static str {
        "exec"
    }

    async fn run(&self, config: DriverConfig) -> Result<()> {
        let command = config.require_str("command")?;
        validate_command(&command)?;
        let interval_secs = config.u64_or("interval_secs", 60);

        let data_topic = config.data_topic();
        let health_topic = config.health_topic();
        let session = config.session.clone();
        let mut shutdown_rx = config.shutdown_rx.clone();

        // Spawn health heartbeat
        let health_session = session.clone();
        let health_shutdown = config.shutdown_rx.clone();
        tokio::spawn(spawn_health_loop(
            health_session,
            health_topic,
            health_shutdown,
        ));

        let mut interval = tokio::time::interval(Duration::from_secs(interval_secs));
        log::info!(
            "[exec] skill='{}' command='{}' interval={}s",
            config.skill_name,
            command,
            interval_secs
        );

        let parts: Vec<&str> = command.split_whitespace().collect();
        if parts.is_empty() {
            return Err(DriverError::ConfigError("Empty command".to_string()));
        }

        loop {
            tokio::select! {
                biased;
                _ = shutdown_rx.changed() => {
                    log::info!("[exec] '{}' shutting down", config.skill_name);
                    break;
                }
                _ = interval.tick() => {
                    match tokio::process::Command::new(parts[0])
                        .args(&parts[1..])
                        .output()
                        .await
                    {
                        Ok(output) => {
                            let stdout = String::from_utf8_lossy(&output.stdout);
                            let payload = serde_json::json!({
                                "stdout": stdout.trim(),
                                "exit_code": output.status.code(),
                            });
                            if let Err(e) = session.put(&data_topic, payload.to_string()).await {
                                log::warn!("[exec] publish failed: {}", e);
                            }
                        }
                        Err(e) => log::warn!("[exec] command failed: {}", e),
                    }
                }
            }
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn driver_name() {
        assert_eq!(ExecDriver.name(), "exec");
    }

    #[test]
    fn rejects_shell_metacharacters() {
        assert!(validate_command("ls -la").is_ok());
        assert!(validate_command("echo hello | grep h").is_err());
        assert!(validate_command("rm -rf /; echo done").is_err());
        assert!(validate_command("cmd && cmd2").is_err());
        assert!(validate_command("$(whoami)").is_err());
        assert!(validate_command("`whoami`").is_err());
    }
}
