//! TUI module for Bubbaloop
//!
//! This module contains the terminal user interface for Bubbaloop,
//! providing an interactive way to manage nodes and services.

use std::io;
use std::time::Duration;

use anyhow::Result;
use crossterm::{
    event::{
        self as crossterm_event, DisableMouseCapture, EnableMouseCapture, Event, KeyCode,
        KeyModifiers,
    },
    execute,
    terminal::{disable_raw_mode, enable_raw_mode, EnterAlternateScreen, LeaveAlternateScreen},
};
use ratatui::{backend::CrosstermBackend, Terminal};

pub mod app;
pub mod config;
pub mod daemon;
pub mod ui;

use app::App;

/// Run the TUI application
pub async fn run() -> Result<()> {
    // Setup terminal
    enable_raw_mode()?;
    let mut stdout = io::stdout();
    execute!(stdout, EnterAlternateScreen, EnableMouseCapture)?;
    let backend = CrosstermBackend::new(stdout);
    let mut terminal = Terminal::new(backend)?;

    // Create app and run
    let mut app = App::new().await;
    let result = run_app(&mut terminal, &mut app).await;

    // Restore terminal
    disable_raw_mode()?;
    execute!(
        terminal.backend_mut(),
        LeaveAlternateScreen,
        DisableMouseCapture
    )?;
    terminal.show_cursor()?;

    if let Err(err) = result {
        eprintln!("Error: {err:?}");
    }

    Ok(())
}

async fn run_app<B: ratatui::backend::Backend>(
    terminal: &mut Terminal<B>,
    app: &mut App,
) -> Result<()> {
    loop {
        terminal.draw(|f| ui::render(f, app))?;

        // Poll for events with timeout for animations
        if crossterm_event::poll(Duration::from_millis(16))? {
            if let Event::Key(key) = crossterm_event::read()? {
                // Handle Ctrl+C for exit
                if key.modifiers.contains(KeyModifiers::CONTROL)
                    && (key.code == KeyCode::Char('c') || key.code == KeyCode::Char('x'))
                {
                    if app.handle_exit_request() {
                        break;
                    }
                    continue;
                }

                // Pass key to app
                if app.handle_key(key).await {
                    break;
                }
            }
        }

        // Tick for animations and background updates
        app.tick().await?;

        // Check exit warning timeout
        app.check_exit_timeout();
    }

    Ok(())
}
