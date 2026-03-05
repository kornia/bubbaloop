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
use crate::daemon::registry::get_bubbaloop_home;
use crate::mcp::platform::DaemonPlatform;
use serde::Deserialize;
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
#[derive(Debug, Clone, Deserialize)]
pub struct AgentsConfig {
    /// Map of agent_id → agent config.
    #[serde(default)]
    pub agents: HashMap<String, AgentEntry>,
}

/// Per-agent configuration entry.
#[derive(Debug, Clone, Deserialize)]
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
            },
        );
        Self { agents }
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
        let scope = std::env::var("BUBBALOOP_SCOPE").unwrap_or_else(|_| "local".to_string());
        let machine_id = crate::daemon::util::get_machine_id();

        // Create shared platform for all agents
        let platform = Arc::new(DaemonPlatform::new(
            node_manager,
            session.clone(),
            scope.clone(),
            machine_id.clone(),
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
            std::fs::create_dir_all(&soul_dir).ok();
            let soul = if soul_dir.join("identity.md").exists() {
                Soul::load_from_dir(&soul_dir)
            } else {
                Soul::load_or_default()
            };
            let soul = Arc::new(RwLock::new(soul));

            // Initialize provider based on config (claude or ollama)
            let model_name = soul.read().await.capabilities.model_name.clone();
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

            // Create outbox sink
            let outbox = gateway::outbox_topic(&scope, &machine_id, agent_id);
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
                    scope.clone(),
                    machine_id.clone(),
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
            let manifest_topic = gateway::manifest_topic(&scope, &machine_id, agent_id);
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
        let inbox = gateway::inbox_topic(&scope, &machine_id);
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
) {
    let initial_caps = soul.read().await.capabilities.clone();
    let mut arousal = ArousalState::new(&initial_caps);

    log::info!("[Agent:{}] Event loop started", agent_id);

    loop {
        let interval = std::time::Duration::from_secs(arousal.interval_secs());

        // Select on inbox, job notify, heartbeat, or shutdown.
        // Both job_notify and heartbeat lead to job polling after the select.
        let mut poll_jobs = false;

        tokio::select! {
            // Inbox message
            Some(msg) = inbox_rx.recv() => {
                arousal.spike(ArousalSource::UserInput);
                let soul_snapshot = soul.read().await.clone();

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
                    },
                ).await {
                    log::error!("[Agent:{}] Turn failed: {}", agent_id, e);
                    sink.emit(AgentEvent::error(&msg.id, &e.to_string())).await;
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

            for job in &jobs {
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

/// Return the per-agent directory path.
fn agent_directory(agent_id: &str) -> PathBuf {
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
}
