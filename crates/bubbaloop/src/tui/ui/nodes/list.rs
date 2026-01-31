use ratatui::{
    layout::{Constraint, Direction, Layout},
    style::{Modifier, Style},
    text::{Line, Span},
    widgets::{Block, BorderType, Borders, Paragraph, Row, Table},
    Frame,
};

use crate::tui::app::{App, InputMode, NodesTab, View};
use crate::tui::ui::components::{colors, flower_spinner};

fn truncate_path(path: &str, max_chars: usize) -> String {
    let char_count = path.chars().count();
    if char_count > max_chars {
        // Reserve 3 chars for "..." prefix
        let keep = max_chars.saturating_sub(3);
        let skip = char_count.saturating_sub(keep);
        let suffix: String = path.chars().skip(skip).collect();
        format!("...{}", suffix)
    } else {
        path.to_string()
    }
}

pub fn render_list(f: &mut Frame, app: &App) {
    // Handle create node form
    if app.input_mode == InputMode::CreateNode {
        render_create_node_form(f, app);
        return;
    }

    if app.input_mode == InputMode::EditSource {
        render_edit_source_form(f, app);
        return;
    }

    let area = f.area();

    let current_tab = match &app.view {
        View::Nodes(tab) => tab.clone(),
        _ => NodesTab::Installed,
    };

    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(3),
            Constraint::Min(5),
            Constraint::Length(2),
            Constraint::Length(1),
            Constraint::Length(1),
        ])
        .split(area);

    render_header(f, chunks[0], &current_tab);

    match current_tab {
        NodesTab::Installed => render_installed_tab(f, app, chunks[1]),
        NodesTab::Discover => render_discover_tab(f, app, chunks[1]),
        NodesTab::Marketplace => render_marketplace_tab(f, app, chunks[1]),
    }

    render_hints(f, app, chunks[2], &current_tab);
    render_messages(f, app, chunks[3]);

    if app.exit_warning {
        let warning = Line::from(Span::styled(
            "Press Ctrl+C again to exit",
            Style::default().fg(colors::ERROR),
        ));
        f.render_widget(Paragraph::new(warning), chunks[4]);
    }
}

fn render_header(f: &mut Frame, area: ratatui::layout::Rect, current_tab: &NodesTab) {
    let tabs = [
        ("1", "Installed", NodesTab::Installed),
        ("2", "Discover", NodesTab::Discover),
        ("3", "Marketplace", NodesTab::Marketplace),
    ];

    let tab_spans: Vec<Span> = tabs
        .iter()
        .flat_map(|(num, name, tab)| {
            let is_active = tab == current_tab;
            let style = if is_active {
                Style::default()
                    .fg(colors::PRIMARY)
                    .add_modifier(Modifier::BOLD)
            } else {
                Style::default().fg(colors::DIMMED)
            };
            vec![
                Span::styled(format!("[{}] ", num), Style::default().fg(colors::PRIMARY)),
                Span::styled(format!("{} ", name), style),
                Span::raw("  "),
            ]
        })
        .collect();

    let block = Block::default()
        .title(Span::styled(
            " Nodes ",
            Style::default()
                .fg(colors::PRIMARY)
                .add_modifier(Modifier::BOLD),
        ))
        .borders(Borders::ALL)
        .border_type(BorderType::Rounded)
        .border_style(Style::default().fg(colors::PRIMARY));

    let inner = block.inner(area);
    f.render_widget(block, area);

    let tabs_line = Line::from(tab_spans);
    f.render_widget(Paragraph::new(tabs_line), inner);
}

fn render_installed_tab(f: &mut Frame, app: &App, area: ratatui::layout::Rect) {
    let block = Block::default()
        .borders(Borders::ALL)
        .border_type(BorderType::Plain)
        .border_style(Style::default().fg(colors::BORDER));

    let inner = block.inner(area);
    f.render_widget(block, area);

    if app.nodes.is_empty() {
        let text = Paragraph::new(Line::from(Span::styled(
            "No nodes registered.",
            Style::default().fg(colors::DIMMED),
        )));
        f.render_widget(text, inner);
        return;
    }

    let header = Row::new(vec![
        Span::styled(
            "St",
            Style::default()
                .fg(colors::PRIMARY)
                .add_modifier(Modifier::BOLD),
        ),
        Span::styled(
            "Name",
            Style::default()
                .fg(colors::PRIMARY)
                .add_modifier(Modifier::BOLD),
        ),
        Span::styled(
            "Version",
            Style::default()
                .fg(colors::PRIMARY)
                .add_modifier(Modifier::BOLD),
        ),
        Span::styled(
            "Type",
            Style::default()
                .fg(colors::PRIMARY)
                .add_modifier(Modifier::BOLD),
        ),
        Span::styled(
            "Built",
            Style::default()
                .fg(colors::PRIMARY)
                .add_modifier(Modifier::BOLD),
        ),
        Span::styled(
            "Description",
            Style::default()
                .fg(colors::PRIMARY)
                .add_modifier(Modifier::BOLD),
        ),
    ])
    .height(1);

    let rows: Vec<Row> = app
        .nodes
        .iter()
        .enumerate()
        .map(|(i, node)| {
            let selected = i == app.node_index;
            let is_building = node.status == "building";

            let status_span = if is_building {
                flower_spinner(app.spinner_frame)
            } else {
                let (symbol, color) = match node.status.as_str() {
                    "running" => ("●", colors::SUCCESS),
                    "stopped" => ("○", colors::DIMMED),
                    "failed" => ("✗", colors::ERROR),
                    "not-installed" => ("-", colors::DIMMED),
                    _ => ("?", colors::DIMMED),
                };
                Span::styled(symbol, Style::default().fg(color))
            };

            let name_style = if selected {
                Style::default().fg(colors::PRIMARY)
            } else {
                Style::default().fg(colors::TEXT)
            };
            let name_text = if selected {
                format!("❯ {}", node.name)
            } else {
                format!("  {}", node.name)
            };

            let type_color = if node.node_type == "rust" {
                colors::RUST_TYPE
            } else {
                colors::PYTHON_TYPE
            };

            let built_span = if is_building {
                Span::styled("...", Style::default().fg(colors::WARNING))
            } else if node.is_built {
                Span::styled("yes", Style::default().fg(colors::SUCCESS))
            } else {
                Span::styled("no", Style::default().fg(colors::ERROR))
            };

            Row::new(vec![
                status_span,
                Span::styled(name_text, name_style),
                Span::styled(node.version.clone(), Style::default().fg(colors::SUCCESS)),
                Span::styled(node.node_type.clone(), Style::default().fg(type_color)),
                built_span,
                Span::styled(
                    node.description.chars().take(40).collect::<String>(),
                    Style::default().fg(colors::DIMMED),
                ),
            ])
        })
        .collect();

    let table = Table::new(
        rows,
        [
            Constraint::Length(3),
            Constraint::Percentage(20),
            Constraint::Length(8),
            Constraint::Length(8),
            Constraint::Length(6),
            Constraint::Percentage(50),
        ],
    )
    .header(header);

    f.render_widget(table, inner);
}

fn render_discover_tab(f: &mut Frame, app: &App, area: ratatui::layout::Rect) {
    let block = Block::default()
        .borders(Borders::ALL)
        .border_type(BorderType::Plain)
        .border_style(Style::default().fg(colors::BORDER));

    let inner = block.inner(area);
    f.render_widget(block, area);

    if app.discoverable_nodes.is_empty() {
        let lines = vec![
            Line::from(Span::styled(
                "No discoverable nodes found.",
                Style::default().fg(colors::DIMMED),
            )),
            Line::from(Span::styled(
                "Add entries in [3] Marketplace tab to discover more nodes.",
                Style::default().fg(colors::DIMMED),
            )),
        ];
        f.render_widget(Paragraph::new(lines), inner);
        return;
    }

    let header = Row::new(vec![
        Span::styled(
            "Name",
            Style::default()
                .fg(colors::PRIMARY)
                .add_modifier(Modifier::BOLD),
        ),
        Span::styled(
            "Version",
            Style::default()
                .fg(colors::PRIMARY)
                .add_modifier(Modifier::BOLD),
        ),
        Span::styled(
            "Type",
            Style::default()
                .fg(colors::PRIMARY)
                .add_modifier(Modifier::BOLD),
        ),
        Span::styled(
            "Source",
            Style::default()
                .fg(colors::PRIMARY)
                .add_modifier(Modifier::BOLD),
        ),
        Span::styled(
            "Path",
            Style::default()
                .fg(colors::PRIMARY)
                .add_modifier(Modifier::BOLD),
        ),
    ])
    .height(1);

    let rows: Vec<Row> = app
        .discoverable_nodes
        .iter()
        .enumerate()
        .map(|(i, node)| {
            let selected = i == app.discover_index;
            let name_style = if selected {
                Style::default().fg(colors::PRIMARY)
            } else {
                Style::default().fg(colors::TEXT)
            };
            let name_text = if selected {
                format!("❯ {}", node.name)
            } else {
                format!("  {}", node.name)
            };

            let type_color = if node.node_type == "rust" {
                colors::RUST_TYPE
            } else {
                colors::PYTHON_TYPE
            };

            let path_display = truncate_path(&node.path, 35);

            Row::new(vec![
                Span::styled(name_text, name_style),
                Span::styled(node.version.clone(), Style::default().fg(colors::SUCCESS)),
                Span::styled(node.node_type.clone(), Style::default().fg(type_color)),
                Span::styled(node.source.clone(), Style::default().fg(colors::DIMMED)),
                Span::styled(path_display, Style::default().fg(colors::DIMMED)),
            ])
        })
        .collect();

    let table = Table::new(
        rows,
        [
            Constraint::Percentage(25),
            Constraint::Length(10),
            Constraint::Length(10),
            Constraint::Percentage(15),
            Constraint::Percentage(40),
        ],
    )
    .header(header);

    f.render_widget(table, inner);
}

fn render_marketplace_tab(f: &mut Frame, app: &App, area: ratatui::layout::Rect) {
    let block = Block::default()
        .borders(Borders::ALL)
        .border_type(BorderType::Plain)
        .border_style(Style::default().fg(colors::BORDER));

    let inner = block.inner(area);
    f.render_widget(block, area);

    if app.sources.is_empty() {
        let text = Paragraph::new(Line::from(Span::styled(
            "No marketplace entries. Press [a] to add one.",
            Style::default().fg(colors::DIMMED),
        )));
        f.render_widget(text, inner);
        return;
    }

    let header = Row::new(vec![
        Span::styled(
            "On",
            Style::default()
                .fg(colors::PRIMARY)
                .add_modifier(Modifier::BOLD),
        ),
        Span::styled(
            "Name",
            Style::default()
                .fg(colors::PRIMARY)
                .add_modifier(Modifier::BOLD),
        ),
        Span::styled(
            "Type",
            Style::default()
                .fg(colors::PRIMARY)
                .add_modifier(Modifier::BOLD),
        ),
        Span::styled(
            "Path",
            Style::default()
                .fg(colors::PRIMARY)
                .add_modifier(Modifier::BOLD),
        ),
    ])
    .height(1);

    let rows: Vec<Row> = app
        .sources
        .iter()
        .enumerate()
        .map(|(i, source)| {
            let selected = i == app.source_index;
            let enabled_symbol = if source.enabled { "●" } else { "○" };
            let enabled_color = if source.enabled {
                colors::SUCCESS
            } else {
                colors::DIMMED
            };

            let name_style = if selected {
                Style::default().fg(colors::PRIMARY)
            } else {
                Style::default().fg(colors::TEXT)
            };
            let name_text = if selected {
                format!("❯ {}", source.name)
            } else {
                format!("  {}", source.name)
            };

            let type_color = match source.source_type.as_str() {
                "git" => colors::ERROR,
                "builtin" => colors::PRIMARY,
                _ => colors::SUCCESS,
            };

            let path_display = truncate_path(&source.path, 50);

            Row::new(vec![
                Span::styled(enabled_symbol, Style::default().fg(enabled_color)),
                Span::styled(name_text, name_style),
                Span::styled(source.source_type.clone(), Style::default().fg(type_color)),
                Span::styled(path_display, Style::default().fg(colors::DIMMED)),
            ])
        })
        .collect();

    let table = Table::new(
        rows,
        [
            Constraint::Length(3),
            Constraint::Percentage(20),
            Constraint::Length(10),
            Constraint::Percentage(67),
        ],
    )
    .header(header);

    f.render_widget(table, inner);
}

fn render_hints(f: &mut Frame, app: &App, area: ratatui::layout::Rect, current_tab: &NodesTab) {
    let hints = match current_tab {
        NodesTab::Installed => {
            if app.nodes.is_empty() {
                Line::from(vec![
                    Span::styled("tab", Style::default().fg(colors::PRIMARY)),
                    Span::styled(" switch tabs • ", Style::default().fg(colors::DIMMED)),
                    Span::styled("esc/q", Style::default().fg(colors::PRIMARY)),
                    Span::styled(" back", Style::default().fg(colors::DIMMED)),
                ])
            } else if app.confirm_uninstall {
                let suffix = app
                    .nodes
                    .get(app.node_index)
                    .map(|n| format!(" {}", n.name))
                    .unwrap_or_default();
                Line::from(Span::styled(
                    format!("Press [u] again to UNINSTALL{}", suffix),
                    Style::default().fg(colors::ERROR),
                ))
            } else {
                Line::from(vec![
                    Span::styled("s", Style::default().fg(colors::PRIMARY)),
                    Span::styled(" start/stop • ", Style::default().fg(colors::DIMMED)),
                    Span::styled("u", Style::default().fg(colors::PRIMARY)),
                    Span::styled(" uninstall • ", Style::default().fg(colors::DIMMED)),
                    Span::styled("enter", Style::default().fg(colors::PRIMARY)),
                    Span::styled(" details • ", Style::default().fg(colors::DIMMED)),
                    Span::styled("↑↓", Style::default().fg(colors::PRIMARY)),
                    Span::styled(" select", Style::default().fg(colors::DIMMED)),
                ])
            }
        }
        NodesTab::Discover => Line::from(vec![
            Span::styled("[enter/a]", Style::default().fg(colors::PRIMARY)),
            Span::styled(" add selected • ", Style::default().fg(colors::DIMMED)),
            Span::styled("[n]", Style::default().fg(colors::PRIMARY)),
            Span::styled(" new node", Style::default().fg(colors::DIMMED)),
        ]),
        NodesTab::Marketplace => {
            let mut spans = vec![
                Span::styled("[a]", Style::default().fg(colors::PRIMARY)),
                Span::styled("dd", Style::default().fg(colors::DIMMED)),
            ];

            if !app.sources.is_empty() {
                spans.extend(vec![
                    Span::styled("  [enter]", Style::default().fg(colors::PRIMARY)),
                    Span::styled(" edit", Style::default().fg(colors::DIMMED)),
                ]);

                if let Some(source) = app.sources.get(app.source_index) {
                    if source.enabled {
                        spans.extend(vec![
                            Span::styled("  [d]", Style::default().fg(colors::PRIMARY)),
                            Span::styled("isable", Style::default().fg(colors::DIMMED)),
                        ]);
                    } else {
                        spans.extend(vec![
                            Span::styled("  [e]", Style::default().fg(colors::PRIMARY)),
                            Span::styled("nable", Style::default().fg(colors::DIMMED)),
                        ]);
                    }
                }

                if app.confirm_remove {
                    spans.extend(vec![Span::styled(
                        "  [r] CONFIRM?",
                        Style::default().fg(colors::ERROR),
                    )]);
                } else {
                    spans.extend(vec![
                        Span::styled("  [r]", Style::default().fg(colors::PRIMARY)),
                        Span::styled("emove", Style::default().fg(colors::DIMMED)),
                    ]);
                }
            }

            Line::from(spans)
        }
    };

    f.render_widget(Paragraph::new(hints), area);
}

fn render_messages(f: &mut Frame, app: &App, area: ratatui::layout::Rect) {
    if let Some((text, msg_type)) = app.messages.last() {
        let color = match msg_type {
            crate::tui::app::MessageType::Info => colors::DIMMED,
            crate::tui::app::MessageType::Success => colors::SUCCESS,
            crate::tui::app::MessageType::Warning => colors::WARNING,
            crate::tui::app::MessageType::Error => colors::ERROR,
        };
        let line = Line::from(Span::styled(text.clone(), Style::default().fg(color)));
        f.render_widget(Paragraph::new(line), area);
    }
}

fn render_text_field(
    f: &mut Frame,
    area: ratatui::layout::Rect,
    label: &str,
    value: &str,
    placeholder: &str,
    is_active: bool,
) {
    let label_style = if is_active {
        Style::default().fg(colors::PRIMARY)
    } else {
        Style::default().fg(colors::DIMMED)
    };
    let border_color = if is_active {
        colors::PRIMARY
    } else {
        colors::BORDER
    };

    let block = Block::default()
        .title(Span::styled(format!(" {} ", label), label_style))
        .borders(Borders::ALL)
        .border_type(BorderType::Rounded)
        .border_style(Style::default().fg(border_color));

    let inner = block.inner(area);
    f.render_widget(block, area);

    let content = if is_active {
        format!("{}|", value)
    } else if value.is_empty() {
        placeholder.to_string()
    } else {
        value.to_string()
    };
    let text_style = if is_active {
        Style::default().fg(colors::TEXT)
    } else if value.is_empty() {
        Style::default().fg(colors::DIMMED)
    } else {
        Style::default().fg(colors::TEXT)
    };
    f.render_widget(Paragraph::new(Span::styled(content, text_style)), inner);
}

fn render_edit_source_form(f: &mut Frame, app: &App) {
    let area = f.area();
    let is_editing = app.marketplace_edit_path.is_some();
    let title = if is_editing {
        " Edit Marketplace Entry "
    } else {
        " Add Marketplace Entry "
    };

    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(3),
            Constraint::Length(3),
            Constraint::Length(3),
            Constraint::Length(3),
            Constraint::Min(1),
            Constraint::Length(1),
            Constraint::Length(1),
        ])
        .split(area);

    let header_block = Block::default()
        .title(Span::styled(
            title,
            Style::default()
                .fg(colors::PRIMARY)
                .add_modifier(Modifier::BOLD),
        ))
        .borders(Borders::ALL)
        .border_type(BorderType::Rounded)
        .border_style(Style::default().fg(colors::PRIMARY));

    let header_inner = header_block.inner(chunks[0]);
    f.render_widget(header_block, chunks[0]);

    let hints = Line::from(vec![
        Span::styled("[tab]", Style::default().fg(colors::PRIMARY)),
        Span::styled(" switch field  ", Style::default().fg(colors::DIMMED)),
        Span::styled("[enter]", Style::default().fg(colors::PRIMARY)),
        Span::styled(" save  ", Style::default().fg(colors::DIMMED)),
        Span::styled("[esc]", Style::default().fg(colors::PRIMARY)),
        Span::styled(" cancel", Style::default().fg(colors::DIMMED)),
    ]);
    f.render_widget(Paragraph::new(hints), header_inner);

    render_text_field(
        f,
        chunks[1],
        "Name",
        &app.marketplace_name,
        "(empty)",
        app.marketplace_active_field == 0,
    );

    render_text_field(
        f,
        chunks[2],
        "Path",
        &app.marketplace_path,
        "(empty)",
        app.marketplace_active_field == 1,
    );

    let help_lines = vec![
        Line::from(Span::styled(
            "Marketplace entries are directories containing nodes.",
            Style::default().fg(colors::DIMMED),
        )),
        Line::from(Span::styled(
            "They will be scanned in the Discover tab.",
            Style::default().fg(colors::DIMMED),
        )),
    ];
    f.render_widget(Paragraph::new(help_lines), chunks[3]);

    render_messages(f, app, chunks[5]);

    if app.exit_warning {
        let warning = Line::from(Span::styled(
            "Press Ctrl+C again to exit",
            Style::default().fg(colors::ERROR),
        ));
        f.render_widget(Paragraph::new(warning), chunks[6]);
    }
}

fn render_create_node_form(f: &mut Frame, app: &App) {
    let area = f.area();

    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(3), // Header
            Constraint::Length(3), // Name
            Constraint::Length(3), // Type
            Constraint::Length(3), // Description
            Constraint::Min(1),    // Spacer
            Constraint::Length(1), // Messages
            Constraint::Length(1), // Exit warning
        ])
        .split(area);

    // Header
    let header_block = Block::default()
        .title(Span::styled(
            " Create New Node ",
            Style::default()
                .fg(colors::PRIMARY)
                .add_modifier(Modifier::BOLD),
        ))
        .borders(Borders::ALL)
        .border_type(BorderType::Rounded)
        .border_style(Style::default().fg(colors::PRIMARY));

    let header_inner = header_block.inner(chunks[0]);
    f.render_widget(header_block, chunks[0]);

    let hints = Line::from(vec![
        Span::styled("[tab]", Style::default().fg(colors::PRIMARY)),
        Span::styled(" next field  ", Style::default().fg(colors::DIMMED)),
        Span::styled("[←/→]", Style::default().fg(colors::PRIMARY)),
        Span::styled(" toggle type  ", Style::default().fg(colors::DIMMED)),
        Span::styled("[enter]", Style::default().fg(colors::PRIMARY)),
        Span::styled(" submit  ", Style::default().fg(colors::DIMMED)),
        Span::styled("[esc]", Style::default().fg(colors::PRIMARY)),
        Span::styled(" cancel", Style::default().fg(colors::DIMMED)),
    ]);
    f.render_widget(Paragraph::new(hints), header_inner);

    // Name field
    render_text_field(
        f,
        chunks[1],
        "Name",
        &app.create_node_name,
        "(e.g., my-sensor)",
        app.create_node_active_field == 0,
    );

    // Type field
    let type_active = app.create_node_active_field == 1;
    let type_style = if type_active {
        Style::default().fg(colors::PRIMARY)
    } else {
        Style::default().fg(colors::DIMMED)
    };
    let type_border_color = if type_active {
        colors::PRIMARY
    } else {
        colors::BORDER
    };

    let type_block = Block::default()
        .title(Span::styled(" Type ", type_style))
        .borders(Borders::ALL)
        .border_type(BorderType::Rounded)
        .border_style(Style::default().fg(type_border_color));

    let type_inner = type_block.inner(chunks[2]);
    f.render_widget(type_block, chunks[2]);

    let rust_style = if app.create_node_type == 0 {
        Style::default()
            .fg(colors::RUST_TYPE)
            .add_modifier(Modifier::BOLD)
    } else {
        Style::default().fg(colors::DIMMED)
    };
    let python_style = if app.create_node_type == 1 {
        Style::default()
            .fg(colors::PYTHON_TYPE)
            .add_modifier(Modifier::BOLD)
    } else {
        Style::default().fg(colors::DIMMED)
    };

    let type_spans = vec![
        if app.create_node_type == 0 {
            Span::styled("[Rust]", rust_style)
        } else {
            Span::styled(" Rust ", rust_style)
        },
        Span::raw("  "),
        if app.create_node_type == 1 {
            Span::styled("[Python]", python_style)
        } else {
            Span::styled(" Python ", python_style)
        },
    ];
    f.render_widget(Paragraph::new(Line::from(type_spans)), type_inner);

    // Description field
    render_text_field(
        f,
        chunks[3],
        "Description",
        &app.create_node_description,
        "(optional)",
        app.create_node_active_field == 2,
    );

    // Messages
    render_messages(f, app, chunks[5]);

    // Exit warning
    if app.exit_warning {
        let warning = Line::from(Span::styled(
            "Press Ctrl+C again to exit",
            Style::default().fg(colors::ERROR),
        ));
        f.render_widget(Paragraph::new(warning), chunks[6]);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_truncate_path_short() {
        assert_eq!(truncate_path("/home/user", 20), "/home/user");
    }

    #[test]
    fn test_truncate_path_exact() {
        assert_eq!(truncate_path("12345", 5), "12345");
    }

    #[test]
    fn test_truncate_path_respects_max_chars() {
        let path = "/home/user/very/long/path/to/node";
        let result = truncate_path(path, 20);
        // Result should not exceed max_chars (including "..." prefix)
        assert!(result.chars().count() <= 20);
        assert!(result.starts_with("..."));
    }

    #[test]
    fn test_truncate_path_keeps_suffix() {
        let path = "/home/user/very/long/path/to/node";
        let result = truncate_path(path, 15);
        // Should keep the end of the path
        assert!(result.ends_with("node"));
        assert!(result.starts_with("..."));
        assert!(result.chars().count() <= 15);
    }

    #[test]
    fn test_truncate_path_small_max() {
        // Edge case: max_chars smaller than "..." length
        let result = truncate_path("/home/user/node", 3);
        assert_eq!(result, "...");
    }
}
