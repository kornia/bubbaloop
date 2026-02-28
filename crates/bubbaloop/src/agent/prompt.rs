//! System prompt builder — injects live sensor inventory and schedules.

use crate::agent::memory::{Schedule, SensorEvent};

/// Build the system prompt with live sensor data injected.
///
/// Includes role description, current node inventory, active schedules,
/// and recent sensor events. The prompt gives the Claude agent full
/// context about the physical system it controls.
pub fn build_system_prompt(
    node_inventory: &str,
    schedules: &[Schedule],
    recent_events: &[SensorEvent],
) -> String {
    let mut parts = Vec::new();

    // Role description
    parts.push(
        "You are Bubbaloop, an AI agent that controls physical sensors and hardware. \
         You manage a fleet of sensor nodes (cameras, weather stations, telemetry devices) \
         through the Bubbaloop skill runtime.\n\
         \n\
         You can list nodes, start/stop them, check their health, install new nodes from \
         the marketplace, send commands, and query Zenoh topics. Always verify the current \
         state before making changes.\n\
         \n\
         Be concise but informative. When you perform actions, report what you did and the result."
            .to_string(),
    );

    // Current sensor inventory
    parts.push(format!(
        "\n## Current Sensor Inventory\n\n{}",
        if node_inventory.is_empty() {
            "No sensors installed."
        } else {
            node_inventory
        }
    ));

    // Active schedules
    if !schedules.is_empty() {
        let mut schedule_lines = vec!["\n## Active Schedules\n".to_string()];
        for sched in schedules {
            let last = sched.last_run.as_deref().unwrap_or("never");
            let next = sched.next_run.as_deref().unwrap_or("not scheduled");
            schedule_lines.push(format!(
                "- {} (cron: {}, tier: {}, last run: {}, next: {})",
                sched.name, sched.cron, sched.tier, last, next
            ));
        }
        parts.push(schedule_lines.join("\n"));
    }

    // Recent events (limit to 10)
    if !recent_events.is_empty() {
        let mut event_lines = vec!["\n## Recent Events\n".to_string()];
        for event in recent_events.iter().take(10) {
            let details = event.details.as_deref().unwrap_or("");
            if details.is_empty() {
                event_lines.push(format!(
                    "- [{}] {} {}",
                    event.timestamp, event.node_name, event.event_type
                ));
            } else {
                event_lines.push(format!(
                    "- [{}] {} {} ({})",
                    event.timestamp, event.node_name, event.event_type, details
                ));
            }
        }
        parts.push(event_lines.join("\n"));
    }

    parts.join("\n")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn prompt_with_empty_inventory() {
        let prompt = build_system_prompt("", &[], &[]);
        assert!(prompt.contains("You are Bubbaloop"));
        assert!(prompt.contains("No sensors installed."));
        // Should not contain schedule or event sections
        assert!(!prompt.contains("Active Schedules"));
        assert!(!prompt.contains("Recent Events"));
    }

    #[test]
    fn prompt_with_nodes() {
        let inventory = "2 node(s) registered:\n  \
                         - rtsp-camera [status=Running, health=Healthy]\n  \
                         - openmeteo [status=Stopped, health=Unknown]";
        let prompt = build_system_prompt(inventory, &[], &[]);
        assert!(prompt.contains("You are Bubbaloop"));
        assert!(prompt.contains("rtsp-camera"));
        assert!(prompt.contains("openmeteo"));
        assert!(prompt.contains("2 node(s) registered"));
        // Empty inventory text should NOT appear
        assert!(!prompt.contains("No sensors installed."));
    }

    #[test]
    fn prompt_with_schedules() {
        let schedules = vec![
            Schedule {
                id: "s1".into(),
                name: "health-patrol".into(),
                cron: "*/15 * * * *".into(),
                actions: r#"["check_all_health"]"#.into(),
                tier: 1,
                last_run: Some("1709136000".into()),
                next_run: Some("1709136900".into()),
                created_by: "yaml".into(),
            },
            Schedule {
                id: "s2".into(),
                name: "daily-restart".into(),
                cron: "0 3 * * *".into(),
                actions: r#"["restart_all"]"#.into(),
                tier: 2,
                last_run: None,
                next_run: None,
                created_by: "agent".into(),
            },
        ];
        let prompt = build_system_prompt("No nodes registered.", &schedules, &[]);
        assert!(prompt.contains("Active Schedules"));
        assert!(prompt.contains("health-patrol"));
        assert!(prompt.contains("*/15 * * * *"));
        assert!(prompt.contains("tier: 1"));
        assert!(prompt.contains("last run: 1709136000"));
        assert!(prompt.contains("daily-restart"));
        assert!(prompt.contains("last run: never"));
        assert!(prompt.contains("next: not scheduled"));
    }

    #[test]
    fn prompt_with_events() {
        let events: Vec<SensorEvent> = (0..15)
            .map(|i| SensorEvent {
                id: format!("e{}", i),
                timestamp: format!("{}", 1709136000 + i),
                node_name: "rtsp-camera".into(),
                event_type: if i % 2 == 0 {
                    "health_ok".into()
                } else {
                    "started".into()
                },
                details: if i == 0 {
                    Some(r#"{"uptime":120}"#.into())
                } else {
                    None
                },
            })
            .collect();

        let prompt = build_system_prompt("No nodes registered.", &[], &events);
        assert!(prompt.contains("Recent Events"));
        assert!(prompt.contains("rtsp-camera"));
        assert!(prompt.contains("health_ok"));
        // First event should have details
        assert!(prompt.contains(r#"{"uptime":120}"#));
        // Should be limited to 10 events (we passed 15)
        let event_count = prompt.matches("rtsp-camera").count();
        assert_eq!(
            event_count, 10,
            "should limit to 10 events, got {}",
            event_count
        );
    }
}
