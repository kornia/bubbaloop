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
    pub fn run(&self) -> Result<()> {
        // 1. Resolve skills directory
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

        // 2. Load all YAML skills
        let skill_configs = skills::load_skills(&skills_dir)?;
        if skill_configs.is_empty() {
            println!("No skills found in {}", skills_dir.display());
            return Ok(());
        }

        println!("Found {} skill(s)", skill_configs.len());

        // 3. Load marketplace registry (refresh cache if empty)
        let mut registry_nodes = registry::load_cached_registry();
        if registry_nodes.is_empty() {
            log::info!("Registry cache empty, refreshing from upstream...");
            registry::refresh_cache().map_err(UpError::Registry)?;
            registry_nodes = registry::load_cached_registry();
        }

        // 4. Process each skill
        let mut installed_count: usize = 0;
        let mut already_installed: usize = 0;
        let mut skipped_count: usize = 0;

        for skill in &skill_configs {
            println!("\n  skill:  {}", skill.name);
            println!("  driver: {}", skill.driver);

            // Resolve driver name → DriverEntry (marketplace node + description)
            let driver_entry = match skills::resolve_driver(&skill.driver) {
                Some(d) => d,
                None => {
                    println!(
                        "  [warn] Unknown driver '{}', skipping",
                        skill.driver
                    );
                    skipped_count += 1;
                    continue;
                }
            };

            let marketplace_node = &driver_entry.marketplace_node;
            println!("  node:   {}", marketplace_node);

            // Find the registry entry for this node
            let registry_node = match registry::find_by_name(&registry_nodes, marketplace_node) {
                Some(n) => n,
                None => {
                    println!(
                        "  [warn] Node '{}' not in registry, skipping",
                        marketplace_node
                    );
                    skipped_count += 1;
                    continue;
                }
            };

            // Check installation status and install if needed
            if is_node_installed(marketplace_node) {
                println!("  [ok] Already installed");
                already_installed += 1;
            } else if self.dry_run {
                println!("  [dry-run] Would install {}", marketplace_node);
            } else {
                println!("  Installing {}...", marketplace_node);
                let node_dir = marketplace::download_precompiled(&registry_node)?;
                println!("  Installed to {}", node_dir);
                installed_count += 1;
            }

            // Inject skill config into node directory
            if !skill.config.is_empty() {
                let node_dir = resolve_node_dir(&registry_node);
                if self.dry_run {
                    println!(
                        "  [dry-run] Would write config to {}",
                        node_dir.display()
                    );
                } else {
                    match write_node_config(&node_dir, &skill.config) {
                        Ok(()) => {
                            println!("  Config written to {}", node_dir.display())
                        }
                        Err(e) => println!("  [warn] Config write failed: {}", e),
                    }
                }
            }

            // Print start hint (full systemd integration wired in by daemon)
            if !self.dry_run {
                println!("  Hint: bubbaloop node start {}", marketplace_node);
            }
        }

        // 5. Summary
        println!();
        println!(
            "Done: {} skill(s) loaded | {} installed | {} already present | {} skipped",
            skill_configs.len(),
            installed_count,
            already_installed,
            skipped_count
        );

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
