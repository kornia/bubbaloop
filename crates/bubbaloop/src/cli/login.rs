//! Login and logout commands for managing the Anthropic API key.
//!
//! `bubbaloop login`  — interactive browser + paste + validate + save flow
//! `bubbaloop logout` — remove saved key

use std::path::PathBuf;

// ── Errors ──────────────────────────────────────────────────────────

#[derive(Debug, thiserror::Error)]
pub enum LoginError {
    #[error("IO error: {0}")]
    Io(#[from] std::io::Error),

    #[error("HTTP error: {0}")]
    Http(#[from] reqwest::Error),

    #[error("invalid API key: {0}")]
    InvalidKey(String),

    #[error("no API key configured")]
    NoKeyConfigured,

    #[error("cannot determine home directory")]
    NoHomeDir,
}

type Result<T> = std::result::Result<T, LoginError>;

// ── Commands ────────────────────────────────────────────────────────

/// Authenticate with Anthropic API
#[derive(argh::FromArgs)]
#[argh(subcommand, name = "login")]
pub struct LoginCommand {
    /// show current authentication status
    #[argh(switch)]
    pub status: bool,

    /// save the key without validating against the API
    #[argh(switch)]
    pub skip_validation: bool,
}

/// Remove saved Anthropic API key
#[derive(argh::FromArgs)]
#[argh(subcommand, name = "logout")]
pub struct LogoutCommand {}

// ── Constants ───────────────────────────────────────────────────────

const CONSOLE_URL: &str = "https://console.anthropic.com/settings/keys";
const MODELS_URL: &str = "https://api.anthropic.com/v1/models?limit=1";
const API_VERSION: &str = "2023-06-01";
const KEY_FILENAME: &str = "anthropic-key";

// ── Public API ──────────────────────────────────────────────────────

impl LoginCommand {
    pub async fn run(&self) -> Result<()> {
        if self.status {
            show_status().await
        } else {
            run_login(self.skip_validation).await
        }
    }
}

impl LogoutCommand {
    pub fn run(&self) -> Result<()> {
        let path = key_file_path()?;
        if path.exists() {
            std::fs::remove_file(&path)?;
            println!("Removed API key from {}", path.display());
        } else {
            println!("No API key file found at {}", path.display());
        }

        if std::env::var("ANTHROPIC_API_KEY").is_ok() {
            println!(
                "\nNote: ANTHROPIC_API_KEY environment variable is still set.\n\
                 Unset it to fully log out: unset ANTHROPIC_API_KEY"
            );
        }

        Ok(())
    }
}

// ── Interactive login flow ──────────────────────────────────────────

async fn run_login(skip_validation: bool) -> Result<()> {
    println!("\n  Bubbaloop Login\n");
    println!("  To use the agent, you need an Anthropic API key.\n");

    // Step 1: Open browser
    println!("  1. Opening {}", CONSOLE_URL);
    if open::that(CONSOLE_URL).is_err() {
        println!("     (Could not open browser — copy the URL above)");
    }

    println!("\n  2. Create a new API key and copy it.\n");

    // Step 2: Prompt for key (hidden input, requires real terminal)
    let key = match rpassword::prompt_password("  Paste your API key: ") {
        Ok(k) => k.trim().to_string(),
        Err(_) => {
            return Err(LoginError::Io(std::io::Error::other(
                "this command requires an interactive terminal",
            )));
        }
    };

    if key.is_empty() {
        return Err(LoginError::InvalidKey("empty input".to_string()));
    }

    // Step 3: Validate (unless skipped)
    if skip_validation {
        println!("\n  Skipping validation (--skip-validation)");
    } else {
        print!("\n  Validating... ");
        match validate_api_key(&key).await {
            Ok(model_name) => {
                println!("Key is valid ({} available)", model_name);
            }
            Err(LoginError::InvalidKey(msg)) => {
                println!("Invalid key: {}", msg);
                return Err(LoginError::InvalidKey(msg));
            }
            Err(e) => {
                // Network error — save anyway, warn user
                println!(
                    "Could not validate ({})\n  Saving key anyway — check later with: bubbaloop login --status",
                    e
                );
            }
        }
    }

    // Step 4: Save
    let path = key_file_path()?;
    save_key_file(&path, &key)?;
    println!("\n  Saved to {}\n", path.display());
    println!("  You're all set! Run 'bubbaloop agent' to start chatting.\n");

    Ok(())
}

// ── Status check ────────────────────────────────────────────────────

async fn show_status() -> Result<()> {
    let path = key_file_path()?;

    // Check env var first
    if let Ok(env_key) = std::env::var("ANTHROPIC_API_KEY") {
        println!("API key source: ANTHROPIC_API_KEY environment variable");
        print!("Validating... ");
        match validate_api_key(&env_key).await {
            Ok(model) => println!("Valid ({} available)", model),
            Err(LoginError::InvalidKey(msg)) => println!("Invalid: {}", msg),
            Err(e) => println!("Could not validate: {}", e),
        }
        return Ok(());
    }

    // Check file
    if !path.exists() {
        println!("No API key configured.");
        println!("Run 'bubbaloop login' to set up your API key.");
        return Ok(());
    }

    let content = std::fs::read_to_string(&path)?;
    let key = content.lines().next().unwrap_or("").trim();
    if key.is_empty() {
        println!("Key file exists but is empty: {}", path.display());
        println!("Run 'bubbaloop login' to set up your API key.");
        return Ok(());
    }

    println!("API key source: {}", path.display());
    let masked = mask_key(key);
    println!("Key: {}", masked);
    print!("Validating... ");
    match validate_api_key(key).await {
        Ok(model) => println!("Valid ({} available)", model),
        Err(LoginError::InvalidKey(msg)) => println!("Invalid: {}", msg),
        Err(e) => println!("Could not validate: {}", e),
    }

    Ok(())
}

// ── Helpers ─────────────────────────────────────────────────────────

/// Validate an API key by hitting GET /v1/models (free, no token cost).
/// Returns the first model name on success.
async fn validate_api_key(key: &str) -> Result<String> {
    let client = reqwest::Client::new();
    let resp = client
        .get(MODELS_URL)
        .header("x-api-key", key)
        .header("anthropic-version", API_VERSION)
        .send()
        .await?;

    let status = resp.status().as_u16();
    if status == 401 {
        return Err(LoginError::InvalidKey(
            "authentication failed (401)".to_string(),
        ));
    }
    if status == 403 {
        return Err(LoginError::InvalidKey("access denied (403)".to_string()));
    }
    if !resp.status().is_success() {
        return Err(LoginError::InvalidKey(format!(
            "unexpected status {}",
            status
        )));
    }

    // Parse response to extract a model name
    let body: serde_json::Value = resp.json().await?;
    let model_name = body
        .get("data")
        .and_then(|d| d.as_array())
        .and_then(|arr| arr.first())
        .and_then(|m| m.get("id"))
        .and_then(|id| id.as_str())
        .unwrap_or("models endpoint reachable")
        .to_string();

    Ok(model_name)
}

/// Write the key to disk with 0600 permissions.
fn save_key_file(path: &PathBuf, key: &str) -> Result<()> {
    // Ensure parent directory exists
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)?;
    }

    #[cfg(unix)]
    {
        use std::io::Write;
        use std::os::unix::fs::OpenOptionsExt;
        let mut file = std::fs::OpenOptions::new()
            .write(true)
            .create(true)
            .truncate(true)
            .mode(0o600)
            .open(path)?;
        write!(file, "{}", key)?;
    }

    #[cfg(not(unix))]
    {
        std::fs::write(path, key)?;
    }

    Ok(())
}

/// Get the path to the API key file: `~/.bubbaloop/anthropic-key`.
fn key_file_path() -> Result<PathBuf> {
    let home = dirs::home_dir().ok_or(LoginError::NoHomeDir)?;
    Ok(home.join(".bubbaloop").join(KEY_FILENAME))
}

/// Mask an API key for display: show first 8 and last 4 chars.
fn mask_key(key: &str) -> String {
    if key.len() <= 12 {
        return "****".to_string();
    }
    let prefix = &key[..8];
    let suffix = &key[key.len() - 4..];
    format!("{}...{}", prefix, suffix)
}

// ── Tests ───────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_key_file_path() {
        let path = key_file_path().unwrap();
        assert!(path.ends_with(".bubbaloop/anthropic-key"));
    }

    #[test]
    fn test_mask_key_normal() {
        let masked = mask_key("sk-ant-api03-1234567890abcdef");
        assert_eq!(masked, "sk-ant-a...cdef");
    }

    #[test]
    fn test_mask_key_short() {
        assert_eq!(mask_key("short"), "****");
    }

    #[test]
    fn test_save_and_read_roundtrip() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("anthropic-key");

        save_key_file(&path, "sk-ant-test-key-123").unwrap();

        let content = std::fs::read_to_string(&path).unwrap();
        assert_eq!(content, "sk-ant-test-key-123");
    }

    #[test]
    fn test_save_creates_parent_dirs() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("nested").join("dir").join("key");

        save_key_file(&path, "test-key").unwrap();

        assert!(path.exists());
        assert_eq!(std::fs::read_to_string(&path).unwrap(), "test-key");
    }

    #[test]
    fn test_save_overwrites_existing() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("key");

        save_key_file(&path, "first-key").unwrap();
        save_key_file(&path, "second-key").unwrap();

        assert_eq!(std::fs::read_to_string(&path).unwrap(), "second-key");
    }

    #[cfg(unix)]
    #[test]
    fn test_save_file_permissions() {
        use std::os::unix::fs::PermissionsExt;

        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("key");

        save_key_file(&path, "test-key").unwrap();

        let metadata = std::fs::metadata(&path).unwrap();
        let mode = metadata.permissions().mode() & 0o777;
        assert_eq!(mode, 0o600, "key file should have 0600 permissions");
    }
}
