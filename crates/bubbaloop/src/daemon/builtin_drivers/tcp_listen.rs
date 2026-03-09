//! TCP listen driver — accept raw TCP connections and publish incoming data to Zenoh.

use super::{spawn_health_loop, BuiltinDriver, DriverConfig, Result};
use tokio::io::AsyncReadExt;

pub struct TcpListenDriver;

#[async_trait::async_trait]
impl BuiltinDriver for TcpListenDriver {
    fn name(&self) -> &'static str {
        "tcp-listen"
    }

    async fn run(&self, config: DriverConfig) -> Result<()> {
        let port = config.u16_or("port", 5000);

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

        // Security: bind localhost only
        let addr = format!("127.0.0.1:{}", port);
        let listener = tokio::net::TcpListener::bind(&addr)
            .await
            .map_err(|e| super::DriverError::StartFailed(format!("bind {}: {}", addr, e)))?;

        log::info!(
            "[tcp-listen] skill='{}' listening on {}",
            config.skill_name,
            addr
        );

        loop {
            tokio::select! {
                biased;
                _ = shutdown_rx.changed() => {
                    log::info!("[tcp-listen] '{}' shutting down", config.skill_name);
                    break;
                }
                accept = listener.accept() => {
                    match accept {
                        Ok((mut stream, peer)) => {
                            let session = session.clone();
                            let topic = data_topic.clone();
                            tokio::spawn(async move {
                                let mut buf = vec![0u8; 65536];
                                loop {
                                    match stream.read(&mut buf).await {
                                        Ok(0) => break, // connection closed
                                        Ok(n) => {
                                            let data = String::from_utf8_lossy(&buf[..n]);
                                            let payload = serde_json::json!({
                                                "peer": peer.to_string(),
                                                "data": data.trim(),
                                            });
                                            if let Err(e) = session.put(&topic, payload.to_string()).await {
                                                log::warn!("[tcp-listen] publish failed: {}", e);
                                            }
                                        }
                                        Err(e) => {
                                            log::warn!("[tcp-listen] read error from {}: {}", peer, e);
                                            break;
                                        }
                                    }
                                }
                            });
                        }
                        Err(e) => log::warn!("[tcp-listen] accept failed: {}", e),
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
        assert_eq!(TcpListenDriver.name(), "tcp-listen");
    }
}
