//! Exec driver — run a shell command on interval, publish stdout bytes.

use super::{BuiltInContext, BuiltInDriver};
use std::time::Duration;

pub struct ExecDriver;

/// Allowlist: only these command binary names are permitted.
/// Allowlist: only these non-shell command binary names are permitted.
/// Shells (bash, sh, zsh) are intentionally excluded — a `command: bash -c '...'`
/// config would bypass all allowlist protection.
const EXEC_ALLOWLIST: &[&str] = &[
    "python", "python3", "node", "cat", "echo", "curl", "jq", "date",
];

fn is_allowed_command(cmd: &str) -> bool {
    let first = cmd.split_whitespace().next().unwrap_or("");
    // Strip path prefix so "/usr/bin/python3" matches "python3"
    let binary = std::path::Path::new(first)
        .file_name()
        .and_then(|n| n.to_str())
        .unwrap_or(first);
    EXEC_ALLOWLIST.contains(&binary)
}

impl BuiltInDriver for ExecDriver {
    async fn run(self, mut ctx: BuiltInContext) -> anyhow::Result<()> {
        let command = ctx
            .config
            .get("command")
            .and_then(|v| v.as_str())
            .ok_or_else(|| anyhow::anyhow!("exec: missing required config key 'command'"))?
            .to_string();

        if !is_allowed_command(&command) {
            return Err(anyhow::anyhow!(
                "exec: command '{}' is not in the allowlist. Allowed: {:?}",
                command,
                EXEC_ALLOWLIST
            ));
        }

        let interval_secs = ctx
            .config
            .get("interval_secs")
            .and_then(|v| v.as_u64())
            .unwrap_or(60);

        let data_topic = ctx.topic("data");
        let health_topic = ctx.topic("health");
        let data_pub = ctx
            .session
            .declare_publisher(&data_topic)
            .await
            .map_err(|e| anyhow::anyhow!("exec: publisher error: {}", e))?;
        let health_pub = ctx
            .session
            .declare_publisher(&health_topic)
            .await
            .map_err(|e| anyhow::anyhow!("exec: health publisher error: {}", e))?;

        let mut ticker = tokio::time::interval(Duration::from_secs(interval_secs));
        let mut health_ticker = tokio::time::interval(Duration::from_secs(5));

        log::info!(
            "[exec] {} starting, command={:?}, interval={}s",
            ctx.skill_name,
            command,
            interval_secs
        );

        loop {
            tokio::select! {
                _ = ticker.tick() => {
                    let mut parts = command.split_whitespace();
                    let program = match parts.next() {
                        Some(p) => p,
                        None => continue,
                    };
                    let args: Vec<&str> = parts.collect();

                    match tokio::process::Command::new(program)
                        .args(&args)
                        .output()
                        .await
                    {
                        Ok(output) => {
                            if !output.stdout.is_empty() {
                                if let Err(e) = data_pub.put(output.stdout).await {
                                    log::warn!("[exec] {}: publish error: {}", ctx.skill_name, e);
                                }
                            }
                            if !output.status.success() {
                                log::warn!(
                                    "[exec] {}: command exited with {}",
                                    ctx.skill_name,
                                    output.status
                                );
                            }
                        }
                        Err(e) => log::warn!("[exec] {}: command error: {}", ctx.skill_name, e),
                    }
                }
                _ = health_ticker.tick() => {
                    if let Err(e) = health_pub.put(b"ok".to_vec()).await {
                        log::warn!("[exec] {}: health publish error: {}", ctx.skill_name, e);
                    }
                }
                _ = ctx.shutdown_rx.changed() => {
                    log::info!("[exec] {} shutting down", ctx.skill_name);
                    break;
                }
            }
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn allowlist_accepts_known_binaries() {
        assert!(is_allowed_command("echo hello"));
        assert!(is_allowed_command("python3 /tmp/script.py"));
        assert!(is_allowed_command("date"));
        assert!(is_allowed_command("jq '.foo'"));
    }

    #[test]
    fn allowlist_rejects_unknown_binaries() {
        assert!(!is_allowed_command("rm -rf /"));
        assert!(!is_allowed_command("nc -l 4444"));
        assert!(!is_allowed_command("wget http://example.com"));
        assert!(!is_allowed_command("sudo bash"));
        // Shells are rejected — they bypass argument-level allowlisting
        assert!(!is_allowed_command("bash -c 'rm -rf /'"));
        assert!(!is_allowed_command("sh -c 'echo bad'"));
        assert!(!is_allowed_command(""));
    }

    #[test]
    fn allowlist_handles_path_prefix() {
        // Strip /usr/bin/ prefix before matching
        assert!(is_allowed_command("/usr/bin/python3 script.py"));
        assert!(!is_allowed_command("/usr/bin/wget http://x.com"));
    }
}
