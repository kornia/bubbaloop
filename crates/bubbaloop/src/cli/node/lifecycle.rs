//! Node lifecycle commands: start, stop, restart, logs.

use std::process::Command;

use super::{send_command, LogsArgs, NodeError, Result};

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
        // Use journalctl directly for follow mode (streaming, no daemon needed)
        let service = format!("bubbaloop-{}.service", args.name);
        let status = Command::new("journalctl")
            .args(["--user", "-u", &service, "-f", "--no-pager"])
            .status()?;

        if !status.success() {
            return Err(NodeError::CommandFailed(format!(
                "journalctl failed for service {}. Is the service installed?",
                service
            )));
        }
        return Ok(());
    }

    // Use REST API for non-follow mode
    super::send_command(&args.name, "logs").await
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
