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
use serde::{Deserialize, Serialize};
use std::time::Duration;
use tokio::process::Command;
use zenoh::query::QueryTarget;
use zenoh::Session;

const TIMEOUT_SECS: u64 = 5;

/// Actions that can be automatically fixed
#[derive(Debug, Clone)]
pub enum FixAction {
    StartZenohd,
    StartDaemonService,
    RestartDaemonService,
    StartBridgeService,
    CreateZenohConfig,
    CreateMarketplaceSources,
}

impl FixAction {
    fn description(&self) -> &'static str {
        match self {
            FixAction::StartZenohd => "Start zenohd router",
            FixAction::StartDaemonService => "Start bubbaloop-daemon service",
            FixAction::RestartDaemonService => "Restart bubbaloop-daemon service",
            FixAction::StartBridgeService => "Start zenoh-bridge service",
            FixAction::CreateZenohConfig => "Create Zenoh config file",
            FixAction::CreateMarketplaceSources => {
                "Create marketplace sources with official registry"
            }
        }
    }

    async fn execute(&self) -> Result<String> {
        match self {
            FixAction::StartZenohd => {
                // Start zenohd in background
                let mut cmd = Command::new("zenohd");
                cmd.stdout(std::process::Stdio::null())
                    .stderr(std::process::Stdio::null());

                let child = cmd.spawn()?;
                // Detach the process
                std::mem::forget(child);

                // Wait a moment for it to start
                tokio::time::sleep(Duration::from_millis(500)).await;

                // Verify it started
                if is_process_running("zenohd").await {
                    Ok("zenohd started successfully".to_string())
                } else {
                    Err(anyhow!("Failed to start zenohd"))
                }
            }
            FixAction::StartDaemonService => {
                let output = Command::new("systemctl")
                    .args(["--user", "start", "bubbaloop-daemon.service"])
                    .output()
                    .await?;

                if output.status.success() {
                    Ok("bubbaloop-daemon service started".to_string())
                } else {
                    Err(anyhow!(
                        "Failed to start: {}",
                        String::from_utf8_lossy(&output.stderr)
                    ))
                }
            }
            FixAction::RestartDaemonService => {
                let output = Command::new("systemctl")
                    .args(["--user", "restart", "bubbaloop-daemon.service"])
                    .output()
                    .await?;

                if output.status.success() {
                    Ok("bubbaloop-daemon service restarted".to_string())
                } else {
                    Err(anyhow!(
                        "Failed to restart: {}",
                        String::from_utf8_lossy(&output.stderr)
                    ))
                }
            }
            FixAction::StartBridgeService => {
                let output = Command::new("systemctl")
                    .args(["--user", "start", "bubbaloop-bridge.service"])
                    .output()
                    .await?;

                if output.status.success() {
                    Ok("zenoh-bridge service started".to_string())
                } else {
                    Err(anyhow!(
                        "Failed to start: {}",
                        String::from_utf8_lossy(&output.stderr)
                    ))
                }
            }
            FixAction::CreateZenohConfig => {
                let home = dirs::home_dir().ok_or_else(|| anyhow!("HOME not set"))?;
                let zenoh_dir = home.join(".bubbaloop/zenoh");
                std::fs::create_dir_all(&zenoh_dir)?;

                let config_path = zenoh_dir.join("zenohd.json5");
                let config_content = r#"{
  mode: "router",
  listen: {
    endpoints: ["tcp/127.0.0.1:7447"]
  },
  scouting: {
    multicast: {
      enabled: false
    },
    gossip: {
      enabled: false
    }
  }
}"#;
                std::fs::write(&config_path, config_content)?;
                Ok(format!("Created {}", config_path.display()))
            }
            FixAction::CreateMarketplaceSources => {
                let home = dirs::home_dir().ok_or_else(|| anyhow!("HOME not set"))?;
                let sources_path = home.join(".bubbaloop/sources.json");

                let sources_content = r#"{
  "sources": [
    {
      "name": "Official Nodes",
      "path": "kornia/bubbaloop-nodes-official",
      "type": "builtin",
      "enabled": true
    }
  ]
}"#;
                std::fs::write(&sources_path, sources_content)?;
                Ok(format!(
                    "Created {} with official nodes registry",
                    sources_path.display()
                ))
            }
        }
    }
}

#[derive(Debug, Serialize)]
struct DiagnosticResult {
    check: String,
    passed: bool,
    message: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    fix: Option<String>,
    #[serde(skip)]
    fix_action: Option<FixAction>,
    #[serde(skip_serializing_if = "Option::is_none")]
    details: Option<serde_json::Value>,
}

impl DiagnosticResult {
    fn pass(check: &str, message: &str) -> Self {
        Self {
            check: check.to_string(),
            passed: true,
            message: message.to_string(),
            fix: None,
            fix_action: None,
            details: None,
        }
    }

    fn pass_with_details(check: &str, message: &str, details: serde_json::Value) -> Self {
        Self {
            check: check.to_string(),
            passed: true,
            message: message.to_string(),
            fix: None,
            fix_action: None,
            details: Some(details),
        }
    }

    fn fail(check: &str, message: &str, fix: &str) -> Self {
        Self {
            check: check.to_string(),
            passed: false,
            message: message.to_string(),
            fix: Some(fix.to_string()),
            fix_action: None,
            details: None,
        }
    }

    fn fail_with_action(check: &str, message: &str, fix: &str, action: FixAction) -> Self {
        Self {
            check: check.to_string(),
            passed: false,
            message: message.to_string(),
            fix: Some(fix.to_string()),
            fix_action: Some(action),
            details: None,
        }
    }

    fn fail_with_details(
        check: &str,
        message: &str,
        fix: &str,
        details: serde_json::Value,
    ) -> Self {
        Self {
            check: check.to_string(),
            passed: false,
            message: message.to_string(),
            fix: Some(fix.to_string()),
            fix_action: None,
            details: Some(details),
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

pub async fn run(fix: bool, json: bool, check: &str) -> Result<()> {
    // Normalize check name
    let check_type = check.to_lowercase();

    if !json {
        if fix {
            println!("bubbaloop doctor --fix");
            println!("=====================");
        } else {
            println!("bubbaloop doctor");
            println!("================");
        }
        println!();
    }

    let mut results = Vec::new();
    let mut fixes_applied = 0;

    // Determine which checks to run
    let run_config = check_type == "all" || check_type == "config";
    let run_services = check_type == "all" || check_type == "zenoh";
    let run_zenoh = check_type == "all" || check_type == "zenoh";
    let run_daemon = check_type == "all" || check_type == "daemon";

    let mut session: Option<Session> = None;

    // 0. Check configuration files
    if run_config {
        if !json {
            println!("[0/4] Checking configuration...");
        }
        results.extend(check_configuration().await);

        // Apply fixes for configuration if --fix flag is set
        if fix && !json {
            fixes_applied += apply_fixes(&mut results).await;
        }
        if !json {
            println!();
        }
    }

    // 1. Check system services
    if run_services {
        if !json {
            println!("[1/4] Checking system services...");
        }
        results.extend(check_system_services().await);

        // Apply fixes for system services if --fix flag is set
        if fix && !json {
            fixes_applied += apply_fixes(&mut results).await;
        }
        if !json {
            println!();
        }
    }

    // 2. Check Zenoh connectivity
    if run_zenoh {
        if !json {
            println!("[2/4] Checking Zenoh connectivity...");
        }
        let zenoh_results = check_zenoh_comprehensive().await;
        let has_session = zenoh_results
            .iter()
            .any(|r| r.check == "Zenoh connection" && r.passed);

        results.extend(zenoh_results);

        if has_session {
            // Try to get a session for further checks
            if let Ok(s) = get_zenoh_session().await {
                session = Some(s);
            }
        }

        if !json {
            println!();
        }
    }

    // 3. Check daemon health (requires Zenoh connection)
    if run_daemon {
        if !json {
            println!("[3/4] Checking daemon health...");
        }

        // Get session if we don't already have one
        if session.is_none() && check_type == "daemon" {
            if let Ok(s) = get_zenoh_session().await {
                session = Some(s);
            }
        }

        if let Some(ref session) = session {
            results.extend(check_daemon_health(session).await);
        } else {
            results.push(DiagnosticResult::fail(
                "Daemon health",
                "Skipped (no Zenoh connection)",
                "Fix Zenoh connection first",
            ));
        }
        if !json {
            println!();
        }
    }

    // 4. Check node subscriptions (requires Zenoh connection)
    if check_type == "all" {
        if !json {
            println!("[4/4] Checking node subscriptions...");
        }
        if let Some(ref session) = session {
            results.extend(check_node_subscriptions(session).await);
        } else {
            results.push(DiagnosticResult::fail(
                "Node subscriptions",
                "Skipped (no Zenoh connection)",
                "Fix Zenoh connection first",
            ));
        }
        if !json {
            println!();
        }
    }

    // Output results
    if json {
        print_json_results(&results, fixes_applied)?;
    } else {
        print_human_results(&results, fixes_applied, fix)?;
    }

    // Close Zenoh session if open
    if let Some(session) = session {
        let _ = session.close().await;
    }

    Ok(())
}

fn print_json_results(results: &[DiagnosticResult], fixes_applied: usize) -> Result<()> {
    let passed = results.iter().filter(|r| r.passed).count();
    let failed = results.iter().filter(|r| !r.passed).count();

    let output = serde_json::json!({
        "summary": {
            "total": results.len(),
            "passed": passed,
            "failed": failed,
            "fixes_applied": fixes_applied,
        },
        "checks": results,
    });

    println!("{}", serde_json::to_string_pretty(&output)?);
    Ok(())
}

fn print_human_results(
    results: &[DiagnosticResult],
    fixes_applied: usize,
    fix: bool,
) -> Result<()> {
    println!("Summary");
    println!("=======");
    println!();

    let mut issues_found = 0;

    for result in results {
        let symbol = if result.passed { "✓" } else { "✗" };
        println!("[{}] {}: {}", symbol, result.check, result.message);

        if !result.passed {
            issues_found += 1;
            if let Some(fix_hint) = &result.fix {
                if result.fix_action.is_some() {
                    println!("    → Auto-fixable: {}", fix_hint);
                } else {
                    println!("    → Fix: {}", fix_hint);
                }
            }
        }

        // Print details if available
        if let Some(details) = &result.details {
            if let Some(obj) = details.as_object() {
                for (key, value) in obj {
                    println!("    {} = {}", key, value);
                }
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
        if fixes_applied > 0 {
            println!(
                "Applied {} fix{}",
                fixes_applied,
                if fixes_applied == 1 { "" } else { "es" }
            );
        } else if !fix {
            // Count auto-fixable issues
            let auto_fixable: usize = results
                .iter()
                .filter(|r| !r.passed && r.fix_action.is_some())
                .count();
            if auto_fixable > 0 {
                println!();
                println!(
                    "Tip: Run 'bubbaloop doctor --fix' to automatically fix {} issue{}",
                    auto_fixable,
                    if auto_fixable == 1 { "" } else { "s" }
                );
            }
        }
    }

    Ok(())
}

/// Apply all available fixes and return count of fixes applied
async fn apply_fixes(results: &mut [DiagnosticResult]) -> usize {
    let mut fixes_applied = 0;

    for result in results.iter_mut() {
        if result.passed || result.fix_action.is_none() {
            continue;
        }

        let action = result.fix_action.clone().unwrap();
        println!("    → Fixing: {}", action.description());

        match action.execute().await {
            Ok(msg) => {
                println!("      ✓ {}", msg);
                result.passed = true;
                result.message = format!("{} (fixed)", result.message);
                fixes_applied += 1;
            }
            Err(e) => {
                println!("      ✗ Failed: {}", e);
            }
        }
    }

    fixes_applied
}

/// Check configuration files exist
async fn check_configuration() -> Vec<DiagnosticResult> {
    let mut results = Vec::new();

    let home = match dirs::home_dir() {
        Some(h) => h,
        None => {
            results.push(DiagnosticResult::fail(
                "Home directory",
                "HOME not set",
                "Set HOME environment variable",
            ));
            return results;
        }
    };

    let bubbaloop_dir = home.join(".bubbaloop");

    // Check zenoh config exists
    let zenoh_config = bubbaloop_dir.join("zenoh/zenohd.json5");
    if zenoh_config.exists() {
        results.push(DiagnosticResult::pass(
            "Zenoh config",
            &format!("exists at {}", zenoh_config.display()),
        ));
    } else {
        // Also check legacy location
        let legacy_config = bubbaloop_dir.join("zenoh.json5");
        if legacy_config.exists() {
            results.push(DiagnosticResult::pass(
                "Zenoh config",
                &format!("exists at {} (legacy location)", legacy_config.display()),
            ));
        } else {
            results.push(DiagnosticResult::fail_with_action(
                "Zenoh config",
                "not found",
                "Run: bubbaloop doctor --fix",
                FixAction::CreateZenohConfig,
            ));
        }
    }

    // Check marketplace sources exist
    let sources_file = bubbaloop_dir.join("sources.json");
    if sources_file.exists() {
        // Check if it has at least one source
        if let Ok(content) = std::fs::read_to_string(&sources_file) {
            if content.contains("Official Nodes") || content.contains("bubbaloop-nodes-official") {
                results.push(DiagnosticResult::pass(
                    "Marketplace sources",
                    "configured with official registry",
                ));
            } else {
                results.push(DiagnosticResult::fail_with_action(
                    "Marketplace sources",
                    "exists but missing official registry",
                    "Run: bubbaloop marketplace add official kornia/bubbaloop-nodes-official",
                    FixAction::CreateMarketplaceSources,
                ));
            }
        } else {
            results.push(DiagnosticResult::fail_with_action(
                "Marketplace sources",
                "file exists but unreadable",
                "Run: bubbaloop doctor --fix",
                FixAction::CreateMarketplaceSources,
            ));
        }
    } else {
        results.push(DiagnosticResult::fail_with_action(
            "Marketplace sources",
            "not configured",
            "Run: bubbaloop doctor --fix",
            FixAction::CreateMarketplaceSources,
        ));
    }

    // Check bin directory
    let bin_dir = bubbaloop_dir.join("bin");
    if bin_dir.exists() {
        let bubbaloop_bin = bin_dir.join("bubbaloop");
        if bubbaloop_bin.exists() {
            results.push(DiagnosticResult::pass(
                "Bubbaloop binary",
                &format!("installed at {}", bubbaloop_bin.display()),
            ));
        } else {
            results.push(DiagnosticResult::fail(
                "Bubbaloop binary",
                "not found in ~/.bubbaloop/bin/",
                "Re-run the install script",
            ));
        }
    } else {
        results.push(DiagnosticResult::fail(
            "Bubbaloop installation",
            "~/.bubbaloop/bin/ not found",
            "Run: curl -fsSL https://raw.githubusercontent.com/kornia/bubbaloop/main/scripts/install.sh | bash",
        ));
    }

    results
}

async fn check_system_services() -> Vec<DiagnosticResult> {
    let mut results = Vec::new();

    // Check zenohd
    let zenohd_running = is_process_running("zenohd").await;
    if zenohd_running {
        // Additional check: is port 7447 listening?
        let port_check = check_port(7447).await;
        if port_check {
            results.push(DiagnosticResult::pass("zenohd", "running on port 7447"));
        } else {
            results.push(DiagnosticResult::fail(
                "zenohd",
                "running but port 7447 not accessible",
                "Check if zenohd is configured to listen on tcp/127.0.0.1:7447",
            ));
        }
    } else {
        results.push(DiagnosticResult::fail_with_action(
            "zenohd",
            "not running",
            "Run: zenohd &",
            FixAction::StartZenohd,
        ));
    }

    // Check bubbaloop-daemon
    let daemon_service = check_systemd_service("bubbaloop-daemon.service").await;
    if daemon_service == "active" {
        results.push(DiagnosticResult::pass("bubbaloop-daemon", "service active"));
    } else if daemon_service == "inactive" {
        results.push(DiagnosticResult::fail_with_action(
            "bubbaloop-daemon",
            "service inactive",
            "Run: systemctl --user start bubbaloop-daemon",
            FixAction::StartDaemonService,
        ));
    } else if daemon_service == "failed" {
        results.push(DiagnosticResult::fail_with_action(
            "bubbaloop-daemon",
            "service failed",
            "Run: systemctl --user restart bubbaloop-daemon",
            FixAction::RestartDaemonService,
        ));
    } else {
        results.push(DiagnosticResult::fail(
            "bubbaloop-daemon",
            &format!("service {}", daemon_service),
            "Run: systemctl --user status bubbaloop-daemon",
        ));
    }

    // Check zenoh-bridge (optional, so we don't fail hard)
    let bridge_service = check_systemd_service("bubbaloop-bridge.service").await;
    if bridge_service == "active" {
        results.push(DiagnosticResult::pass("zenoh-bridge", "service active"));
    } else {
        results.push(DiagnosticResult::fail_with_action(
            "zenoh-bridge",
            "not running (optional for CLI, required for dashboard)",
            "Run: systemctl --user start zenoh-bridge",
            FixAction::StartBridgeService,
        ));
    }

    results
}

async fn is_process_running(name: &str) -> bool {
    let output = Command::new("pgrep").arg("-x").arg(name).output().await;

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

async fn get_zenoh_session() -> Result<Session> {
    let mut config = zenoh::Config::default();

    config
        .insert_json5("mode", "\"client\"")
        .map_err(|e| anyhow!("Failed to configure client mode: {}", e))?;

    let endpoint = std::env::var("BUBBALOOP_ZENOH_ENDPOINT")
        .unwrap_or_else(|_| "tcp/127.0.0.1:7447".to_string());

    config
        .insert_json5("connect/endpoints", &format!("[\"{}\"]", endpoint))
        .map_err(|e| anyhow!("Failed to configure endpoint: {}", e))?;

    // Disable scouting
    let _ = config.insert_json5("scouting/multicast/enabled", "false");
    let _ = config.insert_json5("scouting/gossip/enabled", "false");

    zenoh::open(config)
        .await
        .map_err(|e| anyhow!("Failed to open Zenoh session: {}", e))
}

/// Comprehensive Zenoh connectivity checks
async fn check_zenoh_comprehensive() -> Vec<DiagnosticResult> {
    let mut results = Vec::new();

    let endpoint = std::env::var("BUBBALOOP_ZENOH_ENDPOINT")
        .unwrap_or_else(|_| "tcp/127.0.0.1:7447".to_string());

    // Parse endpoint to get host and port
    let (host, port) = parse_zenoh_endpoint(&endpoint);

    // Check 1: Can we create a session?
    let session_result = get_zenoh_session().await;
    match &session_result {
        Ok(session) => {
            let session_id = session.zid().to_string();
            results.push(DiagnosticResult::pass_with_details(
                "Zenoh connection",
                &format!("connected to {}", endpoint),
                serde_json::json!({
                    "endpoint": endpoint,
                    "session_id": session_id,
                    "mode": "client",
                }),
            ));

            // Check 2: Can we declare a test queryable?
            match session.declare_queryable("bubbaloop/doctor/test").await {
                Ok(queryable) => {
                    results.push(DiagnosticResult::pass(
                        "Zenoh queryable",
                        "can declare queryables",
                    ));
                    drop(queryable);
                }
                Err(e) => {
                    results.push(DiagnosticResult::fail(
                        "Zenoh queryable",
                        &format!("failed to declare queryable: {}", e),
                        "Check Zenoh router permissions and configuration",
                    ));
                }
            }

            // Check 3: Can we query ourselves?
            let test_key = "bubbaloop/doctor/test/query";
            let _test_queryable = session.declare_queryable(test_key).await;

            if let Ok(test_queryable) = _test_queryable {
                // Spawn a task to respond to our own query
                let handle = tokio::spawn(async move {
                    if let Ok(query) = test_queryable.recv_async().await {
                        let _ = query.reply(test_key, "pong").await;
                    }
                });

                tokio::time::sleep(Duration::from_millis(100)).await;

                // Try to query
                match session.get(test_key).timeout(Duration::from_secs(2)).await {
                    Ok(receiver) => {
                        let replies: Vec<_> = receiver.into_iter().collect();
                        if replies.is_empty() {
                            results.push(DiagnosticResult::fail_with_details(
                                "Zenoh query/reply",
                                "query succeeded but no replies received (timeout)",
                                "This is the 'Didn't receive final reply for query: Timeout' error. Check if zenohd is running and accessible. Increase query timeout if on slow network.",
                                serde_json::json!({
                                    "error_type": "timeout",
                                    "common_cause": "zenohd not routing queries correctly",
                                    "timeout_ms": 2000,
                                }),
                            ));
                        } else {
                            results.push(DiagnosticResult::pass(
                                "Zenoh query/reply",
                                "can send queries and receive replies",
                            ));
                        }
                    }
                    Err(e) => {
                        results.push(DiagnosticResult::fail_with_details(
                            "Zenoh query/reply",
                            &format!("query failed: {}", e),
                            "This indicates Zenoh routing is broken. Check zenohd logs and configuration.",
                            serde_json::json!({
                                "error": e.to_string(),
                                "error_type": "query_failed",
                            }),
                        ));
                    }
                }

                handle.abort();
            } else {
                results.push(DiagnosticResult::fail(
                    "Zenoh query/reply",
                    "skipped (could not declare test queryable)",
                    "Fix queryable declaration first",
                ));
            }

            let _ = session.close().await;
        }
        Err(e) => {
            results.push(DiagnosticResult::fail_with_details(
                "Zenoh connection",
                &format!("failed to connect: {}", e),
                &format!(
                    "Ensure zenohd is running and accessible at {}. Run: zenohd &",
                    endpoint
                ),
                serde_json::json!({
                    "endpoint": endpoint,
                    "host": host,
                    "port": port,
                    "error": e.to_string(),
                }),
            ));

            // Check if port is even listening
            if let Ok(can_connect) = tokio::time::timeout(
                Duration::from_secs(1),
                tokio::net::TcpStream::connect(format!("{}:{}", host, port)),
            )
            .await
            {
                if can_connect.is_ok() {
                    results.push(DiagnosticResult::pass(
                        "Zenoh port",
                        &format!("port {} is listening", port),
                    ));
                } else {
                    results.push(DiagnosticResult::fail(
                        "Zenoh port",
                        &format!("port {} is not accessible", port),
                        "Start zenohd or check firewall settings",
                    ));
                }
            } else {
                results.push(DiagnosticResult::fail(
                    "Zenoh port",
                    &format!("timeout connecting to port {}", port),
                    "Check if zenohd is running and network is accessible",
                ));
            }
        }
    }

    results
}

fn parse_zenoh_endpoint(endpoint: &str) -> (String, u16) {
    // Parse "tcp/127.0.0.1:7447" format
    if let Some(addr_part) = endpoint.strip_prefix("tcp/") {
        if let Some((host, port_str)) = addr_part.rsplit_once(':') {
            if let Ok(port) = port_str.parse() {
                return (host.to_string(), port);
            }
        }
    }
    // Default
    ("127.0.0.1".to_string(), 7447)
}

async fn check_daemon_health(session: &Session) -> Vec<DiagnosticResult> {
    let mut results = Vec::new();

    // Query health endpoint
    match query_with_timeout::<HealthResponse>(session, "bubbaloop/daemon/api/health").await {
        Ok(response) => {
            if response.status == "ok" {
                results.push(DiagnosticResult::pass("Daemon health", "ok"));
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

    match tokio::time::timeout(timeout_duration, replies.recv_async()).await {
        Ok(Ok(reply)) => {
            if let Ok(sample) = reply.result() {
                let bytes = sample.payload().to_bytes();
                let text = String::from_utf8_lossy(&bytes);
                let result: T = serde_json::from_str(&text)?;
                return Ok(result);
            }
        }
        Ok(Err(e)) => return Err(anyhow!("Failed to receive reply: {}", e)),
        Err(_) => { /* timeout - fall through to error below */ }
    }

    Err(anyhow!(
        "No reply received for {} (timeout after {}s)",
        key_expr,
        TIMEOUT_SECS
    ))
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
