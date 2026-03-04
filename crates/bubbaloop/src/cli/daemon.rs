//! CLI subcommands for daemon lifecycle management.
//!
//! Mirrors the pattern from `cli/agent.rs`:
//! - `bubbaloop daemon run` — foreground (current behavior)
//! - `bubbaloop daemon start` — install systemd service + start
//! - `bubbaloop daemon stop` — graceful stop via Zenoh gateway
//! - `bubbaloop daemon restart` — stop + start
//! - `bubbaloop daemon status` — query manifest, show uptime/nodes/agents
//! - `bubbaloop daemon logs` — journalctl follow
//! - `bubbaloop daemon fix` — doctor-style auto-fix

use argh::FromArgs;

/// Manage the daemon lifecycle
#[derive(FromArgs, PartialEq, Debug)]
#[argh(subcommand, name = "daemon")]
pub struct DaemonCommand {
    /// zenoh endpoint to connect to (default: auto-discover)
    #[argh(option, short = 'z')]
    pub zenoh_endpoint: Option<String>,

    #[argh(subcommand)]
    pub subcommand: Option<DaemonSubcommand>,
}

#[derive(FromArgs, PartialEq, Debug)]
#[argh(subcommand)]
pub enum DaemonSubcommand {
    Run(RunCommand),
    Start(StartCommand),
    Stop(StopCommand),
    Restart(RestartCommand),
    Status(StatusCommand),
    Logs(LogsCommand),
    Fix(FixCommand),
}

/// Run the daemon in foreground (default behavior)
#[derive(FromArgs, PartialEq, Debug)]
#[argh(subcommand, name = "run")]
pub struct RunCommand {}

/// Start the daemon as a background systemd service
#[derive(FromArgs, PartialEq, Debug)]
#[argh(subcommand, name = "start")]
pub struct StartCommand {}

/// Stop the daemon gracefully via Zenoh gateway
#[derive(FromArgs, PartialEq, Debug)]
#[argh(subcommand, name = "stop")]
pub struct StopCommand {}

/// Restart the daemon (stop + start)
#[derive(FromArgs, PartialEq, Debug)]
#[argh(subcommand, name = "restart")]
pub struct RestartCommand {}

/// Show daemon status (uptime, nodes, agents)
#[derive(FromArgs, PartialEq, Debug)]
#[argh(subcommand, name = "status")]
pub struct StatusCommand {}

/// Follow daemon logs via journalctl
#[derive(FromArgs, PartialEq, Debug)]
#[argh(subcommand, name = "logs")]
pub struct LogsCommand {}

/// Auto-fix daemon issues (restart if unhealthy)
#[derive(FromArgs, PartialEq, Debug)]
#[argh(subcommand, name = "fix")]
pub struct FixCommand {}
