//! Plugin management commands (deprecated - use `bubbaloop node` instead)

use argh::FromArgs;
use std::fs;
use std::path::PathBuf;
use thiserror::Error;

use crate::templates;

#[derive(Debug, Error)]
pub enum PluginError {
    #[error("Invalid plugin type '{0}'. Use 'rust' or 'python'")]
    InvalidType(String),
    #[error("HOME environment variable not set")]
    HomeNotSet,
    #[error("Plugin directory already exists: {0}")]
    DirectoryExists(String),
    #[error("Template directory not found for type '{0}'")]
    TemplateNotFound(String),
    #[error("IO error: {0}")]
    Io(#[from] std::io::Error),
    #[error("YAML error: {0}")]
    Yaml(#[from] serde_yaml::Error),
    #[error("Path error: {0}")]
    StripPrefix(#[from] std::path::StripPrefixError),
    #[error("Walk error: {0}")]
    Walk(#[from] walkdir::Error),
}

pub type Result<T> = std::result::Result<T, PluginError>;

/// Plugin management commands
#[derive(FromArgs)]
#[argh(subcommand, name = "plugin")]
pub struct PluginCommand {
    #[argh(subcommand)]
    action: PluginAction,
}

#[derive(FromArgs)]
#[argh(subcommand)]
enum PluginAction {
    Init(InitArgs),
    List(ListArgs),
}

/// Initialize a new plugin from template
#[derive(FromArgs)]
#[argh(subcommand, name = "init")]
struct InitArgs {
    /// plugin name (e.g., "my-sensor")
    #[argh(positional)]
    name: String,

    /// plugin type: rust or python
    #[argh(option, short = 't', default = "String::from(\"rust\")")]
    plugin_type: String,

    /// output directory (default: ~/.bubbaloop/plugins/<name>)
    #[argh(option, short = 'o')]
    output: Option<String>,

    /// author name
    #[argh(option, default = "String::from(\"Anonymous\")")]
    author: String,

    /// plugin description
    #[argh(option, short = 'd', default = "String::from(\"A Bubbaloop plugin\")")]
    description: String,
}

/// List installed plugins
#[derive(FromArgs)]
#[argh(subcommand, name = "list")]
struct ListArgs {}

impl PluginCommand {
    pub fn run(self) -> Result<()> {
        match self.action {
            PluginAction::Init(args) => init_plugin(args),
            PluginAction::List(args) => list_plugins(args),
        }
    }
}

fn init_plugin(args: InitArgs) -> Result<()> {
    // Print deprecation warning
    eprintln!("WARNING: 'bubbaloop plugin init' is deprecated.");
    eprintln!("         Use 'bubbaloop node init' instead.");
    eprintln!();

    // Validate plugin type
    let plugin_type = args.plugin_type.to_lowercase();
    if plugin_type != "rust" && plugin_type != "python" {
        return Err(PluginError::InvalidType(args.plugin_type));
    }

    // Determine output directory (maintain old default behavior for backwards compat)
    let output_dir = if let Some(output) = &args.output {
        PathBuf::from(output)
    } else {
        let home = std::env::var("HOME").map_err(|_| PluginError::HomeNotSet)?;
        PathBuf::from(home)
            .join(".bubbaloop")
            .join("plugins")
            .join(&args.name)
    };

    // Use shared template module
    let output_dir = templates::create_node_at(
        &args.name,
        &plugin_type,
        &args.author,
        &args.description,
        &output_dir,
    )
    .map_err(|e| PluginError::Io(std::io::Error::other(e.to_string())))?;

    println!(
        "Plugin '{}' created at: {}",
        args.name,
        output_dir.display()
    );
    println!();
    println!("Next steps:");

    if plugin_type == "rust" {
        println!("  cd {}", output_dir.display());
        println!("  cargo build --release");
        println!("  ./target/release/{}_node", args.name);
    } else {
        println!("  cd {}", output_dir.display());
        println!("  python -m venv venv");
        println!("  source venv/bin/activate");
        println!("  pip install -r requirements.txt");
        println!("  python main.py");
    }

    Ok(())
}

fn list_plugins(_args: ListArgs) -> Result<()> {
    let home = std::env::var("HOME").map_err(|_| PluginError::HomeNotSet)?;
    let plugins_dir = PathBuf::from(home).join(".bubbaloop").join("plugins");

    if !plugins_dir.exists() {
        println!(
            "No plugins installed (directory does not exist: {})",
            plugins_dir.display()
        );
        return Ok(());
    }

    let mut found = false;
    for entry in fs::read_dir(&plugins_dir)? {
        let entry = entry?;
        let path = entry.path();

        if path.is_dir() {
            // Check for node.yaml (daemon compatible) or plugin.yaml (legacy)
            let manifest_path = path.join("node.yaml");
            let manifest_path = if manifest_path.exists() {
                manifest_path
            } else {
                path.join("plugin.yaml")
            };
            if manifest_path.exists() {
                let manifest = fs::read_to_string(&manifest_path)?;
                let manifest: serde_yaml::Value = serde_yaml::from_str(&manifest)?;

                let name = manifest["name"].as_str().unwrap_or("unknown");
                let version = manifest["version"].as_str().unwrap_or("0.0.0");
                let plugin_type = manifest["type"].as_str().unwrap_or("unknown");
                let description = manifest["description"].as_str().unwrap_or("");

                println!(
                    "{} (v{}) [{}] - {}",
                    name, version, plugin_type, description
                );
                found = true;
            }
        }
    }

    if !found {
        println!("No plugins installed in {}", plugins_dir.display());
    }

    Ok(())
}

// Template functions moved to crate::templates module
