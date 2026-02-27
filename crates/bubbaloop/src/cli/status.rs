//! Status command for quick system overview
//!
//! Provides a concise view of the bubbaloop system status:
//! - Zenoh router (running/stopped, port)
//! - Daemon (running/stopped, node count via MCP /health endpoint)
//! - Bridge (running/stopped)
//! - Node summary (running/stopped/not-installed counts)
//!
//! Supports --json for machine-readable output.

use std::time::Duration;

use anyhow::Result;
use serde::{Deserialize, Serialize};
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::process::Command;

#[derive(Debug, Serialize)]
struct StatusOutput {
    zenoh: ZenohStatus,
    daemon: DaemonStatus,
    bridge: BridgeStatus,
    nodes: NodeSummary,
}

#[derive(Debug, Serialize)]
struct ZenohStatus {
    running: bool,
    port: u16,
}

#[derive(Debug, Serialize)]
struct DaemonStatus {
    running: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    nodes: Option<usize>,
}

#[derive(Debug, Serialize)]
struct BridgeStatus {
    running: bool,
}

#[derive(Debug, Serialize)]
struct NodeSummary {
    running: usize,
    stopped: usize,
    not_installed: usize,
}

#[derive(Debug, Deserialize)]
struct DaemonHealthResponse {
    #[allow(dead_code)]
    status: String,
    nodes_total: usize,
    nodes_running: usize,
}

async fn check_systemd_service(service_name: &str) -> String {
    let output = Command::new("systemctl")
        .args(["--user", "is-active", service_name])
        .output()
        .await;

    match output {
        Ok(out) => {
            let status = String::from_utf8_lossy(&out.stdout).trim().to_string();
            if status == "active" {
                "running".to_string()
            } else {
                status
            }
        }
        Err(_) => "unknown".to_string(),
    }
}

/// Query the daemon's MCP /health endpoint via raw HTTP.
/// Returns None if the daemon is unreachable.
async fn check_daemon_mcp(port: u16) -> Option<DaemonHealthResponse> {
    let addr = format!("127.0.0.1:{}", port);
    let mut stream = tokio::net::TcpStream::connect(&addr).await.ok()?;
    let request = format!(
        "GET /health HTTP/1.1\r\nHost: 127.0.0.1:{}\r\nConnection: close\r\n\r\n",
        port
    );
    stream.write_all(request.as_bytes()).await.ok()?;
    let mut buf = Vec::new();
    tokio::time::timeout(Duration::from_secs(3), stream.read_to_end(&mut buf))
        .await
        .ok()?
        .ok()?;
    let text = String::from_utf8_lossy(&buf);
    let body = text.split("\r\n\r\n").nth(1)?;
    serde_json::from_str(body).ok()
}

fn print_table(status: &StatusOutput) {
    println!("bubbaloop status");
    println!("================");

    // Zenoh Router
    let zenoh_symbol = if status.zenoh.running { "✓" } else { "✗" };
    let zenoh_state = if status.zenoh.running {
        format!("running (port {})", status.zenoh.port)
    } else {
        "stopped".to_string()
    };
    println!("Zenoh Router:  {} {}", zenoh_symbol, zenoh_state);

    // Daemon
    let daemon_symbol = if status.daemon.running { "✓" } else { "✗" };
    let daemon_state = if status.daemon.running {
        if let Some(count) = status.daemon.nodes {
            format!("running ({} nodes)", count)
        } else {
            "running".to_string()
        }
    } else {
        "stopped".to_string()
    };
    println!("Daemon:        {} {}", daemon_symbol, daemon_state);

    // Bridge
    let bridge_symbol = if status.bridge.running { "✓" } else { "✗" };
    let bridge_state = if status.bridge.running {
        "running".to_string()
    } else {
        "stopped".to_string()
    };
    println!("Bridge:        {} {}", bridge_symbol, bridge_state);

    // Node summary
    println!();
    let total = status.nodes.running + status.nodes.stopped + status.nodes.not_installed;
    if total > 0 {
        println!(
            "Nodes: {} running, {} stopped, {} not installed",
            status.nodes.running, status.nodes.stopped, status.nodes.not_installed
        );
    } else {
        println!("Nodes: none registered");
    }
}

fn print_json(status: &StatusOutput) -> Result<()> {
    let json = serde_json::to_string_pretty(status)?;
    println!("{}", json);
    Ok(())
}

pub async fn run(format: &str) -> Result<()> {
    // Collect status information
    let status = collect_status().await?;

    // Output in requested format
    match format {
        "json" => print_json(&status)?,
        "table" => print_table(&status),
        _ => print_table(&status),
    }

    Ok(())
}

async fn collect_status() -> Result<StatusOutput> {
    let zenoh_running = is_process_running("zenohd").await;
    let zenoh = ZenohStatus {
        running: zenoh_running,
        port: 7447,
    };

    let mcp_port = std::env::var("BUBBALOOP_MCP_PORT")
        .ok()
        .and_then(|p| p.parse().ok())
        .unwrap_or(crate::mcp::MCP_PORT);

    // Primary: query MCP /health endpoint. Fallback: systemd service check.
    let (daemon_running, node_count, node_summary) =
        if let Some(health) = check_daemon_mcp(mcp_port).await {
            (
                true,
                Some(health.nodes_total),
                NodeSummary {
                    running: health.nodes_running,
                    stopped: health.nodes_total.saturating_sub(health.nodes_running),
                    not_installed: 0,
                },
            )
        } else {
            let svc = check_systemd_service("bubbaloop-daemon.service").await;
            (
                svc == "running",
                None,
                NodeSummary {
                    running: 0,
                    stopped: 0,
                    not_installed: 0,
                },
            )
        };

    let daemon = DaemonStatus {
        running: daemon_running,
        nodes: node_count,
    };

    let bridge_service = check_systemd_service("bubbaloop-bridge.service").await;
    let bridge = BridgeStatus {
        running: bridge_service == "running",
    };

    Ok(StatusOutput {
        zenoh,
        daemon,
        bridge,
        nodes: node_summary,
    })
}

async fn is_process_running(name: &str) -> bool {
    let output = Command::new("pgrep").arg("-x").arg(name).output().await;

    matches!(output, Ok(out) if out.status.success())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_status_output_serialization() {
        let status = StatusOutput {
            zenoh: ZenohStatus {
                running: true,
                port: 7447,
            },
            daemon: DaemonStatus {
                running: true,
                nodes: Some(3),
            },
            bridge: BridgeStatus { running: false },
            nodes: NodeSummary {
                running: 1,
                stopped: 2,
                not_installed: 0,
            },
        };

        let json = serde_json::to_string(&status).unwrap();
        assert!(json.contains("\"running\":true"));
        assert!(json.contains("\"port\":7447"));
    }

    #[test]
    fn test_zenoh_status_serialization() {
        let status = ZenohStatus {
            running: true,
            port: 7447,
        };
        let json = serde_json::to_string(&status).unwrap();
        assert!(json.contains("true"));
        assert!(json.contains("7447"));
    }

    #[test]
    fn test_daemon_status_serialization() {
        let status = DaemonStatus {
            running: true,
            nodes: Some(5),
        };
        let json = serde_json::to_string(&status).unwrap();
        assert!(json.contains("true"));
        assert!(json.contains("5"));
    }

    #[test]
    fn test_bridge_status_serialization() {
        let status = BridgeStatus { running: false };
        let json = serde_json::to_string(&status).unwrap();
        assert!(json.contains("false"));
    }

    #[test]
    fn test_node_summary_serialization() {
        let summary = NodeSummary {
            running: 1,
            stopped: 2,
            not_installed: 3,
        };
        let json = serde_json::to_string(&summary).unwrap();
        assert!(json.contains("\"running\":1"));
        assert!(json.contains("\"stopped\":2"));
        assert!(json.contains("\"not_installed\":3"));
    }

    #[test]
    fn test_daemon_health_response_deserialization() {
        let json = r#"{"status": "ok", "version": "0.0.6", "nodes_total": 5, "nodes_running": 3}"#;
        let resp: DaemonHealthResponse = serde_json::from_str(json).unwrap();
        assert_eq!(resp.status, "ok");
        assert_eq!(resp.nodes_total, 5);
        assert_eq!(resp.nodes_running, 3);
    }
}
