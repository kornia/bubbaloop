//! Job poller — integrated with heartbeat.
//!
//! Unlike the old scheduler (standalone 60s background loop), the new
//! scheduler is called from the heartbeat loop on each beat. Jobs are
//! picked up from SQLite when `status = 'pending' AND next_run_at <= now`.

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

#[cfg(test)]
mod tests {
    use super::*;

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
