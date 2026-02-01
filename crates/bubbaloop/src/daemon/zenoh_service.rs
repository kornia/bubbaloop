//! Zenoh service layer
//!
//! Handles pub/sub communication with clients via Zenoh.

use crate::daemon::node_manager::NodeManager;
use crate::schemas::daemon::v1::{CommandResult, NodeCommand};
use prost::Message;
use std::sync::Arc;
use std::time::{Duration, SystemTime, UNIX_EPOCH};
use thiserror::Error;
use tokio::sync::watch;
use zenoh::bytes::ZBytes;
use zenoh::Session;

#[derive(Error, Debug)]
pub enum ZenohServiceError {
    #[error("Zenoh error: {0}")]
    Zenoh(#[from] zenoh::Error),

    #[error("Protobuf decode error: {0}")]
    ProtoDecode(#[from] prost::DecodeError),

    #[error("Channel closed")]
    ChannelClosed,
}

pub type Result<T> = std::result::Result<T, ZenohServiceError>;

/// Get machine ID from environment or hostname
fn get_machine_id() -> String {
    std::env::var("BUBBALOOP_MACHINE_ID").unwrap_or_else(|_| {
        hostname::get()
            .map(|h| h.to_string_lossy().to_string())
            .unwrap_or_else(|_| "unknown".to_string())
    })
}

/// Key expressions for daemon topics
pub mod keys {
    use super::get_machine_id;

    // Legacy keys (for backward compatibility)
    /// Full node list (legacy, no machine_id)
    pub const NODES_LIST_LEGACY: &str = "bubbaloop/daemon/nodes";

    /// Individual node state updates (legacy, no machine_id)
    pub const NODE_STATE_PREFIX_LEGACY: &str = "bubbaloop/daemon/nodes/";

    /// Command queryable (legacy, no machine_id)
    pub const COMMAND_LEGACY: &str = "bubbaloop/daemon/command";

    /// Node events (legacy, no machine_id)
    pub const EVENTS_LEGACY: &str = "bubbaloop/daemon/events";

    // New machine-scoped keys
    /// Get key for full node list with machine_id
    pub fn nodes_list_key(machine_id: &str) -> String {
        format!("bubbaloop/{}/daemon/nodes", machine_id)
    }

    /// Get key for individual node state with machine_id
    pub fn node_state_key(machine_id: &str, name: &str) -> String {
        format!("bubbaloop/{}/daemon/nodes/{}/state", machine_id, name)
    }

    /// Get key for events with machine_id
    pub fn events_key(machine_id: &str) -> String {
        format!("bubbaloop/{}/daemon/events", machine_id)
    }

    /// Get key for command queryable with machine_id
    pub fn command_key(machine_id: &str) -> String {
        format!("bubbaloop/{}/daemon/command", machine_id)
    }

    /// Get key for individual node state (legacy, no machine_id)
    pub fn node_state_key_legacy(name: &str) -> String {
        format!("{}{}/state", NODE_STATE_PREFIX_LEGACY, name)
    }

    /// Get current machine ID
    pub fn get_current_machine_id() -> String {
        get_machine_id()
    }
}

/// Create a Zenoh session with optional endpoint configuration
///
/// Endpoint resolution order:
/// 1. Explicit `endpoint` parameter (if Some)
/// 2. `BUBBALOOP_ZENOH_ENDPOINT` environment variable
/// 3. Default: `tcp/127.0.0.1:7447`
///
/// For distributed deployments, set `BUBBALOOP_ZENOH_ENDPOINT` to point to the local router.
pub async fn create_session(endpoint: Option<&str>) -> Result<Arc<Session>> {
    // Configure Zenoh session
    let mut config = zenoh::Config::default();

    // Run as peer mode - connect to router but can also accept connections for shared memory
    config.insert_json5("mode", "\"peer\"").ok();

    // Resolve endpoint: parameter > env var > default
    let env_endpoint = std::env::var("BUBBALOOP_ZENOH_ENDPOINT").ok();
    let ep = endpoint
        .or(env_endpoint.as_deref())
        .unwrap_or("tcp/127.0.0.1:7447");

    log::info!("Connecting to Zenoh router at: {}", ep);

    config
        .insert_json5("connect/endpoints", &format!("[\"{}\"]", ep))
        .ok();

    // Disable all scouting to prevent connecting to remote peers via Tailscale/VPN
    config
        .insert_json5("scouting/multicast/enabled", "false")
        .ok();
    config.insert_json5("scouting/gossip/enabled", "false").ok();

    // Disable shared memory - it prevents the bridge from receiving larger payloads
    // because the bridge doesn't participate in the SHM segment
    config
        .insert_json5("transport/shared_memory/enabled", "false")
        .ok();

    let session = zenoh::open(config).await?;

    Ok(Arc::new(session))
}

/// Zenoh service for the daemon
pub struct ZenohService {
    session: Arc<Session>,
    node_manager: Arc<NodeManager>,
    machine_id: String,
}

impl ZenohService {
    /// Create a new Zenoh service with an existing session
    pub fn new(session: Arc<Session>, node_manager: Arc<NodeManager>) -> Self {
        let machine_id = get_machine_id();
        log::info!("ZenohService using machine_id: {}", machine_id);
        Self {
            session,
            node_manager,
            machine_id,
        }
    }

    /// Encode a protobuf message to ZBytes
    fn encode_proto<M: Message>(msg: &M) -> ZBytes {
        let mut buf = Vec::new();
        msg.encode(&mut buf).ok();
        ZBytes::from(buf)
    }

    /// Get current timestamp in milliseconds
    fn now_ms() -> i64 {
        SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_millis() as i64
    }

    /// Run the Zenoh service
    pub async fn run(self, mut shutdown: watch::Receiver<()>) -> Result<()> {
        log::info!("Starting Zenoh service...");

        // Declare queryables for commands (both legacy and new)
        let command_key_new = keys::command_key(&self.machine_id);
        let queryable = self.session.declare_queryable(keys::COMMAND_LEGACY).await?;
        let queryable_new = self.session.declare_queryable(&command_key_new).await?;

        log::info!("Declared queryable on {}", keys::COMMAND_LEGACY);
        log::info!("Declared queryable on {}", command_key_new);

        // Declare publishers for node list (both legacy and new)
        let list_publisher = self
            .session
            .declare_publisher(keys::NODES_LIST_LEGACY)
            .await?;
        let nodes_list_key_new = keys::nodes_list_key(&self.machine_id);
        let list_publisher_new = self.session.declare_publisher(&nodes_list_key_new).await?;

        log::info!("Declared publisher on {}", keys::NODES_LIST_LEGACY);
        log::info!("Declared publisher on {}", nodes_list_key_new);

        // Declare publishers for events (both legacy and new)
        let event_publisher = self.session.declare_publisher(keys::EVENTS_LEGACY).await?;
        let events_key_new = keys::events_key(&self.machine_id);
        let event_publisher_new = self.session.declare_publisher(&events_key_new).await?;

        log::info!("Declared publisher on {}", keys::EVENTS_LEGACY);
        log::info!("Declared publisher on {}", events_key_new);

        // Declare queryables for node list (returns current state on GET)
        let nodes_queryable = self
            .session
            .declare_queryable(keys::NODES_LIST_LEGACY)
            .await?;
        let nodes_queryable_new = self.session.declare_queryable(&nodes_list_key_new).await?;
        log::info!("Declared nodes queryable on {}", keys::NODES_LIST_LEGACY);
        log::info!("Declared nodes queryable on {}", nodes_list_key_new);

        // Give Zenoh time to propagate queryable declarations across the network
        tokio::time::sleep(std::time::Duration::from_millis(100)).await;
        log::info!("All queryables registered, ready to accept commands");

        // Subscribe to node manager events
        let mut event_rx = self.node_manager.subscribe();

        // Publish initial state (both legacy and new)
        let initial_list = self.node_manager.get_node_list().await;
        let initial_bytes = Self::encode_proto(&initial_list);
        list_publisher.put(initial_bytes.clone()).await?;
        list_publisher_new.put(initial_bytes).await?;
        log::info!(
            "Published initial node list ({} nodes)",
            initial_list.nodes.len()
        );

        // Create periodic publish timer (5s - frequent enough that new subscribers get data quickly)
        let mut interval = tokio::time::interval(Duration::from_secs(5));

        loop {
            tokio::select! {
                // Handle shutdown
                _ = shutdown.changed() => {
                    log::info!("Zenoh service shutting down...");
                    break;
                }

                // Periodic state publish
                _ = interval.tick() => {
                    // Refresh state from systemd
                    if let Err(e) = self.node_manager.refresh_all().await {
                        log::warn!("Failed to refresh node state: {}", e);
                    }

                    // Publish current state (both legacy and new), but only if non-empty
                    let list = self.node_manager.get_node_list().await;
                    if !list.nodes.is_empty() {
                        let bytes = Self::encode_proto(&list);
                        if let Err(e) = list_publisher.put(bytes.clone()).await {
                            log::warn!("Failed to publish node list (legacy): {}", e);
                        }
                        if let Err(e) = list_publisher_new.put(bytes).await {
                            log::warn!("Failed to publish node list (new): {}", e);
                        }
                    }
                }

                // Handle node list queries - legacy
                nodes_query = nodes_queryable.recv_async() => {
                    self.handle_nodes_query(nodes_query, "legacy").await;
                }

                // Handle node list queries - machine-scoped
                nodes_query_new = nodes_queryable_new.recv_async() => {
                    self.handle_nodes_query(nodes_query_new, "new").await;
                }

                // Handle incoming queries (commands) - legacy
                query = queryable.recv_async() => {
                    match query {
                        Ok(query) => {
                            self.handle_query(&query).await;
                        }
                        Err(e) => {
                            log::warn!("Query receive error (legacy): {}", e);
                        }
                    }
                }

                // Handle incoming queries (commands) - new
                query_new = queryable_new.recv_async() => {
                    match query_new {
                        Ok(query) => {
                            self.handle_query(&query).await;
                        }
                        Err(e) => {
                            log::warn!("Query receive error (new): {}", e);
                        }
                    }
                }

                // Handle node manager events
                event = event_rx.recv() => {
                    match event {
                        Ok(event) => {
                            // Publish event (both legacy and new)
                            let event_bytes = Self::encode_proto(&event);
                            if let Err(e) = event_publisher.put(event_bytes.clone()).await {
                                log::warn!("Failed to publish event (legacy): {}", e);
                            }
                            if let Err(e) = event_publisher_new.put(event_bytes).await {
                                log::warn!("Failed to publish event (new): {}", e);
                            }

                            // Also publish updated node state (both legacy and new)
                            if let Some(ref state) = event.state {
                                let key_legacy = keys::node_state_key_legacy(&state.name);
                                let key_new = keys::node_state_key(&self.machine_id, &state.name);
                                let state_bytes = Self::encode_proto(state);
                                if let Err(e) = self.session.put(&key_legacy, state_bytes.clone()).await {
                                    log::warn!("Failed to publish node state (legacy): {}", e);
                                }
                                if let Err(e) = self.session.put(&key_new, state_bytes).await {
                                    log::warn!("Failed to publish node state (new): {}", e);
                                }
                            }

                            // Publish updated list (both legacy and new), but only if non-empty
                            let list = self.node_manager.get_node_list().await;
                            if !list.nodes.is_empty() {
                                let list_bytes = Self::encode_proto(&list);
                                if let Err(e) = list_publisher.put(list_bytes.clone()).await {
                                    log::warn!("Failed to publish node list (legacy): {}", e);
                                }
                                if let Err(e) = list_publisher_new.put(list_bytes).await {
                                    log::warn!("Failed to publish node list (new): {}", e);
                                }
                            }
                        }
                        Err(_) => {
                            // Channel lagged, continue
                        }
                    }
                }
            }
        }

        Ok(())
    }

    /// Handle a node list query (GET on nodes key)
    async fn handle_nodes_query(
        &self,
        result: std::result::Result<zenoh::query::Query, zenoh::Error>,
        label: &str,
    ) {
        match result {
            Ok(query) => {
                let list = self.node_manager.get_node_list().await;
                let bytes = Self::encode_proto(&list);
                log::debug!(
                    "Replying to nodes query ({}) with {} nodes ({} bytes)",
                    label,
                    list.nodes.len(),
                    bytes.len()
                );
                if let Err(e) = query.reply(query.key_expr(), bytes).await {
                    log::warn!("Failed to reply to nodes query ({}): {}", label, e);
                }
            }
            Err(e) => {
                log::warn!("Nodes query receive error ({}): {}", label, e);
            }
        }
    }

    /// Handle an incoming query (command request)
    async fn handle_query(&self, query: &zenoh::query::Query) {
        log::debug!("Received query on {}", query.key_expr());

        // Get payload from query
        let payload = match query.payload() {
            Some(p) => p.to_bytes().to_vec(),
            None => {
                log::warn!("Query has no payload");
                let result = CommandResult {
                    request_id: String::new(),
                    success: false,
                    message: "No payload in query".to_string(),
                    output: String::new(),
                    node_state: None,
                    timestamp_ms: Self::now_ms(),
                    responding_machine: self.machine_id.clone(),
                };
                let result_bytes = Self::encode_proto(&result);
                query.reply(query.key_expr(), result_bytes).await.ok();
                return;
            }
        };

        // Decode command
        let cmd = match NodeCommand::decode(&payload[..]) {
            Ok(cmd) => cmd,
            Err(e) => {
                log::warn!("Failed to decode command: {}", e);
                let result = CommandResult {
                    request_id: String::new(),
                    success: false,
                    message: format!("Failed to decode command: {}", e),
                    output: String::new(),
                    node_state: None,
                    timestamp_ms: Self::now_ms(),
                    responding_machine: self.machine_id.clone(),
                };
                let result_bytes = Self::encode_proto(&result);
                query.reply(query.key_expr(), result_bytes).await.ok();
                return;
            }
        };

        log::info!(
            "Executing command {:?} for node '{}'",
            cmd.command,
            cmd.node_name
        );

        // Execute command
        let result = self.node_manager.execute_command(cmd).await;

        log::info!(
            "Command result: success={}, message='{}'",
            result.success,
            result.message
        );

        // Reply with result
        let result_bytes = Self::encode_proto(&result);
        if let Err(e) = query.reply(query.key_expr(), result_bytes).await {
            log::warn!("Failed to send reply: {}", e);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::schemas::daemon::v1::{HealthStatus, NodeState, NodeStatus};

    #[test]
    fn test_encode_proto_encodes_valid_message() {
        let state = NodeState {
            name: "test-node".to_string(),
            path: "/path/to/node".to_string(),
            status: NodeStatus::Running as i32,
            installed: true,
            autostart_enabled: false,
            version: "0.1.0".to_string(),
            description: "Test node".to_string(),
            node_type: "rust".to_string(),
            is_built: true,
            last_updated_ms: 1234567890,
            build_output: vec![],
            health_status: HealthStatus::Healthy as i32,
            last_health_check_ms: 1234567890,
            machine_id: "test-machine".to_string(),
            machine_hostname: "test-hostname".to_string(),
            machine_ips: vec!["192.168.1.100".to_string()],
        };

        let bytes = ZenohService::encode_proto(&state);

        // Verify it's not empty
        assert!(!bytes.is_empty());

        // Verify we can decode it back
        let decoded = NodeState::decode(&bytes.to_bytes()[..]).unwrap();
        assert_eq!(decoded.name, "test-node");
        assert_eq!(decoded.path, "/path/to/node");
        assert_eq!(decoded.version, "0.1.0");
        assert_eq!(decoded.status, NodeStatus::Running as i32);
    }

    #[test]
    fn test_encode_proto_handles_empty_message() {
        let state = NodeState::default();
        let bytes = ZenohService::encode_proto(&state);

        // Empty messages may encode to an empty buffer (no fields to encode)
        // but should still be decodeable
        let decoded = NodeState::decode(&bytes.to_bytes()[..]).unwrap();
        assert_eq!(decoded.name, "");
        assert_eq!(decoded.status, 0);
    }

    #[test]
    fn test_encode_proto_with_build_output() {
        let state = NodeState {
            name: "test-node".to_string(),
            build_output: vec![
                "Building...".to_string(),
                "Compiling...".to_string(),
                "Finished".to_string(),
            ],
            ..Default::default()
        };

        let bytes = ZenohService::encode_proto(&state);
        let decoded = NodeState::decode(&bytes.to_bytes()[..]).unwrap();

        assert_eq!(decoded.build_output.len(), 3);
        assert_eq!(decoded.build_output[0], "Building...");
        assert_eq!(decoded.build_output[2], "Finished");
    }
}
