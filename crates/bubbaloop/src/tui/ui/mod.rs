mod components;
mod home;
mod nodes;
mod services;

use ratatui::Frame;

use crate::tui::app::{App, View};

/// Main render function - dispatches to view-specific renderers
pub fn render(f: &mut Frame, app: &App) {
    match &app.view {
        View::Home => home::render(f, app),
        View::Services => services::render(f, app),
        View::Nodes(_) => nodes::render_list(f, app),
        View::NodeDetail(name) => nodes::render_detail(f, app, name),
        View::NodeLogs(name) => nodes::render_logs(f, app, name),
    }
}
