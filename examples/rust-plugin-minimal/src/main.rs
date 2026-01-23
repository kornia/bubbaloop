//! Minimal Bubbaloop Plugin Example
//!
//! Copy this file and modify:
//! 1. Config struct - add your config fields
//! 2. PluginData struct - define your message format
//! 3. run() method - add your logic where marked "EDIT HERE"

use bubbaloop::prelude::*;
use serde::Serialize;
use std::time::Duration;

// ============================================================================
// STEP 1: Define your configuration
// ============================================================================

#[derive(Debug, Deserialize)]
pub struct Config {
    #[serde(default = "default_topic")]
    pub topic: String,

    #[serde(default = "default_interval")]
    pub interval_secs: u64,

    // ADD YOUR CONFIG FIELDS HERE:
    // pub sensor_id: String,
    // pub threshold: f64,
}

fn default_topic() -> String { "my-plugin/data".to_string() }
fn default_interval() -> u64 { 10 }

// ============================================================================
// STEP 2: Define your data format
// ============================================================================

#[derive(Debug, Serialize)]
struct PluginData {
    value: f64,
    timestamp: u64,
    // ADD YOUR FIELDS HERE:
    // temperature: f64,
    // status: String,
}

// ============================================================================
// STEP 3: Implement your plugin
// ============================================================================

pub struct MyPluginNode {
    ctx: Arc<ZContext>,
    config: Config,
}

#[async_trait]
impl BubbleNode for MyPluginNode {
    type Config = Config;

    fn metadata() -> BubbleMetadata {
        BubbleMetadata {
            name: "my-plugin",
            version: "0.1.0",
            description: "My custom plugin",
            topics_published: &["my-plugin/data"],
            topics_subscribed: &[],
        }
    }

    fn new(ctx: Arc<ZContext>, config: Self::Config) -> Result<Self, NodeError> {
        info!("Starting my-plugin, topic: {}", config.topic);
        Ok(Self { ctx, config })
    }

    async fn run(self, mut shutdown: watch::Receiver<()>) -> Result<(), NodeError> {
        let session = self.ctx.session();
        let publisher = session
            .declare_publisher(&self.config.topic)
            .await
            .map_err(|e| NodeError::Zenoh(e.to_string()))?;

        let interval = Duration::from_secs(self.config.interval_secs);
        let mut counter = 0u64;

        loop {
            tokio::select! {
                _ = shutdown.changed() => break,
                _ = tokio::time::sleep(interval) => {
                    // ================================================
                    // EDIT HERE: Your plugin logic
                    // ================================================
                    let data = PluginData {
                        value: 42.0 + (counter as f64 * 0.1),
                        timestamp: std::time::SystemTime::now()
                            .duration_since(std::time::UNIX_EPOCH)
                            .unwrap()
                            .as_secs(),
                    };

                    let json = serde_json::to_string(&data)
                        .map_err(|e| NodeError::Runtime(e.to_string()))?;
                    publisher.put(json.as_bytes()).await
                        .map_err(|e| NodeError::Zenoh(e.to_string()))?;

                    info!("Published: {:?}", data);
                    counter += 1;
                }
            }
        }
        Ok(())
    }
}

// ============================================================================
// MAIN - No changes needed
// ============================================================================

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    run_node::<MyPluginNode>().await
}
