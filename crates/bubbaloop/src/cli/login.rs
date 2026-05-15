//! Login and logout commands for managing the Anthropic API key.
//!
//! `bubbaloop login`  — interactive browser + paste + validate + save flow
//!                      supports API key login or Claude setup-token for subscribers
//! `bubbaloop logout` — remove saved key and/or OAuth credentials

use std::path::PathBuf;

use crate::agent::provider::claude::OAUTH_BETA_HEADERS;

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

    #[error("OAuth error: {0}")]
    OAuth(String),

    #[error("JSON error: {0}")]
    Json(#[from] serde_json::Error),
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

const OAUTH_CREDENTIALS_FILENAME: &str = "oauth-credentials.json";
const SETUP_TOKEN_PREFIX: &str = "sk-ant-oat01-";
const SETUP_TOKEN_MIN_LENGTH: usize = 80;

// ── OAuth credential type ───────────────────────────────────────────

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct OAuthCredentials {
    pub access_token: String,
    pub refresh_token: String,
    pub expires_at: u64, // Unix timestamp in seconds
}

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
        let key_path = key_file_path()?;
        let oauth_path = oauth_credentials_path().ok();

        let mut removed = false;

        if key_path.exists() {
            std::fs::remove_file(&key_path)?;
            log::info!("Removed API key from {}", key_path.display());
            removed = true;
        }

        if let Some(ref oauth_path) = oauth_path {
            if oauth_path.exists() {
                std::fs::remove_file(oauth_path)?;
                log::info!("Removed OAuth credentials from {}", oauth_path.display());
                removed = true;
            }
        }

        if !removed {
            log::info!("No credentials found");
        }

        if std::env::var("ANTHROPIC_API_KEY").is_ok() {
            log::warn!(
                "ANTHROPIC_API_KEY environment variable is still set. \
                 Unset it to fully log out: unset ANTHROPIC_API_KEY"
            );
        }

        Ok(())
    }
}

// ── Interactive login flow ──────────────────────────────────────────

async fn run_login(skip_validation: bool) -> Result<()> {
    log::info!("Bubbaloop Login");
    log::info!("Choose authentication method:");
    log::info!("  [1] API Key (from console.anthropic.com)");
    log::info!("  [2] Claude Subscription (via 'claude setup-token')");

    let choice = loop {
        log::info!("Enter choice (1 or 2):");
        let mut line = String::new();
        std::io::stdin().read_line(&mut line)?;
        match line.trim() {
            "1" => break 1,
            "2" => break 2,
            _ => log::info!("Please enter 1 or 2."),
        }
    };

    match choice {
        1 => run_api_key_login(skip_validation).await,
        2 => run_setup_token_login().await,
        _ => unreachable!(),
    }
}

async fn run_api_key_login(skip_validation: bool) -> Result<()> {
    log::info!("API Key Login");
    log::info!("To use the agent, you need an Anthropic API key.");
    log::info!("1. Opening {}", CONSOLE_URL);
    if open::that(CONSOLE_URL).is_err() {
        log::info!("Could not open browser — copy the URL above");
    }
    log::info!("2. Create a new API key and copy it.");

    let key = match rpassword::prompt_password("Paste your API key: ") {
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

    if skip_validation {
        log::info!("Skipping validation (--skip-validation)");
    } else {
        match validate_api_key(&key).await {
            Ok(model_name) => log::info!("Key is valid ({} available)", model_name),
            Err(LoginError::InvalidKey(msg)) => {
                log::warn!("Invalid key: {}", msg);
                return Err(LoginError::InvalidKey(msg));
            }
            Err(e) => {
                log::warn!(
                    "Could not validate ({}). Saving key anyway — check later with: bubbaloop login --status",
                    e
                );
            }
        }
    }

    let path = key_file_path()?;
    save_key_file(&path, &key)?;
    log::info!("Saved to {}", path.display());
    log::info!("You're all set! Run 'bubbaloop agent' to start chatting.");

    Ok(())
}

async fn run_setup_token_login() -> Result<()> {
    log::info!("Claude Subscription Login (setup-token)");
    log::info!("This method uses Claude Code to generate an auth token.");
    log::info!("You need Claude Code installed (npm install -g @anthropic-ai/claude-code).");
    log::info!("1. Run this command in another terminal:");
    log::info!("   claude setup-token");
    log::info!("2. Follow the prompts in Claude Code to authenticate.");
    log::info!("   It will generate a token starting with 'sk-ant-oat01-'.");

    let token = match rpassword::prompt_password("Paste the setup-token: ") {
        Ok(t) => t.trim().to_string(),
        Err(_) => {
            return Err(LoginError::Io(std::io::Error::other(
                "this command requires an interactive terminal",
            )));
        }
    };

    if token.is_empty() {
        return Err(LoginError::OAuth("empty token".to_string()));
    }

    if !token.starts_with(SETUP_TOKEN_PREFIX) {
        return Err(LoginError::OAuth(format!(
            "expected token starting with '{}', got '{}'",
            SETUP_TOKEN_PREFIX,
            &token[..token.len().min(8)]
        )));
    }

    if token.len() < SETUP_TOKEN_MIN_LENGTH {
        return Err(LoginError::OAuth(
            "token looks too short — paste the full setup-token".to_string(),
        ));
    }

    match validate_bearer_token(&token).await {
        Ok(model_name) => log::info!("Token valid ({} available)", model_name),
        Err(LoginError::OAuth(msg)) => {
            log::warn!("Invalid token: {}", msg);
            return Err(LoginError::OAuth(msg));
        }
        Err(e) => {
            log::warn!(
                "Could not validate ({}). Saving token anyway — check later with: bubbaloop login --status",
                e
            );
        }
    }

    let now = unix_now();
    let credentials = OAuthCredentials {
        access_token: token,
        refresh_token: String::new(),
        expires_at: now + 365 * 24 * 3600,
    };

    let path = oauth_credentials_path()?;
    save_oauth_credentials(&path, &credentials)?;
    log::info!("Saved to {}", path.display());
    log::info!("You're all set! Run 'bubbaloop agent' to start chatting.");

    Ok(())
}

// ── Status check ────────────────────────────────────────────────────

async fn show_status() -> Result<()> {
    let mut any = false;

    // ── Gemini ───────────────────────────────────────────────────────
    let gemini_key = std::env::var("GEMINI_API_KEY")
        .ok()
        .filter(|v| !v.trim().is_empty())
        .or_else(|| read_key_file(dirs::home_dir()?.join(".bubbaloop").join("gemini-key")));

    if let Some(ref key) = gemini_key {
        log::info!("Gemini: configured ({})", mask_key(key));
        any = true;
    } else {
        log::info!("Gemini: not configured  (set GEMINI_API_KEY or ~/.bubbaloop/gemini-key)");
    }

    // ── Claude ───────────────────────────────────────────────────────
    let claude_key = std::env::var("ANTHROPIC_API_KEY")
        .ok()
        .filter(|v| !v.trim().is_empty())
        .or_else(|| read_key_file(key_file_path().ok()?));

    if let Some(ref key) = claude_key {
        let status = match validate_api_key(key).await {
            Ok(model) => format!("valid ({} available)", model),
            Err(LoginError::InvalidKey(msg)) => format!("invalid: {}", msg),
            Err(e) => format!("could not validate: {}", e),
        };
        log::info!("Claude: configured ({})  {}", mask_key(key), status);
        any = true;
    } else {
        match load_oauth_credentials() {
            Ok(Some(creds)) => {
                let now = unix_now();
                let expiry = if creds.expires_at > now {
                    let r = creds.expires_at - now;
                    format!("expires in {}h {}m", r / 3600, (r % 3600) / 60)
                } else {
                    "EXPIRED — run 'bubbaloop login' to refresh".to_string()
                };
                let token_status = match validate_bearer_token(&creds.access_token).await {
                    Ok(model) => format!("valid ({} available)", model),
                    Err(e) => format!("invalid: {}", e),
                };
                log::info!("Claude: OAuth ({})  {}", expiry, token_status);
                log::warn!(
                    "OAuth blocked for dashboard chat (Anthropic Feb 2026 ToS). \
                     Use GEMINI_API_KEY (free) or ANTHROPIC_API_KEY instead."
                );
                any = true;
            }
            Ok(None) => {
                log::info!(
                    "Claude: not configured  (run 'bubbaloop login' or set ANTHROPIC_API_KEY)"
                );
            }
            Err(e) => log::warn!("Failed to read OAuth credentials: {}", e),
        }
    }

    // ── Ollama ───────────────────────────────────────────────────────
    let host =
        std::env::var("OLLAMA_HOST").unwrap_or_else(|_| "http://localhost:11434".to_string());
    let reachable = reqwest::Client::new()
        .get(format!("{}/api/tags", host.trim_end_matches('/')))
        .timeout(std::time::Duration::from_secs(2))
        .send()
        .await
        .map(|r| r.status().is_success())
        .unwrap_or(false);
    log::info!(
        "Ollama:  {} at {}",
        if reachable {
            "reachable"
        } else {
            "not reachable (optional)"
        },
        host
    );
    if reachable {
        any = true;
    }

    if !any {
        log::warn!(
            "No providers configured. Recommended: set GEMINI_API_KEY (free tier) — \
             https://aistudio.google.com/app/apikey"
        );
    }
    Ok(())
}

fn read_key_file(path: std::path::PathBuf) -> Option<String> {
    let content = std::fs::read_to_string(path).ok()?;
    let key = content.lines().next()?.trim().to_string();
    (!key.is_empty()).then_some(key)
}

fn unix_now() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs()
}

// ── Bearer token validation ─────────────────────────────────────────

/// Validate a bearer token (setup-token) against the Anthropic API.
/// Returns the first model name on success.
async fn validate_bearer_token(token: &str) -> Result<String> {
    let client = reqwest::Client::new();
    let resp = client
        .get(MODELS_URL)
        .header("authorization", format!("Bearer {}", token))
        .header("anthropic-version", API_VERSION)
        .header("user-agent", "claude-cli/2.1.62")
        .header("x-app", "cli")
        .header("anthropic-beta", OAUTH_BETA_HEADERS)
        .send()
        .await?;

    let status = resp.status().as_u16();
    if status == 401 {
        return Err(LoginError::OAuth("authentication failed (401)".to_string()));
    }
    if !resp.status().is_success() {
        return Err(LoginError::OAuth(format!("unexpected status {}", status)));
    }

    let body: serde_json::Value = resp.json().await?;
    Ok(parse_model_name(&body))
}

// ── OAuth credential storage ────────────────────────────────────────

/// Returns the path to `~/.bubbaloop/oauth-credentials.json`.
pub fn oauth_credentials_path() -> Result<PathBuf> {
    let home = dirs::home_dir().ok_or(LoginError::NoHomeDir)?;
    Ok(home.join(".bubbaloop").join(OAUTH_CREDENTIALS_FILENAME))
}

/// Load OAuth credentials from disk. Returns `None` if the file does not exist.
pub fn load_oauth_credentials() -> Result<Option<OAuthCredentials>> {
    let path = oauth_credentials_path()?;
    if !path.exists() {
        return Ok(None);
    }
    let content = std::fs::read_to_string(&path)?;
    let creds: OAuthCredentials = serde_json::from_str(&content)?;
    Ok(Some(creds))
}

fn save_oauth_credentials(path: &PathBuf, creds: &OAuthCredentials) -> Result<()> {
    save_file_0600(path, &serde_json::to_string_pretty(creds)?)
}

// ── API key helpers ─────────────────────────────────────────────────

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

    let body: serde_json::Value = resp.json().await?;
    Ok(parse_model_name(&body))
}

/// Write content to `path` with 0600 permissions, creating parent dirs as needed.
fn save_file_0600(path: &PathBuf, content: &str) -> Result<()> {
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
        write!(file, "{}", content)?;
    }
    #[cfg(not(unix))]
    {
        std::fs::write(path, content)?;
    }
    Ok(())
}

fn save_key_file(path: &PathBuf, key: &str) -> Result<()> {
    save_file_0600(path, key)
}

/// Extract the first model name from a `/v1/models` response body.
fn parse_model_name(body: &serde_json::Value) -> String {
    body.get("data")
        .and_then(|d| d.as_array())
        .and_then(|arr| arr.first())
        .and_then(|m| m.get("id"))
        .and_then(|id| id.as_str())
        .unwrap_or("models endpoint reachable")
        .to_string()
}

/// Check if any Claude API credentials are configured (env var, OAuth, or key file).
pub fn has_claude_credentials() -> bool {
    if std::env::var("ANTHROPIC_API_KEY").is_ok() {
        return true;
    }
    if let Ok(Some(_)) = load_oauth_credentials() {
        return true;
    }
    if let Ok(path) = key_file_path() {
        if path.exists() {
            return true;
        }
    }
    false
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

    #[test]
    fn test_oauth_credentials_serde() {
        let creds = OAuthCredentials {
            access_token: "sk-ant-oat01-test".to_string(),
            refresh_token: "sk-ant-ort01-test".to_string(),
            expires_at: 1234567890,
        };
        let json = serde_json::to_string(&creds).unwrap();
        let parsed: OAuthCredentials = serde_json::from_str(&json).unwrap();
        assert_eq!(parsed.access_token, "sk-ant-oat01-test");
        assert_eq!(parsed.refresh_token, "sk-ant-ort01-test");
        assert_eq!(parsed.expires_at, 1234567890);
    }

    #[test]
    fn test_oauth_credentials_save_and_load() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("oauth-credentials.json");

        let creds = OAuthCredentials {
            access_token: "sk-ant-oat01-test".to_string(),
            refresh_token: "sk-ant-ort01-test".to_string(),
            expires_at: 1234567890,
        };

        save_oauth_credentials(&path, &creds).unwrap();

        let content = std::fs::read_to_string(&path).unwrap();
        let loaded: OAuthCredentials = serde_json::from_str(&content).unwrap();
        assert_eq!(loaded.access_token, "sk-ant-oat01-test");
    }

    #[cfg(unix)]
    #[test]
    fn test_oauth_credentials_file_permissions() {
        use std::os::unix::fs::PermissionsExt;
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("oauth-credentials.json");

        let creds = OAuthCredentials {
            access_token: "test".to_string(),
            refresh_token: "test".to_string(),
            expires_at: 0,
        };

        save_oauth_credentials(&path, &creds).unwrap();

        let metadata = std::fs::metadata(&path).unwrap();
        let mode = metadata.permissions().mode() & 0o777;
        assert_eq!(
            mode, 0o600,
            "OAuth credentials should have 0600 permissions"
        );
    }
}
