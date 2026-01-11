use ros_z::{context::ZContext, Result as ZResult};
use std::sync::Arc;

/// A single Foxglove bridge node that subscribes to multiple topics
pub struct FoxgloveNode {
    ctx: Arc<ZContext>,
    topics: Vec<String>,
}

impl FoxgloveNode {
    /// Create a new Foxglove bridge node that will subscribe to a list of topics
    pub fn new(ctx: Arc<ZContext>, topics: &[String]) -> ZResult<Self> {
        log::info!(
            "Foxglove bridge initialized with {} topics to subscribe",
            topics.len()
        );

        Ok(Self {
            ctx,
            topics: topics.to_vec(),
        })
    }

    pub async fn run(self, shutdown_tx: tokio::sync::watch::Sender<()>) -> ZResult<()> {
        let mut shutdown_rx = shutdown_tx.subscribe();

        log::info!("Foxglove bridge started");

        let mut tasks = Vec::new();
        for topic in self.topics {
            let ctx = self.ctx.clone();
            let topic_clone = topic.clone();
            let shutdown_rx_task = shutdown_tx.subscribe();

            let task = spawn_message_handler!(&topic_clone, ctx, shutdown_rx_task);
            tasks.push(task);
        }

        let _ = shutdown_rx.changed().await;
        log::info!("Shutting down Foxglove bridge...");

        for task in tasks {
            task.abort();
        }

        Ok(())
    }
}
