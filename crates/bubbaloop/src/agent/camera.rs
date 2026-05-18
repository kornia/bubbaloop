//! Camera frame capture for the agent's `grab_frame` tool.
//!
//! Sends a Zenoh query to the camera node's `grab_frame` queryable and returns
//! the JPEG frame + metadata. The actual image encoding (RGBA→JPEG via kornia-rs)
//! lives in the camera node itself — the daemon receives only the base64 result.

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

    let payload = sample.payload().to_bytes();
    let v: serde_json::Value = serde_json::from_slice(payload.as_ref())
        .map_err(|e| anyhow::anyhow!("grab_frame JSON parse failed: {}", e))?;

    // Surface error from camera node
    if let Some(err) = v.get("error").and_then(|e| e.as_str()) {
        return Err(anyhow::anyhow!("camera '{}' error: {}", node_name, err));
    }

    // Reject stale frames — camera is likely stopped or not streaming.
    // 10 s is generous for live cameras; anything older means the cached
    // frame is from a previous run and would mislead vision analysis.
    const MAX_AGE_MS: u64 = 10_000;
    if let Some(age_ms) = v.get("age_ms").and_then(|a| a.as_u64()) {
        if age_ms > MAX_AGE_MS {
            return Err(anyhow::anyhow!(
                "camera '{}' frame is stale ({}ms old, max {}ms) — node may not be streaming",
                node_name,
                age_ms,
                MAX_AGE_MS
            ));
        }
    }

    let jpeg_b64 = v
        .get("jpeg_b64")
        .and_then(|s| s.as_str())
        .ok_or_else(|| anyhow::anyhow!("grab_frame response missing jpeg_b64"))?
        .to_string();

    let media_type = v
        .get("media_type")
        .and_then(|s| s.as_str())
        .unwrap_or("image/jpeg")
        .to_string();

    let label = v
        .get("label")
        .and_then(|s| s.as_str())
        .unwrap_or(node_name)
        .to_string();

    // Build receipt JSON from metadata fields (strip the large jpeg_b64 field)
    let receipt = {
        let mut r = v.clone();
        if let serde_json::Value::Object(ref mut m) = r {
            m.remove("jpeg_b64");
        }
        r.to_string()
    };

    Ok((
        receipt,
        ImageAttachment {
            media_type,
            base64: jpeg_b64,
            label,
        },
    ))
}
