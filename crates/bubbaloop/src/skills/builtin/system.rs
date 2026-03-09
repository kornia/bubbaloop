//! System metrics driver — collects CPU/RAM/disk via sysinfo, publishes as JSON.

use super::{BuiltInContext, BuiltInDriver};
use std::time::Duration;

pub struct SystemDriver;

impl BuiltInDriver for SystemDriver {
    async fn run(self, mut ctx: BuiltInContext) -> anyhow::Result<()> {
        let interval_secs = ctx
            .config
            .get("interval_secs")
            .and_then(|v| v.as_u64())
            .unwrap_or(10);

        let interval = Duration::from_secs(interval_secs);
        let data_topic = ctx.topic("data");
        let health_topic = ctx.topic("health");

        let data_pub = ctx
            .session
            .declare_publisher(&data_topic)
            .await
            .map_err(|e| anyhow::anyhow!("system: failed to declare publisher: {}", e))?;
        let health_pub = ctx
            .session
            .declare_publisher(&health_topic)
            .await
            .map_err(|e| anyhow::anyhow!("system: failed to declare health publisher: {}", e))?;

        let mut ticker = tokio::time::interval(interval);
        let mut health_ticker = tokio::time::interval(Duration::from_secs(5));

        log::info!(
            "[system] {} starting, interval={}s",
            ctx.skill_name,
            interval_secs
        );

        loop {
            tokio::select! {
                _ = ticker.tick() => {
                    // Use sysinfo to collect metrics (run blocking op in spawn_blocking)
                    let metrics = tokio::task::spawn_blocking(collect_system_metrics).await?;
                    match serde_json::to_vec(&metrics) {
                        Ok(bytes) => {
                            if let Err(e) = data_pub.put(bytes).await {
                                log::warn!("[system] {}: failed to publish: {}", ctx.skill_name, e);
                            }
                        }
                        Err(e) => log::warn!("[system] {}: failed to serialize metrics: {}", ctx.skill_name, e),
                    }
                }
                _ = health_ticker.tick() => {
                    if let Err(e) = health_pub.put(b"ok".to_vec()).await {
                        log::warn!("[system] {}: failed to publish health: {}", ctx.skill_name, e);
                    }
                }
                _ = ctx.shutdown_rx.changed() => {
                    log::info!("[system] {} shutting down", ctx.skill_name);
                    break;
                }
            }
        }
        Ok(())
    }
}

#[derive(serde::Serialize)]
struct SystemMetrics {
    cpu_percent: f32,
    memory_used_mb: u64,
    memory_total_mb: u64,
    disk_used_gb: f64,
    disk_total_gb: f64,
}

fn collect_system_metrics() -> SystemMetrics {
    use sysinfo::System;
    let mut sys = System::new_all();
    sys.refresh_all();

    let cpu_percent = sys.global_cpu_usage();
    let memory_used_mb = sys.used_memory() / 1024 / 1024;
    let memory_total_mb = sys.total_memory() / 1024 / 1024;

    // Disk: sum all disks
    use sysinfo::Disks;
    let disks = Disks::new_with_refreshed_list();
    let (disk_used, disk_total) = disks.iter().fold((0u64, 0u64), |(u, t), d| {
        (
            u + d.total_space() - d.available_space(),
            t + d.total_space(),
        )
    });
    let disk_used_gb = disk_used as f64 / 1_073_741_824.0;
    let disk_total_gb = disk_total as f64 / 1_073_741_824.0;

    SystemMetrics {
        cpu_percent,
        memory_used_mb,
        memory_total_mb,
        disk_used_gb,
        disk_total_gb,
    }
}
