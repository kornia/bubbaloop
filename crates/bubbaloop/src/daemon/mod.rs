//! Bubbaloop Skill Runtime
//!
//! Lightweight daemon for managing sensor/actuator nodes as discoverable skills.
//!
//! # Architecture
//!
//! - Registry: tracks installed nodes in ~/.bubbaloop/nodes.json
//! - Lifecycle: start/stop/restart via systemd D-Bus (zbus)
//! - Health: monitors node heartbeats via Zenoh pub/sub
//! - MCP Gateway: exposes all operations as MCP tools for AI agents
//!
//! External AI agents (Claude Code, etc.) interact exclusively through MCP.
//! The daemon never makes autonomous decisions — it's a passive skill runtime.

pub mod belief_updater;
pub mod constraints;
pub mod context_provider;
pub mod federated;
pub mod gateway;
pub mod mission;
pub mod node_manager;
pub mod reactive;
pub mod registry;
pub mod systemd;
pub mod telemetry;
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

    // Peer mode — allows direct connections from co-located nodes
    config
        .insert_json5("mode", "\"peer\"")
        .expect("Failed to set Zenoh mode");

    // Resolve endpoint: parameter > env var > default
    let env_endpoint = std::env::var("BUBBALOOP_ZENOH_ENDPOINT").ok();
    let ep = endpoint
        .or(env_endpoint.as_deref())
        .unwrap_or("tcp/127.0.0.1:7447");

    log::info!("Connecting to Zenoh router at: {}", ep);

    config
        .insert_json5("connect/endpoints", &format!("[\"{}\"]", ep))
        .expect("Failed to set Zenoh endpoint");

    // Disable all scouting to prevent connecting to remote peers via Tailscale/VPN
    config
        .insert_json5("scouting/multicast/enabled", "false")
        .expect("Failed to disable multicast scouting");
    config
        .insert_json5("scouting/gossip/enabled", "false")
        .expect("Failed to disable gossip scouting");

    // Disable shared memory — bridge doesn't participate in the SHM segment
    config
        .insert_json5("transport/shared_memory/enabled", "false")
        .expect("Failed to disable shared memory");

    // Retry loop with exponential backoff (max 30 attempts ≈ 15 min with 30s cap)
    let max_backoff = Duration::from_secs(30);
    let max_attempts = 30u32;
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
                if attempt >= max_attempts {
                    log::error!(
                        "Zenoh connection failed after {} attempts, giving up",
                        max_attempts
                    );
                    return Err(e);
                }
                log::warn!(
                    "Zenoh connection attempt {}/{} failed: {}. Retrying in {:?}...",
                    attempt,
                    max_attempts,
                    e,
                    backoff
                );
                tokio::time::sleep(backoff).await;
                backoff = (backoff * 2).min(max_backoff);
            }
        }
    }
}

/// Run the daemon gateway: manifest queryable + command/event handling.
///
/// Registers the daemon on Zenoh so CLI clients can discover and control it
/// without HTTP, using the same gateway pattern as agents.
async fn run_daemon_gateway(
    session: Arc<Session>,
    node_manager: Arc<NodeManager>,
    mcp_port: u16,
    shutdown_tx: tokio::sync::watch::Sender<()>,
    mut shutdown_rx: tokio::sync::watch::Receiver<()>,
) -> Result<(), Box<dyn std::error::Error>> {
    let scope = std::env::var("BUBBALOOP_SCOPE").unwrap_or_else(|_| "local".to_string());
    let machine_id = util::get_machine_id();
    let start_time = std::time::Instant::now();

    // Create platform for dispatching commands
    let platform = std::sync::Arc::new(crate::mcp::platform::DaemonPlatform::new(
        node_manager.clone(),
        session.clone(),
        scope.clone(),
        machine_id.clone(),
    ));

    // 1. Register manifest queryable
    let manifest_key = gateway::manifest_topic(&scope, &machine_id);
    let manifest_session = session.clone();
    let manifest_machine_id = machine_id.clone();
    let manifest_nm = node_manager.clone();
    let manifest_port = mcp_port;
    let manifest_start = start_time;
    let mut manifest_shutdown = shutdown_rx.clone();
    tokio::spawn(async move {
        match manifest_session.declare_queryable(&manifest_key).await {
            Ok(queryable) => loop {
                tokio::select! {
                    result = queryable.recv_async() => {
                        match result {
                            Ok(query) => {
                                let node_list = manifest_nm.get_node_list().await;
                                let manifest = gateway::DaemonManifest {
                                    version: env!("CARGO_PKG_VERSION").to_string(),
                                    machine_id: manifest_machine_id.clone(),
                                    uptime_secs: manifest_start.elapsed().as_secs(),
                                    node_count: node_list.nodes.len(),
                                    agent_count: 0, // TODO: get from agent runtime
                                    mcp_port: manifest_port,
                                };
                                let payload = serde_json::to_vec(&manifest).unwrap_or_default();
                                let _ = query.reply(&manifest_key, payload).await;
                            }
                            Err(_) => break,
                        }
                    }
                    _ = manifest_shutdown.changed() => break,
                }
            },
            Err(e) => {
                log::warn!("[Gateway] Failed to register manifest queryable: {}", e);
            }
        }
    });

    // 2. Subscribe to command topic and dispatch
    let cmd_topic = gateway::command_topic(&scope, &machine_id);
    let evt_topic = gateway::events_topic(&scope, &machine_id);

    let subscriber = session.declare_subscriber(&cmd_topic).await.map_err(
        |e| -> Box<dyn std::error::Error> {
            format!("Failed to subscribe to command topic: {}", e).into()
        },
    )?;

    let publisher =
        session
            .declare_publisher(&evt_topic)
            .await
            .map_err(|e| -> Box<dyn std::error::Error> {
                format!("Failed to declare events publisher: {}", e).into()
            })?;

    log::info!(
        "[Gateway] Daemon gateway started: cmd={}, events={}, manifest={}",
        cmd_topic,
        evt_topic,
        gateway::manifest_topic(&scope, &machine_id)
    );

    loop {
        tokio::select! {
            result = subscriber.recv_async() => {
                match result {
                    Ok(sample) => {
                        let payload = sample.payload().to_bytes().to_vec();
                        match serde_json::from_slice::<gateway::DaemonCommand>(&payload) {
                            Ok(cmd) => {
                                let events = dispatch_daemon_command(
                                    &cmd,
                                    &platform,
                                    &shutdown_tx,
                                    start_time,
                                    mcp_port,
                                    &machine_id,
                                ).await;
                                for event in events {
                                    if let Ok(bytes) = serde_json::to_vec(&event) {
                                        if let Err(e) = publisher.put(bytes).await {
                                            log::warn!("[Gateway] Failed to publish event: {}", e);
                                        }
                                    }
                                }
                            }
                            Err(e) => {
                                log::warn!("[Gateway] Invalid command message: {}", e);
                            }
                        }
                    }
                    Err(e) => {
                        log::warn!("[Gateway] Command subscriber error: {}", e);
                        break;
                    }
                }
            }
            _ = shutdown_rx.changed() => {
                log::info!("[Gateway] Daemon gateway shutting down");
                break;
            }
        }
    }

    Ok(())
}

/// Dispatch a daemon command and return response events.
async fn dispatch_daemon_command(
    cmd: &gateway::DaemonCommand,
    platform: &std::sync::Arc<crate::mcp::platform::DaemonPlatform>,
    shutdown_tx: &tokio::sync::watch::Sender<()>,
    start_time: std::time::Instant,
    mcp_port: u16,
    machine_id: &str,
) -> Vec<gateway::DaemonEvent> {
    use crate::mcp::platform::PlatformOperations;

    let id = &cmd.id;
    let mut events = Vec::new();

    // Helper: validate node names from Zenoh messages before dispatching
    macro_rules! validate_name {
        ($name:expr) => {
            if let Err(e) = crate::validation::validate_node_name($name) {
                events.push(gateway::DaemonEvent::error(id, &e));
                events.push(gateway::DaemonEvent::done(id));
                return events;
            }
        };
    }

    match &cmd.command {
        gateway::DaemonCommandType::ListNodes => match platform.list_nodes().await {
            Ok(nodes) => {
                let text = serde_json::to_string(&nodes).unwrap_or_default();
                events.push(gateway::DaemonEvent::result(id, &text));
            }
            Err(e) => {
                events.push(gateway::DaemonEvent::error(id, &e.to_string()));
            }
        },
        gateway::DaemonCommandType::StartNode { name } => {
            validate_name!(name);
            match platform
                .execute_command(name, crate::mcp::platform::NodeCommand::Start)
                .await
            {
                Ok(msg) => events.push(gateway::DaemonEvent::result(id, &msg)),
                Err(e) => events.push(gateway::DaemonEvent::error(id, &e.to_string())),
            }
        }
        gateway::DaemonCommandType::StopNode { name } => {
            validate_name!(name);
            match platform
                .execute_command(name, crate::mcp::platform::NodeCommand::Stop)
                .await
            {
                Ok(msg) => events.push(gateway::DaemonEvent::result(id, &msg)),
                Err(e) => events.push(gateway::DaemonEvent::error(id, &e.to_string())),
            }
        }
        gateway::DaemonCommandType::RestartNode { name } => {
            validate_name!(name);
            match platform
                .execute_command(name, crate::mcp::platform::NodeCommand::Restart)
                .await
            {
                Ok(msg) => events.push(gateway::DaemonEvent::result(id, &msg)),
                Err(e) => events.push(gateway::DaemonEvent::error(id, &e.to_string())),
            }
        }
        gateway::DaemonCommandType::GetLogs { name } => {
            validate_name!(name);
            match platform
                .execute_command(name, crate::mcp::platform::NodeCommand::GetLogs)
                .await
            {
                Ok(msg) => events.push(gateway::DaemonEvent::result(id, &msg)),
                Err(e) => events.push(gateway::DaemonEvent::error(id, &e.to_string())),
            }
        }
        gateway::DaemonCommandType::BuildNode { name } => {
            validate_name!(name);
            match platform
                .execute_command(name, crate::mcp::platform::NodeCommand::Build)
                .await
            {
                Ok(msg) => events.push(gateway::DaemonEvent::result(id, &msg)),
                Err(e) => events.push(gateway::DaemonEvent::error(id, &e.to_string())),
            }
        }
        gateway::DaemonCommandType::InstallService { name } => {
            validate_name!(name);
            match platform
                .execute_command(name, crate::mcp::platform::NodeCommand::Install)
                .await
            {
                Ok(msg) => events.push(gateway::DaemonEvent::result(id, &msg)),
                Err(e) => events.push(gateway::DaemonEvent::error(id, &e.to_string())),
            }
        }
        gateway::DaemonCommandType::UninstallNode { name } => {
            validate_name!(name);
            match platform
                .execute_command(name, crate::mcp::platform::NodeCommand::Uninstall)
                .await
            {
                Ok(msg) => events.push(gateway::DaemonEvent::result(id, &msg)),
                Err(e) => events.push(gateway::DaemonEvent::error(id, &e.to_string())),
            }
        }
        gateway::DaemonCommandType::CleanNode { name } => {
            validate_name!(name);
            match platform
                .execute_command(name, crate::mcp::platform::NodeCommand::Clean)
                .await
            {
                Ok(msg) => events.push(gateway::DaemonEvent::result(id, &msg)),
                Err(e) => events.push(gateway::DaemonEvent::error(id, &e.to_string())),
            }
        }
        gateway::DaemonCommandType::EnableAutostart { name } => {
            validate_name!(name);
            match platform
                .execute_command(name, crate::mcp::platform::NodeCommand::EnableAutostart)
                .await
            {
                Ok(msg) => events.push(gateway::DaemonEvent::result(id, &msg)),
                Err(e) => events.push(gateway::DaemonEvent::error(id, &e.to_string())),
            }
        }
        gateway::DaemonCommandType::DisableAutostart { name } => {
            validate_name!(name);
            match platform
                .execute_command(name, crate::mcp::platform::NodeCommand::DisableAutostart)
                .await
            {
                Ok(msg) => events.push(gateway::DaemonEvent::result(id, &msg)),
                Err(e) => events.push(gateway::DaemonEvent::error(id, &e.to_string())),
            }
        }
        gateway::DaemonCommandType::InstallNode {
            source,
            name,
            config,
        } => {
            match platform
                .add_node(source, name.as_deref(), config.as_deref())
                .await
            {
                Ok(msg) => events.push(gateway::DaemonEvent::result(id, &msg)),
                Err(e) => events.push(gateway::DaemonEvent::error(id, &e.to_string())),
            }
        }
        gateway::DaemonCommandType::RemoveNode { name } => {
            validate_name!(name);
            match platform.remove_node(name).await {
                Ok(msg) => events.push(gateway::DaemonEvent::result(id, &msg)),
                Err(e) => events.push(gateway::DaemonEvent::error(id, &e.to_string())),
            }
        }
        gateway::DaemonCommandType::Health => {
            let node_list = platform.list_nodes().await.unwrap_or_default();
            let manifest = gateway::DaemonManifest {
                version: env!("CARGO_PKG_VERSION").to_string(),
                machine_id: machine_id.to_string(),
                uptime_secs: start_time.elapsed().as_secs(),
                node_count: node_list.len(),
                agent_count: 0,
                mcp_port,
            };
            let text = serde_json::to_string(&manifest).unwrap_or_default();
            events.push(gateway::DaemonEvent::result(id, &text));
        }
        gateway::DaemonCommandType::Shutdown => {
            log::info!("[Gateway] Received shutdown command");
            events.push(gateway::DaemonEvent::result(id, "shutting down"));
            shutdown_tx.send(()).ok();
        }
    }

    events.push(gateway::DaemonEvent::done(id));
    events
}

/// Run the daemon with the given configuration.
///
/// This is the main entry point called by `bubbaloop daemon`.
pub async fn run(zenoh_endpoint: Option<String>) -> Result<(), Box<dyn std::error::Error>> {
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

    // Note: Duplicate daemon detection via Zenoh queryable removed.
    // MCP server will bind to port and fail if already in use.

    // Start health monitor for Zenoh heartbeats
    log::info!("Starting health monitor...");
    if let Err(e) = node_manager
        .clone()
        .start_health_monitor(session.clone(), shutdown_rx.clone())
        .await
    {
        log::warn!("Failed to start health monitor: {}", e);
    }

    // Start telemetry watchdog
    log::info!("Starting telemetry watchdog...");
    let telemetry_service = std::sync::Arc::new(
        telemetry::TelemetryService::start(node_manager.clone(), shutdown_rx.clone()).await,
    );

    // Start MCP server (HTTP on port 8088)
    let mcp_port: u16 = std::env::var("BUBBALOOP_MCP_PORT")
        .ok()
        .and_then(|p| p.parse().ok())
        .unwrap_or(crate::mcp::MCP_PORT);

    let mcp_task = {
        let mcp_session = session.clone();
        let mcp_manager = node_manager.clone();
        let mcp_shutdown = shutdown_rx.clone();
        tokio::spawn(async move {
            if let Err(e) =
                crate::mcp::run_mcp_server(mcp_session, mcp_manager, mcp_port, mcp_shutdown).await
            {
                log::error!("MCP server error: {}", e);
            }
        })
    };

    // Start agent runtime (multi-agent Zenoh gateway)
    let agent_task = {
        let agent_session = session.clone();
        let agent_manager = node_manager.clone();
        let agent_shutdown = shutdown_rx.clone();
        let agent_telemetry = Some(telemetry_service.clone());
        tokio::spawn(async move {
            if let Err(e) = crate::agent::runtime::run_agent_runtime(
                agent_session,
                agent_manager,
                agent_shutdown,
                agent_telemetry,
            )
            .await
            {
                log::error!("Agent runtime error: {}", e);
            }
        })
    };

    // Start built-in skill runtime
    let skill_task = {
        let skill_session = session.clone();
        let skill_shutdown = shutdown_rx.clone();
        tokio::spawn(async move {
            let skills_dir = crate::daemon::registry::get_bubbaloop_home().join("skills");
            let scope = std::env::var("BUBBALOOP_SCOPE").unwrap_or_else(|_| "local".to_string());
            let machine_id = crate::daemon::util::get_machine_id();
            if let Err(e) = crate::skills::runtime::run_skill_runtime(
                skill_session,
                &skills_dir,
                &scope,
                &machine_id,
                skill_shutdown,
            )
            .await
            {
                log::error!("Skill runtime error: {}", e);
            }
        })
    };

    // Start daemon gateway (Zenoh manifest + command/event topics)
    let gateway_task = {
        let gw_session = session.clone();
        let gw_manager = node_manager.clone();
        let gw_shutdown_tx = shutdown_tx.clone();
        let gw_shutdown_rx = shutdown_rx.clone();
        tokio::spawn(async move {
            if let Err(e) = run_daemon_gateway(
                gw_session,
                gw_manager,
                mcp_port,
                gw_shutdown_tx,
                gw_shutdown_rx,
            )
            .await
            {
                log::error!("Daemon gateway error: {}", e);
            }
        })
    };

    log::info!("Bubbaloop skill runtime started.");
    log::info!("  MCP server: http://127.0.0.1:{}/mcp", mcp_port);
    log::info!("  Agent runtime: active");
    log::info!("  Daemon gateway: active");
    log::info!("  Skill runtime: active (0 built-in skills at startup; count updated at runtime)");
    log::info!("  Nodes: {} registered", initial_list.nodes.len());
    log::info!("  Health monitor: active (Zenoh heartbeats)");
    log::info!("  Telemetry watchdog: active");

    // Wait for shutdown signal
    let mut shutdown_wait = shutdown_rx.clone();
    shutdown_wait.changed().await.ok();

    log::info!("Shutdown signal received, waiting for tasks to gracefully finish...");

    // Wait for all tasks to complete gracefully (they all listen to shutdown_rx)
    let _ = tokio::join!(mcp_task, agent_task, gateway_task, skill_task);

    log::info!("Bubbaloop daemon stopped.");

    Ok(())
}
