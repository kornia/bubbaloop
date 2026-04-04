use std::sync::Arc;
use std::time::Duration;
use zenoh::handlers::FifoChannel;
use zenoh::sample::Sample;

use crate::error::{NodeError, Result};

/// Subscribe to `key_expr` and return the first sample received within `timeout`.
///
/// Useful for agents that need to pull a current value from a continuously-publishing
/// node without maintaining a long-lived subscription. A new subscriber is declared,
/// one sample is awaited, then the subscriber is dropped.
///
/// Returns [`NodeError::GetSampleTimeout`] if no sample arrives before the deadline.
///
/// # Example
///
/// ```ignore
/// let sample = get_sample(&session, "bubbaloop/local/host/openmeteo/weather",
///                          Duration::from_secs(5)).await?;
/// let payload = sample.payload().to_bytes();
/// let weather: serde_json::Value = serde_json::from_slice(&payload)?;
/// ```
pub async fn get_sample(
    session: &Arc<zenoh::Session>,
    key_expr: &str,
    timeout: Duration,
) -> Result<Sample> {
    let subscriber = session
        .declare_subscriber(key_expr.to_string())
        .with(FifoChannel::new(1))
        .await
        .map_err(|e| NodeError::SubscriberDeclare {
            topic: key_expr.to_string(),
            source: e,
        })?;

    tokio::time::timeout(timeout, subscriber.handler().recv_async())
        .await
        .ok()
        .and_then(|r| r.ok())
        .ok_or_else(|| NodeError::GetSampleTimeout {
            topic: key_expr.to_string(),
        })
}
