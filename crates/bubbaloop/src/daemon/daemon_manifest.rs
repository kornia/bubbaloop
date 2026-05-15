//! Daemon-level manifest queryable.
//!
//! Serves `bubbaloop/global/{machine_id}/daemon/manifest` with the HTTP
//! endpoint and version so the dashboard can discover where to send chat turns.
//! This is separate from `AgentManifest` (per-agent capabilities/model) because
//! the HTTP address is daemon-scoped, not agent-scoped.

use serde::{Deserialize, Serialize};
use std::sync::Arc;

/// Wire format for the daemon manifest response.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DaemonManifest {
    /// HTTP base URL where the daemon's REST API is reachable.
    ///
    /// Set to the value of `BUBBALOOP_HTTP_BIND` (or `127.0.0.1:8088`).
    /// The dashboard joins `agent.machine_id` to this daemon to form the
    /// POST URL: `{http_endpoint}/api/v1/agents/{agent_id}/turns`.
    pub http_endpoint: String,
    /// Daemon version string (from `CARGO_PKG_VERSION`).
    pub version: String,
    /// Machine identifier (same as in all topic paths).
    pub machine_id: String,
}

/// Zenoh key expression for a specific machine's daemon manifest.
pub fn daemon_manifest_topic(machine_id: &str) -> String {
    format!("bubbaloop/global/{}/daemon/manifest", machine_id)
}

/// Wildcard for discovering daemon manifests on all machines.
pub fn daemon_manifest_wildcard() -> &'static str {
    "bubbaloop/global/*/daemon/manifest"
}

/// Register a queryable that serves the daemon manifest.
///
/// Spawns a background task that answers queries on the daemon manifest topic.
/// The manifest includes the HTTP endpoint derived from `BUBBALOOP_HTTP_BIND`
/// (defaulting to `http://127.0.0.1:8088`).
pub async fn register_daemon_manifest(
    session: &Arc<zenoh::Session>,
    machine_id: &str,
    port: u16,
    mut shutdown_rx: tokio::sync::watch::Receiver<()>,
) {
    let http_endpoint = build_http_endpoint(port);
    let manifest = DaemonManifest {
        http_endpoint,
        version: env!("CARGO_PKG_VERSION").to_string(),
        machine_id: machine_id.to_string(),
    };
    let manifest_json = match serde_json::to_vec(&manifest) {
        Ok(b) => b,
        Err(e) => {
            log::error!("[DaemonManifest] Failed to serialize manifest: {}", e);
            return;
        }
    };

    let topic = daemon_manifest_topic(machine_id);
    let session = session.clone();

    tokio::spawn(async move {
        match session.declare_queryable(&topic).await {
            Ok(queryable) => {
                log::info!("[DaemonManifest] Serving manifest at {}", topic);
                loop {
                    tokio::select! {
                        Ok(query) = queryable.recv_async() => {
                            let _ = query.reply(&topic, manifest_json.clone()).await;
                        }
                        _ = shutdown_rx.changed() => {
                            log::debug!("[DaemonManifest] Shutting down manifest queryable");
                            break;
                        }
                    }
                }
            }
            Err(e) => {
                log::warn!(
                    "[DaemonManifest] Failed to register queryable at {}: {}",
                    topic,
                    e
                );
            }
        }
    });
}

/// Derive the public HTTP endpoint published in the daemon manifest.
///
/// Resolution order:
/// 1. `BUBBALOOP_HTTP_PUBLIC_URL` — full URL, e.g. `http://100.119.20.61:8088`
/// 2. `BUBBALOOP_HTTP_BIND` — bind address coerced to URL (strip `0.0.0.0` → localhost)
/// 3. Default: `http://127.0.0.1:{port}`
fn build_http_endpoint(port: u16) -> String {
    // Explicit public URL takes precedence
    if let Ok(url) = std::env::var("BUBBALOOP_HTTP_PUBLIC_URL") {
        if url.starts_with("http://") || url.starts_with("https://") {
            return url;
        }
    }
    // Fall back to bind address, replacing 0.0.0.0 with localhost
    let bind =
        std::env::var("BUBBALOOP_HTTP_BIND").unwrap_or_else(|_| format!("127.0.0.1:{}", port));
    if bind.starts_with("http://") || bind.starts_with("https://") {
        return bind;
    }
    // Replace unroutable 0.0.0.0 with 127.0.0.1
    let bind = bind.replace("0.0.0.0", "127.0.0.1");
    format!("http://{}", bind)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn daemon_manifest_topic_format() {
        assert_eq!(
            daemon_manifest_topic("jetson01"),
            "bubbaloop/global/jetson01/daemon/manifest"
        );
    }

    #[test]
    fn daemon_manifest_wildcard_format() {
        assert_eq!(
            daemon_manifest_wildcard(),
            "bubbaloop/global/*/daemon/manifest"
        );
    }

    #[test]
    fn build_http_endpoint_default() {
        // Without BUBBALOOP_HTTP_BIND set: uses default localhost bind
        // (env var may or may not be set in test environment — just verify no panic)
        let ep = build_http_endpoint(8088);
        assert!(
            ep.starts_with("http://"),
            "endpoint must be an HTTP URL: {}",
            ep
        );
    }

    #[test]
    fn build_http_endpoint_passthrough_full_url() {
        std::env::set_var("BUBBALOOP_HTTP_BIND", "http://jetson.local:8088");
        let ep = build_http_endpoint(8088);
        assert_eq!(ep, "http://jetson.local:8088");
        std::env::remove_var("BUBBALOOP_HTTP_BIND");
    }

    #[test]
    fn daemon_manifest_serde_roundtrip() {
        let m = DaemonManifest {
            http_endpoint: "http://127.0.0.1:8088".to_string(),
            version: "0.0.14".to_string(),
            machine_id: "jetson01".to_string(),
        };
        let json = serde_json::to_string(&m).unwrap();
        let parsed: DaemonManifest = serde_json::from_str(&json).unwrap();
        assert_eq!(parsed.http_endpoint, m.http_endpoint);
        assert_eq!(parsed.version, m.version);
        assert_eq!(parsed.machine_id, m.machine_id);
    }
}
