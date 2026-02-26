//! Bubbaloop Daemon
//!
//! Central service for managing bubbaloop nodes.
//!
//! # Features
//!
//! - Maintains authoritative node state in memory
//! - Communicates with systemd via D-Bus (native, no shell spawning)
//! - Provides MCP tools for external access
//! - Runs as a systemd user service

pub mod node_manager;
pub mod registry;
pub mod systemd;
pub mod util;

pub use node_manager::NodeManager;

use std::sync::Arc;
use std::time::Duration;
use zenoh::Session;

/// Create a Zenoh session with optional endpoint configuration
///
/// Endpoint resolution order:
/// 1. Explicit `endpoint` parameter (if Some)
/// 2. `BUBBALOOP_ZENOH_ENDPOINT` environment variable
/// 3. Default: `tcp/127.0.0.1:7447`
///
/// For distributed deployments, set `BUBBALOOP_ZENOH_ENDPOINT` to point to the local router.
pub async fn create_session(endpoint: Option<&str>) -> Result<Arc<Session>, zenoh::Error> {
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

    // Retry loop with exponential backoff
    let max_backoff = Duration::from_secs(30);
    let mut backoff = Duration::from_secs(1);
    let mut attempt = 0u32;

    loop {
        attempt += 1;
        match zenoh::open(config.clone()).await {
            Ok(session) => {
                if attempt > 1 {
                    log::info!("Zenoh session established after {} attempts", attempt);
                }
                return Ok(Arc::new(session));
            }
            Err(e) => {
                log::warn!(
                    "Zenoh connection attempt {} failed: {}. Retrying in {:?}...",
                    attempt,
                    e,
                    backoff
                );
                tokio::time::sleep(backoff).await;
                backoff = (backoff * 2).min(max_backoff);
            }
        }
    }
}

/// Run the daemon with the given configuration.
///
/// This is the main entry point called by `bubbaloop daemon`.
pub async fn run(
    zenoh_endpoint: Option<String>,
    strict: bool,
) -> Result<(), Box<dyn std::error::Error>> {
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
    let session = create_session(zenoh_endpoint.as_deref())
        .await
        .map_err(|e| e as Box<dyn std::error::Error>)?;

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

    // Start MCP server (HTTP on port 8088)
    let mcp_task = {
        let mcp_session = session.clone();
        let mcp_manager = node_manager.clone();
        let mcp_shutdown = shutdown_rx.clone();
        let mcp_port = std::env::var("BUBBALOOP_MCP_PORT")
            .ok()
            .and_then(|p| p.parse().ok())
            .unwrap_or(crate::mcp::MCP_PORT);
        tokio::spawn(async move {
            if let Err(e) =
                crate::mcp::run_mcp_server(mcp_session, mcp_manager, mcp_port, mcp_shutdown).await
            {
                log::error!("MCP server error: {}", e);
            }
        })
    };

    log::info!("Bubbaloop daemon running. Press Ctrl+C to exit.");
    log::info!(
        "  MCP server: http://127.0.0.1:{}/mcp",
        std::env::var("BUBBALOOP_MCP_PORT")
            .ok()
            .and_then(|p| p.parse::<u16>().ok())
            .unwrap_or(crate::mcp::MCP_PORT)
    );

    // Wait for shutdown signal
    let mut shutdown_wait = shutdown_rx.clone();
    shutdown_wait.changed().await.ok();

    // Abort MCP server
    mcp_task.abort();

    log::info!("Bubbaloop daemon stopped.");

    Ok(())
}
