use ratatui::{style::Style, text::Span};

use super::colors;

/// Flower spinner frames
const FLOWER_FRAMES: &[char] = &['✻', '✼', '✽', '✾', '✿', '❀', '❁'];

/// Get flower spinner character for current frame
pub fn flower_spinner(frame: usize) -> Span<'static> {
    let ch = FLOWER_FRAMES[frame % FLOWER_FRAMES.len()];
    Span::styled(ch.to_string(), Style::default().fg(colors::WARNING))
}

/// Random logs verb
pub fn logs_verb(frame: usize) -> &'static str {
    const VERBS: &[&str] = &[
        "Observing",
        "Watching",
        "Monitoring",
        "Scrutinizing",
        "Surveying",
        "Peering",
    ];
    VERBS[(frame / 10) % VERBS.len()]
}
