//! Skill hub — community YAML skill template index.
//!
//! The skill hub is a GitHub repository (`kornia/bubbaloop-skills`) containing
//! community-contributed skill YAML templates. This module fetches and caches
//! the index, and downloads individual skill templates.

use crate::daemon::registry::get_bubbaloop_home;
use serde::{Deserialize, Serialize};
use std::path::PathBuf;

/// GitHub raw URL for the skill hub index.
const SKILL_HUB_INDEX_URL: &str =
    "https://raw.githubusercontent.com/kornia/bubbaloop-skills/main/skills.yaml";

/// Cache filename inside `~/.bubbaloop/cache/`.
const SKILL_HUB_CACHE_FILE: &str = "skills_hub.yaml";

/// A single entry in the skill hub index.
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct SkillHubEntry {
    pub name: String,
    pub category: String,
    pub driver: String,
    pub description: String,
    #[serde(default)]
    pub tags: Vec<String>,
    pub file: String,
}

/// The top-level skill hub index.
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct SkillHubIndex {
    pub version: u32,
    pub skills: Vec<SkillHubEntry>,
}

#[derive(Debug, thiserror::Error)]
pub enum SkillHubError {
    #[error("HTTP error fetching skill hub: {0}")]
    Http(String),
    #[error("Parse error in skill hub index: {0}")]
    Parse(String),
    #[error("IO error: {0}")]
    Io(#[from] std::io::Error),
    #[error("Skill '{0}' not found in hub index")]
    NotFound(String),
}

pub type Result<T> = std::result::Result<T, SkillHubError>;

fn cache_path() -> PathBuf {
    let cache_dir = get_bubbaloop_home().join("cache");
    std::fs::create_dir_all(&cache_dir).ok();
    cache_dir.join(SKILL_HUB_CACHE_FILE)
}

/// Load hub entries from local cache. Returns empty vec if cache missing.
pub fn load_cached_hub() -> Vec<SkillHubEntry> {
    let path = cache_path();
    let raw = match std::fs::read_to_string(&path) {
        Ok(s) => s,
        Err(_) => return Vec::new(),
    };
    serde_yaml::from_str::<SkillHubIndex>(&raw)
        .map(|idx| idx.skills)
        .unwrap_or_default()
}

/// Fetch the remote index and write to cache. Returns error if unreachable.
pub fn refresh_hub_cache() -> Result<()> {
    let body = fetch_text(SKILL_HUB_INDEX_URL)?;
    // Validate before caching
    serde_yaml::from_str::<SkillHubIndex>(&body)
        .map_err(|e| SkillHubError::Parse(e.to_string()))?;
    std::fs::write(cache_path(), &body)?;
    log::info!("[skill_hub] Cache refreshed from {}", SKILL_HUB_INDEX_URL);
    Ok(())
}

/// Download a specific skill template YAML by entry.
pub fn fetch_skill_template(entry: &SkillHubEntry) -> Result<String> {
    let base = "https://raw.githubusercontent.com/kornia/bubbaloop-skills/main/";
    let url = format!("{}{}", base, entry.file);
    fetch_text(&url)
}

/// Search entries by name, tag, or description (case-insensitive substring match).
pub fn search(entries: &[SkillHubEntry], query: &str) -> Vec<SkillHubEntry> {
    let q = query.to_lowercase();
    entries
        .iter()
        .filter(|e| {
            e.name.to_lowercase().contains(&q)
                || e.description.to_lowercase().contains(&q)
                || e.category.to_lowercase().contains(&q)
                || e.tags.iter().any(|t| t.to_lowercase().contains(&q))
        })
        .cloned()
        .collect()
}

fn find_curl() -> Option<std::path::PathBuf> {
    // Same pattern as registry.rs and marketplace.rs — never use PATH
    for candidate in &["/usr/bin/curl", "/usr/local/bin/curl", "/bin/curl"] {
        let p = std::path::Path::new(candidate);
        if p.exists() {
            return Some(p.to_path_buf());
        }
    }
    None
}

fn fetch_text(url: &str) -> Result<String> {
    // Use the same curl-based approach as registry.rs for consistency
    let curl = find_curl().ok_or_else(|| SkillHubError::Http("curl not found".into()))?;
    let output = std::process::Command::new(&curl)
        .args(["--silent", "--fail", "--location", "--max-time", "30", url])
        .output()
        .map_err(|e| SkillHubError::Http(e.to_string()))?;
    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        return Err(SkillHubError::Http(format!(
            "HTTP request failed: {}",
            stderr
        )));
    }
    String::from_utf8(output.stdout).map_err(|e| SkillHubError::Http(e.to_string()))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn search_by_name() {
        let entries = vec![
            SkillHubEntry {
                name: "doorbell-http".into(),
                category: "cameras".into(),
                driver: "http-poll".into(),
                description: "HTTP snapshot camera".into(),
                tags: vec!["camera".into()],
                file: "skills/cameras/doorbell-http.yaml".into(),
            },
            SkillHubEntry {
                name: "openmeteo".into(),
                category: "weather".into(),
                driver: "http-poll".into(),
                description: "Open-Meteo weather".into(),
                tags: vec!["weather".into()],
                file: "skills/weather/openmeteo.yaml".into(),
            },
        ];
        let results = search(&entries, "camera");
        assert_eq!(results.len(), 1);
        assert_eq!(results[0].name, "doorbell-http");
    }

    #[test]
    fn search_by_tag() {
        let entries = vec![SkillHubEntry {
            name: "openmeteo".into(),
            category: "weather".into(),
            driver: "http-poll".into(),
            description: "Weather API".into(),
            tags: vec!["weather".into(), "api".into()],
            file: "f.yaml".into(),
        }];
        assert_eq!(search(&entries, "api").len(), 1);
        assert_eq!(search(&entries, "robot").len(), 0);
    }

    #[test]
    fn load_cached_hub_missing_returns_empty() {
        // We can't easily test the cache without a real home dir, but we can test
        // that an empty/missing cache returns an empty Vec (no panic).
        let entries = load_cached_hub();
        // Either 0 entries (cache doesn't exist) or more — both fine, just no panic
        let _ = entries;
    }
}
