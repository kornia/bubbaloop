//! Marketplace CLI commands
//!
//! Manages marketplace sources (node registries) from the command line.
//! Sources are stored in ~/.bubbaloop/sources.json (same format as TUI Registry).

use argh::FromArgs;
use serde::{Deserialize, Serialize};
use std::fs;
use std::path::PathBuf;
use thiserror::Error;

#[derive(Debug, Error)]
pub enum MarketplaceError {
    #[error("IO error: {0}")]
    Io(#[from] std::io::Error),
    #[error("JSON error: {0}")]
    Json(#[from] serde_json::Error),
    #[error("{0}")]
    Other(String),
}

pub type Result<T> = std::result::Result<T, MarketplaceError>;

/// Manage marketplace sources (node registries)
#[derive(FromArgs)]
#[argh(subcommand, name = "marketplace")]
pub struct MarketplaceCommand {
    #[argh(subcommand)]
    action: MarketplaceAction,
}

#[derive(FromArgs)]
#[argh(subcommand)]
enum MarketplaceAction {
    List(ListArgs),
    Add(AddArgs),
    Remove(RemoveArgs),
    Enable(EnableArgs),
    Disable(DisableArgs),
}

/// List all marketplace sources
#[derive(FromArgs)]
#[argh(subcommand, name = "list")]
struct ListArgs {
    /// output format: table, json (default: table)
    #[argh(option, short = 'f', default = "String::from(\"table\")")]
    format: String,
}

/// Add a marketplace source
#[derive(FromArgs)]
#[argh(subcommand, name = "add")]
struct AddArgs {
    /// source name
    #[argh(positional)]
    name: String,

    /// source path (local directory)
    #[argh(positional)]
    path: String,

    /// source type: local or git (default: local)
    #[argh(option, short = 't', default = "String::from(\"local\")")]
    source_type: String,
}

/// Remove a marketplace source
#[derive(FromArgs)]
#[argh(subcommand, name = "remove")]
struct RemoveArgs {
    /// source name or path to remove
    #[argh(positional)]
    name_or_path: String,
}

/// Enable a marketplace source
#[derive(FromArgs)]
#[argh(subcommand, name = "enable")]
struct EnableArgs {
    /// source name or path to enable
    #[argh(positional)]
    name_or_path: String,
}

/// Disable a marketplace source
#[derive(FromArgs)]
#[argh(subcommand, name = "disable")]
struct DisableArgs {
    /// source name or path to disable
    #[argh(positional)]
    name_or_path: String,
}

const OFFICIAL_SOURCE_TYPE: &str = "builtin";

#[derive(Debug, Serialize, Deserialize, Default)]
struct SourcesRegistry {
    sources: Vec<SourceEntry>,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
struct SourceEntry {
    name: String,
    path: String,
    #[serde(rename = "type")]
    source_type: String,
    enabled: bool,
}

fn sources_path() -> PathBuf {
    dirs::home_dir()
        .unwrap_or_else(|| PathBuf::from("."))
        .join(".bubbaloop")
        .join("sources.json")
}

fn load_sources() -> SourcesRegistry {
    let path = sources_path();
    if path.exists() {
        fs::read_to_string(&path)
            .ok()
            .and_then(|s| serde_json::from_str(&s).ok())
            .unwrap_or_default()
    } else {
        SourcesRegistry::default()
    }
}

fn save_sources(registry: &SourcesRegistry) -> Result<()> {
    let path = sources_path();
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)?;
    }
    let json = serde_json::to_string_pretty(registry)?;
    fs::write(path, json)?;
    Ok(())
}

fn find_source_mut<'a>(
    registry: &'a mut SourcesRegistry,
    name_or_path: &str,
) -> Option<&'a mut SourceEntry> {
    registry
        .sources
        .iter_mut()
        .find(|s| s.name == name_or_path || s.path == name_or_path)
}

impl MarketplaceCommand {
    pub async fn run(self) -> Result<()> {
        match self.action {
            MarketplaceAction::List(args) => list_sources(args),
            MarketplaceAction::Add(args) => add_source(args),
            MarketplaceAction::Remove(args) => remove_source(args),
            MarketplaceAction::Enable(args) => enable_source(args),
            MarketplaceAction::Disable(args) => disable_source(args),
        }
    }
}

fn list_sources(args: ListArgs) -> Result<()> {
    let registry = load_sources();

    if args.format == "json" {
        println!("{}", serde_json::to_string_pretty(&registry.sources)?);
        return Ok(());
    }

    if registry.sources.is_empty() {
        println!("No marketplace sources configured.");
        println!("Add one with: bubbaloop marketplace add <name> <path>");
        return Ok(());
    }

    println!("{:<3} {:<20} {:<10} PATH", "ON", "NAME", "TYPE");
    println!("{}", "-".repeat(70));
    for source in &registry.sources {
        let enabled = if source.enabled { "yes" } else { "no" };
        println!(
            "{:<3} {:<20} {:<10} {}",
            enabled, source.name, source.source_type, source.path
        );
    }

    Ok(())
}

fn add_source(args: AddArgs) -> Result<()> {
    let mut registry = load_sources();

    if registry.sources.iter().any(|s| s.path == args.path) {
        return Err(MarketplaceError::Other(
            "Source with this path already exists".into(),
        ));
    }

    registry.sources.push(SourceEntry {
        name: args.name.clone(),
        path: args.path.clone(),
        source_type: args.source_type,
        enabled: true,
    });

    save_sources(&registry)?;
    println!("Added marketplace source: {}", args.name);
    Ok(())
}

fn remove_source(args: RemoveArgs) -> Result<()> {
    let mut registry = load_sources();

    // Prevent removing builtin sources
    if let Some(source) = registry
        .sources
        .iter()
        .find(|s| s.name == args.name_or_path || s.path == args.name_or_path)
    {
        if source.source_type == OFFICIAL_SOURCE_TYPE {
            return Err(MarketplaceError::Other(
                "Cannot remove builtin source".into(),
            ));
        }
    }

    let before = registry.sources.len();
    registry
        .sources
        .retain(|s| s.name != args.name_or_path && s.path != args.name_or_path);

    if registry.sources.len() == before {
        return Err(MarketplaceError::Other(format!(
            "Source '{}' not found",
            args.name_or_path
        )));
    }

    save_sources(&registry)?;
    println!("Removed marketplace source: {}", args.name_or_path);
    Ok(())
}

fn enable_source(args: EnableArgs) -> Result<()> {
    update_source_enabled(&args.name_or_path, true, "Enabled")
}

fn disable_source(args: DisableArgs) -> Result<()> {
    update_source_enabled(&args.name_or_path, false, "Disabled")
}

fn update_source_enabled(name_or_path: &str, enabled: bool, action: &str) -> Result<()> {
    let mut registry = load_sources();

    let source = find_source_mut(&mut registry, name_or_path)
        .ok_or_else(|| MarketplaceError::Other(format!("Source '{}' not found", name_or_path)))?;

    source.enabled = enabled;
    save_sources(&registry)?;
    println!("{} marketplace source: {}", action, name_or_path);
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_sources_registry_round_trip() {
        let registry = SourcesRegistry {
            sources: vec![
                SourceEntry {
                    name: "Official Nodes".into(),
                    path: "kornia/bubbaloop-nodes-official".into(),
                    source_type: "builtin".into(),
                    enabled: true,
                },
                SourceEntry {
                    name: "My Local".into(),
                    path: "/home/user/nodes".into(),
                    source_type: "local".into(),
                    enabled: false,
                },
            ],
        };

        let json = serde_json::to_string_pretty(&registry).unwrap();
        let parsed: SourcesRegistry = serde_json::from_str(&json).unwrap();

        assert_eq!(parsed.sources.len(), 2);
        assert_eq!(parsed.sources[0].name, "Official Nodes");
        assert_eq!(parsed.sources[0].source_type, "builtin");
        assert!(parsed.sources[0].enabled);
        assert_eq!(parsed.sources[1].name, "My Local");
        assert!(!parsed.sources[1].enabled);
    }

    #[test]
    fn test_serde_rename_type_field() {
        let json = r#"{"name":"test","path":"/path","type":"local","enabled":true}"#;
        let entry: SourceEntry = serde_json::from_str(json).unwrap();
        assert_eq!(entry.source_type, "local");

        // Verify it serializes back with "type" key
        let serialized = serde_json::to_string(&entry).unwrap();
        assert!(serialized.contains("\"type\""));
        assert!(!serialized.contains("\"source_type\""));
    }

    #[test]
    fn test_cannot_remove_builtin() {
        let mut registry = SourcesRegistry {
            sources: vec![SourceEntry {
                name: "Official Nodes".into(),
                path: "kornia/bubbaloop-nodes-official".into(),
                source_type: "builtin".into(),
                enabled: true,
            }],
        };

        // Verify the source is found as builtin
        let source = registry
            .sources
            .iter()
            .find(|s| s.name == "Official Nodes")
            .unwrap();
        assert_eq!(source.source_type, OFFICIAL_SOURCE_TYPE);

        // Simulate removal check
        let is_builtin = registry
            .sources
            .iter()
            .find(|s| s.name == "Official Nodes")
            .map(|s| s.source_type == OFFICIAL_SOURCE_TYPE)
            .unwrap_or(false);
        assert!(is_builtin);

        // Non-builtin should be removable
        registry.sources.push(SourceEntry {
            name: "Removable".into(),
            path: "/tmp/nodes".into(),
            source_type: "local".into(),
            enabled: true,
        });
        let before = registry.sources.len();
        registry.sources.retain(|s| s.name != "Removable");
        assert_eq!(registry.sources.len(), before - 1);
    }

    #[test]
    fn test_find_source_by_name_or_path() {
        let mut registry = SourcesRegistry {
            sources: vec![SourceEntry {
                name: "My Nodes".into(),
                path: "/home/user/nodes".into(),
                source_type: "local".into(),
                enabled: true,
            }],
        };

        assert!(find_source_mut(&mut registry, "My Nodes").is_some());
        assert!(find_source_mut(&mut registry, "/home/user/nodes").is_some());
        assert!(find_source_mut(&mut registry, "nonexistent").is_none());
    }
}
