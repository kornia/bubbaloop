use argh::FromArgs;

/// Interact with AI agents via the daemon
#[derive(FromArgs, PartialEq, Debug)]
#[argh(subcommand, name = "agent")]
pub struct AgentCommand {
    /// zenoh endpoint to connect to (default: auto-discover)
    #[argh(option, short = 'z')]
    pub zenoh_endpoint: Option<String>,

    #[argh(subcommand)]
    pub subcommand: AgentSubcommand,
}

#[derive(FromArgs, PartialEq, Debug)]
#[argh(subcommand)]
pub enum AgentSubcommand {
    Chat(ChatCommand),
    List(ListCommand),
}

/// Send messages to an agent via the daemon (auto-starts daemon if needed)
#[derive(FromArgs, PartialEq, Debug)]
#[argh(subcommand, name = "chat")]
pub struct ChatCommand {
    /// target agent ID (default: routes to default agent)
    #[argh(option, short = 'a')]
    pub agent: Option<String>,

    /// single message to send (exits after response, no REPL)
    #[argh(positional)]
    pub message: Option<String>,
}

/// List running agents and their capabilities
#[derive(FromArgs, PartialEq, Debug)]
#[argh(subcommand, name = "list")]
pub struct ListCommand {
    /// discover agents on all machines (not just local)
    #[argh(switch)]
    pub all: bool,
}
