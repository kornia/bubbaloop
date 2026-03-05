//! Shared system utility functions for CLI commands.
//!
//! Provides common process and service inspection helpers used by
//! `doctor` and `status` commands.

use tokio::process::Command;

/// Returns true if a process with the given name is running.
///
/// Uses `pgrep -x <name>` for an exact-name match.
pub async fn is_process_running(name: &str) -> bool {
    let output = Command::new("pgrep").arg("-x").arg(name).output().await;
    matches!(output, Ok(out) if out.status.success())
}

/// Returns the raw systemd active state for a user service (e.g. "active", "inactive", "failed").
///
/// Returns `"unknown"` if `systemctl` cannot be invoked.
pub async fn check_systemd_service(service_name: &str) -> String {
    let output = Command::new("systemctl")
        .args(["--user", "is-active", service_name])
        .output()
        .await;

    match output {
        Ok(out) => String::from_utf8_lossy(&out.stdout).trim().to_string(),
        Err(_) => "unknown".to_string(),
    }
}
