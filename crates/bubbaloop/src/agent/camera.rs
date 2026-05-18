//! Camera frame capture for the agent's `grab_frame` tool.
//!
//! Sends a Zenoh query to the camera node's `grab_frame` queryable and returns
//! the JPEG frame + metadata. The actual image encoding (RGBA→JPEG via kornia-rs)
//! lives in the camera node itself — the daemon base64-encodes the raw JPEG for the LLM.

use base64::Engine as _;
use std::sync::Arc;
use std::time::Duration;

/// An image captured from a camera, ready for injection into the next agent turn.
pub struct ImageAttachment {
    pub media_type: String,
    pub base64: String,
    /// Text label prepended before the image in the labeled-pair message.
    pub label: String,
}

/// Query the camera node's `grab_frame` queryable and return
/// `(receipt_json, ImageAttachment)` on success.
pub async fn grab_frame(
    session: &Arc<zenoh::Session>,
    node_name: &str,
    machine_id: &str,
) -> anyhow::Result<(String, ImageAttachment)> {
    let key = format!("bubbaloop/local/{}/{}/grab_frame", machine_id, node_name);

    let replies = session
        .get(&key)
        .timeout(Duration::from_secs(5))
        .await
        .map_err(|e| anyhow::anyhow!("Zenoh get failed: {}", e))?;

    let reply = tokio::time::timeout(Duration::from_secs(5), replies.recv_async())
        .await
        .map_err(|_| {
            anyhow::anyhow!(
                "camera '{}' did not respond within 5s on {}",
                node_name,
                key
            )
        })?
        .map_err(|_| anyhow::anyhow!("grab_frame channel closed for '{}'", node_name))?;

    let sample = reply
        .result()
        .map_err(|e| anyhow::anyhow!("grab_frame reply error for '{}': {:?}", node_name, e))?;

    // Camera nodes send raw JPEG bytes as payload and small JSON metadata as attachment.
    let jpeg_bytes = sample.payload().to_bytes().to_vec();

    let meta: serde_json::Value = sample
        .attachment()
        .and_then(|a| serde_json::from_slice(a.to_bytes().as_ref()).ok())
        .unwrap_or_default();

    // Surface error from camera node (legacy text error path)
    if let Ok(err_json) = serde_json::from_slice::<serde_json::Value>(&jpeg_bytes) {
        if let Some(err) = err_json.get("error").and_then(|e| e.as_str()) {
            return Err(anyhow::anyhow!("camera '{}' error: {}", node_name, err));
        }
    }

    // Reject stale frames — 10s is generous for live cameras.
    const MAX_AGE_MS: u64 = 10_000;
    if let Some(age_ms) = meta.get("age_ms").and_then(|a| a.as_u64()) {
        if age_ms > MAX_AGE_MS {
            return Err(anyhow::anyhow!(
                "camera '{}' frame is stale ({}ms old, max {}ms) — node may not be streaming",
                node_name,
                age_ms,
                MAX_AGE_MS
            ));
        }
    }

    let jpeg_b64 = base64::engine::general_purpose::STANDARD.encode(&jpeg_bytes);

    let media_type = meta
        .get("media_type")
        .and_then(|s| s.as_str())
        .unwrap_or("image/jpeg")
        .to_string();

    let label = meta
        .get("label")
        .and_then(|s| s.as_str())
        .unwrap_or(node_name)
        .to_string();

    let receipt = meta.to_string();

    Ok((
        receipt,
        ImageAttachment {
            media_type,
            base64: jpeg_b64,
            label,
        },
    ))
}
