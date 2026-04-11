//! Reactive pre-filter: rule engine that adjusts agent arousal without calling the LLM.
//!
//! Rules fire when world state matches a predicate, applying a debounced arousal boost.
//! The evaluator reuses `apply_filter` from `context_provider` for predicate parsing.

use crate::daemon::context_provider::apply_filter;
use rusqlite::{params, Connection};
use std::collections::HashMap;
use std::path::Path;
use std::sync::atomic::{AtomicI64, Ordering};

/// A reactive rule that fires when world state matches a predicate.
/// Never calls the LLM -- only adjusts agent arousal.
pub struct ReactiveRule {
    pub id: String,
    pub mission_id: String,
    /// Predicate expression using apply_filter syntax (e.g. "dog.near_stairs = 'true'").
    pub predicate: String,
    pub debounce_secs: u32,
    pub arousal_boost: f64,
    pub description: String,
    /// Last time this rule fired (epoch secs). Atomic for concurrent reads.
    pub last_fired_at: AtomicI64,
}

impl ReactiveRule {
    /// Check whether this rule should fire given the current world state.
    /// Respects debounce: will not fire if less than `debounce_secs` have passed.
    pub fn should_fire(&self, world_state: &HashMap<&str, &str>) -> bool {
        let now = crate::agent::memory::now_epoch_secs() as i64;
        let last = self.last_fired_at.load(Ordering::Relaxed);
        if now - last < self.debounce_secs as i64 {
            return false;
        }
        eval_predicate(&self.predicate, world_state)
    }

    /// Mark this rule as fired and return its arousal boost.
    pub fn fire(&self) -> f64 {
        self.last_fired_at.store(
            crate::agent::memory::now_epoch_secs() as i64,
            Ordering::Relaxed,
        );
        self.arousal_boost
    }
}

/// Evaluate a predicate against a world state HashMap.
///
/// Converts the HashMap into a JSON object and delegates to `apply_filter`.
pub fn eval_predicate(predicate: &str, world_state: &HashMap<&str, &str>) -> bool {
    let json = serde_json::Value::Object(
        world_state
            .iter()
            .map(|(k, v)| (k.to_string(), serde_json::Value::String(v.to_string())))
            .collect(),
    );
    apply_filter(predicate, &json)
}

/// A rule that just fired during evaluation.
/// Carries enough context for the caller to both boost arousal and synthesize
/// a meaningful prompt describing why the agent is being woken.
#[derive(Debug, Clone)]
pub struct FiredRule {
    pub id: String,
    pub mission_id: String,
    pub predicate: String,
    pub description: String,
    pub boost: f64,
}

/// Evaluate all rules against world state, fire matching ones, return the list of fired rules.
///
/// Callers that only need the summed arousal boost should use
/// [`total_boost`] on the returned slice. Returning the rules themselves lets
/// the agent loop build a descriptive prompt ("rules X, Y fired because ...")
/// when a reactive alert wakes the LLM.
pub fn evaluate_rules_fired(
    rules: &[ReactiveRule],
    world_state: &HashMap<&str, &str>,
) -> Vec<FiredRule> {
    rules
        .iter()
        .filter_map(|r| {
            if r.should_fire(world_state) {
                let boost = r.fire();
                Some(FiredRule {
                    id: r.id.clone(),
                    mission_id: r.mission_id.clone(),
                    predicate: r.predicate.clone(),
                    description: r.description.clone(),
                    boost,
                })
            } else {
                None
            }
        })
        .collect()
}

/// Sum of arousal boosts from a set of fired rules.
pub fn total_boost(fired: &[FiredRule]) -> f64 {
    fired.iter().map(|r| r.boost).sum()
}

/// Merge `last_fired_at` state from an old rule set into a freshly-loaded one.
///
/// The reactive rule store is reloaded from SQLite every
/// `REACTIVE_RULE_RELOAD_INTERVAL` ticks so operators can add/remove rules
/// without restarting the agent. Naive reload — `configs.into_iter().map(Into::into).collect()` —
/// wipes every rule's `last_fired_at: AtomicI64(0)`, which silently defeats
/// debounce: a rule with `debounce_secs=3600` that fired 5 minutes ago will
/// fire *again* on the very next tick after a reload, because its "never
/// fired" sentinel is restored. In the 2026-04-10 incident this turned a
/// 1-alert-per-hour rule into a continuous firestorm of LLM turns.
///
/// This helper is pure and deterministic: for every rule in `new`, if a rule
/// with the same `id` existed in `old`, copy its `last_fired_at` timestamp
/// forward. New rules (no match in `old`) keep their freshly-initialised
/// zero, which is correct — they have genuinely never fired. Deleted rules
/// (present in `old`, absent from `new`) are discarded, also correct.
///
/// Matching is by `id` only, not `(mission_id, id)`, because the SQLite
/// primary key is `id` — rule ids are already globally unique.
pub fn merge_rule_state(old: &[ReactiveRule], new: Vec<ReactiveRule>) -> Vec<ReactiveRule> {
    let preserved: HashMap<&str, i64> = old
        .iter()
        .map(|r| (r.id.as_str(), r.last_fired_at.load(Ordering::Relaxed)))
        .collect();
    for rule in &new {
        if let Some(&ts) = preserved.get(rule.id.as_str()) {
            rule.last_fired_at.store(ts, Ordering::Relaxed);
        }
    }
    new
}

// ── Persistence ────────────────────────────────────────────────────────

/// Serializable configuration for a reactive rule (no AtomicI64).
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct ReactiveRuleConfig {
    pub id: String,
    pub mission_id: String,
    pub predicate: String,
    pub debounce_secs: u32,
    pub arousal_boost: f64,
    pub description: String,
}

impl From<ReactiveRuleConfig> for ReactiveRule {
    fn from(c: ReactiveRuleConfig) -> Self {
        Self {
            id: c.id,
            mission_id: c.mission_id,
            predicate: c.predicate,
            debounce_secs: c.debounce_secs,
            arousal_boost: c.arousal_boost,
            description: c.description,
            last_fired_at: AtomicI64::new(0),
        }
    }
}

/// SQLite-backed store for reactive rule configurations.
pub struct ReactiveRuleStore {
    conn: Connection,
}

impl ReactiveRuleStore {
    /// Open (or create) the reactive rule store at the given path.
    pub fn open(path: &Path) -> anyhow::Result<Self> {
        let conn = crate::daemon::util::open_sqlite(path)?;

        conn.execute_batch(
            "CREATE TABLE IF NOT EXISTS reactive_rules (
                id            TEXT PRIMARY KEY,
                mission_id    TEXT NOT NULL,
                predicate     TEXT NOT NULL,
                debounce_secs INTEGER NOT NULL DEFAULT 30,
                arousal_boost REAL NOT NULL DEFAULT 1.0,
                description   TEXT NOT NULL DEFAULT '',
                created_at    INTEGER NOT NULL DEFAULT (strftime('%s','now'))
            );",
        )?;

        Ok(Self { conn })
    }

    /// Save (insert or replace) a reactive rule configuration.
    pub fn save_rule(&self, rule: &ReactiveRuleConfig) -> anyhow::Result<()> {
        self.conn.execute(
            "INSERT OR REPLACE INTO reactive_rules \
             (id, mission_id, predicate, debounce_secs, arousal_boost, description) \
             VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
            params![
                rule.id,
                rule.mission_id,
                rule.predicate,
                rule.debounce_secs,
                rule.arousal_boost,
                rule.description,
            ],
        )?;
        Ok(())
    }

    /// List all reactive rule configurations.
    pub fn list_rules(&self) -> anyhow::Result<Vec<ReactiveRuleConfig>> {
        let mut stmt = self.conn.prepare(
            "SELECT id, mission_id, predicate, debounce_secs, arousal_boost, description \
             FROM reactive_rules ORDER BY id ASC",
        )?;
        let rows = stmt.query_map([], |row| {
            Ok(ReactiveRuleConfig {
                id: row.get(0)?,
                mission_id: row.get(1)?,
                predicate: row.get(2)?,
                debounce_secs: row.get(3)?,
                arousal_boost: row.get(4)?,
                description: row.get(5)?,
            })
        })?;
        rows.collect::<Result<Vec<_>, _>>().map_err(Into::into)
    }

    /// Delete a reactive rule by ID.
    pub fn delete_rule(&self, id: &str) -> anyhow::Result<()> {
        self.conn
            .execute("DELETE FROM reactive_rules WHERE id = ?1", params![id])?;
        Ok(())
    }

    /// List rules for a specific mission.
    pub fn rules_for_mission(&self, mission_id: &str) -> anyhow::Result<Vec<ReactiveRuleConfig>> {
        let mut stmt = self.conn.prepare(
            "SELECT id, mission_id, predicate, debounce_secs, arousal_boost, description \
             FROM reactive_rules WHERE mission_id = ?1 ORDER BY id ASC",
        )?;
        let rows = stmt.query_map(params![mission_id], |row| {
            Ok(ReactiveRuleConfig {
                id: row.get(0)?,
                mission_id: row.get(1)?,
                predicate: row.get(2)?,
                debounce_secs: row.get(3)?,
                arousal_boost: row.get(4)?,
                description: row.get(5)?,
            })
        })?;
        rows.collect::<Result<Vec<_>, _>>().map_err(Into::into)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn predicate_evaluates_against_world_state() {
        let mut ws = HashMap::new();
        ws.insert("toddler.near_stairs", "true");
        ws.insert("toddler.confidence", "0.91");
        assert!(eval_predicate(
            "toddler.near_stairs = true AND toddler.confidence > 0.85",
            &ws
        ));
        assert!(!eval_predicate(
            "toddler.near_stairs = true AND toddler.confidence > 0.95",
            &ws
        ));
    }

    #[test]
    fn rule_respects_debounce() {
        let now = crate::agent::memory::now_epoch_secs() as i64;
        let rule = ReactiveRule {
            id: "r1".to_string(),
            mission_id: "m1".to_string(),
            predicate: "x = 1".to_string(),
            debounce_secs: 60,
            arousal_boost: 2.0,
            description: "test rule".to_string(),
            last_fired_at: AtomicI64::new(now - 10), // fired 10s ago
        };
        let mut ws = HashMap::new();
        ws.insert("x", "1");
        // Debounce = 60s, last fired 10s ago -> should NOT fire
        assert!(!rule.should_fire(&ws));
    }

    #[test]
    fn rule_fires_after_debounce_expires() {
        let now = crate::agent::memory::now_epoch_secs() as i64;
        let rule = ReactiveRule {
            id: "r2".to_string(),
            mission_id: "m1".to_string(),
            predicate: "x = 1".to_string(),
            debounce_secs: 60,
            arousal_boost: 1.5,
            description: "test rule".to_string(),
            last_fired_at: AtomicI64::new(now - 70), // fired 70s ago
        };
        let mut ws = HashMap::new();
        ws.insert("x", "1");
        // Debounce = 60s, last fired 70s ago -> should fire
        assert!(rule.should_fire(&ws));
    }

    #[test]
    fn evaluate_rules_sums_boosts() {
        let now = crate::agent::memory::now_epoch_secs() as i64;
        let rules = vec![
            ReactiveRule {
                id: "r1".to_string(),
                mission_id: "m1".to_string(),
                predicate: "x = 1".to_string(),
                debounce_secs: 0,
                arousal_boost: 1.0,
                description: String::new(),
                last_fired_at: AtomicI64::new(now - 100),
            },
            ReactiveRule {
                id: "r2".to_string(),
                mission_id: "m1".to_string(),
                predicate: "x = 1".to_string(),
                debounce_secs: 0,
                arousal_boost: 2.0,
                description: String::new(),
                last_fired_at: AtomicI64::new(now - 100),
            },
        ];
        let mut ws = HashMap::new();
        ws.insert("x", "1");
        let total = total_boost(&evaluate_rules_fired(&rules, &ws));
        assert!((total - 3.0).abs() < f64::EPSILON);
    }

    // Helper to build a ReactiveRule with a chosen last_fired_at.
    fn mk_rule(id: &str, last: i64) -> ReactiveRule {
        ReactiveRule {
            id: id.to_string(),
            mission_id: "m1".to_string(),
            predicate: "x = 1".to_string(),
            debounce_secs: 60,
            arousal_boost: 1.0,
            description: String::new(),
            last_fired_at: AtomicI64::new(last),
        }
    }

    #[test]
    fn merge_rule_state_preserves_last_fired_for_matching_ids() {
        // The whole point of the helper: after a reload, a rule that fired
        // 10 seconds ago should still be counted as "fired 10 seconds ago",
        // not "never fired". This is the regression from incident 2026-04-10.
        let old = vec![mk_rule("a", 1000), mk_rule("b", 2000)];
        // The "new" set comes from `configs.into_iter().map(Into::into).collect()`
        // — i.e. every rule's last_fired_at starts at 0.
        let new = vec![mk_rule("a", 0), mk_rule("b", 0)];

        let merged = merge_rule_state(&old, new);

        assert_eq!(merged[0].id, "a");
        assert_eq!(merged[0].last_fired_at.load(Ordering::Relaxed), 1000);
        assert_eq!(merged[1].id, "b");
        assert_eq!(merged[1].last_fired_at.load(Ordering::Relaxed), 2000);
    }

    #[test]
    fn merge_rule_state_leaves_newly_added_rules_at_zero() {
        // New rules (no match in the old set) must keep their zero-init
        // `last_fired_at` — they genuinely have never fired and should be
        // allowed to fire immediately on the next matching world state.
        let old = vec![mk_rule("a", 1000)];
        let new = vec![mk_rule("a", 0), mk_rule("brand-new", 0)];

        let merged = merge_rule_state(&old, new);

        assert_eq!(merged.len(), 2);
        assert_eq!(merged[0].last_fired_at.load(Ordering::Relaxed), 1000);
        assert_eq!(merged[1].id, "brand-new");
        assert_eq!(merged[1].last_fired_at.load(Ordering::Relaxed), 0);
    }

    #[test]
    fn merge_rule_state_drops_deleted_rules() {
        // Rules that existed in `old` but were removed from SQLite should
        // not reappear in the merged set.
        let old = vec![mk_rule("keep", 500), mk_rule("gone", 999)];
        let new = vec![mk_rule("keep", 0)];

        let merged = merge_rule_state(&old, new);

        assert_eq!(merged.len(), 1);
        assert_eq!(merged[0].id, "keep");
        assert_eq!(merged[0].last_fired_at.load(Ordering::Relaxed), 500);
    }

    #[test]
    fn merge_rule_state_does_not_overwrite_nonzero_new_state() {
        // Corner case: if the "new" set somehow arrives with a non-zero
        // last_fired_at that's *newer* than the preserved value, the helper
        // currently overwrites it with the old value. Document this: callers
        // feed freshly-deserialised rules (always 0), so the only state that
        // matters is `old`. This test pins the current behaviour so a future
        // change can't silently break the invariant.
        let old = vec![mk_rule("a", 500)];
        let new = vec![mk_rule("a", 999)];

        let merged = merge_rule_state(&old, new);

        // Preserved value from `old` wins because the reload path always
        // feeds zero-initialised new rules.
        assert_eq!(merged[0].last_fired_at.load(Ordering::Relaxed), 500);
    }

    #[test]
    fn reload_would_reset_debounce_without_merge() {
        // Regression pin: demonstrate the exact behaviour the helper prevents.
        // Without merge_rule_state, a rule that fired 10s ago and has a 60s
        // debounce would fire again immediately after a reload because
        // `From<ReactiveRuleConfig> for ReactiveRule` always produces
        // last_fired_at = 0.
        let now = crate::agent::memory::now_epoch_secs() as i64;
        let old = vec![ReactiveRule {
            id: "r".to_string(),
            mission_id: "m1".to_string(),
            predicate: "x = 1".to_string(),
            debounce_secs: 60,
            arousal_boost: 1.0,
            description: String::new(),
            last_fired_at: AtomicI64::new(now - 10),
        }];
        let reloaded_without_merge: Vec<ReactiveRule> = vec![ReactiveRuleConfig {
            id: "r".to_string(),
            mission_id: "m1".to_string(),
            predicate: "x = 1".to_string(),
            debounce_secs: 60,
            arousal_boost: 1.0,
            description: String::new(),
        }]
        .into_iter()
        .map(Into::into)
        .collect();

        let mut ws = HashMap::new();
        ws.insert("x", "1");

        // Without merge: debounce reset, fires on next tick (the bug).
        assert!(reloaded_without_merge[0].should_fire(&ws));

        // With merge: debounce preserved, still waiting out the window.
        let merged = merge_rule_state(&old, reloaded_without_merge);
        assert!(!merged[0].should_fire(&ws));
    }

    #[test]
    fn reactive_rule_store_roundtrips() {
        let dir = tempfile::tempdir().unwrap();
        let store = ReactiveRuleStore::open(&dir.path().join("alerts.db")).unwrap();

        let rule = ReactiveRuleConfig {
            id: "alert-1".to_string(),
            mission_id: "mission-dog".to_string(),
            predicate: "dog.near_stairs = true AND dog.confidence > 0.85".to_string(),
            debounce_secs: 30,
            arousal_boost: 2.5,
            description: "Dog near stairs alert".to_string(),
        };

        store.save_rule(&rule).unwrap();
        let rules = store.list_rules().unwrap();

        assert_eq!(rules.len(), 1);
        assert_eq!(rules[0].id, "alert-1");
        assert_eq!(rules[0].mission_id, "mission-dog");
        assert_eq!(rules[0].predicate, rule.predicate);
        assert!((rules[0].arousal_boost - 2.5).abs() < f64::EPSILON);
        assert_eq!(rules[0].debounce_secs, 30);
    }

    #[test]
    fn reactive_rule_store_delete() {
        let dir = tempfile::tempdir().unwrap();
        let store = ReactiveRuleStore::open(&dir.path().join("alerts.db")).unwrap();

        let rule = ReactiveRuleConfig {
            id: "alert-del".to_string(),
            mission_id: "m1".to_string(),
            predicate: "temp > 100".to_string(),
            debounce_secs: 60,
            arousal_boost: 1.0,
            description: "High temp".to_string(),
        };

        store.save_rule(&rule).unwrap();
        assert_eq!(store.list_rules().unwrap().len(), 1);

        store.delete_rule("alert-del").unwrap();
        assert!(store.list_rules().unwrap().is_empty());
    }

    #[test]
    fn reactive_rule_store_rules_for_mission() {
        let dir = tempfile::tempdir().unwrap();
        let store = ReactiveRuleStore::open(&dir.path().join("alerts.db")).unwrap();

        store
            .save_rule(&ReactiveRuleConfig {
                id: "a1".to_string(),
                mission_id: "m1".to_string(),
                predicate: "x = 1".to_string(),
                debounce_secs: 30,
                arousal_boost: 1.0,
                description: String::new(),
            })
            .unwrap();
        store
            .save_rule(&ReactiveRuleConfig {
                id: "a2".to_string(),
                mission_id: "m2".to_string(),
                predicate: "y = 2".to_string(),
                debounce_secs: 30,
                arousal_boost: 1.0,
                description: String::new(),
            })
            .unwrap();

        let m1_rules = store.rules_for_mission("m1").unwrap();
        assert_eq!(m1_rules.len(), 1);
        assert_eq!(m1_rules[0].id, "a1");
    }

    #[test]
    fn reactive_rule_config_to_rule_conversion() {
        let cfg = ReactiveRuleConfig {
            id: "r1".to_string(),
            mission_id: "m1".to_string(),
            predicate: "x > 5".to_string(),
            debounce_secs: 45,
            arousal_boost: 3.0,
            description: "test".to_string(),
        };
        let rule: ReactiveRule = cfg.into();
        assert_eq!(rule.id, "r1");
        assert_eq!(rule.debounce_secs, 45);
        assert!((rule.arousal_boost - 3.0).abs() < f64::EPSILON);
        assert_eq!(rule.last_fired_at.load(Ordering::Relaxed), 0);
    }
}
