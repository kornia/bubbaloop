//! System prompt builder — injects Soul identity, live node inventory,
//! job schedules, episodic search results, and world state.

use crate::agent::memory::episodic::LogEntry;
use crate::agent::memory::semantic::Job;
use crate::agent::memory::WorldStateEntry;
use crate::agent::soul::Soul;

/// Build the system prompt from the Soul identity and live state.
///
/// The prompt structure:
/// 1. Soul identity (from ~/.bubbaloop/soul/identity.md)
/// 2. Current node inventory
/// 3. Active jobs (from semantic memory)
/// 4. Recent episodic context (from FTS5 search)
// Default identity prompt for new agents that haven't been onboarded yet.
const NEW_AGENT_IDENTITY: &str = r#"You are a new Bubbaloop agent that hasn't been configured yet.

## First-Run Onboarding

This is your first conversation. Your job is to learn who you are from the user.

Ask the user these questions (one at a time, conversationally):
1. "What's my name?" — what should you be called
2. "What's my focus?" — what you'll be monitoring or managing (e.g., cameras, weather, robots)
3. "How should I communicate?" — terse/detailed, personality traits

After you have the answers, do TWO things:
1. Write your identity file using the write_file tool:
   - path: {soul_path}
   - content: a personality prompt in this format:
     "You are [name], an AI agent that manages [focus] through the Bubbaloop skill runtime.

     Your focus: [focus description]

     [Any personality traits the user specified]

     Your job is to keep the fleet healthy and do what the user asks.
     When given a task, DO it. Use your tools, get results, report back.
     Be concise. Report what you did and the result."

2. Confirm to the user: "I'm all set! I'm [name], focused on [focus]. Restart the daemon and I'll be ready."

Keep it brief and friendly. Don't over-explain."#;

/// Format world state entries into a token-budget-aware prompt section.
/// Stale entries are listed first with a warning marker. Never exceeds budget.
pub fn format_world_state_section(entries: &[WorldStateEntry], token_budget: usize) -> String {
    if entries.is_empty() {
        return String::new();
    }

    let mut lines = vec!["## WORLD STATE (sensor-grounded, live)".to_string()];
    let mut approx_tokens = 8usize;

    // Stale entries first (safety-critical awareness)
    let (stale, fresh): (Vec<_>, Vec<_>) = entries.iter().partition(|e| e.stale);

    for entry in stale.iter().chain(fresh.iter()) {
        let line = if entry.stale {
            format!(
                "  [STALE] {} = {} (conf={:.2})",
                entry.key, entry.value, entry.confidence
            )
        } else {
            format!(
                "  {} = {} (conf={:.2})",
                entry.key, entry.value, entry.confidence
            )
        };
        let line_tokens = line.len() / 4 + 1;
        if approx_tokens + line_tokens > token_budget {
            break;
        }
        approx_tokens += line_tokens;
        lines.push(line);
    }

    lines.join("\n")
}

pub fn build_system_prompt(
    soul: &Soul,
    node_inventory: &str,
    active_jobs: &[Job],
    relevant_episodes: &[LogEntry],
    recent_plan: Option<&str>,
    recovered_context: Option<&str>,
    resource_summary: Option<&str>,
) -> String {
    build_system_prompt_with_soul_path(
        soul,
        node_inventory,
        active_jobs,
        relevant_episodes,
        recent_plan,
        recovered_context,
        resource_summary,
        None,
    )
}

/// Build system prompt, optionally injecting a first-run onboarding flow
/// when `soul_path` is Some (meaning the agent has no identity.md yet).
#[allow(clippy::too_many_arguments)]
pub fn build_system_prompt_with_soul_path(
    soul: &Soul,
    node_inventory: &str,
    active_jobs: &[Job],
    relevant_episodes: &[LogEntry],
    recent_plan: Option<&str>,
    recovered_context: Option<&str>,
    resource_summary: Option<&str>,
    soul_path: Option<&str>,
) -> String {
    let mut parts = Vec::new();

    // Soul identity — use onboarding prompt for new agents
    if let Some(path) = soul_path {
        parts.push(NEW_AGENT_IDENTITY.replace("{soul_path}", path));
    } else {
        parts.push(soul.identity.clone());
    }

    // Post-compaction context recovery from episodic memory.
    // When the LLM context window is truncated, this section restores
    // the most recent flush entry so the agent can maintain continuity.
    if let Some(ctx) = recovered_context {
        parts.push(format!(
            "\n## Previously Persisted Context (recovered from memory)\n\n\
             The following was saved before your last context compaction. \
             This is recovered state, not current conversation.\n\n{}",
            ctx
        ));
    }

    // Capabilities summary
    parts.push(format!(
        "\n## Configuration\n\n\
         - Model: {}\n\
         - Max turns per job: {}",
        soul.capabilities.model_name, soul.capabilities.max_turns,
    ));

    // Operating mode directive
    if soul.capabilities.default_approval_mode == "propose" {
        parts.push(
            "\n## Operating Mode: Propose\n\n\
             For state-changing actions, explain what you plan to do and why, then\n\
             wait for approval. Read-only operations execute immediately."
                .to_string(),
        );
    } else {
        parts.push(
            "\n## Operating Mode: Autonomous (Ralph Loop)\n\n\
             You run in an autonomous loop. For every task:\n\n\
             1. **Plan** — Break the task into concrete steps. State them briefly.\n\
             2. **Execute** — Do step 1 with tools. Then step 2. Keep going.\n\
             3. **Evaluate** — After each step, check: did it work? Is the goal met?\n\
             4. **Iterate** — If not done, adjust and continue. If done, report results.\n\n\
             ### Rules\n\n\
             - **Act, don't ask.** Never say \"Would you like me to...?\" or \"Should I...?\" — do it.\n\
             - **Keep going.** Don't stop after one tool call if the task needs more.\n\
             - **Verify.** After making a change, confirm it worked (check health, read output, etc).\n\
             - **Be concise.** Report what you did and the result. Skip preamble.\n\
             - **Chain tools.** Use multiple tools in sequence to complete complex tasks.\n\n\
             ### Examples of Autonomous Behavior\n\n\
             User: \"Set up a temperature monitor\"\n\
             → Plan: install node, build, start, verify health\n\
             → Execute: install_node → build_node → start_node → get_node_health\n\
             → Report: \"Installed and started openmeteo. Health: Healthy.\"\n\n\
             User: \"Check why the camera is down\"\n\
             → get_node_health → get_node_logs → diagnose → restart_node → verify\n\
             → Report: \"Camera crashed (OOM). Restarted. Now healthy.\""
                .to_string(),
        );
    }

    // Scope — tells the LLM what tools it has
    parts.push(
        "\n## Tools\n\n\
         You have 30 tools across three categories:\n\
         - **Node management:** install, build, start, stop, restart, configure, monitor, query nodes\n\
         - **System:** read and write files, run shell commands\n\
         - **Memory:** search and manage episodic memory\n\n\
         Use the right tool for the job. For node operations, use the dedicated node tools.\n\
         For everything else, use read_file, write_file, or run_command."
            .to_string(),
    );

    // TODO(phase-2): wire world_state into build_system_prompt — pass entries from
    // MemoryBackend::world_state_snapshot() and call format_world_state_section() here.

    if let Some(summary) = resource_summary {
        parts.push(summary.to_string());
    }

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

    // Current plan (persisted from a previous turn, survives context compaction)
    if let Some(plan) = recent_plan {
        parts.push(format!("\n## Current Plan\n\n{}", plan));
    }

    parts.join("\n")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn prompt_with_default_soul() {
        let soul = Soul::default();
        let prompt = build_system_prompt(&soul, "", &[], &[], None, None, None);
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
        let prompt = build_system_prompt(
            &soul,
            "1 node(s) registered:\n  - cam [Running]",
            &[],
            &[],
            None,
            None,
            None,
        );
        assert!(prompt.contains("I am a test agent."));
        assert!(prompt.contains("test-model"));
        assert!(prompt.contains("Operating Mode: Propose"));
        assert!(prompt.contains("cam"));
        assert!(!prompt.contains("No sensors installed."));
    }

    #[test]
    fn prompt_with_active_jobs() {
        let soul = Soul::default();
        let jobs = vec![Job {
            id: "job-1".to_string(),
            cron_schedule: Some("*/15 * * * *".to_string()),
            next_run_at: 0,
            prompt_payload: "health patrol".to_string(),
            status: "pending".to_string(),
            recurrence: true,
            retry_count: 0,
            last_error: None,
        }];
        let prompt = build_system_prompt(&soul, "", &jobs, &[], None, None, None);
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
            id: None,
            cause_id: None,
            salience: None,
            mission_id: None,
        }];
        let prompt = build_system_prompt(&soul, "", &[], &episodes, None, None, None);
        assert!(prompt.contains("Relevant Context"));
        assert!(prompt.contains("front-door camera"));
    }

    #[test]
    fn prompt_skips_completed_jobs() {
        let soul = Soul::default();
        let jobs = vec![Job {
            id: "job-1".to_string(),
            cron_schedule: None,
            next_run_at: 0,
            prompt_payload: "done task".to_string(),
            status: "completed".to_string(),
            recurrence: false,
            retry_count: 0,
            last_error: None,
        }];
        let prompt = build_system_prompt(&soul, "", &jobs, &[], None, None, None);
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
                id: None,
                cause_id: None,
                salience: None,
                mission_id: None,
            })
            .collect();
        let prompt = build_system_prompt(&soul, "", &[], &episodes, None, None, None);
        let count = prompt.matches("message ").count();
        assert_eq!(count, 5, "should limit episodic context to 5 entries");
    }

    #[test]
    fn prompt_auto_mode_includes_autonomous_directive() {
        let soul = Soul::default(); // default is auto mode
        let prompt = build_system_prompt(&soul, "", &[], &[], None, None, None);
        assert!(
            prompt.contains("Autonomous (Ralph Loop)"),
            "auto mode should include autonomous directive"
        );
        assert!(
            prompt.contains("Act, don't ask"),
            "auto mode should include 'Act, don't ask' instruction"
        );
        assert!(
            prompt.contains("Plan"),
            "auto mode should include planning step"
        );
        assert!(
            prompt.contains("Evaluate"),
            "auto mode should include evaluation step"
        );
    }

    #[test]
    fn prompt_includes_scope_boundary() {
        let soul = Soul::default();
        let prompt = build_system_prompt(&soul, "", &[], &[], None, None, None);
        assert!(
            prompt.contains("## Tools"),
            "prompt should include Tools section"
        );
        assert!(
            prompt.contains("Node management"),
            "prompt should mention node management tools"
        );
        assert!(
            prompt.contains("read_file"),
            "prompt should mention system tools"
        );
    }

    #[test]
    fn prompt_propose_mode_includes_propose_directive() {
        let soul = Soul {
            identity: "You are TestBot, a test agent.".to_string(),
            capabilities: crate::agent::soul::Capabilities {
                default_approval_mode: "propose".to_string(),
                ..Default::default()
            },
        };
        let prompt = build_system_prompt(&soul, "", &[], &[], None, None, None);
        assert!(
            prompt.contains("Operating Mode: Propose"),
            "propose mode should include propose directive"
        );
        assert!(
            prompt.contains("wait for approval"),
            "propose mode should mention waiting for approval"
        );
    }

    #[test]
    fn prompt_with_plan_includes_current_plan() {
        let soul = Soul::default();
        let plan_text = "1. Install camera node\n2. Build and start\n3. Verify health";
        let prompt = build_system_prompt(&soul, "", &[], &[], Some(plan_text), None, None);
        assert!(
            prompt.contains("## Current Plan"),
            "prompt should include Current Plan section"
        );
        assert!(
            prompt.contains("Install camera node"),
            "prompt should include the plan content"
        );
    }

    #[test]
    fn prompt_without_plan_omits_section() {
        let soul = Soul::default();
        let prompt = build_system_prompt(&soul, "", &[], &[], None, None, None);
        assert!(
            !prompt.contains("Current Plan"),
            "prompt should not include Current Plan when None"
        );
    }

    #[test]
    fn prompt_with_recovered_context() {
        let soul = Soul::default();
        let recovered = "Camera node was restarted. Job health-patrol is running.";
        let prompt = build_system_prompt(&soul, "", &[], &[], None, Some(recovered), None);
        assert!(
            prompt.contains("Previously Persisted Context"),
            "prompt should include recovered context section"
        );
        assert!(
            prompt.contains("recovered from memory"),
            "prompt should label context as recovered"
        );
        assert!(
            prompt.contains("Camera node was restarted"),
            "prompt should include the recovered content"
        );
    }

    #[test]
    fn prompt_without_recovered_context_omits_section() {
        let soul = Soul::default();
        let prompt = build_system_prompt(&soul, "", &[], &[], None, None, None);
        assert!(
            !prompt.contains("Previously Persisted Context"),
            "prompt should not include recovered context when None"
        );
    }

    #[test]
    fn prompt_recovered_context_appears_after_identity() {
        let soul = Soul::default();
        let recovered = "Some recovered state";
        let prompt = build_system_prompt(&soul, "", &[], &[], None, Some(recovered), None);
        let identity_pos = prompt.find("Bubbaloop").unwrap();
        let recovered_pos = prompt.find("Previously Persisted Context").unwrap();
        let config_pos = prompt.find("## Configuration").unwrap();
        assert!(
            recovered_pos > identity_pos,
            "recovered context should appear after identity"
        );
        assert!(
            recovered_pos < config_pos,
            "recovered context should appear before configuration"
        );
    }

    #[test]
    fn world_state_section_formats_stale_entries_first() {
        let entries = vec![
            WorldStateEntry {
                key: "cam.status".to_string(),
                value: "online".to_string(),
                confidence: 0.95,
                source_topic: None,
                source_node: None,
                last_seen_at: 1000,
                max_age_secs: 300,
                stale: false,
            },
            WorldStateEntry {
                key: "temp.reading".to_string(),
                value: "42".to_string(),
                confidence: 0.80,
                source_topic: None,
                source_node: None,
                last_seen_at: 500,
                max_age_secs: 300,
                stale: true,
            },
        ];
        let section = format_world_state_section(&entries, 500);
        assert!(section.contains("WORLD STATE"));
        // Stale entry should appear before fresh
        let stale_pos = section.find("[STALE]").unwrap();
        let fresh_pos = section.find("cam.status").unwrap();
        assert!(
            stale_pos < fresh_pos,
            "stale entry should appear before fresh"
        );
    }

    #[test]
    fn world_state_section_respects_token_budget() {
        let entries: Vec<WorldStateEntry> = (0..20)
            .map(|i| WorldStateEntry {
                key: format!("sensor_{}.reading", i),
                value: format!("value_{}", i),
                confidence: 0.9,
                source_topic: None,
                source_node: None,
                last_seen_at: 1000,
                max_age_secs: 300,
                stale: false,
            })
            .collect();
        // With budget=10, the header alone is ~8 tokens, so very few entries fit
        let section = format_world_state_section(&entries, 10);
        // Should have header but only 0-1 entries
        assert!(section.contains("WORLD STATE"));
        let entry_count = section.matches("sensor_").count();
        assert!(
            entry_count <= 2,
            "should have very few entries with budget=10, got {}",
            entry_count
        );
    }

    #[test]
    fn world_state_section_empty_returns_empty_string() {
        let section = format_world_state_section(&[], 500);
        assert!(section.is_empty());
    }

    #[test]
    fn prompt_with_resource_summary() {
        let soul = Soul::default();
        let prompt = build_system_prompt(
            &soul,
            "",
            &[],
            &[],
            None,
            None,
            Some("## System Resources\nMemory: 62% used (Yellow)"),
        );
        assert!(prompt.contains("System Resources"));
        assert!(prompt.contains("62% used"));
    }
}
