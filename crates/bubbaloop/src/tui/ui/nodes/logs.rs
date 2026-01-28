use ratatui::{
    layout::{Constraint, Direction, Layout},
    style::{Modifier, Style},
    text::{Line, Span},
    widgets::{Block, BorderType, Borders, Paragraph},
    Frame,
};

use crate::tui::app::App;
use crate::tui::ui::components::spinner::logs_verb;
use crate::tui::ui::components::{colors, flower_spinner};

pub fn render_logs(f: &mut Frame, app: &App, node_name: &str) {
    let area = f.area();

    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(3),
            Constraint::Min(10),
            Constraint::Length(1),
        ])
        .split(area);

    let header_block = Block::default()
        .borders(Borders::ALL)
        .border_type(BorderType::Rounded)
        .border_style(Style::default().fg(colors::SUCCESS));

    let header_inner = header_block.inner(chunks[0]);
    f.render_widget(header_block, chunks[0]);

    let title_line = Line::from(vec![
        flower_spinner(app.spinner_frame),
        Span::styled(
            format!(" {} ", logs_verb(app.spinner_frame)),
            Style::default().fg(colors::WARNING),
        ),
        Span::styled(
            node_name,
            Style::default()
                .fg(colors::SUCCESS)
                .add_modifier(Modifier::BOLD),
        ),
        Span::styled(
            format!(" ({}/{})", app.node_index + 1, app.nodes.len()),
            Style::default().fg(colors::DIMMED),
        ),
    ]);

    let nav_line = Line::from(vec![
        Span::styled("[tab]", Style::default().fg(colors::PRIMARY)),
        Span::styled(" next node  ", Style::default().fg(colors::DIMMED)),
        Span::styled("[esc]", Style::default().fg(colors::PRIMARY)),
        Span::styled(" exit", Style::default().fg(colors::DIMMED)),
    ]);

    let header_layout = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([Constraint::Percentage(50), Constraint::Percentage(50)])
        .split(header_inner);

    f.render_widget(Paragraph::new(title_line), header_layout[0]);
    f.render_widget(
        Paragraph::new(nav_line).alignment(ratatui::layout::Alignment::Right),
        header_layout[1],
    );

    let logs_block = Block::default()
        .borders(Borders::ALL)
        .border_type(BorderType::Plain)
        .border_style(Style::default().fg(colors::BORDER));

    let logs_inner = logs_block.inner(chunks[1]);
    f.render_widget(logs_block, chunks[1]);

    let max_lines = logs_inner.height as usize;
    let log_lines: Vec<Line> = if app.logs.is_empty() {
        vec![Line::from(Span::styled(
            "Waiting for logs...",
            Style::default().fg(colors::DIMMED),
        ))]
    } else {
        app.logs
            .iter()
            .rev()
            .take(max_lines)
            .rev()
            .map(|line| {
                let color = if line.starts_with("[err]") {
                    colors::ERROR
                } else if line.starts_with("===") {
                    colors::PRIMARY
                } else {
                    colors::TEXT
                };
                Line::from(Span::styled(line.clone(), Style::default().fg(color)))
            })
            .collect()
    };

    f.render_widget(Paragraph::new(log_lines), logs_inner);

    if app.exit_warning {
        let warning = Line::from(Span::styled(
            "Press Ctrl+C again to exit",
            Style::default().fg(colors::ERROR),
        ));
        f.render_widget(Paragraph::new(warning), chunks[2]);
    }
}
