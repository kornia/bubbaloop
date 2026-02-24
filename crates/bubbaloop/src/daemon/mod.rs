//! Bubbaloop Daemon
//!
//! Central service for managing bubbaloop nodes via Zenoh.
//!
//! # Features
//!
//! - Maintains authoritative node state in memory
//! - Communicates with systemd via D-Bus (native, no shell spawning)
//! - Publishes state to Zenoh topics (protobuf-encoded)
//! - Accepts commands via Zenoh queryables
//! - Provides REST-like API via Zenoh queryables (JSON)
//! - Runs as a systemd user service

pub mod node_manager;
pub mod registry;
pub mod systemd;
pub mod util;
pub mod zenoh_api;
pub mod zenoh_service;

pub use node_manager::NodeManager;
pub use zenoh_api::run_zenoh_api_server;
pub use zenoh_service::{create_session, ZenohService};

/// Run the daemon with the given configuration.
///
/// This is the main entry point called by `bubbaloop daemon`.
pub async fn run(
    zenoh_endpoint: Option<String>,
    strict: bool,
) -> Result<(), Box<dyn std::error::Error>> {
    use std::sync::Arc;
    use tokio::sync::watch;

    log::info!("Starting bubbaloop daemon...");

    // Create shutdown channel
    let (shutdown_tx, shutdown_rx) = watch::channel(());

    // Set up Ctrl+C handler
    ctrlc::set_handler({
        let shutdown_tx = shutdown_tx.clone();
        move || {
            log::info!("Received Ctrl+C, shutting down gracefully...");
            shutdown_tx.send(()).ok();
        }
    })?;

    // Create node manager
    log::info!("Initializing node manager...");
    let node_manager = NodeManager::new().await?;

    // Log initial state
    let initial_list = node_manager.get_node_list().await;
    log::info!(
        "Node manager initialized with {} nodes",
        initial_list.nodes.len()
    );

    for node in &initial_list.nodes {
        log::info!(
            "  - {} (status: {:?}, installed: {}, built: {})",
            node.name,
            node.status,
            node.installed,
            node.is_built
        );
    }

    // Start systemd signal listener for real-time state updates
    log::info!("Starting systemd signal listener...");
    if let Err(e) = node_manager.clone().start_signal_listener().await {
        log::warn!(
            "Failed to start signal listener (will rely on polling): {}",
            e
        );
    }

    // Create shared Zenoh session
    log::info!("Connecting to Zenoh...");
    let session = create_session(zenoh_endpoint.as_deref()).await?;

    // Check for duplicate daemon instances
    log::info!("Checking for duplicate daemon instances...");
    match session
        .get("bubbaloop/daemon/api/health")
        .timeout(std::time::Duration::from_secs(1))
        .await
    {
        Ok(replies) => {
            // Try to get a reply
            let mut has_response = false;
            while let Ok(reply) = replies.recv_async().await {
                match reply.result() {
                    Ok(_) => {
                        has_response = true;
                        break;
                    }
                    Err(_) => continue,
                }
            }

            if has_response {
                log::warn!("Another daemon is already running!");
                log::warn!("Multiple daemons will cause conflicting queryables and 'Query not found' errors.");
                log::warn!(
                    "To prevent this, use the --strict flag to exit when a duplicate is detected."
                );

                if strict {
                    return Err("Another daemon is already running (strict mode)".into());
                }
            } else {
                log::info!("No existing daemon detected, proceeding with startup.");
            }
        }
        Err(e) => {
            log::debug!(
                "Health check query failed (this is expected if no daemon is running): {}",
                e
            );
        }
    }

    // Start health monitor for Zenoh heartbeats
    log::info!("Starting health monitor...");
    if let Err(e) = node_manager
        .clone()
        .start_health_monitor(session.clone())
        .await
    {
        log::warn!("Failed to start health monitor: {}", e);
    }

    // Start Zenoh API service (REST-like queryables with JSON responses)
    let api_manager = node_manager.clone();
    let api_session = session.clone();
    let api_shutdown = shutdown_rx.clone();
    let api_task = tokio::spawn(async move {
        if let Err(e) = run_zenoh_api_server(api_session, api_manager, api_shutdown).await {
            log::error!("Zenoh API service error: {}", e);
        }
    });

    // Start agent rule engine
    let agent = Arc::new(crate::agent::Agent::new(
        session.clone(),
        node_manager.clone(),
    ));
    let agent_shutdown = shutdown_rx.clone();
    let agent_ref = agent.clone();
    let agent_task = tokio::spawn(async move {
        agent_ref.run(agent_shutdown).await;
    });

    // Start MCP server (HTTP on port 8088, feature-gated)
    #[cfg(feature = "mcp")]
    let mcp_task = {
        let mcp_session = session.clone();
        let mcp_manager = node_manager.clone();
        let mcp_agent = Some(agent.clone());
        let mcp_shutdown = shutdown_rx.clone();
        let mcp_port = std::env::var("BUBBALOOP_MCP_PORT")
            .ok()
            .and_then(|p| p.parse().ok())
            .unwrap_or(crate::mcp::MCP_PORT);
        tokio::spawn(async move {
            if let Err(e) = crate::mcp::run_mcp_server(
                mcp_session,
                mcp_manager,
                mcp_agent,
                mcp_port,
                mcp_shutdown,
            )
            .await
            {
                log::error!("MCP server error: {}", e);
            }
        })
    };

    // Create and run Zenoh service (pub/sub with protobuf)
    let zenoh_service = ZenohService::new(session, node_manager);

    log::info!("Bubbaloop daemon running. Press Ctrl+C to exit.");
    log::info!("  Zenoh pub/sub topics: bubbaloop/daemon/*");
    log::info!("  Zenoh API queryables: bubbaloop/daemon/api/*");
    log::info!("  Agent rule engine: active");
    #[cfg(feature = "mcp")]
    log::info!(
        "  MCP server: http://127.0.0.1:{}/mcp",
        std::env::var("BUBBALOOP_MCP_PORT")
            .ok()
            .and_then(|p| p.parse::<u16>().ok())
            .unwrap_or(crate::mcp::MCP_PORT)
    );

    // Run the Zenoh service (blocks until shutdown)
    zenoh_service.run(shutdown_rx).await?;

    // Abort services
    api_task.abort();
    agent_task.abort();

    // Abort MCP server
    #[cfg(feature = "mcp")]
    mcp_task.abort();

    log::info!("Bubbaloop daemon stopped.");

    Ok(())
}
