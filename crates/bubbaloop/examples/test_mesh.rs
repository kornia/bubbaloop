//! Zenoh mesh connectivity test
//!
//! This example verifies that pub/sub and queryables work across the Zenoh mesh.
//!
//! Usage:
//!   # Run as publisher (on Jetson 1)
//!   cargo run --example test_mesh -- pub
//!
//!   # Run as subscriber (on Jetson 2)
//!   cargo run --example test_mesh -- sub
//!
//!   # Run as queryable service (simulates a Jetson service)
//!   cargo run --example test_mesh -- service
//!
//!   # Run as query client (calls a remote service)
//!   cargo run --example test_mesh -- query

use anyhow::{anyhow, Result};
use std::time::Duration;
use zenoh::Config;

const TEST_TOPIC: &str = "bubbaloop/test/mesh";
const QUERY_KEY: &str = "bubbaloop/test/query";

// Helper to convert Zenoh errors
fn zerr<T>(r: std::result::Result<T, Box<dyn std::error::Error + Send + Sync>>) -> Result<T> {
    r.map_err(|e| anyhow!("{}", e))
}

#[tokio::main]
async fn main() -> Result<()> {
    env_logger::init();

    let args: Vec<String> = std::env::args().collect();
    let mode = args.get(1).map(|s| s.as_str()).unwrap_or("help");

    // Get endpoint from environment or use default
    let endpoint = std::env::var("BUBBALOOP_ZENOH_ENDPOINT")
        .unwrap_or_else(|_| "tcp/127.0.0.1:7447".to_string());

    println!("Connecting to Zenoh at: {}", endpoint);

    // Create Zenoh session
    let mut config = Config::default();
    config.insert_json5("mode", "\"peer\"").ok();
    config
        .insert_json5("connect/endpoints", &format!("[\"{}\"]", endpoint))
        .ok();
    config
        .insert_json5("scouting/multicast/enabled", "false")
        .ok();
    config.insert_json5("scouting/gossip/enabled", "false").ok();

    let session = zerr(zenoh::open(config).await)?;
    println!("Connected! Session ID: {}", session.zid());

    match mode {
        "pub" | "publisher" => run_publisher(&session).await?,
        "sub" | "subscriber" => run_subscriber(&session).await?,
        "service" | "queryable" => run_queryable(&session).await?,
        "query" | "client" => run_query_client(&session).await?,
        "selftest" => run_selftest(&session).await?,
        _ => {
            println!("Zenoh Mesh Connectivity Test");
            println!("============================");
            println!();
            println!("Usage: cargo run --example test_mesh -- <mode>");
            println!();
            println!("Modes:");
            println!("  pub       - Publish test messages every 2 seconds");
            println!("  sub       - Subscribe and print received messages");
            println!("  service   - Run as queryable service (responds to queries)");
            println!("  query     - Send a query to a remote service");
            println!("  selftest  - Run local pub/sub and query test");
            println!();
            println!("Environment:");
            println!(
                "  BUBBALOOP_ZENOH_ENDPOINT - Zenoh router endpoint (default: tcp/127.0.0.1:7447)"
            );
            println!();
            println!("Cross-Jetson test:");
            println!("  Jetson 1: cargo run --example test_mesh -- pub");
            println!("  Jetson 2: cargo run --example test_mesh -- sub");
            println!();
            println!("Both should see messages from each other if the mesh is working.");
        }
    }

    zerr(session.close().await)?;
    Ok(())
}

async fn run_publisher(session: &zenoh::Session) -> Result<()> {
    println!("Publishing to: {}", TEST_TOPIC);
    println!("Press Ctrl+C to stop\n");

    let hostname = hostname::get()
        .map(|h| h.to_string_lossy().to_string())
        .unwrap_or_else(|_| "unknown".to_string());

    let mut i = 0u64;
    loop {
        let msg = format!(
            "{{\"seq\":{},\"from\":\"{}\",\"ts\":{}}}",
            i,
            hostname,
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_millis()
        );

        zerr(session.put(TEST_TOPIC, msg.clone()).await)?;
        println!("[PUB] {}", msg);

        i += 1;
        tokio::time::sleep(Duration::from_secs(2)).await;
    }
}

async fn run_subscriber(session: &zenoh::Session) -> Result<()> {
    println!("Subscribing to: {}", TEST_TOPIC);
    println!("Waiting for messages from other Jetsons...\n");

    let subscriber = zerr(session.declare_subscriber(TEST_TOPIC).await)?;

    loop {
        match subscriber.recv_async().await {
            Ok(sample) => {
                let payload = sample.payload().to_bytes();
                let msg = String::from_utf8_lossy(&payload);
                println!("[SUB] Received: {}", msg);
            }
            Err(e) => {
                eprintln!("[SUB] Error: {}", e);
            }
        }
    }
}

async fn run_queryable(session: &zenoh::Session) -> Result<()> {
    println!("Queryable service on: {}", QUERY_KEY);
    println!("Waiting for queries...\n");

    let hostname = hostname::get()
        .map(|h| h.to_string_lossy().to_string())
        .unwrap_or_else(|_| "unknown".to_string());

    let queryable = zerr(session.declare_queryable(QUERY_KEY).await)?;

    loop {
        match queryable.recv_async().await {
            Ok(query) => {
                println!("[QUERY] Received query: {}", query.key_expr());

                let response = format!(
                    "{{\"from\":\"{}\",\"status\":\"ok\",\"ts\":{}}}",
                    hostname,
                    std::time::SystemTime::now()
                        .duration_since(std::time::UNIX_EPOCH)
                        .unwrap()
                        .as_millis()
                );

                let _ = zerr(query.reply(query.key_expr(), response.clone()).await);
                println!("[QUERY] Replied: {}", response);
            }
            Err(e) => {
                eprintln!("[QUERY] Error: {}", e);
            }
        }
    }
}

async fn run_query_client(session: &zenoh::Session) -> Result<()> {
    println!("Querying: {}", QUERY_KEY);

    let replies = zerr(session.get(QUERY_KEY).timeout(Duration::from_secs(5)).await)?;

    let mut got_reply = false;
    while let Ok(reply) = replies.recv_async().await {
        match reply.result() {
            Ok(sample) => {
                let payload = sample.payload().to_bytes();
                let msg = String::from_utf8_lossy(&payload);
                println!("[REPLY] From {}: {}", sample.key_expr(), msg);
                got_reply = true;
            }
            Err(e) => {
                eprintln!("[REPLY] Error: {:?}", e);
            }
        }
    }

    if !got_reply {
        println!("[REPLY] No reply received. Is the service running?");
        println!("        Start it with: cargo run --example test_mesh -- service");
    }

    Ok(())
}

async fn run_selftest(session: &zenoh::Session) -> Result<()> {
    println!("Running self-test...\n");

    // Test 1: Pub/Sub
    println!("Test 1: Local Pub/Sub");
    let subscriber = zerr(session.declare_subscriber(TEST_TOPIC).await)?;

    let test_msg = format!("selftest-{}", std::process::id());
    zerr(session.put(TEST_TOPIC, test_msg.clone()).await)?;
    println!("  Published: {}", test_msg);

    tokio::select! {
        result = subscriber.recv_async() => {
            match result {
                Ok(sample) => {
                    let payload = sample.payload().to_bytes();
                    let msg = String::from_utf8_lossy(&payload);
                    if msg.contains(&test_msg) {
                        println!("  Received: {}", msg);
                        println!("  [OK] Pub/Sub works!");
                    } else {
                        println!("  [WARN] Received different message: {}", msg);
                    }
                }
                Err(e) => println!("  [FAIL] Error: {}", e),
            }
        }
        _ = tokio::time::sleep(Duration::from_secs(2)) => {
            println!("  [FAIL] Timeout waiting for message");
        }
    }
    drop(subscriber);

    println!();

    // Test 2: Queryable
    println!("Test 2: Local Queryable");

    let queryable = zerr(session.declare_queryable(QUERY_KEY).await)?;

    // Spawn handler
    let session_clone = session.clone();
    let handle = tokio::spawn(async move {
        if let Ok(query) = queryable.recv_async().await {
            let _ = query.reply(query.key_expr(), "selftest-response").await;
        }
    });

    tokio::time::sleep(Duration::from_millis(100)).await;

    let replies = zerr(
        session_clone
            .get(QUERY_KEY)
            .timeout(Duration::from_secs(2))
            .await,
    )?;

    let mut got_reply = false;
    while let Ok(reply) = replies.recv_async().await {
        if let Ok(sample) = reply.result() {
            let payload = sample.payload().to_bytes();
            let msg = String::from_utf8_lossy(&payload);
            println!("  Query response: {}", msg);
            got_reply = true;
        }
    }

    handle.abort();

    if got_reply {
        println!("  [OK] Queryable works!");
    } else {
        println!("  [FAIL] No query response");
    }

    println!();
    println!("============================");
    println!("Self-test complete!");
    println!();
    println!("To test cross-Jetson communication:");
    println!("  Jetson 1: cargo run --example test_mesh -- pub");
    println!("  Jetson 2: cargo run --example test_mesh -- sub");

    Ok(())
}
