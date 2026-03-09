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
        }
    }
}

impl DriversCommand {
    pub fn run(&self) -> anyhow::Result<()> {
        println!("{:<14} {:<12} {}", "Driver", "Tier", "Description");
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
        println!(
            "{:<20} {:<14} {:<10} {}",
            "Name", "Driver", "Status", "Intent"
        );
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
