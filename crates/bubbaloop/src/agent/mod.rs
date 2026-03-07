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
pub(crate) mod dispatch_security;
pub mod gateway;
pub mod heartbeat;
pub mod memory;
pub mod prompt;
pub mod provider;
pub mod runtime;
pub mod scheduler;
pub mod soul;

use crate::agent::dispatch::Dispatcher;
use crate::agent::gateway::AgentEvent;
use crate::agent::memory::episodic::EpisodicLog;
use crate::agent::memory::Memory;
use crate::agent::provider::{ContentBlock, Message, ModelProvider, StreamEvent, ToolCall, Usage};
use crate::agent::soul::Soul;
use std::collections::HashMap;
use std::hash::{Hash, Hasher};
use std::sync::Arc;
use std::time::Duration;

// ── EventSink trait ──────────────────────────────────────────────

/// Trait for emitting agent events to an output channel.
///
/// Implementations:
/// - `StdoutSink` — prints to terminal (preserves current CLI behavior)
/// - `ZenohSink` — publishes JSON events to a Zenoh outbox topic (runtime)
pub trait EventSink: Send + Sync {
    /// Emit an agent event. Implementations should not block.
    fn emit(&self, event: AgentEvent) -> impl std::future::Future<Output = ()> + Send;
}

/// Prints agent events to stdout (preserves original CLI behavior).
pub struct StdoutSink;

impl EventSink for StdoutSink {
    async fn emit(&self, event: AgentEvent) {
        use crate::agent::gateway::AgentEventType;
        match event.event_type {
            AgentEventType::Delta => {
                if let Some(text) = &event.text {
                    print!("{}", text);
                    std::io::Write::flush(&mut std::io::stdout()).ok();
                }
            }
            AgentEventType::Tool => {
                if let Some(name) = &event.text {
                    println!("  [calling {}...]", name);
                }
            }
            AgentEventType::ToolResult => {
                // Tool results are not printed in the original CLI
            }
            AgentEventType::Error => {
                if let Some(msg) = &event.text {
                    println!("Error: {}", msg);
                }
            }
            AgentEventType::Done => {
                // Done is implicit in stdout mode
            }
        }
    }
}

/// Maximum number of conversation messages to keep in short-term memory.
const MAX_CONVERSATION_MESSAGES: usize = 40;

/// Default context window size (tokens). Used for flush threshold calculation.
const DEFAULT_CONTEXT_WINDOW: u32 = 200_000;

/// Number of identical tool calls before injecting a loop-break advisory.
const LOOP_BREAK_THRESHOLD: u32 = 6;

/// Number of identical tool calls before logging a warning.
const LOOP_WARN_THRESHOLD: u32 = 3;

/// Maximum time (seconds) for an entire agent turn before aborting.
const TURN_TIMEOUT_SECS: u64 = 120;

/// Maximum time (seconds) for a single tool call before returning an error.
const TOOL_CALL_TIMEOUT_SECS: u64 = 30;

/// Maximum characters in a tool result before truncation.
const MAX_TOOL_RESULT_CHARS: usize = 4096;

/// Compute a u64 hash key for a (tool_name, input_json) pair.
fn tool_call_hash(name: &str, input: &serde_json::Value) -> u64 {
    let mut hasher = std::collections::hash_map::DefaultHasher::new();
    name.hash(&mut hasher);
    // Canonical JSON string ensures identical inputs produce identical hashes
    serde_json::to_string(input)
        .unwrap_or_default()
        .hash(&mut hasher);
    hasher.finish()
}

/// Per-call input for `run_agent_turn`, grouping the request-specific parameters.
pub struct AgentTurnInput<'a> {
    pub user_input: Option<&'a str>,
    pub job_id: Option<&'a str>,
    pub correlation_id: &'a str,
    /// When set, triggers first-run onboarding prompt (path to identity.md).
    pub soul_path: Option<&'a str>,
}

/// Internal context passed to `run_turn_loop`, grouping agent-config parameters.
struct TurnContext<'a> {
    soul: &'a Soul,
    system_prompt: &'a str,
    tools: &'a [provider::ToolDefinition],
}

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
    #[error("Turn timed out after {0}s")]
    TurnTimeout(u64),
}

pub type Result<T> = std::result::Result<T, AgentError>;

/// Create a lightweight Zenoh session for the agent (client mode).
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
///
/// The `sink` parameter controls where output goes (stdout, Zenoh, etc.).
/// The `correlation_id` tags all emitted events for response correlation.
/// The `job_notify` wakes the agent loop immediately when a near-future job is scheduled.
pub async fn run_agent_turn<
    M: ModelProvider,
    P: crate::mcp::platform::PlatformOperations,
    S: EventSink,
>(
    provider: &M,
    dispatcher: &Dispatcher<P>,
    memory: &mut Memory,
    soul: &Soul,
    sink: &S,
    input: &AgentTurnInput<'_>,
) -> Result<()> {
    let user_input = input.user_input;
    let job_id = input.job_id;
    let correlation_id = input.correlation_id;
    // 1. Build system prompt
    let inventory = dispatcher.get_node_inventory().await;
    let (active_jobs, relevant_episodes, recent_plan, recovered_context) = {
        let backend = memory.backend.lock().await;
        let active_jobs = backend.semantic.pending_jobs().unwrap_or_default();
        let decay_half_life = soul.capabilities.episodic_decay_half_life_days;
        let relevant_episodes = match user_input {
            Some(input) => backend
                .episodic
                .search_with_decay(input, 5, decay_half_life)
                .unwrap_or_default(),
            None => Vec::new(),
        };
        let recent_plan = backend
            .episodic
            .latest_plan()
            .ok()
            .flatten()
            .map(|e| e.content);
        let recovered_context = backend
            .episodic
            .latest_flush()
            .ok()
            .flatten()
            .map(|e| EpisodicLog::strip_flush_prefix(&e.content).to_string());
        (
            active_jobs,
            relevant_episodes,
            recent_plan,
            recovered_context,
        )
    };
    let resource_summary = dispatcher.telemetry_prompt_summary().await;
    let system_prompt = prompt::build_system_prompt_with_soul_path(
        soul,
        &inventory,
        &active_jobs,
        &relevant_episodes,
        recent_plan.as_deref(),
        recovered_context.as_deref(),
        resource_summary.as_deref(),
        input.soul_path,
    );

    // Add user input to short-term memory
    if let Some(input) = user_input {
        memory.short_term.push(Message::user(input));
        // Log to episodic
        let entry = EpisodicLog::make_entry("user", input, job_id);
        let backend = memory.backend.lock().await;
        if let Err(e) = backend.episodic.append(&entry) {
            log::warn!("Failed to log user message to episodic: {}", e);
        }
    }

    let tools = Dispatcher::<P>::tool_definitions();

    // 2. Turn cycle (max_turns) — wrapped in a turn-level timeout
    let turn_result = tokio::time::timeout(
        Duration::from_secs(TURN_TIMEOUT_SECS),
        run_turn_loop(
            provider,
            dispatcher,
            memory,
            &TurnContext {
                soul,
                system_prompt: &system_prompt,
                tools: &tools,
            },
            sink,
            input,
        ),
    )
    .await;

    match turn_result {
        Ok(inner) => inner?,
        Err(_elapsed) => {
            log::error!("[Agent] Turn timed out after {}s", TURN_TIMEOUT_SECS);
            sink.emit(AgentEvent::error(
                correlation_id,
                &format!("Turn timed out after {}s", TURN_TIMEOUT_SECS),
            ))
            .await;
            return Err(AgentError::TurnTimeout(TURN_TIMEOUT_SECS));
        }
    }

    // 3. Signal turn complete
    sink.emit(AgentEvent::done(correlation_id)).await;

    // 4. Trim short-term memory
    if memory.short_term.len() > MAX_CONVERSATION_MESSAGES {
        let drain_count = memory.short_term.len() - MAX_CONVERSATION_MESSAGES;
        memory.short_term.drain(..drain_count);
    }

    Ok(())
}

/// Inner turn loop extracted for timeout wrapping.
async fn run_turn_loop<
    M: ModelProvider,
    P: crate::mcp::platform::PlatformOperations,
    S: EventSink,
>(
    provider: &M,
    dispatcher: &Dispatcher<P>,
    memory: &mut Memory,
    ctx: &TurnContext<'_>,
    sink: &S,
    input: &AgentTurnInput<'_>,
) -> Result<()> {
    let soul = ctx.soul;
    let system_prompt = ctx.system_prompt;
    let tools = ctx.tools;
    let job_id = input.job_id;
    let correlation_id = input.correlation_id;
    let mut last_input_tokens = 0u32;
    let mut tool_call_counts: HashMap<u64, u32> = HashMap::new();
    let mut loop_detected = false;
    for _turn in 0..soul.capabilities.max_turns {
        if loop_detected {
            break;
        }
        // Stream response from provider (retry handled inside ClaudeProvider)
        let mut rx = match provider
            .generate_stream(Some(system_prompt), &memory.short_term, tools)
            .await
        {
            Ok(r) => r,
            Err(e) => {
                log::error!("Provider error: {}", e);
                sink.emit(AgentEvent::error(correlation_id, &e.to_string()))
                    .await;
                break;
            }
        };

        let mut text_buffer = String::new();
        let mut tool_calls: Vec<ToolCall> = Vec::new();
        let mut stream_usage = Usage {
            input_tokens: 0,
            output_tokens: 0,
        };
        let mut _stop_reason: Option<String> = None;

        while let Some(event) = rx.recv().await {
            match event {
                StreamEvent::TextDelta(delta) => {
                    sink.emit(AgentEvent::delta(correlation_id, &delta)).await;
                    text_buffer.push_str(&delta);
                }
                StreamEvent::ToolUse { id, name, input } => {
                    tool_calls.push(ToolCall { id, name, input });
                }
                StreamEvent::Done {
                    usage,
                    stop_reason: sr,
                } => {
                    stream_usage = usage;
                    _stop_reason = sr;
                }
                StreamEvent::Error(e) => {
                    log::error!("Stream error: {}", e);
                    break;
                }
            }
        }
        if !text_buffer.is_empty() {
            // StdoutSink handles its own newline via Delta events; for Zenoh
            // the client renders newlines. Emit a final empty delta for newline.
            sink.emit(AgentEvent::delta(correlation_id, "\n")).await;
        }

        last_input_tokens = stream_usage.input_tokens;

        // Build assistant message from streamed content
        let mut content_blocks = Vec::new();
        if !text_buffer.is_empty() {
            content_blocks.push(ContentBlock::Text {
                text: text_buffer.clone(),
            });
        }
        for tc in &tool_calls {
            content_blocks.push(ContentBlock::ToolUse {
                id: tc.id.clone(),
                name: tc.name.clone(),
                input: tc.input.clone(),
            });
        }
        let assistant_msg = Message {
            role: "assistant".to_string(),
            content: content_blocks,
        };
        memory.short_term.push(assistant_msg);

        // Log text to episodic
        let text = text_buffer;
        if !text.is_empty() {
            let entry = EpisodicLog::make_entry("assistant", &text, job_id);
            let backend = memory.backend.lock().await;
            if let Err(e) = backend.episodic.append(&entry) {
                log::warn!("Failed to log assistant message to episodic: {}", e);
            }
        }

        // Check for tool calls

        // Plan detection: if model outputs BOTH text AND tool calls, the text is
        // likely a plan/reasoning (Ralph Loop: Plan → Execute). Persist it.
        if !text.is_empty() && !tool_calls.is_empty() {
            let plan_entry = EpisodicLog::make_entry("plan", &text, job_id);
            let backend = memory.backend.lock().await;
            if let Err(e) = backend.episodic.append(&plan_entry) {
                log::warn!("Failed to persist plan to episodic: {}", e);
            }
        }

        if tool_calls.is_empty() {
            break; // Text-only response, turn complete
        }

        // Dispatch tool calls
        let mut tool_results = Vec::new();
        for tc in &tool_calls {
            // Loop detection: hash (name, input) and count duplicates
            let call_key = tool_call_hash(&tc.name, &tc.input);
            let count = tool_call_counts.entry(call_key).or_insert(0);
            *count += 1;
            if *count == LOOP_WARN_THRESHOLD {
                log::warn!(
                    "[Agent] Repeated tool call detected: {} ({}x)",
                    tc.name,
                    LOOP_WARN_THRESHOLD
                );
            }
            if *count >= LOOP_BREAK_THRESHOLD {
                sink.emit(AgentEvent::error(
                    correlation_id,
                    &format!(
                        "loop detected: {} called {}x with same args, stopping",
                        tc.name, count
                    ),
                ))
                .await;
                memory.short_term.push(Message::user(&format!(
                    "SYSTEM: Loop detected — you've called the same tool {} times with identical \
                         arguments. Stop repeating and summarize what you've accomplished so far.",
                    LOOP_BREAK_THRESHOLD
                )));
                loop_detected = true;
                break;
            }

            sink.emit(AgentEvent::tool(correlation_id, &tc.name)).await;
            log::info!("[Agent] calling tool: {}", tc.name);

            // All tools dispatched through the Dispatcher (memory tools included)
            let result = match tokio::time::timeout(
                Duration::from_secs(TOOL_CALL_TIMEOUT_SECS),
                dispatcher.call_tool(&tc.id, &tc.name, &tc.input),
            )
            .await
            {
                Ok(r) => r,
                Err(_elapsed) => {
                    log::warn!(
                        "[Agent] Tool '{}' timed out after {}s",
                        tc.name,
                        TOOL_CALL_TIMEOUT_SECS
                    );
                    ContentBlock::ToolResult {
                        tool_use_id: tc.id.clone(),
                        content: format!(
                            "Error: tool '{}' timed out after {}s",
                            tc.name, TOOL_CALL_TIMEOUT_SECS
                        ),
                        is_error: Some(true),
                    }
                }
            };

            if let ContentBlock::ToolResult {
                tool_use_id,
                content,
                is_error,
            } = &result
            {
                // Truncate large tool results to prevent context blow-up
                let content = truncate_tool_result(content);
                tool_results.push((tool_use_id.clone(), content.clone(), *is_error));
                // Log tool result to episodic
                let entry = EpisodicLog::make_entry("tool", &content, job_id);
                let backend = memory.backend.lock().await;
                if let Err(e) = backend.episodic.append(&entry) {
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
            .generate(Some(system_prompt), &memory.short_term, tools)
            .await
        {
            let flush_text = flush_response.text();
            if is_flush_substantive(&flush_text) {
                // Log flush to episodic with flush: true tag
                let entry = EpisodicLog::make_flush_entry(&flush_text, job_id);
                let backend = memory.backend.lock().await;
                if let Err(e) = backend.episodic.append(&entry) {
                    log::warn!("Failed to log flush to episodic: {}", e);
                }
                log::info!("[Agent] Flush persisted ({} chars)", flush_text.len());
            } else {
                log::debug!("[Agent] Flush skipped (not substantive)");
            }
        }
    }

    Ok(())
}

/// Truncate a tool result string if it exceeds `MAX_TOOL_RESULT_CHARS`.
///
/// Uses char-boundary-safe slicing to avoid panicking on multi-byte UTF-8.
fn truncate_tool_result(content: &str) -> String {
    if content.len() <= MAX_TOOL_RESULT_CHARS {
        content.to_string()
    } else {
        // Find the nearest char boundary at or before the limit
        let end = (0..=MAX_TOOL_RESULT_CHARS)
            .rev()
            .find(|&i| content.is_char_boundary(i))
            .unwrap_or(0);
        format!(
            "{}\n\n[Output truncated at {} chars. Use more specific queries to get detailed results.]",
            &content[..end], MAX_TOOL_RESULT_CHARS
        )
    }
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

    #[test]
    fn loop_detection_identical_calls_same_hash() {
        let input = serde_json::json!({"node": "camera"});
        let h1 = tool_call_hash("list_nodes", &input);
        let h2 = tool_call_hash("list_nodes", &input);
        assert_eq!(h1, h2, "identical tool calls must produce the same hash");
    }

    #[test]
    fn loop_detection_different_inputs_different_hash() {
        let input_a = serde_json::json!({"node": "camera"});
        let input_b = serde_json::json!({"node": "weather"});
        let h1 = tool_call_hash("list_nodes", &input_a);
        let h2 = tool_call_hash("list_nodes", &input_b);
        assert_ne!(h1, h2, "different inputs must produce different hashes");
    }

    #[test]
    fn loop_detection_different_tools_different_hash() {
        let input = serde_json::json!({});
        let h1 = tool_call_hash("list_nodes", &input);
        let h2 = tool_call_hash("get_health", &input);
        assert_ne!(h1, h2, "different tool names must produce different hashes");
    }

    #[test]
    fn loop_detection_thresholds() {
        assert_eq!(LOOP_WARN_THRESHOLD, 3);
        assert_eq!(LOOP_BREAK_THRESHOLD, 6);
        // Verify warn < break (const assertion would be cleaner but clippy
        // flags assert! on constants in test functions)
        let warn = LOOP_WARN_THRESHOLD;
        let brk = LOOP_BREAK_THRESHOLD;
        assert!(warn < brk, "warn threshold must be below break threshold");
    }

    #[test]
    fn timeout_constants_are_sane() {
        assert_eq!(TURN_TIMEOUT_SECS, 120);
        assert_eq!(TOOL_CALL_TIMEOUT_SECS, 30);
        const { assert!(TOOL_CALL_TIMEOUT_SECS < TURN_TIMEOUT_SECS) };
    }

    #[test]
    fn truncate_short_content_unchanged() {
        let short = "Hello, world!";
        assert_eq!(truncate_tool_result(short), short);
    }

    #[test]
    fn truncate_exact_limit_unchanged() {
        let exact = "a".repeat(MAX_TOOL_RESULT_CHARS);
        assert_eq!(truncate_tool_result(&exact), exact);
    }

    #[test]
    fn truncate_over_limit_adds_suffix() {
        let long = "x".repeat(MAX_TOOL_RESULT_CHARS + 100);
        let result = truncate_tool_result(&long);
        assert!(result.len() < long.len() + 100); // truncated + suffix
        assert!(result.starts_with(&"x".repeat(MAX_TOOL_RESULT_CHARS)));
        assert!(result.contains("[Output truncated at"));
    }

    #[test]
    fn truncate_max_tool_result_chars_constant() {
        assert_eq!(MAX_TOOL_RESULT_CHARS, 4096);
    }
}
