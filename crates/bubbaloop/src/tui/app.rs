use std::sync::Arc;
use std::time::{Duration, Instant};

use anyhow::Result;
use crossterm::event::{KeyCode, KeyEvent};
use tokio::sync::mpsc;

use crate::templates;
use crate::tui::config::Registry;
use crate::tui::daemon::{DaemonClient, DaemonSubscription};

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
    Nodes,
    Instances,
    Marketplace,
}

/// What build-related activity is in progress
#[derive(Debug, Clone, PartialEq)]
pub enum BuildActivity {
    Idle,
    Building,
    Cleaning,
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
    pub build_output: Vec<String>,
    pub base_node: String,
    pub config_override: String,
}

/// Discoverable node
#[derive(Debug, Clone)]
pub struct DiscoverableNode {
    pub path: String,
    pub name: String,
    pub version: String,
    pub node_type: String,
    pub description: String,
    pub source: String,
    pub source_type: String, // "builtin", "local", "git"
    pub is_added: bool,
    pub is_built: bool,
    pub instance_count: usize,
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
    Command,        // Typing a /command
    EditSource,     // Editing marketplace source
    CreateNode,     // Creating a new node form
    CreateInstance, // Creating a new instance from discover tab
    EditConfig,     // Editing config path for an instance
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
    /// Base nodes (base_node field is empty) — derived from `nodes` via `split_nodes()`
    pub base_nodes: Vec<NodeInfo>,
    /// Instance nodes (base_node field is non-empty) — derived from `nodes` via `split_nodes()`
    pub instances: Vec<NodeInfo>,
    pub instance_index: usize,
    pub discoverable_nodes: Vec<DiscoverableNode>,
    pub discover_index: usize,
    pub sources: Vec<MarketplaceSource>,
    pub source_index: usize,

    /// Daemon client
    pub daemon_client: Option<DaemonClient>,
    pub daemon_available: bool,

    /// Background subscription for real-time node updates
    daemon_subscription: Option<Arc<DaemonSubscription>>,

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
    pub build_activity: BuildActivity,
    /// Name of the node the current build/clean targets
    pub build_activity_node: String,
    /// When the build/clean activity started (for timeout fallback)
    build_started_at: Instant,
    /// Whether the daemon has confirmed the build/clean (status became "building")
    build_confirmed: bool,

    /// Pending start/stop commands: (node_name, expected_status, started_at)
    pending_commands: Vec<(String, String, Instant)>,

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

    /// Create instance form state
    pub instance_base_node: String,
    pub instance_base_path: String,
    pub instance_name: String,
    pub instance_config_path: String,
    pub instance_active_field: usize, // 0 = name, 1 = config

    /// Edit config form state
    pub edit_config_node: String,
    pub edit_config_path: String,

    /// Channel for background tasks to send messages back to the UI
    message_tx: mpsc::UnboundedSender<(String, MessageType)>,
    message_rx: mpsc::UnboundedReceiver<(String, MessageType)>,
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

        // Fetch official nodes registry from GitHub (non-blocking, cached)
        registry.refresh_builtin_cache();

        // Try to connect to daemon
        let (daemon_client, daemon_available) = match DaemonClient::new().await {
            Ok(client) => {
                let available = client.is_available().await;
                (Some(client), available)
            }
            Err(_) => (None, false),
        };

        // HYBRID STARTUP:
        // 1. Get initial nodes via query (immediate data for UI)
        let initial_nodes = match &daemon_client {
            Some(client) => match client.list_nodes().await {
                Ok(mut nodes) => {
                    nodes.sort_by(|a, b| a.name.cmp(&b.name));
                    nodes
                }
                Err(e) => {
                    log::debug!("Initial node query failed: {}", e);
                    Vec::new()
                }
            },
            None => Vec::new(),
        };

        // 2. Start subscription for real-time updates (background)
        let daemon_subscription = match &daemon_client {
            Some(client) => {
                let client_arc = Arc::new(client.clone());
                match DaemonSubscription::start(client_arc).await {
                    Ok(sub) => Some(Arc::new(sub)),
                    Err(e) => {
                        log::debug!("Failed to start subscription: {}", e);
                        None
                    }
                }
            }
            None => None,
        };

        let now = Instant::now();
        let (message_tx, message_rx) = mpsc::unbounded_channel();

        let mut app = Self {
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
            nodes: initial_nodes,
            node_index: 0,
            base_nodes: Vec::new(),
            instances: Vec::new(),
            instance_index: 0,
            discoverable_nodes: Vec::new(),
            discover_index: 0,
            sources: Vec::new(),
            source_index: 0,
            daemon_client,
            daemon_available,
            daemon_subscription,
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
            build_activity: BuildActivity::Idle,
            build_started_at: now,
            build_confirmed: false,
            build_activity_node: String::new(),
            pending_commands: Vec::new(),
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
            instance_base_node: String::new(),
            instance_base_path: String::new(),
            instance_name: String::new(),
            instance_config_path: String::new(),
            instance_active_field: 0,
            edit_config_node: String::new(),
            edit_config_path: String::new(),
            message_tx,
            message_rx,
        };
        app.split_nodes();
        app
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
                self.reset_confirmations();
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
                self.systemctl_service("start").await;
            }
            KeyCode::Char('x') => {
                self.systemctl_service("stop").await;
            }
            KeyCode::Char('r') => {
                self.systemctl_service("restart").await;
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

        if self.input_mode == InputMode::CreateInstance {
            return self.handle_create_instance_key(key);
        }

        if self.input_mode == InputMode::EditSource {
            return self.handle_edit_source_key(key);
        }

        if self.input_mode == InputMode::EditConfig {
            return self.handle_edit_config_key(key);
        }

        let current_tab = if let View::Nodes(tab) = &self.view {
            tab.clone()
        } else {
            return false;
        };

        match key.code {
            KeyCode::Esc | KeyCode::Char('q') => {
                self.reset_confirmations();
                self.view = View::Home;
                self.messages.clear();
            }
            KeyCode::Tab => {
                let next = match current_tab {
                    NodesTab::Nodes => NodesTab::Instances,
                    NodesTab::Instances => NodesTab::Marketplace,
                    NodesTab::Marketplace => NodesTab::Nodes,
                };
                self.view = View::Nodes(next.clone());
                self.refresh_tab_data(&next);
                self.reset_confirmations();
            }
            KeyCode::BackTab => {
                let prev = match current_tab {
                    NodesTab::Nodes => NodesTab::Marketplace,
                    NodesTab::Instances => NodesTab::Nodes,
                    NodesTab::Marketplace => NodesTab::Instances,
                };
                self.view = View::Nodes(prev.clone());
                self.refresh_tab_data(&prev);
                self.reset_confirmations();
            }
            KeyCode::Char('1') => {
                self.view = View::Nodes(NodesTab::Nodes);
                self.refresh_tab_data(&NodesTab::Nodes);
                self.reset_confirmations();
            }
            KeyCode::Char('2') => {
                self.view = View::Nodes(NodesTab::Instances);
                self.refresh_tab_data(&NodesTab::Instances);
                self.reset_confirmations();
            }
            KeyCode::Char('3') => {
                self.view = View::Nodes(NodesTab::Marketplace);
                self.refresh_tab_data(&NodesTab::Marketplace);
                self.reset_confirmations();
            }
            _ => match current_tab {
                NodesTab::Nodes => self.handle_nodes_tab_key(key).await,
                NodesTab::Instances => self.handle_instances_tab_key(key).await,
                NodesTab::Marketplace => self.handle_marketplace_tab_key(key).await,
            },
        }
        false
    }

    async fn handle_nodes_tab_key(&mut self, key: KeyEvent) {
        match key.code {
            KeyCode::Up | KeyCode::Char('k') => {
                if self.discover_index > 0 {
                    self.discover_index -= 1;
                }
                self.confirm_uninstall = false;
                self.confirm_clean = false;
            }
            KeyCode::Down | KeyCode::Char('j') => {
                if self.discover_index < self.discoverable_nodes.len().saturating_sub(1) {
                    self.discover_index += 1;
                }
                self.confirm_uninstall = false;
                self.confirm_clean = false;
            }
            KeyCode::Enter => {
                if let Some(node) = self.discoverable_nodes.get(self.discover_index) {
                    if node.is_added && node.is_built {
                        // Open create instance form
                        self.input_mode = InputMode::CreateInstance;
                        self.instance_base_node = node.name.clone();
                        self.instance_base_path = node.path.clone();
                        self.instance_name = format!("{}-", node.name);
                        self.instance_config_path.clear();
                        self.instance_active_field = 0;
                    } else if node.is_added && !node.is_built {
                        self.add_message(
                            "Build node first (press [b])".into(),
                            MessageType::Warning,
                        );
                    } else {
                        // Add the node
                        self.add_discovered_node();
                    }
                }
            }
            KeyCode::Char('a') => {
                // Add node (only if not already added)
                if let Some(node) = self.discoverable_nodes.get(self.discover_index) {
                    if !node.is_added {
                        self.add_discovered_node();
                    }
                }
            }
            KeyCode::Char('s') | KeyCode::Char(' ') => {
                // Start/stop (only for added nodes)
                if let Some(node) = self.discoverable_nodes.get(self.discover_index) {
                    if node.is_added {
                        if let Some(base) = self.base_nodes.iter().find(|n| n.name == node.name) {
                            let name = base.name.clone();
                            let status = base.status.clone();
                            let is_built = base.is_built;
                            if status == "running" {
                                self.stop_node(&name);
                            } else if is_built {
                                self.start_node(&name);
                            } else {
                                self.add_message(
                                    "Build first (press [b])".into(),
                                    MessageType::Warning,
                                );
                            }
                        }
                    }
                }
            }
            KeyCode::Char('b') => {
                // Build (only for added nodes)
                if let Some(node) = self.discoverable_nodes.get(self.discover_index) {
                    if node.is_added {
                        if let Some(base) = self.base_nodes.iter().find(|n| n.name == node.name) {
                            if !self.is_node_busy(base) {
                                let name = base.name.clone();
                                self.start_build_activity_for(
                                    &name,
                                    BuildActivity::Building,
                                    "build",
                                );
                            }
                        }
                    }
                }
            }
            KeyCode::Char('c') => {
                // Clean (only for added nodes)
                if let Some(node) = self.discoverable_nodes.get(self.discover_index) {
                    if node.is_added {
                        if let Some(base) = self.base_nodes.iter().find(|n| n.name == node.name) {
                            if !self.is_node_busy(base) {
                                if self.confirm_clean {
                                    let name = base.name.clone();
                                    self.start_build_activity_for(
                                        &name,
                                        BuildActivity::Cleaning,
                                        "clean",
                                    );
                                    self.confirm_clean = false;
                                } else {
                                    self.confirm_clean = true;
                                    self.add_message(
                                        "Press [c] again to confirm clean".into(),
                                        MessageType::Warning,
                                    );
                                }
                            }
                        }
                    }
                }
            }
            KeyCode::Char('u') => {
                // Uninstall (only for added nodes)
                if let Some(node) = self.discoverable_nodes.get(self.discover_index) {
                    if node.is_added {
                        if self.confirm_uninstall {
                            // Find the base node index for uninstall
                            if let Some(idx) =
                                self.base_nodes.iter().position(|n| n.name == node.name)
                            {
                                self.node_index = idx;
                                self.uninstall_selected_node();
                            }
                            self.confirm_uninstall = false;
                        } else {
                            self.confirm_uninstall = true;
                            self.add_message(
                                "Press [u] again to confirm uninstall".into(),
                                MessageType::Warning,
                            );
                        }
                    }
                }
            }
            KeyCode::Char('i') => {
                // Create instance — only when node is added AND built
                if let Some(node) = self.discoverable_nodes.get(self.discover_index) {
                    if node.is_added && node.is_built {
                        self.input_mode = InputMode::CreateInstance;
                        self.instance_base_node = node.name.clone();
                        self.instance_base_path = node.path.clone();
                        self.instance_name = format!("{}-", node.name);
                        self.instance_config_path.clear();
                        self.instance_active_field = 0;
                    } else if node.is_added && !node.is_built {
                        self.add_message(
                            "Build node first (press [b])".into(),
                            MessageType::Warning,
                        );
                    } else {
                        self.add_message("Add node first (press [a])".into(), MessageType::Warning);
                    }
                }
            }
            KeyCode::Char('n') => {
                // Enter create node form
                self.input_mode = InputMode::CreateNode;
                self.create_node_name.clear();
                self.create_node_type = 0;
                self.create_node_description.clear();
                self.create_node_active_field = 0;
            }
            KeyCode::Char('l') => {
                // View logs (only for added nodes)
                if let Some(node) = self.discoverable_nodes.get(self.discover_index) {
                    if node.is_added {
                        if let Some(idx) = self.base_nodes.iter().position(|n| n.name == node.name)
                        {
                            self.node_index = idx;
                            let name = node.name.clone();
                            self.view = View::NodeLogs(name.clone());
                            self.fetch_node_logs(&name).await;
                        }
                    }
                }
            }
            _ => {}
        }
    }

    async fn handle_instances_tab_key(&mut self, key: KeyEvent) {
        match key.code {
            KeyCode::Up | KeyCode::Char('k') => {
                if self.instance_index > 0 {
                    self.instance_index -= 1;
                }
            }
            KeyCode::Down | KeyCode::Char('j') => {
                if self.instance_index < self.instances.len().saturating_sub(1) {
                    self.instance_index += 1;
                }
            }
            KeyCode::Enter => {
                if let Some(node) = self.instances.get(self.instance_index) {
                    let node_name = node.name.clone();
                    self.view = View::NodeDetail(node_name.clone());
                    self.fetch_service_status(&node_name).await;
                }
            }
            KeyCode::Char('s') | KeyCode::Char(' ') => {
                self.toggle_instance_node();
            }
            KeyCode::Char('e') => {
                if let Some(node) = self.instances.get(self.instance_index) {
                    let name = node.name.clone();
                    self.send_node_command(&name, "install", "stopped");
                    self.add_message(format!("Installing {}...", name), MessageType::Info);
                }
            }
            KeyCode::Char('d') => {
                if let Some(node) = self.instances.get(self.instance_index) {
                    let name = node.name.clone();
                    self.send_node_command(&name, "uninstall", "not-installed");
                    self.add_message(format!("Disabling {}...", name), MessageType::Info);
                }
            }
            KeyCode::Char('l') => {
                if let Some(node) = self.instances.get(self.instance_index) {
                    let name = node.name.clone();
                    self.view = View::NodeLogs(name.clone());
                    self.fetch_node_logs(&name).await;
                }
            }
            KeyCode::Char('r') => {
                // Remove instance (uninstall + remove)
                if let Some(node) = self.instances.get(self.instance_index) {
                    if let Some(client) = &self.daemon_client {
                        let client = client.clone();
                        let name = node.name.clone();
                        let tx = self.message_tx.clone();
                        self.add_message(
                            format!("Removing instance {}...", name),
                            MessageType::Info,
                        );
                        tokio::spawn(async move {
                            if let Err(e) = client.send_command(&name, "uninstall").await {
                                let _ = tx.send((format!("Error: {}", e), MessageType::Error));
                                return;
                            }
                            if let Err(e) = client.send_command(&name, "remove").await {
                                let _ = tx.send((format!("Error: {}", e), MessageType::Error));
                            }
                        });
                    }
                }
            }
            KeyCode::Char('c') => {
                // Edit config path for selected instance
                if let Some(node) = self.instances.get(self.instance_index) {
                    self.edit_config_node = node.name.clone();
                    self.edit_config_path = node.config_override.clone();
                    self.input_mode = InputMode::EditConfig;
                }
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
                self.enable_source();
            }
            KeyCode::Char('d') => {
                self.disable_source();
            }
            KeyCode::Char('r') => {
                if self.confirm_remove {
                    self.remove_source();
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
                self.reset_confirmations();
                self.view = View::Nodes(NodesTab::Nodes);
            }
            KeyCode::Tab => {
                self.reset_confirmations();
                self.cycle_node_index(true);
                if let Some(node) = self.nodes.get(self.node_index) {
                    let name = node.name.clone();
                    self.view = View::NodeDetail(name.clone());
                    self.service_status_text.clear();
                    self.fetch_service_status(&name).await;
                }
            }
            KeyCode::BackTab => {
                self.reset_confirmations();
                self.cycle_node_index(false);
                if let Some(node) = self.nodes.get(self.node_index) {
                    let name = node.name.clone();
                    self.view = View::NodeDetail(name.clone());
                    self.service_status_text.clear();
                    self.fetch_service_status(&name).await;
                }
            }
            KeyCode::Char('s') if !self.current_node_busy() => {
                self.toggle_current_node();
            }
            KeyCode::Char('e') if !self.current_node_busy() => {
                self.send_current_node_daemon_command("install", "Installing");
            }
            KeyCode::Char('d') if !self.current_node_busy() => {
                self.send_current_node_daemon_command("uninstall", "Disabling");
            }
            KeyCode::Char('l') => {
                self.reset_confirmations();
                if let Some(name) = self.current_node_name().map(str::to_owned) {
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
                if let Some(name) = self.current_node_name().map(str::to_owned) {
                    self.view = View::NodeDetail(name);
                }
            }
            KeyCode::Tab => {
                self.reset_confirmations();
                self.cycle_node_index(true);
                self.open_logs_for_current_node().await;
            }
            KeyCode::BackTab => {
                self.reset_confirmations();
                self.cycle_node_index(false);
                self.open_logs_for_current_node().await;
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
                self.view = View::Nodes(NodesTab::Nodes);
                self.refresh_tab_data(&NodesTab::Nodes);
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

    /// Whether a specific node is busy (building/cleaning).
    pub fn is_node_busy(&self, node: &NodeInfo) -> bool {
        node.status == "building"
            || (self.build_activity != BuildActivity::Idle && self.build_activity_node == node.name)
    }

    /// Return the node name if the current view is NodeDetail or NodeLogs.
    fn current_node_name(&self) -> Option<&str> {
        match &self.view {
            View::NodeDetail(name) | View::NodeLogs(name) => Some(name),
            _ => None,
        }
    }

    /// Whether the currently viewed node is busy.
    fn current_node_busy(&self) -> bool {
        self.current_node_name()
            .and_then(|name| self.nodes.iter().find(|n| n.name == name))
            .map(|node| self.is_node_busy(node))
            .unwrap_or(false)
    }

    /// Advance or rewind node_index (wrapping), used by Tab/BackTab in detail and logs views.
    fn cycle_node_index(&mut self, forward: bool) {
        let len = self.nodes.len();
        if len == 0 {
            return;
        }
        if forward {
            self.node_index = (self.node_index + 1) % len;
        } else if self.node_index > 0 {
            self.node_index -= 1;
        } else {
            self.node_index = len - 1;
        }
    }

    /// Switch to NodeLogs view for the currently selected node.
    async fn open_logs_for_current_node(&mut self) {
        if let Some(node) = self.nodes.get(self.node_index) {
            let name = node.name.clone();
            self.view = View::NodeLogs(name.clone());
            self.fetch_node_logs(&name).await;
        }
    }

    fn reset_confirmations(&mut self) {
        self.confirm_remove = false;
        self.confirm_uninstall = false;
        self.confirm_clean = false;
    }

    pub async fn tick(&mut self) -> Result<()> {
        let now = Instant::now();
        self.check_exit_timeout();

        // Drain messages from background tasks
        while let Ok((msg, msg_type)) = self.message_rx.try_recv() {
            self.add_message(msg, msg_type);
        }

        // Animation updates only - no network I/O
        if now.duration_since(self.last_spinner_update) > Duration::from_millis(150) {
            self.spinner_frame = (self.spinner_frame + 1) % 7;
            self.last_spinner_update = now;
        }

        if now.duration_since(self.last_robot_blink) > Duration::from_millis(800) {
            self.robot_eyes_on = !self.robot_eyes_on;
            self.last_robot_blink = now;
        }

        // Get nodes from subscription (non-blocking read)
        // Subscription data OVERRIDES initial query data
        let mut nodes_updated = false;
        if let Some(sub) = self.daemon_subscription.clone() {
            let mut sub_nodes = sub.get_nodes().await;
            if !sub_nodes.is_empty() {
                sub_nodes.sort_by(|a, b| a.name.cmp(&b.name));
                nodes_updated = true;
                self.nodes = sub_nodes;
                self.split_nodes();
            }
            self.daemon_available = sub.is_connected().await;
        }
        if nodes_updated {
            self.refresh_discoverable_nodes();
        }

        // Reset build activity — scoped to the node that initiated the build.
        // Works regardless of which view the user is on (Bug 1 fix).
        if self.build_activity != BuildActivity::Idle {
            let elapsed = now.duration_since(self.build_started_at);
            let target = self.build_activity_node.clone();

            if let Some(node) = self.nodes.iter().find(|n| n.name == target) {
                if node.status == "building" {
                    self.build_confirmed = true;
                    // Stream live build output from subscription
                    if !node.build_output.is_empty() {
                        self.build_output = node.build_output.clone();
                    }
                } else if self.build_confirmed {
                    // Build/clean finished — grab final output, then reset
                    if !node.build_output.is_empty() {
                        self.build_output = node.build_output.clone();
                    }
                    self.build_activity = BuildActivity::Idle;
                    self.build_confirmed = false;
                } else if elapsed > Duration::from_secs(10) {
                    // Daemon never confirmed - command was probably lost
                    let label = if self.build_activity == BuildActivity::Cleaning {
                        "Clean"
                    } else {
                        "Build"
                    };
                    self.add_message(
                        format!("{} timed out — daemon did not respond", label),
                        MessageType::Warning,
                    );
                    self.build_activity = BuildActivity::Idle;
                    self.build_output.clear();
                }
            } else if elapsed > Duration::from_secs(10) {
                // Node disappeared entirely — reset
                self.add_message(
                    format!(
                        "Node '{}' disappeared during operation",
                        self.build_activity_node
                    ),
                    MessageType::Warning,
                );
                self.build_activity = BuildActivity::Idle;
                self.build_output.clear();
            }
        }

        // Check pending start/stop commands — drain confirmed or timed-out entries
        {
            let mut to_remove = Vec::new();
            let mut messages = Vec::new();
            for (i, (cmd_name, expected_status, started_at)) in
                self.pending_commands.iter().enumerate()
            {
                let elapsed = now.duration_since(*started_at);
                if let Some(node) = self.nodes.iter().find(|n| &n.name == cmd_name) {
                    if node.status == *expected_status {
                        let label = if expected_status == "running" {
                            "Started"
                        } else {
                            "Stopped"
                        };
                        messages.push(format!("{} {}", label, cmd_name));
                        to_remove.push(i);
                    } else if elapsed > Duration::from_secs(15) {
                        to_remove.push(i);
                    }
                } else if elapsed > Duration::from_secs(15) {
                    to_remove.push(i);
                }
            }
            for i in to_remove.into_iter().rev() {
                self.pending_commands.remove(i);
            }
            for msg in messages {
                self.add_message(msg, MessageType::Success);
            }
        }

        // Less frequent checks for non-node data (services, logs, etc.)
        if now.duration_since(self.last_refresh) > Duration::from_secs(5) {
            self.refresh_status_non_blocking().await?;
            self.last_refresh = now;
        }

        Ok(())
    }

    /// Non-blocking status refresh - excludes node list queries
    async fn refresh_status_non_blocking(&mut self) -> Result<()> {
        // Services check is local (systemctl), not network
        self.refresh_services().await;

        // Register pending node with daemon (best effort, don't block)
        if let Some(path) = self.pending_node_path.take() {
            if let Some(client) = &self.daemon_client {
                let client = client.clone();
                let tx = self.message_tx.clone();
                // Spawn in background so we don't block
                tokio::spawn(async move {
                    if let Err(e) = client.add_node(&path).await {
                        let _ = tx.send((
                            format!("Failed to register node: {}", e),
                            MessageType::Error,
                        ));
                    }
                });
            }
        }

        // View-specific refreshes (local operations)
        match self.view {
            View::Nodes(NodesTab::Nodes) => self.refresh_discoverable_nodes(),
            View::Nodes(NodesTab::Marketplace) => self.refresh_sources(),
            _ => {}
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
        let mut handles = Vec::new();
        for name in &names {
            let name = name.clone();
            handles.push(tokio::spawn(
                async move { check_service_status(&name).await },
            ));
        }
        for (i, handle) in handles.into_iter().enumerate() {
            if let Ok(status) = handle.await {
                if let Some(service) = self.services.get_mut(i) {
                    service.status = status;
                }
            }
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
                    log::warn!(
                        "journalctl failed: {}",
                        String::from_utf8_lossy(&out.stderr)
                    );
                    self.logs
                        .push("Error fetching logs (service may not exist)".to_string());
                }
            }
            Err(e) => {
                log::warn!("Failed to run journalctl: {}", e);
                self.logs
                    .push("Unable to fetch logs (journalctl unavailable)".to_string());
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

    /// Immediately refresh data for the given tab (avoids waiting for tick)
    fn refresh_tab_data(&mut self, tab: &NodesTab) {
        match tab {
            NodesTab::Nodes => self.refresh_discoverable_nodes(),
            NodesTab::Marketplace => self.refresh_sources(),
            NodesTab::Instances => {}
        }
    }

    fn refresh_discoverable_nodes(&mut self) {
        self.discoverable_nodes = self.registry.scan_discoverable_nodes(&self.nodes);
        if !self.discoverable_nodes.is_empty() {
            self.discover_index = self.discover_index.min(self.discoverable_nodes.len() - 1);
        } else {
            self.discover_index = 0;
        }
    }

    fn refresh_sources(&mut self) {
        self.sources = self.registry.get_sources();
        if !self.sources.is_empty() {
            self.source_index = self.source_index.min(self.sources.len() - 1);
        } else {
            self.source_index = 0;
        }
    }

    async fn systemctl_service(&mut self, action: &str) {
        if let Some(service) = self.services.get(self.service_index) {
            let name = service.name.clone();
            match tokio::process::Command::new("systemctl")
                .args(["--user", action, &name])
                .output()
                .await
            {
                Ok(output) if output.status.success() => {
                    let verb = capitalize(action);
                    self.add_message(format!("{}ing {}", verb, name), MessageType::Info);
                }
                Ok(output) => {
                    let stderr = String::from_utf8_lossy(&output.stderr);
                    self.add_message(
                        format!(
                            "Failed to {} {}: {}",
                            action,
                            name,
                            stderr.lines().next().unwrap_or("unknown error")
                        ),
                        MessageType::Error,
                    );
                }
                Err(e) => {
                    self.add_message(
                        format!("Failed to run systemctl: {}", e),
                        MessageType::Error,
                    );
                }
            }
        }
    }

    /// Split `self.nodes` into `base_nodes` (base_node is empty) and `instances` (non-empty).
    pub fn split_nodes(&mut self) {
        self.base_nodes = self
            .nodes
            .iter()
            .filter(|n| n.base_node.is_empty())
            .cloned()
            .collect();
        self.instances = self
            .nodes
            .iter()
            .filter(|n| !n.base_node.is_empty())
            .cloned()
            .collect();
        if !self.base_nodes.is_empty() {
            self.node_index = self.node_index.min(self.base_nodes.len() - 1);
        } else {
            self.node_index = 0;
        }
        if !self.instances.is_empty() {
            self.instance_index = self.instance_index.min(self.instances.len() - 1);
        } else {
            self.instance_index = 0;
        }
    }

    fn toggle_instance_node(&mut self) {
        let node_data = self
            .instances
            .get(self.instance_index)
            .map(|n| (n.name.clone(), n.status.clone(), n.is_built));

        if let Some((name, status, is_built)) = node_data {
            if status == "running" {
                self.stop_node(&name);
            } else if is_built {
                self.start_node(&name);
            } else {
                self.add_message("Build the base node first".into(), MessageType::Warning);
            }
        }
    }

    /// Send a start or stop command for a node.
    /// `command` is "start" or "stop"; `expected_status` is the status to wait for.
    fn send_node_command(&mut self, name: &str, command: &str, expected_status: &str) {
        if let Some(client) = &self.daemon_client {
            let client = client.clone();
            let name = name.to_string();
            let tx = self.message_tx.clone();
            let verb = if command == "start" {
                "Starting"
            } else {
                "Stopping"
            };
            self.add_message(format!("{}  {}...", verb, name), MessageType::Info);
            self.pending_commands
                .push((name.clone(), expected_status.to_string(), Instant::now()));
            let cmd = command.to_string();
            tokio::spawn(async move {
                if let Err(e) = client.send_command(&name, &cmd).await {
                    let _ = tx.send((format!("Error: {}", e), MessageType::Error));
                }
            });
        }
    }

    fn start_node(&mut self, name: &str) {
        self.send_node_command(name, "start", "running");
    }

    fn stop_node(&mut self, name: &str) {
        self.send_node_command(name, "stop", "stopped");
    }

    fn toggle_current_node(&mut self) {
        if let Some(name) = self.current_node_name().map(str::to_owned) {
            if let Some(node) = self.nodes.iter().find(|n| n.name == name) {
                if node.status == "running" {
                    self.stop_node(&name);
                } else if node.is_built {
                    self.start_node(&name);
                } else {
                    self.add_message("Cannot start: node not built".into(), MessageType::Warning);
                }
            }
        }
    }

    /// Kick off a build or clean for a given node by name.
    fn start_build_activity_for(&mut self, name: &str, activity: BuildActivity, command: &str) {
        if let Some(client) = &self.daemon_client {
            let client = client.clone();
            let name = name.to_string();
            let tx = self.message_tx.clone();

            let label = if command == "build" {
                "building"
            } else {
                "cleaning"
            };
            let is_running = self
                .nodes
                .iter()
                .find(|n| n.name == *name)
                .map(|n| n.status == "running")
                .unwrap_or(false);
            let msg = if is_running {
                format!("Stopping & {} {}...", label, name)
            } else {
                format!("{}  {}...", capitalize(label), name)
            };
            self.add_message(msg, MessageType::Info);
            self.build_activity = activity;
            self.build_activity_node = name.clone();
            self.build_started_at = Instant::now();
            self.build_confirmed = false;
            self.build_output.clear();

            let cmd = command.to_string();
            tokio::spawn(async move {
                if let Err(e) = client.send_command(&name, &cmd).await {
                    let _ = tx.send((format!("Error: {}", e), MessageType::Error));
                }
            });
        }
    }

    /// Send a daemon command for the node currently shown in detail view.
    fn send_current_node_daemon_command(&mut self, command: &str, msg_verb: &str) {
        if let Some(name) = self.current_node_name().map(str::to_owned) {
            if let Some(client) = &self.daemon_client {
                let client = client.clone();
                let tx = self.message_tx.clone();
                let cmd = command.to_string();
                self.add_message(format!("{} {}...", msg_verb, name), MessageType::Info);
                tokio::spawn(async move {
                    if let Err(e) = client.send_command(&name, &cmd).await {
                        let _ = tx.send((format!("Error: {}", e), MessageType::Error));
                    }
                });
            }
        }
    }

    fn uninstall_selected_node(&mut self) {
        if let Some(node) = self.base_nodes.get(self.node_index) {
            if let Some(client) = &self.daemon_client {
                let client = client.clone();
                let name = node.name.clone();
                let tx = self.message_tx.clone();
                self.add_message(format!("Uninstalling {}...", name), MessageType::Info);

                // Optimistic removal
                self.nodes.retain(|n| n.name != name);
                self.split_nodes();
                self.refresh_discoverable_nodes();

                tokio::spawn(async move {
                    if let Err(e) = client.send_command(&name, "uninstall").await {
                        let _ = tx.send((
                            format!("Error uninstalling {}: {}", name, e),
                            MessageType::Error,
                        ));
                        return;
                    }
                    if let Err(e) = client.send_command(&name, "remove").await {
                        let _ = tx.send((
                            format!("Error removing {}: {}", name, e),
                            MessageType::Error,
                        ));
                    }
                });
            }
        }
    }

    fn add_discovered_node(&mut self) {
        if let Some(node) = self.discoverable_nodes.get(self.discover_index).cloned() {
            let path = node.path.clone();
            let is_remote = path.contains("--subdir")
                && !path.starts_with('/')
                && !path.starts_with('.')
                && !path.starts_with('~');

            // For local nodes, we need a daemon client
            if !is_remote && self.daemon_client.is_none() {
                return;
            }

            let label = if is_remote { "Installing" } else { "Adding" };
            self.add_message(format!("{} {}...", label, node.name), MessageType::Info);

            // Optimistic local state update (shared by both paths)
            let new_node = NodeInfo {
                name: node.name.clone(),
                path: node.path.clone(),
                version: node.version.clone(),
                node_type: node.node_type.clone(),
                description: String::new(),
                status: "stopped".to_string(),
                is_built: false,
                build_output: Vec::new(),
                base_node: String::new(),
                config_override: String::new(),
            };
            self.nodes.push(new_node);
            self.nodes.sort_by(|a, b| a.name.cmp(&b.name));
            self.split_nodes();
            self.refresh_discoverable_nodes();
            // Stay on Nodes tab (discover_index already points at this node)

            let tx = self.message_tx.clone();
            let node_name = node.name.clone();

            if is_remote {
                // Parse expected "owner/repo --subdir name" format safely.
                // Only pass validated repo and subdir as explicit args to
                // prevent injection of extra CLI flags from a tampered cache.
                let parts: Vec<&str> = path.split_whitespace().collect();
                let (repo, subdir) = if parts.len() == 3 && parts[1] == "--subdir" {
                    (parts[0].to_string(), parts[2].to_string())
                } else {
                    let _ = tx.send((
                        format!("Invalid marketplace path for {}", node_name),
                        MessageType::Error,
                    ));
                    return;
                };

                // Validate repo format
                if crate::registry::validate_repo(&repo).is_err() {
                    let _ = tx.send((
                        format!("Invalid repo '{}' for {}", repo, node_name),
                        MessageType::Error,
                    ));
                    return;
                }

                tokio::spawn(async move {
                    let exe = std::env::current_exe().unwrap_or_else(|_| "bubbaloop".into());
                    let mut cmd = tokio::process::Command::new(exe);
                    cmd.args(["node", "add", &repo, "--subdir", &subdir]);

                    match cmd.output().await {
                        Ok(output) if output.status.success() => {
                            let _ =
                                tx.send((format!("Installed {}", node_name), MessageType::Success));
                        }
                        Ok(output) => {
                            let stderr = String::from_utf8_lossy(&output.stderr);
                            let msg = stderr.lines().next().unwrap_or("unknown error").to_string();
                            let _ = tx.send((
                                format!("Error installing {}: {}", node_name, msg),
                                MessageType::Error,
                            ));
                        }
                        Err(e) => {
                            let _ = tx.send((
                                format!("Error installing {}: {}", node_name, e),
                                MessageType::Error,
                            ));
                        }
                    }
                });
            } else {
                let client = self.daemon_client.clone().unwrap();
                tokio::spawn(async move {
                    if let Err(e) = client.send_add_node(&path).await {
                        let _ = tx.send((
                            format!("Error adding {}: {}", node_name, e),
                            MessageType::Error,
                        ));
                    }
                });
            }
        }
    }

    fn enable_source(&mut self) {
        if let Some(source) = self.sources.get(self.source_index) {
            if !source.enabled {
                self.registry.toggle_source(&source.path);
                self.add_message(format!("Enabled: {}", source.name), MessageType::Success);
                self.refresh_sources();
            }
        }
    }

    fn disable_source(&mut self) {
        if let Some(source) = self.sources.get(self.source_index) {
            if source.enabled {
                self.registry.toggle_source(&source.path);
                self.add_message(format!("Disabled: {}", source.name), MessageType::Success);
                self.refresh_sources();
            }
        }
    }

    fn remove_source(&mut self) {
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

    fn handle_create_instance_key(&mut self, key: KeyEvent) -> bool {
        match key.code {
            KeyCode::Esc => {
                self.input_mode = InputMode::Normal;
                self.instance_name.clear();
                self.instance_config_path.clear();
                self.instance_active_field = 0;
            }
            KeyCode::Tab => {
                self.instance_active_field = if self.instance_active_field == 0 {
                    1
                } else {
                    0
                };
            }
            KeyCode::BackTab => {
                self.instance_active_field = if self.instance_active_field == 0 {
                    1
                } else {
                    0
                };
            }
            KeyCode::Enter => {
                self.submit_create_instance_form();
            }
            KeyCode::Backspace => {
                if self.instance_active_field == 0 {
                    self.instance_name.pop();
                } else {
                    self.instance_config_path.pop();
                }
            }
            KeyCode::Char(c) => {
                if self.instance_active_field == 0 {
                    self.instance_name.push(c);
                } else {
                    self.instance_config_path.push(c);
                }
            }
            _ => {}
        }
        false
    }

    fn handle_edit_config_key(&mut self, key: KeyEvent) -> bool {
        match key.code {
            KeyCode::Esc => {
                self.input_mode = InputMode::Normal;
                self.edit_config_node.clear();
                self.edit_config_path.clear();
            }
            KeyCode::Enter => {
                self.submit_edit_config_form();
            }
            KeyCode::Backspace => {
                self.edit_config_path.pop();
            }
            KeyCode::Char(c) => {
                self.edit_config_path.push(c);
            }
            _ => {}
        }
        false
    }

    fn submit_edit_config_form(&mut self) {
        let config_path = self.edit_config_path.trim().to_string();
        let node_name = self.edit_config_node.clone();

        if config_path.is_empty() {
            self.add_message(
                "Error: Config path cannot be empty".into(),
                MessageType::Error,
            );
            return;
        }

        // Expand ~ in config path
        let expanded = if config_path.starts_with('~') {
            if let Some(home) = dirs::home_dir() {
                config_path.replacen('~', &home.to_string_lossy(), 1)
            } else {
                config_path.clone()
            }
        } else {
            config_path.clone()
        };

        // Verify file exists (warn, don't block)
        if !std::path::Path::new(&expanded).exists() {
            self.add_message(
                format!(
                    "Warning: '{}' does not exist yet, setting anyway",
                    config_path
                ),
                MessageType::Warning,
            );
        }

        // Send update-config command to daemon
        if let Some(client) = &self.daemon_client {
            let client = client.clone();
            let tx = self.message_tx.clone();
            let name = node_name.clone();
            let config = expanded;
            tokio::spawn(async move {
                if let Err(e) = client.send_update_config(&name, &config).await {
                    let _ = tx.send((
                        format!("Error updating config for {}: {}", name, e),
                        MessageType::Error,
                    ));
                }
            });
        }

        self.add_message(
            format!("Updating config for {}...", node_name),
            MessageType::Info,
        );

        self.input_mode = InputMode::Normal;
        self.edit_config_node.clear();
        self.edit_config_path.clear();
    }

    fn submit_create_instance_form(&mut self) {
        let name = self.instance_name.trim().to_string();
        if name.is_empty() {
            self.add_message("Error: Name cannot be empty".into(), MessageType::Error);
            return;
        }

        // Validate instance name
        if !name
            .chars()
            .all(|c| c.is_alphanumeric() || c == '-' || c == '_')
        {
            self.add_message(
                "Error: Name may only contain alphanumeric characters, hyphens, and underscores"
                    .into(),
                MessageType::Error,
            );
            return;
        }
        if name.starts_with('-') || name.starts_with('.') {
            self.add_message(
                "Error: Name must not start with '-' or '.'".into(),
                MessageType::Error,
            );
            return;
        }

        // Check for duplicates
        if self.nodes.iter().any(|n| n.name == name) {
            self.add_message(
                format!("Error: Node '{}' already exists", name),
                MessageType::Error,
            );
            return;
        }

        // Resolve the base node's path from registered nodes
        let base_path = self
            .nodes
            .iter()
            .find(|n| n.name == self.instance_base_node && n.base_node.is_empty())
            .map(|n| n.path.clone());

        let base_path = match base_path {
            Some(p) => p,
            None => {
                self.add_message(
                    format!(
                        "Error: Base node '{}' not found in registered nodes",
                        self.instance_base_node
                    ),
                    MessageType::Error,
                );
                return;
            }
        };

        // Expand ~ in config path
        let config = if self.instance_config_path.trim().is_empty() {
            None
        } else {
            let p = self.instance_config_path.trim().to_string();
            let expanded = if p.starts_with('~') {
                if let Some(home) = dirs::home_dir() {
                    p.replacen('~', &home.to_string_lossy(), 1)
                } else {
                    p
                }
            } else {
                p
            };
            Some(expanded)
        };

        self.add_message(format!("Creating instance {}...", name), MessageType::Info);

        // Optimistic local state update
        let new_node = NodeInfo {
            name: name.clone(),
            path: base_path.clone(),
            version: self
                .nodes
                .iter()
                .find(|n| n.name == self.instance_base_node)
                .map(|n| n.version.clone())
                .unwrap_or_else(|| "0.1.0".to_string()),
            node_type: self
                .nodes
                .iter()
                .find(|n| n.name == self.instance_base_node)
                .map(|n| n.node_type.clone())
                .unwrap_or_else(|| "rust".to_string()),
            description: format!("Instance of {}", self.instance_base_node),
            status: "stopped".to_string(),
            is_built: true, // Instances share the base binary
            build_output: Vec::new(),
            base_node: self.instance_base_node.clone(),
            config_override: config.clone().unwrap_or_default(),
        };
        self.nodes.push(new_node);
        self.nodes.sort_by(|a, b| a.name.cmp(&b.name));
        self.split_nodes();
        self.refresh_discoverable_nodes();

        // Switch to Instances tab
        self.view = View::Nodes(NodesTab::Instances);
        self.input_mode = InputMode::Normal;
        self.instance_index = self
            .instances
            .iter()
            .position(|n| n.name == name)
            .unwrap_or(0);

        // Send to daemon
        if let Some(client) = &self.daemon_client {
            let client = client.clone();
            let tx = self.message_tx.clone();
            let instance_name = name.clone();
            let path = base_path;
            let name_override = Some(name);
            let config_override = config;
            tokio::spawn(async move {
                if let Err(e) = client
                    .send_add_node_instance(&path, name_override, config_override)
                    .await
                {
                    let _ = tx.send((
                        format!("Error creating instance {}: {}", instance_name, e),
                        MessageType::Error,
                    ));
                }
            });
        }

        // Clear form state
        self.instance_name.clear();
        self.instance_config_path.clear();
        self.instance_active_field = 0;
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

        // Validate node name: only alphanumeric, hyphens, underscores allowed
        if !name
            .chars()
            .all(|c| c.is_alphanumeric() || c == '-' || c == '_')
        {
            self.add_message(
                "Error: Name may only contain alphanumeric characters, hyphens, and underscores"
                    .into(),
                MessageType::Error,
            );
            return;
        }
        if name.starts_with('-') || name.starts_with('.') {
            self.add_message(
                "Error: Name must not start with '-' or '.'".into(),
                MessageType::Error,
            );
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

        // Default output path: ~/.bubbaloop/nodes/<name>
        let home = dirs::home_dir().unwrap_or_default();
        let output_path = home.join(".bubbaloop").join("nodes").join(&name);

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
                    build_output: Vec::new(),
                    base_node: String::new(),
                    config_override: String::new(),
                };
                self.nodes.push(new_node);
                self.nodes.sort_by(|a, b| a.name.cmp(&b.name));
                self.split_nodes();

                // Switch to Nodes tab
                self.view = View::Nodes(NodesTab::Nodes);
                self.refresh_discoverable_nodes();
                self.input_mode = InputMode::Normal;
                self.create_node_name.clear();
                self.create_node_description.clear();

                // Store pending path for async daemon registration
                self.pending_node_path = Some(created_path.to_string_lossy().to_string());
            }
            Err(e) => {
                self.add_message(format!("Error: {}", e), MessageType::Error);
            }
        }
    }
}

/// Capitalize the first letter of a string (used for user-facing messages).
fn capitalize(s: &str) -> String {
    let mut chars = s.chars();
    match chars.next() {
        None => String::new(),
        Some(c) => c.to_uppercase().collect::<String>() + chars.as_str(),
    }
}

async fn check_service_status(service_name: &str) -> ServiceStatus {
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
