//! Bubbaloop CLI
//!
//! Main command-line interface for Bubbaloop.
//!
//! Usage:
//!   bubbaloop plugin init my-sensor --type rust
//!   bubbaloop plugin init my-sensor --type python
//!   bubbaloop plugin list

use argh::FromArgs;
use bubbaloop::cli::{PluginCommand, PluginError};

/// Bubbaloop - Physical AI camera streaming platform
#[derive(FromArgs)]
struct Args {
    #[argh(subcommand)]
    command: Command,
}

#[derive(FromArgs)]
#[argh(subcommand)]
enum Command {
    Plugin(PluginCommand),
}

fn main() -> Result<(), PluginError> {
    // Initialize logging
    env_logger::Builder::from_env(env_logger::Env::default().default_filter_or("info"))
        .target(env_logger::Target::Stderr)
        .init();

    let args: Args = argh::from_env();

    match args.command {
        Command::Plugin(cmd) => cmd.run(),
    }
}
