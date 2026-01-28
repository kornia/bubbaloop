use std::time::{Duration, Instant};

use anyhow::Result;
use crossterm::event::{KeyCode, KeyEvent};

use crate::templates;
use crate::tui::config::Registry;
use crate::tui::daemon::DaemonClient;

/// Current view mode
#[derive(Debug, Clone, PartialEq)]
pub enum View {
    Home,
    Services,
    Nodes(NodesTab),
    NodeDetail(String), // Node name
    NodeLogs(String),   // Node name
}

/// Nodes view tab
#[derive(Debug, Clone, PartialEq, Default)]
pub enum NodesTab {
    #[default]
    Installed,
    Discover,
    Marketplace,
}

/// Service status
#[derive(Debug, Clone, PartialEq)]
pub enum ServiceStatus {
    Running,
    Stopped,
    Failed,
    Unknown,
}

/// Service info
#[derive(Debug, Clone)]
pub struct ServiceInfo {
    pub name: String,
    pub display_name: String,
    pub status: ServiceStatus,
}

/// Node info from daemon
#[derive(Debug, Clone)]
pub struct NodeInfo {
    pub name: String,
    pub path: String,
    pub version: String,
    pub node_type: String,
    pub description: String,
    pub status: String,
    pub is_built: bool,
}

/// Discoverable node
#[derive(Debug, Clone)]
pub struct DiscoverableNode {
    pub path: String,
    pub name: String,
    pub version: String,
    pub node_type: String,
    pub source: String,
}

/// Marketplace source
#[derive(Debug, Clone)]
pub struct MarketplaceSource {
    pub name: String,
    pub path: String,
    pub source_type: String,
    pub enabled: bool,
}

/// Input mode for text entry
#[derive(Debug, Clone, PartialEq)]
pub enum InputMode {
    Normal,
    Command,    // Typing a /command
    EditSource, // Editing marketplace source
    CreateNode, // Creating a new node form
}

/// Application state
pub struct App {
    /// Current view
    pub view: View,

    /// Should exit flag (set by /quit command)
    pub should_exit: bool,

    /// Exit warning state (double Ctrl+C)
    pub exit_warning: bool,
    pub exit_warning_time: Option<Instant>,

    /// Input state
    pub input_mode: InputMode,
    pub input: String,
    pub input_cursor: usize,
    pub command_history: Vec<String>,

    /// Command suggestions
    pub command_index: usize,

    /// Messages to display
    pub messages: Vec<(String, MessageType)>,

    /// Services
    pub services: Vec<ServiceInfo>,
    pub service_index: usize,

    /// Nodes
    pub nodes: Vec<NodeInfo>,
    pub node_index: usize,
    pub discoverable_nodes: Vec<DiscoverableNode>,
    pub discover_index: usize,
    pub sources: Vec<MarketplaceSource>,
    pub source_index: usize,

    /// Daemon client
    pub daemon_client: Option<DaemonClient>,
    pub daemon_available: bool,

    /// Config registry
    pub registry: Registry,

    /// Animation frame
    pub spinner_frame: usize,
    pub last_spinner_update: Instant,
    pub robot_eyes_on: bool,
    pub last_robot_blink: Instant,

    /// Last status refresh
    pub last_refresh: Instant,

    /// Confirmation states
    pub confirm_remove: bool,
    pub confirm_uninstall: bool,
    pub confirm_clean: bool,

    /// Logs content
    pub logs: Vec<String>,

    /// Build output
    pub build_output: Vec<String>,
    pub is_building: bool,

    /// Service status output
    pub service_status_text: Vec<String>,

    /// Marketplace form state
    pub marketplace_name: String,
    pub marketplace_path: String,
    pub marketplace_active_field: usize, // 0 = name, 1 = path
    pub marketplace_edit_path: Option<String>, // None = add, Some = edit

    /// Create node form state
    pub create_node_name: String,
    pub create_node_type: usize, // 0 = rust, 1 = python
    pub create_node_description: String,
    pub create_node_active_field: usize, // 0 = name, 1 = type, 2 = description

    /// Pending node path for async daemon registration
    pub pending_node_path: Option<String>,
}

#[derive(Debug, Clone, PartialEq)]
pub enum MessageType {
    Info,
    Success,
    Warning,
    Error,
}

/// Available commands
pub const COMMANDS: &[(&str, &str)] = &[
    ("/nodes", "Manage local nodes"),
    ("/services", "Show service status"),
    ("/quit", "Exit Bubbaloop"),
];

impl App {
    pub async fn new() -> Self {
        let registry = Registry::load();

        // Try to connect to daemon
        let (daemon_client, daemon_available) = match DaemonClient::new().await {
            Ok(client) => {
                let available = client.is_available().await;
                (Some(client), available)
            }
            Err(_) => (None, false),
        };

        let now = Instant::now();

        Self {
            view: View::Home,
            should_exit: false,
            exit_warning: false,
            exit_warning_time: None,
            input_mode: InputMode::Normal,
            input: String::new(),
            input_cursor: 0,
            command_history: Vec::new(),
            command_index: 0,
            messages: Vec::new(),
            services: vec![
                ServiceInfo {
                    name: "bubbaloop-zenohd".into(),
                    display_name: "zenohd".into(),
                    status: ServiceStatus::Unknown,
                },
                ServiceInfo {
                    name: "bubbaloop-bridge".into(),
                    display_name: "bridge".into(),
                    status: ServiceStatus::Unknown,
                },
                ServiceInfo {
                    name: "bubbaloop-daemon".into(),
                    display_name: "daemon".into(),
                    status: ServiceStatus::Unknown,
                },
            ],
            service_index: 0,
            nodes: Vec::new(),
            node_index: 0,
            discoverable_nodes: Vec::new(),
            discover_index: 0,
            sources: Vec::new(),
            source_index: 0,
            daemon_client,
            daemon_available,
            registry,
            spinner_frame: 0,
            last_spinner_update: now,
            robot_eyes_on: true,
            last_robot_blink: now,
            last_refresh: now - Duration::from_secs(10), // Force initial refresh
            confirm_remove: false,
            confirm_uninstall: false,
            confirm_clean: false,
            logs: Vec::new(),
            build_output: Vec::new(),
            is_building: false,
            service_status_text: Vec::new(),
            marketplace_name: String::new(),
            marketplace_path: String::new(),
            marketplace_active_field: 0,
            marketplace_edit_path: None,
            create_node_name: String::new(),
            create_node_type: 0,
            create_node_description: String::new(),
            create_node_active_field: 0,
            pending_node_path: None,
        }
    }

    /// Handle exit request (Ctrl+C)
    /// Returns true if should exit
    pub fn handle_exit_request(&mut self) -> bool {
        if self.exit_warning {
            true
        } else {
            self.exit_warning = true;
            self.exit_warning_time = Some(Instant::now());
            false
        }
    }

    /// Check exit warning timeout
    pub fn check_exit_timeout(&mut self) {
        if let Some(time) = self.exit_warning_time {
            if time.elapsed() > Duration::from_secs(2) {
                self.exit_warning = false;
                self.exit_warning_time = None;
            }
        }
    }

    /// Handle key event
    /// Returns true if should exit
    pub async fn handle_key(&mut self, key: KeyEvent) -> bool {
        match &self.view {
            View::Home => self.handle_home_key(key).await,
            View::Services => self.handle_services_key(key).await,
            View::Nodes(_) => self.handle_nodes_key(key).await,
            View::NodeDetail(_) => self.handle_node_detail_key(key).await,
            View::NodeLogs(_) => self.handle_node_logs_key(key).await,
        }
    }

    async fn handle_home_key(&mut self, key: KeyEvent) -> bool {
        match self.input_mode {
            InputMode::Normal => match key.code {
                KeyCode::Char('/') => {
                    self.input_mode = InputMode::Command;
                    self.input = "/".into();
                    self.input_cursor = 1;
                }
                KeyCode::Esc => {
                    self.input.clear();
                    self.input_cursor = 0;
                }
                _ => {}
            },
            InputMode::Command => match key.code {
                KeyCode::Enter => {
                    self.execute_command().await;
                    if self.should_exit {
                        return true;
                    }
                }
                KeyCode::Esc => {
                    self.input_mode = InputMode::Normal;
                    self.input.clear();
                    self.input_cursor = 0;
                }
                KeyCode::Backspace => {
                    if self.input_cursor > 0 {
                        self.input.remove(self.input_cursor - 1);
                        self.input_cursor -= 1;
                    }
                    if self.input.is_empty() {
                        self.input_mode = InputMode::Normal;
                    }
                }
                KeyCode::Left => {
                    if self.input_cursor > 0 {
                        self.input_cursor -= 1;
                    }
                }
                KeyCode::Right => {
                    if self.input_cursor < self.input.len() {
                        self.input_cursor += 1;
                    }
                }
                KeyCode::Up => {
                    if self.command_index > 0 {
                        self.command_index -= 1;
                    }
                }
                KeyCode::Down => {
                    let filtered = self.filtered_commands();
                    if self.command_index < filtered.len().saturating_sub(1) {
                        self.command_index += 1;
                    }
                }
                KeyCode::Tab => {
                    let filtered = self.filtered_commands();
                    if let Some((cmd, _)) = filtered.get(self.command_index) {
                        self.input = (*cmd).to_string();
                        self.input_cursor = self.input.len();
                    }
                }
                KeyCode::Char(c) => {
                    self.input.insert(self.input_cursor, c);
                    self.input_cursor += 1;
                    self.command_index = 0;
                }
                _ => {}
            },
            _ => {}
        }
        false
    }

    async fn handle_services_key(&mut self, key: KeyEvent) -> bool {
        match key.code {
            KeyCode::Esc | KeyCode::Char('q') => {
                self.view = View::Home;
                self.messages.clear();
            }
            KeyCode::Up | KeyCode::Char('k') => {
                if self.service_index > 0 {
                    self.service_index -= 1;
                }
            }
            KeyCode::Down | KeyCode::Char('j') => {
                if self.service_index < self.services.len().saturating_sub(1) {
                    self.service_index += 1;
                }
            }
            KeyCode::Char('s') => {
                self.start_service().await;
            }
            KeyCode::Char('x') => {
                self.stop_service().await;
            }
            KeyCode::Char('r') => {
                self.restart_service().await;
            }
            _ => {}
        }
        false
    }

    async fn handle_nodes_key(&mut self, key: KeyEvent) -> bool {
        // Handle create node form first
        if self.input_mode == InputMode::CreateNode {
            return self.handle_create_node_key(key);
        }

        if self.input_mode == InputMode::EditSource {
            return self.handle_edit_source_key(key);
        }

        let current_tab = if let View::Nodes(tab) = &self.view {
            tab.clone()
        } else {
            return false;
        };

        match key.code {
            KeyCode::Esc | KeyCode::Char('q') => {
                self.view = View::Home;
                self.messages.clear();
            }
            KeyCode::Tab => {
                let next = match current_tab {
                    NodesTab::Installed => NodesTab::Discover,
                    NodesTab::Discover => NodesTab::Marketplace,
                    NodesTab::Marketplace => NodesTab::Installed,
                };
                self.view = View::Nodes(next);
                self.reset_confirmations();
            }
            KeyCode::BackTab => {
                let prev = match current_tab {
                    NodesTab::Installed => NodesTab::Marketplace,
                    NodesTab::Discover => NodesTab::Installed,
                    NodesTab::Marketplace => NodesTab::Discover,
                };
                self.view = View::Nodes(prev);
                self.reset_confirmations();
            }
            KeyCode::Char('1') => {
                self.view = View::Nodes(NodesTab::Installed);
                self.reset_confirmations();
            }
            KeyCode::Char('2') => {
                self.view = View::Nodes(NodesTab::Discover);
                self.reset_confirmations();
            }
            KeyCode::Char('3') => {
                self.view = View::Nodes(NodesTab::Marketplace);
                self.reset_confirmations();
            }
            _ => match current_tab {
                NodesTab::Installed => self.handle_installed_tab_key(key).await,
                NodesTab::Discover => self.handle_discover_tab_key(key).await,
                NodesTab::Marketplace => self.handle_marketplace_tab_key(key).await,
            },
        }
        false
    }

    async fn handle_installed_tab_key(&mut self, key: KeyEvent) {
        match key.code {
            KeyCode::Up | KeyCode::Char('k') => {
                if self.node_index > 0 {
                    self.node_index -= 1;
                }
            }
            KeyCode::Down | KeyCode::Char('j') => {
                if self.node_index < self.nodes.len().saturating_sub(1) {
                    self.node_index += 1;
                }
            }
            KeyCode::Enter => {
                if let Some(node) = self.nodes.get(self.node_index) {
                    let node_name = node.name.clone();
                    self.view = View::NodeDetail(node_name.clone());
                    self.fetch_service_status(&node_name).await;
                }
            }
            KeyCode::Char('s') | KeyCode::Char(' ') => {
                self.toggle_node().await;
            }
            _ => {}
        }
    }

    async fn handle_discover_tab_key(&mut self, key: KeyEvent) {
        match key.code {
            KeyCode::Up | KeyCode::Char('k') => {
                if self.discover_index > 0 {
                    self.discover_index -= 1;
                }
            }
            KeyCode::Down | KeyCode::Char('j') => {
                if self.discover_index < self.discoverable_nodes.len().saturating_sub(1) {
                    self.discover_index += 1;
                }
            }
            KeyCode::Enter | KeyCode::Char('a') => {
                self.add_discovered_node().await;
            }
            KeyCode::Char('n') => {
                // Enter create node form
                self.input_mode = InputMode::CreateNode;
                self.create_node_name.clear();
                self.create_node_type = 0;
                self.create_node_description.clear();
                self.create_node_active_field = 0;
            }
            _ => {}
        }
    }

    async fn handle_marketplace_tab_key(&mut self, key: KeyEvent) {
        match key.code {
            KeyCode::Up | KeyCode::Char('k') => {
                if self.source_index > 0 {
                    self.source_index -= 1;
                }
                self.confirm_remove = false;
            }
            KeyCode::Down | KeyCode::Char('j') => {
                if self.source_index < self.sources.len().saturating_sub(1) {
                    self.source_index += 1;
                }
                self.confirm_remove = false;
            }
            KeyCode::Char('a') => {
                self.input_mode = InputMode::EditSource;
                self.marketplace_name.clear();
                self.marketplace_path.clear();
                self.marketplace_active_field = 0;
                self.marketplace_edit_path = None;
            }
            KeyCode::Enter => {
                if let Some(source) = self.sources.get(self.source_index) {
                    self.input_mode = InputMode::EditSource;
                    self.marketplace_name = source.name.clone();
                    self.marketplace_path = source.path.clone();
                    self.marketplace_active_field = 0;
                    self.marketplace_edit_path = Some(source.path.clone());
                }
            }
            KeyCode::Char('e') => {
                self.enable_source().await;
            }
            KeyCode::Char('d') => {
                self.disable_source().await;
            }
            KeyCode::Char('r') => {
                if self.confirm_remove {
                    self.remove_source().await;
                    self.confirm_remove = false;
                } else {
                    self.confirm_remove = true;
                    self.add_message(
                        "Press [r] again to confirm removal".into(),
                        MessageType::Warning,
                    );
                }
            }
            _ => {}
        }
    }

    async fn handle_node_detail_key(&mut self, key: KeyEvent) -> bool {
        match key.code {
            KeyCode::Esc => {
                self.view = View::Nodes(NodesTab::Installed);
            }
            KeyCode::Tab => {
                if self.node_index < self.nodes.len().saturating_sub(1) {
                    self.node_index += 1;
                } else {
                    self.node_index = 0;
                }
                if let Some(node) = self.nodes.get(self.node_index) {
                    self.view = View::NodeDetail(node.name.clone());
                }
            }
            KeyCode::BackTab => {
                if self.node_index > 0 {
                    self.node_index -= 1;
                } else {
                    self.node_index = self.nodes.len().saturating_sub(1);
                }
                if let Some(node) = self.nodes.get(self.node_index) {
                    self.view = View::NodeDetail(node.name.clone());
                }
            }
            KeyCode::Char('s') => {
                self.toggle_current_node().await;
            }
            KeyCode::Char('b') => {
                self.build_current_node().await;
            }
            KeyCode::Char('c') => {
                if self.confirm_clean {
                    self.clean_current_node().await;
                    self.confirm_clean = false;
                } else {
                    self.confirm_clean = true;
                    self.add_message(
                        "Press [c] again to confirm clean".into(),
                        MessageType::Warning,
                    );
                }
            }
            KeyCode::Char('e') => {
                self.install_current_node().await;
            }
            KeyCode::Char('d') => {
                self.uninstall_current_node_service().await;
            }
            KeyCode::Char('u') => {
                if self.confirm_uninstall {
                    self.uninstall_current_node().await;
                    self.confirm_uninstall = false;
                } else {
                    self.confirm_uninstall = true;
                    self.add_message(
                        "Press [u] again to confirm uninstall".into(),
                        MessageType::Warning,
                    );
                }
            }
            KeyCode::Char('l') => {
                let name = if let View::NodeDetail(n) = &self.view {
                    Some(n.clone())
                } else {
                    None
                };
                if let Some(name) = name {
                    self.view = View::NodeLogs(name.clone());
                    self.fetch_node_logs(&name).await;
                }
            }
            _ => {}
        }
        false
    }

    async fn handle_node_logs_key(&mut self, key: KeyEvent) -> bool {
        match key.code {
            KeyCode::Esc => {
                let name = if let View::NodeLogs(n) = &self.view {
                    Some(n.clone())
                } else {
                    None
                };
                if let Some(name) = name {
                    self.view = View::NodeDetail(name);
                }
            }
            KeyCode::Tab => {
                if self.node_index < self.nodes.len().saturating_sub(1) {
                    self.node_index += 1;
                } else {
                    self.node_index = 0;
                }
                if let Some(node) = self.nodes.get(self.node_index) {
                    let name = node.name.clone();
                    self.view = View::NodeLogs(name.clone());
                    self.fetch_node_logs(&name).await;
                }
            }
            KeyCode::BackTab => {
                if self.node_index > 0 {
                    self.node_index -= 1;
                } else {
                    self.node_index = self.nodes.len().saturating_sub(1);
                }
                if let Some(node) = self.nodes.get(self.node_index) {
                    let name = node.name.clone();
                    self.view = View::NodeLogs(name.clone());
                    self.fetch_node_logs(&name).await;
                }
            }
            _ => {}
        }
        false
    }

    async fn execute_command(&mut self) {
        let mut cmd = self.input.trim().to_lowercase();

        let is_exact = COMMANDS.iter().any(|(c, _)| *c == cmd);
        if !is_exact && cmd.starts_with('/') {
            let filtered = self.filtered_commands();
            if let Some((selected_cmd, _)) = filtered.get(self.command_index) {
                cmd = selected_cmd.to_string();
            }
        }

        if !cmd.is_empty() && self.command_history.last() != Some(&cmd) {
            self.command_history.push(cmd.clone());
        }

        match cmd.as_str() {
            "/quit" | "/exit" | "/q" => {
                self.should_exit = true;
                return;
            }
            "/nodes" => {
                self.view = View::Nodes(NodesTab::Installed);
            }
            "/services" => {
                self.view = View::Services;
            }
            _ => {
                if cmd.starts_with('/') {
                    self.add_message(format!("Unknown command: {}", cmd), MessageType::Error);
                }
            }
        }

        self.input.clear();
        self.input_cursor = 0;
        self.input_mode = InputMode::Normal;
    }

    pub fn filtered_commands(&self) -> Vec<(&'static str, &'static str)> {
        COMMANDS
            .iter()
            .filter(|(cmd, _)| cmd.to_lowercase().starts_with(&self.input.to_lowercase()))
            .copied()
            .collect()
    }

    pub fn add_message(&mut self, text: String, msg_type: MessageType) {
        self.messages.push((text, msg_type));
        if self.messages.len() > 10 {
            self.messages.remove(0);
        }
    }

    fn reset_confirmations(&mut self) {
        self.confirm_remove = false;
        self.confirm_uninstall = false;
        self.confirm_clean = false;
    }

    pub async fn tick(&mut self) -> Result<()> {
        let now = Instant::now();

        if now.duration_since(self.last_spinner_update) > Duration::from_millis(150) {
            self.spinner_frame = (self.spinner_frame + 1) % 7;
            self.last_spinner_update = now;
        }

        if now.duration_since(self.last_robot_blink) > Duration::from_millis(800) {
            self.robot_eyes_on = !self.robot_eyes_on;
            self.last_robot_blink = now;
        }

        if now.duration_since(self.last_refresh) > Duration::from_millis(250) {
            self.refresh_status().await?;
            self.last_refresh = now;
        }

        Ok(())
    }

    async fn refresh_status(&mut self) -> Result<()> {
        self.refresh_services().await;

        // Register pending node with daemon
        if let Some(path) = self.pending_node_path.take() {
            if let Some(client) = &self.daemon_client {
                let _ = client.add_node(&path).await;
            }
        }

        if let Some(client) = &self.daemon_client {
            if let Ok(nodes) = client.list_nodes().await {
                if !nodes.is_empty() || self.nodes.is_empty() {
                    self.nodes = nodes;
                    self.daemon_available = true;
                }
            } else {
                self.daemon_available = false;
            }
        }

        if let View::Nodes(NodesTab::Discover) = self.view {
            self.refresh_discoverable_nodes();
        }
        if let View::Nodes(NodesTab::Marketplace) = self.view {
            self.refresh_sources();
        }

        if let View::NodeLogs(ref node_name) = self.view {
            let name = node_name.clone();
            self.fetch_node_logs(&name).await;
        }

        if let View::NodeDetail(ref node_name) = self.view {
            let name = node_name.clone();
            self.fetch_service_status(&name).await;
        }

        Ok(())
    }

    async fn refresh_services(&mut self) {
        let names: Vec<String> = self.services.iter().map(|s| s.name.clone()).collect();
        for (i, name) in names.iter().enumerate() {
            let status = self.check_service_status(name).await;
            if let Some(service) = self.services.get_mut(i) {
                service.status = status;
            }
        }
    }

    async fn check_service_status(&self, service_name: &str) -> ServiceStatus {
        let output = tokio::process::Command::new("systemctl")
            .args(["--user", "is-active", service_name])
            .output()
            .await;

        match output {
            Ok(out) => {
                let status = String::from_utf8_lossy(&out.stdout).trim().to_string();
                match status.as_str() {
                    "active" => ServiceStatus::Running,
                    "inactive" => ServiceStatus::Stopped,
                    "failed" => ServiceStatus::Failed,
                    _ => ServiceStatus::Unknown,
                }
            }
            Err(_) => ServiceStatus::Unknown,
        }
    }

    async fn fetch_node_logs(&mut self, node_name: &str) {
        let service_unit = format!("_SYSTEMD_USER_UNIT=bubbaloop-{}.service", node_name);
        let output = tokio::process::Command::new("journalctl")
            .args([&service_unit, "-n", "50", "--no-pager"])
            .output()
            .await;

        self.logs.clear();
        self.logs.push(format!("=== Logs for {} ===", node_name));

        match output {
            Ok(out) => {
                if out.status.success() {
                    let log_text = String::from_utf8_lossy(&out.stdout);
                    if log_text.trim().is_empty() {
                        self.logs.push("No logs available".to_string());
                    } else {
                        for line in log_text.lines() {
                            self.logs.push(line.to_string());
                        }
                    }
                } else {
                    let error = String::from_utf8_lossy(&out.stderr);
                    self.logs.push(format!("Error fetching logs: {}", error));
                }
            }
            Err(e) => {
                self.logs.push(format!("Failed to run journalctl: {}", e));
            }
        }
    }

    async fn fetch_service_status(&mut self, node_name: &str) {
        let service_name = format!("bubbaloop-{}", node_name);
        let output = tokio::process::Command::new("systemctl")
            .args(["--user", "status", &service_name])
            .output()
            .await;

        match output {
            Ok(out) => {
                let stdout = String::from_utf8_lossy(&out.stdout);
                self.service_status_text = stdout.lines().map(|s| s.to_string()).collect();
            }
            Err(e) => {
                self.service_status_text = vec![format!("Error fetching status: {}", e)];
            }
        }
    }

    fn refresh_discoverable_nodes(&mut self) {
        self.discoverable_nodes = self.registry.scan_discoverable_nodes(&self.nodes);
    }

    fn refresh_sources(&mut self) {
        self.sources = self.registry.get_sources();
    }

    async fn start_service(&mut self) {
        if let Some(service) = self.services.get(self.service_index) {
            let name = service.name.clone();
            let _ = tokio::process::Command::new("systemctl")
                .args(["--user", "start", &name])
                .output()
                .await;
            self.add_message(format!("Starting {}", name), MessageType::Info);
        }
    }

    async fn stop_service(&mut self) {
        if let Some(service) = self.services.get(self.service_index) {
            let name = service.name.clone();
            let _ = tokio::process::Command::new("systemctl")
                .args(["--user", "stop", &name])
                .output()
                .await;
            self.add_message(format!("Stopping {}", name), MessageType::Info);
        }
    }

    async fn restart_service(&mut self) {
        if let Some(service) = self.services.get(self.service_index) {
            let name = service.name.clone();
            let _ = tokio::process::Command::new("systemctl")
                .args(["--user", "restart", &name])
                .output()
                .await;
            self.add_message(format!("Restarting {}", name), MessageType::Info);
        }
    }

    async fn toggle_node(&mut self) {
        let node_data = self
            .nodes
            .get(self.node_index)
            .map(|n| (n.name.clone(), n.status.clone(), n.is_built));

        if let Some((name, status, is_built)) = node_data {
            if status == "running" {
                self.stop_node(&name).await;
            } else if is_built {
                self.start_node(&name).await;
            } else {
                self.add_message(
                    "Build first (press enter for details)".into(),
                    MessageType::Warning,
                );
            }
        }
    }

    async fn start_node(&mut self, name: &str) {
        if let Some(client) = &self.daemon_client {
            match client.execute_command(name, "start").await {
                Ok(_) => {
                    self.add_message(format!("Started: {}", name), MessageType::Success);
                    let _ = self.refresh_status().await;
                }
                Err(e) => self.add_message(format!("Error: {}", e), MessageType::Error),
            }
        }
    }

    async fn stop_node(&mut self, name: &str) {
        if let Some(client) = &self.daemon_client {
            match client.execute_command(name, "stop").await {
                Ok(_) => {
                    self.add_message(format!("Stopped: {}", name), MessageType::Success);
                    let _ = self.refresh_status().await;
                }
                Err(e) => self.add_message(format!("Error: {}", e), MessageType::Error),
            }
        }
    }

    async fn toggle_current_node(&mut self) {
        if let View::NodeDetail(name) = &self.view {
            let name = name.clone();
            if let Some(node) = self.nodes.iter().find(|n| n.name == name) {
                if node.status == "running" {
                    self.stop_node(&name).await;
                } else if node.is_built {
                    self.start_node(&name).await;
                } else {
                    self.add_message("Cannot start: node not built".into(), MessageType::Warning);
                }
            }
        }
    }

    async fn build_current_node(&mut self) {
        if let View::NodeDetail(name) = &self.view {
            if let Some(client) = &self.daemon_client {
                match client.execute_command(name, "build").await {
                    Ok(_) => {
                        self.add_message(format!("Building: {}", name), MessageType::Info);
                        self.is_building = true;
                    }
                    Err(e) => self.add_message(format!("Error: {}", e), MessageType::Error),
                }
            }
        }
    }

    async fn clean_current_node(&mut self) {
        if let View::NodeDetail(name) = &self.view {
            if let Some(client) = &self.daemon_client {
                match client.execute_command(name, "clean").await {
                    Ok(_) => self.add_message(format!("Cleaning: {}", name), MessageType::Info),
                    Err(e) => self.add_message(format!("Error: {}", e), MessageType::Error),
                }
            }
        }
    }

    async fn install_current_node(&mut self) {
        if let View::NodeDetail(name) = &self.view {
            if let Some(client) = &self.daemon_client {
                match client.execute_command(name, "install").await {
                    Ok(_) => self.add_message(format!("Installed: {}", name), MessageType::Success),
                    Err(e) => self.add_message(format!("Error: {}", e), MessageType::Error),
                }
            }
        }
    }

    async fn uninstall_current_node_service(&mut self) {
        if let View::NodeDetail(name) = &self.view {
            if let Some(client) = &self.daemon_client {
                match client.execute_command(name, "uninstall").await {
                    Ok(_) => self.add_message(format!("Disabled: {}", name), MessageType::Success),
                    Err(e) => self.add_message(format!("Error: {}", e), MessageType::Error),
                }
            }
        }
    }

    async fn uninstall_current_node(&mut self) {
        if let View::NodeDetail(name) = &self.view {
            let name = name.clone();
            if let Some(client) = &self.daemon_client {
                let _ = client.execute_command(&name, "uninstall").await;
                match client.execute_command(&name, "remove").await {
                    Ok(_) => {
                        self.add_message(format!("Uninstalled: {}", name), MessageType::Success);
                        self.view = View::Nodes(NodesTab::Installed);
                    }
                    Err(e) => self.add_message(format!("Error: {}", e), MessageType::Error),
                }
            }
        }
    }

    async fn add_discovered_node(&mut self) {
        if let Some(node) = self.discoverable_nodes.get(self.discover_index).cloned() {
            let path = node.path.clone();
            if let Some(client) = &self.daemon_client {
                match client.add_node(&path).await {
                    Ok(_) => {
                        self.add_message(format!("Added: {}", node.name), MessageType::Success);

                        let new_node = NodeInfo {
                            name: node.name.clone(),
                            path: node.path.clone(),
                            version: node.version.clone(),
                            node_type: node.node_type.clone(),
                            description: String::new(),
                            status: "stopped".to_string(),
                            is_built: false,
                        };
                        self.nodes.push(new_node);

                        self.discoverable_nodes.retain(|n| n.path != path);

                        self.view = View::Nodes(NodesTab::Installed);
                        self.node_index = self.nodes.len().saturating_sub(1);

                        let _ = self.refresh_status().await;
                    }
                    Err(e) => self.add_message(format!("Error: {}", e), MessageType::Error),
                }
            }
        }
    }

    async fn enable_source(&mut self) {
        if let Some(source) = self.sources.get(self.source_index) {
            if !source.enabled {
                self.registry.toggle_source(&source.path);
                self.add_message(format!("Enabled: {}", source.name), MessageType::Success);
                self.refresh_sources();
            }
        }
    }

    async fn disable_source(&mut self) {
        if let Some(source) = self.sources.get(self.source_index) {
            if source.enabled {
                self.registry.toggle_source(&source.path);
                self.add_message(format!("Disabled: {}", source.name), MessageType::Success);
                self.refresh_sources();
            }
        }
    }

    async fn remove_source(&mut self) {
        if let Some(source) = self.sources.get(self.source_index) {
            let name = source.name.clone();
            self.registry.remove_source(&source.path);
            self.add_message(format!("Removed: {}", name), MessageType::Success);
            if self.source_index > 0 {
                self.source_index -= 1;
            }
            self.refresh_sources();
        }
    }

    fn handle_edit_source_key(&mut self, key: KeyEvent) -> bool {
        match key.code {
            KeyCode::Esc => {
                self.input_mode = InputMode::Normal;
                self.marketplace_name.clear();
                self.marketplace_path.clear();
                self.marketplace_active_field = 0;
                self.marketplace_edit_path = None;
            }
            KeyCode::Tab => {
                self.marketplace_active_field = if self.marketplace_active_field == 0 {
                    1
                } else {
                    0
                };
            }
            KeyCode::Enter => {
                self.save_marketplace_source();
            }
            KeyCode::Backspace => {
                if self.marketplace_active_field == 0 {
                    if !self.marketplace_name.is_empty() {
                        self.marketplace_name.pop();
                    }
                } else if !self.marketplace_path.is_empty() {
                    self.marketplace_path.pop();
                }
            }
            KeyCode::Char(c) => {
                if self.marketplace_active_field == 0 {
                    self.marketplace_name.push(c);
                } else {
                    self.marketplace_path.push(c);
                }
            }
            _ => {}
        }
        false
    }

    fn save_marketplace_source(&mut self) {
        let name = self.marketplace_name.trim().to_string();
        let path = self.marketplace_path.trim().to_string();

        if name.is_empty() {
            self.add_message("Error: Name cannot be empty".into(), MessageType::Error);
            return;
        }
        if path.is_empty() {
            self.add_message("Error: Path cannot be empty".into(), MessageType::Error);
            return;
        }

        if path.contains("github.com") || path.starts_with("git@") || path.ends_with(".git") {
            self.add_message(
                "Git sources coming soon! Use local paths for now.".into(),
                MessageType::Warning,
            );
            return;
        }

        let expanded_path = if path.starts_with('~') {
            if let Some(home) = dirs::home_dir() {
                path.replacen('~', &home.to_string_lossy(), 1)
            } else {
                path.clone()
            }
        } else {
            path.clone()
        };

        let result = if let Some(ref original_path) = self.marketplace_edit_path {
            self.registry
                .update_source(original_path, &name, &expanded_path)
        } else {
            self.registry.add_source(&name, &expanded_path, "local")
        };

        match result {
            Ok(()) => {
                let action = if self.marketplace_edit_path.is_some() {
                    "Updated"
                } else {
                    "Added"
                };
                self.add_message(format!("{}: {}", action, name), MessageType::Success);
                self.input_mode = InputMode::Normal;
                self.marketplace_name.clear();
                self.marketplace_path.clear();
                self.marketplace_active_field = 0;
                self.marketplace_edit_path = None;
                self.refresh_sources();
            }
            Err(e) => {
                self.add_message(format!("Error: {}", e), MessageType::Error);
            }
        }
    }

    fn handle_create_node_key(&mut self, key: KeyEvent) -> bool {
        match key.code {
            KeyCode::Esc => {
                // Cancel form
                self.input_mode = InputMode::Normal;
                self.create_node_name.clear();
                self.create_node_description.clear();
                self.create_node_active_field = 0;
            }
            KeyCode::Tab => {
                // Cycle fields: name -> type -> description -> name
                self.create_node_active_field = (self.create_node_active_field + 1) % 3;
            }
            KeyCode::BackTab => {
                // Reverse cycle
                self.create_node_active_field = if self.create_node_active_field == 0 {
                    2
                } else {
                    self.create_node_active_field - 1
                };
            }
            KeyCode::Enter => {
                if self.create_node_active_field == 1 {
                    // Toggle type when on type field
                    self.create_node_type = if self.create_node_type == 0 { 1 } else { 0 };
                } else {
                    // Submit form
                    self.submit_create_node_form();
                }
            }
            KeyCode::Left | KeyCode::Right if self.create_node_active_field == 1 => {
                // Toggle type with arrows
                self.create_node_type = if self.create_node_type == 0 { 1 } else { 0 };
            }
            KeyCode::Backspace => match self.create_node_active_field {
                0 => {
                    self.create_node_name.pop();
                }
                2 => {
                    self.create_node_description.pop();
                }
                _ => {}
            },
            KeyCode::Char(c) => match self.create_node_active_field {
                0 => self.create_node_name.push(c),
                2 => self.create_node_description.push(c),
                _ => {}
            },
            _ => {}
        }
        false
    }

    fn submit_create_node_form(&mut self) {
        let name = self.create_node_name.trim().to_string();
        if name.is_empty() {
            self.add_message("Error: Name cannot be empty".into(), MessageType::Error);
            return;
        }

        let node_type = if self.create_node_type == 0 {
            "rust"
        } else {
            "python"
        };
        let description = if self.create_node_description.is_empty() {
            format!("A {} node", node_type)
        } else {
            self.create_node_description.clone()
        };

        // Default output path: ~/.bubbaloop/plugins/<name>
        let home = dirs::home_dir().unwrap_or_default();
        let output_path = home.join(".bubbaloop").join("plugins").join(&name);

        // Create node using templates module (synchronous)
        match templates::create_node_at(&name, node_type, "Anonymous", &description, &output_path) {
            Ok(created_path) => {
                self.add_message(format!("Created: {}", name), MessageType::Success);

                // Add to local nodes list
                let new_node = NodeInfo {
                    name: name.clone(),
                    path: created_path.to_string_lossy().to_string(),
                    version: "0.1.0".to_string(),
                    node_type: node_type.to_string(),
                    description: description.clone(),
                    status: "stopped".to_string(),
                    is_built: false,
                };
                self.nodes.push(new_node);

                // Switch to installed tab
                self.view = View::Nodes(NodesTab::Installed);
                self.input_mode = InputMode::Normal;
                self.create_node_name.clear();
                self.create_node_description.clear();
                self.node_index = self.nodes.len().saturating_sub(1);

                // Store pending path for async daemon registration
                self.pending_node_path = Some(created_path.to_string_lossy().to_string());
            }
            Err(e) => {
                self.add_message(format!("Error: {}", e), MessageType::Error);
            }
        }
    }
}
