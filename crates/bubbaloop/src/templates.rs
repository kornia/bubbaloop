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
    let prefixes = [
        "".to_string(),
        env_var_or_empty("BUBBALOOP_ROOT"),
        "/usr/share/bubbaloop".to_string(),
        format!("{}/bubbaloop", env_var_or_empty("HOME")),
    ];
    // Preferred suffix first, then legacy fallback
    let suffixes = ["node", "plugin"];

    for suffix in &suffixes {
        for prefix in &prefixes {
            let dir = format!("{}-{}", node_type, suffix);
            let path = if prefix.is_empty() {
                PathBuf::from("templates").join(&dir)
            } else {
                PathBuf::from(prefix).join("templates").join(&dir)
            };
            if path.exists() {
                log::debug!("Found template at: {}", path.display());
                return Ok(path);
            }
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_to_kebab_case() {
        assert_eq!(to_kebab_case("my_sensor"), "my-sensor");
        assert_eq!(to_kebab_case("My Sensor"), "my-sensor");
        assert_eq!(to_kebab_case("already-kebab"), "already-kebab");
        assert_eq!(
            to_kebab_case("MixedCase_And_Dashes"),
            "mixedcase-and-dashes"
        );
    }

    #[test]
    fn test_to_pascal_case() {
        assert_eq!(to_pascal_case("my-sensor"), "MySensor");
        assert_eq!(to_pascal_case("my_sensor"), "MySensor");
        assert_eq!(to_pascal_case("my sensor"), "MySensor");
        assert_eq!(to_pascal_case("rtsp-camera"), "RtspCamera");
        assert_eq!(to_pascal_case("a"), "A");
    }

    #[test]
    fn test_to_snake_case() {
        assert_eq!(to_snake_case("my-sensor"), "my_sensor");
        assert_eq!(to_snake_case("My Sensor"), "my_sensor");
        assert_eq!(to_snake_case("already_snake"), "already_snake");
    }

    #[test]
    fn test_to_pascal_case_consecutive_separators() {
        assert_eq!(to_pascal_case("a--b"), "AB");
        assert_eq!(to_pascal_case("--leading"), "Leading");
    }

    #[test]
    fn test_process_template_substitution() {
        let vars = TemplateVars::new("my-sensor", "Test Author", "A test node");
        let input = "name = \"{{node_name}}\"\nclass {{node_name_pascal}}Node\nmod {{node_name_snake}}\nauthor: {{author}}\ndesc: {{description}}";
        let result = process_template(input, &vars);
        assert!(result.contains("name = \"my-sensor\""));
        assert!(result.contains("class MySensorNode"));
        assert!(result.contains("mod my_sensor"));
        assert!(result.contains("author: Test Author"));
        assert!(result.contains("desc: A test node"));
    }

    #[test]
    fn test_process_template_legacy_plugin_vars() {
        let vars = TemplateVars::new("my-sensor", "Author", "desc");
        let input = "{{plugin_name}} / {{plugin_name_pascal}}";
        let result = process_template(input, &vars);
        assert_eq!(result, "my-sensor / MySensor");
    }

    #[test]
    fn test_template_vars_new() {
        let vars = TemplateVars::new("rtsp-camera", "Team", "Camera node");
        assert_eq!(vars.node_name, "rtsp-camera");
        assert_eq!(vars.node_name_pascal, "RtspCamera");
        assert_eq!(vars.node_name_snake, "rtsp_camera");
    }

    #[test]
    fn test_create_node_at_invalid_type() {
        let tmp = std::env::temp_dir().join("bubbaloop_test_invalid_type");
        let result = create_node_at("test", "java", "author", "desc", &tmp);
        assert!(matches!(result, Err(TemplateError::InvalidType(_))));
    }

    #[test]
    fn test_rust_template_cargo_toml_no_ambiguous_dep() {
        // Verify the Rust Cargo.toml template doesn't combine git + path
        // (Cargo rejects this: "specification is ambiguous. Only one of git or path is allowed")
        let template_path = PathBuf::from("templates/rust-node/Cargo.toml.template");
        if template_path.exists() {
            let content = fs::read_to_string(&template_path).unwrap();
            // Check no line has both "git =" and "path =" for bubbaloop-schemas
            for line in content.lines() {
                if line.contains("bubbaloop-schemas") {
                    let has_git = line.contains("git =");
                    let has_path = line.contains("path =");
                    assert!(
                        !(has_git && has_path),
                        "bubbaloop-schemas dep must not combine git and path: {line}"
                    );
                }
            }
        }
    }

    #[test]
    fn test_rust_template_has_workspace_opt_out() {
        let template_path = PathBuf::from("templates/rust-node/Cargo.toml.template");
        if template_path.exists() {
            let content = fs::read_to_string(&template_path).unwrap();
            assert!(
                content.contains("[workspace]"),
                "Template must include [workspace] for standalone builds"
            );
        }
    }

    #[test]
    fn test_create_node_at_rejects_nonempty_dir() {
        let tmp = std::env::temp_dir().join("bubbaloop_test_nonempty");
        let _ = fs::create_dir_all(&tmp);
        let _ = fs::write(tmp.join("existing.txt"), "data");
        let result = create_node_at("test", "rust", "author", "desc", &tmp);
        assert!(matches!(result, Err(TemplateError::DirectoryExists(_))));
        let _ = fs::remove_dir_all(&tmp);
    }
}
