use argh::FromArgs;
use bubbaloop::services::recording::{
    schemas::{StartRecordingRequest, StopRecordingRequest},
    StartRecording, StopRecording,
};
use ros_z::{context::ZContextBuilder, Builder, Result};
use serde_json::json;
use std::time::Duration;

#[derive(FromArgs)]
/// Bubbaloop CLI - control bubbaloop services
struct Args {
    #[argh(subcommand)]
    command: Command,
}

#[derive(FromArgs)]
#[argh(subcommand)]
enum Command {
    Record(RecordCmd),
    Topics(TopicsCmd),
}

#[derive(FromArgs)]
#[argh(subcommand, name = "topics")]
/// Topic discovery commands
struct TopicsCmd {
    #[argh(subcommand)]
    action: TopicsAction,
}

#[derive(FromArgs)]
#[argh(subcommand)]
enum TopicsAction {
    List(TopicsListCmd),
}

#[derive(FromArgs)]
#[argh(subcommand, name = "list")]
/// List available topics
struct TopicsListCmd {}

#[derive(FromArgs)]
#[argh(subcommand, name = "record")]
/// Control MCAP recording
struct RecordCmd {
    #[argh(subcommand)]
    action: RecordAction,
}

#[derive(FromArgs)]
#[argh(subcommand)]
enum RecordAction {
    Start(StartCmd),
    Stop(StopCmd),
}

#[derive(FromArgs)]
#[argh(subcommand, name = "start")]
/// Start recording topics to MCAP
struct StartCmd {
    /// topics to record (comma-separated or multiple arguments)
    #[argh(positional)]
    topics: Vec<String>,
}

#[derive(FromArgs)]
#[argh(subcommand, name = "stop")]
/// Stop recording
struct StopCmd {}

#[tokio::main]
async fn main() -> Result<()> {
    // Initialize logging
    let env = env_logger::Env::default().default_filter_or("info");
    env_logger::init_from_env(env);

    let args: Args = argh::from_env();

    // Connect to Zenoh
    let zenoh_endpoint =
        std::env::var("ZENOH_ENDPOINT").unwrap_or_else(|_| "tcp/127.0.0.1:7448".to_string());

    log::info!("Connecting to Zenoh at: {}", zenoh_endpoint);

    let ctx = ZContextBuilder::default()
        .with_json("connect/endpoints", json!([zenoh_endpoint]))
        .build()?;

    let node = ctx.create_node("bubbaloop_cli").build()?;

    match args.command {
        Command::Record(record_cmd) => match record_cmd.action {
            RecordAction::Start(cmd) => {
                let client = node
                    .create_client::<StartRecording>("recorder/start")
                    .build()?;

                // Support both comma-separated and space-separated topics
                let topics: Vec<String> = cmd
                    .topics
                    .iter()
                    .flat_map(|s| s.split(','))
                    .map(|s| s.trim().to_string())
                    .filter(|s| !s.is_empty())
                    .collect();

                if topics.is_empty() {
                    eprintln!("✗ Error: No topics specified.");
                    eprintln!("Usage: bubbaloop record start <topic1> [topic2] ...");
                    std::process::exit(1);
                }

                log::info!("Starting recording of {} topics...", topics.len());

                let request = StartRecordingRequest { topics };

                client.send_request(&request).await?;
                let response = match client.take_response_timeout(Duration::from_secs(5)) {
                    Ok(r) => r,
                    Err(_) => {
                        eprintln!("✗ Timeout: recorder service not responding");
                        eprintln!("  Make sure mcap_recorder is running");
                        std::process::exit(1);
                    }
                };

                if response.success {
                    println!("✓ Recording started: {}", response.file_path);
                    println!("  {}", response.message);
                } else {
                    eprintln!("✗ Failed: {}", response.message);
                    std::process::exit(1);
                }
            }
            RecordAction::Stop(_) => {
                let client = node
                    .create_client::<StopRecording>("recorder/stop")
                    .build()?;

                log::info!("Stopping recording...");

                let request = StopRecordingRequest {};

                client.send_request(&request).await?;
                let response = match client.take_response_timeout(Duration::from_secs(5)) {
                    Ok(r) => r,
                    Err(_) => {
                        eprintln!("✗ Timeout: recorder service not responding");
                        eprintln!("  Make sure mcap_recorder is running");
                        std::process::exit(1);
                    }
                };

                if response.success {
                    println!("✓ Recording stopped");
                    println!("  {} messages recorded", response.messages_recorded);
                    if !response.file_path.is_empty() {
                        println!("  File: {}", response.file_path);
                    }
                } else {
                    eprintln!("✗ Failed: {}", response.message);
                    std::process::exit(1);
                }
            }
        },
        Command::Topics(topics_cmd) => match topics_cmd.action {
            TopicsAction::List(_) => {
                // Wait a bit for topic discovery
                tokio::time::sleep(Duration::from_millis(500)).await;

                let topics = node.graph.get_topic_names_and_types();

                if topics.is_empty() {
                    println!("No topics found");
                } else {
                    println!("Topics ({}):", topics.len());
                    for (name, type_name) in topics {
                        println!("  {} [{}]", name, type_name);
                    }
                }
            }
        },
    }

    Ok(())
}
