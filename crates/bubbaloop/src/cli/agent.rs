use argh::FromArgs;

/// Start the interactive AI agent for hardware control
#[derive(FromArgs, PartialEq, Debug)]
#[argh(subcommand, name = "agent")]
pub struct AgentCommand {
    /// claude model to use (default: claude-sonnet-4-20250514)
    #[argh(option, short = 'm')]
    pub model: Option<String>,

    /// zenoh endpoint to connect to (default: auto-discover)
    #[argh(option, short = 'z')]
    pub zenoh_endpoint: Option<String>,
}
