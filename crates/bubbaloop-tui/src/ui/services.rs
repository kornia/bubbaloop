use ratatui::{
    layout::{Constraint, Direction, Layout},
    style::{Modifier, Style},
    text::{Line, Span},
    widgets::{Block, BorderType, Borders, Paragraph},
    Frame,
};

use super::components::colors;
use crate::app::{App, ServiceStatus};

pub fn render(f: &mut Frame, app: &App) {
    let area = f.area();

    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(3), // Header
            Constraint::Min(5),    // Service list
            Constraint::Length(2), // Hints
            Constraint::Length(1), // Exit warning
        ])
        .split(area);

    // Header
    let header_block = Block::default()
        .title(Span::styled(
            " Services ",
            Style::default()
                .fg(colors::PRIMARY)
                .add_modifier(Modifier::BOLD),
        ))
        .title_alignment(ratatui::layout::Alignment::Left)
        .borders(Borders::ALL)
        .border_type(BorderType::Rounded)
        .border_style(Style::default().fg(colors::PRIMARY));

    let header_text = Line::from(vec![
        Span::styled("esc/q", Style::default().fg(colors::PRIMARY)),
        Span::styled(" back", Style::default().fg(colors::DIMMED)),
    ]);

    let header = Paragraph::new(header_text)
        .block(header_block)
        .alignment(ratatui::layout::Alignment::Right);
    f.render_widget(header, chunks[0]);

    // Service list
    let list_block = Block::default()
        .borders(Borders::ALL)
        .border_type(BorderType::Plain)
        .border_style(Style::default().fg(colors::BORDER));

    let list_inner = list_block.inner(chunks[1]);
    f.render_widget(list_block, chunks[1]);

    let lines: Vec<Line> = app
        .services
        .iter()
        .enumerate()
        .map(|(i, service)| {
            let selected = i == app.service_index;
            let indicator = if selected { "❯ " } else { "  " };

            let (status_symbol, status_color) = match service.status {
                ServiceStatus::Running => ("●", colors::SUCCESS),
                ServiceStatus::Stopped => ("○", colors::DIMMED),
                ServiceStatus::Failed => ("✗", colors::ERROR),
                ServiceStatus::Unknown => ("?", colors::DIMMED),
            };

            let status_text = match service.status {
                ServiceStatus::Running => "active",
                ServiceStatus::Stopped => "inactive",
                ServiceStatus::Failed => "failed",
                ServiceStatus::Unknown => "unknown",
            };

            let name_style = if selected {
                Style::default().fg(colors::TEXT)
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
                Span::styled(status_symbol, Style::default().fg(status_color)),
                Span::styled(format!(" {} ", service.display_name), name_style),
                Span::styled(
                    format!("- {}", status_text),
                    Style::default().fg(colors::DIMMED),
                ),
            ])
        })
        .collect();

    let paragraph = Paragraph::new(lines);
    f.render_widget(paragraph, list_inner);

    // Hints
    let hints = Line::from(vec![
        Span::styled("s", Style::default().fg(colors::PRIMARY)),
        Span::styled(" start • ", Style::default().fg(colors::DIMMED)),
        Span::styled("x", Style::default().fg(colors::PRIMARY)),
        Span::styled(" stop • ", Style::default().fg(colors::DIMMED)),
        Span::styled("r", Style::default().fg(colors::PRIMARY)),
        Span::styled(" restart • ", Style::default().fg(colors::DIMMED)),
        Span::styled("↑↓", Style::default().fg(colors::PRIMARY)),
        Span::styled(" select • ", Style::default().fg(colors::DIMMED)),
        Span::styled("esc/q", Style::default().fg(colors::PRIMARY)),
        Span::styled(" back", Style::default().fg(colors::DIMMED)),
    ]);
    f.render_widget(Paragraph::new(hints), chunks[2]);

    // Exit warning
    if app.exit_warning {
        let warning = Line::from(Span::styled(
            "Press Ctrl+C again to exit",
            Style::default().fg(colors::ERROR),
        ));
        f.render_widget(Paragraph::new(warning), chunks[3]);
    }
}
