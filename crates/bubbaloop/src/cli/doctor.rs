//! Doctor command for system diagnostics
//!
//! Performs comprehensive health checks on all bubbaloop components:
//! - System services (zenohd, daemon, bridge)
//! - Zenoh connectivity
//! - Daemon API endpoints
//! - Node subscriptions
//!
//! Provides actionable fixes for each issue found.

use anyhow::{anyhow, Result};
use serde::Deserialize;
use std::time::Duration;
use tokio::process::Command;
use zenoh::query::QueryTarget;
use zenoh::Session;

const TIMEOUT_SECS: u64 = 5;

#[derive(Debug)]
struct DiagnosticResult {
    check: String,
    passed: bool,
    message: String,
    fix: Option<String>,
}

impl DiagnosticResult {
    fn pass(check: &str, message: &str) -> Self {
        Self {
            check: check.to_string(),
            passed: true,
            message: message.to_string(),
            fix: None,
        }
    }

    fn fail(check: &str, message: &str, fix: &str) -> Self {
        Self {
            check: check.to_string(),
            passed: false,
            message: message.to_string(),
            fix: Some(fix.to_string()),
        }
    }
}

#[derive(Deserialize)]
struct HealthResponse {
    status: String,
}

#[derive(Deserialize)]
struct NodeListResponse {
    nodes: Vec<NodeState>,
    #[allow(dead_code)]
    timestamp_ms: u64,
}

#[derive(Deserialize)]
struct NodeState {
    #[allow(dead_code)]
    name: String,
    #[allow(dead_code)]
    path: String,
    #[allow(dead_code)]
    status: String,
    #[allow(dead_code)]
    installed: bool,
    #[allow(dead_code)]
    autostart_enabled: bool,
    #[allow(dead_code)]
    version: String,
    #[allow(dead_code)]
    description: String,
    #[allow(dead_code)]
    node_type: String,
    #[allow(dead_code)]
    is_built: bool,
    #[allow(dead_code)]
    build_output: Vec<String>,
}

pub async fn run() -> Result<()> {
    println!("bubbaloop doctor");
    println!("================");
    println!();

    let mut results = Vec::new();

    // 1. Check system services
    println!("[1/4] Checking system services...");
    results.extend(check_system_services().await);
    println!();

    // 2. Check Zenoh connectivity
    println!("[2/4] Checking Zenoh connectivity...");
    let session = match check_zenoh_connection().await {
        Ok((result, session)) => {
            results.push(result);
            Some(session)
        }
        Err(result) => {
            results.push(result);
            None
        }
    };
    println!();

    // 3. Check daemon health (requires Zenoh connection)
    println!("[3/4] Checking daemon health...");
    if let Some(ref session) = session {
        results.extend(check_daemon_health(session).await);
    } else {
        results.push(DiagnosticResult::fail(
            "Daemon health",
            "Skipped (no Zenoh connection)",
            "Fix Zenoh connection first",
        ));
    }
    println!();

    // 4. Check node subscriptions (requires Zenoh connection)
    println!("[4/4] Checking node subscriptions...");
    if let Some(ref session) = session {
        results.extend(check_node_subscriptions(session).await);
    } else {
        results.push(DiagnosticResult::fail(
            "Node subscriptions",
            "Skipped (no Zenoh connection)",
            "Fix Zenoh connection first",
        ));
    }
    println!();

    // Print results
    println!("Summary");
    println!("=======");
    println!();

    let mut issues_found = 0;

    for result in &results {
        let symbol = if result.passed { "✓" } else { "✗" };
        println!("[{}] {}: {}", symbol, result.check, result.message);

        if !result.passed {
            issues_found += 1;
            if let Some(fix) = &result.fix {
                println!("    → Fix: {}", fix);
            }
        }
    }

    println!();
    if issues_found == 0 {
        println!("All checks passed!");
    } else {
        println!(
            "Found {} issue{}",
            issues_found,
            if issues_found == 1 { "" } else { "s" }
        );
    }

    // Close Zenoh session if open
    if let Some(session) = session {
        let _ = session.close().await;
    }

    Ok(())
}

async fn check_system_services() -> Vec<DiagnosticResult> {
    let mut results = Vec::new();

    // Check zenohd
    let zenohd_running = is_process_running("zenohd").await;
    if zenohd_running {
        // Additional check: is port 7447 listening?
        let port_check = check_port(7447).await;
        if port_check {
            results.push(DiagnosticResult::pass(
                "zenohd",
                "running on port 7447",
            ));
        } else {
            results.push(DiagnosticResult::fail(
                "zenohd",
                "running but port 7447 not accessible",
                "Check if zenohd is configured to listen on tcp/127.0.0.1:7447",
            ));
        }
    } else {
        results.push(DiagnosticResult::fail(
            "zenohd",
            "not running",
            "Run: zenohd &",
        ));
    }

    // Check bubbaloop-daemon
    let daemon_service = check_systemd_service("bubbaloop-daemon.service").await;
    if daemon_service == "active" {
        results.push(DiagnosticResult::pass(
            "bubbaloop-daemon",
            "service active",
        ));
    } else if daemon_service == "inactive" {
        results.push(DiagnosticResult::fail(
            "bubbaloop-daemon",
            "service inactive",
            "Run: systemctl --user start bubbaloop-daemon",
        ));
    } else if daemon_service == "failed" {
        results.push(DiagnosticResult::fail(
            "bubbaloop-daemon",
            "service failed",
            "Run: systemctl --user restart bubbaloop-daemon && journalctl --user -u bubbaloop-daemon -f",
        ));
    } else {
        results.push(DiagnosticResult::fail(
            "bubbaloop-daemon",
            &format!("service {}", daemon_service),
            "Run: systemctl --user status bubbaloop-daemon",
        ));
    }

    // Check zenoh-bridge (optional, so we don't fail hard)
    let bridge_service = check_systemd_service("zenoh-bridge.service").await;
    if bridge_service == "active" {
        results.push(DiagnosticResult::pass(
            "zenoh-bridge",
            "service active",
        ));
    } else {
        results.push(DiagnosticResult::fail(
            "zenoh-bridge",
            "not running (optional for CLI, required for dashboard)",
            "Run: systemctl --user start zenoh-bridge",
        ));
    }

    results
}

async fn is_process_running(name: &str) -> bool {
    let output = Command::new("pgrep")
        .arg("-x")
        .arg(name)
        .output()
        .await;

    matches!(output, Ok(out) if out.status.success())
}

async fn check_port(port: u16) -> bool {
    // Try to connect to the port
    tokio::net::TcpStream::connect(format!("127.0.0.1:{}", port))
        .await
        .is_ok()
}

async fn check_systemd_service(service_name: &str) -> String {
    let output = Command::new("systemctl")
        .args(["--user", "is-active", service_name])
        .output()
        .await;

    match output {
        Ok(out) => String::from_utf8_lossy(&out.stdout).trim().to_string(),
        Err(_) => "unknown".to_string(),
    }
}

async fn check_zenoh_connection() -> std::result::Result<(DiagnosticResult, Session), DiagnosticResult> {
    let mut config = zenoh::Config::default();

    // Run as client mode
    if config.insert_json5("mode", "\"client\"").is_err() {
        return Err(DiagnosticResult::fail(
            "Zenoh connection",
            "failed to configure client mode",
            "Check Zenoh installation",
        ));
    }

    let endpoint = std::env::var("BUBBALOOP_ZENOH_ENDPOINT")
        .unwrap_or_else(|_| "tcp/127.0.0.1:7447".to_string());

    if config
        .insert_json5("connect/endpoints", &format!("[\"{}\"]", endpoint))
        .is_err()
    {
        return Err(DiagnosticResult::fail(
            "Zenoh connection",
            "failed to configure endpoint",
            "Check BUBBALOOP_ZENOH_ENDPOINT environment variable",
        ));
    }

    // Disable scouting
    let _ = config.insert_json5("scouting/multicast/enabled", "false");
    let _ = config.insert_json5("scouting/gossip/enabled", "false");

    match zenoh::open(config).await {
        Ok(session) => Ok((
            DiagnosticResult::pass("Zenoh connection", &format!("connected to {}", endpoint)),
            session,
        )),
        Err(e) => Err(DiagnosticResult::fail(
            "Zenoh connection",
            &format!("failed to connect: {}", e),
            "Ensure zenohd is running on port 7447. Check with: pgrep zenohd",
        )),
    }
}

async fn check_daemon_health(session: &Session) -> Vec<DiagnosticResult> {
    let mut results = Vec::new();

    // Query health endpoint
    match query_with_timeout::<HealthResponse>(session, "bubbaloop/daemon/api/health").await {
        Ok(response) => {
            if response.status == "ok" {
                results.push(DiagnosticResult::pass(
                    "Daemon health",
                    "ok",
                ));
            } else {
                results.push(DiagnosticResult::fail(
                    "Daemon health",
                    &format!("unexpected status: {}", response.status),
                    "Run: systemctl --user restart bubbaloop-daemon",
                ));
            }
        }
        Err(e) => {
            results.push(DiagnosticResult::fail(
                "Daemon health",
                &format!("query failed: {}", e),
                "Check if daemon is connected to the same Zenoh router. Run: systemctl --user status bubbaloop-daemon",
            ));
        }
    }

    // Query nodes endpoint
    match query_with_timeout::<NodeListResponse>(session, "bubbaloop/daemon/api/nodes").await {
        Ok(response) => {
            results.push(DiagnosticResult::pass(
                "Node list",
                &format!("accessible ({} nodes)", response.nodes.len()),
            ));
        }
        Err(e) => {
            results.push(DiagnosticResult::fail(
                "Node list",
                &format!("query failed: {}", e),
                "Check if daemon is connected to the same Zenoh router",
            ));
        }
    }

    results
}

async fn check_node_subscriptions(session: &Session) -> Vec<DiagnosticResult> {
    let mut results = Vec::new();

    // Try to subscribe to daemon nodes topic
    match session.declare_subscriber("bubbaloop/daemon/nodes").await {
        Ok(_subscriber) => {
            results.push(DiagnosticResult::pass(
                "Node subscription",
                "can subscribe to bubbaloop/daemon/nodes",
            ));
        }
        Err(e) => {
            results.push(DiagnosticResult::fail(
                "Node subscription",
                &format!("failed: {}", e),
                "Check Zenoh router configuration",
            ));
        }
    }

    results
}

async fn query_with_timeout<T: for<'de> Deserialize<'de>>(
    session: &Session,
    key_expr: &str,
) -> Result<T> {
    let replies = session
        .get(key_expr)
        .target(QueryTarget::BestMatching)
        .timeout(Duration::from_secs(TIMEOUT_SECS))
        .await
        .map_err(|e| anyhow!("Zenoh query failed: {}", e))?;

    let timeout_duration = Duration::from_secs(TIMEOUT_SECS);
    let start = std::time::Instant::now();

    while start.elapsed() < timeout_duration {
        match tokio::time::timeout(
            timeout_duration - start.elapsed(),
            replies.recv_async(),
        )
        .await
        {
            Ok(Ok(reply)) => {
                if let Ok(sample) = reply.result() {
                    let bytes = sample.payload().to_bytes();
                    let text = String::from_utf8_lossy(&bytes);
                    let result: T = serde_json::from_str(&text)?;
                    return Ok(result);
                }
            }
            Ok(Err(_)) | Err(_) => break,
        }
    }

    Err(anyhow!("No reply received for {} (timeout after {}s)", key_expr, TIMEOUT_SECS))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_diagnostic_result_pass() {
        let result = DiagnosticResult::pass("test", "all good");
        assert!(result.passed);
        assert_eq!(result.check, "test");
        assert_eq!(result.message, "all good");
        assert!(result.fix.is_none());
    }

    #[test]
    fn test_diagnostic_result_fail() {
        let result = DiagnosticResult::fail("test", "something wrong", "do this");
        assert!(!result.passed);
        assert_eq!(result.check, "test");
        assert_eq!(result.message, "something wrong");
        assert_eq!(result.fix, Some("do this".to_string()));
    }

    #[test]
    fn test_health_response_deserialization() {
        let json = r#"{"status": "ok"}"#;
        let response: HealthResponse = serde_json::from_str(json).unwrap();
        assert_eq!(response.status, "ok");
    }

    #[test]
    fn test_node_list_response_deserialization() {
        let json = r#"{
            "nodes": [],
            "timestamp_ms": 1234567890
        }"#;
        let response: NodeListResponse = serde_json::from_str(json).unwrap();
        assert_eq!(response.nodes.len(), 0);
        assert_eq!(response.timestamp_ms, 1234567890);
    }

    #[test]
    fn test_node_state_deserialization() {
        let json = r#"{
            "name": "test-node",
            "path": "/path",
            "status": "running",
            "installed": true,
            "autostart_enabled": false,
            "version": "1.0.0",
            "description": "Test",
            "node_type": "rust",
            "is_built": true,
            "build_output": []
        }"#;
        let node: NodeState = serde_json::from_str(json).unwrap();
        assert_eq!(node.name, "test-node");
        assert_eq!(node.status, "running");
    }
}
