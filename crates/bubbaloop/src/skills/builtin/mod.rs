//! Built-in driver implementations — run as tokio tasks inside the daemon.

pub mod http_poll;
pub mod system;

use std::collections::HashMap;
use std::sync::Arc;
use tokio::sync::watch;
use zenoh::Session;

/// Context passed to every built-in driver.
#[derive(Clone)]
pub struct BuiltInContext {
    pub session: Arc<Session>,
    pub scope: String,
    pub machine_id: String,
    pub skill_name: String,
    pub config: HashMap<String, serde_yaml::Value>,
    pub shutdown_rx: watch::Receiver<()>,
}

impl BuiltInContext {
    /// Returns the Zenoh topic for a resource under this skill.
    /// Format: `bubbaloop/{scope}/{machine_id}/{skill_name}/{resource}`
    pub fn topic(&self, resource: &str) -> String {
        format!(
            "bubbaloop/{}/{}/{}/{}",
            self.scope, self.machine_id, self.skill_name, resource
        )
    }
}

/// Trait for all built-in drivers.
///
/// # Panic policy
/// Drivers run inside the daemon process with `panic = "abort"`.
/// A panic WILL crash the daemon. Drivers MUST:
/// - Never call `.unwrap()` or `.expect()` on fallible operations
/// - Return `anyhow::Result<()>` and propagate errors with `?`
/// - Log errors and continue where appropriate (e.g., a failed HTTP poll)
#[allow(async_fn_in_trait)]
pub trait BuiltInDriver: Send + Sync {
    /// Run the driver until the shutdown signal fires.
    async fn run(self, ctx: BuiltInContext) -> anyhow::Result<()>;
}
