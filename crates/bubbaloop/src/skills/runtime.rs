//! Skill runtime — manages built-in driver tasks.

use crate::skills::builtin::exec::ExecDriver;
use crate::skills::builtin::http_poll::HttpPollDriver;
use crate::skills::builtin::system::SystemDriver;
use crate::skills::builtin::tcp_listen::TcpListenDriver;
use crate::skills::builtin::webhook::WebhookDriver;
use crate::skills::builtin::{BuiltInContext, BuiltInDriver};
use crate::skills::{load_skills, resolve_driver, DriverKind, SkillConfig};
use std::collections::HashMap;
use std::path::Path;
use std::sync::Arc;
use tokio::sync::watch;
use tokio::task::JoinHandle;
use zenoh::Session;

/// Manages built-in skill tasks.
pub struct SkillRuntime {
    session: Arc<Session>,
    scope: String,
    machine_id: String,
    shutdown_rx: watch::Receiver<()>,
    /// Active task handles keyed by skill name.
    tasks: HashMap<String, JoinHandle<()>>,
}

impl SkillRuntime {
    pub fn new(
        session: Arc<Session>,
        scope: String,
        machine_id: String,
        shutdown_rx: watch::Receiver<()>,
    ) -> Self {
        Self {
            session,
            scope,
            machine_id,
            shutdown_rx,
            tasks: HashMap::new(),
        }
    }

    /// Start a built-in skill. No-op if the driver is not BuiltIn.
    pub fn start_skill(&mut self, skill: SkillConfig) {
        let entry = match resolve_driver(&skill.driver) {
            Some(e) if e.kind == DriverKind::BuiltIn => e,
            _ => return,
        };

        // Stop existing task if already running with this name
        if let Some(handle) = self.tasks.remove(&skill.name) {
            handle.abort();
        }

        let ctx = BuiltInContext {
            session: self.session.clone(),
            scope: self.scope.clone(),
            machine_id: self.machine_id.clone(),
            skill_name: skill.name.clone(),
            config: skill.config.clone(),
            shutdown_rx: self.shutdown_rx.clone(),
        };

        let name = skill.name.clone();
        let driver_name = entry.driver_name;
        let handle = match driver_name {
            "http-poll" => tokio::spawn(async move {
                if let Err(e) = HttpPollDriver.run(ctx).await {
                    log::error!("[SkillRuntime] http-poll '{}' error: {}", name, e);
                }
            }),
            "system" => tokio::spawn(async move {
                if let Err(e) = SystemDriver.run(ctx).await {
                    log::error!("[SkillRuntime] system '{}' error: {}", name, e);
                }
            }),
            "exec" => tokio::spawn(async move {
                if let Err(e) = ExecDriver.run(ctx).await {
                    log::error!("[SkillRuntime] exec '{}' error: {}", name, e);
                }
            }),
            "webhook" => tokio::spawn(async move {
                if let Err(e) = WebhookDriver.run(ctx).await {
                    log::error!("[SkillRuntime] webhook '{}' error: {}", name, e);
                }
            }),
            "tcp-listen" => tokio::spawn(async move {
                if let Err(e) = TcpListenDriver.run(ctx).await {
                    log::error!("[SkillRuntime] tcp-listen '{}' error: {}", name, e);
                }
            }),
            other => {
                log::warn!(
                    "[SkillRuntime] No implementation for BuiltIn driver '{}'",
                    other
                );
                return;
            }
        };

        log::info!(
            "[SkillRuntime] Started builtin skill '{}' (driver: {})",
            skill.name,
            driver_name
        );
        self.tasks.insert(skill.name, handle);
    }

    /// Stop a skill by name.
    pub fn stop_skill(&mut self, name: &str) {
        if let Some(handle) = self.tasks.remove(name) {
            handle.abort();
            log::info!("[SkillRuntime] Stopped skill '{}'", name);
        }
    }

    /// Abort all running skill tasks and clear the task map.
    pub fn shutdown(&mut self) {
        for (name, handle) in self.tasks.drain() {
            log::debug!("[SkillRuntime] Aborting task for '{}'", name);
            handle.abort();
        }
    }

    /// Watch the skills directory and hot-reload on YAML file changes.
    ///
    /// - YAML created or modified → (re)start the skill if it is enabled and BuiltIn.
    /// - YAML removed → stop the skill whose name matches the file stem.
    ///
    /// Blocks until `shutdown_rx` fires. Designed to run after `load_from_dir()`,
    /// concurrently with the already-spawned skill tasks.
    pub async fn watch_skills_dir(
        &mut self,
        skills_dir: &Path,
        mut shutdown_rx: watch::Receiver<()>,
    ) {
        use notify::event::RemoveKind;
        use notify::{Event, EventKind, RecursiveMode, Watcher};

        let (tx, mut rx) = tokio::sync::mpsc::channel::<Event>(32);

        let mut watcher = match notify::recommended_watcher(move |res: notify::Result<Event>| {
            if let Ok(event) = res {
                let _ = tx.blocking_send(event);
            }
        }) {
            Ok(w) => w,
            Err(e) => {
                log::warn!("[SkillRuntime] Hot-reload watcher failed to start: {}", e);
                let _ = shutdown_rx.changed().await;
                return;
            }
        };

        if let Err(e) = watcher.watch(skills_dir, RecursiveMode::NonRecursive) {
            log::warn!("[SkillRuntime] Failed to watch skills dir: {}", e);
            let _ = shutdown_rx.changed().await;
            return;
        }

        log::info!(
            "[SkillRuntime] Hot-reload watching {}",
            skills_dir.display()
        );

        loop {
            tokio::select! {
                Some(event) = rx.recv() => {
                    // Only process .yaml files
                    let yaml_paths: Vec<_> = event.paths.iter()
                        .filter(|p| p.extension().and_then(|e| e.to_str()) == Some("yaml"))
                        .cloned()
                        .collect();
                    if yaml_paths.is_empty() {
                        continue;
                    }

                    match event.kind {
                        EventKind::Create(_) | EventKind::Modify(_) => {
                            for path in yaml_paths {
                                let parsed = std::fs::read_to_string(&path)
                                    .ok()
                                    .and_then(|s| serde_yaml::from_str::<crate::skills::SkillConfig>(&s).ok());
                                match parsed {
                                    Some(skill) if skill.enabled => {
                                        if let Some(entry) = resolve_driver(&skill.driver) {
                                            if entry.kind == DriverKind::BuiltIn {
                                                log::info!(
                                                    "[SkillRuntime] Hot-reload: (re)starting '{}'",
                                                    skill.name
                                                );
                                                self.start_skill(skill);
                                            }
                                        }
                                    }
                                    Some(skill) => {
                                        // enabled: false — stop if running
                                        log::info!(
                                            "[SkillRuntime] Hot-reload: skill '{}' disabled, stopping",
                                            skill.name
                                        );
                                        self.stop_skill(&skill.name);
                                    }
                                    None => {
                                        log::warn!(
                                            "[SkillRuntime] Hot-reload: failed to parse '{}'",
                                            path.display()
                                        );
                                    }
                                }
                            }
                        }
                        EventKind::Remove(RemoveKind::File) => {
                            for path in yaml_paths {
                                if let Some(stem) = path.file_stem().and_then(|s| s.to_str()) {
                                    log::info!(
                                        "[SkillRuntime] Hot-reload: stopping '{}'",
                                        stem
                                    );
                                    self.stop_skill(stem);
                                }
                            }
                        }
                        _ => {}
                    }
                }
                _ = shutdown_rx.changed() => {
                    log::info!("[SkillRuntime] Hot-reload watcher shutting down");
                    break;
                }
            }
        }
        // watcher is dropped here, unregistering the watch
    }

    /// List active skill names.
    pub fn list_skills(&self) -> Vec<String> {
        self.tasks.keys().cloned().collect()
    }

    /// Returns a synthetic `NodeInfo` for each active built-in skill.
    pub fn get_skill_states(&self) -> Vec<crate::mcp::platform::NodeInfo> {
        self.tasks
            .keys()
            .map(|name| crate::mcp::platform::NodeInfo {
                name: name.clone(),
                status: "Running".to_string(),
                health: "Healthy".to_string(),
                node_type: "builtin".to_string(),
                installed: true,
                is_built: true,
            })
            .collect()
    }

    /// Load and start all enabled BuiltIn skills from the skills directory.
    pub fn load_from_dir(&mut self, skills_dir: &Path) {
        match load_skills(skills_dir) {
            Ok(skills) => {
                for skill in skills {
                    if !skill.enabled {
                        continue;
                    }
                    if let Some(entry) = resolve_driver(&skill.driver) {
                        if entry.kind == DriverKind::BuiltIn {
                            self.start_skill(skill);
                        }
                    }
                }
            }
            Err(e) => log::warn!("[SkillRuntime] Failed to load skills: {}", e),
        }
    }
}

/// Run the skill runtime — load skills from dir and wait for shutdown.
pub async fn run_skill_runtime(
    session: Arc<Session>,
    skills_dir: &Path,
    scope: &str,
    machine_id: &str,
    shutdown_rx: watch::Receiver<()>,
) -> anyhow::Result<()> {
    let mut runtime = SkillRuntime::new(
        session,
        scope.to_string(),
        machine_id.to_string(),
        shutdown_rx.clone(),
    );

    runtime.load_from_dir(skills_dir);
    log::info!(
        "[SkillRuntime] Loaded {} built-in skills",
        runtime.tasks.len()
    );

    // Run hot-reload watcher — blocks until shutdown fires.
    // Skill tasks are already spawned independently; this loop just reacts to
    // filesystem events and (re)starts/stops skills as YAML files change.
    runtime
        .watch_skills_dir(skills_dir, shutdown_rx.clone())
        .await;

    log::info!(
        "[SkillRuntime] Shutting down, aborting {} tasks",
        runtime.tasks.len()
    );
    runtime.shutdown();
    Ok(())
}
