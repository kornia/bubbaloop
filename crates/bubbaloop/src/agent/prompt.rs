//! System prompt builder — injects live sensor inventory and schedules.

use crate::agent::memory::{ConversationRow, Schedule, SensorEvent, SkillSearchResult};
use crate::skills::SkillConfig;

/// Build the system prompt with live sensor data injected.
///
/// Includes role description, loaded skill instructions, current node
/// inventory, active schedules, and recent sensor events. The prompt
/// gives the Claude agent full context about the physical system it
/// controls and the user's intent for each skill.
pub fn build_system_prompt(
    node_inventory: &str,
    schedules: &[Schedule],
    recent_events: &[SensorEvent],
    skills: &[SkillConfig],
    relevant_convos: &[ConversationRow],
    relevant_events: &[SensorEvent],
    relevant_skills: &[SkillSearchResult],
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

    // Loaded skills — agent-readable instructions from ~/.bubbaloop/skills/
    let skills_with_body: Vec<_> = skills.iter().filter(|s| !s.body.is_empty()).collect();
    if !skills_with_body.is_empty() {
        let mut skill_lines = vec!["\n## Skills\n".to_string()];
        for skill in &skills_with_body {
            skill_lines.push(format!("### {} (driver: {})\n", skill.name, skill.driver));
            skill_lines.push(skill.body.clone());
            skill_lines.push(String::new());
        }
        parts.push(skill_lines.join("\n"));
    }

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

    // Relevant context from semantic search (FTS5)
    if !relevant_convos.is_empty() || !relevant_events.is_empty() || !relevant_skills.is_empty() {
        let mut ctx_lines = vec!["\n## Relevant Context (from past interactions)\n".to_string()];

        if !relevant_convos.is_empty() {
            ctx_lines.push("**Related conversations:**".to_string());
            for conv in relevant_convos.iter().take(5) {
                ctx_lines.push(format!("- [{}] {}: {}", conv.timestamp, conv.role, conv.content));
            }
            ctx_lines.push(String::new());
        }

        if !relevant_events.is_empty() {
            ctx_lines.push("**Related events:**".to_string());
            for event in relevant_events.iter().take(5) {
                let details = event.details.as_deref().unwrap_or("");
                if details.is_empty() {
                    ctx_lines.push(format!(
                        "- [{}] {} {}",
                        event.timestamp, event.node_name, event.event_type
                    ));
                } else {
                    ctx_lines.push(format!(
                        "- [{}] {} {} ({})",
                        event.timestamp, event.node_name, event.event_type, details
                    ));
                }
            }
            ctx_lines.push(String::new());
        }

        if !relevant_skills.is_empty() {
            ctx_lines.push("**Related skills:**".to_string());
            for skill in relevant_skills.iter().take(3) {
                // Truncate body to first 200 chars for context
                let body_preview: String = skill.body.chars().take(200).collect();
                ctx_lines.push(format!(
                    "- {} (driver: {}): {}",
                    skill.name, skill.driver, body_preview
                ));
            }
            ctx_lines.push(String::new());
        }

        parts.push(ctx_lines.join("\n"));
    }

    parts.join("\n")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn prompt_with_empty_inventory() {
        let prompt = build_system_prompt("", &[], &[], &[], &[], &[], &[]);
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
        let prompt = build_system_prompt(inventory, &[], &[], &[], &[], &[], &[]);
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
        let prompt = build_system_prompt("No nodes registered.", &schedules, &[], &[], &[], &[], &[]);
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

        let prompt = build_system_prompt("No nodes registered.", &[], &events, &[], &[], &[], &[]);
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

    #[test]
    fn prompt_with_skills() {
        let skills = vec![SkillConfig {
            name: "entrance-cam".into(),
            driver: "rtsp".into(),
            enabled: true,
            config: Default::default(),
            schedule: None,
            actions: Vec::new(),
            body: "# Entrance Camera\n\nTapo C200 at the front door.\nURL: rtsp://192.168.1.141:554/stream2".into(),
        }];
        let prompt = build_system_prompt("", &[], &[], &skills, &[], &[], &[]);
        assert!(prompt.contains("## Skills"));
        assert!(prompt.contains("entrance-cam (driver: rtsp)"));
        assert!(prompt.contains("Entrance Camera"));
        assert!(prompt.contains("front door"));
    }

    #[test]
    fn prompt_skips_skills_without_body() {
        let skills = vec![SkillConfig {
            name: "legacy-cam".into(),
            driver: "rtsp".into(),
            enabled: true,
            config: Default::default(),
            schedule: None,
            actions: Vec::new(),
            body: String::new(),
        }];
        let prompt = build_system_prompt("", &[], &[], &skills, &[], &[], &[]);
        assert!(!prompt.contains("Skills"));
    }

    #[test]
    fn prompt_with_relevant_context() {
        let convos = vec![ConversationRow {
            id: "c1".into(),
            timestamp: "2026-02-28T10:00:00Z".into(),
            role: "user".into(),
            content: "The entrance camera keeps freezing".into(),
            tool_calls: None,
        }];
        let events = vec![SensorEvent {
            id: "e1".into(),
            timestamp: "2026-02-28T09:55:00Z".into(),
            node_name: "rtsp-camera".into(),
            event_type: "health_degraded".into(),
            details: Some("frame_drop_rate=0.3".into()),
        }];
        let prompt = build_system_prompt("", &[], &[], &[], &convos, &events, &[]);
        assert!(prompt.contains("Relevant Context"));
        assert!(prompt.contains("Related conversations"));
        assert!(prompt.contains("entrance camera keeps freezing"));
        assert!(prompt.contains("Related events"));
        assert!(prompt.contains("health_degraded"));
        assert!(prompt.contains("frame_drop_rate"));
    }
}
