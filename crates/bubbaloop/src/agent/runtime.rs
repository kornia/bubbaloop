//! Multi-agent runtime — manages agent instances inside the daemon process.
//!
//! The runtime subscribes to a shared Zenoh inbox topic, routes messages
//! to agent instances, and each agent publishes responses on its outbox topic.

use crate::agent::dispatch::Dispatcher;
use crate::agent::gateway::{self, AgentEvent, AgentManifest, AgentMessage};
use crate::agent::heartbeat::{ArousalSource, ArousalState, HeartbeatState};
use crate::agent::memory::Memory;
use crate::agent::provider::claude::ClaudeProvider;
use crate::agent::provider::ollama::OllamaProvider;
use crate::agent::provider::ModelProvider;
use crate::agent::soul::Soul;
use crate::agent::{run_agent_turn, AgentTurnInput, EventSink};
use crate::daemon::belief_updater::spawn_belief_decay_task;
use crate::daemon::context_provider::{spawn_provider, ProviderStore};
use crate::daemon::mission::{watch_missions_dir, Mission, MissionStatus, MissionStore};
use crate::daemon::reactive::{
    evaluate_rules_fired, total_boost, FiredRule, ReactiveRule, ReactiveRuleStore,
};
use crate::daemon::registry::get_bubbaloop_home;
use crate::mcp::platform::DaemonPlatform;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::Arc;
use tokio::sync::{mpsc, Notify, RwLock};

/// Bounded inbox channel capacity per agent.
const AGENT_INBOX_CAPACITY: usize = 32;

// ── Provider dispatch ────────────────────────────────────────────

/// Enum dispatch for model providers (avoids `dyn` — RPITIT traits aren't object-safe).
enum AnyProvider {
    Claude(ClaudeProvider),
    Ollama(OllamaProvider),
}

impl ModelProvider for AnyProvider {
    async fn generate(
        &self,
        system: Option<&str>,
        messages: &[crate::agent::provider::Message],
        tools: &[crate::agent::provider::ToolDefinition],
    ) -> crate::agent::provider::Result<crate::agent::provider::ModelResponse> {
        match self {
            Self::Claude(p) => p.generate(system, messages, tools).await,
            Self::Ollama(p) => p.generate(system, messages, tools).await,
        }
    }
}

// ── Config ───────────────────────────────────────────────────────

/// Multi-agent configuration from `~/.bubbaloop/agents.toml`.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AgentsConfig {
    /// Map of agent_id → agent config.
    #[serde(default)]
    pub agents: HashMap<String, AgentEntry>,
}

/// Per-agent configuration entry.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AgentEntry {
    /// Whether this agent is active.
    #[serde(default = "default_true")]
    pub enabled: bool,
    /// Whether this is the default agent for unaddressed messages.
    #[serde(default)]
    pub default: bool,
    /// Capability keywords for routing.
    #[serde(default)]
    pub capabilities: Vec<String>,
    /// Provider backend: "claude" (default) or "ollama".
    #[serde(default = "default_provider")]
    pub provider: String,
    /// Model name (e.g., "claude-sonnet-4-20250514", "llama3.2").
    /// Overrides Soul capabilities.toml model_name when set.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub model: Option<String>,
}

fn default_provider() -> String {
    "claude".to_string()
}

fn default_true() -> bool {
    true
}

impl AgentsConfig {
    /// Load from `~/.bubbaloop/agents.toml` or create a default single-agent config.
    pub fn load_or_default() -> Self {
        let path = get_bubbaloop_home().join("agents.toml");
        let mut config = match std::fs::read_to_string(&path) {
            Ok(content) => match toml::from_str(&content) {
                Ok(config) => config,
                Err(e) => {
                    log::warn!("Failed to parse agents.toml: {}, using default", e);
                    Self::single_default()
                }
            },
            Err(_) => {
                log::info!("No agents.toml found, using single default agent");
                Self::single_default()
            }
        };
        // Validate agent IDs — reject invalid ones to prevent path traversal
        config.filter_invalid_agent_ids();
        config
    }

    /// Validate all agent IDs: 1-64 chars, [a-zA-Z0-9_-], no path separators.
    /// Invalid IDs are removed from the config entirely.
    fn filter_invalid_agent_ids(&mut self) {
        self.agents.retain(|id, _| {
            let valid = !id.is_empty()
                && id.len() <= 64
                && !id.contains('/')
                && !id.contains('\\')
                && !id.contains("..")
                && id
                    .chars()
                    .all(|c| c.is_ascii_alphanumeric() || c == '-' || c == '_');
            if !valid {
                log::error!(
                    "Invalid agent ID '{}': must be 1-64 chars, [a-zA-Z0-9_-]. Removing from config.",
                    id
                );
            }
            valid
        });
    }

    /// Default: single agent named "jean-clawd".
    fn single_default() -> Self {
        let mut agents = HashMap::new();
        agents.insert(
            "jean-clawd".to_string(),
            AgentEntry {
                enabled: true,
                default: true,
                capabilities: vec![],
                provider: default_provider(),
                model: None,
            },
        );
        Self { agents }
    }

    /// Save config to `~/.bubbaloop/agents.toml`.
    pub fn save(&self) -> std::io::Result<()> {
        let path = get_bubbaloop_home().join("agents.toml");
        let content = toml::to_string_pretty(self)
            .map_err(|e| std::io::Error::new(std::io::ErrorKind::InvalidData, e))?;
        std::fs::write(&path, content)
    }

    /// Find the default agent ID.
    pub fn default_agent(&self) -> Option<&str> {
        self.agents
            .iter()
            .find(|(_, entry)| entry.enabled && entry.default)
            .map(|(id, _)| id.as_str())
    }
}

// ── ZenohSink ────────────────────────────────────────────────────

/// Publishes AgentEvents as JSON to a Zenoh outbox topic.
pub struct ZenohSink {
    publisher: zenoh::pubsub::Publisher<'static>,
}

impl ZenohSink {
    /// Create a new ZenohSink for the given outbox topic.
    pub async fn new(
        session: &Arc<zenoh::Session>,
        topic: &str,
    ) -> Result<Self, crate::agent::AgentError> {
        let owned_topic = topic.to_string();
        let publisher = session
            .declare_publisher(owned_topic)
            .await
            .map_err(|e| crate::agent::AgentError::Zenoh(e.to_string()))?;
        Ok(Self { publisher })
    }
}

impl EventSink for ZenohSink {
    async fn emit(&self, event: AgentEvent) {
        match serde_json::to_vec(&event) {
            Ok(bytes) => {
                if let Err(e) = self.publisher.put(bytes).await {
                    log::warn!("[Agent] Failed to publish event: {}", e);
                }
            }
            Err(e) => {
                log::warn!("[Agent] Failed to serialize event: {}", e);
            }
        }
    }
}

// ── Agent handle (used by router) ────────────────────────────────

/// Lightweight handle for sending messages to an agent.
struct AgentHandle {
    tx: mpsc::Sender<AgentMessage>,
    /// Stored for future capability-based routing.
    #[allow(dead_code)]
    capabilities: Vec<String>,
    is_default: bool,
}

// ── AgentRuntime ─────────────────────────────────────────────────

/// Manages all agent instances and routes inbox messages.
pub struct AgentRuntime {
    handles: HashMap<String, AgentHandle>,
}

impl AgentRuntime {
    /// Start the multi-agent runtime.
    ///
    /// Loads config, creates agent instances, subscribes to inbox,
    /// registers manifest queryables, and spawns per-agent tokio tasks.
    pub async fn start(
        session: Arc<zenoh::Session>,
        node_manager: Arc<crate::daemon::node_manager::NodeManager>,
        mut shutdown_rx: tokio::sync::watch::Receiver<()>,
        telemetry: Option<Arc<crate::daemon::telemetry::TelemetryService>>,
    ) -> Result<(), crate::agent::AgentError> {
        let config = AgentsConfig::load_or_default();
        let machine_id = crate::daemon::util::get_machine_id();

        // Load authentication token for inbox message validation
        let expected_token = match crate::mcp::auth::load_or_generate_token() {
            Ok(token) => token,
            Err(e) => {
                log::error!("[Runtime] Failed to load MCP token: {}", e);
                return Err(crate::agent::AgentError::Config(format!(
                    "Failed to load MCP token: {}",
                    e
                )));
            }
        };

        // Create shared platform for all agents.
        // `shutdown_rx` is forwarded so `configure_context` (via MCP) can spawn
        // context providers live and tie them to daemon shutdown.
        let platform = Arc::new(DaemonPlatform::new(
            node_manager,
            session.clone(),
            machine_id.clone(),
            Some(shutdown_rx.clone()),
        ));

        let mut handles = HashMap::new();

        for (agent_id, entry) in &config.agents {
            if !entry.enabled {
                log::info!("[Runtime] Agent '{}' is disabled, skipping", agent_id);
                continue;
            }

            // Per-agent directory: ~/.bubbaloop/agents/{agent_id}/
            let agent_dir = agent_directory(agent_id);
            std::fs::create_dir_all(&agent_dir).ok();

            // Load per-agent Soul (falls back to global Soul)
            let soul_dir = agent_dir.join("soul");
            let soul = if soul_dir.join("identity.md").exists() {
                Soul::load_from_dir(&soul_dir)
            } else {
                Soul::load_or_default()
            };
            let soul = Arc::new(RwLock::new(soul));

            // Initialize provider based on config (claude or ollama)
            // Prefer agents.toml model over Soul capabilities.toml model_name
            let model_name = match &entry.model {
                Some(m) => m.clone(),
                None => soul.read().await.capabilities.model_name.clone(),
            };
            let provider: AnyProvider = match entry.provider.as_str() {
                "ollama" => match OllamaProvider::from_env(Some(&model_name)) {
                    Ok(p) => AnyProvider::Ollama(p),
                    Err(e) => {
                        log::error!(
                            "[Runtime] Agent '{}' failed to init Ollama provider: {}",
                            agent_id,
                            e
                        );
                        continue;
                    }
                },
                _ => match ClaudeProvider::from_env(Some(&model_name)) {
                    Ok(p) => AnyProvider::Claude(p),
                    Err(e) => {
                        log::error!(
                            "[Runtime] Agent '{}' failed to init Claude provider: {}",
                            agent_id,
                            e
                        );
                        continue;
                    }
                },
            };

            // Open per-agent memory
            let memory = match Memory::open(&agent_dir) {
                Ok(m) => m,
                Err(e) => {
                    log::error!(
                        "[Runtime] Agent '{}' failed to open memory: {}",
                        agent_id,
                        e
                    );
                    continue;
                }
            };
            {
                let retention = soul.read().await.capabilities.episodic_log_retention_days;
                memory.startup_cleanup(retention).await;
            }

            // Spawn belief confidence decay — runs hourly, multiplies confidence by 0.9.
            // Uses the same memory.db that Memory::open created.
            let belief_db_path = agent_dir.join("memory.db");
            let belief_decay_shutdown = shutdown_rx.clone();
            tokio::spawn(spawn_belief_decay_task(
                belief_db_path,
                0.9,
                3600,
                belief_decay_shutdown,
            ));

            // Spawn context providers — load from providers.db, subscribe to Zenoh topics,
            // write extracted values to world_state (no LLM involved).
            // Created by configure_context MCP tool.
            let providers_db_path = agent_dir.join("providers.db");
            if providers_db_path.exists() {
                match ProviderStore::open(&providers_db_path) {
                    Ok(store) => match store.list_providers() {
                        Ok(providers) => {
                            log::info!(
                                "[Runtime] Agent '{}': spawning {} context provider(s)",
                                agent_id,
                                providers.len()
                            );
                            for cfg in providers {
                                let semantic_db = agent_dir.join("memory.db");
                                let provider_session = session.clone();
                                let provider_shutdown = shutdown_rx.clone();
                                spawn_provider(
                                    cfg,
                                    provider_session,
                                    semantic_db,
                                    provider_shutdown,
                                );
                            }
                        }
                        Err(e) => log::warn!(
                            "[Runtime] Agent '{}': failed to list providers: {}",
                            agent_id,
                            e
                        ),
                    },
                    Err(e) => log::warn!(
                        "[Runtime] Agent '{}': failed to open ProviderStore: {}",
                        agent_id,
                        e
                    ),
                }
            }

            // Create outbox sink
            let outbox = gateway::outbox_topic(&machine_id, agent_id);
            let sink = match ZenohSink::new(&session, &outbox).await {
                Ok(s) => s,
                Err(e) => {
                    log::error!(
                        "[Runtime] Agent '{}' failed to create outbox publisher: {}",
                        agent_id,
                        e
                    );
                    continue;
                }
            };

            // Create job notify and dispatcher with memory backend
            let job_notify = Arc::new(Notify::new());
            let decay = soul.read().await.capabilities.episodic_decay_half_life_days;
            let dispatcher = {
                let d = Dispatcher::new_with_memory(
                    platform.clone(),
                    machine_id.clone(),
                    agent_id.clone(),
                    memory.backend.clone(),
                    Some(job_notify.clone()),
                    decay,
                );
                if let Some(ref telem) = telemetry {
                    d.with_telemetry(telem.clone())
                } else {
                    d
                }
            };

            // Create inbox channel
            let (tx, rx) = mpsc::channel(AGENT_INBOX_CAPACITY);
            handles.insert(
                agent_id.clone(),
                AgentHandle {
                    tx,
                    capabilities: entry.capabilities.clone(),
                    is_default: entry.default,
                },
            );

            // Register manifest queryable
            let manifest = AgentManifest {
                agent_id: agent_id.clone(),
                name: soul.read().await.name().to_string(),
                capabilities: entry.capabilities.clone(),
                model: model_name.clone(),
                is_default: entry.default,
                machine_id: machine_id.clone(),
            };
            let manifest_topic = gateway::manifest_topic(&machine_id, agent_id);
            let manifest_json = serde_json::to_vec(&manifest).unwrap_or_default();
            let manifest_session = session.clone();
            tokio::spawn(async move {
                match manifest_session.declare_queryable(&manifest_topic).await {
                    Ok(queryable) => {
                        while let Ok(query) = queryable.recv_async().await {
                            let _ = query.reply(&manifest_topic, manifest_json.clone()).await;
                        }
                    }
                    Err(e) => {
                        log::warn!(
                            "[Runtime] Failed to register manifest for {}: {}",
                            manifest_topic,
                            e
                        );
                    }
                }
            });

            // Subscribe to this agent's inbox topic so other agents can send messages.
            // Messages are appended to episodic memory and surface in the next prompt turn.
            let inbox_topic = format!("bubbaloop/global/agent/{}/inbox", agent_id);
            let inbox_session = session.clone();
            let inbox_backend = memory.backend.clone();
            let mut inbox_shutdown = shutdown_rx.clone();
            let inbox_agent_id = agent_id.clone();
            tokio::spawn(async move {
                match inbox_session.declare_subscriber(&inbox_topic).await {
                    Ok(sub) => {
                        log::info!(
                            "[Runtime] Agent '{}' inbox: {}",
                            inbox_agent_id,
                            inbox_topic
                        );
                        loop {
                            tokio::select! {
                                Ok(sample) = sub.recv_async() => {
                                    let bytes = sample.payload().to_bytes();
                                    let payload = String::from_utf8_lossy(&bytes).into_owned();
                                    let (sender, message) = if let Ok(v) =
                                        serde_json::from_str::<serde_json::Value>(&payload)
                                    {
                                        let s = v["sender"].as_str().unwrap_or("unknown").to_owned();
                                        let m = v["message"].as_str().unwrap_or(&payload).to_owned();
                                        (s, m)
                                    } else {
                                        ("unknown-agent".to_owned(), payload)
                                    };
                                    let entry = crate::agent::memory::episodic::EpisodicLog::make_entry(
                                        "system",
                                        &format!("[agent_message from {}] {}", sender, message),
                                        None,
                                    );
                                    let backend = inbox_backend.lock().await;
                                    if let Err(e) = backend.episodic.append(&entry) {
                                        log::warn!("[Runtime] Failed to persist inbox message: {}", e);
                                    }
                                }
                                _ = inbox_shutdown.changed() => break,
                            }
                        }
                    }
                    Err(e) => log::warn!(
                        "[Runtime] Agent '{}' could not subscribe to inbox: {}",
                        inbox_agent_id,
                        e
                    ),
                }
            });

            // Spawn per-agent task
            let agent_id_clone = agent_id.clone();
            let soul_watcher_shutdown = shutdown_rx.clone();
            let agent_shutdown = shutdown_rx.clone();
            let soul_clone = soul.clone();

            // Start soul watcher for this agent
            let soul_dir_clone = soul_dir.clone();
            let soul_for_watcher = soul.clone();
            tokio::spawn(async move {
                let watch_dir = if soul_dir_clone.join("identity.md").exists() {
                    Some(soul_dir_clone)
                } else {
                    None // Falls back to global soul_directory()
                };
                crate::agent::soul::soul_watcher(
                    soul_for_watcher,
                    soul_watcher_shutdown,
                    watch_dir,
                )
                .await;
            });

            // Spawn mission file watcher — polls agent_dir/missions/ every 5s.
            // New/changed .md files are upserted into MissionStore as Active.
            // The filename stem becomes the mission ID: "patrol.md" → ID "patrol".
            let missions_dir = agent_dir.join("missions");
            std::fs::create_dir_all(&missions_dir).ok();
            let missions_db_path = agent_dir.join("missions.db");
            let (mission_tx, mut mission_rx) = tokio::sync::mpsc::channel::<String>(16);
            let mission_watcher_shutdown = shutdown_rx.clone();
            tokio::spawn(watch_missions_dir(
                missions_dir.clone(),
                mission_watcher_shutdown,
                mission_tx,
            ));

            // Consume mission IDs and upsert into MissionStore
            tokio::spawn(async move {
                while let Some(mission_id) = mission_rx.recv().await {
                    let md_path = missions_dir.join(format!("{}.md", mission_id));
                    let markdown = match std::fs::read_to_string(&md_path) {
                        Ok(s) => s,
                        Err(e) => {
                            log::warn!("[MissionWatcher] Failed to read {}.md: {}", mission_id, e);
                            continue;
                        }
                    };
                    let store = match MissionStore::open(&missions_db_path) {
                        Ok(s) => s,
                        Err(e) => {
                            log::error!("[MissionWatcher] Failed to open MissionStore: {}", e);
                            continue;
                        }
                    };
                    let compiled_at = std::time::SystemTime::now()
                        .duration_since(std::time::UNIX_EPOCH)
                        .unwrap_or_default()
                        .as_secs() as i64;
                    let mission = Mission {
                        id: mission_id.clone(),
                        markdown,
                        status: MissionStatus::Active,
                        expires_at: None,
                        resources: vec![],
                        sub_mission_ids: vec![],
                        depends_on: vec![],
                        compiled_at,
                    };
                    if let Err(e) = store.save_mission(&mission) {
                        log::error!(
                            "[MissionWatcher] Failed to save mission '{}': {}",
                            mission_id,
                            e
                        );
                    } else {
                        log::info!("[MissionWatcher] Loaded mission '{}'", mission_id);
                    }
                }
            });

            // First-run onboarding: triggered by a marker file placed by `agent setup`.
            // The marker is removed once the agent writes its own identity.md.
            let onboarding_marker = agent_dir.join(".needs-onboarding");
            let identity_path = soul_dir.join("identity.md");

            tokio::spawn(agent_loop(
                agent_id_clone,
                provider,
                dispatcher,
                memory,
                soul_clone,
                rx,
                sink,
                agent_shutdown,
                job_notify,
                identity_path,
                onboarding_marker,
            ));

            log::info!(
                "[Runtime] Agent '{}' ready (default={})",
                agent_id,
                entry.default
            );
        }

        if handles.is_empty() {
            log::warn!("[Runtime] No agents started — check agents.toml config");
            return Ok(());
        }

        let runtime = AgentRuntime { handles };

        // Subscribe to shared inbox
        let inbox = gateway::inbox_topic(&machine_id);
        let subscriber = session
            .declare_subscriber(&inbox)
            .await
            .map_err(|e| crate::agent::AgentError::Zenoh(e.to_string()))?;

        log::info!(
            "[Runtime] Agent runtime started: {} agent(s), inbox={}",
            runtime.handles.len(),
            inbox
        );

        // Main routing loop
        loop {
            tokio::select! {
                Ok(sample) = subscriber.recv_async() => {
                    let payload = sample.payload().to_bytes().to_vec();
                    match serde_json::from_slice::<AgentMessage>(&payload) {
                        Ok(msg) => {
                            // Validate auth token before routing
                            let token_valid = match &msg.auth_token {
                                Some(token) => crate::mcp::auth::validate_token(token, &expected_token),
                                None => false,
                            };
                            if !token_valid {
                                log::warn!(
                                    "[Runtime] Rejected inbox message id={}: missing or invalid auth token",
                                    msg.id
                                );
                                continue;
                            }
                            log::info!(
                                "[Runtime] Inbox message: id={}, agent={:?}, text_len={}",
                                msg.id, msg.agent, msg.text.len()
                            );
                            runtime.route(msg).await;
                        }
                        Err(e) => {
                            log::warn!("[Runtime] Invalid inbox message: {}", e);
                        }
                    }
                }
                _ = shutdown_rx.changed() => {
                    log::info!("[Runtime] Agent runtime shutting down");
                    break;
                }
            }
        }

        Ok(())
    }

    /// Route a message to the appropriate agent.
    ///
    /// Priority:
    /// 1. Explicit `agent` field in message → direct route
    /// 2. Default agent (fallback)
    async fn route(&self, msg: AgentMessage) {
        // 1. Try explicit agent target
        if let Some(target) = &msg.agent {
            if let Some(handle) = self.handles.get(target) {
                if handle.tx.try_send(msg.clone()).is_err() {
                    log::warn!("[Runtime] Agent '{}' inbox full, dropping message", target);
                }
                return;
            }
            log::warn!(
                "[Runtime] Message targets unknown agent '{}', falling back to default",
                target
            );
        }

        // 2. Route to default agent
        for (id, handle) in &self.handles {
            if handle.is_default {
                if handle.tx.try_send(msg).is_err() {
                    log::warn!("[Runtime] Default agent '{}' inbox full, dropping", id);
                }
                return;
            }
        }

        log::warn!("[Runtime] No default agent configured, message dropped");
    }
}

/// Per-agent event loop: processes inbox messages and heartbeat ticks.
#[allow(clippy::too_many_arguments)]
async fn agent_loop(
    agent_id: String,
    provider: AnyProvider,
    dispatcher: Dispatcher<DaemonPlatform>,
    mut memory: Memory,
    soul: Arc<RwLock<Soul>>,
    mut inbox_rx: mpsc::Receiver<AgentMessage>,
    sink: ZenohSink,
    mut shutdown_rx: tokio::sync::watch::Receiver<()>,
    job_notify: Arc<Notify>,
    identity_path: std::path::PathBuf,
    onboarding_marker: std::path::PathBuf,
) {
    let initial_caps = soul.read().await.capabilities.clone();
    let mut arousal = ArousalState::new(&initial_caps);

    // Phase 3: load reactive rules for arousal integration.
    let alerts_db_path = agent_directory(&agent_id).join("alerts.db");
    let mut reactive_rules: Vec<ReactiveRule> = ReactiveRuleStore::open(&alerts_db_path)
        .and_then(|s| s.list_rules())
        .map(|configs| configs.into_iter().map(Into::into).collect())
        .unwrap_or_default();
    let mut tick_count: u64 = 0;
    if !reactive_rules.is_empty() {
        log::info!(
            "[Agent:{}] Loaded {} reactive rules",
            agent_id,
            reactive_rules.len()
        );
    }

    // Rate limiting: minimum 2 seconds between LLM turns to prevent abuse.
    const MIN_TURN_INTERVAL: std::time::Duration = std::time::Duration::from_secs(2);
    let mut last_turn_time: Option<tokio::time::Instant> = None;

    // Reactive-alert debounce: even if rules keep firing, only wake the LLM
    // every `REACTIVE_TURN_MIN_INTERVAL`. The per-rule `debounce_secs` already
    // throttles individual rules; this second gate stops many rules from
    // stacking LLM turns when several alerts go hot together.
    const REACTIVE_TURN_MIN_INTERVAL: std::time::Duration = std::time::Duration::from_secs(30);
    let mut last_reactive_turn_time: Option<tokio::time::Instant> = None;

    // Cache onboarding state in memory — avoids a syscall on every inbox message.
    // True only while the marker exists and identity.md hasn't been written yet.
    let mut needs_onboarding = onboarding_marker.exists() && !identity_path.exists();
    let identity_path_str = identity_path.to_string_lossy().to_string();

    log::info!("[Agent:{}] Event loop started", agent_id);

    loop {
        let interval = std::time::Duration::from_secs(arousal.interval_secs());

        // Select on inbox, job notify, heartbeat, or shutdown.
        // Both job_notify and heartbeat lead to job polling after the select.
        let mut poll_jobs = false;

        tokio::select! {
            // Inbox message
            Some(msg) = inbox_rx.recv() => {
                // Rate limiting: enforce minimum interval between LLM turns
                if let Some(last) = last_turn_time {
                    let elapsed = last.elapsed();
                    if elapsed < MIN_TURN_INTERVAL {
                        let wait = MIN_TURN_INTERVAL - elapsed;
                        log::warn!(
                            "[Agent:{}] Rate limited: {}ms since last turn, waiting {}ms",
                            agent_id, elapsed.as_millis(), wait.as_millis()
                        );
                        tokio::time::sleep(wait).await;
                    }
                }

                arousal.spike(ArousalSource::UserInput);
                let soul_snapshot = soul.read().await.clone();

                // First-run onboarding: pass soul path until agent writes identity.md.
                let onboarding_path = if needs_onboarding {
                    if identity_path.exists() {
                        // Agent just wrote identity.md — clear marker and stop onboarding.
                        let _ = std::fs::remove_file(&onboarding_marker);
                        needs_onboarding = false;
                        None
                    } else {
                        Some(identity_path_str.as_str())
                    }
                } else {
                    None
                };

                last_turn_time = Some(tokio::time::Instant::now());
                if let Err(e) = run_agent_turn(
                    &provider,
                    &dispatcher,
                    &mut memory,
                    &soul_snapshot,
                    &sink,
                    &AgentTurnInput {
                        user_input: Some(&msg.text),
                        job_id: None,
                        correlation_id: &msg.id,
                        soul_path: onboarding_path,
                    },
                ).await {
                    log::error!("[Agent:{}] Turn failed: {}", agent_id, e);
                    // Sanitize error before sending over Zenoh outbox:
                    // truncate to avoid leaking verbose API response details.
                    let err_msg = sanitize_outbox_error(&e.to_string());
                    sink.emit(AgentEvent::error(&msg.id, &err_msg)).await;
                    sink.emit(AgentEvent::done(&msg.id)).await;
                }
            }

            // Job notify — a schedule_task was just created, check jobs immediately
            _ = job_notify.notified() => {
                log::info!("[Agent:{}] Job notify received, checking pending jobs", agent_id);
                arousal.spike(ArousalSource::PendingJobFired);
                poll_jobs = true;
            }

            // Heartbeat tick (job polling)
            _ = tokio::time::sleep(interval) => {
                poll_jobs = true;
            }

            // Shutdown
            _ = shutdown_rx.changed() => {
                log::info!("[Agent:{}] Shutting down", agent_id);
                break;
            }
        }

        // Poll pending jobs (triggered by heartbeat tick or job_notify)
        if poll_jobs {
            let jobs = {
                let backend = memory.backend.lock().await;
                backend.semantic.pending_jobs().unwrap_or_default()
            };
            let has_jobs = !jobs.is_empty();

            let state = if has_jobs {
                HeartbeatState {
                    event_count: jobs.len(),
                    has_changes: true,
                    sources: vec![ArousalSource::PendingJobFired],
                }
            } else {
                HeartbeatState::default()
            };
            arousal.update(&state);

            // Phase 3: evaluate reactive rules against world state.
            tick_count += 1;
            if tick_count.is_multiple_of(10) {
                if let Ok(store) = ReactiveRuleStore::open(&alerts_db_path) {
                    if let Ok(configs) = store.list_rules() {
                        reactive_rules = configs.into_iter().map(Into::into).collect();
                    }
                }
            }
            // Phase 3: evaluate reactive rules. Any rules that fire both boost
            // arousal (shrinks heartbeat interval) and — debounced — trigger an
            // autonomous agent turn so the LLM actually reacts, not just ticks.
            let mut fired_this_tick: Vec<FiredRule> = Vec::new();
            if !reactive_rules.is_empty() {
                let ws_entries = {
                    let backend = memory.backend.lock().await;
                    backend.semantic.world_state_snapshot().unwrap_or_default()
                };
                let ws_map: HashMap<&str, &str> = ws_entries
                    .iter()
                    .map(|e| (e.key.as_str(), e.value.as_str()))
                    .collect();
                if !ws_map.is_empty() {
                    fired_this_tick = evaluate_rules_fired(&reactive_rules, &ws_map);
                    let boost = total_boost(&fired_this_tick);
                    if boost > 0.0 {
                        arousal.add_external_boost(boost);
                        log::info!(
                            "[Agent:{}] Reactive alert boost: {:.2} ({} rule(s) fired)",
                            agent_id,
                            boost,
                            fired_this_tick.len()
                        );
                    }
                }
            }

            // If rules fired and the reactive-turn debounce allows it, wake the
            // LLM with a synthesized prompt that names the fired rules. This is
            // the only place reactive alerts become real LLM activity; without
            // it arousal just shrinks the interval without ever calling the model.
            let reactive_debounce_ok = match last_reactive_turn_time {
                None => true,
                Some(t) => t.elapsed() >= REACTIVE_TURN_MIN_INTERVAL,
            };
            if !fired_this_tick.is_empty() && reactive_debounce_ok {
                // Rate-limit against any other turn on this agent.
                if let Some(last) = last_turn_time {
                    let elapsed = last.elapsed();
                    if elapsed < MIN_TURN_INTERVAL {
                        tokio::time::sleep(MIN_TURN_INTERVAL - elapsed).await;
                    }
                }

                let prompt = build_reactive_prompt(&fired_this_tick);
                let cid = uuid::Uuid::new_v4().to_string();
                let soul_snapshot = soul.read().await.clone();
                last_turn_time = Some(tokio::time::Instant::now());

                log::info!(
                    "[Agent:{}] Reactive turn triggered (cid={}, rules={})",
                    agent_id,
                    cid,
                    fired_this_tick.len()
                );

                let reactive_result = run_agent_turn(
                    &provider,
                    &dispatcher,
                    &mut memory,
                    &soul_snapshot,
                    &sink,
                    &AgentTurnInput {
                        user_input: Some(&prompt),
                        job_id: None,
                        correlation_id: &cid,
                        soul_path: None, // Reactive turns never trigger onboarding
                    },
                )
                .await;

                // Debounce counts from turn COMPLETION, not start. Setting it
                // before would let fast-retrying heartbeat ticks (arousal shrinks
                // the interval to 5s) fire another turn immediately after a
                // 120s-timed-out turn, causing a cascade of failing turns.
                last_reactive_turn_time = Some(tokio::time::Instant::now());

                if let Err(e) = reactive_result {
                    log::error!("[Agent:{}] Reactive turn failed: {}", agent_id, e);
                    let err_msg = sanitize_outbox_error(&e.to_string());
                    sink.emit(AgentEvent::error(&cid, &err_msg)).await;
                    sink.emit(AgentEvent::done(&cid)).await;
                }
            } else if !fired_this_tick.is_empty() {
                log::debug!(
                    "[Agent:{}] Reactive turn suppressed by debounce ({}s)",
                    agent_id,
                    REACTIVE_TURN_MIN_INTERVAL.as_secs()
                );
            }

            for job in &jobs {
                // Rate limiting for job turns
                if let Some(last) = last_turn_time {
                    let elapsed = last.elapsed();
                    if elapsed < MIN_TURN_INTERVAL {
                        tokio::time::sleep(MIN_TURN_INTERVAL - elapsed).await;
                    }
                }

                let soul_snapshot = soul.read().await.clone();
                {
                    let backend = memory.backend.lock().await;
                    if let Err(e) = backend.semantic.start_job(&job.id) {
                        log::warn!(
                            "[Agent:{}] Failed to mark job {} as started: {}",
                            agent_id,
                            job.id,
                            e
                        );
                    }
                }
                let cid = uuid::Uuid::new_v4().to_string();

                last_turn_time = Some(tokio::time::Instant::now());
                if let Err(e) = run_agent_turn(
                    &provider,
                    &dispatcher,
                    &mut memory,
                    &soul_snapshot,
                    &sink,
                    &AgentTurnInput {
                        user_input: Some(&job.prompt_payload),
                        job_id: Some(&job.id),
                        correlation_id: &cid,
                        soul_path: None, // Jobs don't trigger onboarding
                    },
                )
                .await
                {
                    log::error!("[Agent:{}] Job {} failed: {}", agent_id, job.id, e);
                    let max_retries = soul_snapshot.capabilities.max_retries;
                    let backend = memory.backend.lock().await;
                    if let Err(fe) = backend
                        .semantic
                        .fail_job(&job.id, &e.to_string(), max_retries)
                    {
                        log::warn!(
                            "[Agent:{}] Failed to mark job {} as failed: {}",
                            agent_id,
                            job.id,
                            fe
                        );
                    }
                } else {
                    let next_run: Option<i64> = if job.recurrence {
                        job.cron_schedule
                            .as_deref()
                            .and_then(|cron| {
                                crate::agent::scheduler::next_run_after(
                                    cron,
                                    crate::agent::scheduler::now_epoch_secs(),
                                )
                                .ok()
                            })
                            .map(|ts| ts as i64)
                    } else {
                        None
                    };
                    let backend = memory.backend.lock().await;
                    if let Err(e) = backend.semantic.complete_job(&job.id, next_run) {
                        log::warn!(
                            "[Agent:{}] Failed to mark job {} as complete: {}",
                            agent_id,
                            job.id,
                            e
                        );
                    }
                }
                memory.clear_short_term();
            }
        }
    }
}

/// Build the prompt text for a reactive-alert-triggered turn.
///
/// The agent receives this as `user_input`, framed so the LLM understands it's
/// being woken by a background rule rather than a human message. The prompt
/// lists every rule that just fired along with its predicate and description,
/// so the model has enough grounding to decide whether (and how) to react.
fn build_reactive_prompt(fired: &[FiredRule]) -> String {
    let mut out = String::from(
        "[reactive alert] One or more reactive rules just fired against world state. \
         No human is waiting on this turn — you were woken by the rule engine. \
         Use list_world_state to inspect current values, reason about whether the \
         situation needs action, and take action only if the rule's intent requires it. \
         If nothing needs doing, acknowledge briefly and stand down.\n\n\
         Fired rules:\n",
    );
    for r in fired {
        let desc = if r.description.is_empty() {
            "(no description)"
        } else {
            r.description.as_str()
        };
        out.push_str(&format!(
            "- {} [mission={}] predicate=`{}` boost={:.1} — {}\n",
            r.id, r.mission_id, r.predicate, r.boost, desc
        ));
    }
    out
}

/// Sanitize an error message before sending it over the Zenoh outbox.
///
/// Truncates to a maximum length and strips content that might contain
/// verbose API request/response details to avoid leaking sensitive data.
fn sanitize_outbox_error(msg: &str) -> String {
    const MAX_LEN: usize = 200;
    let sanitized: String = msg
        .chars()
        .filter(|c| !c.is_control() || *c == '\n')
        .take(MAX_LEN)
        .collect();
    if msg.len() > MAX_LEN {
        format!("{}... (truncated)", sanitized)
    } else {
        sanitized
    }
}

/// Return the per-agent directory path.
pub fn agent_directory(agent_id: &str) -> PathBuf {
    get_bubbaloop_home().join("agents").join(agent_id)
}

/// Public entry point for starting the agent runtime from the daemon.
pub async fn run_agent_runtime(
    session: Arc<zenoh::Session>,
    node_manager: Arc<crate::daemon::node_manager::NodeManager>,
    shutdown_rx: tokio::sync::watch::Receiver<()>,
    telemetry: Option<Arc<crate::daemon::telemetry::TelemetryService>>,
) -> crate::agent::Result<()> {
    AgentRuntime::start(session, node_manager, shutdown_rx, telemetry).await
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn agents_config_default_has_jean_clawd() {
        let config = AgentsConfig::single_default();
        assert!(config.agents.contains_key("jean-clawd"));
        assert!(config.agents["jean-clawd"].enabled);
        assert!(config.agents["jean-clawd"].default);
    }

    #[test]
    fn agents_config_default_agent() {
        let config = AgentsConfig::single_default();
        assert_eq!(config.default_agent(), Some("jean-clawd"));
    }

    #[test]
    fn agents_config_parse_toml() {
        let toml_str = r#"
[agents.jean-clawd]
enabled = true
default = true

[agents.camera-expert]
enabled = true
capabilities = ["camera", "rtsp", "video"]
"#;
        let config: AgentsConfig = toml::from_str(toml_str).unwrap();
        assert_eq!(config.agents.len(), 2);
        assert!(config.agents["jean-clawd"].default);
        assert!(!config.agents["camera-expert"].default);
        assert_eq!(
            config.agents["camera-expert"].capabilities,
            vec!["camera", "rtsp", "video"]
        );
    }

    #[test]
    fn agents_config_no_default_returns_none() {
        let toml_str = r#"
[agents.agent1]
enabled = true
"#;
        let config: AgentsConfig = toml::from_str(toml_str).unwrap();
        assert_eq!(config.default_agent(), None);
    }

    #[test]
    fn agents_config_disabled_not_default() {
        let toml_str = r#"
[agents.agent1]
enabled = false
default = true
"#;
        let config: AgentsConfig = toml::from_str(toml_str).unwrap();
        assert_eq!(config.default_agent(), None);
    }

    #[test]
    fn agent_directory_path() {
        let dir = agent_directory("jean-clawd");
        assert!(dir
            .to_string_lossy()
            .contains(".bubbaloop/agents/jean-clawd"));
    }

    #[test]
    fn agents_config_provider_defaults_to_claude() {
        let toml_str = r#"
[agents.agent1]
enabled = true
"#;
        let config: AgentsConfig = toml::from_str(toml_str).unwrap();
        assert_eq!(config.agents["agent1"].provider, "claude");
    }

    #[test]
    fn agents_config_provider_ollama() {
        let toml_str = r#"
[agents.local-agent]
enabled = true
provider = "ollama"
"#;
        let config: AgentsConfig = toml::from_str(toml_str).unwrap();
        assert_eq!(config.agents["local-agent"].provider, "ollama");
    }

    #[test]
    fn sanitize_outbox_error_truncates() {
        let long_msg = "x".repeat(300);
        let result = sanitize_outbox_error(&long_msg);
        assert!(result.len() < 250);
        assert!(result.ends_with("(truncated)"));
    }

    #[test]
    fn sanitize_outbox_error_strips_control_chars() {
        let msg = "error\x00with\x01control";
        let result = sanitize_outbox_error(msg);
        assert!(!result.contains('\x00'));
        assert!(!result.contains('\x01'));
    }

    #[test]
    fn sanitize_outbox_error_preserves_short_messages() {
        let msg = "simple error";
        assert_eq!(sanitize_outbox_error(msg), "simple error");
    }

    #[test]
    fn build_reactive_prompt_lists_rules() {
        let fired = vec![
            FiredRule {
                id: "r1".to_string(),
                mission_id: "patrol".to_string(),
                predicate: "motion.level > 0.05".to_string(),
                description: "Motion detected on terrace".to_string(),
                boost: 3.0,
            },
            FiredRule {
                id: "r2".to_string(),
                mission_id: "patrol".to_string(),
                predicate: "dog.near_stairs = 'true'".to_string(),
                description: String::new(),
                boost: 2.5,
            },
        ];
        let prompt = build_reactive_prompt(&fired);
        assert!(prompt.contains("[reactive alert]"));
        assert!(prompt.contains("list_world_state"));
        assert!(prompt.contains("r1"));
        assert!(prompt.contains("Motion detected on terrace"));
        assert!(prompt.contains("r2"));
        assert!(prompt.contains("(no description)"));
        assert!(prompt.contains("motion.level > 0.05"));
        assert!(prompt.contains("boost=3.0"));
    }

    #[test]
    fn build_reactive_prompt_empty_is_well_formed() {
        let prompt = build_reactive_prompt(&[]);
        assert!(prompt.contains("[reactive alert]"));
        assert!(prompt.contains("Fired rules:\n"));
    }
}
