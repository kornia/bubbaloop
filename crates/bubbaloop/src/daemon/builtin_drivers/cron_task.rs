//! Cron task driver — run a command on a cron schedule, publish output to Zenoh.

use super::{spawn_health_loop, BuiltinDriver, DriverConfig, DriverError, Result};
use cron::Schedule;
use std::str::FromStr;
use std::time::Duration;

pub struct CronTaskDriver;

#[async_trait::async_trait]
impl BuiltinDriver for CronTaskDriver {
    fn name(&self) -> &'static str {
        "cron-task"
    }

    async fn run(&self, config: DriverConfig) -> Result<()> {
        let command = config.require_str("command")?;
        let schedule_str = config.require_str("schedule")?;

        // Validate command (same rules as exec driver)
        let forbidden = ['|', ';', '&', '$', '`', '(', ')', '{', '}', '<', '>'];
        for c in &forbidden {
            if command.contains(*c) {
                return Err(DriverError::ConfigError(format!(
                    "Command contains forbidden shell metacharacter '{}'",
                    c
                )));
            }
        }

        // cron crate expects 7-field or 6-field expressions; prepend "sec" if 5-field
        let cron_expr = if schedule_str.split_whitespace().count() == 5 {
            format!("0 {}", schedule_str)
        } else {
            schedule_str.clone()
        };

        let schedule = Schedule::from_str(&cron_expr)
            .map_err(|e| DriverError::ConfigError(format!("Invalid cron: {}", e)))?;

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

        log::info!(
            "[cron-task] skill='{}' command='{}' schedule='{}'",
            config.skill_name,
            command,
            schedule_str
        );

        let parts: Vec<&str> = command.split_whitespace().collect();
        if parts.is_empty() {
            return Err(DriverError::ConfigError("Empty command".to_string()));
        }

        loop {
            // Find next scheduled time
            let next = match schedule.upcoming(chrono::Utc).next() {
                Some(t) => t,
                None => {
                    log::warn!("[cron-task] No upcoming schedule times");
                    break;
                }
            };

            let now = chrono::Utc::now();
            let delay = (next - now).to_std().unwrap_or(Duration::from_secs(1));

            tokio::select! {
                biased;
                _ = shutdown_rx.changed() => {
                    log::info!("[cron-task] '{}' shutting down", config.skill_name);
                    break;
                }
                _ = tokio::time::sleep(delay) => {
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
                                "schedule": schedule_str,
                            });
                            if let Err(e) = session.put(&data_topic, payload.to_string()).await {
                                log::warn!("[cron-task] publish failed: {}", e);
                            }
                        }
                        Err(e) => log::warn!("[cron-task] command failed: {}", e),
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
        assert_eq!(CronTaskDriver.name(), "cron-task");
    }
}
