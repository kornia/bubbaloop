//! UDP listen driver — receive UDP packets and publish to Zenoh.

use super::{spawn_health_loop, BuiltinDriver, DriverConfig, Result};

pub struct UdpListenDriver;

#[async_trait::async_trait]
impl BuiltinDriver for UdpListenDriver {
    fn name(&self) -> &'static str {
        "udp-listen"
    }

    async fn run(&self, config: DriverConfig) -> Result<()> {
        let port = config.u16_or("port", 5001);

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
        let socket = tokio::net::UdpSocket::bind(&addr)
            .await
            .map_err(|e| super::DriverError::StartFailed(format!("bind {}: {}", addr, e)))?;

        log::info!(
            "[udp-listen] skill='{}' listening on {}",
            config.skill_name,
            addr
        );

        let mut buf = vec![0u8; 65536];
        loop {
            tokio::select! {
                biased;
                _ = shutdown_rx.changed() => {
                    log::info!("[udp-listen] '{}' shutting down", config.skill_name);
                    break;
                }
                result = socket.recv_from(&mut buf) => {
                    match result {
                        Ok((n, peer)) => {
                            let data = String::from_utf8_lossy(&buf[..n]);
                            let payload = serde_json::json!({
                                "peer": peer.to_string(),
                                "data": data.trim(),
                            });
                            if let Err(e) = session.put(&data_topic, payload.to_string()).await {
                                log::warn!("[udp-listen] publish failed: {}", e);
                            }
                        }
                        Err(e) => log::warn!("[udp-listen] recv failed: {}", e),
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
        assert_eq!(UdpListenDriver.name(), "udp-listen");
    }
}
