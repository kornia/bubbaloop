use std::sync::Arc;

use crate::error::{NodeError, Result};

/// Open a Zenoh session in client mode.
///
/// Resolution order for endpoint:
/// 1. `ZENOH_ENDPOINT` env var (for compatibility with existing nodes)
/// 2. `BUBBALOOP_ZENOH_ENDPOINT` env var
/// 3. Provided `endpoint` argument
/// 4. Default: `tcp/127.0.0.1:7447`
pub async fn open_zenoh_session(endpoint: &Option<String>) -> Result<Arc<zenoh::Session>> {
    let endpoint = std::env::var("ZENOH_ENDPOINT")
        .or_else(|_| std::env::var("BUBBALOOP_ZENOH_ENDPOINT"))
        .ok()
        .or_else(|| endpoint.clone())
        .unwrap_or_else(|| "tcp/127.0.0.1:7447".to_string());

    log::info!("Connecting to Zenoh at: {}", endpoint);

    let mut config = zenoh::Config::default();
    // Client mode is mandatory — peer mode doesn't route through zenohd router
    config
        .insert_json5("mode", r#""client""#)
        .map_err(|e| NodeError::ZenohConfig {
            key: "mode",
            source: e,
        })?;
    config
        .insert_json5("connect/endpoints", &format!(r#"["{}"]"#, endpoint))
        .map_err(|e| NodeError::ZenohConfig {
            key: "connect/endpoints",
            source: e,
        })?;
    // Disable scouting to prevent connecting to remote peers via Tailscale/VPN
    config
        .insert_json5("scouting/multicast/enabled", "false")
        .map_err(|e| NodeError::ZenohConfig {
            key: "scouting/multicast/enabled",
            source: e,
        })?;
    config
        .insert_json5("scouting/gossip/enabled", "false")
        .map_err(|e| NodeError::ZenohConfig {
            key: "scouting/gossip/enabled",
            source: e,
        })?;

    let session = zenoh::open(config).await.map_err(NodeError::ZenohSession)?;

    log::info!("Connected to Zenoh");
    Ok(Arc::new(session))
}
