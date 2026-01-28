//! Bubbaloop CLI - Unified command-line interface
//!
//! Usage:
//!   bubbaloop                       # Launch TUI (default)
//!   bubbaloop tui                   # Launch TUI explicitly
//!   bubbaloop status [-f format]    # Show services status
//!   bubbaloop doctor                # Run system diagnostics
//!   bubbaloop plugin init name      # Initialize a new plugin
//!   bubbaloop plugin list           # List installed plugins
//!   bubbaloop node list             # List registered nodes
//!   bubbaloop node add <path|url>   # Add node from path or GitHub
//!   bubbaloop node start <name>     # Start a node
//!   bubbaloop node stop <name>      # Stop a node
//!   bubbaloop node logs <name>      # View node logs
//!   bubbaloop debug topics          # List active Zenoh topics
//!   bubbaloop debug subscribe <key> # Subscribe to Zenoh topic
//!   bubbaloop debug query <key>     # Query Zenoh endpoint
//!   bubbaloop debug info            # Show Zenoh connection info

use argh::FromArgs;
use bubbaloop::cli::{DebugCommand, NodeCommand, PluginCommand};

/// Bubbaloop - Physical AI camera streaming platform
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
    Tui(TuiArgs),
    Status(StatusArgs),
    Doctor(DoctorArgs),
    Plugin(PluginCommand),
    Node(NodeCommand),
    Debug(DebugCommand),
}

/// Launch the terminal user interface
#[derive(FromArgs)]
#[argh(subcommand, name = "tui")]
struct TuiArgs {}

/// Show services status (non-interactive)
#[derive(FromArgs)]
#[argh(subcommand, name = "status")]
struct StatusArgs {
    /// output format: table, json, yaml (default: table)
    #[argh(option, short = 'f', default = "String::from(\"table\")")]
    format: String,
}

/// Run system diagnostics and health checks
#[derive(FromArgs)]
#[argh(subcommand, name = "doctor")]
struct DoctorArgs {}

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    // Initialize logging (stderr to avoid interfering with TUI)
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
        // No subcommand = launch TUI (default behavior)
        None => {
            bubbaloop::tui::run().await?;
        }
        Some(Command::Tui(_)) => {
            bubbaloop::tui::run().await?;
        }
        Some(Command::Status(status_args)) => {
            bubbaloop::cli::status::run(&status_args.format).await?;
        }
        Some(Command::Doctor(_)) => {
            bubbaloop::cli::doctor::run().await?;
        }
        Some(Command::Plugin(cmd)) => {
            cmd.run()
                .map_err(|e| Box::new(e) as Box<dyn std::error::Error>)?;
        }
        Some(Command::Node(cmd)) => {
            cmd.run()
                .await
                .map_err(|e| Box::new(e) as Box<dyn std::error::Error>)?;
        }
        Some(Command::Debug(cmd)) => {
            cmd.run()
                .await
                .map_err(|e| Box::new(e) as Box<dyn std::error::Error>)?;
        }
    }

    Ok(())
}
