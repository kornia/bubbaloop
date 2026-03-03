//! System prompt builder — injects Soul identity, live node inventory,
//! job schedules, and episodic search results.

use crate::agent::memory::episodic::LogEntry;
use crate::agent::memory::semantic::Job;
use crate::agent::soul::Soul;

/// Build the system prompt from the Soul identity and live state.
///
/// The prompt structure:
/// 1. Soul identity (from ~/.bubbaloop/soul/identity.md)
/// 2. Current node inventory
/// 3. Active jobs (from semantic memory)
/// 4. Recent episodic context (from FTS5 search)
pub fn build_system_prompt(
    soul: &Soul,
    node_inventory: &str,
    active_jobs: &[Job],
    relevant_episodes: &[LogEntry],
) -> String {
    let mut parts = Vec::new();

    // Soul identity (the core of the agent's personality)
    parts.push(soul.identity.clone());

    // Capabilities summary
    parts.push(format!(
        "\n## Configuration\n\n\
         - Model: {}\n\
         - Max turns per job: {}\n\
         - Approval mode: {}",
        soul.capabilities.model_name,
        soul.capabilities.max_turns,
        soul.capabilities.default_approval_mode,
    ));

    // Current node inventory
    parts.push(format!(
        "\n## Current Sensor Inventory\n\n{}",
        if node_inventory.is_empty() {
            "No sensors installed."
        } else {
            node_inventory
        }
    ));

    // Active jobs
    let pending_jobs: Vec<_> = active_jobs
        .iter()
        .filter(|j| j.status == "pending" || j.status == "running")
        .collect();
    if !pending_jobs.is_empty() {
        let mut job_lines = vec!["\n## Active Jobs\n".to_string()];
        for job in &pending_jobs {
            let cron = job.cron_schedule.as_deref().unwrap_or("one-off");
            job_lines.push(format!(
                "- {} [status={}, schedule={}, retries={}]",
                job.prompt_payload, job.status, cron, job.retry_count
            ));
        }
        parts.push(job_lines.join("\n"));
    }

    // Relevant episodic context
    if !relevant_episodes.is_empty() {
        let mut ep_lines = vec!["\n## Relevant Context (from episodic memory)\n".to_string()];
        for entry in relevant_episodes.iter().take(5) {
            let content_preview: String = entry.content.chars().take(200).collect();
            ep_lines.push(format!(
                "- [{}] {}: {}",
                entry.timestamp, entry.role, content_preview
            ));
        }
        parts.push(ep_lines.join("\n"));
    }

    parts.join("\n")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn prompt_with_default_soul() {
        let soul = Soul::default();
        let prompt = build_system_prompt(&soul, "", &[], &[]);
        assert!(prompt.contains("Bubbaloop"));
        assert!(prompt.contains("No sensors installed."));
        assert!(prompt.contains("claude-sonnet-4-20250514"));
    }

    #[test]
    fn prompt_with_custom_soul() {
        let soul = Soul {
            identity: "I am a test agent.".to_string(),
            capabilities: crate::agent::soul::Capabilities {
                model_name: "test-model".to_string(),
                max_turns: 5,
                default_approval_mode: "propose".to_string(),
                ..Default::default()
            },
        };
        let prompt =
            build_system_prompt(&soul, "1 node(s) registered:\n  - cam [Running]", &[], &[]);
        assert!(prompt.contains("I am a test agent."));
        assert!(prompt.contains("test-model"));
        assert!(prompt.contains("propose"));
        assert!(prompt.contains("cam"));
        assert!(!prompt.contains("No sensors installed."));
    }

    #[test]
    fn prompt_with_active_jobs() {
        let soul = Soul::default();
        let jobs = vec![Job {
            id: "job-1".to_string(),
            cron_schedule: Some("*/15 * * * *".to_string()),
            next_run_at: "0".to_string(),
            prompt_payload: "health patrol".to_string(),
            status: "pending".to_string(),
            recurrence: true,
            retry_count: 0,
            last_error: None,
        }];
        let prompt = build_system_prompt(&soul, "", &jobs, &[]);
        assert!(prompt.contains("Active Jobs"));
        assert!(prompt.contains("health patrol"));
        assert!(prompt.contains("*/15 * * * *"));
    }

    #[test]
    fn prompt_with_episodic_context() {
        let soul = Soul::default();
        let episodes = vec![LogEntry {
            timestamp: "2026-03-03T10:00:00Z".to_string(),
            role: "assistant".to_string(),
            content: "Restarted front-door camera after 5 minutes offline.".to_string(),
            job_id: Some("job-1".to_string()),
            flush: None,
        }];
        let prompt = build_system_prompt(&soul, "", &[], &episodes);
        assert!(prompt.contains("Relevant Context"));
        assert!(prompt.contains("front-door camera"));
    }

    #[test]
    fn prompt_skips_completed_jobs() {
        let soul = Soul::default();
        let jobs = vec![Job {
            id: "job-1".to_string(),
            cron_schedule: None,
            next_run_at: "0".to_string(),
            prompt_payload: "done task".to_string(),
            status: "completed".to_string(),
            recurrence: false,
            retry_count: 0,
            last_error: None,
        }];
        let prompt = build_system_prompt(&soul, "", &jobs, &[]);
        assert!(!prompt.contains("Active Jobs"));
    }

    #[test]
    fn prompt_limits_episodic_to_5() {
        let soul = Soul::default();
        let episodes: Vec<LogEntry> = (0..10)
            .map(|i| LogEntry {
                timestamp: format!("2026-03-03T10:{:02}:00Z", i),
                role: "user".to_string(),
                content: format!("message {}", i),
                job_id: None,
                flush: None,
            })
            .collect();
        let prompt = build_system_prompt(&soul, "", &[], &episodes);
        let count = prompt.matches("message ").count();
        assert_eq!(count, 5, "should limit episodic context to 5 entries");
    }
}
