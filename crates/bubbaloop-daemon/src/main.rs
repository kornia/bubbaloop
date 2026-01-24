//! Bubbaloop Daemon
//!
//! Central service for managing bubbaloop nodes via Zenoh and HTTP.
//!
//! # Features
//!
//! - Maintains authoritative node state in memory
//! - Communicates with systemd via D-Bus (native, no shell spawning)
//! - Publishes state to Zenoh topics (protobuf-encoded)
//! - Accepts commands via Zenoh queryables
//! - Provides HTTP REST API for TUI/dashboard access
//! - Runs as a systemd user service

use argh::FromArgs;
use bubbaloop_daemon::{http_server, NodeManager, ZenohService};
use tokio::sync::watch;

#[derive(FromArgs)]
/// Bubbaloop daemon - central service for node management
struct Args {
    /// zenoh endpoint to connect to (optional)
    /// Default: connects to local zenohd via default discovery
    #[argh(option, short = 'z')]
    zenoh_endpoint: Option<String>,

    /// HTTP server port (default: 8088)
    #[argh(option, short = 'p', default = "8088")]
    http_port: u16,

    /// disable Zenoh service (HTTP only mode)
    #[argh(switch)]
    http_only: bool,
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

    // Start HTTP server
    let http_manager = node_manager.clone();
    let http_port = args.http_port;
    let http_task = tokio::spawn(async move {
        if let Err(e) = http_server::run_http_server(http_manager, http_port).await {
            log::error!("HTTP server error: {}", e);
        }
    });

    // Start Zenoh service (unless in HTTP-only mode)
    if !args.http_only {
        log::info!("Connecting to Zenoh...");
        let zenoh_service = ZenohService::new(node_manager, args.zenoh_endpoint.as_deref()).await?;

        log::info!("Bubbaloop daemon running. Press Ctrl+C to exit.");
        log::info!("  HTTP API: http://localhost:{}", args.http_port);
        log::info!("  Zenoh topics: bubbaloop/daemon/*");

        // Run the Zenoh service (blocks until shutdown)
        zenoh_service.run(shutdown_rx).await?;
    } else {
        log::info!("Bubbaloop daemon running in HTTP-only mode. Press Ctrl+C to exit.");
        log::info!("  HTTP API: http://localhost:{}", args.http_port);

        // Wait for shutdown signal
        let mut shutdown_rx = shutdown_rx;
        shutdown_rx.changed().await.ok();
    }

    // Abort HTTP server
    http_task.abort();

    log::info!("Bubbaloop daemon stopped.");

    Ok(())
}
