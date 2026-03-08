//! Context providers — daemon background tasks that populate Tier 0 world state.
//!
//! Each provider subscribes to a Zenoh topic pattern, applies a filter,
//! and writes to the agent's SemanticStore world_state table.
//! No LLM involvement. Pure data pipeline.

use rusqlite::{params, Connection};
use std::path::Path;

/// Configuration for a single context provider.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct ProviderConfig {
    /// Unique provider ID (e.g. "cp-<uuid>").
    pub id: String,
    /// Mission this provider is attached to.
    pub mission_id: String,
    /// Zenoh key expression pattern (e.g. "bubbaloop/**/vision/detections").
    pub topic_pattern: String,
    /// Template for world state key (e.g. "{label}.location").
    pub world_state_key_template: String,
    /// JSON field path to extract as the value.
    pub value_field: String,
    /// Optional filter expression (e.g. "label=dog AND confidence>0.85").
    pub filter: Option<String>,
    /// Minimum interval between writes for the same key (seconds).
    pub min_interval_secs: u32,
    /// Maximum age before a world state entry is considered stale (seconds).
    pub max_age_secs: u32,
    /// Optional JSON field path to extract confidence from.
    pub confidence_field: Option<String>,
    /// Approximate token budget for this provider's world state entries.
    pub token_budget: u32,
}

/// SQLite-backed store for context provider configurations.
pub struct ProviderStore {
    conn: Connection,
}

impl ProviderStore {
    /// Open (or create) the provider store at the given path.
    pub fn open(path: &Path) -> anyhow::Result<Self> {
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent)?;
        }

        let conn = Connection::open(path)?;

        // WAL mode + busy timeout
        conn.query_row("PRAGMA journal_mode=WAL", [], |_| Ok(()))?;
        conn.query_row("PRAGMA busy_timeout=5000", [], |_| Ok(()))?;

        conn.execute_batch(
            "CREATE TABLE IF NOT EXISTS context_providers (
                id                      TEXT PRIMARY KEY,
                mission_id              TEXT NOT NULL,
                topic_pattern           TEXT NOT NULL,
                world_state_key_template TEXT NOT NULL,
                value_field             TEXT NOT NULL,
                filter                  TEXT,
                min_interval_secs       INTEGER NOT NULL DEFAULT 30,
                max_age_secs            INTEGER NOT NULL DEFAULT 300,
                confidence_field        TEXT,
                token_budget            INTEGER NOT NULL DEFAULT 50,
                created_at              INTEGER NOT NULL DEFAULT (strftime('%s','now'))
            );",
        )?;

        Ok(Self { conn })
    }

    /// Save (insert or replace) a provider configuration.
    pub fn save_provider(&self, cfg: &ProviderConfig) -> anyhow::Result<()> {
        self.conn.execute(
            "INSERT OR REPLACE INTO context_providers \
             (id, mission_id, topic_pattern, world_state_key_template, value_field, \
              filter, min_interval_secs, max_age_secs, confidence_field, token_budget) \
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10)",
            params![
                cfg.id,
                cfg.mission_id,
                cfg.topic_pattern,
                cfg.world_state_key_template,
                cfg.value_field,
                cfg.filter,
                cfg.min_interval_secs,
                cfg.max_age_secs,
                cfg.confidence_field,
                cfg.token_budget,
            ],
        )?;
        Ok(())
    }

    /// List all provider configurations.
    pub fn list_providers(&self) -> anyhow::Result<Vec<ProviderConfig>> {
        let mut stmt = self.conn.prepare(
            "SELECT id, mission_id, topic_pattern, world_state_key_template, value_field, \
             filter, min_interval_secs, max_age_secs, confidence_field, token_budget \
             FROM context_providers ORDER BY id ASC",
        )?;
        let rows = stmt.query_map([], |row| {
            Ok(ProviderConfig {
                id: row.get(0)?,
                mission_id: row.get(1)?,
                topic_pattern: row.get(2)?,
                world_state_key_template: row.get(3)?,
                value_field: row.get(4)?,
                filter: row.get(5)?,
                min_interval_secs: row.get(6)?,
                max_age_secs: row.get(7)?,
                confidence_field: row.get(8)?,
                token_budget: row.get(9)?,
            })
        })?;
        rows.collect::<Result<Vec<_>, _>>().map_err(Into::into)
    }

    /// Delete a provider by ID.
    pub fn delete_provider(&self, id: &str) -> anyhow::Result<()> {
        self.conn
            .execute("DELETE FROM context_providers WHERE id = ?1", params![id])?;
        Ok(())
    }

    /// List providers for a specific mission.
    pub fn providers_for_mission(&self, mission_id: &str) -> anyhow::Result<Vec<ProviderConfig>> {
        let mut stmt = self.conn.prepare(
            "SELECT id, mission_id, topic_pattern, world_state_key_template, value_field, \
             filter, min_interval_secs, max_age_secs, confidence_field, token_budget \
             FROM context_providers WHERE mission_id = ?1 ORDER BY id ASC",
        )?;
        let rows = stmt.query_map(params![mission_id], |row| {
            Ok(ProviderConfig {
                id: row.get(0)?,
                mission_id: row.get(1)?,
                topic_pattern: row.get(2)?,
                world_state_key_template: row.get(3)?,
                value_field: row.get(4)?,
                filter: row.get(5)?,
                min_interval_secs: row.get(6)?,
                max_age_secs: row.get(7)?,
                confidence_field: row.get(8)?,
                token_budget: row.get(9)?,
            })
        })?;
        rows.collect::<Result<Vec<_>, _>>().map_err(Into::into)
    }
}

// ── Filter, template, and field extraction utilities ────────────────

/// Minimal filter evaluator for "field=value AND field2>number" expressions.
/// Supports =, !=, >, <, >=, <= operators. AND conjunction only.
pub fn apply_filter(filter: &str, sample: &serde_json::Value) -> bool {
    for clause in filter.split(" AND ") {
        let clause = clause.trim();
        if clause.is_empty() {
            continue;
        }

        // Parse operator (order matters: >= before >, <= before <, != before =)
        let (field, op, expected) = if let Some(pos) = clause.find("!=") {
            (&clause[..pos], "!=", clause[pos + 2..].trim())
        } else if let Some(pos) = clause.find(">=") {
            (&clause[..pos], ">=", clause[pos + 2..].trim())
        } else if let Some(pos) = clause.find("<=") {
            (&clause[..pos], "<=", clause[pos + 2..].trim())
        } else if let Some(pos) = clause.find('>') {
            (&clause[..pos], ">", clause[pos + 1..].trim())
        } else if let Some(pos) = clause.find('<') {
            (&clause[..pos], "<", clause[pos + 1..].trim())
        } else if let Some(pos) = clause.find('=') {
            (&clause[..pos], "=", clause[pos + 1..].trim())
        } else {
            // Unparseable clause — skip (fail open for robustness)
            continue;
        };

        let field = field.trim();
        let actual = match sample.get(field) {
            Some(v) => v,
            None => return false,
        };

        // Try numeric comparison first
        let expected_num = expected.parse::<f64>();
        let actual_num = actual
            .as_f64()
            .or_else(|| actual.as_str().and_then(|s| s.parse::<f64>().ok()));

        if let (Ok(exp), Some(act)) = (expected_num, actual_num) {
            let pass = match op {
                "=" => (act - exp).abs() < f64::EPSILON,
                "!=" => (act - exp).abs() >= f64::EPSILON,
                ">" => act > exp,
                "<" => act < exp,
                ">=" => act >= exp,
                "<=" => act <= exp,
                _ => true,
            };
            if !pass {
                return false;
            }
        } else {
            // String comparison
            let actual_str = actual
                .as_str()
                .map(|s| s.to_string())
                .unwrap_or_else(|| actual.to_string());
            let pass = match op {
                "=" => actual_str == expected,
                "!=" => actual_str != expected,
                _ => false, // >, <, >=, <= not meaningful for strings
            };
            if !pass {
                return false;
            }
        }
    }
    true
}

/// Replace {field} placeholders in template with values from sample.
pub fn resolve_key_template(template: &str, sample: &serde_json::Value) -> String {
    let mut result = template.to_string();
    // Find all {field} patterns and replace them
    while let Some(start) = result.find('{') {
        if let Some(end) = result[start..].find('}') {
            let field = &result[start + 1..start + end];
            let replacement = sample
                .get(field)
                .and_then(|v| v.as_str().map(|s| s.to_string()))
                .unwrap_or_else(|| {
                    sample
                        .get(field)
                        .map(|v| v.to_string().trim_matches('"').to_string())
                        .unwrap_or_else(|| field.to_string())
                });
            result = format!(
                "{}{}{}",
                &result[..start],
                replacement,
                &result[start + end + 1..]
            );
        } else {
            break;
        }
    }
    result
}

/// Extract a field value from a JSON sample as a string.
pub fn extract_field(field: &str, sample: &serde_json::Value) -> Option<String> {
    sample.get(field).map(|v| {
        if let Some(s) = v.as_str() {
            s.to_string()
        } else {
            v.to_string()
        }
    })
}

/// Spawn a background task that subscribes to a Zenoh topic and writes to world state.
///
/// Each provider opens its own `SemanticStore` connection to the same database file.
/// WAL mode supports concurrent writers serialized by SQLite.
pub fn spawn_provider(
    cfg: ProviderConfig,
    session: std::sync::Arc<zenoh::Session>,
    semantic_db_path: std::path::PathBuf,
    mut shutdown: tokio::sync::watch::Receiver<()>,
) -> tokio::task::JoinHandle<()> {
    tokio::spawn(async move {
        log::info!(
            "[ContextProvider] Starting provider '{}' on topic '{}' (mission={})",
            cfg.id,
            cfg.topic_pattern,
            cfg.mission_id
        );

        // Open our own SemanticStore connection
        let store = match crate::agent::memory::semantic::SemanticStore::open(&semantic_db_path) {
            Ok(s) => s,
            Err(e) => {
                log::error!(
                    "[ContextProvider] Failed to open SemanticStore for provider '{}': {}",
                    cfg.id,
                    e
                );
                return;
            }
        };

        // Subscribe to the Zenoh topic
        let subscriber = match session.declare_subscriber(&cfg.topic_pattern).await {
            Ok(s) => s,
            Err(e) => {
                log::error!(
                    "[ContextProvider] Failed to subscribe to '{}' for provider '{}': {}",
                    cfg.topic_pattern,
                    cfg.id,
                    e
                );
                return;
            }
        };

        // Rate-limiting: track last write time per resolved key
        let mut last_write: std::collections::HashMap<String, u64> =
            std::collections::HashMap::new();
        let min_interval = u64::from(cfg.min_interval_secs);
        let max_age = i64::from(cfg.max_age_secs);
        let default_confidence = 1.0f64;

        loop {
            tokio::select! {
                result = subscriber.recv_async() => {
                    let sample = match result {
                        Ok(s) => s,
                        Err(_) => break,
                    };

                    // Parse UTF-8 payload
                    let bytes = sample.payload().to_bytes();
                    let text = match String::from_utf8(bytes.to_vec()) {
                        Ok(t) => t,
                        Err(_) => continue,
                    };

                    // Parse JSON
                    let json: serde_json::Value = match serde_json::from_str(&text) {
                        Ok(v) => v,
                        Err(_) => continue,
                    };

                    // Apply filter
                    if let Some(ref filter_expr) = cfg.filter {
                        if !apply_filter(filter_expr, &json) {
                            continue;
                        }
                    }

                    // Resolve the world state key
                    let resolved_key = resolve_key_template(&cfg.world_state_key_template, &json);

                    // Rate-limit by resolved key
                    let now_secs = crate::agent::memory::now_epoch_secs();
                    if let Some(&last) = last_write.get(&resolved_key) {
                        if now_secs.saturating_sub(last) < min_interval {
                            continue;
                        }
                    }

                    // Extract value
                    let value = match extract_field(&cfg.value_field, &json) {
                        Some(v) => v,
                        None => continue,
                    };

                    // Extract confidence
                    let confidence = cfg
                        .confidence_field
                        .as_ref()
                        .and_then(|f| {
                            json.get(f.as_str()).and_then(|v| v.as_f64())
                        })
                        .unwrap_or(default_confidence);

                    let source_topic = Some(sample.key_expr().to_string());
                    let source_node: Option<&str> = None;

                    // Write to world state (blocking rusqlite call)
                    let write_result = tokio::task::block_in_place(|| {
                        store.upsert_world_state(
                            &resolved_key,
                            &value,
                            confidence,
                            source_topic.as_deref(),
                            source_node,
                            max_age,
                        )
                    });

                    if let Err(e) = write_result {
                        log::warn!(
                            "[ContextProvider] Failed to write world state for key '{}': {}",
                            resolved_key,
                            e
                        );
                    } else {
                        last_write.insert(resolved_key, now_secs);
                    }
                }
                _ = shutdown.changed() => {
                    break;
                }
            }
        }

        log::info!("[ContextProvider] Provider '{}' shutting down", cfg.id);
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn provider_config_roundtrips_to_db() {
        let dir = tempfile::tempdir().unwrap();
        let store = ProviderStore::open(&dir.path().join("providers.db")).unwrap();

        let cfg = ProviderConfig {
            id: "cp-test-1".to_string(),
            mission_id: "mission-1".to_string(),
            topic_pattern: "bubbaloop/**/detections".to_string(),
            world_state_key_template: "{label}.location".to_string(),
            value_field: "location".to_string(),
            filter: Some("confidence>0.8".to_string()),
            min_interval_secs: 15,
            max_age_secs: 120,
            confidence_field: Some("confidence".to_string()),
            token_budget: 100,
        };

        store.save_provider(&cfg).unwrap();
        let providers = store.list_providers().unwrap();

        assert_eq!(providers.len(), 1);
        assert_eq!(providers[0].id, "cp-test-1");
        assert_eq!(providers[0].min_interval_secs, 15);
        assert_eq!(providers[0].mission_id, "mission-1");
        assert_eq!(providers[0].topic_pattern, "bubbaloop/**/detections");
    }

    #[test]
    fn delete_provider_removes_it() {
        let dir = tempfile::tempdir().unwrap();
        let store = ProviderStore::open(&dir.path().join("providers.db")).unwrap();

        let cfg = ProviderConfig {
            id: "cp-delete-me".to_string(),
            mission_id: "mission-1".to_string(),
            topic_pattern: "bubbaloop/**/status".to_string(),
            world_state_key_template: "node.status".to_string(),
            value_field: "status".to_string(),
            filter: None,
            min_interval_secs: 30,
            max_age_secs: 300,
            confidence_field: None,
            token_budget: 50,
        };

        store.save_provider(&cfg).unwrap();
        assert_eq!(store.list_providers().unwrap().len(), 1);

        store.delete_provider("cp-delete-me").unwrap();
        assert!(store.list_providers().unwrap().is_empty());
    }

    #[test]
    fn providers_for_mission_filters_correctly() {
        let dir = tempfile::tempdir().unwrap();
        let store = ProviderStore::open(&dir.path().join("providers.db")).unwrap();

        let cfg1 = ProviderConfig {
            id: "cp-1".to_string(),
            mission_id: "m1".to_string(),
            topic_pattern: "bubbaloop/**/a".to_string(),
            world_state_key_template: "a".to_string(),
            value_field: "v".to_string(),
            filter: None,
            min_interval_secs: 30,
            max_age_secs: 300,
            confidence_field: None,
            token_budget: 50,
        };
        let cfg2 = ProviderConfig {
            id: "cp-2".to_string(),
            mission_id: "m2".to_string(),
            ..cfg1.clone()
        };

        store.save_provider(&cfg1).unwrap();
        store.save_provider(&cfg2).unwrap();

        let m1_providers = store.providers_for_mission("m1").unwrap();
        assert_eq!(m1_providers.len(), 1);
        assert_eq!(m1_providers[0].id, "cp-1");
    }

    #[test]
    fn filter_matches_correctly() {
        let sample = serde_json::json!({
            "label": "dog",
            "confidence": 0.92,
            "count": 3
        });

        // Simple equality
        assert!(apply_filter("label=dog", &sample));
        // Greater than
        assert!(apply_filter("confidence>0.85", &sample));
        // AND conjunction
        assert!(apply_filter("label=dog AND confidence>0.85", &sample));
        // Greater-equal
        assert!(apply_filter("count>=3", &sample));
    }

    #[test]
    fn filter_rejects_non_matching() {
        let sample = serde_json::json!({
            "label": "cat",
            "confidence": 0.6,
        });

        // Wrong value
        assert!(!apply_filter("label=dog", &sample));
        // Wrong numeric threshold
        assert!(!apply_filter("confidence>0.85", &sample));
        // Missing field
        assert!(!apply_filter("missing_field=hello", &sample));
    }

    #[test]
    fn key_template_substitution() {
        let sample = serde_json::json!({"label": "dog"});
        let result = resolve_key_template("{label}.location", &sample);
        assert_eq!(result, "dog.location");
    }

    #[test]
    fn key_template_multiple_fields() {
        let sample = serde_json::json!({"label": "dog", "zone": "yard"});
        let result = resolve_key_template("{label}.{zone}", &sample);
        assert_eq!(result, "dog.yard");
    }

    #[test]
    fn value_extraction_from_json_path() {
        let sample = serde_json::json!({"location": "front_yard", "count": 42});
        assert_eq!(
            extract_field("location", &sample),
            Some("front_yard".to_string())
        );
        assert_eq!(extract_field("count", &sample), Some("42".to_string()));
        assert_eq!(extract_field("missing", &sample), None);
    }

    #[test]
    fn filter_not_equal() {
        let sample = serde_json::json!({"label": "dog"});
        assert!(apply_filter("label!=cat", &sample));
        assert!(!apply_filter("label!=dog", &sample));
    }

    #[test]
    fn filter_less_than() {
        let sample = serde_json::json!({"confidence": 0.5});
        assert!(apply_filter("confidence<0.8", &sample));
        assert!(!apply_filter("confidence<0.3", &sample));
    }

    #[test]
    fn filter_less_equal() {
        let sample = serde_json::json!({"count": 5});
        assert!(apply_filter("count<=5", &sample));
        assert!(apply_filter("count<=10", &sample));
        assert!(!apply_filter("count<=4", &sample));
    }
}
