use std::time::Duration;

use anyhow::{anyhow, Result};
use serde::{Deserialize, Serialize};
use tokio::process::Command;
use zenoh::Session;

const API_HEALTH: &str = "bubbaloop/daemon/api/health";
const API_NODES: &str = "bubbaloop/daemon/api/nodes";

#[derive(Debug, Serialize, Deserialize)]
struct HealthResponse {
    status: String,
}

#[derive(Debug, Serialize, Deserialize)]
struct NodeState {
    name: String,
    #[allow(dead_code)]
    path: String,
    status: String,
    #[allow(dead_code)]
    installed: bool,
    #[allow(dead_code)]
    autostart_enabled: bool,
    version: String,
    #[allow(dead_code)]
    description: String,
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

#[derive(Debug, Serialize)]
struct StatusOutput {
    services: Vec<ServiceStatus>,
    daemon: DaemonStatus,
    nodes: Vec<NodeStatus>,
}

#[derive(Debug, Serialize)]
struct ServiceStatus {
    name: String,
    status: String,
}

#[derive(Debug, Serialize)]
struct DaemonStatus {
    connected: bool,
}

#[derive(Debug, Serialize)]
struct NodeStatus {
    name: String,
    status: String,
    node_type: String,
    version: String,
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

async fn query_daemon<T: for<'de> Deserialize<'de>>(
    session: &Session,
    key_expr: &str,
) -> Result<T> {
    let replies = session
        .get(key_expr)
        .timeout(Duration::from_secs(3))
        .await
        .map_err(|e| anyhow!("Zenoh query failed: {}", e))?;

    while let Ok(reply) = replies.recv_async().await {
        if let Ok(sample) = reply.result() {
            let bytes = sample.payload().to_bytes();
            let text = String::from_utf8_lossy(&bytes);
            let result: T = serde_json::from_str(&text)?;
            return Ok(result);
        }
    }

    Err(anyhow!("No reply received for {}", key_expr))
}

async fn check_daemon_health(session: &Session) -> bool {
    match query_daemon::<HealthResponse>(session, API_HEALTH).await {
        Ok(response) => response.status == "ok",
        Err(_) => false,
    }
}

async fn get_nodes(session: &Session) -> Result<Vec<NodeState>> {
    let response: NodeListResponse = query_daemon(session, API_NODES).await?;
    Ok(response.nodes)
}

fn format_table(output: &StatusOutput) {
    println!("Bubbaloop Status");
    println!("================");
    println!();
    println!("Services:");
    for service in &output.services {
        let symbol = if service.status == "running" {
            "●"
        } else {
            "○"
        };
        println!("  {} {:<18} {}", symbol, service.name, service.status);
    }
    println!();
    println!(
        "Daemon: {}",
        if output.daemon.connected {
            "connected"
        } else {
            "disconnected"
        }
    );
    println!();
    println!("Nodes:");
    println!("  {:<15} {:<8} {:<6} VERSION", "NAME", "STATUS", "TYPE");
    for node in &output.nodes {
        println!(
            "  {:<15} {:<8} {:<6} {}",
            node.name, node.status, node.node_type, node.version
        );
    }
}

fn format_json(output: &StatusOutput) -> Result<()> {
    let json = serde_json::to_string_pretty(output)?;
    println!("{}", json);
    Ok(())
}

fn format_yaml(output: &StatusOutput) -> Result<()> {
    let yaml = serde_yaml::to_string(output)?;
    println!("{}", yaml);
    Ok(())
}

pub async fn run(format: &str) -> Result<()> {
    // Connect to Zenoh with explicit endpoint
    let mut config = zenoh::Config::default();

    // Run as client mode - only connect to router, don't listen
    config.insert_json5("mode", "\"client\"").ok();

    let endpoint = std::env::var("BUBBALOOP_ZENOH_ENDPOINT")
        .unwrap_or_else(|_| "tcp/127.0.0.1:7447".to_string());
    config
        .insert_json5("connect/endpoints", &format!("[\"{}\"]", endpoint))
        .ok();

    // Disable all scouting to avoid connecting to remote peers via Tailscale
    config
        .insert_json5("scouting/multicast/enabled", "false")
        .ok();
    config.insert_json5("scouting/gossip/enabled", "false").ok();

    let session = zenoh::open(config)
        .await
        .map_err(|e| anyhow!("Failed to connect to zenoh: {}", e))?;

    // Check systemd services
    let services = vec![
        ServiceStatus {
            name: "zenohd".to_string(),
            status: check_systemd_service("zenohd.service").await,
        },
        ServiceStatus {
            name: "zenoh-bridge".to_string(),
            status: check_systemd_service("zenoh-bridge.service").await,
        },
        ServiceStatus {
            name: "bubbaloop-daemon".to_string(),
            status: check_systemd_service("bubbaloop-daemon.service").await,
        },
    ];

    // Check daemon health
    let daemon_connected = check_daemon_health(&session).await;

    // Get nodes
    let nodes = if daemon_connected {
        match get_nodes(&session).await {
            Ok(node_states) => node_states
                .into_iter()
                .map(|n| NodeStatus {
                    name: n.name,
                    status: n.status,
                    node_type: n.node_type,
                    version: n.version,
                })
                .collect(),
            Err(_) => Vec::new(),
        }
    } else {
        Vec::new()
    };

    let output = StatusOutput {
        services,
        daemon: DaemonStatus {
            connected: daemon_connected,
        },
        nodes,
    };

    // Format output
    match format {
        "json" => format_json(&output)?,
        "yaml" => format_yaml(&output)?,
        _ => format_table(&output),
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_service_status_serialization() {
        let status = ServiceStatus {
            name: "zenohd".to_string(),
            status: "running".to_string(),
        };

        let json = serde_json::to_string(&status).unwrap();
        assert!(json.contains("zenohd"));
        assert!(json.contains("running"));
    }

    #[test]
    fn test_daemon_status_serialization() {
        let status = DaemonStatus { connected: true };
        let json = serde_json::to_string(&status).unwrap();
        assert!(json.contains("true"));
    }

    #[test]
    fn test_node_status_serialization() {
        let status = NodeStatus {
            name: "test-node".to_string(),
            status: "running".to_string(),
            node_type: "rust".to_string(),
            version: "1.0.0".to_string(),
        };

        let json = serde_json::to_string(&status).unwrap();
        assert!(json.contains("test-node"));
        assert!(json.contains("running"));
        assert!(json.contains("rust"));
    }

    #[test]
    fn test_status_output_json_formatting() {
        let output = StatusOutput {
            services: vec![ServiceStatus {
                name: "zenohd".to_string(),
                status: "running".to_string(),
            }],
            daemon: DaemonStatus { connected: true },
            nodes: vec![NodeStatus {
                name: "test".to_string(),
                status: "active".to_string(),
                node_type: "rust".to_string(),
                version: "1.0.0".to_string(),
            }],
        };

        let result = format_json(&output);
        assert!(result.is_ok());
    }

    #[test]
    fn test_status_output_yaml_formatting() {
        let output = StatusOutput {
            services: vec![ServiceStatus {
                name: "zenohd".to_string(),
                status: "running".to_string(),
            }],
            daemon: DaemonStatus { connected: false },
            nodes: vec![],
        };

        let result = format_yaml(&output);
        assert!(result.is_ok());
    }

    #[test]
    fn test_health_response_deserialization() {
        let json = r#"{"status": "ok"}"#;
        let response: HealthResponse = serde_json::from_str(json).unwrap();
        assert_eq!(response.status, "ok");
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
