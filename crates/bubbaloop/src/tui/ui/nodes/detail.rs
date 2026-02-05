use ratatui::{
    layout::{Constraint, Direction, Layout},
    style::{Modifier, Style},
    text::{Line, Span},
    widgets::{Block, BorderType, Borders, Paragraph},
    Frame,
};

use crate::tui::app::{App, MessageType, NodeInfo};
use crate::tui::ui::components::{colors, flower_spinner};

pub fn render_detail(f: &mut Frame, app: &App, node_name: &str) {
    let area = f.area();

    let node = app.nodes.iter().find(|n| n.name == node_name);
    let node = match node {
        Some(n) => n,
        None => {
            let text = Paragraph::new("Node not found");
            f.render_widget(text, area);
            return;
        }
    };

    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(3),
            Constraint::Length(2),
            Constraint::Min(10),
            Constraint::Length(1),
        ])
        .split(area);

    let header_block = Block::default()
        .title(Span::styled(
            format!(" {} ", node.name),
            Style::default()
                .fg(colors::PRIMARY)
                .add_modifier(Modifier::BOLD),
        ))
        .borders(Borders::ALL)
        .border_type(BorderType::Rounded)
        .border_style(Style::default().fg(colors::PRIMARY));

    let header_inner = header_block.inner(chunks[0]);
    f.render_widget(header_block, chunks[0]);

    let nav_hints = Line::from(vec![
        Span::styled(
            format!("{}/{}", app.node_index + 1, app.nodes.len()),
            Style::default().fg(colors::DIMMED),
        ),
        Span::styled(" ", Style::default()),
        Span::styled("[tab]", Style::default().fg(colors::PRIMARY)),
        Span::styled(" next ", Style::default().fg(colors::DIMMED)),
        Span::styled("[shift+tab]", Style::default().fg(colors::PRIMARY)),
        Span::styled(" prev ", Style::default().fg(colors::DIMMED)),
        Span::styled("[esc]", Style::default().fg(colors::PRIMARY)),
        Span::styled(" back", Style::default().fg(colors::DIMMED)),
    ]);
    f.render_widget(
        Paragraph::new(nav_hints).alignment(ratatui::layout::Alignment::Right),
        header_inner,
    );

    if let Some((text, msg_type)) = app.messages.last() {
        let color = match msg_type {
            MessageType::Info => colors::DIMMED,
            MessageType::Success => colors::SUCCESS,
            MessageType::Warning => colors::WARNING,
            MessageType::Error => colors::ERROR,
        };
        let line = Line::from(Span::styled(text.clone(), Style::default().fg(color)));
        f.render_widget(Paragraph::new(line), chunks[1]);
    }

    let left_width = if chunks[2].width < 80 {
        Constraint::Length(chunks[2].width.clamp(20, 30))
    } else {
        Constraint::Percentage(25)
    };
    let content_chunks = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([left_width, Constraint::Min(20)])
        .split(chunks[2]);

    render_info_panel(f, app, node, content_chunks[0]);
    render_status_panel(f, app, node, content_chunks[1]);

    if app.exit_warning {
        let warning = Line::from(Span::styled(
            "Press Ctrl+C again to exit",
            Style::default().fg(colors::ERROR),
        ));
        f.render_widget(Paragraph::new(warning), chunks[3]);
    }
}

fn render_info_panel(f: &mut Frame, app: &App, node: &NodeInfo, area: ratatui::layout::Rect) {
    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([Constraint::Min(8), Constraint::Length(8)])
        .split(area);

    let info_block = Block::default()
        .borders(Borders::ALL)
        .border_type(BorderType::Plain)
        .border_style(Style::default().fg(colors::BORDER));

    let info_inner = info_block.inner(chunks[0]);
    f.render_widget(info_block, chunks[0]);

    let type_color = if node.node_type == "rust" {
        colors::RUST_TYPE
    } else {
        colors::PYTHON_TYPE
    };

    let (status_label, status_color) = match node.status.as_str() {
        "running" => ("RUNNING", colors::SUCCESS),
        "stopped" => ("STOPPED", colors::DIMMED),
        "failed" => ("FAILED", colors::ERROR),
        "not-installed" => ("NOT INSTALLED", colors::DIMMED),
        "building" => ("BUILDING", colors::WARNING),
        _ => ("UNKNOWN", colors::DIMMED),
    };

    let is_busy = app.is_node_busy(node);

    let mut info_lines = vec![
        Line::from(vec![
            Span::styled("Version:     ", Style::default().fg(colors::DIMMED)),
            Span::styled(&node.version, Style::default().fg(colors::SUCCESS)),
        ]),
        Line::from(vec![
            Span::styled("Type:        ", Style::default().fg(colors::DIMMED)),
            Span::styled(&node.node_type, Style::default().fg(type_color)),
        ]),
        Line::from(vec![
            Span::styled("Description: ", Style::default().fg(colors::DIMMED)),
            Span::styled(&node.description, Style::default().fg(colors::TEXT)),
        ]),
    ];

    // Show base node if this is an instance
    if !node.base_node.is_empty() {
        info_lines.push(Line::from(vec![
            Span::styled("Base node:   ", Style::default().fg(colors::DIMMED)),
            Span::styled(&node.base_node, Style::default().fg(colors::PRIMARY)),
        ]));
    }

    info_lines.extend(vec![
        Line::from(Span::styled("Path:", Style::default().fg(colors::DIMMED))),
        Line::from(Span::styled(
            format!("  {}", node.path),
            Style::default().fg(colors::TEXT),
        )),
        Line::from(vec![
            Span::styled("Service:     ", Style::default().fg(colors::DIMMED)),
            Span::styled(
                format!("bubbaloop-{}.service", node.name),
                Style::default().fg(colors::TEXT),
            ),
        ]),
        Line::from(""),
        Line::from(vec![
            Span::styled("Status:      ", Style::default().fg(colors::DIMMED)),
            Span::styled(
                status_label,
                Style::default()
                    .fg(status_color)
                    .add_modifier(Modifier::BOLD),
            ),
        ]),
        Line::from(vec![
            Span::styled("Built:       ", Style::default().fg(colors::DIMMED)),
            if is_busy {
                flower_spinner(app.spinner_frame)
            } else if node.is_built {
                Span::styled(
                    "YES",
                    Style::default()
                        .fg(colors::SUCCESS)
                        .add_modifier(Modifier::BOLD),
                )
            } else {
                Span::styled(
                    "NO",
                    Style::default()
                        .fg(colors::ERROR)
                        .add_modifier(Modifier::BOLD),
                )
            },
        ]),
    ]);

    f.render_widget(Paragraph::new(info_lines), info_inner);

    let actions_block = Block::default()
        .title(Span::styled(
            " Actions ",
            Style::default()
                .fg(colors::PRIMARY)
                .add_modifier(Modifier::BOLD),
        ))
        .borders(Borders::ALL)
        .border_type(BorderType::Plain)
        .border_style(Style::default().fg(colors::BORDER));

    let actions_inner = actions_block.inner(chunks[1]);
    f.render_widget(actions_block, chunks[1]);

    let mut action_lines = Vec::new();

    if app.daemon_available {
        if node.status == "not-installed" {
            action_lines.push(Line::from(vec![
                Span::styled("[e]", Style::default().fg(colors::PRIMARY)),
                Span::styled("nable service", Style::default().fg(colors::DIMMED)),
            ]));
        } else if node.status != "unknown" {
            action_lines.push(Line::from(vec![
                Span::styled("[d]", Style::default().fg(colors::PRIMARY)),
                Span::styled("isable service", Style::default().fg(colors::DIMMED)),
            ]));
        }
    }

    if node.status != "not-installed" && node.status != "unknown" {
        if !is_busy {
            if node.status == "running" {
                action_lines.push(Line::from(vec![
                    Span::styled("[s]", Style::default().fg(colors::PRIMARY)),
                    Span::styled("top", Style::default().fg(colors::DIMMED)),
                ]));
            } else if node.is_built {
                action_lines.push(Line::from(vec![
                    Span::styled("[s]", Style::default().fg(colors::PRIMARY)),
                    Span::styled("tart", Style::default().fg(colors::DIMMED)),
                ]));
            } else {
                action_lines.push(Line::from(vec![
                    Span::styled("[s]tart ", Style::default().fg(colors::DIMMED)),
                    Span::styled("(build first)", Style::default().fg(colors::ERROR)),
                ]));
            }
        }

        action_lines.push(Line::from(vec![
            Span::styled("[l]", Style::default().fg(colors::PRIMARY)),
            Span::styled("ogs", Style::default().fg(colors::DIMMED)),
        ]));
    }

    f.render_widget(Paragraph::new(action_lines), actions_inner);
}

fn render_status_panel(f: &mut Frame, app: &App, _node: &NodeInfo, area: ratatui::layout::Rect) {
    // Full-height systemd status panel (build output removed — use [1] Installed tab for builds)
    let systemd_block = Block::default()
        .title(Span::styled(
            " Systemd Status ",
            Style::default().fg(colors::PRIMARY),
        ))
        .borders(Borders::ALL)
        .border_type(BorderType::Plain)
        .border_style(Style::default().fg(colors::BORDER));

    let systemd_inner = systemd_block.inner(area);
    f.render_widget(systemd_block, area);

    let systemd_content: Vec<Line> = if !app.service_status_text.is_empty() {
        app.service_status_text
            .iter()
            .map(|line| {
                Line::from(Span::styled(
                    line.clone(),
                    Style::default().fg(colors::TEXT),
                ))
            })
            .collect()
    } else {
        vec![Line::from(Span::styled(
            "Loading status...",
            Style::default().fg(colors::DIMMED),
        ))]
    };

    f.render_widget(Paragraph::new(systemd_content), systemd_inner);
}
