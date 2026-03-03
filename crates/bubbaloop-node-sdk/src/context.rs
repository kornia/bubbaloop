use std::sync::Arc;

/// Context provided to nodes by the SDK runtime.
pub struct NodeContext {
    /// Shared Zenoh session (Arc-wrapped, safe to clone)
    pub session: Arc<zenoh::Session>,
    /// Deployment scope (from BUBBALOOP_SCOPE env, default: "local")
    pub scope: String,
    /// Machine identifier (from BUBBALOOP_MACHINE_ID env, default: hostname)
    pub machine_id: String,
    /// Shutdown signal receiver — select! on this in your main loop
    pub shutdown_rx: tokio::sync::watch::Receiver<()>,
}

impl NodeContext {
    /// Build a fully-qualified scoped topic: `bubbaloop/{scope}/{machine_id}/{suffix}`
    pub fn topic(&self, suffix: &str) -> anyhow::Result<String> {
        let illegal_chars = ['/', '*', '#'];
        if suffix.contains(&illegal_chars[..]) || suffix.contains("..") {
            anyhow::bail!("Security: Illegal characters in topic suffix '{}'", suffix);
        }
        let safe_suffix = suffix.trim_start_matches('/');
        Ok(format!("bubbaloop/{}/{}/{}", self.scope, self.machine_id, safe_suffix))
    }
}
