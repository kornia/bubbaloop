//! Job poller and Tier 1 actions — integrated with heartbeat.
//!
//! Unlike the old scheduler (standalone 60s background loop), the new
//! scheduler is called from the heartbeat loop on each beat. Jobs are
//! picked up from SQLite when `status = 'pending' AND next_run_at <= now`.

use crate::mcp::platform::{NodeCommand, PlatformOperations};
use serde::Deserialize;
use serde_json::Value;
use std::str::FromStr;

/// Errors from scheduler operations.
#[derive(Debug, thiserror::Error)]
pub enum SchedulerError {
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
    #[serde(rename = "check_all_health")]
    CheckAllHealth,

    #[serde(rename = "restart")]
    Restart,

    #[serde(rename = "start_node")]
    StartNode { node: String },

    #[serde(rename = "stop_node")]
    StopNode { node: String },

    #[serde(rename = "send_command")]
    SendCommand {
        node: String,
        command: String,
        params: Value,
    },

    #[serde(rename = "capture_frame")]
    CaptureFrame { node: String, params: Value },

    #[serde(rename = "log_event")]
    LogEvent { message: String },

    #[serde(rename = "notify")]
    Notify { message: String },
}

/// Compute the next cron occurrence after the given epoch seconds.
pub fn next_run_after(cron_expr: &str, after_epoch_secs: u64) -> Result<u64> {
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

/// Get current epoch seconds.
pub fn now_epoch_secs() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs()
}

/// Normalise a cron expression to 6-field format.
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
pub async fn execute_tier1_action<P: PlatformOperations>(
    action: &Tier1Action,
    platform: &P,
    scope: &str,
    machine_id: &str,
) -> Result<()> {
    match action {
        Tier1Action::CheckAllHealth => {
            let nodes = platform
                .list_nodes()
                .await
                .map_err(|e| SchedulerError::Action(e.to_string()))?;
            for node in &nodes {
                log::info!(
                    "[Scheduler] health check: node={} status={} health={}",
                    node.name,
                    node.status,
                    node.health
                );
            }
            Ok(())
        }

        Tier1Action::Restart => {
            let nodes = platform
                .list_nodes()
                .await
                .map_err(|e| SchedulerError::Action(e.to_string()))?;
            for node in &nodes {
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
    }

    #[test]
    fn parse_start_node_action() {
        let json = r#"[{"action": "start_node", "node": "rtsp-camera"}]"#;
        let actions: Vec<Tier1Action> = serde_json::from_str(json).unwrap();
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
        let after = 1_700_000_000;
        let next = next_run_after("*/15 * * * *", after).unwrap();
        assert!(next > after);
        assert!(next <= after + 15 * 60);
    }

    #[test]
    fn next_run_after_six_field_cron() {
        let after = 1_700_000_000;
        let next = next_run_after("*/30 * * * * *", after).unwrap();
        assert!(next > after);
        assert!(next <= after + 30);
    }

    #[test]
    fn next_run_after_invalid_cron() {
        let result = next_run_after("not a cron expression", 1_700_000_000);
        assert!(result.is_err());
    }

    #[test]
    fn normalize_prepends_seconds_for_5_fields() {
        assert_eq!(normalize_cron_expr("*/15 * * * *"), "0 */15 * * * *");
    }

    #[test]
    fn normalize_keeps_6_fields_unchanged() {
        assert_eq!(normalize_cron_expr("0 */15 * * * *"), "0 */15 * * * *");
    }

    #[test]
    fn now_epoch_secs_is_reasonable() {
        let now = now_epoch_secs();
        // Should be after 2020 and before 2100
        assert!(now > 1_577_836_800);
        assert!(now < 4_102_444_800);
    }
}
