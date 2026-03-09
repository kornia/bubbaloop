//! Webhook driver — listens for HTTP POST on localhost, publishes body bytes.

use super::{BuiltInContext, BuiltInDriver};
use std::net::SocketAddr;
use std::sync::Arc;

pub struct WebhookDriver;

/// Shared state for the axum handler.
#[derive(Clone)]
struct WebhookState {
    session: Arc<zenoh::Session>,
    data_topic: String,
    skill_name: String,
}

async fn post_handler(
    axum::extract::State(state): axum::extract::State<WebhookState>,
    body: axum::body::Bytes,
) -> axum::http::StatusCode {
    if body.is_empty() {
        return axum::http::StatusCode::BAD_REQUEST;
    }
    if let Err(e) = state.session.put(&state.data_topic, body.to_vec()).await {
        log::warn!("[webhook] {}: publish error: {}", state.skill_name, e);
    }
    axum::http::StatusCode::OK
}

impl BuiltInDriver for WebhookDriver {
    async fn run(self, mut ctx: BuiltInContext) -> anyhow::Result<()> {
        let port: u16 = ctx
            .config
            .get("port")
            .and_then(|v| v.as_u64())
            .ok_or_else(|| anyhow::anyhow!("webhook: missing required config key 'port'"))?
            .try_into()
            .map_err(|_| anyhow::anyhow!("webhook: port must be a valid u16"))?;

        // Security: bind localhost only — never 0.0.0.0
        let addr = SocketAddr::from(([127, 0, 0, 1], port));

        let data_topic = ctx.topic("data");
        let health_topic = ctx.topic("health");
        let skill_name = ctx.skill_name.clone();

        let state = WebhookState {
            session: ctx.session.clone(),
            data_topic,
            skill_name: skill_name.clone(),
        };

        let app = axum::Router::new()
            .route("/", axum::routing::post(post_handler))
            .with_state(state);

        let listener = tokio::net::TcpListener::bind(addr)
            .await
            .map_err(|e| anyhow::anyhow!("webhook: failed to bind 127.0.0.1:{} — {}", port, e))?;

        log::info!("[webhook] {} listening on 127.0.0.1:{}", skill_name, port);

        let health_pub = ctx
            .session
            .declare_publisher(health_topic)
            .await
            .map_err(|e| anyhow::anyhow!("webhook: health publisher error: {}", e))?;

        let mut health_ticker = tokio::time::interval(std::time::Duration::from_secs(5));
        let skill_name_health = skill_name.clone();

        // Spawn health heartbeat as a separate task
        let health_task = tokio::spawn(async move {
            loop {
                health_ticker.tick().await;
                if let Err(e) = health_pub.put(b"ok".to_vec()).await {
                    log::warn!(
                        "[webhook] {}: health publish error: {}",
                        skill_name_health,
                        e
                    );
                }
            }
        });

        // Serve with graceful shutdown driven by our watch channel
        axum::serve(listener, app)
            .with_graceful_shutdown(async move {
                ctx.shutdown_rx.changed().await.ok();
            })
            .await
            .map_err(|e| anyhow::anyhow!("webhook {}: server error: {}", skill_name, e))?;

        health_task.abort();
        log::info!("[webhook] {} shutting down", skill_name);
        Ok(())
    }
}
