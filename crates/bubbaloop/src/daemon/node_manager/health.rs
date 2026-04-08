//! Health monitoring for nodes via Zenoh heartbeats.
//!
//! Subscribes to heartbeat topics and marks nodes as unhealthy
//! if no heartbeat is received within the timeout window.

use super::{NodeManager, NodeManagerError, Result};
use crate::schemas::daemon::v1::{HealthStatus, NodeStatus};
use std::sync::Arc;
use std::time::Duration;

/// Health check timeout in milliseconds (30 seconds)
const HEALTH_TIMEOUT_MS: i64 = 30_000;

impl NodeManager {
    /// Start health monitoring via Zenoh heartbeats
    ///
    /// Subscribes to both legacy `bubbaloop/nodes/*/health` and scoped
    /// `bubbaloop/*/*/*/health` topics, and marks nodes as unhealthy
    /// if no heartbeat is received within HEALTH_TIMEOUT_MS.
    pub async fn start_health_monitor(
        self: Arc<Self>,
        session: std::sync::Arc<zenoh::Session>,
        shutdown_rx: tokio::sync::watch::Receiver<()>,
    ) -> Result<()> {
        let manager = self.clone();

        // Subscribe to legacy health heartbeats: bubbaloop/nodes/{name}/health
        let legacy_subscriber = session
            .declare_subscriber("bubbaloop/nodes/*/health")
            .await
            .map_err(|e| NodeManagerError::BuildError(format!("Zenoh subscribe error: {}", e)))?;

        // Subscribe to scoped health heartbeats: bubbaloop/global/{machine}/{name}/health
        let scoped_subscriber = session
            .declare_subscriber("bubbaloop/*/*/*/health")
            .await
            .map_err(|e| NodeManagerError::BuildError(format!("Zenoh subscribe error: {}", e)))?;

        log::info!("Started health monitor, subscribing to bubbaloop/nodes/*/health and bubbaloop/*/*/*/health");

        // Spawn heartbeat receiver task (merges both subscriber streams)
        let manager_heartbeat = manager.clone();
        let mut heartbeat_shutdown = shutdown_rx.clone();
        tokio::spawn(async move {
            let mut error_count: u32 = 0;
            loop {
                let sample = tokio::select! {
                    _ = heartbeat_shutdown.changed() => {
                        log::debug!("Health heartbeat task shutting down");
                        break;
                    }
                    result = legacy_subscriber.recv_async() => {
                        match result {
                            Ok(s) => s,
                            Err(e) => {
                                error_count += 1;
                                if error_count % 10 == 1 {
                                    log::warn!("Legacy health subscriber error (count={}): {}", error_count, e);
                                }
                                tokio::time::sleep(Duration::from_secs(1)).await;
                                continue;
                            }
                        }
                    }
                    result = scoped_subscriber.recv_async() => {
                        match result {
                            Ok(s) => s,
                            Err(e) => {
                                error_count += 1;
                                if error_count % 10 == 1 {
                                    log::warn!("Scoped health subscriber error (count={}): {}", error_count, e);
                                }
                                tokio::time::sleep(Duration::from_secs(1)).await;
                                continue;
                            }
                        }
                    }
                };

                // Reset error count on successful receive
                error_count = 0;

                // Extract node name from key (handles both formats)
                let key_str = sample.key_expr().as_str();
                if let Some(name) = extract_health_node_name(key_str) {
                    let now = Self::now_ms();
                    log::debug!("Received health heartbeat from node: {}", name);

                    // Update health state (match by effective name)
                    let mut nodes = manager_heartbeat.nodes.write().await;
                    for node in nodes.values_mut() {
                        if node.effective_name() == name {
                            node.health_status = HealthStatus::Healthy;
                            node.last_health_check_ms = now;
                            break;
                        }
                    }
                }
            }
        });

        // Spawn staleness checker task (runs every 10 seconds)
        let mut staleness_shutdown = shutdown_rx;
        tokio::spawn(async move {
            let mut interval = tokio::time::interval(Duration::from_secs(10));
            loop {
                tokio::select! {
                    _ = staleness_shutdown.changed() => {
                        log::debug!("Health staleness checker task shutting down");
                        break;
                    }
                    _ = interval.tick() => {}
                }

                let now = Self::now_ms();
                let mut nodes = manager.nodes.write().await;

                for node in nodes.values_mut() {
                    // Only check running nodes
                    if node.status == NodeStatus::Running {
                        // If we've received at least one heartbeat, check staleness
                        if node.last_health_check_ms > 0 {
                            let age = now - node.last_health_check_ms;
                            if age > HEALTH_TIMEOUT_MS
                                && node.health_status != HealthStatus::Unhealthy
                            {
                                let name = node.effective_name();
                                log::warn!(
                                    "Node {} marked unhealthy (no heartbeat for {}ms)",
                                    name,
                                    age
                                );
                                node.health_status = HealthStatus::Unhealthy;
                            }
                        }
                    } else {
                        // Reset health for non-running nodes
                        node.health_status = HealthStatus::Unknown;
                        node.last_health_check_ms = 0;
                    }
                }
            }
        });

        Ok(())
    }
}

/// Extract node name from health topic key.
///
/// Handles two formats:
/// - Legacy:  `bubbaloop/nodes/{name}/health`             -> name at index 2
/// - Scoped:  `bubbaloop/global/{machine}/{name}/health` -> name at index 3
fn extract_health_node_name(key: &str) -> Option<String> {
    let parts: Vec<&str> = key.split('/').collect();
    if parts.is_empty() || parts[0] != "bubbaloop" {
        return None;
    }

    // Legacy format: bubbaloop/nodes/{name}/health
    if parts.len() == 4 && parts[1] == "nodes" && parts[3] == "health" {
        return Some(parts[2].to_string());
    }

    // Scoped format: bubbaloop/global/{machine}/{name}/health
    if parts.len() == 5 && parts[4] == "health" {
        return Some(parts[3].to_string());
    }

    None
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_extract_health_node_name_legacy() {
        assert_eq!(
            extract_health_node_name("bubbaloop/nodes/my-node/health"),
            Some("my-node".to_string())
        );
    }

    #[test]
    fn test_extract_health_node_name_scoped() {
        assert_eq!(
            extract_health_node_name("bubbaloop/scope/machine1/my-node/health"),
            Some("my-node".to_string())
        );
    }

    #[test]
    fn test_extract_health_node_name_invalid() {
        assert_eq!(extract_health_node_name("invalid/path"), None);
        assert_eq!(extract_health_node_name(""), None);
        assert_eq!(extract_health_node_name("other/nodes/x/health"), None);
        // old format no longer recognized
        assert_eq!(
            extract_health_node_name("bubbaloop/scope/machine1/health/my-node"),
            None
        );
    }
}
