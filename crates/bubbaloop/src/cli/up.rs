//! `bubbaloop up` — Load all skills and ensure sensor nodes are running.
//!
//! Reads all YAML skill files from `~/.bubbaloop/skills/`, resolves the
//! corresponding marketplace nodes, installs missing nodes, injects
//! per-skill config, and prints a summary of what was done.

use std::collections::HashMap;
use std::path::{Path, PathBuf};

use argh::FromArgs;
use thiserror::Error;

use crate::daemon::registry::get_bubbaloop_home;
use crate::registry;
use crate::{marketplace, skills};

/// Errors for the `up` command.
#[derive(Debug, Error)]
pub enum UpError {
    #[error("Skill load error: {0}")]
    SkillLoad(#[from] skills::SkillError),
    #[error("Marketplace error: {0}")]
    Marketplace(#[from] marketplace::MarketplaceError),
    #[error("IO error: {0}")]
    Io(#[from] std::io::Error),
    #[error("Registry error: {0}")]
    Registry(String),
    #[error("Daemon error: {0}")]
    Daemon(#[from] crate::cli::daemon_client::DaemonClientError),
}

pub type Result<T> = std::result::Result<T, UpError>;

/// Load all skills and ensure sensor nodes are running
#[derive(FromArgs, PartialEq, Debug)]
#[argh(subcommand, name = "up")]
pub struct UpCommand {
    /// path to skills directory (default: ~/.bubbaloop/skills)
    #[argh(option, short = 's')]
    pub skills_dir: Option<String>,

    /// dry run — show what would be done without doing it
    #[argh(switch)]
    pub dry_run: bool,
}

impl UpCommand {
    pub async fn run(&self) -> Result<()> {
        let skills_dir = match &self.skills_dir {
            Some(p) => PathBuf::from(p),
            None => get_bubbaloop_home().join("skills"),
        };

        println!("Loading skills from: {}", skills_dir.display());

        if !skills_dir.exists() {
            println!("Skills directory not found: {}", skills_dir.display());
            println!("Create it with: mkdir -p {}", skills_dir.display());
            return Ok(());
        }

        if self.dry_run {
            println!("[dry-run] No changes will be made.");
        }

        let skill_configs = skills::load_skills(&skills_dir)?;
        if skill_configs.is_empty() {
            println!("No skills found in {}", skills_dir.display());
            return Ok(());
        }

        // Filter to enabled skills only
        let active_skills: Vec<_> = skill_configs.iter().filter(|s| s.enabled).collect();
        let disabled_count = skill_configs.len() - active_skills.len();
        println!(
            "Found {} skill(s) ({} active, {} disabled)",
            skill_configs.len(),
            active_skills.len(),
            disabled_count
        );

        if active_skills.is_empty() {
            println!("No active skills to process.");
            return Ok(());
        }

        // Load marketplace registry
        let mut registry_nodes = registry::load_cached_registry();
        if registry_nodes.is_empty() {
            log::info!("Registry cache empty, refreshing from upstream...");
            registry::refresh_cache().map_err(UpError::Registry)?;
            registry_nodes = registry::load_cached_registry();
        }

        let client = crate::cli::daemon_client::DaemonClient::new();

        let mut started_count: usize = 0;
        let mut already_running: usize = 0;
        let mut skipped_count: usize = 0;

        for skill in &active_skills {
            println!("\n  skill:  {}", skill.name);
            println!("  driver: {}", skill.driver);

            let driver_entry = match skills::resolve_driver(&skill.driver) {
                Some(d) => d,
                None => {
                    println!("  [skip] Unknown driver '{}'", skill.driver);
                    skipped_count += 1;
                    continue;
                }
            };

            let marketplace_node = driver_entry.marketplace_node;
            println!("  node:   {}", marketplace_node);

            let registry_node = match registry::find_by_name(&registry_nodes, marketplace_node) {
                Some(n) => n,
                None => {
                    println!("  [skip] Node '{}' not in registry", marketplace_node);
                    skipped_count += 1;
                    continue;
                }
            };

            // Step 1: Download if not installed locally
            let node_dir = if is_node_installed(marketplace_node) {
                resolve_node_dir(&registry_node)
            } else if self.dry_run {
                println!("  [dry-run] Would install {}", marketplace_node);
                skipped_count += 1;
                continue;
            } else {
                println!("  Installing {}...", marketplace_node);
                let dir = marketplace::download_precompiled(&registry_node)?;
                println!("  Downloaded to {}", dir);
                PathBuf::from(&dir)
            };

            // Step 2: Write per-skill config (each skill gets its own file)
            let config_path = if !skill.config.is_empty() && !self.dry_run {
                let cfg_dir = get_bubbaloop_home().join("skills-config").join(&skill.name);
                match write_node_config(&cfg_dir, &skill.config) {
                    Ok(()) => {
                        let p = cfg_dir.join("config.yaml");
                        println!("  Config written to {}", p.display());
                        Some(p.display().to_string())
                    }
                    Err(e) => {
                        println!("  [warn] Config write failed: {}", e);
                        None
                    }
                }
            } else {
                None
            };

            if self.dry_run {
                println!("  [dry-run] Would register + start {}", skill.name);
                continue;
            }

            // Step 3: Register with daemon as a named instance
            // Each skill becomes its own node instance (e.g. "entrance-cam" backed by rtsp-camera binary)
            let node_path = node_dir.display().to_string();
            let instance_name = &skill.name;
            match client
                .add_node(&node_path, Some(instance_name), config_path.as_deref())
                .await
            {
                Ok(resp) if resp.success => {
                    log::debug!("Registered {}: {}", instance_name, resp.message);
                }
                Ok(resp) => {
                    // Already registered or other non-fatal issue
                    log::debug!("add_node {}: {}", instance_name, resp.message);
                }
                Err(e) => {
                    println!("  [warn] Could not register with daemon: {}", e);
                    skipped_count += 1;
                    continue;
                }
            }

            // Step 4: Install systemd service
            match client.send_command(instance_name, "install").await {
                Ok(resp) if resp.success => {
                    log::debug!("Installed service for {}", instance_name);
                }
                Ok(_) | Err(_) => {
                    log::debug!(
                        "Service install for {} (may already exist)",
                        instance_name
                    );
                }
            }

            // Step 5: Start the node
            match client.send_command(instance_name, "start").await {
                Ok(resp) if resp.success => {
                    println!("  [ok] Started");
                    started_count += 1;
                }
                Ok(resp) => {
                    // Might already be running
                    if resp.message.contains("already") || resp.message.contains("Running") {
                        println!("  [ok] Already running");
                        already_running += 1;
                    } else {
                        println!("  [warn] Start: {}", resp.message);
                        skipped_count += 1;
                    }
                }
                Err(e) => {
                    println!("  [err] Failed to start: {}", e);
                    skipped_count += 1;
                }
            }
        }

        println!();
        println!(
            "Done: {} started | {} already running | {} skipped | {} disabled",
            started_count, already_running, skipped_count, disabled_count
        );

        // 6. Register skill schedules in memory DB
        let db_path = get_bubbaloop_home().join("memory.db");
        let mem = match crate::agent::memory::Memory::open(&db_path) {
            Ok(m) => m,
            Err(e) => {
                log::warn!(
                    "Could not open memory DB, skipping schedule registration: {}",
                    e
                );
                return Ok(());
            }
        };
        for skill in &skill_configs {
            if let Some(ref schedule_expr) = skill.schedule {
                if !skill.actions.is_empty() {
                    let actions_json =
                        serde_json::to_string(&skill.actions).unwrap_or_else(|_| "[]".to_string());
                    let sched = crate::agent::memory::Schedule {
                        id: uuid::Uuid::new_v4().to_string(),
                        name: skill.name.clone(),
                        cron: schedule_expr.clone(),
                        actions: actions_json,
                        tier: 1,
                        last_run: None,
                        next_run: None,
                        created_by: "yaml".to_string(),
                    };
                    if self.dry_run {
                        println!(
                            "  [dry-run] Would register schedule: {} ({})",
                            skill.name, schedule_expr
                        );
                    } else if let Err(e) = mem.upsert_schedule(&sched) {
                        println!("  [warn] Failed to register schedule: {}", e);
                    } else {
                        println!("  Registered schedule: {} ({})", skill.name, schedule_expr);
                    }
                }
            }
        }

        Ok(())
    }
}

/// Return true if a node directory for `node_name` exists under `~/.bubbaloop/nodes/`.
///
/// The layout is `~/.bubbaloop/nodes/<repo>/<subdir>` so we search two levels deep.
pub fn is_node_installed(node_name: &str) -> bool {
    let nodes_dir = get_bubbaloop_home().join("nodes");
    if !nodes_dir.exists() {
        return false;
    }

    // Walk: ~/.bubbaloop/nodes/<repo>/<subdir>
    let Ok(repo_entries) = std::fs::read_dir(&nodes_dir) else {
        return false;
    };
    for repo_entry in repo_entries.flatten() {
        let repo_path = repo_entry.path();
        if !repo_path.is_dir() {
            continue;
        }
        let Ok(node_entries) = std::fs::read_dir(&repo_path) else {
            continue;
        };
        for node_entry in node_entries.flatten() {
            if node_entry.file_name().to_string_lossy() == node_name {
                return true;
            }
        }
    }
    false
}

/// Write the skill `config:` section as `config.yaml` into the node directory.
pub fn write_node_config(
    node_dir: &Path,
    config: &HashMap<String, serde_yaml::Value>,
) -> std::io::Result<()> {
    std::fs::create_dir_all(node_dir)?;
    let yaml = serde_yaml::to_string(config)
        .map_err(|e| std::io::Error::new(std::io::ErrorKind::InvalidData, e))?;
    let dest = node_dir.join("config.yaml");
    std::fs::write(&dest, yaml)?;
    log::info!("Wrote node config to {}", dest.display());
    Ok(())
}

/// Compute the installed node directory from registry metadata.
///
/// Matches the layout created by `marketplace::download_precompiled`.
fn resolve_node_dir(entry: &registry::RegistryNode) -> PathBuf {
    let repo_name = entry
        .repo
        .rsplit('/')
        .next()
        .unwrap_or("bubbaloop-nodes-official");
    get_bubbaloop_home()
        .join("nodes")
        .join(repo_name)
        .join(&entry.subdir)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn up_command_struct_defaults() {
        let cmd = UpCommand {
            skills_dir: None,
            dry_run: false,
        };
        assert!(cmd.skills_dir.is_none());
        assert!(!cmd.dry_run);
    }

    #[test]
    fn up_command_struct_with_options() {
        let cmd = UpCommand {
            skills_dir: Some("/tmp/skills".to_string()),
            dry_run: true,
        };
        assert_eq!(cmd.skills_dir.as_deref(), Some("/tmp/skills"));
        assert!(cmd.dry_run);
    }

    #[test]
    fn is_node_installed_returns_false_for_nonexistent() {
        assert!(!is_node_installed("definitely-not-installed-xyz-9999"));
    }

    #[test]
    fn write_node_config_creates_valid_yaml() {
        use tempfile::tempdir;

        let dir = tempdir().unwrap();
        let node_dir = dir.path().join("test-node");

        let mut config = HashMap::new();
        config.insert(
            "url".to_string(),
            serde_yaml::Value::String("rtsp://example.com/stream".to_string()),
        );
        config.insert(
            "fps".to_string(),
            serde_yaml::Value::Number(serde_yaml::Number::from(30)),
        );

        write_node_config(&node_dir, &config).unwrap();

        let config_path = node_dir.join("config.yaml");
        assert!(config_path.exists());
        let content = std::fs::read_to_string(&config_path).unwrap();
        assert!(content.contains("rtsp://example.com/stream"));
        assert!(content.contains("30"));
    }

    #[test]
    fn write_node_config_empty_config() {
        use tempfile::tempdir;

        let dir = tempdir().unwrap();
        let node_dir = dir.path().join("empty-node");
        let config: HashMap<String, serde_yaml::Value> = HashMap::new();

        write_node_config(&node_dir, &config).unwrap();
        assert!(node_dir.join("config.yaml").exists());
    }

    #[test]
    fn write_node_config_creates_intermediate_dirs() {
        use tempfile::tempdir;

        let dir = tempdir().unwrap();
        let node_dir = dir.path().join("a").join("b").join("c");

        let mut config = HashMap::new();
        config.insert(
            "key".to_string(),
            serde_yaml::Value::String("val".to_string()),
        );

        write_node_config(&node_dir, &config).unwrap();
        assert!(node_dir.join("config.yaml").exists());
    }
}
