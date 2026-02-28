/// Raw reqwest Claude API client with tool_use support.
pub mod claude;

/// Internal MCP tool dispatch — calls PlatformOperations directly.
pub mod dispatch;

/// SQLite memory layer — conversations, sensor events, schedules.
pub mod memory;

/// System prompt builder — injects live sensor inventory and schedules.
pub mod prompt;

/// Cron-based scheduler with Tier 1 built-in actions.
pub mod scheduler;

use crate::agent::claude::{ClaudeClient, ContentBlock, Message};
use crate::agent::dispatch::Dispatcher;
use crate::agent::memory::Memory;
use crate::agent::prompt::build_system_prompt;
use crate::daemon::registry::get_bubbaloop_home;
use crate::mcp::platform::DaemonPlatform;
use std::sync::Arc;

/// Maximum number of conversation messages to keep in context.
///
/// Older messages are dropped to avoid exceeding Claude's context window.
/// Each pair (user + assistant) counts as 2, so 40 messages ≈ 20 exchanges.
const MAX_CONVERSATION_MESSAGES: usize = 40;

/// Errors from agent operations.
#[derive(Debug, thiserror::Error)]
pub enum AgentError {
    #[error("Claude API error: {0}")]
    Claude(#[from] claude::ClaudeError),
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
    /// Claude model override (uses default if None).
    pub model: Option<String>,
}

/// Create a lightweight Zenoh session for the agent.
///
/// Uses client mode with disabled scouting for fast startup (~0s instead of
/// ~5s). Falls back gracefully — tools that need Zenoh will return errors
/// but the chat loop still works.
pub async fn create_agent_session(endpoint: Option<&str>) -> Result<Arc<zenoh::Session>> {
    let mut config = zenoh::Config::default();

    // Client mode — lighter weight, no routing
    config
        .insert_json5("mode", "\"client\"")
        .expect("Failed to set Zenoh mode");

    // Resolve endpoint
    let env_endpoint = std::env::var("BUBBALOOP_ZENOH_ENDPOINT").ok();
    let ep = endpoint
        .or(env_endpoint.as_deref())
        .unwrap_or("tcp/127.0.0.1:7447");

    config
        .insert_json5("connect/endpoints", &format!("[\"{}\"]", ep))
        .expect("Failed to set Zenoh endpoint");

    // Disable scouting entirely for instant startup
    config
        .insert_json5("scouting/multicast/enabled", "false")
        .expect("Failed to disable multicast scouting");
    config
        .insert_json5("scouting/gossip/enabled", "false")
        .expect("Failed to disable gossip scouting");

    // Single attempt — don't block if router is down
    let session = zenoh::open(config)
        .await
        .map_err(|e| AgentError::Zenoh(e.to_string()))?;

    Ok(Arc::new(session))
}

/// Run the interactive agent REPL.
///
/// Connects to the Claude API, initialises the tool dispatcher and memory
/// store, then loops reading user input from stdin. Each message is sent
/// to Claude along with the live system prompt (sensor inventory, schedules,
/// recent events). Tool-use responses are dispatched and fed back until
/// the model produces a final text reply.
pub async fn run_agent(
    config: AgentConfig,
    session: Arc<zenoh::Session>,
    node_manager: Arc<crate::daemon::node_manager::NodeManager>,
) -> Result<()> {
    // 1. Initialise Claude client from ANTHROPIC_API_KEY env var
    let client = ClaudeClient::from_env(config.model.as_deref())?;

    // 2. Create dispatcher with DaemonPlatform
    let scope = std::env::var("BUBBALOOP_SCOPE").unwrap_or_else(|_| "local".to_string());
    let machine_id = crate::daemon::util::get_machine_id();
    let platform = Arc::new(DaemonPlatform {
        node_manager,
        session,
        scope: scope.clone(),
        machine_id: machine_id.clone(),
    });
    let sched_scope = scope.clone();
    let sched_machine_id = machine_id.clone();
    let dispatcher = Dispatcher::new(platform.clone(), scope, machine_id);

    // 3. Get tool definitions
    let tools = Dispatcher::<DaemonPlatform>::tool_definitions();

    // 4. Open memory store
    let memory_path = get_bubbaloop_home().join("memory.db");
    let memory = Memory::open(&memory_path)?;

    // 5. Start scheduler in background
    let (shutdown_tx, shutdown_rx) = tokio::sync::watch::channel(());
    tokio::spawn(scheduler::run_scheduler(
        memory_path.clone(),
        platform.clone(),
        sched_scope,
        sched_machine_id,
        shutdown_rx,
    ));

    // 6. Welcome message
    let model_name = config.model.as_deref().unwrap_or(claude::DEFAULT_MODEL);
    let node_count = match dispatcher.get_node_inventory().await {
        ref s if s.starts_with("No nodes") => "0".to_string(),
        ref s => s
            .lines()
            .next()
            .and_then(|l| l.split_whitespace().next())
            .unwrap_or("0")
            .to_string(),
    };
    println!();
    println!("  Bubbaloop Agent v{}", env!("CARGO_PKG_VERSION"));
    println!("  Model: {}", model_name);
    println!("  Tools: {} | Nodes: {}", tools.len(), node_count);
    println!();
    println!("  Type a message to chat, 'quit' to exit.");

    // 7. Main REPL loop
    let stdin = std::io::stdin();
    let mut conversation: Vec<Message> = Vec::new();

    loop {
        // Read user input
        print!("> ");
        use std::io::Write;
        std::io::stdout().flush().ok();

        let mut line = String::new();
        let bytes_read = stdin.read_line(&mut line)?;

        // EOF (Ctrl-D)
        if bytes_read == 0 {
            println!();
            break;
        }

        let input = line.trim();
        if input.is_empty() {
            continue;
        }
        if input == "quit" || input == "exit" {
            break;
        }

        // Build live system prompt
        let inventory = dispatcher.get_node_inventory().await;
        let schedules = memory.list_schedules().unwrap_or_default();
        let recent_events = memory.recent_events(10).unwrap_or_default();
        let system_prompt = build_system_prompt(&inventory, &schedules, &recent_events);

        // Add user message
        conversation.push(Message::user(input));

        // Log user message to memory
        if let Err(e) = memory.log_message("user", input, None) {
            log::warn!("Failed to log user message: {}", e);
        }

        // Send to Claude and handle tool-use loop
        loop {
            let response = match client
                .send(Some(&system_prompt), &conversation, &tools)
                .await
            {
                Ok(r) => r,
                Err(e) => {
                    println!("Error: {}", e);
                    log::error!("Claude API error: {}", e);
                    break;
                }
            };

            // Build assistant message from response content
            let assistant_msg = Message {
                role: "assistant".to_string(),
                content: response.content.clone(),
            };
            conversation.push(assistant_msg);

            // Print any text blocks
            for block in &response.content {
                if let ContentBlock::Text { text } = block {
                    println!("{}", text);
                }
            }

            // Check for tool calls
            let tool_uses: Vec<_> = response
                .content
                .iter()
                .filter_map(|b| match b {
                    ContentBlock::ToolUse { id, name, input } => Some((id, name, input)),
                    _ => None,
                })
                .collect();

            if tool_uses.is_empty() {
                // No tool calls — log assistant response and break
                let response_text: String = response
                    .content
                    .iter()
                    .filter_map(|b| match b {
                        ContentBlock::Text { text } => Some(text.as_str()),
                        _ => None,
                    })
                    .collect::<Vec<_>>()
                    .join("");
                if let Err(e) = memory.log_message("assistant", &response_text, None) {
                    log::warn!("Failed to log assistant message: {}", e);
                }
                break;
            }

            // Dispatch tool calls — show progress to user
            let mut tool_results = Vec::new();
            for (id, name, input) in &tool_uses {
                println!("  [calling {}...]", name);
                log::info!("[Agent] calling tool: {}", name);
                let result = dispatcher.call_tool(id, name, input).await;
                if let ContentBlock::ToolResult {
                    tool_use_id,
                    content,
                    is_error,
                } = &result
                {
                    tool_results.push((tool_use_id.clone(), content.clone(), *is_error));
                }
            }

            // Log tool calls to memory
            let tool_calls_json = serde_json::to_string(
                &tool_uses
                    .iter()
                    .map(|(_, name, _)| name)
                    .collect::<Vec<_>>(),
            )
            .unwrap_or_default();
            if let Err(e) = memory.log_message("assistant", "", Some(&tool_calls_json)) {
                log::warn!("Failed to log tool calls: {}", e);
            }

            // Add tool results to conversation
            conversation.push(Message::tool_results(tool_results));
        }

        // Trim conversation to avoid blowing the context window.
        // Keep the most recent messages, dropping from the front.
        if conversation.len() > MAX_CONVERSATION_MESSAGES {
            let drain_count = conversation.len() - MAX_CONVERSATION_MESSAGES;
            conversation.drain(..drain_count);
        }
    }

    // Signal scheduler to shut down
    drop(shutdown_tx);

    println!("Goodbye.");
    Ok(())
}
