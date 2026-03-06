//! Thin Zenoh CLI client for agent communication.
//!
//! Publishes messages to the shared agent inbox and renders outbox events.
//! All LLM processing happens daemon-side — this is purely I/O.
//!
//! The daemon must be started separately (`bubbaloop daemon start`).

use crate::agent::gateway::{self, AgentEvent, AgentEventType, AgentManifest, AgentMessage};
use std::sync::Arc;
use std::time::Duration;
use zenoh::Session;

/// Timeout waiting for first response from daemon.
const RESPONSE_TIMEOUT: Duration = Duration::from_secs(10);

/// Check if the daemon is reachable by querying the daemon manifest.
pub async fn is_daemon_running(session: &Arc<Session>, scope: &str, machine_id: &str) -> bool {
    let pattern = crate::daemon::gateway::manifest_topic(scope, machine_id);
    match session
        .get(&pattern)
        .target(zenoh::query::QueryTarget::BestMatching)
        .timeout(Duration::from_secs(2))
        .await
    {
        Ok(replies) => replies.recv_async().await.is_ok(),
        Err(_) => false,
    }
}

/// Run a single message or interactive REPL via Zenoh.
pub async fn run_agent_client(
    session: Arc<Session>,
    scope: &str,
    machine_id: &str,
    agent: Option<&str>,
    message: Option<&str>,
) -> Result<(), Box<dyn std::error::Error>> {
    if let Some(msg) = message {
        // Single-message mode: send and wait for Done
        send_and_render(&session, scope, machine_id, agent, msg).await?;
    } else {
        // Interactive REPL mode
        run_repl(&session, scope, machine_id, agent).await?;
    }
    Ok(())
}

/// Send a single message and render the streamed response.
async fn send_and_render(
    session: &Arc<Session>,
    scope: &str,
    machine_id: &str,
    agent: Option<&str>,
    text: &str,
) -> Result<(), Box<dyn std::error::Error>> {
    let correlation_id = uuid::Uuid::new_v4().to_string();

    // Subscribe to outbox BEFORE publishing (avoid missing early events)
    let outbox_pattern = gateway::outbox_wildcard(scope, machine_id);
    let subscriber = session
        .declare_subscriber(&outbox_pattern)
        .await
        .map_err(|e| format!("Failed to subscribe to outbox: {}", e))?;

    // Publish to inbox
    let inbox = gateway::inbox_topic(scope, machine_id);
    let msg = AgentMessage {
        id: correlation_id.clone(),
        text: text.to_string(),
        agent: agent.map(|s| s.to_string()),
    };
    let payload = serde_json::to_vec(&msg)?;
    session
        .put(&inbox, payload)
        .await
        .map_err(|e| format!("Failed to publish to inbox: {}", e))?;

    // Render outbox events, filtering by correlation ID
    let mut got_first = false;
    let mut shown_agent_header = false;
    loop {
        let timeout = if got_first {
            Duration::from_secs(120) // generous timeout during streaming
        } else {
            RESPONSE_TIMEOUT
        };

        tokio::select! {
            result = subscriber.recv_async() => {
                match result {
                    Ok(sample) => {
                        // Extract agent_id from topic key expression:
                        // bubbaloop/{scope}/{machine}/agent/{agent_id}/outbox
                        let agent_id_from_topic = sample.key_expr().as_str()
                            .split('/')
                            .nth(4)
                            .unwrap_or("agent");

                        let bytes = sample.payload().to_bytes();
                        if let Ok(event) = serde_json::from_slice::<AgentEvent>(&bytes) {
                            if event.id != correlation_id {
                                continue; // Not our message
                            }
                            got_first = true;
                            match event.event_type {
                                AgentEventType::Delta => {
                                    if !shown_agent_header {
                                        println!("\n[{}]", agent_id_from_topic);
                                        shown_agent_header = true;
                                    }
                                    if let Some(text) = &event.text {
                                        print!("{}", text);
                                        std::io::Write::flush(&mut std::io::stdout()).ok();
                                    }
                                }
                                AgentEventType::Tool => {
                                    if let Some(name) = &event.text {
                                        println!("  [calling {}...]", name);
                                    }
                                }
                                AgentEventType::ToolResult => {
                                    // Silently consumed
                                }
                                AgentEventType::Error => {
                                    if let Some(msg) = &event.text {
                                        eprintln!("Error: {}", msg);
                                    }
                                }
                                AgentEventType::Done => {
                                    println!();
                                    return Ok(());
                                }
                            }
                        }
                    }
                    Err(e) => {
                        return Err(format!("Outbox subscription error: {}", e).into());
                    }
                }
            }
            _ = tokio::time::sleep(timeout) => {
                if !got_first {
                    eprintln!("Error: No response from daemon within {}s. Is `bubbaloop up` running?", RESPONSE_TIMEOUT.as_secs());
                } else {
                    eprintln!("Error: Response timed out during streaming.");
                }
                return Err("timeout".into());
            }
        }
    }
}

/// Interactive REPL: read stdin, send via Zenoh, render responses.
async fn run_repl(
    session: &Arc<Session>,
    scope: &str,
    machine_id: &str,
    agent: Option<&str>,
) -> Result<(), Box<dyn std::error::Error>> {
    println!();
    println!("  bubbaloop agent ({})", env!("CARGO_PKG_VERSION"));
    if let Some(a) = agent {
        println!("  Target agent: {}", a);
    }
    println!("  Type a message to chat, 'quit' to exit.");
    println!();

    let stdin = std::io::stdin();
    let (repl_tx, mut repl_rx) = tokio::sync::mpsc::channel::<String>(1);

    std::thread::spawn(move || loop {
        print!("> ");
        use std::io::Write;
        std::io::stdout().flush().ok();

        let mut line = String::new();
        match stdin.read_line(&mut line) {
            Ok(0) => break,
            Ok(_) => {
                let trimmed = line.trim().to_string();
                if trimmed == "quit" || trimmed == "exit" {
                    let _ = repl_tx.blocking_send(trimmed);
                    break;
                }
                if !trimmed.is_empty() {
                    let _ = repl_tx.blocking_send(trimmed);
                }
            }
            Err(_) => break,
        }
    });

    while let Some(input) = repl_rx.recv().await {
        if input == "quit" || input == "exit" {
            break;
        }
        if let Err(e) = send_and_render(session, scope, machine_id, agent, &input).await {
            let err_str = e.to_string();
            if err_str != "timeout" {
                eprintln!("Error: {}", e);
            }
        }
    }

    println!("Goodbye.");
    Ok(())
}

/// Query all agent manifests and print a table.
pub async fn run_agent_list(
    session: Arc<Session>,
    scope: &str,
    machine_id: &str,
    all_machines: bool,
) -> Result<(), Box<dyn std::error::Error>> {
    let pattern = if all_machines {
        gateway::manifest_wildcard_all(scope)
    } else {
        gateway::manifest_wildcard(scope, machine_id)
    };

    let replies = session
        .get(&pattern)
        .timeout(Duration::from_secs(3))
        .await
        .map_err(|e| format!("Manifest query failed: {}", e))?;

    let mut manifests = Vec::new();
    while let Ok(reply) = replies.recv_async().await {
        if let Ok(sample) = reply.into_result() {
            let bytes = sample.payload().to_bytes();
            if let Ok(manifest) = serde_json::from_slice::<AgentManifest>(&bytes) {
                manifests.push(manifest);
            }
        }
    }

    if manifests.is_empty() {
        if all_machines {
            println!("No agents found on any machine.");
        } else {
            println!("No agents found. Is `bubbaloop up` running?");
        }
        return Ok(());
    }

    if all_machines {
        println!(
            "{:<20} {:<20} {:<25} {:<10} {:<30} CAPABILITIES",
            "ID", "MACHINE", "NAME", "DEFAULT", "MODEL",
        );
        println!("{}", "-".repeat(115));
        for m in &manifests {
            println!(
                "{:<20} {:<20} {:<25} {:<10} {:<30} {}",
                m.agent_id,
                m.machine_id,
                m.name,
                if m.is_default { "yes" } else { "" },
                m.model,
                m.capabilities.join(", "),
            );
        }
    } else {
        println!(
            "{:<20} {:<25} {:<10} {:<30} CAPABILITIES",
            "ID", "NAME", "DEFAULT", "MODEL",
        );
        println!("{}", "-".repeat(95));
        for m in &manifests {
            println!(
                "{:<20} {:<25} {:<10} {:<30} {}",
                m.agent_id,
                m.name,
                if m.is_default { "yes" } else { "" },
                m.model,
                m.capabilities.join(", "),
            );
        }
    }

    Ok(())
}
