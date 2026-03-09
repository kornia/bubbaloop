//! HTTP polling driver — GET a URL on interval, publish response bytes.

use super::{BuiltInContext, BuiltInDriver};
use std::time::Duration;

pub struct HttpPollDriver;

impl BuiltInDriver for HttpPollDriver {
    async fn run(self, mut ctx: BuiltInContext) -> anyhow::Result<()> {
        let url = ctx
            .config
            .get("url")
            .and_then(|v| v.as_str())
            .ok_or_else(|| anyhow::anyhow!("http-poll: missing required config key 'url'"))?
            .to_string();

        let interval_secs = ctx
            .config
            .get("interval_secs")
            .and_then(|v| v.as_u64())
            .unwrap_or(60);

        let interval = Duration::from_secs(interval_secs);
        let client = reqwest::Client::builder()
            .timeout(Duration::from_secs(30))
            .build()?;

        let data_topic = ctx.topic("data");
        let health_topic = ctx.topic("health");

        let data_pub = ctx
            .session
            .declare_publisher(&data_topic)
            .await
            .map_err(|e| anyhow::anyhow!("http-poll: failed to declare publisher: {}", e))?;
        let health_pub = ctx
            .session
            .declare_publisher(&health_topic)
            .await
            .map_err(|e| anyhow::anyhow!("http-poll: failed to declare health publisher: {}", e))?;

        let mut ticker = tokio::time::interval(interval);
        let mut health_ticker = tokio::time::interval(Duration::from_secs(5));

        log::info!(
            "[http-poll] {} starting, url={}, interval={}s",
            ctx.skill_name,
            url,
            interval_secs
        );

        loop {
            tokio::select! {
                _ = ticker.tick() => {
                    match client.get(&url).send().await {
                        Ok(resp) => {
                            match resp.bytes().await {
                                Ok(bytes) => {
                                    if let Err(e) = data_pub.put(bytes.to_vec()).await {
                                        log::warn!("[http-poll] {}: failed to publish: {}", ctx.skill_name, e);
                                    }
                                }
                                Err(e) => log::warn!("[http-poll] {}: failed to read response: {}", ctx.skill_name, e),
                            }
                        }
                        Err(e) => log::warn!("[http-poll] {}: request failed: {}", ctx.skill_name, e),
                    }
                }
                _ = health_ticker.tick() => {
                    let payload = b"ok".to_vec();
                    if let Err(e) = health_pub.put(payload).await {
                        log::warn!("[http-poll] {}: failed to publish health: {}", ctx.skill_name, e);
                    }
                }
                _ = ctx.shutdown_rx.changed() => {
                    log::info!("[http-poll] {} shutting down", ctx.skill_name);
                    break;
                }
            }
        }
        Ok(())
    }
}
