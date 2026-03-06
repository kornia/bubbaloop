// doctor spawns systemctl directly for diagnostic independence — if zbus/dbus is broken,
// doctor must still work

//! Individual diagnostic check functions
//!
//! Each check function returns a `Vec<DiagnosticResult>` containing one or more
//! results for the checks it performs.

use crate::cli::system_utils::{check_systemd_service, is_process_running};

use super::fixes::FixAction;
use super::DiagnosticResult;

/// Check configuration files exist
pub async fn check_configuration() -> Vec<DiagnosticResult> {
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

pub async fn check_system_services() -> Vec<DiagnosticResult> {
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
    let daemon_installed = is_service_installed("bubbaloop-daemon.service").await;
    if daemon_service == "active" {
        results.push(DiagnosticResult::pass("bubbaloop-daemon", "service active"));
    } else if !daemon_installed {
        // Service not installed — install it first, then start
        results.push(DiagnosticResult::fail_with_action(
            "bubbaloop-daemon",
            "service not installed",
            "Run: bubbaloop daemon start",
            FixAction::InstallAndStartDaemon,
        ));
    } else if daemon_service == "inactive" {
        results.push(DiagnosticResult::fail_with_action(
            "bubbaloop-daemon",
            "service inactive",
            "Run: systemctl --user start bubbaloop-daemon",
            FixAction::StartDaemonService,
        ));
    } else if daemon_service == "failed" || daemon_service == "activating" {
        // "activating" means stuck — restart it
        results.push(DiagnosticResult::fail_with_action(
            "bubbaloop-daemon",
            &format!("service {} — restarting", daemon_service),
            "Run: systemctl --user restart bubbaloop-daemon",
            FixAction::RestartDaemonService,
        ));
    } else {
        // Unknown state — try restart
        results.push(DiagnosticResult::fail_with_action(
            "bubbaloop-daemon",
            &format!("service {}", daemon_service),
            "Run: systemctl --user restart bubbaloop-daemon",
            FixAction::RestartDaemonService,
        ));
    }

    // Check zenoh-bridge (optional — only needed for dashboard WebSocket bridge)
    let bridge_service = check_systemd_service("bubbaloop-bridge.service").await;
    let bridge_installed = is_service_installed("bubbaloop-bridge.service").await;
    if bridge_service == "active" {
        results.push(DiagnosticResult::pass("zenoh-bridge", "service active"));
    } else if !bridge_installed {
        results.push(DiagnosticResult::pass(
            "zenoh-bridge",
            "not installed (optional — only needed for dashboard)",
        ));
    } else {
        results.push(DiagnosticResult::fail_with_action(
            "zenoh-bridge",
            &format!("service {} (required for dashboard)", bridge_service),
            "Run: systemctl --user start bubbaloop-bridge",
            FixAction::StartBridgeService,
        ));
    }

    results
}

/// Check if a systemd service unit file exists (installed).
async fn is_service_installed(service_name: &str) -> bool {
    let output = tokio::process::Command::new("systemctl")
        .args(["--user", "cat", service_name])
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .status()
        .await;
    matches!(output, Ok(s) if s.success())
}

pub async fn check_port(port: u16) -> bool {
    // Try to connect to the port
    tokio::net::TcpStream::connect(format!("127.0.0.1:{}", port))
        .await
        .is_ok()
}

/// Check daemon connectivity via Zenoh manifest query.
pub async fn check_daemon_connectivity() -> Vec<DiagnosticResult> {
    let mut results = Vec::new();

    match crate::cli::daemon_client::DaemonClient::connect().await {
        Ok(client) => match client.health().await {
            Ok(manifest) => {
                results.push(DiagnosticResult::pass_with_details(
                    "Daemon Zenoh",
                    &format!(
                        "reachable (v{}, uptime={}s)",
                        manifest.version, manifest.uptime_secs
                    ),
                    serde_json::json!({
                        "version": manifest.version,
                        "machine_id": manifest.machine_id,
                        "uptime_secs": manifest.uptime_secs,
                        "node_count": manifest.node_count,
                        "agent_count": manifest.agent_count,
                        "mcp_port": manifest.mcp_port,
                    }),
                ));
            }
            Err(_) => {
                results.push(DiagnosticResult::fail(
                    "Daemon Zenoh",
                    "not reachable via Zenoh",
                    "Is the daemon running? Check: systemctl --user status bubbaloop-daemon",
                ));
            }
        },
        Err(_) => {
            results.push(DiagnosticResult::fail(
                "Daemon Zenoh",
                "cannot connect to Zenoh",
                "Is zenohd running? Check: bubbaloop doctor -c zenoh",
            ));
        }
    }

    results
}

pub async fn check_daemon_health() -> Vec<DiagnosticResult> {
    let mut results = Vec::new();

    let client = match crate::cli::daemon_client::DaemonClient::connect().await {
        Ok(c) => c,
        Err(e) => {
            results.push(DiagnosticResult::fail(
                "Daemon health",
                &format!("cannot connect: {}", e),
                "Check if daemon is running. Run: systemctl --user status bubbaloop-daemon",
            ));
            return results;
        }
    };

    // Check health via manifest query
    match client.health().await {
        Ok(manifest) => {
            results.push(DiagnosticResult::pass(
                "Daemon health",
                &format!(
                    "ok (v{}, {} nodes, {} agents)",
                    manifest.version, manifest.node_count, manifest.agent_count
                ),
            ));
        }
        Err(e) => {
            results.push(DiagnosticResult::fail(
                "Daemon health",
                &format!("health check failed: {}", e),
                "Check if daemon is running. Run: systemctl --user status bubbaloop-daemon",
            ));
        }
    }

    // Check nodes via gateway command
    match client.list_nodes().await {
        Ok(json) => {
            let nodes: Vec<crate::mcp::platform::NodeInfo> =
                serde_json::from_str(&json).unwrap_or_default();
            results.push(DiagnosticResult::pass(
                "Node list",
                &format!("accessible ({} nodes)", nodes.len()),
            ));
        }
        Err(e) => {
            results.push(DiagnosticResult::fail(
                "Node list",
                &format!("query failed: {}", e),
                "Check if daemon gateway is responding",
            ));
        }
    }

    results
}

pub async fn check_node_subscriptions() -> Vec<DiagnosticResult> {
    let mut results = Vec::new();

    // Zenoh data plane check: just verify zenohd port is accessible
    let zenoh_port_open = check_port(7447).await;
    if zenoh_port_open {
        results.push(DiagnosticResult::pass(
            "Zenoh data plane",
            "port 7447 accessible (used by nodes for streaming)",
        ));
    } else {
        results.push(DiagnosticResult::fail(
            "Zenoh data plane",
            "port 7447 not accessible",
            "Zenoh data plane is used by nodes for streaming. Start zenohd if needed.",
        ));
    }

    results
}

/// Check security posture of the deployment
pub async fn check_security() -> Vec<DiagnosticResult> {
    let mut results = Vec::new();

    let home = match dirs::home_dir() {
        Some(h) => h,
        None => return results,
    };

    let bubbaloop_dir = home.join(".bubbaloop");

    // Check 1: TLS enabled/disabled
    results.push(check_tls_status(&bubbaloop_dir));

    // Check 2: ACL configuration present
    results.push(check_acl_status(&bubbaloop_dir));

    // Check 3: Scouting disabled in zenoh config
    results.push(check_scouting_disabled(&bubbaloop_dir));

    // Check 4: Localhost-only binding
    results.push(check_localhost_binding(&bubbaloop_dir));

    // Check 5: Python node sandbox check
    results.extend(check_python_sandbox());

    // Check 6: Node name validation
    results.extend(check_node_names(&bubbaloop_dir));

    results
}

/// Check if TLS is configured in Zenoh config
fn check_tls_status(bubbaloop_dir: &std::path::Path) -> DiagnosticResult {
    let zenoh_config = bubbaloop_dir.join("zenoh/zenohd.json5");
    if let Ok(content) = std::fs::read_to_string(&zenoh_config) {
        if content.contains("tls") && content.contains("server_certificate") {
            DiagnosticResult::pass("TLS config", "TLS is configured")
        } else {
            DiagnosticResult::fail(
                "TLS config",
                "TLS not configured (plaintext transport)",
                "Run: bubbaloop init-tls",
            )
        }
    } else {
        DiagnosticResult::fail(
            "TLS config",
            "Zenoh config not found",
            "Run: bubbaloop doctor --fix",
        )
    }
}

/// Check if ACL rules are configured
fn check_acl_status(bubbaloop_dir: &std::path::Path) -> DiagnosticResult {
    let zenoh_config = bubbaloop_dir.join("zenoh/zenohd.json5");
    if let Ok(content) = std::fs::read_to_string(&zenoh_config) {
        if content.contains("access_control") {
            DiagnosticResult::pass("ACL config", "access control rules present")
        } else {
            DiagnosticResult::fail(
                "ACL config",
                "no access control rules configured",
                "See: configs/zenoh/acl-example.json5",
            )
        }
    } else {
        DiagnosticResult::fail(
            "ACL config",
            "Zenoh config not found",
            "Run: bubbaloop doctor --fix",
        )
    }
}

/// Check if scouting is disabled in zenoh config
fn check_scouting_disabled(bubbaloop_dir: &std::path::Path) -> DiagnosticResult {
    let zenoh_config = bubbaloop_dir.join("zenoh/zenohd.json5");
    if let Ok(content) = std::fs::read_to_string(&zenoh_config) {
        // Check for multicast disabled
        let multicast_disabled = content.contains("multicast")
            && content.contains("enabled")
            && content.contains("false");
        if multicast_disabled {
            DiagnosticResult::pass("Scouting", "multicast scouting disabled")
        } else {
            DiagnosticResult::fail(
                "Scouting",
                "multicast scouting may be enabled (security risk on shared networks)",
                "Add scouting.multicast.enabled: false to zenohd.json5",
            )
        }
    } else {
        DiagnosticResult::fail(
            "Scouting",
            "Zenoh config not found",
            "Run: bubbaloop doctor --fix",
        )
    }
}

/// Check if Zenoh binds to localhost only
fn check_localhost_binding(bubbaloop_dir: &std::path::Path) -> DiagnosticResult {
    let zenoh_config = bubbaloop_dir.join("zenoh/zenohd.json5");
    if let Ok(content) = std::fs::read_to_string(&zenoh_config) {
        // Check for 0.0.0.0 binding (allows remote connections)
        if content.contains("0.0.0.0") {
            DiagnosticResult::fail(
                "Localhost binding",
                "Zenoh binds to 0.0.0.0 (all interfaces)",
                "Use 127.0.0.1 for local-only or enable TLS for remote access",
            )
        } else {
            DiagnosticResult::pass("Localhost binding", "not binding to all interfaces")
        }
    } else {
        DiagnosticResult::fail(
            "Localhost binding",
            "Zenoh config not found",
            "Run: bubbaloop doctor --fix",
        )
    }
}

/// Check if Python node systemd units have sandboxing directives
fn check_python_sandbox() -> Vec<DiagnosticResult> {
    let mut results = Vec::new();

    let systemd_dir = dirs::home_dir()
        .map(|h| h.join(".config/systemd/user"))
        .unwrap_or_default();

    if !systemd_dir.exists() {
        return results;
    }

    let entries = match std::fs::read_dir(&systemd_dir) {
        Ok(e) => e,
        Err(_) => return results,
    };

    for entry in entries.flatten() {
        let path = entry.path();
        let name = entry.file_name().to_string_lossy().to_string();
        if !name.starts_with("bubbaloop-") || !name.ends_with(".service") {
            continue;
        }

        if let Ok(content) = std::fs::read_to_string(&path) {
            // Only check Python nodes (they have PYTHONUNBUFFERED)
            if !content.contains("PYTHONUNBUFFERED") {
                continue;
            }

            let has_sandbox = content.contains("ProtectHome=read-only")
                && content.contains("MemoryMax=")
                && content.contains("RestrictSUIDSGID=true");

            if has_sandbox {
                results.push(DiagnosticResult::pass(
                    &format!("Python sandbox ({})", name),
                    "sandboxing directives present",
                ));
            } else {
                results.push(DiagnosticResult::fail(
                    &format!("Python sandbox ({})", name),
                    "missing sandboxing directives",
                    "Reinstall node to get updated unit file with sandboxing",
                ));
            }
        }
    }

    results
}

/// Check that registered node names are valid
fn check_node_names(bubbaloop_dir: &std::path::Path) -> Vec<DiagnosticResult> {
    let mut results = Vec::new();
    let nodes_json = bubbaloop_dir.join("nodes.json");

    if let Ok(content) = std::fs::read_to_string(&nodes_json) {
        // Simple check: parse as JSON and validate each node name
        if let Ok(parsed) = serde_json::from_str::<serde_json::Value>(&content) {
            if let Some(nodes) = parsed.get("nodes").and_then(|n| n.as_array()) {
                for node in nodes {
                    if let Some(name) = node.get("name").and_then(|n| n.as_str()) {
                        if crate::validation::validate_node_name(name).is_ok() {
                            results.push(DiagnosticResult::pass(
                                &format!("Node name ({})", name),
                                "valid",
                            ));
                        } else {
                            results.push(DiagnosticResult::fail(
                                &format!("Node name ({})", name),
                                "invalid characters or length",
                                "Node names must be 1-64 chars, [a-zA-Z0-9_-]",
                            ));
                        }
                    }
                }
            }
        }
    }

    results
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_check_tls_status_with_tls_config() {
        let dir = tempfile::tempdir().unwrap();
        let zenoh_dir = dir.path().join("zenoh");
        std::fs::create_dir_all(&zenoh_dir).unwrap();
        std::fs::write(
            zenoh_dir.join("zenohd.json5"),
            r#"{ transport: { link: { tls: { server_certificate: "/path/cert.pem" } } } }"#,
        )
        .unwrap();

        let result = check_tls_status(dir.path());
        assert!(result.passed, "Expected TLS check to pass");
    }

    #[test]
    fn test_check_tls_status_without_tls() {
        let dir = tempfile::tempdir().unwrap();
        let zenoh_dir = dir.path().join("zenoh");
        std::fs::create_dir_all(&zenoh_dir).unwrap();
        std::fs::write(zenoh_dir.join("zenohd.json5"), r#"{ mode: "router" }"#).unwrap();

        let result = check_tls_status(dir.path());
        assert!(!result.passed, "Expected TLS check to fail");
    }

    #[test]
    fn test_check_scouting_disabled_passes() {
        let dir = tempfile::tempdir().unwrap();
        let zenoh_dir = dir.path().join("zenoh");
        std::fs::create_dir_all(&zenoh_dir).unwrap();
        std::fs::write(
            zenoh_dir.join("zenohd.json5"),
            r#"{ scouting: { multicast: { enabled: false } } }"#,
        )
        .unwrap();

        let result = check_scouting_disabled(dir.path());
        assert!(result.passed, "Expected scouting check to pass");
    }

    #[test]
    fn test_check_scouting_disabled_fails_when_enabled() {
        let dir = tempfile::tempdir().unwrap();
        let zenoh_dir = dir.path().join("zenoh");
        std::fs::create_dir_all(&zenoh_dir).unwrap();
        std::fs::write(zenoh_dir.join("zenohd.json5"), r#"{ mode: "router" }"#).unwrap();

        let result = check_scouting_disabled(dir.path());
        assert!(!result.passed, "Expected scouting check to fail");
    }

    #[test]
    fn test_check_localhost_binding_passes() {
        let dir = tempfile::tempdir().unwrap();
        let zenoh_dir = dir.path().join("zenoh");
        std::fs::create_dir_all(&zenoh_dir).unwrap();
        std::fs::write(
            zenoh_dir.join("zenohd.json5"),
            r#"{ listen: { endpoints: ["tcp/127.0.0.1:7447"] } }"#,
        )
        .unwrap();

        let result = check_localhost_binding(dir.path());
        assert!(result.passed, "Expected localhost check to pass");
    }

    #[test]
    fn test_check_localhost_binding_fails_wildcard() {
        let dir = tempfile::tempdir().unwrap();
        let zenoh_dir = dir.path().join("zenoh");
        std::fs::create_dir_all(&zenoh_dir).unwrap();
        std::fs::write(
            zenoh_dir.join("zenohd.json5"),
            r#"{ listen: { endpoints: ["tcp/0.0.0.0:7447"] } }"#,
        )
        .unwrap();

        let result = check_localhost_binding(dir.path());
        assert!(
            !result.passed,
            "Expected localhost check to fail for 0.0.0.0"
        );
    }

    #[test]
    fn test_check_acl_status_passes() {
        let dir = tempfile::tempdir().unwrap();
        let zenoh_dir = dir.path().join("zenoh");
        std::fs::create_dir_all(&zenoh_dir).unwrap();
        std::fs::write(
            zenoh_dir.join("zenohd.json5"),
            r#"{ access_control: { enabled: true } }"#,
        )
        .unwrap();

        let result = check_acl_status(dir.path());
        assert!(result.passed, "Expected ACL check to pass");
    }

    #[test]
    fn test_check_acl_status_fails() {
        let dir = tempfile::tempdir().unwrap();
        let zenoh_dir = dir.path().join("zenoh");
        std::fs::create_dir_all(&zenoh_dir).unwrap();
        std::fs::write(zenoh_dir.join("zenohd.json5"), r#"{ mode: "router" }"#).unwrap();

        let result = check_acl_status(dir.path());
        assert!(!result.passed, "Expected ACL check to fail");
    }

    #[test]
    fn test_check_node_names_valid() {
        let dir = tempfile::tempdir().unwrap();
        std::fs::write(
            dir.path().join("nodes.json"),
            r#"{"nodes": [{"name": "camera"}, {"name": "rtsp-cam"}]}"#,
        )
        .unwrap();

        let results = check_node_names(dir.path());
        assert_eq!(results.len(), 2);
        assert!(results.iter().all(|r| r.passed));
    }

    #[test]
    fn test_check_node_names_invalid() {
        let dir = tempfile::tempdir().unwrap();
        std::fs::write(
            dir.path().join("nodes.json"),
            r#"{"nodes": [{"name": "good-node"}, {"name": "bad node!"}]}"#,
        )
        .unwrap();

        let results = check_node_names(dir.path());
        assert_eq!(results.len(), 2);
        assert!(results[0].passed);
        assert!(!results[1].passed);
    }
}
