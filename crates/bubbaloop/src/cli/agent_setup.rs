//! Interactive agent model/provider setup wizard.
//!
//! `bubbaloop agent setup` — configure which provider and model each agent uses.
//! Writes to `~/.bubbaloop/agents.toml`. No Zenoh or daemon needed.

use crate::agent::runtime::{agent_directory, AgentEntry, AgentsConfig};
use crate::cli::login;
use std::io::{self, Write};

/// Known Claude models for interactive selection.
const CLAUDE_MODELS: &[(&str, &str)] = &[
    ("claude-sonnet-4-20250514", "Sonnet 4 (recommended)"),
    ("claude-haiku-4-5-20251001", "Haiku 4.5 (fast, cheap)"),
    ("claude-opus-4-20250514", "Opus 4 (most capable)"),
];

/// Run the interactive setup wizard.
pub async fn run_setup(target_agent: Option<&str>) -> Result<(), Box<dyn std::error::Error>> {
    let mut config = AgentsConfig::load_or_default();

    println!("\n  Agent Model Setup\n");

    // Select which agent to configure
    let is_new_agent;
    let agent_id = match target_agent {
        Some(id) => {
            is_new_agent = !config.agents.contains_key(id);
            if is_new_agent {
                println!("  Creating new agent '{}'.\n", id);
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
            println!(
                "  Current: provider={}, model={}",
                entry.provider, model_display
            );
        }
    }

    // Choose provider
    println!("\n  Choose provider:\n");
    println!("  [1] Claude API (cloud)");
    println!("  [2] Ollama (local)\n");

    let provider_choice = prompt_choice("  Enter choice (1 or 2)", &["1", "2"])?;
    let provider = if provider_choice == "1" {
        "claude"
    } else {
        "ollama"
    };

    // Nudge login if picking Claude without credentials
    if provider == "claude" && !login::has_claude_credentials() {
        println!("\n  No Claude API credentials found.");
        println!("  Run 'bubbaloop login' first to authenticate.\n");
        println!("  Continue anyway? Model config will be saved. [y/N]");
        let mut line = String::new();
        print!("  ");
        io::stdout().flush()?;
        io::stdin().read_line(&mut line)?;
        if !line.trim().eq_ignore_ascii_case("y") {
            println!("  Setup cancelled. Run 'bubbaloop login' then try again.");
            return Ok(());
        }
    }

    // Choose model based on provider
    let model = match provider {
        "ollama" => select_ollama_model().await?,
        _ => select_claude_model()?,
    };

    // Update config
    if let Some(entry) = config.agents.get_mut(&agent_id) {
        entry.provider = provider.to_string();
        entry.model = Some(model.clone());
    }

    // Save
    config.save()?;

    println!("\n  Updated agent '{}':", agent_id);
    println!("    provider: {}", provider);
    println!("    model: {}", model);
    // For new agents, ask for a quick identity description and write identity.md
    if is_new_agent {
        let soul_dir = agent_directory(&agent_id).join("soul");
        std::fs::create_dir_all(&soul_dir).ok();

        println!("\n  Describe this agent's role in one sentence");
        println!("  (e.g., \"analyzes camera images and produces captions\"):\n");
        let description = prompt_line("  Role")?;

        let identity = format!(
            "You are {}, an AI agent that {} through the Bubbaloop skill runtime.\n\n\
             Be concise. Report what you did and the result. No fluff.\n",
            agent_id, description
        );
        let identity_path = soul_dir.join("identity.md");
        std::fs::write(&identity_path, &identity)?;
        println!("  Wrote identity to {}", identity_path.display());
    }

    println!("\n  Saved to ~/.bubbaloop/agents.toml");

    // Offer to restart the daemon so changes take effect immediately
    println!("\n  Restart daemon for changes to take effect? [Y/n]");
    let mut line = String::new();
    print!("  ");
    io::stdout().flush()?;
    io::stdin().read_line(&mut line)?;
    let should_restart = line.trim().is_empty() || line.trim().eq_ignore_ascii_case("y");

    if should_restart {
        restart_daemon().await;
        println!("\n  Chat with your agent:");
        println!("    bubbaloop agent chat -a {}", agent_id);
    } else if is_new_agent {
        println!("  When ready, restart the daemon then chat:");
        println!("    bubbaloop daemon restart");
        println!("    bubbaloop agent chat -a {}", agent_id);
    } else {
        println!("  Run 'bubbaloop daemon restart' for changes to take effect.");
    }
    println!();

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
        println!("  Agent: {}", id);
        return Ok(id);
    }

    println!("  Available agents:\n");
    for (i, (id, entry)) in agents.iter().enumerate() {
        let default_tag = if entry.default { " (default)" } else { "" };
        let model_display = entry.model.as_deref().unwrap_or("soul default");
        println!(
            "  [{}] {}{} — {} / {}",
            i + 1,
            id,
            default_tag,
            entry.provider,
            model_display
        );
    }
    println!();

    let valid: Vec<String> = (1..=agents.len()).map(|i| i.to_string()).collect();
    let valid_refs: Vec<&str> = valid.iter().map(|s| s.as_str()).collect();
    let choice = prompt_choice("  Select agent", &valid_refs)?;
    let idx: usize = choice.parse::<usize>()? - 1;
    Ok(agents[idx].0.clone())
}

/// Interactive Claude model selection.
fn select_claude_model() -> Result<String, Box<dyn std::error::Error>> {
    println!("\n  Available Claude models:\n");
    for (i, (id, desc)) in CLAUDE_MODELS.iter().enumerate() {
        println!("  [{}] {} — {}", i + 1, id, desc);
    }
    println!("  [{}] Custom (enter model ID)\n", CLAUDE_MODELS.len() + 1);

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

    print!("  Checking Ollama at {}... ", endpoint);
    io::stdout().flush()?;

    let client = reqwest::Client::new();
    let resp = match client
        .get(&url)
        .timeout(std::time::Duration::from_secs(5))
        .send()
        .await
    {
        Ok(r) => r,
        Err(e) => {
            println!("failed");
            return Err(format!(
                "Cannot reach Ollama at {}. Is it running?\n  Error: {}",
                endpoint, e
            )
            .into());
        }
    };

    if !resp.status().is_success() {
        println!("failed (status {})", resp.status());
        return Err("Ollama returned an error".into());
    }

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

    println!("OK\n");

    // Build unified menu: local models + recommended (not yet pulled) + custom
    let mut menu_idx: usize = 0;

    // Section 1: Local models
    if !local_models.is_empty() {
        println!("  Local models:\n");
        for m in &local_models {
            menu_idx += 1;
            let quant_display = if m.quant.is_empty() {
                String::new()
            } else {
                format!(", {}", m.quant)
            };
            println!("  [{}] {} ({}{})", menu_idx, m.name, m.size, quant_display);
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
        println!("\n  Recommended (will be downloaded):\n");
        for &(name, desc) in &recommended {
            menu_idx += 1;
            println!("  [{}] \u{2B07} {} — {}", menu_idx, name, desc);
        }
    }

    // Section 3: Custom
    menu_idx += 1;
    let custom_idx = menu_idx;
    println!("\n  [{}] Custom (enter model name)\n", custom_idx);

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
    println!("\n  Pulling {}...\n", model);

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

    println!("\n  Pull complete.");
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

    print!("  Restarting daemon... ");
    io::stdout().flush().ok();

    // Try graceful stop via Zenoh (best-effort — daemon might not be running)
    if let Ok(session) = zenoh_session::create_zenoh_session(None).await {
        let _ = daemon_client::run_daemon_stop(session).await;
        tokio::time::sleep(std::time::Duration::from_secs(2)).await;
    }

    match daemon_client::run_daemon_start().await {
        Ok(()) => println!("done."),
        Err(e) => eprintln!("failed: {}", e),
    }
}

/// Prompt for a choice from a set of valid values.
fn prompt_choice(prompt: &str, valid: &[&str]) -> Result<String, Box<dyn std::error::Error>> {
    loop {
        print!("{}: ", prompt);
        io::stdout().flush()?;
        let mut line = String::new();
        io::stdin().read_line(&mut line)?;
        let trimmed = line.trim();
        if valid.contains(&trimmed) {
            return Ok(trimmed.to_string());
        }
        println!("  Please enter one of: {}", valid.join(", "));
    }
}

/// Prompt for a non-empty line of input.
fn prompt_line(prompt: &str) -> Result<String, Box<dyn std::error::Error>> {
    loop {
        print!("{}: ", prompt);
        io::stdout().flush()?;
        let mut line = String::new();
        io::stdin().read_line(&mut line)?;
        let trimmed = line.trim().to_string();
        if !trimmed.is_empty() {
            return Ok(trimmed);
        }
        println!("  Input cannot be empty.");
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
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
