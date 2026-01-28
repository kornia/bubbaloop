pub mod spinner;

pub use spinner::flower_spinner;

use ratatui::{
    style::Style,
    text::{Line, Span},
};

/// Color palette - matching the Ink TUI
pub mod colors {
    use ratatui::style::Color;

    pub const PRIMARY: Color = Color::Rgb(78, 205, 196); // #4ECDC4
    pub const SUCCESS: Color = Color::Rgb(149, 225, 211); // #95E1D3
    pub const WARNING: Color = Color::Rgb(255, 217, 61); // #FFD93D
    pub const ERROR: Color = Color::Rgb(255, 107, 107); // #FF6B6B
    pub const DIMMED: Color = Color::Rgb(136, 136, 136); // #888
    pub const TEXT: Color = Color::Rgb(204, 204, 204); // #CCC
    pub const BORDER: Color = Color::Rgb(68, 68, 68); // #444
    pub const RUST_TYPE: Color = Color::Rgb(255, 217, 61); // #FFD93D (same as warning)
    pub const PYTHON_TYPE: Color = Color::Rgb(78, 205, 196); // #4ECDC4 (same as primary)
}

/// Create a styled header line with Claude Code style borders
pub fn header_line(title: &str, version: &str, width: usize) -> Line<'static> {
    let title_part = format!("─── {} ", title);
    let version_part = format!("v{} ", version);
    let remaining = width.saturating_sub(title_part.len() + version_part.len() + 2); // 2 for ╭ and ╮

    Line::from(vec![
        Span::styled("╭", Style::default().fg(colors::PRIMARY)),
        Span::styled(title_part, Style::default().fg(colors::PRIMARY)),
        Span::styled(version_part, Style::default().fg(colors::DIMMED)),
        Span::styled("─".repeat(remaining), Style::default().fg(colors::PRIMARY)),
        Span::styled("╮", Style::default().fg(colors::PRIMARY)),
    ])
}

/// Create a footer line
pub fn footer_line(width: usize) -> Line<'static> {
    let inner = "─".repeat(width.saturating_sub(2));
    Line::from(vec![
        Span::styled("╰", Style::default().fg(colors::PRIMARY)),
        Span::styled(inner, Style::default().fg(colors::PRIMARY)),
        Span::styled("╯", Style::default().fg(colors::PRIMARY)),
    ])
}
