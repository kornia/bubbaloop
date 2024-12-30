use argh::FromArgs;
use serde_json::json;

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
    Compute(ComputeCommand),
    Stats(StatsCommand),
}

#[derive(FromArgs)]
#[argh(subcommand, name = "compute")]
/// Compute mean and std of an image
struct ComputeCommand {
    #[argh(subcommand)]
    mode: ComputeMode,
}

#[derive(FromArgs)]
#[argh(subcommand)]
enum ComputeMode {
    MeanStd(ComputeMeanStdCommand),
}

/// Compute mean and std of an image
#[derive(FromArgs)]
#[argh(subcommand, name = "mean-std")]
struct ComputeMeanStdCommand {
    #[argh(option, short = 'i')]
    /// the images directory to compute the mean and std of
    images_dir: String,

    #[argh(option, short = 'n')]
    /// the number of threads to use
    num_threads: Option<usize>,
}

#[derive(FromArgs)]
#[argh(subcommand, name = "stats")]
/// Print the whoami
struct StatsCommand {
    #[argh(subcommand)]
    mode: StatsMode,
}

#[derive(FromArgs)]
#[argh(subcommand)]
enum StatsMode {
    Whoami(StatsWhoamiCommand),
}

#[derive(FromArgs)]
#[argh(subcommand, name = "whoami")]
/// Print the whoami
struct StatsWhoamiCommand {}

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let args: CLIArgs = argh::from_env();

    let client = reqwest::Client::new();

    // format the host and port
    let addr = format!("{}:{}", args.host, args.port);

    match args.commands {
        Commands::Compute(compute_command) => match compute_command.mode {
            ComputeMode::MeanStd(mean_std_command) => {
                let mut post_payload = json!({
                    "images_dir": mean_std_command.images_dir,
                });

                if let Some(num_threads) = mean_std_command.num_threads {
                    post_payload["num_threads"] = json!(num_threads);
                }

                let response = client
                    .post(format!("http://{}/api/v0/compute/mean_std", addr))
                    .json(&post_payload)
                    .send()
                    .await?;

                let result = response.json::<serde_json::Value>().await?;
                println!("Result: {:?}", result);
            }
        },
        Commands::Stats(stats_command) => match stats_command.mode {
            StatsMode::Whoami(_whoami_command) => {
                let response = client
                    .post(format!("http://{}/api/v0/stats/whoami", addr))
                    .send()
                    .await?;

                let result = response.json::<serde_json::Value>().await?;
                println!("Result: {:?}", result);
            }
        },
    }

    Ok(())
}