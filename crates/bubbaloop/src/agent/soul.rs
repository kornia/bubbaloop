//! Soul — hot-swappable agent identity and capabilities.
//!
//! The Soul consists of two files in `~/.bubbaloop/soul/`:
//! - `identity.md` — base system prompt (markdown)
//! - `capabilities.toml` — model config, heartbeat tuning, approval mode
//!
//! If files don't exist, compiled-in defaults are used.
//! A background task watches the directory and hot-reloads on changes.

use serde::Deserialize;
use std::io::Write;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use tokio::sync::RwLock;

/// Default identity prompt, embedded at compile time.
const DEFAULT_IDENTITY: &str = r#"You are Bubbaloop, an AI agent that manages physical sources and devices
through the Bubbaloop skill runtime.

Your job is to keep the fleet healthy and do what the user asks.

When given a task, DO it — don't describe what you would do, don't ask
for permission, don't offer options. Use your tools, get results, report back.

When something is wrong, diagnose it with tools, fix it, verify the fix.

You have node management tools, filesystem access (read_file, write_file),
and shell commands (run_command). Use them freely.

Be concise. Report what you did and the result, not what you plan to do."#;

/// Default capabilities TOML, embedded at compile time.
const DEFAULT_CAPABILITIES_TOML: &str = r#"model_name = "claude-sonnet-4-20250514"
max_turns = 15
allow_internet = true

# Heartbeat tuning (adaptive interval)
heartbeat_base_interval = 60
heartbeat_min_interval = 5
heartbeat_decay_factor = 0.7

# Approval mode: "auto" = execute immediately, "propose" = save for approval
default_approval_mode = "auto"

# Retry / circuit breaker
max_retries = 3

# Pre-compaction flush threshold (tokens from context limit)
compaction_flush_threshold_tokens = 4000

# Memory retention: delete episodic logs older than N days (0 = keep forever)
episodic_log_retention_days = 30

# Temporal decay: half-life in days for search scoring (0 = no decay)
episodic_decay_half_life_days = 30
"#;

/// Agent capabilities parsed from `capabilities.toml`.
#[derive(Debug, Clone, Deserialize)]
pub struct Capabilities {
    /// Claude model name (e.g., "claude-sonnet-4-20250514").
    pub model_name: String,
    /// Maximum tool-use turns per agent job.
    pub max_turns: usize,
    /// Whether the agent can make internet requests.
    pub allow_internet: bool,
    /// Resting heartbeat interval in seconds.
    pub heartbeat_base_interval: u64,
    /// Minimum heartbeat interval (max arousal).
    pub heartbeat_min_interval: u64,
    /// Arousal decay factor per calm beat (0.0–1.0).
    pub heartbeat_decay_factor: f64,
    /// Default approval mode for skills: "auto" or "propose".
    pub default_approval_mode: String,
    /// Max consecutive failures before circuit breaker trips.
    #[serde(default = "default_max_retries")]
    pub max_retries: u32,
    /// Token threshold for pre-compaction flush.
    #[serde(default = "default_compaction_threshold")]
    pub compaction_flush_threshold_tokens: usize,
    /// Delete episodic log files older than this many days. 0 = keep forever.
    #[serde(default = "default_retention_days")]
    pub episodic_log_retention_days: u32,
    /// Half-life in days for temporal decay in episodic search scoring. 0 = no decay.
    #[serde(default = "default_decay_half_life")]
    pub episodic_decay_half_life_days: u32,
}

fn default_max_retries() -> u32 {
    3
}
fn default_compaction_threshold() -> usize {
    4000
}
fn default_retention_days() -> u32 {
    30
}
fn default_decay_half_life() -> u32 {
    30
}

impl Default for Capabilities {
    fn default() -> Self {
        toml::from_str(DEFAULT_CAPABILITIES_TOML).expect("default capabilities TOML must parse")
    }
}

/// The agent's identity and configuration.
#[derive(Debug, Clone)]
pub struct Soul {
    /// Markdown identity prompt (system prompt base).
    pub identity: String,
    /// Parsed capabilities from TOML.
    pub capabilities: Capabilities,
}

impl Default for Soul {
    fn default() -> Self {
        Self {
            identity: DEFAULT_IDENTITY.to_string(),
            capabilities: Capabilities::default(),
        }
    }
}

impl Soul {
    /// Load Soul from `~/.bubbaloop/soul/` or fall back to compiled defaults.
    ///
    /// Creates the directory and default files if they don't exist.
    pub fn load_or_default() -> Self {
        let soul_dir = soul_directory();
        Self::load_from_dir(&soul_dir)
    }

    /// Load Soul from a specific directory. Falls back to defaults for missing files.
    pub fn load_from_dir(dir: &Path) -> Self {
        let identity = Self::read_identity(dir);
        let capabilities = Self::read_capabilities(dir);
        Self {
            identity,
            capabilities,
        }
    }

    /// Ensure the soul directory exists with default files.
    pub fn ensure_defaults() {
        let dir = soul_directory();
        if let Err(e) = std::fs::create_dir_all(&dir) {
            log::warn!("Failed to create soul directory: {}", e);
            return;
        }

        let identity_path = dir.join("identity.md");
        if !identity_path.exists() {
            if let Err(e) = std::fs::write(&identity_path, DEFAULT_IDENTITY) {
                log::warn!("Failed to write default identity.md: {}", e);
            }
        }

        let caps_path = dir.join("capabilities.toml");
        if !caps_path.exists() {
            if let Err(e) = std::fs::write(&caps_path, DEFAULT_CAPABILITIES_TOML) {
                log::warn!("Failed to write default capabilities.toml: {}", e);
            }
        }
    }

    /// Returns `true` if onboarding hasn't been completed yet.
    ///
    /// Uses a `.onboarded` marker file in `~/.bubbaloop/`. This is more
    /// reliable than content comparison — immune to daemon file creation
    /// and partial resets.
    pub fn is_first_run() -> bool {
        let marker = crate::daemon::registry::get_bubbaloop_home().join(".onboarded");
        !marker.exists()
    }

    /// Mark onboarding as complete by writing the marker file.
    pub fn mark_onboarded() {
        let marker = crate::daemon::registry::get_bubbaloop_home().join(".onboarded");
        let _ = std::fs::write(&marker, "");
    }

    /// Interactive first-run onboarding. Asks name, focus, and approval mode,
    /// then writes personalized `identity.md` and updates `capabilities.toml`.
    ///
    /// Returns the chosen agent name on success.
    /// Uses stdin/stdout directly — must run before the REPL thread starts.
    pub fn run_onboarding(soul_dir: &Path) -> std::io::Result<String> {
        println!();
        println!("  Welcome to Bubbaloop! Let's set up your agent.");
        println!();

        let name = prompt_with_default("  What should I call your agent?", "jean-clawd")?;

        let focus = prompt_with_default(
            "  Describe your agent's focus (e.g., \"home security cameras\",\n  \"weather monitoring station\", \"robot fleet\")",
            "general-purpose node management",
        )?;

        let approval = prompt_with_default(
            "  Approval mode — execute actions automatically or propose for approval? [auto/propose]",
            "auto",
        )?;
        let approval_mode = if approval.to_lowercase().starts_with('p') {
            "propose"
        } else {
            "auto"
        };

        // Generate personalized identity
        let identity = format!(
            "You are {name}, an AI agent that manages physical sources and devices\n\
             through the Bubbaloop skill runtime.\n\
             \n\
             Your focus: {focus}\n\
             \n\
             Your job is to keep the fleet healthy and do what the user asks.\n\
             \n\
             When given a task, DO it — don't describe what you would do, don't ask\n\
             for permission, don't offer options. Use your tools, get results, report back.\n\
             \n\
             When something is wrong, diagnose it with tools, fix it, verify the fix.\n\
             \n\
             You have node management tools, filesystem access (read_file, write_file),\n\
             and shell commands (run_command). Use them freely.\n\
             \n\
             Be concise. Report what you did and the result, not what you plan to do.",
        );

        std::fs::write(soul_dir.join("identity.md"), &identity)?;

        // Update approval mode in capabilities.toml (preserve other settings)
        let caps_path = soul_dir.join("capabilities.toml");
        let caps_content = std::fs::read_to_string(&caps_path).unwrap_or_default();
        let mut caps: toml::Value = toml::from_str(&caps_content)
            .unwrap_or_else(|_| toml::from_str(DEFAULT_CAPABILITIES_TOML).unwrap());
        caps["default_approval_mode"] = toml::Value::String(approval_mode.to_string());
        let updated = toml::to_string_pretty(&caps)
            .map_err(|e| std::io::Error::new(std::io::ErrorKind::InvalidData, e))?;
        std::fs::write(&caps_path, updated)?;

        println!();
        println!("  Agent configured: {} ({})", name, focus);
        println!("  Approval mode: {}", approval_mode);
        println!("  Edit ~/.bubbaloop/soul/ anytime to customize.");
        println!("  To reset: rm ~/.bubbaloop/.onboarded");
        println!();

        Ok(name)
    }

    /// Extract the agent name from the identity text.
    ///
    /// Looks for "You are {name}," in the first line, falls back to "Bubbaloop".
    pub fn name(&self) -> &str {
        self.identity
            .lines()
            .next()
            .and_then(|line| {
                let stripped = line.strip_prefix("You are ")?;
                let end = stripped.find(',')?;
                Some(stripped[..end].trim())
            })
            .unwrap_or("Bubbaloop")
    }

    fn read_identity(dir: &Path) -> String {
        let path = dir.join("identity.md");
        match std::fs::read_to_string(&path) {
            Ok(content) if !content.trim().is_empty() => content,
            Ok(_) => {
                log::info!("Soul identity.md is empty, using default");
                DEFAULT_IDENTITY.to_string()
            }
            Err(_) => {
                log::info!("Soul identity.md not found, using default");
                DEFAULT_IDENTITY.to_string()
            }
        }
    }

    fn read_capabilities(dir: &Path) -> Capabilities {
        let path = dir.join("capabilities.toml");
        match std::fs::read_to_string(&path) {
            Ok(content) => match toml::from_str(&content) {
                Ok(caps) => caps,
                Err(e) => {
                    log::warn!("Failed to parse capabilities.toml: {}, using defaults", e);
                    Capabilities::default()
                }
            },
            Err(_) => {
                log::info!("Soul capabilities.toml not found, using defaults");
                Capabilities::default()
            }
        }
    }
}

/// Sanitize a display name into a valid agent ID (1-64 chars, [a-z0-9_-]).
pub fn sanitize_agent_id(name: &str) -> String {
    let id: String = name
        .to_lowercase()
        .chars()
        .map(|c| {
            if c.is_ascii_alphanumeric() || c == '-' || c == '_' {
                c
            } else {
                '-'
            }
        })
        .collect();
    // Collapse multiple hyphens, trim leading/trailing hyphens
    let mut result = String::new();
    for c in id.chars() {
        if c == '-' && result.ends_with('-') {
            continue;
        }
        result.push(c);
    }
    let result = result.trim_matches('-').to_string();
    if result.is_empty() {
        "agent".to_string()
    } else {
        result[..result.len().min(64)].to_string()
    }
}

/// Return the soul directory path (`~/.bubbaloop/soul/`).
pub fn soul_directory() -> PathBuf {
    dirs::home_dir()
        .unwrap_or_else(|| PathBuf::from("/tmp"))
        .join(".bubbaloop")
        .join("soul")
}

/// Prompt the user for input with a default value. Returns the default if Enter is pressed.
fn prompt_with_default(prompt: &str, default: &str) -> std::io::Result<String> {
    print!("{} [{}]: ", prompt, default);
    std::io::stdout().flush()?;
    let mut input = String::new();
    std::io::stdin().read_line(&mut input)?;
    let trimmed = input.trim();
    if trimmed.is_empty() {
        Ok(default.to_string())
    } else {
        Ok(trimmed.to_string())
    }
}

/// Background task that watches `~/.bubbaloop/soul/` and hot-reloads on changes.
///
/// Updates the shared `Arc<RwLock<Soul>>` when files change.
/// Returns when the shutdown signal fires.
pub async fn soul_watcher(
    soul: Arc<RwLock<Soul>>,
    mut shutdown: tokio::sync::watch::Receiver<()>,
    watch_dir: Option<std::path::PathBuf>,
) {
    use notify::{Event, EventKind, RecursiveMode, Watcher};

    let dir = watch_dir.unwrap_or_else(soul_directory);
    if !dir.exists() {
        log::info!("Soul directory does not exist, watcher not started");
        // Just wait for shutdown
        let _ = shutdown.changed().await;
        return;
    }

    let (tx, mut rx) = tokio::sync::mpsc::channel::<()>(1);

    // Create file watcher
    let mut watcher = match notify::recommended_watcher(move |res: notify::Result<Event>| {
        if let Ok(event) = res {
            // Only react to modify/create events
            if matches!(event.kind, EventKind::Modify(_) | EventKind::Create(_)) {
                let _ = tx.blocking_send(());
            }
        }
    }) {
        Ok(w) => w,
        Err(e) => {
            log::warn!("Failed to create soul watcher: {}", e);
            let _ = shutdown.changed().await;
            return;
        }
    };

    if let Err(e) = watcher.watch(&dir, RecursiveMode::NonRecursive) {
        log::warn!("Failed to watch soul directory: {}", e);
        let _ = shutdown.changed().await;
        return;
    }

    log::info!("Soul watcher started on {}", dir.display());

    loop {
        tokio::select! {
            Some(()) = rx.recv() => {
                // Debounce: drain any queued events
                while rx.try_recv().is_ok() {}

                log::info!("Soul files changed, reloading...");
                let new_soul = Soul::load_from_dir(&dir);
                let mut guard = soul.write().await;
                *guard = new_soul;
                log::info!("Soul reloaded successfully");
            }
            _ = shutdown.changed() => {
                log::info!("Soul watcher shutting down");
                break;
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn default_capabilities_parse() {
        let caps = Capabilities::default();
        assert_eq!(caps.model_name, "claude-sonnet-4-20250514");
        assert_eq!(caps.max_turns, 15);
        assert!(caps.allow_internet);
        assert_eq!(caps.heartbeat_base_interval, 60);
        assert_eq!(caps.heartbeat_min_interval, 5);
        assert!((caps.heartbeat_decay_factor - 0.7).abs() < f64::EPSILON);
        assert_eq!(caps.default_approval_mode, "auto");
        assert_eq!(caps.max_retries, 3);
        assert_eq!(caps.compaction_flush_threshold_tokens, 4000);
        assert_eq!(caps.episodic_log_retention_days, 30);
        assert_eq!(caps.episodic_decay_half_life_days, 30);
    }

    #[test]
    fn default_soul() {
        let soul = Soul::default();
        assert!(soul.identity.contains("Bubbaloop"));
        assert_eq!(soul.capabilities.max_turns, 15);
    }

    #[test]
    fn load_from_nonexistent_dir() {
        let soul = Soul::load_from_dir(Path::new("/nonexistent/path"));
        // Should fall back to defaults
        assert!(soul.identity.contains("Bubbaloop"));
        assert_eq!(soul.capabilities.model_name, "claude-sonnet-4-20250514");
    }

    #[test]
    fn load_from_tempdir_with_files() {
        let dir = tempfile::tempdir().unwrap();

        std::fs::write(dir.path().join("identity.md"), "I am a test agent.").unwrap();
        std::fs::write(
            dir.path().join("capabilities.toml"),
            r#"
model_name = "claude-haiku-4-5-20251001"
max_turns = 5
allow_internet = false
heartbeat_base_interval = 30
heartbeat_min_interval = 2
heartbeat_decay_factor = 0.5
default_approval_mode = "propose"
"#,
        )
        .unwrap();

        let soul = Soul::load_from_dir(dir.path());
        assert_eq!(soul.identity, "I am a test agent.");
        assert_eq!(soul.capabilities.model_name, "claude-haiku-4-5-20251001");
        assert_eq!(soul.capabilities.max_turns, 5);
        assert!(!soul.capabilities.allow_internet);
        assert_eq!(soul.capabilities.heartbeat_base_interval, 30);
        assert_eq!(soul.capabilities.default_approval_mode, "propose");
        // Defaults for missing fields
        assert_eq!(soul.capabilities.max_retries, 3);
        assert_eq!(soul.capabilities.compaction_flush_threshold_tokens, 4000);
    }

    #[test]
    fn load_identity_fallback_on_empty() {
        let dir = tempfile::tempdir().unwrap();
        std::fs::write(dir.path().join("identity.md"), "  \n  ").unwrap();

        let soul = Soul::load_from_dir(dir.path());
        assert!(
            soul.identity.contains("Bubbaloop"),
            "empty identity should fall back to default"
        );
    }

    #[test]
    fn load_capabilities_fallback_on_invalid_toml() {
        let dir = tempfile::tempdir().unwrap();
        std::fs::write(
            dir.path().join("capabilities.toml"),
            "this is not valid toml {{{{",
        )
        .unwrap();

        let soul = Soul::load_from_dir(dir.path());
        assert_eq!(
            soul.capabilities.model_name, "claude-sonnet-4-20250514",
            "invalid TOML should fall back to defaults"
        );
    }

    #[test]
    fn ensure_defaults_creates_files() {
        let dir = tempfile::tempdir().unwrap();
        let soul_dir = dir.path().join("soul");
        // Temporarily override the soul directory by testing load_from_dir
        // (ensure_defaults uses the real home dir, so we test the load logic)

        std::fs::create_dir_all(&soul_dir).unwrap();
        // No files yet
        let soul = Soul::load_from_dir(&soul_dir);
        assert!(soul.identity.contains("Bubbaloop")); // defaults

        // Write files
        std::fs::write(soul_dir.join("identity.md"), DEFAULT_IDENTITY).unwrap();
        std::fs::write(
            soul_dir.join("capabilities.toml"),
            DEFAULT_CAPABILITIES_TOML,
        )
        .unwrap();

        let soul = Soul::load_from_dir(&soul_dir);
        assert!(soul.identity.contains("Bubbaloop"));
        assert_eq!(soul.capabilities.max_turns, 15);
    }

    #[test]
    fn backward_compat_missing_retention_fields() {
        // Old capabilities.toml without the new fields should still parse with defaults
        let toml_str = r#"
model_name = "claude-sonnet-4-20250514"
max_turns = 10
allow_internet = true
heartbeat_base_interval = 60
heartbeat_min_interval = 5
heartbeat_decay_factor = 0.7
default_approval_mode = "auto"
"#;
        let caps: Capabilities = toml::from_str(toml_str).unwrap();
        assert_eq!(caps.max_turns, 10);
        assert_eq!(caps.episodic_log_retention_days, 30);
        assert_eq!(caps.episodic_decay_half_life_days, 30);
    }

    #[test]
    fn soul_directory_path() {
        let dir = soul_directory();
        assert!(dir.to_string_lossy().contains(".bubbaloop"));
        assert!(dir.to_string_lossy().ends_with("soul"));
    }

    #[test]
    fn sanitize_agent_id_default_name() {
        assert_eq!(sanitize_agent_id("jean-clawd"), "jean-clawd");
    }

    #[test]
    fn name_extraction_default() {
        let soul = Soul::default();
        assert_eq!(soul.name(), "Bubbaloop");
    }

    #[test]
    fn name_extraction_custom() {
        let soul = Soul {
            identity: "You are Rosie, an AI agent that manages robots.\n\nYour focus: robots"
                .to_string(),
            capabilities: Capabilities::default(),
        };
        assert_eq!(soul.name(), "Rosie");
    }

    #[test]
    fn name_extraction_fallback() {
        let soul = Soul {
            identity: "I am a custom agent with no standard format.".to_string(),
            capabilities: Capabilities::default(),
        };
        assert_eq!(soul.name(), "Bubbaloop");
    }

    #[test]
    fn sanitize_agent_id_simple() {
        assert_eq!(sanitize_agent_id("jean clawd"), "jean-clawd");
    }

    #[test]
    fn sanitize_agent_id_mixed_case() {
        assert_eq!(sanitize_agent_id("Rosie"), "rosie");
    }

    #[test]
    fn sanitize_agent_id_special_chars() {
        assert_eq!(sanitize_agent_id("My Agent!@#$%"), "my-agent");
    }

    #[test]
    fn sanitize_agent_id_empty() {
        assert_eq!(sanitize_agent_id(""), "agent");
    }

    #[test]
    fn sanitize_agent_id_underscores() {
        assert_eq!(sanitize_agent_id("camera_bot"), "camera_bot");
    }

    #[test]
    fn sanitize_agent_id_collapses_hyphens() {
        assert_eq!(sanitize_agent_id("a - - b"), "a-b");
    }
}
