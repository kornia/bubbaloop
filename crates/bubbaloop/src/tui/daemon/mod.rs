mod client;

use std::sync::Arc;

use anyhow::Result;
use tokio::sync::{watch, RwLock};

pub use client::DaemonClient;

use crate::tui::app::NodeInfo;

/// Background subscription manager for daemon node list updates
/// Provides non-blocking access to the latest node state
pub struct DaemonSubscription {
    /// Latest node list from subscription
    nodes: Arc<RwLock<Vec<NodeInfo>>>,
    /// Connection status - true when receiving updates
    connected: Arc<RwLock<bool>>,
    /// Shutdown signal
    #[allow(dead_code)]
    shutdown_tx: watch::Sender<()>,
}

impl DaemonSubscription {
    /// Start a background subscription to node list updates
    /// Uses the hybrid approach: initial query + subscription
    pub async fn start(client: Arc<DaemonClient>) -> Result<Self> {
        let nodes = Arc::new(RwLock::new(Vec::new()));
        let connected = Arc::new(RwLock::new(false));
        let (shutdown_tx, shutdown_rx) = watch::channel(());

        // Start subscription task
        let nodes_clone = nodes.clone();
        let connected_clone = connected.clone();
        let mut node_rx = client.subscribe_nodes().await?;

        tokio::spawn(async move {
            let mut shutdown = shutdown_rx;
            loop {
                tokio::select! {
                    _ = shutdown.changed() => {
                        break;
                    }
                    Some(new_nodes) = node_rx.recv() => {
                        *connected_clone.write().await = true;
                        *nodes_clone.write().await = new_nodes;
                    }
                }
            }
        });

        Ok(Self {
            nodes,
            connected,
            shutdown_tx,
        })
    }

    /// Get current nodes (non-blocking read)
    pub async fn get_nodes(&self) -> Vec<NodeInfo> {
        self.nodes.read().await.clone()
    }

    /// Check if connected to daemon (receiving subscription updates)
    pub async fn is_connected(&self) -> bool {
        *self.connected.read().await
    }
}

impl Drop for DaemonSubscription {
    fn drop(&mut self) {
        let _ = self.shutdown_tx.send(());
    }
}
