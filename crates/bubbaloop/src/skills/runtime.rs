//! Skill runtime — manages built-in driver tasks.

use crate::skills::builtin::http_poll::HttpPollDriver;
use crate::skills::builtin::system::SystemDriver;
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

    /// List active skill names.
    pub fn list_skills(&self) -> Vec<String> {
        self.tasks.keys().cloned().collect()
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

    // Wait for shutdown
    let mut rx = shutdown_rx;
    rx.changed().await.ok();
    log::info!(
        "[SkillRuntime] Shutting down, aborting {} tasks",
        runtime.tasks.len()
    );
    for (name, handle) in runtime.tasks.drain() {
        log::debug!("[SkillRuntime] Aborting task for '{}'", name);
        handle.abort();
    }
    Ok(())
}
