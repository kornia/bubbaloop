//! Zenoh service layer
//!
//! Handles pub/sub communication with clients via Zenoh.

use crate::node_manager::NodeManager;
use crate::proto::{CommandResult, NodeCommand};
use prost::Message;
use std::sync::Arc;
use std::time::Duration;
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

/// Key expressions for daemon topics
pub mod keys {
    /// Full node list (published periodically)
    pub const NODES_LIST: &str = "bubbaloop/daemon/nodes";

    /// Individual node state updates
    pub const NODE_STATE_PREFIX: &str = "bubbaloop/daemon/nodes/";

    /// Command queryable
    pub const COMMAND: &str = "bubbaloop/daemon/command";

    /// Node events (state changes)
    pub const EVENTS: &str = "bubbaloop/daemon/events";

    /// Get key for individual node state
    pub fn node_state_key(name: &str) -> String {
        format!("{}{}/state", NODE_STATE_PREFIX, name)
    }
}

/// Zenoh service for the daemon
pub struct ZenohService {
    session: Session,
    node_manager: Arc<NodeManager>,
}

impl ZenohService {
    /// Create a new Zenoh service
    pub async fn new(node_manager: Arc<NodeManager>, endpoint: Option<&str>) -> Result<Self> {
        // Configure Zenoh session
        let mut config = zenoh::Config::default();

        // If endpoint provided, connect to it
        if let Some(ep) = endpoint {
            config
                .insert_json5("connect/endpoints", &format!("[\"{}\"]", ep))
                .ok();
        }

        // Enable shared memory if available
        config
            .insert_json5("transport/shared_memory/enabled", "true")
            .ok();

        let session = zenoh::open(config).await?;

        Ok(Self {
            session,
            node_manager,
        })
    }

    /// Encode a protobuf message to ZBytes
    fn encode_proto<M: Message>(msg: &M) -> ZBytes {
        let mut buf = Vec::new();
        msg.encode(&mut buf).ok();
        ZBytes::from(buf)
    }

    /// Run the Zenoh service
    pub async fn run(self, mut shutdown: watch::Receiver<()>) -> Result<()> {
        log::info!("Starting Zenoh service...");

        // Declare queryable for commands
        let queryable = self.session.declare_queryable(keys::COMMAND).await?;

        log::info!("Declared queryable on {}", keys::COMMAND);

        // Declare publisher for node list
        let list_publisher = self.session.declare_publisher(keys::NODES_LIST).await?;

        log::info!("Declared publisher on {}", keys::NODES_LIST);

        // Declare publisher for events
        let event_publisher = self.session.declare_publisher(keys::EVENTS).await?;

        log::info!("Declared publisher on {}", keys::EVENTS);

        // Subscribe to node manager events
        let mut event_rx = self.node_manager.subscribe();

        // Publish initial state
        let initial_list = self.node_manager.get_node_list().await;
        let initial_bytes = Self::encode_proto(&initial_list);
        list_publisher.put(initial_bytes).await?;
        log::info!(
            "Published initial node list ({} nodes)",
            initial_list.nodes.len()
        );

        // Create periodic publish timer
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

                    // Publish current state
                    let list = self.node_manager.get_node_list().await;
                    let bytes = Self::encode_proto(&list);
                    if let Err(e) = list_publisher.put(bytes).await {
                        log::warn!("Failed to publish node list: {}", e);
                    }
                }

                // Handle incoming queries (commands)
                query = queryable.recv_async() => {
                    match query {
                        Ok(query) => {
                            self.handle_query(&query).await;
                        }
                        Err(e) => {
                            log::warn!("Query receive error: {}", e);
                        }
                    }
                }

                // Handle node manager events
                event = event_rx.recv() => {
                    match event {
                        Ok(event) => {
                            // Publish event
                            let event_bytes = Self::encode_proto(&event);
                            if let Err(e) = event_publisher.put(event_bytes).await {
                                log::warn!("Failed to publish event: {}", e);
                            }

                            // Also publish updated node state
                            if let Some(ref state) = event.state {
                                let key = keys::node_state_key(&state.name);
                                let state_bytes = Self::encode_proto(state);
                                if let Err(e) = self.session.put(&key, state_bytes).await {
                                    log::warn!("Failed to publish node state: {}", e);
                                }
                            }

                            // Publish updated list
                            let list = self.node_manager.get_node_list().await;
                            let list_bytes = Self::encode_proto(&list);
                            if let Err(e) = list_publisher.put(list_bytes).await {
                                log::warn!("Failed to publish node list: {}", e);
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
