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

/// Configuration for the interactive agent session.
pub struct AgentConfig {
    /// Claude model override (uses default if None).
    pub model: Option<String>,
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
) -> anyhow::Result<()> {
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
    let dispatcher = Dispatcher::new(platform.clone(), scope, machine_id);

    // 3. Get tool definitions
    let tools = Dispatcher::<DaemonPlatform>::tool_definitions();

    // 4. Open memory store
    let memory_path = get_bubbaloop_home().join("memory.db");
    let memory = Memory::open(&memory_path)?;

    // 5. Welcome message
    println!(
        "Bubbaloop Agent ready ({} tools available). Type 'quit' or 'exit' to stop.",
        tools.len()
    );

    // 6. Main REPL loop
    let stdin = std::io::stdin();
    let mut conversation: Vec<Message> = Vec::new();

    loop {
        // Read user input
        print!("> ");
        // Flush stdout so the prompt appears before blocking on stdin
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

            // Dispatch tool calls
            let mut tool_results = Vec::new();
            for (id, name, input) in &tool_uses {
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
    }

    println!("Goodbye.");
    Ok(())
}
