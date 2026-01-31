use ratatui::{
    layout::{Constraint, Direction, Layout, Rect},
    style::{Modifier, Style},
    text::{Line, Span},
    widgets::{Block, BorderType, Borders, Paragraph},
    Frame,
};

use super::components::{colors, footer_line, header_line};
use crate::tui::app::{App, InputMode, MessageType};

const VERSION: &str = "0.1.0";

pub fn render(f: &mut Frame, app: &App) {
    let area = f.area();

    // Fixed chrome: header(1) + footer(1) + messages(3) + input(3) + hints(1) = 9
    let main_height = area.height.saturating_sub(9).max(3);

    // Create main layout with dynamic box height
    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(1),           // Header
            Constraint::Length(main_height), // Main content (dynamic)
            Constraint::Length(1),           // Footer border
            Constraint::Min(0),              // Spacer (absorbs extra space)
            Constraint::Length(3),           // Messages (if any)
            Constraint::Length(3),           // Input area
            Constraint::Length(1),           // Exit warning / hints
        ])
        .split(area);

    // Render header
    let header = header_line("Bubbaloop", VERSION, area.width as usize);
    f.render_widget(Paragraph::new(header), chunks[0]);

    // Render main content (two columns)
    render_main_content(f, app, chunks[1]);

    // Render footer border
    let footer = footer_line(area.width as usize);
    f.render_widget(Paragraph::new(footer), chunks[2]);

    // Render messages
    render_messages(f, app, chunks[4]);

    // Render command suggestions if typing
    if matches!(app.input_mode, InputMode::Command) {
        render_suggestions(f, app, chunks[4]);
    }

    // Render input area
    render_input(f, app, chunks[5]);

    // Render hints/warnings
    render_hints(f, app, chunks[6]);
}

fn render_main_content(f: &mut Frame, app: &App, area: Rect) {
    use ratatui::style::Color;

    let border_style = Style::default().fg(colors::PRIMARY);

    // Build left and right border strings for the entire height
    let left_border: String = "│\n".repeat(area.height as usize);
    let right_border: String = "│\n".repeat(area.height as usize);

    // Render left border
    f.render_widget(
        Paragraph::new(left_border.trim_end()).style(border_style),
        Rect::new(area.x, area.y, 1, area.height),
    );

    // Render right border
    f.render_widget(
        Paragraph::new(right_border.trim_end()).style(border_style),
        Rect::new(area.x + area.width - 1, area.y, 1, area.height),
    );

    // Inner area (excluding borders)
    let inner = Rect::new(
        area.x + 1,
        area.y,
        area.width.saturating_sub(2),
        area.height,
    );

    // Split into two columns
    let columns = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([Constraint::Percentage(50), Constraint::Percentage(50)])
        .split(inner);

    // Left column - Welcome + Robot
    let username = whoami::username();

    // Robot mascot
    let eyes_color = if app.robot_eyes_on {
        Color::Rgb(107, 181, 255) // #6BB5FF
    } else {
        Color::Rgb(42, 90, 138) // #2A5A8A
    };

    let lines = vec![
        Line::from(""),
        Line::from(Span::styled(
            format!("Welcome {}!", username),
            Style::default()
                .fg(colors::TEXT)
                .add_modifier(Modifier::BOLD),
        )),
        Line::from(""),
        Line::from(Span::styled(" ▄▀▀▀▄", Style::default().fg(colors::PRIMARY))),
        Line::from(vec![
            Span::styled("█", Style::default().fg(colors::PRIMARY)),
            Span::styled(" ▓ ▓ ", Style::default().fg(eyes_color)),
            Span::styled("█", Style::default().fg(colors::PRIMARY)),
        ]),
        Line::from(Span::styled(" ▀▄█▄▀", Style::default().fg(colors::PRIMARY))),
        Line::from(""),
        Line::from(Span::styled(
            "Multi-agent orchestration",
            Style::default().fg(colors::DIMMED),
        )),
        Line::from(Span::styled(
            "for Physical AI",
            Style::default().fg(colors::DIMMED),
        )),
    ];

    let paragraph = Paragraph::new(lines).alignment(ratatui::layout::Alignment::Center);
    f.render_widget(paragraph, columns[0]);

    // Right column - Commands
    let commands = vec![
        Line::from(""),
        Line::from(Span::styled(
            "Node Management",
            Style::default()
                .fg(colors::WARNING)
                .add_modifier(Modifier::BOLD),
        )),
        Line::from(vec![
            Span::styled("/nodes", Style::default().fg(colors::PRIMARY)),
            Span::styled(" manage local nodes", Style::default().fg(colors::TEXT)),
        ]),
        Line::from(vec![
            Span::styled("/services", Style::default().fg(colors::PRIMARY)),
            Span::styled(" service status", Style::default().fg(colors::TEXT)),
        ]),
        Line::from(vec![
            Span::styled("/quit", Style::default().fg(colors::PRIMARY)),
            Span::styled(" exit", Style::default().fg(colors::TEXT)),
        ]),
    ];

    let paragraph = Paragraph::new(commands);
    f.render_widget(paragraph, columns[1]);
}

fn render_messages(f: &mut Frame, app: &App, area: Rect) {
    if app.messages.is_empty() {
        return;
    }

    let lines: Vec<Line> = app
        .messages
        .iter()
        .map(|(text, msg_type)| {
            let color = match msg_type {
                MessageType::Info => colors::DIMMED,
                MessageType::Success => colors::SUCCESS,
                MessageType::Warning => colors::WARNING,
                MessageType::Error => colors::ERROR,
            };
            Line::from(Span::styled(text.clone(), Style::default().fg(color)))
        })
        .collect();

    let paragraph = Paragraph::new(lines);
    f.render_widget(paragraph, area);
}

fn render_suggestions(f: &mut Frame, app: &App, area: Rect) {
    let filtered = app.filtered_commands();
    if filtered.is_empty() {
        return;
    }

    let lines: Vec<Line> = filtered
        .iter()
        .enumerate()
        .map(|(i, (cmd, desc))| {
            let selected = i == app.command_index;
            let indicator = if selected { "❯ " } else { "  " };
            let cmd_style = if selected {
                Style::default()
                    .fg(colors::TEXT)
                    .add_modifier(Modifier::BOLD)
            } else {
                Style::default().fg(colors::DIMMED)
            };
            Line::from(vec![
                Span::styled(
                    indicator,
                    Style::default().fg(if selected {
                        colors::PRIMARY
                    } else {
                        colors::DIMMED
                    }),
                ),
                Span::styled((*cmd).to_string(), cmd_style),
                Span::styled(format!(" - {}", desc), Style::default().fg(colors::DIMMED)),
            ])
        })
        .collect();

    let paragraph = Paragraph::new(lines);
    f.render_widget(paragraph, area);
}

fn render_input(f: &mut Frame, app: &App, area: Rect) {
    let is_active = matches!(app.input_mode, InputMode::Command);
    let border_color = if is_active {
        colors::PRIMARY
    } else {
        colors::DIMMED
    };

    let block = Block::default()
        .borders(Borders::ALL)
        .border_type(BorderType::Rounded)
        .border_style(Style::default().fg(border_color));

    let inner = block.inner(area);
    f.render_widget(block, area);

    let prompt = Span::styled(
        "❯ ",
        Style::default()
            .fg(colors::PRIMARY)
            .add_modifier(Modifier::BOLD),
    );

    let content = if app.input.is_empty() && !is_active {
        vec![
            prompt,
            Span::styled(
                "Type \"/\" for commands",
                Style::default().fg(colors::DIMMED),
            ),
        ]
    } else {
        vec![
            prompt,
            Span::styled(app.input.clone(), Style::default().fg(colors::TEXT)),
        ]
    };

    let paragraph = Paragraph::new(Line::from(content));
    f.render_widget(paragraph, inner);

    // Show cursor when typing
    if is_active {
        f.set_cursor_position((inner.x + 2 + app.input_cursor as u16, inner.y));
    }
}

fn render_hints(f: &mut Frame, app: &App, area: Rect) {
    if app.exit_warning {
        let warning = Line::from(Span::styled(
            "Press Ctrl+C again to exit",
            Style::default().fg(colors::ERROR),
        ));
        f.render_widget(Paragraph::new(warning), area);
    } else {
        let hints = Line::from(vec![
            Span::styled("esc", Style::default().fg(colors::PRIMARY)),
            Span::styled(" clear • ", Style::default().fg(colors::DIMMED)),
            Span::styled("↑↓", Style::default().fg(colors::PRIMARY)),
            Span::styled(" history • ", Style::default().fg(colors::DIMMED)),
            Span::styled("/quit", Style::default().fg(colors::PRIMARY)),
            Span::styled(" exit", Style::default().fg(colors::DIMMED)),
        ]);
        f.render_widget(Paragraph::new(hints), area);
    }
}
