use futures::future;
use ros_z::{node::ZNode, Result as ZResult};
use std::sync::Arc;

/// A single Foxglove bridge node that subscribes to multiple topics
pub struct FoxgloveNode {
    node: Arc<ZNode>,
    topics: Vec<String>,
}

impl FoxgloveNode {
    /// Create a new Foxglove bridge node that will subscribe to a list of topics
    pub fn new(node: Arc<ZNode>, topics: &[String]) -> ZResult<Self> {
        log::info!(
            "Foxglove bridge initialized with {} topics to subscribe",
            topics.len()
        );

        Ok(Self {
            node,
            topics: topics.to_vec(),
        })
    }

    pub async fn run(self, shutdown_tx: tokio::sync::watch::Sender<()>) -> ZResult<()> {
        let mut shutdown_rx = shutdown_tx.subscribe();

        log::info!("Foxglove bridge started");

        let tasks: Vec<_> = self
            .topics
            .into_iter()
            .map(|topic| {
                let node_clone = self.node.clone();
                let shutdown_rx_task = shutdown_tx.subscribe();
                spawn_message_handler!(&topic, node_clone, shutdown_rx_task)
            })
            .collect();

        let _ = shutdown_rx.changed().await;
        log::info!("Shutting down Foxglove bridge...");

        future::join_all(tasks).await;

        Ok(())
    }
}
