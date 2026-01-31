//! Status command for quick system overview
//!
//! Provides a concise view of the bubbaloop system status:
//! - Zenoh router (running/stopped, port)
//! - Daemon (running/stopped, node count)
//! - Bridge (running/stopped)
//! - Node summary (running/stopped/not-installed counts)
//!
//! Supports --json for machine-readable output.

use std::time::Duration;

use anyhow::{anyhow, Result};
use serde::{Deserialize, Serialize};
use tokio::process::Command;
use zenoh::query::QueryTarget;
use zenoh::Session;

const API_NODES: &str = "bubbaloop/daemon/api/nodes";

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

#[derive(Debug, Serialize, Deserialize)]
struct NodeState {
    #[allow(dead_code)]
    name: String,
    #[allow(dead_code)]
    path: String,
    status: String,
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

#[derive(Debug, Serialize, Deserialize)]
struct NodeListResponse {
    nodes: Vec<NodeState>,
    #[allow(dead_code)]
    timestamp_ms: u64,
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

async fn query_nodes(session: &Session) -> Result<NodeListResponse> {
    let replies = session
        .get(API_NODES)
        .target(QueryTarget::BestMatching)
        .timeout(Duration::from_secs(5))
        .await
        .map_err(|e| anyhow!("Zenoh query failed: {}", e))?;

    let timeout_duration = Duration::from_secs(5);
    let start = std::time::Instant::now();

    while start.elapsed() < timeout_duration {
        match tokio::time::timeout(timeout_duration - start.elapsed(), replies.recv_async()).await {
            Ok(Ok(reply)) => {
                if let Ok(sample) = reply.result() {
                    let bytes = sample.payload().to_bytes();
                    let result: NodeListResponse = serde_json::from_slice(&bytes)?;
                    return Ok(result);
                }
            }
            Ok(Err(_)) | Err(_) => break,
        }
    }

    Err(anyhow!("No reply received from daemon (timeout after 5s)"))
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
    // Check Zenoh router
    let zenoh_running = is_process_running("zenohd").await;
    let zenoh = ZenohStatus {
        running: zenoh_running,
        port: 7447,
    };

    // Check daemon
    let daemon_service = check_systemd_service("bubbaloop-daemon.service").await;
    let daemon_running = daemon_service == "active";

    // Try to get node count from daemon if it's running
    let mut node_count = None;
    let mut node_summary = NodeSummary {
        running: 0,
        stopped: 0,
        not_installed: 0,
    };

    if daemon_running && zenoh_running {
        if let Ok(session) = get_zenoh_session().await {
            if let Ok(response) = query_nodes(&session).await {
                node_count = Some(response.nodes.len());

                // Count node states
                for node in response.nodes {
                    if !node.installed {
                        node_summary.not_installed += 1;
                    } else if node.status.to_lowercase() == "running" {
                        node_summary.running += 1;
                    } else {
                        node_summary.stopped += 1;
                    }
                }
            }
            let _ = session.close().await;
        }
    }

    let daemon = DaemonStatus {
        running: daemon_running,
        nodes: node_count,
    };

    // Check bridge
    let bridge_service = check_systemd_service("bubbaloop-bridge.service").await;
    let bridge = BridgeStatus {
        running: bridge_service == "active",
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
    fn test_node_state_deserialization() {
        let json = r#"{
            "name": "rtsp-camera",
            "path": "/path/to/node",
            "status": "running",
            "installed": true,
            "autostart_enabled": true,
            "version": "0.1.0",
            "description": "RTSP Camera Node",
            "node_type": "rust",
            "is_built": true,
            "build_output": ["Building...", "Done"]
        }"#;

        let node: NodeState = serde_json::from_str(json).unwrap();
        assert_eq!(node.name, "rtsp-camera");
        assert_eq!(node.status, "running");
        assert_eq!(node.node_type, "rust");
        assert!(node.installed);
        assert!(node.is_built);
    }

    #[test]
    fn test_node_list_response_deserialization() {
        let json = r#"{
            "nodes": [
                {
                    "name": "node1",
                    "path": "/path1",
                    "status": "running",
                    "installed": true,
                    "autostart_enabled": false,
                    "version": "1.0.0",
                    "description": "First node",
                    "node_type": "rust",
                    "is_built": true,
                    "build_output": []
                }
            ],
            "timestamp_ms": 1234567890
        }"#;

        let response: NodeListResponse = serde_json::from_str(json).unwrap();
        assert_eq!(response.nodes.len(), 1);
        assert_eq!(response.nodes[0].name, "node1");
        assert_eq!(response.timestamp_ms, 1234567890);
    }

    #[test]
    fn test_node_list_response_empty() {
        let json = r#"{"nodes": [], "timestamp_ms": 0}"#;
        let response: NodeListResponse = serde_json::from_str(json).unwrap();
        assert_eq!(response.nodes.len(), 0);
    }
}
