use argh::FromArgs;
use bubbaloop::api::models::chat::{ChatQuery, ChatResponse};
use crossterm::{
    event::{self, Event, KeyCode},
    terminal::{self, EnterAlternateScreen, LeaveAlternateScreen},
};
use ratatui::{
    backend::CrosstermBackend,
    layout::{Constraint, Direction, Layout},
    style::{Modifier, Style},
    widgets::{Block, Borders, List, ListItem, Paragraph},
    Terminal,
};

const DEFAULT_HOST: &str = "localhost";
const DEFAULT_PORT: u16 = 3000;

#[derive(FromArgs)]
#[argh(description = "Bubbaloop chat client")]
struct CLIArgs {
    #[argh(option, short = 'h', default = "DEFAULT_HOST.to_string()")]
    /// the host to connect to
    host: String,

    #[argh(option, short = 'p', default = "DEFAULT_PORT")]
    /// the port to connect to
    port: u16,
}

struct ChatAppConfig {
    host: String,
    port: u16,
}

struct ChatApp {
    messages: Vec<String>,
    input: String,
    scroll: usize,
    config: ChatAppConfig,
}

impl ChatApp {
    fn new(config: ChatAppConfig) -> Self {
        Self {
            messages: Vec::new(),
            input: String::new(),
            scroll: 0,
            config,
        }
    }

    /// Process a message and get a response from the server
    async fn process_message(&self, input: &str) -> Result<String, Box<dyn std::error::Error>> {
        let client = reqwest::Client::new();
        let response = client
            .post(format!(
                "http://{}:{}/api/v0/inference/chat",
                self.config.host, self.config.port
            ))
            .json(&ChatQuery {
                message: input.to_string(),
            })
            .send()
            .await?;
        let body: ChatResponse = response.json().await?;
        match body {
            ChatResponse::Success(message) => Ok(message),
            ChatResponse::Error { error } => Err(Box::new(std::io::Error::new(
                std::io::ErrorKind::Other,
                error,
            ))),
        }
    }

    async fn run(&mut self) -> Result<(), Box<dyn std::error::Error>> {
        // Setup terminal
        terminal::enable_raw_mode()?;
        let mut stdout = std::io::stdout();
        crossterm::execute!(stdout, EnterAlternateScreen)?;
        let backend = CrosstermBackend::new(stdout);
        let mut terminal = Terminal::new(backend)?;

        loop {
            // Draw UI
            terminal.draw(|f| {
                let chunks = Layout::default()
                    .direction(Direction::Vertical)
                    .constraints([Constraint::Min(1), Constraint::Length(3)].as_ref())
                    .split(f.area());

                // Chat messages
                let messages: Vec<ListItem> = self
                    .messages
                    .iter()
                    .map(|m| ListItem::new(m.as_str()))
                    .collect();
                let messages = List::new(messages)
                    .block(Block::default().borders(Borders::ALL).title("Messages"))
                    .highlight_style(Style::default().add_modifier(Modifier::BOLD));
                f.render_widget(messages, chunks[0]);

                // Input box
                let input = Paragraph::new(self.input.as_str())
                    .block(Block::default().borders(Borders::ALL).title("Input"));
                f.render_widget(input, chunks[1]);
            })?;

            // Handle input
            if let Event::Key(key) = event::read()? {
                match key.code {
                    KeyCode::Enter => {
                        if !self.input.is_empty() {
                            self.messages.push(format!("> {}", self.input));

                            let response = self.process_message(&self.input).await?;
                            self.messages.push(response);

                            self.input.clear();
                        }
                    }
                    KeyCode::Char(c) => {
                        self.input.push(c);
                    }
                    KeyCode::Backspace => {
                        self.input.pop();
                    }
                    KeyCode::Esc => {
                        break;
                    }
                    KeyCode::Up => {
                        if self.scroll > 0 {
                            self.scroll -= 1;
                        }
                    }
                    KeyCode::Down => {
                        if self.scroll < self.messages.len().saturating_sub(1) {
                            self.scroll += 1;
                        }
                    }
                    _ => {}
                }
            }
        }

        // Cleanup
        terminal::disable_raw_mode()?;
        crossterm::execute!(terminal.backend_mut(), LeaveAlternateScreen)?;

        Ok(())
    }
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let args: CLIArgs = argh::from_env();
    let config = ChatAppConfig {
        host: args.host,
        port: args.port,
    };
    let mut app = ChatApp::new(config);
    app.run().await?;
    Ok(())
}
