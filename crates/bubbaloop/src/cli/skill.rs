//! `bubbaloop skill` — manage skills and show driver catalog.

use argh::FromArgs;

use crate::daemon::registry::get_bubbaloop_home;
use crate::skills::{self, DriverKind, DRIVER_CATALOG};

/// Manage skills and show driver catalog
#[derive(FromArgs, PartialEq, Debug)]
#[argh(subcommand, name = "skill")]
pub struct SkillCommand {
    #[argh(subcommand)]
    subcommand: SkillSubcommand,
}

#[derive(FromArgs, PartialEq, Debug)]
#[argh(subcommand)]
enum SkillSubcommand {
    Drivers(DriversCommand),
    List(ListCommand),
    Validate(ValidateCommand),
    Hub(HubCommand),
}

/// List all available drivers
#[derive(FromArgs, PartialEq, Debug)]
#[argh(subcommand, name = "drivers")]
pub struct DriversCommand {}

/// List active skills in ~/.bubbaloop/skills/
#[derive(FromArgs, PartialEq, Debug)]
#[argh(subcommand, name = "list")]
pub struct ListCommand {}

/// Validate a skill YAML file
#[derive(FromArgs, PartialEq, Debug)]
#[argh(subcommand, name = "validate")]
pub struct ValidateCommand {
    /// path to skill YAML file
    #[argh(positional)]
    pub file: String,
}

impl SkillCommand {
    pub async fn run(&self) -> anyhow::Result<()> {
        match &self.subcommand {
            SkillSubcommand::Drivers(cmd) => cmd.run(),
            SkillSubcommand::List(cmd) => cmd.run(),
            SkillSubcommand::Validate(cmd) => cmd.run(),
            SkillSubcommand::Hub(cmd) => cmd.run(),
        }
    }
}

impl DriversCommand {
    pub fn run(&self) -> anyhow::Result<()> {
        println!("{:<14} {:<12} Description", "Driver", "Tier");
        println!("{}", "-".repeat(60));
        for entry in DRIVER_CATALOG {
            let tier = match entry.kind {
                DriverKind::BuiltIn => "builtin",
                DriverKind::Marketplace => "marketplace",
            };
            println!(
                "{:<14} {:<12} {}",
                entry.driver_name, tier, entry.description
            );
        }
        Ok(())
    }
}

impl ListCommand {
    pub fn run(&self) -> anyhow::Result<()> {
        let skills_dir = get_bubbaloop_home().join("skills");
        let skill_configs = skills::load_skills(&skills_dir).unwrap_or_default();
        if skill_configs.is_empty() {
            println!("No skills found in {}", skills_dir.display());
            println!("Create a skill YAML file to get started.");
            return Ok(());
        }
        println!("{:<20} {:<14} {:<10} Intent", "Name", "Driver", "Status");
        println!("{}", "-".repeat(70));
        for skill in &skill_configs {
            let status = if skill.enabled { "enabled" } else { "disabled" };
            let intent = if skill.intent.is_empty() {
                "(none)"
            } else {
                skill.intent.lines().next().unwrap_or("")
            };
            println!(
                "{:<20} {:<14} {:<10} {}",
                skill.name, skill.driver, status, intent
            );
        }
        Ok(())
    }
}

impl ValidateCommand {
    pub fn run(&self) -> anyhow::Result<()> {
        let path = std::path::Path::new(&self.file);
        let raw = std::fs::read_to_string(path)
            .map_err(|e| anyhow::anyhow!("Cannot read '{}': {}", self.file, e))?;
        let skill: skills::SkillConfig =
            serde_yaml::from_str(&raw).map_err(|e| anyhow::anyhow!("YAML parse error: {}", e))?;
        skills::validate_skill(&skill).map_err(|e| anyhow::anyhow!("Validation failed: {}", e))?;
        println!(
            "  '{}' is valid (driver: {}, kind: {:?})",
            skill.name,
            skill.driver,
            skills::resolve_driver(&skill.driver).map(|e| e.kind)
        );
        Ok(())
    }
}

/// Browse and fetch skill templates from the community hub
#[derive(FromArgs, PartialEq, Debug)]
#[argh(subcommand, name = "hub")]
pub struct HubCommand {
    #[argh(subcommand)]
    subcommand: HubSubcommand,
}

#[derive(FromArgs, PartialEq, Debug)]
#[argh(subcommand)]
enum HubSubcommand {
    List(HubListCommand),
    Search(HubSearchCommand),
    Get(HubGetCommand),
    Refresh(HubRefreshCommand),
}

/// List community skill templates
#[derive(FromArgs, PartialEq, Debug)]
#[argh(subcommand, name = "list")]
pub struct HubListCommand {}

/// Search community skill templates
#[derive(FromArgs, PartialEq, Debug)]
#[argh(subcommand, name = "search")]
pub struct HubSearchCommand {
    /// search query (name, tag, category)
    #[argh(positional)]
    pub query: String,
}

/// Download a skill template to ~/.bubbaloop/skills/
#[derive(FromArgs, PartialEq, Debug)]
#[argh(subcommand, name = "get")]
pub struct HubGetCommand {
    /// skill name to download
    #[argh(positional)]
    pub name: String,
}

/// Refresh the local hub index cache
#[derive(FromArgs, PartialEq, Debug)]
#[argh(subcommand, name = "refresh")]
pub struct HubRefreshCommand {}

impl HubCommand {
    pub fn run(&self) -> anyhow::Result<()> {
        match &self.subcommand {
            HubSubcommand::List(cmd) => cmd.run(),
            HubSubcommand::Search(cmd) => cmd.run(),
            HubSubcommand::Get(cmd) => cmd.run(),
            HubSubcommand::Refresh(cmd) => cmd.run(),
        }
    }
}

impl HubListCommand {
    pub fn run(&self) -> anyhow::Result<()> {
        use crate::skill_hub;
        let mut entries = skill_hub::load_cached_hub();
        if entries.is_empty() {
            println!("No cached skills. Fetching from hub...");
            match skill_hub::refresh_hub_cache() {
                Ok(()) => entries = skill_hub::load_cached_hub(),
                Err(e) => {
                    println!(
                        "Warning: could not fetch hub ({}). No skills available offline.",
                        e
                    );
                    return Ok(());
                }
            }
        }
        if entries.is_empty() {
            println!("Hub is empty or not yet set up.");
            return Ok(());
        }
        println!(
            "{:<24} {:<14} {:<14} Description",
            "Name", "Category", "Driver"
        );
        println!("{}", "-".repeat(80));
        for e in &entries {
            println!(
                "{:<24} {:<14} {:<14} {}",
                e.name, e.category, e.driver, e.description
            );
        }
        Ok(())
    }
}

impl HubSearchCommand {
    pub fn run(&self) -> anyhow::Result<()> {
        use crate::skill_hub;
        let entries = skill_hub::load_cached_hub();
        if entries.is_empty() {
            println!("Hub cache is empty. Run `bubbaloop skill hub refresh` first.");
            return Ok(());
        }
        let results = skill_hub::search(&entries, &self.query);
        if results.is_empty() {
            println!("No skills matching '{}'", self.query);
            return Ok(());
        }
        for e in &results {
            println!("{} ({}) — {}", e.name, e.driver, e.description);
        }
        Ok(())
    }
}

impl HubGetCommand {
    pub fn run(&self) -> anyhow::Result<()> {
        use crate::skill_hub;
        let entries = skill_hub::load_cached_hub();
        let entry = entries
            .iter()
            .find(|e| e.name == self.name)
            .ok_or_else(|| {
                anyhow::anyhow!(
                    "Skill '{}' not found in hub index. Run `bubbaloop skill hub refresh` first.",
                    self.name
                )
            })?;
        let yaml = skill_hub::fetch_skill_template(entry)
            .map_err(|e| anyhow::anyhow!("Failed to fetch template: {}", e))?;
        let dest = get_bubbaloop_home()
            .join("skills")
            .join(format!("{}.yaml", self.name));
        std::fs::create_dir_all(dest.parent().unwrap())?;
        std::fs::write(&dest, &yaml)?;
        println!("Downloaded '{}' to {}", self.name, dest.display());
        println!(
            "Edit the file and replace any YOUR_* placeholders before running `bubbaloop up`."
        );
        Ok(())
    }
}

impl HubRefreshCommand {
    pub fn run(&self) -> anyhow::Result<()> {
        use crate::skill_hub;
        println!("Refreshing skill hub cache...");
        match skill_hub::refresh_hub_cache() {
            Ok(()) => {
                let entries = skill_hub::load_cached_hub();
                println!("Done. {} skills available.", entries.len());
            }
            Err(e) => println!("Failed to refresh: {}", e),
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn drivers_command_constructs() {
        let cmd = DriversCommand {};
        assert!(cmd.run().is_ok());
    }

    #[test]
    fn validate_command_rejects_missing_file() {
        let cmd = ValidateCommand {
            file: "/nonexistent/path/skill.yaml".to_string(),
        };
        assert!(cmd.run().is_err());
    }

    #[test]
    fn validate_command_rejects_bad_yaml() {
        use tempfile::tempdir;

        let dir = tempdir().unwrap();
        let path = dir.path().join("bad.yaml");
        std::fs::write(&path, ": : invalid yaml").unwrap();

        let cmd = ValidateCommand {
            file: path.display().to_string(),
        };
        assert!(cmd.run().is_err());
    }

    #[test]
    fn validate_command_accepts_valid_skill() {
        use tempfile::tempdir;

        let dir = tempdir().unwrap();
        let path = dir.path().join("cam.yaml");
        std::fs::write(&path, "name: my-cam\ndriver: rtsp\n").unwrap();

        let cmd = ValidateCommand {
            file: path.display().to_string(),
        };
        assert!(cmd.run().is_ok());
    }

    #[test]
    fn validate_command_rejects_unknown_driver() {
        use tempfile::tempdir;

        let dir = tempdir().unwrap();
        let path = dir.path().join("bad-driver.yaml");
        std::fs::write(&path, "name: my-cam\ndriver: not-a-driver\n").unwrap();

        let cmd = ValidateCommand {
            file: path.display().to_string(),
        };
        assert!(cmd.run().is_err());
    }

    #[test]
    fn list_command_empty_dir_does_not_panic() {
        // ListCommand reads ~/.bubbaloop/skills — just ensure it doesn't panic
        // when the directory doesn't exist (returns empty gracefully).
        let cmd = ListCommand {};
        // run() may succeed or fail, but must not panic
        let _ = cmd.run();
    }
}
