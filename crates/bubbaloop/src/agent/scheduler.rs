//! Cron-based scheduler with Tier 1 built-in actions.
//!
//! Tier 1 actions run offline without an LLM — health checks, node
//! start/stop, log events, etc. The scheduler reads schedules from
//! the SQLite memory layer, evaluates cron expressions, and executes
//! due actions on each tick (every 60 seconds).

use crate::agent::memory::Memory;
use crate::mcp::platform::{NodeCommand, PlatformOperations};
use serde::Deserialize;
use serde_json::Value;
use std::path::PathBuf;
use std::str::FromStr;
use std::sync::{Arc, Mutex};
use tokio::sync::watch;

/// Errors from scheduler operations.
#[derive(Debug, thiserror::Error)]
pub enum SchedulerError {
    #[error("Memory error: {0}")]
    Memory(#[from] crate::agent::memory::MemoryError),
    #[error("Invalid cron expression: {0}")]
    CronParse(String),
    #[error("Action failed: {0}")]
    Action(String),
}

type Result<T> = std::result::Result<T, SchedulerError>;

/// A Tier 1 action that can be executed without an LLM.
#[derive(Debug, Clone, Deserialize)]
#[serde(tag = "action")]
pub enum Tier1Action {
    /// Check health of all nodes and log results.
    #[serde(rename = "check_all_health")]
    CheckAllHealth,

    /// Restart any unhealthy running nodes.
    #[serde(rename = "restart")]
    Restart,

    /// Start a specific node.
    #[serde(rename = "start_node")]
    StartNode { node: String },

    /// Stop a specific node.
    #[serde(rename = "stop_node")]
    StopNode { node: String },

    /// Send a Zenoh command to a node.
    #[serde(rename = "send_command")]
    SendCommand {
        node: String,
        command: String,
        params: Value,
    },

    /// Capture a frame from a camera node.
    #[serde(rename = "capture_frame")]
    CaptureFrame { node: String, params: Value },

    /// Log an event to the memory store.
    #[serde(rename = "log_event")]
    LogEvent { message: String },

    /// Print a notification to the terminal.
    #[serde(rename = "notify")]
    Notify { message: String },
}

/// Compute the next cron occurrence after the given epoch seconds.
///
/// The `cron` crate expects 6-field expressions (sec min hr dom month dow).
/// Standard 5-field cron expressions are detected and "0 " is prepended
/// automatically to pin the seconds field to zero.
pub fn next_run_after(cron_expr: &str, after_epoch_secs: u64) -> Result<u64> {
    // Detect standard 5-field cron and prepend "0 " for seconds.
    let expr = normalize_cron_expr(cron_expr);

    let schedule = cron::Schedule::from_str(&expr)
        .map_err(|e| SchedulerError::CronParse(format!("{}: {}", cron_expr, e)))?;

    let after_dt =
        chrono::DateTime::from_timestamp(after_epoch_secs as i64, 0).ok_or_else(|| {
            SchedulerError::CronParse(format!("invalid epoch seconds: {}", after_epoch_secs))
        })?;

    let next = schedule
        .after(&after_dt)
        .next()
        .ok_or_else(|| SchedulerError::CronParse("no next occurrence".to_string()))?;

    Ok(next.timestamp() as u64)
}

/// Normalise a cron expression to 6-field format.
///
/// Standard cron has 5 fields (min hr dom month dow). The `cron` crate
/// expects 6 fields (sec min hr dom month dow). If the expression has
/// exactly 5 whitespace-separated fields, we prepend "0 " to pin
/// the seconds to zero.
fn normalize_cron_expr(expr: &str) -> String {
    let trimmed = expr.trim();
    let fields: Vec<&str> = trimmed.split_whitespace().collect();
    if fields.len() == 5 {
        format!("0 {}", trimmed)
    } else {
        trimmed.to_string()
    }
}

/// Execute a single Tier 1 action.
///
/// Memory is wrapped in a `Mutex` and locked only for the brief
/// synchronous calls that need it, never held across `.await` points.
pub async fn execute_tier1_action<P: PlatformOperations>(
    action: &Tier1Action,
    platform: &P,
    memory: &Mutex<Memory>,
    scope: &str,
    machine_id: &str,
) -> Result<()> {
    match action {
        Tier1Action::CheckAllHealth => {
            let nodes = platform
                .list_nodes()
                .await
                .map_err(|e| SchedulerError::Action(e.to_string()))?;
            let mem = memory
                .lock()
                .map_err(|e| SchedulerError::Action(format!("memory lock poisoned: {}", e)))?;
            for node in &nodes {
                log::info!(
                    "[Scheduler] health check: node={} status={} health={}",
                    node.name,
                    node.status,
                    node.health
                );
                let details = format!(
                    r#"{{"status":"{}","health":"{}"}}"#,
                    node.status, node.health
                );
                if let Err(e) = mem.log_event(&node.name, "health_check", Some(&details)) {
                    log::warn!("[Scheduler] failed to log health event: {}", e);
                }
            }
            Ok(())
        }

        Tier1Action::Restart => {
            let nodes = platform
                .list_nodes()
                .await
                .map_err(|e| SchedulerError::Action(e.to_string()))?;
            for node in &nodes {
                // Only restart nodes that are running but unhealthy
                if node.status == "Running" && node.health != "Healthy" {
                    log::info!(
                        "[Scheduler] restarting unhealthy node: {} (health={})",
                        node.name,
                        node.health
                    );
                    if let Err(e) = platform
                        .execute_command(&node.name, NodeCommand::Restart)
                        .await
                    {
                        log::error!("[Scheduler] restart failed for {}: {}", node.name, e);
                    }
                }
            }
            Ok(())
        }

        Tier1Action::StartNode { node } => {
            platform
                .execute_command(node, NodeCommand::Start)
                .await
                .map_err(|e| SchedulerError::Action(e.to_string()))?;
            log::info!("[Scheduler] started node: {}", node);
            Ok(())
        }

        Tier1Action::StopNode { node } => {
            platform
                .execute_command(node, NodeCommand::Stop)
                .await
                .map_err(|e| SchedulerError::Action(e.to_string()))?;
            log::info!("[Scheduler] stopped node: {}", node);
            Ok(())
        }

        Tier1Action::SendCommand {
            node,
            command,
            params,
        } => {
            let key_expr = format!(
                "bubbaloop/{}/{}/{}/command/{}",
                scope, machine_id, node, command
            );
            let payload =
                serde_json::to_vec(params).map_err(|e| SchedulerError::Action(e.to_string()))?;
            platform
                .send_zenoh_query(&key_expr, payload)
                .await
                .map_err(|e| SchedulerError::Action(e.to_string()))?;
            log::info!("[Scheduler] sent command {} to node {}", command, node);
            Ok(())
        }

        Tier1Action::CaptureFrame { node, params } => {
            let key_expr = format!(
                "bubbaloop/{}/{}/{}/command/capture_frame",
                scope, machine_id, node
            );
            let payload =
                serde_json::to_vec(params).map_err(|e| SchedulerError::Action(e.to_string()))?;
            platform
                .send_zenoh_query(&key_expr, payload)
                .await
                .map_err(|e| SchedulerError::Action(e.to_string()))?;
            log::info!("[Scheduler] capture_frame sent to node {}", node);
            Ok(())
        }

        Tier1Action::LogEvent { message } => {
            let mem = memory
                .lock()
                .map_err(|e| SchedulerError::Action(format!("memory lock poisoned: {}", e)))?;
            mem.log_event("scheduler", "log_event", Some(message))
                .map_err(SchedulerError::Memory)?;
            log::info!("[Scheduler] logged event: {}", message);
            Ok(())
        }

        Tier1Action::Notify { message } => {
            println!("[Scheduler] {}", message);
            log::info!("[Scheduler] notification: {}", message);
            Ok(())
        }
    }
}

/// Run the scheduler background loop.
///
/// Ticks every 60 seconds, checking for due Tier 1 schedules.
/// Opens its own SQLite connection from `memory_path`. The `Memory`
/// is wrapped in a `Mutex` so the future is `Send` (rusqlite
/// `Connection` is `Send` but not `Sync`).
/// Exits cleanly when the shutdown signal fires.
pub async fn run_scheduler<P: PlatformOperations>(
    memory_path: PathBuf,
    platform: Arc<P>,
    scope: String,
    machine_id: String,
    mut shutdown: watch::Receiver<()>,
) {
    let memory = match Memory::open(&memory_path) {
        Ok(m) => Arc::new(Mutex::new(m)),
        Err(e) => {
            log::error!("[Scheduler] failed to open memory DB: {}", e);
            return;
        }
    };

    log::info!("[Scheduler] starting background loop (60s interval)");

    let mut interval = tokio::time::interval(std::time::Duration::from_secs(60));
    // First tick fires immediately; consume it to avoid double-tick on startup.
    interval.tick().await;

    loop {
        tokio::select! {
            _ = interval.tick() => {
                if let Err(e) = tick(&memory, &*platform, &scope, &machine_id).await {
                    log::error!("[Scheduler] tick error: {}", e);
                }
            }
            _ = shutdown.changed() => {
                log::info!("[Scheduler] shutdown signal received, exiting");
                break;
            }
        }
    }
}

/// Collect due schedules from the memory DB (sync, under lock).
fn collect_due_schedules(
    memory: &Mutex<Memory>,
    now_secs: u64,
) -> Result<Vec<crate::agent::memory::Schedule>> {
    let mem = memory
        .lock()
        .map_err(|e| SchedulerError::Action(format!("memory lock poisoned: {}", e)))?;
    let schedules = mem.list_schedules()?;
    Ok(schedules
        .into_iter()
        .filter(|s| {
            if s.tier != 1 {
                return false;
            }
            match &s.next_run {
                Some(next_run_str) => {
                    let next_run: u64 = next_run_str.parse().unwrap_or(0);
                    next_run <= now_secs
                }
                None => true,
            }
        })
        .collect())
}

/// Update schedule run timestamps in the memory DB (sync, under lock).
fn update_schedule_after_run(memory: &Mutex<Memory>, name: &str, now_secs: u64, cron_expr: &str) {
    let last_run = format!("{}", now_secs);
    match next_run_after(cron_expr, now_secs) {
        Ok(next) => {
            let next_run = format!("{}", next);
            let mem = match memory.lock() {
                Ok(m) => m,
                Err(e) => {
                    log::error!("[Scheduler] memory lock poisoned: {}", e);
                    return;
                }
            };
            if let Err(e) = mem.update_schedule_run(name, &last_run, &next_run) {
                log::error!("[Scheduler] failed to update schedule '{}': {}", name, e);
            }
        }
        Err(e) => {
            log::error!(
                "[Scheduler] failed to compute next run for '{}': {}",
                name,
                e
            );
        }
    }
}

/// Check all schedules and execute due Tier 1 jobs.
///
/// Memory access is done synchronously under a `Mutex` lock, ensuring
/// the lock is not held across `.await` points.
async fn tick<P: PlatformOperations>(
    memory: &Mutex<Memory>,
    platform: &P,
    scope: &str,
    machine_id: &str,
) -> Result<()> {
    let now_secs = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs();

    let due_schedules = collect_due_schedules(memory, now_secs)?;

    for sched in &due_schedules {
        log::info!("[Scheduler] executing schedule: {}", sched.name);

        // Parse actions JSON into Vec<Tier1Action>
        let actions: Vec<Tier1Action> = serde_json::from_str(&sched.actions).map_err(|e| {
            SchedulerError::Action(format!(
                "failed to parse actions for schedule '{}': {}",
                sched.name, e
            ))
        })?;

        // Execute each action — memory is locked briefly inside each action
        for action in &actions {
            if let Err(e) = execute_tier1_action(action, platform, memory, scope, machine_id).await
            {
                log::error!(
                    "[Scheduler] action failed in schedule '{}': {}",
                    sched.name,
                    e
                );
            }
        }

        // Update run timestamps
        update_schedule_after_run(memory, &sched.name, now_secs, &sched.cron);
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_tier1_actions() {
        let json = r#"[
            {"action": "check_all_health"},
            {"action": "restart"},
            {"action": "log_event", "message": "patrol complete"},
            {"action": "notify", "message": "hello"}
        ]"#;
        let actions: Vec<Tier1Action> = serde_json::from_str(json).unwrap();
        assert_eq!(actions.len(), 4);
        assert!(matches!(actions[0], Tier1Action::CheckAllHealth));
        assert!(matches!(actions[1], Tier1Action::Restart));
        assert!(matches!(actions[2], Tier1Action::LogEvent { .. }));
        assert!(matches!(actions[3], Tier1Action::Notify { .. }));
    }

    #[test]
    fn parse_start_node_action() {
        let json = r#"[{"action": "start_node", "node": "rtsp-camera"}]"#;
        let actions: Vec<Tier1Action> = serde_json::from_str(json).unwrap();
        assert_eq!(actions.len(), 1);
        match &actions[0] {
            Tier1Action::StartNode { node } => assert_eq!(node, "rtsp-camera"),
            other => panic!("expected StartNode, got {:?}", other),
        }
    }

    #[test]
    fn parse_send_command_action() {
        let json = r#"[{
            "action": "send_command",
            "node": "rtsp-camera",
            "command": "set_exposure",
            "params": {"value": 100}
        }]"#;
        let actions: Vec<Tier1Action> = serde_json::from_str(json).unwrap();
        assert_eq!(actions.len(), 1);
        match &actions[0] {
            Tier1Action::SendCommand {
                node,
                command,
                params,
            } => {
                assert_eq!(node, "rtsp-camera");
                assert_eq!(command, "set_exposure");
                assert_eq!(params["value"], 100);
            }
            other => panic!("expected SendCommand, got {:?}", other),
        }
    }

    #[test]
    fn parse_unknown_action_fails() {
        let json = r#"[{"action": "nonexistent_action"}]"#;
        let result: std::result::Result<Vec<Tier1Action>, _> = serde_json::from_str(json);
        assert!(result.is_err());
    }

    #[test]
    fn next_run_after_valid_cron() {
        // Standard 5-field: every 15 minutes
        let after = 1_700_000_000; // 2023-11-14T22:13:20Z
        let next = next_run_after("*/15 * * * *", after).unwrap();
        assert!(next > after, "next ({}) should be after ({})", next, after);
        // Should be within 15 minutes
        assert!(
            next <= after + 15 * 60,
            "next ({}) should be within 15 minutes of after ({})",
            next,
            after
        );
    }

    #[test]
    fn next_run_after_six_field_cron() {
        // 6-field cron: every 30 seconds
        let after = 1_700_000_000;
        let next = next_run_after("*/30 * * * * *", after).unwrap();
        assert!(next > after);
        assert!(next <= after + 30);
    }

    #[test]
    fn next_run_after_invalid_cron() {
        let result = next_run_after("not a cron expression", 1_700_000_000);
        assert!(result.is_err());
        match result {
            Err(SchedulerError::CronParse(_)) => {}
            other => panic!("expected CronParse error, got {:?}", other),
        }
    }

    #[test]
    fn normalize_prepends_seconds_for_5_fields() {
        assert_eq!(normalize_cron_expr("*/15 * * * *"), "0 */15 * * * *");
    }

    #[test]
    fn normalize_keeps_6_fields_unchanged() {
        assert_eq!(normalize_cron_expr("0 */15 * * * *"), "0 */15 * * * *");
    }
}
