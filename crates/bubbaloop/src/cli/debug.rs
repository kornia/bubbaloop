//! Debug CLI commands for Zenoh network inspection
//!
//! Provides tools for debugging and monitoring Zenoh communication:
//! - List active topics (publishers/subscribers)
//! - Subscribe to topics and watch messages
//! - Query queryables
//! - Show connection info

use argh::FromArgs;
use serde_json::json;
use std::time::Duration;
use thiserror::Error;
use zenoh::query::QueryTarget;

#[derive(Debug, Error)]
pub enum DebugError {
    #[error("Zenoh error: {0}")]
    Zenoh(String),
    #[error("JSON error: {0}")]
    Json(#[from] serde_json::Error),
    #[error("Timeout waiting for response")]
    Timeout,
}

pub type Result<T> = std::result::Result<T, DebugError>;

/// Debug commands for Zenoh network inspection
#[derive(FromArgs)]
#[argh(subcommand, name = "debug")]
pub struct DebugCommand {
    #[argh(subcommand)]
    action: Option<DebugAction>,
}

#[derive(FromArgs)]
#[argh(subcommand)]
enum DebugAction {
    Topics(TopicsArgs),
    Subscribe(SubscribeArgs),
    Query(QueryArgs),
    Info(InfoArgs),
}

/// List all active Zenoh topics
#[derive(FromArgs)]
#[argh(subcommand, name = "topics")]
struct TopicsArgs {
    /// output format: table, json (default: table)
    #[argh(option, short = 'f', default = "String::from(\"table\")")]
    format: String,

    /// timeout in seconds (default: 5)
    #[argh(option, short = 't', default = "5")]
    timeout: u64,
}

/// Subscribe to a Zenoh topic and watch messages
#[derive(FromArgs)]
#[argh(subcommand, name = "subscribe")]
struct SubscribeArgs {
    /// topic key expression (e.g., "bubbaloop/daemon/**")
    #[argh(positional)]
    topic: String,

    /// decode protobuf messages
    #[argh(switch)]
    decode: bool,

    /// output as JSON
    #[argh(switch)]
    json: bool,
}

/// Query a Zenoh queryable endpoint
#[derive(FromArgs)]
#[argh(subcommand, name = "query")]
struct QueryArgs {
    /// query key expression (e.g., "bubbaloop/daemon/api/health")
    #[argh(positional)]
    key: String,

    /// timeout in seconds (default: 3)
    #[argh(option, short = 't', default = "3")]
    timeout: u64,

    /// output as JSON
    #[argh(switch)]
    json: bool,

    /// query payload (optional)
    #[argh(option, short = 'p')]
    payload: Option<String>,
}

/// Show Zenoh connection information
#[derive(FromArgs)]
#[argh(subcommand, name = "info")]
struct InfoArgs {
    /// output as JSON
    #[argh(switch)]
    json: bool,
}

impl DebugCommand {
    pub async fn run(self) -> Result<()> {
        match self.action {
            None => {
                Self::print_help();
                Ok(())
            }
            Some(DebugAction::Topics(args)) => list_topics(args).await,
            Some(DebugAction::Subscribe(args)) => subscribe_topic(args).await,
            Some(DebugAction::Query(args)) => query_endpoint(args).await,
            Some(DebugAction::Info(args)) => show_info(args).await,
        }
    }

    fn print_help() {
        eprintln!("Debug commands for Zenoh network inspection\n");
        eprintln!("Usage: bubbaloop debug <command>\n");
        eprintln!("Commands:");
        eprintln!("  info        Show Zenoh connection information");
        eprintln!("  topics      List all active Zenoh topics");
        eprintln!("  query       Query a Zenoh queryable endpoint");
        eprintln!("  subscribe   Subscribe to a Zenoh topic and watch messages");
        eprintln!("\nRun 'bubbaloop debug <command> --help' for more information.");
    }
}

async fn get_zenoh_session() -> Result<zenoh::Session> {
    let mut config = zenoh::Config::default();

    // Run as client mode
    config
        .insert_json5("mode", "\"client\"")
        .map_err(|e| DebugError::Zenoh(e.to_string()))?;

    let endpoint = std::env::var("BUBBALOOP_ZENOH_ENDPOINT")
        .unwrap_or_else(|_| "tcp/127.0.0.1:7447".to_string());
    config
        .insert_json5("connect/endpoints", &format!("[\"{}\"]", endpoint))
        .map_err(|e| DebugError::Zenoh(e.to_string()))?;

    // Disable scouting
    config
        .insert_json5("scouting/multicast/enabled", "false")
        .map_err(|e| DebugError::Zenoh(e.to_string()))?;
    config
        .insert_json5("scouting/gossip/enabled", "false")
        .map_err(|e| DebugError::Zenoh(e.to_string()))?;

    let session = zenoh::open(config)
        .await
        .map_err(|e| DebugError::Zenoh(e.to_string()))?;
    Ok(session)
}

async fn list_topics(args: TopicsArgs) -> Result<()> {
    let session = get_zenoh_session().await?;

    println!("Scanning Zenoh network for topics (timeout: {}s)...", args.timeout);

    // Query the admin space for topic information
    // Note: This requires Zenoh admin API which may not be available in all deployments
    // Fallback to scanning known bubbaloop topics
    let known_topics = vec![
        "bubbaloop/daemon/**",
        "bubbaloop/nodes/**",
        "/camera/**",
        "/weather/**",
    ];

    let mut found_topics = std::collections::HashSet::new();

    for pattern in &known_topics {
        let replies: Vec<_> = session
            .get(*pattern)
            .target(QueryTarget::All)
            .timeout(Duration::from_secs(args.timeout))
            .await
            .map_err(|e| DebugError::Zenoh(e.to_string()))?
            .into_iter()
            .collect();

        for reply in replies {
            if let Ok(sample) = reply.into_result() {
                found_topics.insert(sample.key_expr().to_string());
            }
        }
    }

    if args.format == "json" {
        let topics: Vec<_> = found_topics.iter().collect();
        println!("{}", serde_json::to_string_pretty(&json!({
            "topics": topics,
            "count": topics.len()
        }))?);
    } else if found_topics.is_empty() {
        println!("No topics found. The network may be empty or daemon not running.");
    } else {
        println!("\nActive Topics ({}):", found_topics.len());
        println!("{}", "-".repeat(80));
        let mut sorted: Vec<_> = found_topics.iter().collect();
        sorted.sort();
        for topic in sorted {
            println!("  {}", topic);
        }
    }

    session
        .close()
        .await
        .map_err(|e| DebugError::Zenoh(e.to_string()))?;
    Ok(())
}

async fn subscribe_topic(args: SubscribeArgs) -> Result<()> {
    let session = get_zenoh_session().await?;

    println!("Subscribing to: {}", args.topic);
    println!("Press Ctrl+C to stop...\n");

    let subscriber = session
        .declare_subscriber(&args.topic)
        .await
        .map_err(|e| DebugError::Zenoh(e.to_string()))?;

    loop {
        let sample = subscriber
            .recv_async()
            .await
            .map_err(|e| DebugError::Zenoh(e.to_string()))?;

        let key = sample.key_expr().to_string();
        let payload = sample.payload().to_bytes();
        let timestamp = sample.timestamp().map(|t| t.to_string()).unwrap_or_else(|| "N/A".to_string());

        if args.json {
            // Try to parse as JSON
            if let Ok(json_val) = serde_json::from_slice::<serde_json::Value>(&payload) {
                println!("{}", serde_json::to_string_pretty(&json!({
                    "key": key,
                    "timestamp": timestamp,
                    "payload": json_val
                }))?);
            } else {
                println!("{}", serde_json::to_string_pretty(&json!({
                    "key": key,
                    "timestamp": timestamp,
                    "payload": String::from_utf8_lossy(&payload)
                }))?);
            }
        } else {
            println!("[{}] {}", timestamp, key);

            if args.decode {
                // Try to decode as protobuf (placeholder - would need schema)
                println!("  Protobuf decoding not yet implemented");
                println!("  Raw bytes: {} bytes", payload.len());
            }

            // Try to display as string or JSON
            if let Ok(s) = std::str::from_utf8(&payload) {
                if let Ok(json_val) = serde_json::from_str::<serde_json::Value>(s) {
                    println!("  {}", serde_json::to_string_pretty(&json_val)?);
                } else {
                    println!("  {}", s);
                }
            } else {
                println!("  Binary data: {} bytes", payload.len());
                println!("  Hex: {}", hex::encode(&payload[..payload.len().min(64)]));
                if payload.len() > 64 {
                    println!("  ... ({} more bytes)", payload.len() - 64);
                }
            }
            println!();
        }
    }
}

async fn query_endpoint(args: QueryArgs) -> Result<()> {
    let session = get_zenoh_session().await?;

    if !args.json {
        println!("Querying: {}", args.key);
        println!("Timeout: {}s\n", args.timeout);
    }

    let mut get_builder = session
        .get(&args.key)
        .target(QueryTarget::All)
        .timeout(Duration::from_secs(args.timeout));

    if let Some(ref payload) = args.payload {
        get_builder = get_builder.payload(payload.clone());
    }

    let replies: Vec<_> = get_builder
        .await
        .map_err(|e| DebugError::Zenoh(e.to_string()))?
        .into_iter()
        .collect();

    if replies.is_empty() {
        if args.json {
            println!("{}", serde_json::to_string_pretty(&json!({
                "error": "No replies received",
                "key": args.key
            }))?);
        } else {
            println!("No replies received. Endpoint may not exist or is not responding.");
        }
        return Err(DebugError::Timeout);
    }

    for (i, reply) in replies.iter().enumerate() {
        match reply.result() {
            Ok(sample) => {
                let payload = sample.payload().to_bytes();

                if args.json {
                    // Try to parse as JSON
                    if let Ok(json_val) = serde_json::from_slice::<serde_json::Value>(&payload) {
                        println!("{}", serde_json::to_string_pretty(&json!({
                            "reply": i + 1,
                            "key": sample.key_expr().to_string(),
                            "payload": json_val
                        }))?);
                    } else {
                        println!("{}", serde_json::to_string_pretty(&json!({
                            "reply": i + 1,
                            "key": sample.key_expr().to_string(),
                            "payload": String::from_utf8_lossy(&payload)
                        }))?);
                    }
                } else {
                    println!("Reply {} from: {}", i + 1, sample.key_expr());

                    if let Ok(s) = std::str::from_utf8(&payload) {
                        if let Ok(json_val) = serde_json::from_str::<serde_json::Value>(s) {
                            println!("{}", serde_json::to_string_pretty(&json_val)?);
                        } else {
                            println!("{}", s);
                        }
                    } else {
                        println!("Binary data: {} bytes", payload.len());
                        println!("Hex: {}", hex::encode(&payload[..payload.len().min(64)]));
                    }

                    if i < replies.len() - 1 {
                        println!("\n{}", "-".repeat(80));
                        println!();
                    }
                }
            }
            Err(e) => {
                if args.json {
                    println!("{}", serde_json::to_string_pretty(&json!({
                        "reply": i + 1,
                        "error": e.to_string()
                    }))?);
                } else {
                    println!("Reply {} ERROR: {}", i + 1, e);
                }
            }
        }
    }

    session
        .close()
        .await
        .map_err(|e| DebugError::Zenoh(e.to_string()))?;
    Ok(())
}

async fn show_info(args: InfoArgs) -> Result<()> {
    let session = get_zenoh_session().await?;

    let endpoint = std::env::var("BUBBALOOP_ZENOH_ENDPOINT")
        .unwrap_or_else(|_| "tcp/127.0.0.1:7447".to_string());

    let session_id = session.zid().to_string();

    if args.json {
        println!("{}", serde_json::to_string_pretty(&json!({
            "endpoint": endpoint,
            "session_id": session_id,
            "mode": "client",
            "status": "connected"
        }))?);
    } else {
        println!("Zenoh Connection Info");
        println!("{}", "=".repeat(80));
        println!("Endpoint:     {}", endpoint);
        println!("Session ID:   {}", session_id);
        println!("Mode:         client");
        println!("Status:       connected");
        println!("\nEnvironment:");
        println!("  BUBBALOOP_ZENOH_ENDPOINT: {}",
            std::env::var("BUBBALOOP_ZENOH_ENDPOINT").unwrap_or_else(|_| "(not set, using default)".to_string()));
        println!("  RUST_LOG: {}",
            std::env::var("RUST_LOG").unwrap_or_else(|_| "(not set)".to_string()));
    }

    session
        .close()
        .await
        .map_err(|e| DebugError::Zenoh(e.to_string()))?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_debug_error_display() {
        let err = DebugError::Timeout;
        assert_eq!(err.to_string(), "Timeout waiting for response");

        let err = DebugError::Zenoh("connection failed".to_string());
        assert_eq!(err.to_string(), "Zenoh error: connection failed");
    }
}
