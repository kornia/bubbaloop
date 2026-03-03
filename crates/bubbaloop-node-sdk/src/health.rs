use std::sync::Arc;
use tokio::sync::watch;

/// Spawn a background task that publishes health heartbeats every 5 seconds.
///
/// Publishes `"ok"` to `bubbaloop/{scope}/{machine_id}/health/{node_name}`.
/// Stops when the shutdown signal fires.
pub async fn spawn_health_heartbeat(
    session: Arc<zenoh::Session>,
    scope: &str,
    machine_id: &str,
    node_name: &str,
    mut shutdown_rx: watch::Receiver<()>,
) -> anyhow::Result<tokio::task::JoinHandle<()>> {
    let health_topic = format!("bubbaloop/{}/{}/health/{}", scope, machine_id, node_name);
    log::info!("Health heartbeat: {}", health_topic);
    let publisher = session
        .declare_publisher(health_topic)
        .await
        .map_err(|e| anyhow::anyhow!("Failed to create health publisher: {}", e))?;

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
