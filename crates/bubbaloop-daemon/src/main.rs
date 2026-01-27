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

use argh::FromArgs;
use bubbaloop_daemon::{create_session, run_zenoh_api_server, NodeManager, ZenohService};
use tokio::sync::watch;

#[derive(FromArgs)]
/// Bubbaloop daemon - central service for node management
struct Args {
    /// zenoh endpoint to connect to (optional)
    /// Default: connects to local zenohd via default discovery
    #[argh(option, short = 'z')]
    zenoh_endpoint: Option<String>,
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    // Initialize logging
    let env = env_logger::Env::default().default_filter_or("info");
    env_logger::init_from_env(env);

    let args: Args = argh::from_env();

    log::info!("Starting bubbaloop-daemon...");

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
    let session = create_session(args.zenoh_endpoint.as_deref()).await?;

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

    // Create and run Zenoh service (pub/sub with protobuf)
    let zenoh_service = ZenohService::new(session, node_manager);

    log::info!("Bubbaloop daemon running. Press Ctrl+C to exit.");
    log::info!("  Zenoh pub/sub topics: bubbaloop/daemon/*");
    log::info!("  Zenoh API queryables: bubbaloop/daemon/api/*");

    // Run the Zenoh service (blocks until shutdown)
    zenoh_service.run(shutdown_rx).await?;

    // Abort API service
    api_task.abort();

    log::info!("Bubbaloop daemon stopped.");

    Ok(())
}
