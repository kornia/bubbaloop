//! Plugin management commands

use argh::FromArgs;
use std::fs;
use std::path::{Path, PathBuf};
use thiserror::Error;

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
    // Validate plugin type
    let plugin_type = args.plugin_type.to_lowercase();
    if plugin_type != "rust" && plugin_type != "python" {
        return Err(PluginError::InvalidType(args.plugin_type));
    }

    // Determine output directory
    let output_dir = if let Some(output) = args.output {
        PathBuf::from(output)
    } else {
        let home = std::env::var("HOME").map_err(|_| PluginError::HomeNotSet)?;
        PathBuf::from(home)
            .join(".bubbaloop")
            .join("plugins")
            .join(&args.name)
    };

    // Check if directory already exists
    if output_dir.exists() {
        return Err(PluginError::DirectoryExists(
            output_dir.display().to_string(),
        ));
    }

    // Find template directory
    let template_dir = find_template_dir(&plugin_type)?;

    // Create output directory
    fs::create_dir_all(&output_dir)?;
    log::info!("Creating plugin at: {}", output_dir.display());

    // Template variables
    let vars = TemplateVars {
        plugin_name: args.name.clone(),
        plugin_name_pascal: to_pascal_case(&args.name),
        author: args.author,
        description: args.description,
    };

    // Copy and process template files
    copy_template(&template_dir, &output_dir, &vars)?;

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
        println!("  ./target/release/{}", args.name);
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
            let manifest_path = path.join("plugin.yaml");
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

struct TemplateVars {
    plugin_name: String,
    plugin_name_pascal: String,
    author: String,
    description: String,
}

/// Convert a kebab-case or snake_case string to PascalCase
fn to_pascal_case(s: &str) -> String {
    s.split(['-', '_'])
        .map(|word| {
            let mut chars = word.chars();
            match chars.next() {
                None => String::new(),
                Some(first) => first.to_uppercase().chain(chars).collect(),
            }
        })
        .collect()
}

fn find_template_dir(plugin_type: &str) -> Result<PathBuf> {
    // Try to find templates relative to the executable or in known locations
    let search_paths = vec![
        // Relative to current directory (for development)
        PathBuf::from(format!("templates/{}-plugin", plugin_type)),
        // Relative to project root
        PathBuf::from(format!(
            "{}/templates/{}-plugin",
            std::env::var("BUBBALOOP_ROOT").unwrap_or_default(),
            plugin_type
        )),
        // Standard install location
        PathBuf::from(format!(
            "/usr/share/bubbaloop/templates/{}-plugin",
            plugin_type
        )),
        // Home directory
        PathBuf::from(format!(
            "{}/bubbaloop/templates/{}-plugin",
            std::env::var("HOME").unwrap_or_default(),
            plugin_type
        )),
    ];

    for path in &search_paths {
        if path.exists() {
            log::debug!("Found template at: {}", path.display());
            return Ok(path.clone());
        }
    }

    Err(PluginError::TemplateNotFound(plugin_type.to_string()))
}

fn copy_template(template_dir: &Path, output_dir: &Path, vars: &TemplateVars) -> Result<()> {
    for entry in walkdir::WalkDir::new(template_dir) {
        let entry = entry?;
        let src_path = entry.path();

        // Get relative path from template dir
        let rel_path = src_path.strip_prefix(template_dir)?;

        // Skip if it's the root directory
        if rel_path.as_os_str().is_empty() {
            continue;
        }

        // Process filename (remove .template suffix if present)
        let dest_name = rel_path.to_string_lossy().replace(".template", "");
        let dest_path = output_dir.join(&dest_name);

        if src_path.is_dir() {
            fs::create_dir_all(&dest_path)?;
            log::debug!("Created directory: {}", dest_path.display());
        } else {
            // Create parent directories if needed
            if let Some(parent) = dest_path.parent() {
                fs::create_dir_all(parent)?;
            }

            // Read and process template
            let content = fs::read_to_string(src_path)?;
            let processed = process_template(&content, vars);

            // Write processed file
            fs::write(&dest_path, processed)?;
            log::debug!("Created file: {}", dest_path.display());
        }
    }

    Ok(())
}

fn process_template(content: &str, vars: &TemplateVars) -> String {
    content
        .replace("{{plugin_name}}", &vars.plugin_name)
        .replace("{{plugin_name_pascal}}", &vars.plugin_name_pascal)
        .replace("{{author}}", &vars.author)
        .replace("{{description}}", &vars.description)
}
