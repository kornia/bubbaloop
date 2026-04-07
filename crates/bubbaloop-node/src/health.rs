use std::sync::Arc;
use tokio::sync::watch;

use crate::error::{NodeError, Result};

/// Spawn a background task that publishes health heartbeats every 5 seconds.
///
/// Publishes `"ok"` to `bubbaloop/global/{machine_id}/{node_name}/health`.
/// Stops when the shutdown signal fires.
pub async fn spawn_health_heartbeat(
    session: Arc<zenoh::Session>,
    machine_id: &str,
    node_name: &str,
    mut shutdown_rx: watch::Receiver<()>,
) -> Result<tokio::task::JoinHandle<()>> {
    let health_topic = format!("bubbaloop/global/{}/{}/health", machine_id, node_name);
    log::info!("Health heartbeat: {}", health_topic);
    let publisher = session
        .declare_publisher(health_topic)
        .await
        .map_err(NodeError::HealthPublisher)?;

    let handle = tokio::spawn(async move {
        let mut interval = tokio::time::interval(std::time::Duration::from_secs(5));
        loop {
            tokio::select! {
                biased;
                _ = shutdown_rx.changed() => {
                    log::debug!("Health heartbeat stopping");
                    break;
                }
                _ = interval.tick() => {
                    if let Err(e) = publisher.put("ok").await {
                        log::warn!("Health heartbeat failed: {}", e);
                    }
                }
            }
        }
    });

    Ok(handle)
}
