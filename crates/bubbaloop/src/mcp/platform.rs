//! Clean layer boundary between MCP server and daemon internals.
//!
//! MCP tools call PlatformOperations instead of Arc<NodeManager> directly,
//! making the MCP server testable with mock implementations.

use serde_json::Value;

/// Result type for platform operations.
pub type PlatformResult<T> = Result<T, PlatformError>;

/// Errors from platform operations.
#[derive(Debug, thiserror::Error)]
pub enum PlatformError {
    #[error("Node not found: {0}")]
    NodeNotFound(String),
    #[error("Command failed: {0}")]
    CommandFailed(String),
    #[error("Invalid input: {0}")]
    InvalidInput(String),
    #[error("Internal error: {0}")]
    Internal(String),
}

/// Node summary for list operations.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct NodeInfo {
    pub name: String,
    pub status: String,
    pub health: String,
    pub node_type: String,
    pub installed: bool,
    pub is_built: bool,
}

/// Command to execute on a node.
#[derive(Debug, Clone)]
pub enum NodeCommand {
    Start,
    Stop,
    Restart,
    Build,
    GetLogs,
    Install,
    Uninstall,
    Clean,
    EnableAutostart,
    DisableAutostart,
}

/// Abstraction over daemon internals.
///
/// MCP tools call this trait instead of `Arc<NodeManager>` directly.
/// This makes the MCP server testable with mock implementations.
pub trait PlatformOperations: Send + Sync + 'static {
    fn list_nodes(&self)
        -> impl std::future::Future<Output = PlatformResult<Vec<NodeInfo>>> + Send;
    fn get_node_detail(
        &self,
        name: &str,
    ) -> impl std::future::Future<Output = PlatformResult<Value>> + Send;
    fn execute_command(
        &self,
        name: &str,
        cmd: NodeCommand,
    ) -> impl std::future::Future<Output = PlatformResult<String>> + Send;
    fn get_node_config(
        &self,
        name: &str,
    ) -> impl std::future::Future<Output = PlatformResult<Value>> + Send;
    fn query_zenoh(
        &self,
        key_expr: &str,
    ) -> impl std::future::Future<Output = PlatformResult<String>> + Send;

    /// Send a Zenoh query with a payload (e.g., for node commands).
    ///
    /// Returns the collected reply strings.
    fn send_zenoh_query(
        &self,
        key_expr: &str,
        payload: Vec<u8>,
    ) -> impl std::future::Future<Output = PlatformResult<Vec<String>>> + Send;

    /// Get cached manifests from registered nodes, optionally filtered by capability.
    ///
    /// Returns `(node_name, manifest_json)` pairs. When `capability_filter` is set,
    /// only nodes whose manifest contains the matching capability are returned.
    fn get_manifests(
        &self,
        capability_filter: Option<&str>,
    ) -> impl std::future::Future<Output = PlatformResult<Vec<(String, Value)>>> + Send;

    /// Install a node from a source path (local directory or GitHub user/repo).
    /// Returns the name of the installed node.
    fn install_node(
        &self,
        source: &str,
    ) -> impl std::future::Future<Output = PlatformResult<String>> + Send;

    /// Register a node with optional name and config overrides.
    /// Used by `bubbaloop up` to create per-skill instances of the same node type.
    fn add_node(
        &self,
        source: &str,
        name_override: Option<&str>,
        config_override: Option<&str>,
    ) -> impl std::future::Future<Output = PlatformResult<String>> + Send;

    /// Remove a registered node. Stops it first if running.
    fn remove_node(
        &self,
        name: &str,
    ) -> impl std::future::Future<Output = PlatformResult<String>> + Send;

    /// Install a node from the marketplace by name.
    ///
    /// Fetches the registry, downloads the precompiled binary, registers
    /// the node with the daemon, and creates the systemd service.
    fn install_from_marketplace(
        &self,
        name: &str,
    ) -> impl std::future::Future<Output = PlatformResult<String>> + Send;

    // ── Agent proposals & scheduling ─────────────────────────────────

    /// List proposals, optionally filtered by status (pending, approved, rejected).
    fn list_proposals(
        &self,
        status_filter: Option<&str>,
    ) -> impl std::future::Future<Output = PlatformResult<String>> + Send;

    /// Approve a pending proposal by ID. Returns the approved proposal as JSON.
    fn approve_proposal(
        &self,
        id: &str,
        decided_by: &str,
    ) -> impl std::future::Future<Output = PlatformResult<String>> + Send;

    /// Reject a pending proposal by ID. Returns the rejected proposal as JSON.
    fn reject_proposal(
        &self,
        id: &str,
        decided_by: &str,
    ) -> impl std::future::Future<Output = PlatformResult<String>> + Send;

    /// Schedule a job for the agent. Returns the new job ID.
    fn schedule_job(
        &self,
        prompt: &str,
        cron_schedule: Option<&str>,
        recurrence: bool,
    ) -> impl std::future::Future<Output = PlatformResult<String>> + Send;

    // ── Memory admin ────────────────────────────────────────────────

    /// List jobs, optionally filtered by status. Returns JSON string.
    fn list_jobs(
        &self,
        status_filter: Option<&str>,
    ) -> impl std::future::Future<Output = PlatformResult<String>> + Send;

    /// Delete a specific job by ID.
    fn delete_job(
        &self,
        id: &str,
    ) -> impl std::future::Future<Output = PlatformResult<String>> + Send;

    /// Clear episodic memory older than N days. Returns count of pruned files.
    fn clear_episodic_memory(
        &self,
        older_than_days: u32,
    ) -> impl std::future::Future<Output = PlatformResult<String>> + Send;
}

// ── Re-exports for backward compatibility ─────────────────────────────

pub use super::daemon_platform::DaemonPlatform;

#[cfg(any(test, feature = "test-harness"))]
pub mod mock {
    pub use super::super::mock_platform::MockPlatform;
}
