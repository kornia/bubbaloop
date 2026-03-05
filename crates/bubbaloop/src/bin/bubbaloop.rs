//! Bubbaloop CLI - Unified command-line interface
//!
//! Usage:
//!   bubbaloop status [-f format]       # Show services status
//!   bubbaloop doctor                   # Run all system diagnostics
//!   bubbaloop doctor -c zenoh          # Check Zenoh connectivity only
//!   bubbaloop doctor -c daemon         # Check daemon health only
//!   bubbaloop doctor --json            # Output diagnostics as JSON
//!   bubbaloop doctor --fix             # Auto-fix issues
//!   bubbaloop node list                # List registered nodes
//!   bubbaloop node add <path|url>      # Add node from path or GitHub
//!   bubbaloop node start <name>        # Start a node
//!   bubbaloop node stop <name>         # Stop a node
//!   bubbaloop node logs <name>         # View node logs
//!   bubbaloop debug topics             # List active Zenoh topics
//!   bubbaloop debug subscribe <key>    # Subscribe to Zenoh topic
//!   bubbaloop debug query <key>        # Query Zenoh endpoint
//!   bubbaloop debug info               # Show Zenoh connection info

use argh::FromArgs;
use bubbaloop::cli::launch::LaunchCommand;
use bubbaloop::cli::{
    AgentCommand, DaemonCommand, DebugCommand, LoginCommand, LogoutCommand, MarketplaceCommand,
    NodeCommand, UpCommand,
};

/// Bubbaloop - AI-native orchestration for Physical AI
#[derive(FromArgs)]
struct Args {
    /// show version information
    #[argh(switch, short = 'V')]
    version: bool,

    #[argh(subcommand)]
    command: Option<Command>,
}

#[derive(FromArgs)]
#[argh(subcommand)]
enum Command {
    Agent(AgentCommand),
    Login(LoginCommand),
    Logout(LogoutCommand),
    Status(StatusArgs),
    Doctor(DoctorArgs),
    Daemon(DaemonCommand),
    Mcp(McpArgs),
    Node(NodeCommand),
    Launch(LaunchCommand),
    Marketplace(MarketplaceCommand),
    Debug(DebugCommand),
    Up(UpCommand),
    InitTls(InitTlsArgs),
}

/// Show services status (non-interactive)
#[derive(FromArgs)]
#[argh(subcommand, name = "status")]
struct StatusArgs {
    /// output format: table, json, yaml (default: table)
    #[argh(option, short = 'f', default = "String::from(\"table\")")]
    format: String,
}

/// Print TLS/mTLS certificate generation guide
#[derive(FromArgs)]
#[argh(subcommand, name = "init-tls")]
struct InitTlsArgs {
    /// output directory for certificates (default: ~/.bubbaloop/certs)
    #[argh(option, short = 'o')]
    output_dir: Option<String>,
}

/// Run system diagnostics and health checks
#[derive(FromArgs)]
#[argh(subcommand, name = "doctor")]
struct DoctorArgs {
    /// automatically fix issues that can be resolved
    #[argh(switch)]
    fix: bool,

    /// output results as JSON
    #[argh(switch)]
    json: bool,

    /// specific check to run: all, zenoh, daemon (default: all)
    #[argh(option, short = 'c', default = "String::from(\"all\")")]
    check: String,
}

/// Run MCP server for AI agent integration
#[derive(FromArgs)]
#[argh(subcommand, name = "mcp")]
struct McpArgs {
    /// run in stdio mode (reads JSON-RPC from stdin, writes to stdout)
    #[argh(switch)]
    stdio: bool,

    /// HTTP port (only used without --stdio, default: 8088)
    #[argh(option, short = 'p', default = "8088")]
    port: u16,

    /// zenoh endpoint to connect to (default: auto-discover local zenohd)
    #[argh(option, short = 'z')]
    zenoh_endpoint: Option<String>,
}

/// Run the MCP server (stdio or HTTP mode).
///
/// In stdio mode, logs are redirected to ~/.bubbaloop/mcp-stdio.log to avoid
/// corrupting the JSON-RPC protocol on stdout/stderr.
async fn run_mcp_command(args: McpArgs) -> Result<(), Box<dyn std::error::Error>> {
    if args.stdio {
        // Redirect logs to file to avoid corrupting the MCP JSON-RPC protocol.
        // stdout/stderr must stay clean for JSON-RPC messages.
        let bubbaloop_dir = dirs::home_dir()
            .unwrap_or_else(|| std::path::PathBuf::from("/tmp"))
            .join(".bubbaloop");
        std::fs::create_dir_all(&bubbaloop_dir).ok();
        let log_path = bubbaloop_dir.join("mcp-stdio.log");
        let log_file = std::fs::OpenOptions::new()
            .create(true)
            .append(true)
            .open(&log_path)
            .map_err(|e| format!("Failed to open log file {}: {}", log_path.display(), e))?;
        drop(
            env_logger::Builder::from_env(env_logger::Env::default().default_filter_or("info"))
                .target(env_logger::Target::Pipe(Box::new(log_file)))
                .try_init(),
        );
        log::info!(
            "MCP stdio server starting (logs redirected to {})",
            log_path.display()
        );
    } else {
        // HTTP mode: logs to stderr at info level
        init_logger("info");
    }

    // Create Zenoh session
    log::info!("Connecting to Zenoh...");
    let session = bubbaloop::daemon::create_session(args.zenoh_endpoint.as_deref())
        .await
        .map_err(|e| e as Box<dyn std::error::Error>)?;

    // Create node manager
    log::info!("Initializing node manager...");
    let node_manager = bubbaloop::daemon::NodeManager::new().await?;

    if args.stdio {
        log::info!("Starting MCP server in stdio mode...");
        bubbaloop::mcp::run_mcp_stdio(session, node_manager)
            .await
            .map_err(|e| e as Box<dyn std::error::Error>)?;
    } else {
        log::info!("Starting MCP server on HTTP port {}...", args.port);
        let (_shutdown_tx, shutdown_rx) = tokio::sync::watch::channel(());
        bubbaloop::mcp::run_mcp_server(session, node_manager, args.port, shutdown_rx)
            .await
            .map_err(|e| e as Box<dyn std::error::Error>)?;
    }

    Ok(())
}

/// Initialize the env_logger with a default filter string.
///
/// Uses `try_init` so it is safe to call multiple times (later calls are no-ops).
/// This is the standard pattern for CLI subcommands that override the log level set
/// in `main()`.
fn init_logger(default_filter: &str) {
    drop(
        env_logger::Builder::from_env(env_logger::Env::default().default_filter_or(default_filter))
            .try_init(),
    );
}

/// Create a Zenoh client session, printing a user-friendly error and returning
/// `None` if the connection fails (e.g. zenohd is not running).
async fn try_zenoh_session(endpoint: Option<&str>) -> Option<std::sync::Arc<zenoh::Session>> {
    match bubbaloop::cli::zenoh_session::create_zenoh_session(endpoint).await {
        Ok(s) => Some(s),
        Err(e) => {
            eprintln!("Error: Cannot connect to Zenoh — is zenohd running?");
            eprintln!("  {}", e);
            None
        }
    }
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    // Initialize logging — warn level by default; subcommands may override via init_logger().
    env_logger::Builder::from_env(env_logger::Env::default().default_filter_or("warn"))
        .target(env_logger::Target::Stderr)
        .init();

    let args: Args = argh::from_env();

    // Handle --version flag
    if args.version {
        println!("bubbaloop {}", env!("CARGO_PKG_VERSION"));
        return Ok(());
    }

    match args.command {
        // No subcommand = show help
        None => {
            eprintln!("Bubbaloop - AI-native orchestration for Physical AI\n");
            eprintln!("Usage: bubbaloop <command>\n");
            eprintln!("Commands:");
            eprintln!("  status    Show services status (non-interactive)");
            eprintln!("  doctor    Run system diagnostics and health checks");
            eprintln!("              --json: Output as JSON");
            eprintln!("              -c, --check <type>: all|zenoh|daemon (default: all)");
            eprintln!("              --fix: Auto-fix issues");
            eprintln!("  daemon    Manage the daemon lifecycle:");
            eprintln!("              run: Run in foreground (default)");
            eprintln!("              start: Start as systemd service");
            eprintln!("              stop: Graceful shutdown via Zenoh");
            eprintln!("              restart: Stop + start");
            eprintln!("              status: Show uptime, nodes, agents");
            eprintln!("              logs: Follow daemon logs");
            eprintln!("              fix: Auto-fix daemon issues");
            eprintln!("  mcp       Run MCP server for AI agent integration:");
            eprintln!("              --stdio: JSON-RPC over stdin/stdout");
            eprintln!("              -p, --port <port>: HTTP mode (default: 8088)");
            eprintln!("  node      Manage nodes:");
            eprintln!("              init, validate, list, add, remove");
            eprintln!("              install, uninstall, start, stop, restart");
            eprintln!("              logs, build");
            eprintln!("  launch    Launch node instances from a YAML file");
            eprintln!("              (default: ~/.bubbaloop/launch.yaml)");
            eprintln!("  marketplace  Manage marketplace sources:");
            eprintln!("              list, add, remove, enable, disable");
            eprintln!("  login     Authenticate with Anthropic API:");
            eprintln!("              --status: Show current auth status");
            eprintln!("  logout    Remove saved Anthropic API key");
            eprintln!("  agent     Interact with AI agents via the daemon:");
            eprintln!("              chat [-a agent[@machine]] [message]: Send messages to agents");
            eprintln!("              list [--all]: Show running agents and capabilities");
            eprintln!("  up        Load skills and ensure sensor nodes are running:");
            eprintln!("              -s, --skills-dir <path>: Skills directory");
            eprintln!("              --dry-run: Show what would be done");
            eprintln!("  debug     Debug Zenoh connectivity:");
            eprintln!("              info, topics, query, subscribe");
            eprintln!("  init-tls  Print TLS/mTLS certificate generation guide");
            eprintln!("\nRun 'bubbaloop <command> --help' for more information.");
            return Ok(());
        }
        Some(Command::Login(cmd)) => {
            if let Err(e) = cmd.run().await {
                eprintln!("Error: {}", e);
            }
        }
        Some(Command::Logout(cmd)) => {
            if let Err(e) = cmd.run() {
                eprintln!("Error: {}", e);
            }
        }
        Some(Command::Status(status_args)) => {
            bubbaloop::cli::status::run(&status_args.format).await?;
        }
        Some(Command::Doctor(args)) => {
            bubbaloop::cli::doctor::run(args.fix, args.json, &args.check).await?;
        }
        Some(Command::Daemon(cmd)) => {
            use bubbaloop::cli::daemon::DaemonSubcommand;

            match cmd.subcommand {
                // `bubbaloop daemon` with no subcommand = run in foreground (backward compat)
                None | Some(DaemonSubcommand::Run(_)) => {
                    // Re-initialize logging for daemon (info level, not warn)
                    init_logger("info");
                    bubbaloop::daemon::run(cmd.zenoh_endpoint).await?;
                }
                Some(DaemonSubcommand::Start(_)) => {
                    bubbaloop::cli::daemon_client::run_daemon_start().await?;
                }
                Some(DaemonSubcommand::Stop(_)) => {
                    init_logger("warn,zenoh=warn");
                    let Some(session) = try_zenoh_session(cmd.zenoh_endpoint.as_deref()).await
                    else {
                        return Ok(());
                    };
                    bubbaloop::cli::daemon_client::run_daemon_stop(session).await?;
                }
                Some(DaemonSubcommand::Restart(_)) => {
                    // Stop (best-effort) then start
                    init_logger("warn,zenoh=warn");
                    if let Some(session) = try_zenoh_session(cmd.zenoh_endpoint.as_deref()).await {
                        let _ = bubbaloop::cli::daemon_client::run_daemon_stop(session).await;
                        tokio::time::sleep(std::time::Duration::from_secs(2)).await;
                    }
                    bubbaloop::cli::daemon_client::run_daemon_start().await?;
                }
                Some(DaemonSubcommand::Status(_)) => {
                    init_logger("warn,zenoh=warn");
                    let Some(session) = try_zenoh_session(cmd.zenoh_endpoint.as_deref()).await
                    else {
                        return Ok(());
                    };
                    bubbaloop::cli::daemon_client::run_daemon_status(session).await?;
                }
                Some(DaemonSubcommand::Logs(_)) => {
                    bubbaloop::cli::daemon_client::run_daemon_logs()?;
                }
                Some(DaemonSubcommand::Fix(_)) => {
                    init_logger("warn,zenoh=warn");
                    let Some(session) = try_zenoh_session(cmd.zenoh_endpoint.as_deref()).await
                    else {
                        return Ok(());
                    };
                    bubbaloop::cli::daemon_client::run_daemon_fix(session).await?;
                }
            }
        }
        Some(Command::Mcp(args)) => {
            run_mcp_command(args).await?;
        }
        Some(Command::Node(cmd)) => {
            cmd.run()
                .await
                .map_err(|e| Box::new(e) as Box<dyn std::error::Error>)?;
        }
        Some(Command::Launch(cmd)) => {
            cmd.run()
                .await
                .map_err(|e| Box::new(e) as Box<dyn std::error::Error>)?;
        }
        Some(Command::Marketplace(cmd)) => {
            cmd.run()
                .await
                .map_err(|e| Box::new(e) as Box<dyn std::error::Error>)?;
        }
        Some(Command::Debug(cmd)) => {
            cmd.run()
                .await
                .map_err(|e| Box::new(e) as Box<dyn std::error::Error>)?;
        }
        Some(Command::Agent(cmd)) => {
            // First-run onboarding: interactive interview BEFORE anything else.
            // Pure stdin/stdout — no Zenoh, no daemon needed.
            if matches!(
                &cmd.subcommand,
                bubbaloop::cli::agent::AgentSubcommand::Chat(c) if c.message.is_none()
            ) {
                let soul_dir = bubbaloop::agent::soul::soul_directory();
                bubbaloop::agent::soul::Soul::ensure_defaults();
                if bubbaloop::agent::soul::Soul::is_first_run(&soul_dir) {
                    if let Err(e) = bubbaloop::agent::soul::Soul::run_onboarding(&soul_dir) {
                        eprintln!("Warning: onboarding failed: {}", e);
                    }
                }
            }

            // Suppress noisy Zenoh logs
            init_logger("warn,zenoh=warn");

            // Create Zenoh client session for all agent subcommands
            let Some(session) = try_zenoh_session(cmd.zenoh_endpoint.as_deref()).await else {
                return Ok(());
            };

            let scope = std::env::var("BUBBALOOP_SCOPE").unwrap_or_else(|_| "local".to_string());
            let local_machine_id = bubbaloop::daemon::util::get_machine_id();

            // Check daemon is reachable
            if !bubbaloop::cli::agent_client::is_daemon_running(&session, &scope, &local_machine_id)
                .await
            {
                eprintln!("Error: Daemon is not running.");
                eprintln!("  Start it with: bubbaloop daemon start");
                return Ok(());
            }

            match cmd.subcommand {
                bubbaloop::cli::agent::AgentSubcommand::Chat(chat_cmd) => {
                    let (agent, target_machine) =
                        parse_agent_target(chat_cmd.agent.as_deref(), &local_machine_id);
                    if let Err(e) = bubbaloop::cli::agent_client::run_agent_client(
                        session,
                        &scope,
                        target_machine,
                        agent,
                        chat_cmd.message.as_deref(),
                    )
                    .await
                    {
                        let err_str = e.to_string();
                        if err_str != "timeout" {
                            eprintln!("Error: {}", e);
                        }
                    }
                }
                bubbaloop::cli::agent::AgentSubcommand::List(list_cmd) => {
                    if let Err(e) = bubbaloop::cli::agent_client::run_agent_list(
                        session,
                        &scope,
                        &local_machine_id,
                        list_cmd.all,
                    )
                    .await
                    {
                        eprintln!("Error: {}", e);
                    }
                }
            }
        }
        Some(Command::Up(cmd)) => {
            cmd.run()
                .await
                .map_err(|e| Box::new(e) as Box<dyn std::error::Error>)?;
        }
        Some(Command::InitTls(args)) => {
            let cert_dir = args.output_dir.unwrap_or_else(|| {
                let home =
                    dirs::home_dir().unwrap_or_else(|| std::path::PathBuf::from("/home/user"));
                home.join(".bubbaloop/certs").to_string_lossy().to_string()
            });
            println!("TLS/mTLS Certificate Setup Guide");
            println!("=================================\n");
            println!("Target directory: {}\n", cert_dir);
            println!("1. Generate a CA key and certificate:");
            println!("   openssl genrsa -out {}/ca-key.pem 4096", cert_dir);
            println!(
                "   openssl req -new -x509 -key {}/ca-key.pem -sha256 \\",
                cert_dir
            );
            println!(
                "     -subj \"/CN=bubbaloop-ca\" -days 3650 -out {}/ca.pem\n",
                cert_dir
            );
            println!("2. Generate a server key and certificate:");
            println!("   openssl genrsa -out {}/server-key.pem 4096", cert_dir);
            println!("   openssl req -new -key {}/server-key.pem \\", cert_dir);
            println!(
                "     -subj \"/CN=$(hostname)\" -out {}/server.csr",
                cert_dir
            );
            println!("   openssl x509 -req -in {}/server.csr \\", cert_dir);
            println!(
                "     -CA {}/ca.pem -CAkey {}/ca-key.pem -CAcreateserial \\",
                cert_dir, cert_dir
            );
            println!("     -days 365 -sha256 -out {}/server-cert.pem\n", cert_dir);
            println!("3. Copy ca.pem to ALL machines in the deployment.\n");
            println!("4. Update Zenoh config (~/.bubbaloop/zenoh/zenohd.json5):");
            println!("   See: configs/zenoh/tls-example.json5\n");
            println!("5. Set BUBBALOOP_ZENOH_ENDPOINT on clients:");
            println!("   export BUBBALOOP_ZENOH_ENDPOINT=\"tls/<router-ip>:7447\"\n");
            println!("6. Verify with: bubbaloop doctor -c security");
        }
    }

    Ok(())
}

/// Parse "agent@machine" syntax. Returns (agent_id, machine_id).
/// - `Some("camera-expert@jetson_orin")` → `(Some("camera-expert"), "jetson_orin")`
/// - `Some("camera-expert")` → `(Some("camera-expert"), local_machine_id)`
/// - `None` → `(None, local_machine_id)`
fn parse_agent_target<'a>(agent: Option<&'a str>, local: &'a str) -> (Option<&'a str>, &'a str) {
    match agent {
        Some(s) if s.contains('@') => {
            let (name, machine) = s.split_once('@').unwrap();
            (Some(name), machine)
        }
        other => (other, local),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_agent_target_with_machine() {
        let (agent, machine) = parse_agent_target(Some("cam@jetson_orin"), "local_host");
        assert_eq!(agent, Some("cam"));
        assert_eq!(machine, "jetson_orin");
    }

    #[test]
    fn parse_agent_target_without_machine() {
        let (agent, machine) = parse_agent_target(Some("cam"), "local_host");
        assert_eq!(agent, Some("cam"));
        assert_eq!(machine, "local_host");
    }

    #[test]
    fn parse_agent_target_none() {
        let (agent, machine) = parse_agent_target(None, "local_host");
        assert_eq!(agent, None);
        assert_eq!(machine, "local_host");
    }
}
