//! Interactive agent model/provider setup wizard.
//!
//! `bubbaloop agent setup` — configure which provider and model each agent uses.
//! Writes to `~/.bubbaloop/agents.toml`. No Zenoh or daemon needed.

use crate::agent::runtime::{agent_directory, AgentEntry, AgentsConfig};
use crate::cli::login;
use std::io;

/// Known Claude models for interactive selection.
const CLAUDE_MODELS: &[(&str, &str)] = &[
    ("claude-sonnet-4-20250514", "Sonnet 4 (recommended)"),
    ("claude-haiku-4-5-20251001", "Haiku 4.5 (fast, cheap)"),
    ("claude-opus-4-20250514", "Opus 4 (most capable)"),
];

/// Known Gemini models for interactive selection.
const GEMINI_MODELS: &[(&str, &str)] = &[
    (
        "gemini-2.5-flash",
        "2.5 Flash — free tier, fast vision+text",
    ),
    ("gemini-2.5-pro", "2.5 Pro — strongest reasoning, billed"),
];

/// Run the interactive setup wizard.
pub async fn run_setup(target_agent: Option<&str>) -> Result<(), Box<dyn std::error::Error>> {
    let mut config = AgentsConfig::load_or_default();

    log::info!("\n  Agent Model Setup\n");

    // Select which agent to configure
    let is_new_agent;
    let agent_id = match target_agent {
        Some(id) => {
            is_new_agent = !config.agents.contains_key(id);
            if is_new_agent {
                log::info!("  Creating new agent '{}'.\n", id);
                config.agents.insert(
                    id.to_string(),
                    AgentEntry {
                        enabled: true,
                        default: config.agents.is_empty(),
                        capabilities: vec![],
                        provider: "claude".to_string(),
                        model: None,
                    },
                );
            }
            id.to_string()
        }
        None => {
            is_new_agent = false;
            select_agent(&config)?
        }
    };

    // Show current config for existing agents
    if !is_new_agent {
        if let Some(entry) = config.agents.get(&agent_id) {
            let model_display = entry.model.as_deref().unwrap_or("(from soul defaults)");
            log::info!(
                "  Current: provider={}, model={}",
                entry.provider,
                model_display
            );
        }
    }

    // Choose provider (Gemini first — recommended, free tier, no ToS concerns)
    log::info!("\n  Choose provider:\n");
    log::info!("  [1] Gemini (recommended — free tier, strong vision)");
    log::info!("  [2] Claude API (cloud, pay-per-token)");
    log::info!("  [3] Ollama (local, no cloud bill)\n");

    let provider_choice = prompt_choice("  Enter choice (1, 2, or 3)", &["1", "2", "3"])?;
    let provider = match provider_choice.as_str() {
        "1" => "gemini",
        "2" => "claude",
        _ => "ollama",
    };

    // Nudge setup if credentials are missing for cloud providers
    if provider == "gemini" && !crate::agent::provider::gemini::GeminiProvider::has_credentials() {
        log::info!("\n  No Gemini API key found.");
        log::info!("  Set GEMINI_API_KEY or place key in ~/.bubbaloop/gemini-key");
        log::info!("  Get a free key at: https://aistudio.google.com/app/apikey");
        log::info!("\n  Continue anyway? Model config will be saved. [y/N]");
        let mut line = String::new();
        io::stdin().read_line(&mut line)?;
        if !line.trim().eq_ignore_ascii_case("y") {
            log::info!("  Setup paused. Set GEMINI_API_KEY then run setup again.");
            return Ok(());
        }
    }

    if provider == "claude" && !login::has_claude_credentials() {
        log::info!("\n  No Claude API credentials found.");
        log::info!("  Run 'bubbaloop login' first to authenticate.");
        log::info!("  Tip: GEMINI_API_KEY has a free tier — try provider [1] instead.\n");
        log::info!("  Continue anyway? Model config will be saved. [y/N]");
        let mut line = String::new();
        io::stdin().read_line(&mut line)?;
        if !line.trim().eq_ignore_ascii_case("y") {
            log::info!("  Setup cancelled. Run 'bubbaloop login' then try again.");
            return Ok(());
        }
    }

    // Choose model based on provider
    let model = match provider {
        "ollama" => select_ollama_model().await?,
        "gemini" => select_gemini_model()?,
        _ => select_claude_model()?,
    };

    // Update config
    if let Some(entry) = config.agents.get_mut(&agent_id) {
        entry.provider = provider.to_string();
        entry.model = Some(model.clone());
    }

    // Save
    config.save()?;

    log::info!("\n  Updated agent '{}':", agent_id);
    log::info!("    provider: {}", provider);
    log::info!("    model: {}", model);
    // For new agents, ask for a quick identity description and write identity.md
    if is_new_agent {
        let soul_dir = agent_directory(&agent_id).join("soul");
        std::fs::create_dir_all(&soul_dir).ok();

        log::info!("\n  Describe this agent's role in one sentence");
        log::info!("  (e.g., \"analyzes camera images and produces captions\"):\n");
        let description = prompt_line("  Role")?;

        let identity = format!(
            "You are {}, an AI agent that {} through the Bubbaloop skill runtime.\n\n\
             Be concise. Report what you did and the result. No fluff.\n",
            agent_id, description
        );
        let identity_path = soul_dir.join("identity.md");
        std::fs::write(&identity_path, &identity)?;
        log::info!("  Wrote identity to {}", identity_path.display());
    }

    log::info!("\n  Saved to ~/.bubbaloop/agents.toml");

    // Offer to restart the daemon so changes take effect immediately
    log::info!("\n  Restart daemon for changes to take effect? [Y/n]");
    let mut line = String::new();
    io::stdin().read_line(&mut line)?;
    let should_restart = line.trim().is_empty() || line.trim().eq_ignore_ascii_case("y");

    if should_restart {
        restart_daemon().await;
        log::info!("\n  Chat with your agent:");
        log::info!("    bubbaloop agent chat -a {}", agent_id);
    } else if is_new_agent {
        log::info!("  When ready, restart the daemon then chat:");
        log::info!("    bubbaloop daemon restart");
        log::info!("    bubbaloop agent chat -a {}", agent_id);
    } else {
        log::info!("  Run 'bubbaloop daemon restart' for changes to take effect.");
    }
    log::info!("");

    Ok(())
}

/// Let the user pick an agent from the current config.
fn select_agent(config: &AgentsConfig) -> Result<String, Box<dyn std::error::Error>> {
    let agents: Vec<(&String, &AgentEntry)> = config.agents.iter().collect();

    if agents.is_empty() {
        return Err("No agents configured. Use --agent <name> to create one.".into());
    }

    if agents.len() == 1 {
        let id = agents[0].0.clone();
        log::info!("  Agent: {}", id);
        return Ok(id);
    }

    log::info!("  Available agents:\n");
    for (i, (id, entry)) in agents.iter().enumerate() {
        let default_tag = if entry.default { " (default)" } else { "" };
        let model_display = entry.model.as_deref().unwrap_or("soul default");
        log::info!(
            "  [{}] {}{} — {} / {}",
            i + 1,
            id,
            default_tag,
            entry.provider,
            model_display
        );
    }
    log::info!("");

    let valid: Vec<String> = (1..=agents.len()).map(|i| i.to_string()).collect();
    let valid_refs: Vec<&str> = valid.iter().map(|s| s.as_str()).collect();
    let choice = prompt_choice("  Select agent", &valid_refs)?;
    let idx: usize = choice.parse::<usize>()? - 1;
    Ok(agents[idx].0.clone())
}

/// Interactive Claude model selection.
fn select_claude_model() -> Result<String, Box<dyn std::error::Error>> {
    log::info!("\n  Available Claude models:\n");
    for (i, (id, desc)) in CLAUDE_MODELS.iter().enumerate() {
        log::info!("  [{}] {} — {}", i + 1, id, desc);
    }
    log::info!("  [{}] Custom (enter model ID)\n", CLAUDE_MODELS.len() + 1);

    let valid: Vec<String> = (1..=CLAUDE_MODELS.len() + 1)
        .map(|i| i.to_string())
        .collect();
    let valid_refs: Vec<&str> = valid.iter().map(|s| s.as_str()).collect();
    let choice = prompt_choice("  Select model", &valid_refs)?;
    let idx: usize = choice.parse::<usize>()? - 1;

    if idx < CLAUDE_MODELS.len() {
        Ok(CLAUDE_MODELS[idx].0.to_string())
    } else {
        prompt_line("  Enter Claude model ID")
    }
}

/// Interactive Gemini model selection.
fn select_gemini_model() -> Result<String, Box<dyn std::error::Error>> {
    log::info!("\n  Available Gemini models:\n");
    for (i, (id, desc)) in GEMINI_MODELS.iter().enumerate() {
        log::info!("  [{}] {} — {}", i + 1, id, desc);
    }
    log::info!("  [{}] Custom (enter model ID)\n", GEMINI_MODELS.len() + 1);

    let valid: Vec<String> = (1..=GEMINI_MODELS.len() + 1)
        .map(|i| i.to_string())
        .collect();
    let valid_refs: Vec<&str> = valid.iter().map(|s| s.as_str()).collect();
    let choice = prompt_choice("  Select model", &valid_refs)?;
    let idx: usize = choice.parse::<usize>()? - 1;

    if idx < GEMINI_MODELS.len() {
        Ok(GEMINI_MODELS[idx].0.to_string())
    } else {
        prompt_line("  Enter Gemini model ID")
    }
}

/// Recommended Ollama models known to work well with tool calling.
const RECOMMENDED_OLLAMA_MODELS: &[(&str, &str)] = &[
    ("qwen3.5:9b", "9B — strongest small model, needs ~8GB"),
    ("qwen3.5:4b", "4B — best for 8GB devices"),
    ("qwen3.5:2b", "2B — lightweight, fast"),
    ("qwen3.5:0.8b", "0.8B — ultralight, ~2GB"),
    ("qwen3-coder:latest", "Code-focused, tool calling"),
    ("llama3.2:latest", "Meta Llama 3.2, general purpose"),
];

/// Query Ollama for available models and let the user pick one.
/// Shows local models first, then recommended models that can be pulled inline.
async fn select_ollama_model() -> Result<String, Box<dyn std::error::Error>> {
    let endpoint =
        std::env::var("OLLAMA_HOST").unwrap_or_else(|_| "http://localhost:11434".to_string());
    let url = format!("{}/api/tags", endpoint.trim_end_matches('/'));

    let client = reqwest::Client::new();
    let resp = match client
        .get(&url)
        .timeout(std::time::Duration::from_secs(5))
        .send()
        .await
    {
        Ok(r) => r,
        Err(e) => {
            log::warn!("  Checking Ollama at {}... failed", endpoint);
            return Err(format!(
                "Cannot reach Ollama at {}. Is it running?\n  Error: {}",
                endpoint, e
            )
            .into());
        }
    };

    if !resp.status().is_success() {
        log::warn!(
            "  Checking Ollama at {}... failed (status {})",
            endpoint,
            resp.status()
        );
        return Err("Ollama returned an error".into());
    }
    log::info!("  Checking Ollama at {}... OK\n", endpoint);

    let body: serde_json::Value = resp.json().await?;
    let local_models: Vec<OllamaModel> = body
        .get("models")
        .and_then(|m| m.as_array())
        .map(|arr| {
            arr.iter()
                .filter_map(|m| {
                    let name = m.get("name")?.as_str()?.to_string();
                    let size = m
                        .pointer("/details/parameter_size")
                        .and_then(|s| s.as_str())
                        .unwrap_or("?")
                        .to_string();
                    let quant = m
                        .pointer("/details/quantization_level")
                        .and_then(|s| s.as_str())
                        .unwrap_or("")
                        .to_string();
                    Some(OllamaModel { name, size, quant })
                })
                .collect()
        })
        .unwrap_or_default();

    // Build unified menu: local models + recommended (not yet pulled) + custom
    let mut menu_idx: usize = 0;

    // Section 1: Local models
    if !local_models.is_empty() {
        log::info!("  Local models:\n");
        for m in &local_models {
            menu_idx += 1;
            let quant_display = if m.quant.is_empty() {
                String::new()
            } else {
                format!(", {}", m.quant)
            };
            log::info!("  [{}] {} ({}{})", menu_idx, m.name, m.size, quant_display);
        }
    }

    // Section 2: Recommended models not yet pulled
    let local_names: Vec<&str> = local_models.iter().map(|m| m.name.as_str()).collect();
    let recommended: Vec<(&str, &str)> = RECOMMENDED_OLLAMA_MODELS
        .iter()
        .filter(|(name, _)| {
            let base = name.split(':').next().unwrap_or("");
            !local_names.iter().any(|local| local.starts_with(base))
        })
        .copied()
        .collect();

    let recommended_start = menu_idx;
    if !recommended.is_empty() {
        log::info!("\n  Recommended (will be downloaded):\n");
        for &(name, desc) in &recommended {
            menu_idx += 1;
            log::info!("  [{}] \u{2B07} {} — {}", menu_idx, name, desc);
        }
    }

    // Section 3: Custom
    menu_idx += 1;
    let custom_idx = menu_idx;
    log::info!("\n  [{}] Custom (enter model name)\n", custom_idx);

    let valid: Vec<String> = (1..=custom_idx).map(|i| i.to_string()).collect();
    let valid_refs: Vec<&str> = valid.iter().map(|s| s.as_str()).collect();
    let choice = prompt_choice("  Select model", &valid_refs)?;
    let idx: usize = choice.parse::<usize>()?;

    let model_name = if idx <= local_models.len() {
        // Local model — use directly
        local_models[idx - 1].name.clone()
    } else if idx < custom_idx {
        // Recommended model — needs pull
        let rec_idx = idx - recommended_start - 1;
        let (name, _) = recommended[rec_idx];
        pull_ollama_model(name, &endpoint).await?;
        name.to_string()
    } else {
        // Custom
        let name = prompt_line("  Enter Ollama model name")?;
        pull_ollama_model(&name, &endpoint).await?;
        name
    };

    Ok(model_name)
}

/// Find the ollama binary in standard system paths.
///
/// Searches well-known directories first, then falls back to PATH with a warning.
fn find_ollama() -> std::path::PathBuf {
    let candidates = &[
        "/usr/bin/ollama",
        "/usr/local/bin/ollama",
        "/bin/ollama",
        // macOS Homebrew paths
        "/opt/homebrew/bin/ollama",
        "/usr/local/Cellar/ollama/bin/ollama",
    ];
    for path in candidates {
        let p = std::path::Path::new(path);
        if p.exists() {
            return p.to_path_buf();
        }
    }
    log::warn!("ollama not found in standard paths, falling back to PATH lookup");
    std::path::PathBuf::from("ollama")
}

/// Pull an Ollama model, showing progress via the CLI `ollama pull` command.
async fn pull_ollama_model(model: &str, _endpoint: &str) -> Result<(), Box<dyn std::error::Error>> {
    log::info!("\n  Pulling {}...\n", model);

    let ollama_bin = find_ollama();
    let status = tokio::process::Command::new(&ollama_bin)
        .args(["pull", model])
        .stdout(std::process::Stdio::inherit())
        .stderr(std::process::Stdio::inherit())
        .status()
        .await?;

    if !status.success() {
        return Err(format!("Failed to pull model '{}'. Check your connection.", model).into());
    }

    log::info!("\n  Pull complete.");
    Ok(())
}

struct OllamaModel {
    name: String,
    size: String,
    quant: String,
}

/// Best-effort daemon restart: stop via Zenoh (if reachable), then start via systemd.
async fn restart_daemon() {
    use crate::cli::daemon_client;
    use crate::cli::zenoh_session;

    // Try graceful stop via Zenoh (best-effort — daemon might not be running)
    if let Ok(session) = zenoh_session::create_zenoh_session(None).await {
        let _ = daemon_client::run_daemon_stop(session).await;
        tokio::time::sleep(std::time::Duration::from_secs(2)).await;
    }

    match daemon_client::run_daemon_start().await {
        Ok(()) => log::info!("  Restarting daemon... done."),
        Err(e) => log::warn!("  Restarting daemon... failed: {}", e),
    }
}

/// Prompt for a choice from a set of valid values.
fn prompt_choice(prompt: &str, valid: &[&str]) -> Result<String, Box<dyn std::error::Error>> {
    loop {
        log::info!("{}: ", prompt);
        let mut line = String::new();
        io::stdin().read_line(&mut line)?;
        let trimmed = line.trim();
        if valid.contains(&trimmed) {
            return Ok(trimmed.to_string());
        }
        log::info!("  Please enter one of: {}", valid.join(", "));
    }
}

/// Prompt for a non-empty line of input.
fn prompt_line(prompt: &str) -> Result<String, Box<dyn std::error::Error>> {
    loop {
        log::info!("{}: ", prompt);
        let mut line = String::new();
        io::stdin().read_line(&mut line)?;
        let trimmed = line.trim().to_string();
        if !trimmed.is_empty() {
            return Ok(trimmed);
        }
        log::info!("  Input cannot be empty.");
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    #[allow(clippy::const_is_empty)]
    fn claude_models_not_empty() {
        assert!(!CLAUDE_MODELS.is_empty());
    }

    #[test]
    fn claude_models_have_valid_ids() {
        for (id, desc) in CLAUDE_MODELS {
            assert!(
                id.starts_with("claude-"),
                "model ID should start with claude-: {}",
                id
            );
            assert!(!desc.is_empty(), "model description should not be empty");
        }
    }
}
