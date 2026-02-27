//! Node lifecycle commands: start, stop, restart, logs.

use std::process::Command;

use zenoh::query::QueryTarget;

use super::{LogsArgs, LogsResponse, NodeError, Result};
use super::{get_zenoh_session, send_command};

pub(crate) async fn start_node(name: &str) -> Result<()> {
    send_command(name, "start").await
}

pub(crate) async fn stop_node(name: &str) -> Result<()> {
    send_command(name, "stop").await
}

pub(crate) async fn restart_node(name: &str) -> Result<()> {
    send_command(name, "restart").await
}

pub(crate) async fn view_logs(args: LogsArgs) -> Result<()> {
    if args.follow {
        // Use journalctl directly for follow mode
        let service = format!("bubbaloop-{}.service", args.name);
        let status = Command::new("journalctl")
            .args(["--user", "-u", &service, "-f", "--no-pager"])
            .status()?;

        if !status.success() {
            // Fallback to systemctl status
            let _ = Command::new("systemctl")
                .args(["--user", "status", "-l", "--no-pager", &service])
                .status();
        }
        return Ok(());
    }

    let session = get_zenoh_session().await?;

    let key = format!("bubbaloop/daemon/api/nodes/{}/logs", args.name);
    let replies: Vec<_> = session
        .get(&key)
        .target(QueryTarget::BestMatching)
        .timeout(std::time::Duration::from_secs(30))
        .await
        .map_err(|e| NodeError::Zenoh(e.to_string()))?
        .into_iter()
        .collect();

    for reply in replies {
        if let Ok(sample) = reply.into_result() {
            let data: LogsResponse = serde_json::from_slice(&sample.payload().to_bytes())?;
            if data.success {
                for line in data.lines.iter().take(args.lines) {
                    println!("{}", line);
                }
            } else if let Some(error) = data.error {
                return Err(NodeError::CommandFailed(error));
            }
        }
    }

    session
        .close()
        .await
        .map_err(|e| NodeError::Zenoh(e.to_string()))?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::super::LogsResponse;

    #[test]
    fn test_logs_response_deserialization() {
        let json = r#"{"lines": ["line1", "line2"], "success": true}"#;
        let response: LogsResponse = serde_json::from_str(json).unwrap();
        assert!(response.success);
        assert_eq!(response.lines.len(), 2);
        assert_eq!(response.lines[0], "line1");
    }

    #[test]
    fn test_logs_response_with_error() {
        let json = r#"{"lines": [], "success": false, "error": "Node not found"}"#;
        let response: LogsResponse = serde_json::from_str(json).unwrap();
        assert!(!response.success);
        assert_eq!(response.error, Some("Node not found".to_string()));
    }
}
