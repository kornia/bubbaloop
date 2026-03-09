//! System driver — publish CPU, RAM, disk metrics via sysinfo.

use super::{spawn_health_loop, BuiltinDriver, DriverConfig, Result};
use std::time::Duration;
use sysinfo::System;

pub struct SystemDriver;

#[async_trait::async_trait]
impl BuiltinDriver for SystemDriver {
    fn name(&self) -> &'static str {
        "system"
    }

    async fn run(&self, config: DriverConfig) -> Result<()> {
        let interval_secs = config.u64_or("interval_secs", 30);

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
        let mut sys = System::new_all();

        log::info!(
            "[system] skill='{}' interval={}s",
            config.skill_name,
            interval_secs
        );

        loop {
            tokio::select! {
                biased;
                _ = shutdown_rx.changed() => {
                    log::info!("[system] '{}' shutting down", config.skill_name);
                    break;
                }
                _ = interval.tick() => {
                    sys.refresh_all();

                    let payload = serde_json::json!({
                        "cpu_usage_percent": sys.global_cpu_usage(),
                        "memory_total_bytes": sys.total_memory(),
                        "memory_used_bytes": sys.used_memory(),
                        "memory_available_bytes": sys.available_memory(),
                        "swap_total_bytes": sys.total_swap(),
                        "swap_used_bytes": sys.used_swap(),
                    });

                    if let Err(e) = session.put(&data_topic, payload.to_string()).await {
                        log::warn!("[system] publish failed: {}", e);
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
        assert_eq!(SystemDriver.name(), "system");
    }

    #[test]
    fn sysinfo_produces_valid_data() {
        let mut sys = System::new_all();
        sys.refresh_all();
        // Just verify these don't panic and return reasonable values
        let _ = sys.global_cpu_usage();
        assert!(sys.total_memory() > 0);
    }
}
