//! HTTP poll driver — GET/POST a URL on a fixed interval, publish response to Zenoh.

use super::{spawn_health_loop, BuiltinDriver, DriverConfig, Result};
use std::time::Duration;

pub struct HttpPollDriver;

#[async_trait::async_trait]
impl BuiltinDriver for HttpPollDriver {
    fn name(&self) -> &'static str {
        "http-poll"
    }

    async fn run(&self, config: DriverConfig) -> Result<()> {
        let url = config.require_str("url")?;
        let method = config.str_or("method", "GET").to_uppercase();
        let interval_secs = config.u64_or("interval_secs", 60);
        let body = config.str_or("body", "");

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

        let client = reqwest::Client::builder()
            .timeout(Duration::from_secs(30))
            .build()
            .map_err(|e| super::DriverError::StartFailed(e.to_string()))?;

        let mut interval = tokio::time::interval(Duration::from_secs(interval_secs));
        log::info!(
            "[http-poll] skill='{}' url='{}' method={} interval={}s",
            config.skill_name,
            url,
            method,
            interval_secs
        );

        loop {
            tokio::select! {
                biased;
                _ = shutdown_rx.changed() => {
                    log::info!("[http-poll] '{}' shutting down", config.skill_name);
                    break;
                }
                _ = interval.tick() => {
                    let request = match method.as_str() {
                        "POST" => client.post(&url).body(body.clone()),
                        _ => client.get(&url),
                    };

                    match request.send().await {
                        Ok(resp) => {
                            match resp.text().await {
                                Ok(text) => {
                                    if let Err(e) = session.put(&data_topic, text).await {
                                        log::warn!("[http-poll] publish failed: {}", e);
                                    }
                                }
                                Err(e) => log::warn!("[http-poll] body read failed: {}", e),
                            }
                        }
                        Err(e) => log::warn!("[http-poll] request to '{}' failed: {}", url, e),
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
        assert_eq!(HttpPollDriver.name(), "http-poll");
    }
}
