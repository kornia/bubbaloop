//! Thin Zenoh CLI client for agent communication.
//!
//! Publishes messages to the shared agent inbox and renders outbox events.
//! All LLM processing happens daemon-side — this is purely I/O.
//!
//! The daemon must be started separately (`bubbaloop daemon start`).

use crate::agent::gateway::{self, AgentEvent, AgentEventType, AgentManifest, AgentMessage};
use std::sync::Arc;
use std::time::Duration;
use zenoh::Session;

/// Timeout waiting for first response from daemon.
const RESPONSE_TIMEOUT: Duration = Duration::from_secs(60);

/// Check if the daemon is reachable by querying the daemon manifest.
pub async fn is_daemon_running(session: &Arc<Session>, scope: &str, machine_id: &str) -> bool {
    let pattern = crate::daemon::gateway::manifest_topic(scope, machine_id);
    match session
        .get(&pattern)
        .target(zenoh::query::QueryTarget::BestMatching)
        .timeout(Duration::from_secs(2))
        .await
    {
        Ok(replies) => replies.recv_async().await.is_ok(),
        Err(_) => false,
    }
}

/// Run a single message or interactive REPL via Zenoh.
pub async fn run_agent_client(
    session: Arc<Session>,
    scope: &str,
    machine_id: &str,
    agent: Option<&str>,
    message: Option<&str>,
    verbose: bool,
) -> Result<(), Box<dyn std::error::Error>> {
    if let Some(msg) = message {
        // Single-message mode: send and wait for Done (plain stdout, no TUI)
        send_and_render(&session, scope, machine_id, agent, msg, verbose).await?;
    } else {
        // Interactive REPL mode: ratatui two-panel TUI
        run_tui_repl(&session, scope, machine_id, agent, verbose).await?;
    }
    Ok(())
}

/// Send a single message and render the streamed response (plain stdout, used for single-message mode).
async fn send_and_render(
    session: &Arc<Session>,
    scope: &str,
    machine_id: &str,
    agent: Option<&str>,
    text: &str,
    verbose: bool,
) -> Result<(), Box<dyn std::error::Error>> {
    let correlation_id = uuid::Uuid::new_v4().to_string();

    // Subscribe to outbox BEFORE publishing (avoid missing early events)
    let outbox_pattern = gateway::outbox_wildcard(scope, machine_id);
    let subscriber = session
        .declare_subscriber(&outbox_pattern)
        .await
        .map_err(|e| format!("Failed to subscribe to outbox: {}", e))?;

    // Publish to inbox
    let inbox = gateway::inbox_topic(scope, machine_id);
    let msg = AgentMessage {
        id: correlation_id.clone(),
        text: text.to_string(),
        agent: agent.map(|s| s.to_string()),
    };
    let payload = serde_json::to_vec(&msg)?;
    session
        .put(&inbox, payload)
        .await
        .map_err(|e| format!("Failed to publish to inbox: {}", e))?;

    // Render outbox events, filtering by correlation ID
    let mut got_first = false;
    let mut shown_agent_header = false;
    loop {
        let timeout = if got_first {
            Duration::from_secs(120) // generous timeout during streaming
        } else {
            RESPONSE_TIMEOUT
        };

        tokio::select! {
            result = subscriber.recv_async() => {
                match result {
                    Ok(sample) => {
                        // Extract agent_id from topic key expression:
                        // bubbaloop/global/{machine}/agent/{agent_id}/outbox
                        let agent_id_from_topic = sample.key_expr().as_str()
                            .split('/')
                            .nth(4)
                            .unwrap_or("agent");

                        let bytes = sample.payload().to_bytes();
                        if let Ok(event) = serde_json::from_slice::<AgentEvent>(&bytes) {
                            if event.id != correlation_id {
                                continue; // Not our message
                            }
                            got_first = true;
                            match event.event_type {
                                AgentEventType::Delta => {
                                    if !shown_agent_header {
                                        println!("\n[{}]", agent_id_from_topic);
                                        shown_agent_header = true;
                                    }
                                    if let Some(text) = &event.text {
                                        print!("{}", text);
                                        std::io::Write::flush(&mut std::io::stdout()).ok();
                                    }
                                }
                                AgentEventType::Tool => {
                                    if let Some(name) = &event.text {
                                        if verbose {
                                            match event.input.as_deref().filter(|s| !s.is_empty() && *s != "{}") {
                                                Some(inp) => println!("  [calling {} {}]", name, inp),
                                                None => println!("  [calling {}]", name),
                                            }
                                        } else {
                                            println!("  [calling {}...]", name);
                                        }
                                    }
                                }
                                AgentEventType::ToolResult => {
                                    if let Some(result) = &event.text {
                                        let limit = if verbose { 300 } else { 120 };
                                        println!("  → {}", truncate_with_ellipsis(result, limit));
                                    }
                                }
                                AgentEventType::System => {
                                    if verbose {
                                        if let Some(msg) = &event.text {
                                            eprintln!("  ⟳ {}", msg);
                                        }
                                    }
                                }
                                AgentEventType::Error => {
                                    if let Some(msg) = &event.text {
                                        eprintln!("Error: {}", msg);
                                    }
                                }
                                AgentEventType::Done => {
                                    println!();
                                    return Ok(());
                                }
                            }
                        }
                    }
                    Err(e) => {
                        return Err(format!("Outbox subscription error: {}", e).into());
                    }
                }
            }
            _ = tokio::time::sleep(timeout) => {
                if !got_first {
                    eprintln!("Error: No response from daemon within {}s. Is `bubbaloop up` running?", RESPONSE_TIMEOUT.as_secs());
                } else {
                    eprintln!("Error: Response timed out during streaming.");
                }
                return Err("timeout".into());
            }
        }
    }
}

// ── TUI REPL ──────────────────────────────────────────────────────────────────

use crossterm::{
    event::{Event, EventStream, KeyCode, KeyModifiers},
    execute,
    terminal::{disable_raw_mode, enable_raw_mode, EnterAlternateScreen, LeaveAlternateScreen},
};
use futures::StreamExt;
use ratatui::{
    backend::CrosstermBackend,
    layout::{Constraint, Direction, Layout},
    style::{Color, Modifier, Style},
    text::{Line, Span},
    widgets::{Block, Borders, List, ListItem, Paragraph, Wrap},
    Terminal,
};

/// Output line kinds — drive colour rendering.
#[derive(Clone)]
enum OutputLine {
    UserMessage(String),
    AgentHeader(String),
    AgentDelta(String),
    ToolCall(String),
    ToolResult(String),
    ErrorLine(String),
    Separator,
    Info(String),
    /// Dim blue system lifecycle event (world state, memory, turn counter).
    SystemInfo(String),
}

/// RAII guard: restore terminal on drop (handles panics too).
struct TerminalGuard;

impl Drop for TerminalGuard {
    fn drop(&mut self) {
        let _ = disable_raw_mode();
        let _ = execute!(std::io::stdout(), LeaveAlternateScreen);
    }
}

/// Two-panel ratatui REPL.
///
/// Top panel: scrolling output.
/// Bottom panel (3 rows): input prompt, always visible.
async fn run_tui_repl(
    session: &Arc<Session>,
    scope: &str,
    machine_id: &str,
    agent: Option<&str>,
    verbose: bool,
) -> Result<(), Box<dyn std::error::Error>> {
    // ── Terminal setup ────────────────────────────────────────────────────────
    enable_raw_mode()?;
    let mut stdout = std::io::stdout();
    execute!(stdout, EnterAlternateScreen)?;
    let backend = CrosstermBackend::new(stdout);
    let mut terminal = Terminal::new(backend)?;
    let _guard = TerminalGuard; // restored on drop

    // ── Subscribe to outbox once, shared for all turns ────────────────────────
    let outbox_pattern = gateway::outbox_wildcard(scope, machine_id);
    let subscriber = session
        .declare_subscriber(&outbox_pattern)
        .await
        .map_err(|e| format!("Failed to subscribe to outbox: {}", e))?;

    // ── App state ─────────────────────────────────────────────────────────────
    let mut output: Vec<OutputLine> = Vec::new();
    let mut input = String::new();
    let mut scroll_offset: usize = 0;
    let mut waiting_for_agent = false;
    let mut current_correlation_id = String::new();
    let mut agent_header_shown = false;

    // Welcome banner
    output.push(OutputLine::Info(format!(
        "bubbaloop agent v{} — Ctrl-C or 'quit' to exit",
        env!("CARGO_PKG_VERSION")
    )));
    if let Some(a) = agent {
        output.push(OutputLine::Info(format!("Target agent: {}", a)));
    }
    output.push(OutputLine::Separator);

    let mut event_stream = EventStream::new();

    loop {
        // ── Render ────────────────────────────────────────────────────────────
        terminal.draw(|frame| {
            let total = frame.area();
            // Layout: output takes all but 3 bottom rows for input
            let chunks = Layout::default()
                .direction(Direction::Vertical)
                .constraints([Constraint::Min(3), Constraint::Length(3)])
                .split(total);

            let output_area = chunks[0];
            let input_area = chunks[1];

            // Build coloured output lines
            let items: Vec<ListItem> = output
                .iter()
                .flat_map(|line| render_output_line(line))
                .collect();

            // Auto-scroll: 0 means "follow bottom"; otherwise clamp to max
            let visible_rows = output_area.height.saturating_sub(2) as usize;
            let max_scroll = items.len().saturating_sub(visible_rows);
            let scroll = if scroll_offset == 0 {
                max_scroll
            } else {
                scroll_offset.min(max_scroll)
            };

            // Slice visible items
            let visible: Vec<ListItem> =
                items.into_iter().skip(scroll).take(visible_rows).collect();

            let output_widget = List::new(visible).block(
                Block::default()
                    .borders(Borders::ALL)
                    .border_style(Style::default().fg(Color::DarkGray))
                    .title(Span::styled(
                        " output ",
                        Style::default()
                            .fg(Color::Cyan)
                            .add_modifier(Modifier::BOLD),
                    )),
            );
            frame.render_widget(output_widget, output_area);

            // Input prompt
            let prompt_prefix = if waiting_for_agent { "⏳ " } else { "▸ " };
            let prompt_style = if waiting_for_agent {
                Style::default().fg(Color::DarkGray)
            } else {
                Style::default()
                    .fg(Color::Green)
                    .add_modifier(Modifier::BOLD)
            };
            let input_paragraph = Paragraph::new(Line::from(vec![
                Span::styled(prompt_prefix, prompt_style),
                Span::styled(input.as_str(), Style::default().fg(Color::White)),
            ]))
            .wrap(Wrap { trim: false })
            .block(
                Block::default()
                    .borders(Borders::ALL)
                    .border_style(if waiting_for_agent {
                        Style::default().fg(Color::DarkGray)
                    } else {
                        Style::default().fg(Color::Green)
                    })
                    .title(Span::styled(
                        " type a message ",
                        Style::default().fg(Color::DarkGray),
                    )),
            );
            frame.render_widget(input_paragraph, input_area);
        })?;

        // ── Event loop ────────────────────────────────────────────────────────
        tokio::select! {
            // Keyboard input
            maybe_event = event_stream.next() => {
                match maybe_event {
                    Some(Ok(Event::Key(key))) => {
                        match (key.code, key.modifiers) {
                            (KeyCode::Char('c'), KeyModifiers::CONTROL) |
                            (KeyCode::Char('q'), KeyModifiers::NONE) if input.is_empty() => {
                                break; // quit
                            }
                            (KeyCode::Enter, _) => {
                                let trimmed = input.trim().to_string();
                                if trimmed == "quit" || trimmed == "exit" {
                                    break;
                                }
                                if !trimmed.is_empty() && !waiting_for_agent {
                                    // Echo user message to output
                                    output.push(OutputLine::UserMessage(trimmed.clone()));
                                    scroll_offset = 0; // snap to bottom

                                    // Send to daemon
                                    let cid = uuid::Uuid::new_v4().to_string();
                                    current_correlation_id = cid.clone();
                                    agent_header_shown = false;
                                    waiting_for_agent = true;

                                    let inbox = gateway::inbox_topic(scope, machine_id);
                                    let msg = AgentMessage {
                                        id: cid,
                                        text: trimmed,
                                        agent: agent.map(|s| s.to_string()),
                                    };
                                    if let Ok(payload) = serde_json::to_vec(&msg) {
                                        let _ = session.put(&inbox, payload).await;
                                    }
                                    input.clear();
                                }
                            }
                            (KeyCode::Backspace, _) => {
                                input.pop();
                            }
                            (KeyCode::Char(c), _) => {
                                input.push(c);
                            }
                            (KeyCode::Up, _) => {
                                scroll_offset = scroll_offset.saturating_add(1);
                            }
                            (KeyCode::Down, _) => {
                                scroll_offset = scroll_offset.saturating_sub(1);
                            }
                            (KeyCode::PageUp, _) => {
                                scroll_offset = scroll_offset.saturating_add(10);
                            }
                            (KeyCode::PageDown, _) => {
                                scroll_offset = scroll_offset.saturating_sub(10);
                            }
                            _ => {}
                        }
                    }
                    Some(Err(_)) | None => break,
                    _ => {}
                }
            }

            // Zenoh outbox events — always drain the subscriber, never gate on
            // waiting_for_agent. If gated, Done arriving before ToolResult in
            // the buffer causes the guard to flip to false and drops ToolResult.
            // The correlation_id filter below handles routing correctly.
            result = subscriber.recv_async() => {
                if let Ok(sample) = result {
                    let agent_id_from_topic = sample
                        .key_expr()
                        .as_str()
                        .split('/')
                        .nth(4)
                        .unwrap_or("agent")
                        .to_string();

                    let bytes = sample.payload().to_bytes();
                    let event = match serde_json::from_slice::<AgentEvent>(&bytes) {
                        Ok(e) if e.id == current_correlation_id => e,
                        _ => continue, // not ours or unparseable
                    };

                    match event.event_type {
                        AgentEventType::Delta => {
                            if !agent_header_shown {
                                output.push(OutputLine::AgentHeader(agent_id_from_topic));
                                agent_header_shown = true;
                            }
                            if let Some(text) = event.text {
                                if let Some(OutputLine::AgentDelta(last)) = output.last_mut() {
                                    last.push_str(&text);
                                } else {
                                    output.push(OutputLine::AgentDelta(text));
                                }
                                scroll_offset = 0;
                            }
                        }
                        AgentEventType::Tool => {
                            if let Some(name) = event.text {
                                let label = if verbose {
                                    // Don't show "{}" when input is absent or empty object
                                    match event.input.as_deref().filter(|s| !s.is_empty() && *s != "{}") {
                                        Some(inp) => format!("⚙ {} {}", name, inp),
                                        None => format!("⚙ {}", name),
                                    }
                                } else {
                                    let hint = event
                                        .input
                                        .as_deref()
                                        .and_then(tool_input_hint)
                                        .unwrap_or_default();
                                    if hint.is_empty() {
                                        format!("⚙ {}", name)
                                    } else {
                                        format!("⚙ {}  {}", name, hint)
                                    }
                                };
                                output.push(OutputLine::ToolCall(label));
                                scroll_offset = 0;
                            }
                        }
                        AgentEventType::ToolResult => {
                            if let Some(result) = event.text {
                                // Always show a result preview; verbose shows up to 300 chars
                                let limit = if verbose { 300 } else { 120 };
                                let preview = truncate_with_ellipsis(&result, limit);
                                output.push(OutputLine::ToolResult(format!("  → {}", preview)));
                                scroll_offset = 0;
                            }
                        }
                        AgentEventType::Error => {
                            if let Some(msg) = event.text {
                                output.push(OutputLine::ErrorLine(format!("✗ {}", msg)));
                            }
                            waiting_for_agent = false;
                            output.push(OutputLine::Separator);
                            scroll_offset = 0;
                        }
                        AgentEventType::System => {
                            if let Some(msg) = event.text {
                                output.push(OutputLine::SystemInfo(format!("  ⟳ {}", msg)));
                                scroll_offset = 0;
                            }
                        }
                        AgentEventType::Done => {
                            waiting_for_agent = false;
                            output.push(OutputLine::Separator);
                            scroll_offset = 0;
                        }
                    }
                }
            }

            // Timeout while waiting for first response
            _ = tokio::time::sleep(RESPONSE_TIMEOUT), if waiting_for_agent => {
                output.push(OutputLine::ErrorLine(
                    "No response from daemon. Is `bubbaloop up` running?".to_string(),
                ));
                waiting_for_agent = false;
                output.push(OutputLine::Separator);
                scroll_offset = 0;
            }
        }
    }

    Ok(())
}

/// Truncate a string to `max_len` characters, appending an ellipsis if needed.
fn truncate_with_ellipsis(s: &str, max_len: usize) -> String {
    if s.len() > max_len {
        format!("{}…", &s[..max_len])
    } else {
        s.to_string()
    }
}

/// Extract a short human-readable hint from a tool's JSON input.
///
/// Priority order: `path`, `command`, `topic`, `key`, `subject`, `name`, `query` —
/// whichever appears first in the JSON object. Falls back to the first string value.
fn tool_input_hint(input_json: &str) -> Option<String> {
    let v: serde_json::Value = serde_json::from_str(input_json).ok()?;
    let obj = v.as_object()?;

    // Keys that most meaningfully describe "what the tool is acting on"
    const PRIORITY: &[&str] = &[
        "path",
        "command",
        "topic",
        "key",
        "subject",
        "name",
        "query",
        "mission_id",
        "node_name",
        "agent_id",
    ];
    for key in PRIORITY {
        if let Some(val) = obj.get(*key).and_then(|v| v.as_str()) {
            return Some(truncate_with_ellipsis(val, 60));
        }
    }
    // Fallback: first string value in the object
    obj.values()
        .find_map(|v| v.as_str())
        .map(|s| truncate_with_ellipsis(s, 60))
}

/// Create a single styled `ListItem`.
fn styled_item(text: String, style: Style) -> ListItem<'static> {
    ListItem::new(Line::from(Span::styled(text, style)))
}

/// Convert an `OutputLine` into one or more ratatui `ListItem`s with colour styling.
fn render_output_line(line: &OutputLine) -> Vec<ListItem<'static>> {
    match line {
        OutputLine::UserMessage(text) => {
            let style = Style::default()
                .fg(Color::White)
                .add_modifier(Modifier::BOLD);
            vec![styled_item(format!("You  {}", text), style)]
        }
        OutputLine::AgentHeader(id) => {
            let style = Style::default()
                .fg(Color::Cyan)
                .add_modifier(Modifier::BOLD);
            vec![styled_item(format!("╾── {} ──╼", id), style)]
        }
        OutputLine::AgentDelta(text) => {
            let style = Style::default().fg(Color::Green);
            text.split('\n')
                .map(|chunk| styled_item(chunk.to_string(), style))
                .collect()
        }
        OutputLine::ToolCall(label) => {
            let style = Style::default()
                .fg(Color::Yellow)
                .add_modifier(Modifier::ITALIC);
            vec![styled_item(label.clone(), style)]
        }
        OutputLine::ToolResult(text) => {
            vec![styled_item(text.clone(), Style::default().fg(Color::Gray))]
        }
        OutputLine::ErrorLine(text) => {
            let style = Style::default().fg(Color::Red).add_modifier(Modifier::BOLD);
            vec![styled_item(text.clone(), style)]
        }
        OutputLine::Separator => {
            vec![styled_item(
                "─".repeat(60),
                Style::default().fg(Color::DarkGray),
            )]
        }
        OutputLine::Info(text) => {
            let style = Style::default()
                .fg(Color::Blue)
                .add_modifier(Modifier::ITALIC);
            vec![styled_item(text.clone(), style)]
        }
        OutputLine::SystemInfo(text) => {
            // Dim blue-gray — visible but not distracting
            let style = Style::default().fg(Color::Cyan).add_modifier(Modifier::DIM);
            vec![styled_item(text.clone(), style)]
        }
    }
}

/// Query all agent manifests and print a table.
pub async fn run_agent_list(
    session: Arc<Session>,
    scope: &str,
    machine_id: &str,
    all_machines: bool,
) -> Result<(), Box<dyn std::error::Error>> {
    let pattern = if all_machines {
        gateway::manifest_wildcard_all(scope)
    } else {
        gateway::manifest_wildcard(scope, machine_id)
    };

    let replies = session
        .get(&pattern)
        .timeout(Duration::from_secs(3))
        .await
        .map_err(|e| format!("Manifest query failed: {}", e))?;

    let mut manifests = Vec::new();
    while let Ok(reply) = replies.recv_async().await {
        if let Ok(sample) = reply.into_result() {
            let bytes = sample.payload().to_bytes();
            if let Ok(manifest) = serde_json::from_slice::<AgentManifest>(&bytes) {
                manifests.push(manifest);
            }
        }
    }

    if manifests.is_empty() {
        if all_machines {
            println!("No agents found on any machine.");
        } else {
            println!("No agents found. Is `bubbaloop up` running?");
        }
        return Ok(());
    }

    if all_machines {
        println!(
            "{:<20} {:<20} {:<25} {:<10} {:<30} CAPABILITIES",
            "ID", "MACHINE", "NAME", "DEFAULT", "MODEL",
        );
        println!("{}", "-".repeat(115));
        for m in &manifests {
            println!(
                "{:<20} {:<20} {:<25} {:<10} {:<30} {}",
                m.agent_id,
                m.machine_id,
                m.name,
                if m.is_default { "yes" } else { "" },
                m.model,
                m.capabilities.join(", "),
            );
        }
    } else {
        println!(
            "{:<20} {:<25} {:<10} {:<30} CAPABILITIES",
            "ID", "NAME", "DEFAULT", "MODEL",
        );
        println!("{}", "-".repeat(95));
        for m in &manifests {
            println!(
                "{:<20} {:<25} {:<10} {:<30} {}",
                m.agent_id,
                m.name,
                if m.is_default { "yes" } else { "" },
                m.model,
                m.capabilities.join(", "),
            );
        }
    }

    Ok(())
}
