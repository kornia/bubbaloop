//! TCP listener driver — publishes data received from TCP connections.

use super::{BuiltInContext, BuiltInDriver};
use std::net::SocketAddr;
use tokio::io::AsyncReadExt;

pub struct TcpListenDriver;

impl BuiltInDriver for TcpListenDriver {
    async fn run(self, mut ctx: BuiltInContext) -> anyhow::Result<()> {
        let port: u16 = ctx
            .config
            .get("port")
            .and_then(|v| v.as_u64())
            .ok_or_else(|| anyhow::anyhow!("tcp-listen: missing required config key 'port'"))?
            .try_into()
            .map_err(|_| anyhow::anyhow!("tcp-listen: port must be valid u16"))?;

        // Security: bind localhost only — never 0.0.0.0
        let addr = SocketAddr::from(([127, 0, 0, 1], port));
        let listener = tokio::net::TcpListener::bind(addr)
            .await
            .map_err(|e| anyhow::anyhow!("tcp-listen: bind 127.0.0.1:{} failed: {}", port, e))?;

        let data_topic = ctx.topic("data");
        let health_topic = ctx.topic("health");
        let health_pub = ctx
            .session
            .declare_publisher(&health_topic)
            .await
            .map_err(|e| anyhow::anyhow!("tcp-listen: health publisher: {}", e))?;

        let skill_name = ctx.skill_name.clone();
        log::info!(
            "[tcp-listen] {} listening on 127.0.0.1:{}",
            skill_name,
            port
        );

        let mut health_ticker = tokio::time::interval(std::time::Duration::from_secs(5));

        loop {
            tokio::select! {
                result = listener.accept() => {
                    match result {
                        Ok((mut stream, peer)) => {
                            let session = ctx.session.clone();
                            let topic = data_topic.clone();
                            let name = skill_name.clone();
                            tokio::spawn(async move {
                                let mut buf = Vec::new();
                                if let Err(e) = stream.read_to_end(&mut buf).await {
                                    log::warn!(
                                        "[tcp-listen] {}: read error from {}: {}",
                                        name, peer, e
                                    );
                                    return;
                                }
                                if !buf.is_empty() {
                                    if let Err(e) = session.put(&topic, buf).await {
                                        log::warn!(
                                            "[tcp-listen] {}: publish error: {}",
                                            name, e
                                        );
                                    }
                                }
                            });
                        }
                        Err(e) => {
                            log::warn!("[tcp-listen] {}: accept error: {}", skill_name, e);
                        }
                    }
                }
                _ = health_ticker.tick() => {
                    if let Err(e) = health_pub.put(b"ok".to_vec()).await {
                        log::warn!("[tcp-listen] {}: health publish error: {}", skill_name, e);
                    }
                }
                _ = ctx.shutdown_rx.changed() => {
                    log::info!("[tcp-listen] {} shutting down", skill_name);
                    break;
                }
            }
        }
        Ok(())
    }
}
