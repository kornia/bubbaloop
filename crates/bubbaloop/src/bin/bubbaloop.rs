//! Bubbaloop CLI - Unified command-line interface
//!
//! Usage:
//!   bubbaloop tui                   # Launch TUI
//!   bubbaloop status [-f format]    # Show services status
//!   bubbaloop doctor                # Run system diagnostics
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
use bubbaloop::cli::{DebugCommand, NodeCommand};

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
    Tui(TuiArgs),
    Status(StatusArgs),
    Doctor(DoctorArgs),
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
struct DoctorArgs {
    /// automatically fix issues that can be resolved
    #[argh(switch)]
    fix: bool,
}

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
        // No subcommand = show help
        None => {
            eprintln!("Bubbaloop - AI-native orchestration for Physical AI\n");
            eprintln!("Usage: bubbaloop <command>\n");
            eprintln!("Commands:");
            eprintln!("  tui       Launch the terminal user interface");
            eprintln!("  status    Show services status (non-interactive)");
            eprintln!("  doctor    Run system diagnostics and health checks");
            eprintln!("  node      Manage nodes:");
            eprintln!("              init, validate, list, add, remove");
            eprintln!("              install, uninstall, start, stop, restart");
            eprintln!("              logs, build");
            eprintln!("  debug     Debug Zenoh connectivity:");
            eprintln!("              info, topics, query, subscribe");
            eprintln!("\nRun 'bubbaloop <command> --help' for more information.");
            return Ok(());
        }
        Some(Command::Tui(_)) => {
            bubbaloop::tui::run().await?;
        }
        Some(Command::Status(status_args)) => {
            bubbaloop::cli::status::run(&status_args.format).await?;
        }
        Some(Command::Doctor(args)) => {
            bubbaloop::cli::doctor::run(args.fix).await?;
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
