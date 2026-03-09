//! Webhook driver — listen for inbound HTTP POSTs and publish payloads to Zenoh.

use super::{spawn_health_loop, BuiltinDriver, DriverConfig, Result};
use axum::{extract::State, routing::post, Router};
use std::sync::Arc;

struct WebhookState {
    session: Arc<zenoh::Session>,
    data_topic: String,
}

pub struct WebhookDriver;

#[async_trait::async_trait]
impl BuiltinDriver for WebhookDriver {
    fn name(&self) -> &'static str {
        "webhook"
    }

    async fn run(&self, config: DriverConfig) -> Result<()> {
        let port = config.u16_or("port", 9090);
        let path = config.str_or("path", "/");

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

        let state = Arc::new(WebhookState {
            session,
            data_topic,
        });

        let app = Router::new()
            .route(&path, post(handle_webhook))
            .with_state(state);

        // Security: bind localhost only
        let addr = format!("127.0.0.1:{}", port);
        let listener = tokio::net::TcpListener::bind(&addr)
            .await
            .map_err(|e| super::DriverError::StartFailed(format!("bind {}: {}", addr, e)))?;

        log::info!(
            "[webhook] skill='{}' listening on {} path='{}'",
            config.skill_name,
            addr,
            path
        );

        axum::serve(listener, app)
            .with_graceful_shutdown(async move {
                let _ = shutdown_rx.changed().await;
                log::info!("[webhook] '{}' shutting down", config.skill_name);
            })
            .await
            .map_err(|e| super::DriverError::StartFailed(e.to_string()))?;

        Ok(())
    }
}

async fn handle_webhook(State(state): State<Arc<WebhookState>>, body: String) -> &'static str {
    if let Err(e) = state.session.put(&state.data_topic, body).await {
        log::warn!("[webhook] publish failed: {}", e);
        return "error";
    }
    "ok"
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn driver_name() {
        assert_eq!(WebhookDriver.name(), "webhook");
    }
}
