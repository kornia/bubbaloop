//! Shared Zenoh session factory for CLI commands.
//!
//! All CLI subcommands that need a Zenoh session should use [`create_zenoh_session`]
//! instead of duplicating the env-var + client-mode setup.

use std::sync::Arc;

/// Create a Zenoh session in client mode, optionally connecting to a specific endpoint.
///
/// Resolution order:
/// 1. `endpoint` argument (if `Some`)
/// 2. `BUBBALOOP_ZENOH_ENDPOINT` environment variable
/// 3. Default: `tcp/127.0.0.1:7447`
///
/// Scouting (multicast + gossip) is disabled — the CLI always connects directly to a
/// known router endpoint.
pub async fn create_zenoh_session(endpoint: Option<&str>) -> anyhow::Result<Arc<zenoh::Session>> {
    crate::agent::create_agent_session(endpoint)
        .await
        .map_err(|e| anyhow::anyhow!("{}", e))
}
