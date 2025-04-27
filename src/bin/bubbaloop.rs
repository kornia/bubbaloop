use argh::FromArgs;

// defaults for the server
const DEFAULT_HOST: &str = "0.0.0.0";
const DEFAULT_PORT: u16 = 3000;

#[derive(FromArgs)]
/// Bubbaloop CLI
struct CLIArgs {
    #[argh(subcommand)]
    commands: Commands,

    #[argh(option, short = 'h', default = "DEFAULT_HOST.to_string()")]
    /// the host to listen on
    host: String,

    #[argh(option, short = 'p', default = "DEFAULT_PORT")]
    /// the port to listen on
    port: u16,
}

#[derive(FromArgs)]
#[argh(subcommand)]
enum Commands {
    Pipeline(PipelineCommand),
    Recording(RecordingCommand),
    Stats(StatsCommand),
}

#[derive(FromArgs)]
#[argh(subcommand, name = "stats")]
/// Get stats about the server
struct StatsCommand {
    #[argh(subcommand)]
    mode: StatsMode,
}

#[derive(FromArgs)]
#[argh(subcommand)]
enum StatsMode {
    Whoami(StatsWhoamiCommand),
    Sysinfo(StatsSysinfoCommand),
}

#[derive(FromArgs)]
#[argh(subcommand, name = "whoami")]
/// Print the whoami
struct StatsWhoamiCommand {}

#[derive(FromArgs)]
#[argh(subcommand, name = "sysinfo")]
/// Print the sysinfo
struct StatsSysinfoCommand {}

#[derive(FromArgs)]
#[argh(subcommand, name = "recording")]
/// Recording management commands
struct RecordingCommand {
    #[argh(subcommand)]
    mode: RecordingMode,
}

#[derive(FromArgs)]
#[argh(subcommand)]
enum RecordingMode {
    Start(RecordingStartCommand),
    Stop(RecordingStopCommand),
}

#[derive(FromArgs)]
#[argh(subcommand, name = "start")]
/// Start recording
struct RecordingStartCommand {}

#[derive(FromArgs)]
#[argh(subcommand, name = "stop")]
/// Stop recording
struct RecordingStopCommand {}

#[derive(FromArgs)]
#[argh(subcommand, name = "pipeline")]
/// Pipeline management commands
struct PipelineCommand {
    #[argh(subcommand)]
    mode: PipelineMode,
}

#[derive(FromArgs)]
#[argh(subcommand)]
enum PipelineMode {
    Start(PipelineStartCommand),
    Stop(PipelineStopCommand),
    List(PipelineListCommand),
}

#[derive(FromArgs)]
#[argh(subcommand, name = "start")]
/// Start a pipeline
struct PipelineStartCommand {
    #[argh(option, short = 'n')]
    /// the pipeline name
    name: String,
}

#[derive(FromArgs)]
#[argh(subcommand, name = "stop")]
/// Stop a pipeline
struct PipelineStopCommand {
    #[argh(option, short = 'n')]
    /// the pipeline name
    name: String,
}

#[derive(FromArgs)]
#[argh(subcommand, name = "list")]
/// List pipelines
struct PipelineListCommand {}

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let args: CLIArgs = argh::from_env();

    let client = reqwest::Client::new();

    // format the host and port
    let addr = format!("{}:{}", args.host, args.port);

    match args.commands {
        Commands::Stats(stats_command) => match stats_command.mode {
            StatsMode::Whoami(_) => {
                let response = client
                    .get(format!("http://{}/api/v0/stats/whoami", addr))
                    .send()
                    .await?;

                let result = response.json::<serde_json::Value>().await?;
                println!("Result: {}", serde_json::to_string_pretty(&result)?);
            }
            StatsMode::Sysinfo(_) => {
                let response = client
                    .get(format!("http://{}/api/v0/stats/sysinfo", addr))
                    .send()
                    .await?;

                let result = response.json::<serde_json::Value>().await?;
                println!("Result: {}", serde_json::to_string_pretty(&result)?);
            }
        },
        Commands::Recording(recording_command) => match recording_command.mode {
            RecordingMode::Start(_) => {
                let response = client
                    .post(format!("http://{}/api/v0/recording", addr))
                    .json(&bubbaloop::api::models::recording::RecordingQuery {
                        command: bubbaloop::api::models::recording::RecordingCommand::Start,
                    })
                    .send()
                    .await?;

                let result = response.json::<serde_json::Value>().await?;
                println!("Result: {}", serde_json::to_string_pretty(&result)?);
            }
            RecordingMode::Stop(_) => {
                let response = client
                    .post(format!("http://{}/api/v0/recording", addr))
                    .json(&bubbaloop::api::models::recording::RecordingQuery {
                        command: bubbaloop::api::models::recording::RecordingCommand::Stop,
                    })
                    .send()
                    .await?;

                let result = response.json::<serde_json::Value>().await?;
                println!("Result: {}", serde_json::to_string_pretty(&result)?);
            }
        },
        Commands::Pipeline(pipeline_command) => match pipeline_command.mode {
            PipelineMode::Start(pipeline_start_command) => {
                let response = client
                    .post(format!("http://{}/api/v0/pipeline/start", addr))
                    .json(&bubbaloop::api::models::pipeline::PipelineStartRequest {
                        name: pipeline_start_command.name,
                    })
                    .send()
                    .await?;

                let result = response.json::<serde_json::Value>().await?;
                println!("Result: {}", serde_json::to_string_pretty(&result)?);
            }
            PipelineMode::Stop(pipeline_stop_command) => {
                let response = client
                    .post(format!("http://{}/api/v0/pipeline/stop", addr))
                    .json(&bubbaloop::api::models::pipeline::PipelineStopRequest {
                        name: pipeline_stop_command.name,
                    })
                    .send()
                    .await?;

                let result = response.json::<serde_json::Value>().await?;
                println!("Result: {}", serde_json::to_string_pretty(&result)?);
            }
            PipelineMode::List(_pipeline_list_command) => {
                let response = client
                    .get(format!("http://{}/api/v0/pipeline/list", addr))
                    .send()
                    .await?;

                let result = response.json::<serde_json::Value>().await?;
                println!("Result: {}", serde_json::to_string_pretty(&result)?);
            }
        },
    }

    Ok(())
}
