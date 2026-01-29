//! Shared template processing for node/plugin creation

use std::fs;
use std::path::{Path, PathBuf};
use thiserror::Error;
use walkdir::WalkDir;

#[derive(Debug, Error)]
pub enum TemplateError {
    #[error("Invalid node type '{0}'. Use 'rust' or 'python'")]
    InvalidType(String),
    #[error("Template directory not found for type '{0}'")]
    TemplateNotFound(String),
    #[error("Directory already exists: {0}")]
    DirectoryExists(String),
    #[error("HOME environment variable not set")]
    HomeNotSet,
    #[error("IO error: {0}")]
    Io(#[from] std::io::Error),
    #[error("Path error: {0}")]
    StripPrefix(#[from] std::path::StripPrefixError),
    #[error("Walk error: {0}")]
    Walk(#[from] walkdir::Error),
}

pub type Result<T> = std::result::Result<T, TemplateError>;

/// Template variable substitution values
pub struct TemplateVars {
    pub node_name: String,        // kebab-case: my-sensor
    pub node_name_pascal: String, // PascalCase: MySensor
    pub node_name_snake: String,  // snake_case: my_sensor
    pub author: String,
    pub description: String,
}

impl TemplateVars {
    pub fn new(name: &str, author: &str, description: &str) -> Self {
        Self {
            node_name: to_kebab_case(name),
            node_name_pascal: to_pascal_case(name),
            node_name_snake: to_snake_case(name),
            author: author.to_string(),
            description: description.to_string(),
        }
    }
}

/// Convert string to kebab-case
pub fn to_kebab_case(s: &str) -> String {
    s.to_lowercase().replace(['_', ' '], "-")
}

/// Convert string to PascalCase
pub fn to_pascal_case(s: &str) -> String {
    s.split(['-', '_', ' '])
        .filter(|w| !w.is_empty())
        .map(|word| {
            let mut chars = word.chars();
            match chars.next() {
                None => String::new(),
                Some(first) => first.to_uppercase().chain(chars).collect(),
            }
        })
        .collect()
}

/// Convert string to snake_case
pub fn to_snake_case(s: &str) -> String {
    s.to_lowercase().replace(['-', ' '], "_")
}

fn env_var_or_empty(name: &str) -> String {
    std::env::var(name).unwrap_or_default()
}

/// Find template directory, searching multiple locations.
/// Searches for `*-node/` templates first (preferred), then falls back to `*-plugin/` (legacy).
pub fn find_template_dir(node_type: &str) -> Result<PathBuf> {
    let search_paths = vec![
        // Preferred: *-node templates
        format!("templates/{}-node", node_type),
        format!(
            "{}/templates/{}-node",
            env_var_or_empty("BUBBALOOP_ROOT"),
            node_type
        ),
        format!("/usr/share/bubbaloop/templates/{}-node", node_type),
        format!(
            "{}/bubbaloop/templates/{}-node",
            env_var_or_empty("HOME"),
            node_type
        ),
        // Legacy fallback: *-plugin templates
        format!("templates/{}-plugin", node_type),
        format!(
            "{}/templates/{}-plugin",
            env_var_or_empty("BUBBALOOP_ROOT"),
            node_type
        ),
        format!("/usr/share/bubbaloop/templates/{}-plugin", node_type),
        format!(
            "{}/bubbaloop/templates/{}-plugin",
            env_var_or_empty("HOME"),
            node_type
        ),
    ];

    for path_str in &search_paths {
        if path_str.is_empty() || (path_str.starts_with('/') && path_str.len() < 3) {
            continue; // Skip invalid paths from empty env vars
        }
        let path = PathBuf::from(path_str);
        if path.exists() {
            log::debug!("Found template at: {}", path.display());
            return Ok(path);
        }
    }

    Err(TemplateError::TemplateNotFound(node_type.to_string()))
}

/// Copy template directory to output, processing template variables
pub fn copy_template(template_dir: &Path, output_dir: &Path, vars: &TemplateVars) -> Result<()> {
    for entry in WalkDir::new(template_dir) {
        let entry = entry?;
        let src_path = entry.path();

        // Get relative path from template dir
        let rel_path = src_path.strip_prefix(template_dir)?;

        // Skip if it's the root directory
        if rel_path.as_os_str().is_empty() {
            continue;
        }

        // Process filename (remove .template suffix, substitute variables)
        let dest_name = rel_path
            .to_string_lossy()
            .replace(".template", "")
            .replace("{{node_name}}", &vars.node_name);
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

/// Process template content, replacing all template variables
pub fn process_template(content: &str, vars: &TemplateVars) -> String {
    content
        // New node-style variables
        .replace("{{node_name}}", &vars.node_name)
        .replace("{{node_name_pascal}}", &vars.node_name_pascal)
        .replace("{{node_name_snake}}", &vars.node_name_snake)
        // Legacy plugin-style variables (for backwards compatibility with *-plugin templates)
        .replace("{{plugin_name}}", &vars.node_name)
        .replace("{{plugin_name_pascal}}", &vars.node_name_pascal)
        // Common variables
        .replace("{{author}}", &vars.author)
        .replace("{{description}}", &vars.description)
}

/// Create a new node from template at a specific path
pub fn create_node_at(
    name: &str,
    node_type: &str,
    author: &str,
    description: &str,
    output_dir: &Path,
) -> Result<PathBuf> {
    // Validate node type
    let node_type = node_type.to_lowercase();
    if node_type != "rust" && node_type != "python" {
        return Err(TemplateError::InvalidType(node_type));
    }

    // Check if directory already exists and is not empty
    if output_dir.exists()
        && output_dir
            .read_dir()
            .map(|mut i| i.next().is_some())
            .unwrap_or(false)
    {
        return Err(TemplateError::DirectoryExists(
            output_dir.display().to_string(),
        ));
    }

    // Find template directory
    let template_dir = find_template_dir(&node_type)?;

    // Create output directory
    fs::create_dir_all(output_dir)?;
    log::info!("Creating node at: {}", output_dir.display());

    // Template variables
    let vars = TemplateVars::new(name, author, description);

    // Copy and process template files
    copy_template(&template_dir, output_dir, &vars)?;

    Ok(output_dir.to_path_buf())
}
