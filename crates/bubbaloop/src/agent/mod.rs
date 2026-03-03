//! Agent — OpenClaw-inspired architecture with Soul, 3-tier memory, and adaptive heartbeat.
//!
//! Module structure (design doc Section 6):
//! - `soul` — Hot-swappable identity + capabilities
//! - `provider` — ModelProvider trait + Claude/Ollama backends
//! - `memory` — 3-tier: short-term (RAM) + episodic (NDJSON) + semantic (SQLite)
//! - `heartbeat` — Adaptive heartbeat with arousal/decay
//! - `dispatch` — Internal MCP tool dispatch
//! - `prompt` — System prompt builder
//! - `scheduler` — Job poller integrated with heartbeat

pub mod dispatch;
pub mod heartbeat;
pub mod memory;
pub mod prompt;
pub mod provider;
pub mod scheduler;
pub mod soul;

use crate::agent::dispatch::Dispatcher;
use crate::agent::heartbeat::{ArousalSource, ArousalState, HeartbeatState};
use crate::agent::memory::episodic::EpisodicLog;
use crate::agent::memory::Memory;
use crate::agent::provider::claude::{ClaudeProvider, DEFAULT_MODEL};
use crate::agent::provider::{ContentBlock, Message, ModelProvider};
use crate::agent::soul::Soul;
use crate::daemon::registry::get_bubbaloop_home;
use crate::mcp::platform::DaemonPlatform;
use std::sync::Arc;
use tokio::sync::RwLock;

/// Maximum number of conversation messages to keep in short-term memory.
const MAX_CONVERSATION_MESSAGES: usize = 40;

/// Default context window size (tokens). Used for flush threshold calculation.
const DEFAULT_CONTEXT_WINDOW: u32 = 200_000;

/// Errors from agent operations.
#[derive(Debug, thiserror::Error)]
pub enum AgentError {
    #[error("Provider error: {0}")]
    Provider(#[from] provider::ProviderError),
    #[error("Memory error: {0}")]
    Memory(#[from] memory::MemoryError),
    #[error("Zenoh connection failed: {0}")]
    Zenoh(String),
    #[error("IO error: {0}")]
    Io(#[from] std::io::Error),
}

pub type Result<T> = std::result::Result<T, AgentError>;

/// Configuration for the interactive agent session.
pub struct AgentConfig {
    /// Claude model override (uses Soul default if None).
    pub model: Option<String>,
}

/// Create a lightweight Zenoh session for the agent.
pub async fn create_agent_session(endpoint: Option<&str>) -> Result<Arc<zenoh::Session>> {
    let mut config = zenoh::Config::default();

    config
        .insert_json5("mode", "\"client\"")
        .expect("Failed to set Zenoh mode");

    let env_endpoint = std::env::var("BUBBALOOP_ZENOH_ENDPOINT").ok();
    let ep = endpoint
        .or(env_endpoint.as_deref())
        .unwrap_or("tcp/127.0.0.1:7447");

    config
        .insert_json5("connect/endpoints", &format!("[\"{}\"]", ep))
        .expect("Failed to set Zenoh endpoint");

    config
        .insert_json5("scouting/multicast/enabled", "false")
        .expect("Failed to disable multicast scouting");
    config
        .insert_json5("scouting/gossip/enabled", "false")
        .expect("Failed to disable gossip scouting");

    let session = zenoh::open(config)
        .await
        .map_err(|e| AgentError::Zenoh(e.to_string()))?;

    Ok(Arc::new(session))
}

/// Run a single agent job (one complete turn cycle).
///
/// State machine:
/// 1. Hydrate: read Soul, build system prompt with state context
/// 2. Cycle: max_turns, call provider.generate(), log to episodic, dispatch tools
/// 3. Finalize: update job status, handle failures
async fn run_agent_turn<M: ModelProvider, P: crate::mcp::platform::PlatformOperations>(
    provider: &M,
    dispatcher: &Dispatcher<P>,
    memory: &mut Memory,
    soul: &Soul,
    user_input: Option<&str>,
    job_id: Option<&str>,
) -> Result<()> {
    // 1. Build system prompt
    let inventory = dispatcher.get_node_inventory().await;
    let active_jobs = memory.semantic.pending_jobs().unwrap_or_default();
    let decay_half_life = soul.capabilities.episodic_decay_half_life_days;
    let relevant_episodes = match user_input {
        Some(input) => memory
            .episodic
            .search_with_decay(input, 5, decay_half_life)
            .unwrap_or_default(),
        None => Vec::new(),
    };
    let system_prompt =
        prompt::build_system_prompt(soul, &inventory, &active_jobs, &relevant_episodes);

    // Add user input to short-term memory
    if let Some(input) = user_input {
        memory.short_term.push(Message::user(input));
        // Log to episodic
        let entry = EpisodicLog::make_entry("user", input, job_id);
        if let Err(e) = memory.episodic.append(&entry) {
            log::warn!("Failed to log user message to episodic: {}", e);
        }
    }

    let tools = Dispatcher::<P>::tool_definitions();

    // 2. Turn cycle (max_turns)
    let mut last_input_tokens = 0u32;
    for _turn in 0..soul.capabilities.max_turns {
        let response = match provider
            .generate(Some(&system_prompt), &memory.short_term, &tools)
            .await
        {
            Ok(r) => r,
            Err(e) => {
                log::error!("Provider error: {}", e);
                println!("Error: {}", e);
                break;
            }
        };

        last_input_tokens = response.usage.input_tokens;

        // Build assistant message and add to short-term
        let assistant_msg = Message {
            role: "assistant".to_string(),
            content: response.content.clone(),
        };
        memory.short_term.push(assistant_msg);

        // Print text blocks
        let text = response.text();
        if !text.is_empty() {
            println!("{}", text);
            // Log to episodic
            let entry = EpisodicLog::make_entry("assistant", &text, job_id);
            if let Err(e) = memory.episodic.append(&entry) {
                log::warn!("Failed to log assistant message to episodic: {}", e);
            }
        }

        // Check for tool calls
        let tool_calls = response.tool_calls();
        if tool_calls.is_empty() {
            break; // Text-only response, turn complete
        }

        // Dispatch tool calls
        let mut tool_results = Vec::new();
        for tc in &tool_calls {
            println!("  [calling {}...]", tc.name);
            log::info!("[Agent] calling tool: {}", tc.name);

            // Memory tools are handled inline (EpisodicLog holds !Send Connection)
            let result = match tc.name.as_str() {
                "memory_search" => {
                    let query = tc.input.get("query").and_then(|v| v.as_str()).unwrap_or("");
                    let limit =
                        tc.input.get("limit").and_then(|v| v.as_u64()).unwrap_or(10) as usize;
                    let decay = soul.capabilities.episodic_decay_half_life_days;
                    match memory.episodic.search_with_decay(query, limit, decay) {
                        Ok(entries) if entries.is_empty() => ContentBlock::ToolResult {
                            tool_use_id: tc.id.clone(),
                            content: "No matching entries found.".to_string(),
                            is_error: None,
                        },
                        Ok(entries) => {
                            let text = entries
                                .iter()
                                .map(|e| format!("[{}] {}: {}", e.timestamp, e.role, e.content))
                                .collect::<Vec<_>>()
                                .join("\n");
                            ContentBlock::ToolResult {
                                tool_use_id: tc.id.clone(),
                                content: text,
                                is_error: None,
                            }
                        }
                        Err(e) => ContentBlock::ToolResult {
                            tool_use_id: tc.id.clone(),
                            content: format!("Error: {}", e),
                            is_error: Some(true),
                        },
                    }
                }
                "memory_forget" => {
                    let query = tc.input.get("query").and_then(|v| v.as_str()).unwrap_or("");
                    let reason = tc
                        .input
                        .get("reason")
                        .and_then(|v| v.as_str())
                        .unwrap_or("agent requested");
                    match memory.episodic.forget(query, reason) {
                        Ok(0) => ContentBlock::ToolResult {
                            tool_use_id: tc.id.clone(),
                            content: "No matching entries found to forget.".to_string(),
                            is_error: None,
                        },
                        Ok(n) => ContentBlock::ToolResult {
                            tool_use_id: tc.id.clone(),
                            content: format!("Forgot {} entries matching '{}'.", n, query),
                            is_error: None,
                        },
                        Err(e) => ContentBlock::ToolResult {
                            tool_use_id: tc.id.clone(),
                            content: format!("Error: {}", e),
                            is_error: Some(true),
                        },
                    }
                }
                _ => dispatcher.call_tool(&tc.id, &tc.name, &tc.input).await,
            };

            if let ContentBlock::ToolResult {
                tool_use_id,
                content,
                is_error,
            } = &result
            {
                tool_results.push((tool_use_id.clone(), content.clone(), *is_error));
                // Log tool result to episodic
                let entry = EpisodicLog::make_entry("tool", content, job_id);
                if let Err(e) = memory.episodic.append(&entry) {
                    log::warn!("Failed to log tool result to episodic: {}", e);
                }
            }
        }

        // Add tool results to short-term memory
        memory.short_term.push(Message::tool_results(tool_results));
    }

    // 2.5. Pre-compaction flush (design Section 5.5)
    //
    // When context usage approaches the window limit, trigger a silent flush turn.
    // The flush turn asks the model to persist important state before context compaction.
    // This does NOT count toward max_turns.
    let flush_threshold = soul.capabilities.compaction_flush_threshold_tokens as u32;
    if last_input_tokens > 0
        && last_input_tokens > DEFAULT_CONTEXT_WINDOW.saturating_sub(flush_threshold)
    {
        log::info!(
            "[Agent] Triggering pre-compaction flush (input_tokens={}, threshold={})",
            last_input_tokens,
            DEFAULT_CONTEXT_WINDOW - flush_threshold
        );

        // Add flush instruction as a user message
        let flush_instruction =
            "SYSTEM: Context window is approaching capacity. Persist any important state \
             to memory before context compaction. Summarize: active proposals, node health \
             assessments, unresolved decisions, and key reasoning. Be concise.";
        memory.short_term.push(Message::user(flush_instruction));

        if let Ok(flush_response) = provider
            .generate(Some(&system_prompt), &memory.short_term, &tools)
            .await
        {
            let flush_text = flush_response.text();
            if is_flush_substantive(&flush_text) {
                // Log flush to episodic with flush: true tag
                let entry = EpisodicLog::make_flush_entry(&flush_text, job_id);
                if let Err(e) = memory.episodic.append(&entry) {
                    log::warn!("Failed to log flush to episodic: {}", e);
                }
                log::info!("[Agent] Flush persisted ({} chars)", flush_text.len());
            } else {
                log::debug!("[Agent] Flush skipped (not substantive)");
            }
        }
    }

    // 3. Trim short-term memory
    if memory.short_term.len() > MAX_CONVERSATION_MESSAGES {
        let drain_count = memory.short_term.len() - MAX_CONVERSATION_MESSAGES;
        memory.short_term.drain(..drain_count);
    }

    Ok(())
}

/// Check whether flush text is substantive enough to persist.
///
/// Returns false for empty/short text or phrases indicating nothing to save.
/// This prevents context clutter from trivial flush responses.
fn is_flush_substantive(text: &str) -> bool {
    if text.len() < 50 {
        return false;
    }
    let lower = text.to_lowercase();
    let no_content_phrases = [
        "nothing to persist",
        "no important state",
        "nothing significant",
        "no active proposals",
        "nothing noteworthy",
    ];
    !no_content_phrases.iter().any(|p| lower.contains(p))
}

/// Run the interactive agent REPL with adaptive heartbeat.
pub async fn run_agent(
    config: AgentConfig,
    session: Arc<zenoh::Session>,
    node_manager: Arc<crate::daemon::node_manager::NodeManager>,
) -> Result<()> {
    // 1. Load Soul (creates defaults if needed)
    Soul::ensure_defaults();

    // First-run onboarding — ask name, focus, preferences
    let soul_dir = soul::soul_directory();
    if Soul::is_first_run(&soul_dir) {
        if let Err(e) = Soul::run_onboarding(&soul_dir) {
            log::warn!("Onboarding failed (using defaults): {}", e);
        }
    }

    let soul = Arc::new(RwLock::new(Soul::load_or_default()));

    // Resolve model: CLI override > Soul capability
    let model_name = {
        let s = soul.read().await;
        config
            .model
            .clone()
            .unwrap_or_else(|| s.capabilities.model_name.clone())
    };

    // 2. Initialize provider
    let provider = ClaudeProvider::from_env(Some(&model_name))?;

    // 3. Create dispatcher
    let scope = std::env::var("BUBBALOOP_SCOPE").unwrap_or_else(|_| "local".to_string());
    let machine_id = crate::daemon::util::get_machine_id();
    let platform = Arc::new(DaemonPlatform {
        node_manager: node_manager.clone(),
        session,
        scope: scope.clone(),
        machine_id: machine_id.clone(),
    });
    let dispatcher = Dispatcher::new(platform.clone(), scope, machine_id);

    // 4. Open memory + startup cleanup
    let base = get_bubbaloop_home();
    let mut memory = Memory::open(&base)?;
    {
        let retention = soul.read().await.capabilities.episodic_log_retention_days;
        memory.startup_cleanup(retention);
    }

    // 5. Set up shutdown signal
    let (shutdown_tx, _shutdown_rx) = tokio::sync::watch::channel(());

    // 6. Start soul watcher in background
    let soul_clone = soul.clone();
    let watcher_shutdown = shutdown_tx.subscribe();
    tokio::spawn(soul::soul_watcher(soul_clone, watcher_shutdown));

    // 7. Initialize arousal state
    let initial_caps = soul.read().await.capabilities.clone();
    let mut arousal = ArousalState::new(&initial_caps);

    // 8. Welcome message
    let display_model = config.model.as_deref().unwrap_or(DEFAULT_MODEL);
    let tools = Dispatcher::<DaemonPlatform>::tool_definitions();
    let inventory = dispatcher.get_node_inventory().await;
    let node_count = inventory
        .lines()
        .next()
        .and_then(|l| l.split_whitespace().next())
        .and_then(|n| n.parse::<usize>().ok())
        .unwrap_or(0);
    let agent_name = soul.read().await.name().to_string();
    println!();
    println!("  {} v2 ({})", agent_name, env!("CARGO_PKG_VERSION"));
    println!("  Model: {}", display_model);
    println!("  Tools: {} | Nodes: {}", tools.len(), node_count);
    println!(
        "  Heartbeat: {}s base, {}s min",
        initial_caps.heartbeat_base_interval, initial_caps.heartbeat_min_interval
    );
    println!();
    println!("  Type a message to chat, 'quit' to exit.");

    // 9. Main event loop (heartbeat + REPL)
    let stdin = std::io::stdin();
    let (repl_tx, mut repl_rx) = tokio::sync::mpsc::channel::<String>(1);

    // Spawn REPL reader in a blocking thread
    let repl_shutdown = shutdown_tx.subscribe();
    std::thread::spawn(move || {
        loop {
            if repl_shutdown.has_changed().unwrap_or(true) {
                break;
            }
            print!("> ");
            use std::io::Write;
            std::io::stdout().flush().ok();

            let mut line = String::new();
            match stdin.read_line(&mut line) {
                Ok(0) => break, // EOF
                Ok(_) => {
                    let trimmed = line.trim().to_string();
                    if trimmed == "quit" || trimmed == "exit" {
                        let _ = repl_tx.blocking_send(trimmed);
                        break;
                    }
                    if !trimmed.is_empty() {
                        let _ = repl_tx.blocking_send(trimmed);
                    }
                }
                Err(_) => break,
            }
        }
    });

    // Heartbeat + REPL select loop
    let mut heartbeat_shutdown = shutdown_tx.subscribe();
    loop {
        let interval = std::time::Duration::from_secs(arousal.interval_secs());

        tokio::select! {
            // Heartbeat tick
            _ = tokio::time::sleep(interval) => {
                // Collect state (check for pending jobs)
                let jobs = memory.semantic.pending_jobs().unwrap_or_default();
                let has_jobs = !jobs.is_empty();

                let state = if has_jobs {
                    HeartbeatState {
                        event_count: jobs.len(),
                        has_changes: true,
                        sources: vec![ArousalSource::PendingJobFired],
                    }
                } else {
                    HeartbeatState::default()
                };

                arousal.update(&state);

                // Only run agent turn if there's work to do
                if has_jobs {
                    for job in &jobs {
                        let soul_snapshot = soul.read().await.clone();
                        memory.semantic.start_job(&job.id).ok();

                        if let Err(e) = run_agent_turn(
                            &provider,
                            &dispatcher,
                            &mut memory,
                            &soul_snapshot,
                            Some(&job.prompt_payload),
                            Some(&job.id),
                        ).await {
                            log::error!("Agent turn failed for job {}: {}", job.id, e);
                            let max_retries = soul_snapshot.capabilities.max_retries;
                            memory.semantic.fail_job(&job.id, &e.to_string(), max_retries).ok();
                        } else {
                            // Calculate next_run for recurring jobs
                            let next_run = if job.recurrence {
                                job.cron_schedule.as_deref().and_then(|cron| {
                                    scheduler::next_run_after(cron, scheduler::now_epoch_secs()).ok()
                                }).map(|ts| ts.to_string())
                            } else {
                                None
                            };
                            memory.semantic.complete_job(&job.id, next_run.as_deref()).ok();
                        }

                        memory.clear_short_term();
                    }
                }

                if !arousal.is_at_rest() {
                    log::debug!("[Heartbeat] arousal={:.2}, interval={}s", arousal.arousal(), arousal.interval_secs());
                }
            }

            // REPL input
            Some(input) = repl_rx.recv() => {
                if input == "quit" || input == "exit" {
                    break;
                }

                arousal.spike(ArousalSource::UserInput);

                let soul_snapshot = soul.read().await.clone();
                if let Err(e) = run_agent_turn(
                    &provider,
                    &dispatcher,
                    &mut memory,
                    &soul_snapshot,
                    Some(&input),
                    None,
                ).await {
                    log::error!("Agent turn failed: {}", e);
                    println!("Error: {}", e);
                }
            }

            // Shutdown signal
            _ = heartbeat_shutdown.changed() => {
                break;
            }
        }
    }

    // Graceful shutdown
    drop(shutdown_tx);
    println!("Goodbye.");
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn flush_short_text_not_substantive() {
        assert!(!is_flush_substantive("OK"));
        assert!(!is_flush_substantive("No state to save."));
        assert!(!is_flush_substantive("")); // 0 len < 50
    }

    #[test]
    fn flush_no_content_phrases() {
        assert!(!is_flush_substantive(
            "After reviewing the current state, there is nothing to persist at this time. All systems are nominal."
        ));
        assert!(!is_flush_substantive(
            "I have checked all nodes and proposals. There is no important state to record right now."
        ));
        assert!(!is_flush_substantive(
            "Current status check complete. Nothing significant has changed since the last flush."
        ));
    }

    #[test]
    fn flush_real_content_is_substantive() {
        assert!(is_flush_substantive(
            "Active proposals: prop-1 (restart rtsp-camera, pending approval). \
             Node health: openmeteo degraded (3 consecutive timeouts). \
             Key reasoning: weather API rate limit may have been hit."
        ));
    }
}
