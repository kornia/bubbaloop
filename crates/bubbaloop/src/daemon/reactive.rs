//! Reactive pre-filter: rule engine that adjusts agent arousal without calling the LLM.
//!
//! Rules fire when world state matches a predicate, applying a debounced arousal boost.
//! The evaluator reuses `apply_filter` from `context_provider` for predicate parsing.

use crate::daemon::context_provider::apply_filter;
use rusqlite::{params, Connection};
use std::collections::HashMap;
use std::path::Path;
use std::sync::atomic::{AtomicI64, Ordering};
use std::time::Duration;
use tokio::time::Instant;

/// After this many consecutive reactive-turn failures, the circuit breaker
/// trips and suspends reactive evaluation for `REACTIVE_BREAKER_COOL_OFF`.
pub const REACTIVE_BREAKER_THRESHOLD: u32 = 3;

/// How long to suspend reactive evaluation once the circuit breaker trips.
/// Five minutes is long enough that a runaway rule does not cause sustained
/// LLM burn, but short enough that recovery is automatic after a transient
/// upstream outage (ollama restart, network flap).
pub const REACTIVE_BREAKER_COOL_OFF: Duration = Duration::from_secs(300);

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

// ── Circuit Breaker ────────────────────────────────────────────────────

/// Last-resort bound on reactive-turn failures.
///
/// Commits A–C of the 2026-04-10 prevention series close three specific
/// holes (HTTP hang, stale world state, reload-reset debounce). This
/// breaker is commit D: a category-level bound that catches *any* other
/// cause of runaway reactive work. After `threshold` consecutive reactive
/// turn failures, the breaker trips and suppresses reactive evaluation
/// for `cool_off`. A single successful reactive turn — or waiting out
/// the cool-off window — closes the breaker and resets the counter.
///
/// Scope: lives inside a single `agent_loop` task, so it does not need
/// `Send + Sync` wrapping. Each agent has its own breaker; a storm on
/// one agent never suspends others.
///
/// Not about job turns: job turns are operator-initiated and should
/// fail loudly. Only reactive turns (autonomous, rule-driven) are gated.
#[derive(Debug)]
pub struct ReactiveCircuitBreaker {
    threshold: u32,
    cool_off: Duration,
    consecutive_failures: u32,
    tripped_until: Option<Instant>,
}

impl ReactiveCircuitBreaker {
    pub fn new(threshold: u32, cool_off: Duration) -> Self {
        Self {
            threshold,
            cool_off,
            consecutive_failures: 0,
            tripped_until: None,
        }
    }

    /// Call after a successful reactive turn. Clears the failure counter
    /// and any open cool-off — a single success means the upstream is
    /// healthy again and we should let rules fire freely.
    pub fn record_success(&mut self) {
        self.consecutive_failures = 0;
        self.tripped_until = None;
    }

    /// Call after a failed reactive turn. Returns `true` if *this* failure
    /// is the one that tripped the breaker (useful to log the transition
    /// exactly once, not on every subsequent check).
    pub fn record_failure(&mut self, now: Instant) -> bool {
        self.consecutive_failures = self.consecutive_failures.saturating_add(1);
        if self.consecutive_failures >= self.threshold && self.tripped_until.is_none() {
            self.tripped_until = Some(now + self.cool_off);
            return true;
        }
        false
    }

    /// Is the breaker currently open (suppressing reactive turns)?
    ///
    /// Mutating because this also performs auto-close: once the cool-off
    /// elapses, the breaker flips back to closed and the counter resets.
    /// Callers get a consistent "can I fire a reactive turn right now?"
    /// answer without having to remember to tick the breaker separately.
    pub fn is_open(&mut self, now: Instant) -> bool {
        match self.tripped_until {
            Some(until) if now < until => true,
            Some(_) => {
                self.tripped_until = None;
                self.consecutive_failures = 0;
                false
            }
            None => false,
        }
    }

    /// Time remaining until the breaker auto-closes. `None` when closed.
    pub fn cool_off_remaining(&self, now: Instant) -> Option<Duration> {
        self.tripped_until
            .map(|until| until.saturating_duration_since(now))
    }

    /// Current consecutive failure count (for diagnostics / list_alerts).
    pub fn consecutive_failures(&self) -> u32 {
        self.consecutive_failures
    }
}

// ── Persistence ────────────────────────────────────────────────────────

/// Maximum length of a predicate string. Prevents accidental/malicious
/// DOS via multi-megabyte predicates that would be parsed on every tick.
pub const MAX_PREDICATE_LEN: usize = 2048;

/// Maximum length of a rule description (operator-facing string, kept
/// sane to bound list_alerts output size).
pub const MAX_DESCRIPTION_LEN: usize = 1024;

/// Smallest allowed debounce. Zero means "fire every tick" which, with
/// a permissive predicate, is exactly the reactive storm the 2026-04-10
/// incident demonstrated. A rule that legitimately needs high frequency
/// should set 1s explicitly rather than 0.
pub const MIN_DEBOUNCE_SECS: u32 = 1;

/// Sanity ceiling on debounce. 1 day is already absurdly long for a
/// reactive rule — anything larger is almost certainly a mistake and
/// will feel like the rule is broken to whoever registered it.
pub const MAX_DEBOUNCE_SECS: u32 = 86_400;

/// Lower bound on arousal boost. Negative boosts would *reduce* arousal
/// on rule fire, which is not a meaningful reactive-alert semantic.
pub const MIN_AROUSAL_BOOST: f64 = 0.0;

/// Upper bound on arousal boost. Picks a value that's high enough to
/// always shrink the heartbeat interval to its minimum, but small enough
/// that a typo (`200.0` instead of `2.0`) is caught before storage.
pub const MAX_AROUSAL_BOOST: f64 = 100.0;

/// Default `debounce_secs` used when the operator does not specify one.
/// Kept in sync across the daemon platform, the mock platform, and the
/// MCP tool handler so all validation paths see the same value.
pub const DEFAULT_DEBOUNCE_SECS: u32 = 60;

/// Default `arousal_boost` used when the operator does not specify one.
pub const DEFAULT_AROUSAL_BOOST: f64 = 2.0;

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

impl ReactiveRuleConfig {
    /// Validate every field against its invariants. Returns a clear error
    /// message pointing at the first violated constraint, or `Ok(())`.
    ///
    /// This is the boundary that catches bad rules *at registration* —
    /// long before evaluation time. Critical categories:
    ///
    /// 1. **Empty / operator-less predicate.** `apply_filter` treats an
    ///    empty string (and any clause with no recognized comparison
    ///    operator) as "match" — every tick, every row. Combined with a
    ///    permissive `debounce_secs`, that is the exact pattern of the
    ///    2026-04-10 reactive storm. Reject both shapes here.
    ///
    /// 2. **`debounce_secs = 0`.** Means "fire every tick" — another
    ///    route to the same storm.
    ///
    /// 3. **Non-finite arousal boost** (NaN, ±∞). Poisons the arousal
    ///    state and can make the heartbeat interval undefined. These
    ///    values have no legitimate use.
    ///
    /// 4. **Out-of-band strings.** Bound predicate and description
    ///    lengths to prevent pathological DB rows and unbounded
    ///    list_alerts output.
    pub fn validate(&self) -> anyhow::Result<()> {
        use anyhow::bail;

        if self.id.trim().is_empty() {
            bail!("rule id must be non-empty");
        }
        if self.mission_id.trim().is_empty() {
            bail!("mission_id must be non-empty");
        }

        let predicate = self.predicate.trim();
        if predicate.is_empty() {
            bail!(
                "predicate must be non-empty (empty predicates match every tick and cause reactive storms)"
            );
        }
        if self.predicate.len() > MAX_PREDICATE_LEN {
            bail!(
                "predicate exceeds maximum length ({} > {})",
                self.predicate.len(),
                MAX_PREDICATE_LEN
            );
        }
        if !predicate_has_operator(predicate) {
            bail!(
                "predicate must contain at least one comparison operator \
                 (=, !=, >, <, >=, <=); got {:?}",
                predicate
            );
        }

        if self.description.len() > MAX_DESCRIPTION_LEN {
            bail!(
                "description exceeds maximum length ({} > {})",
                self.description.len(),
                MAX_DESCRIPTION_LEN
            );
        }

        if self.debounce_secs < MIN_DEBOUNCE_SECS {
            bail!(
                "debounce_secs must be at least {} (got {}); debounce_secs = 0 \
                 causes every-tick firing",
                MIN_DEBOUNCE_SECS,
                self.debounce_secs
            );
        }
        if self.debounce_secs > MAX_DEBOUNCE_SECS {
            bail!(
                "debounce_secs must be at most {} (got {})",
                MAX_DEBOUNCE_SECS,
                self.debounce_secs
            );
        }

        if !self.arousal_boost.is_finite() {
            bail!("arousal_boost must be finite (got {})", self.arousal_boost);
        }
        if self.arousal_boost < MIN_AROUSAL_BOOST || self.arousal_boost > MAX_AROUSAL_BOOST {
            bail!(
                "arousal_boost must be in [{}, {}] (got {})",
                MIN_AROUSAL_BOOST,
                MAX_AROUSAL_BOOST,
                self.arousal_boost
            );
        }

        Ok(())
    }
}

/// Scan a predicate for at least one comparison operator.
///
/// Mirrors the operator set in `apply_filter` in `context_provider.rs`.
/// Kept as its own helper so the validation test suite can pin the
/// operator list; if `apply_filter` grows new operators, update both.
fn predicate_has_operator(predicate: &str) -> bool {
    // Order matters only for readability — any match is sufficient.
    // `==` is NOT in apply_filter's syntax, so `=` alone is the equality
    // operator; that's the reason we check `!=` first (otherwise a `!=`
    // clause would match the `=` branch and look like an equality test).
    predicate.contains("!=")
        || predicate.contains(">=")
        || predicate.contains("<=")
        || predicate.contains('>')
        || predicate.contains('<')
        || predicate.contains('=')
}

/// Extract the field name (LHS) from each clause of a predicate.
///
/// Mirrors the parser in `apply_filter` — same operator precedence,
/// same `" AND "` clause split, same trimming rules. Returns a
/// deduplicated list in first-appearance order so the caller can use
/// it for diagnostics (e.g. "predicate references unknown field X").
///
/// Used by the dangling-reference check that warns when a reactive
/// rule references a world-state key that no registered context
/// provider appears to produce. The 2026-04-10 incident started
/// exactly this way: a rule referenced `motion.level`, no provider
/// was registered to populate it, and a manual `zenoh put` seeded
/// the key by accident — so the rule happily fired forever on a
/// "ghost" value.
pub fn extract_predicate_fields(predicate: &str) -> Vec<String> {
    let mut fields: Vec<String> = Vec::new();
    for clause in predicate.split(" AND ") {
        let clause = clause.trim();
        if clause.is_empty() {
            continue;
        }
        // Operator precedence matches apply_filter: longest multi-char
        // operators first so `!=` isn't confused with `=`.
        let field = if let Some(pos) = clause.find("!=") {
            &clause[..pos]
        } else if let Some(pos) = clause.find(">=") {
            &clause[..pos]
        } else if let Some(pos) = clause.find("<=") {
            &clause[..pos]
        } else if let Some(pos) = clause.find('>') {
            &clause[..pos]
        } else if let Some(pos) = clause.find('<') {
            &clause[..pos]
        } else if let Some(pos) = clause.find('=') {
            &clause[..pos]
        } else {
            continue;
        };
        let field = field.trim().to_string();
        if !field.is_empty() && !fields.contains(&field) {
            fields.push(field);
        }
    }
    fields
}

/// Given a set of provider `world_state_key_template` strings, return
/// the list of `predicate_fields` that no template could plausibly
/// produce.
///
/// Classification of templates:
///   • **Literal** (no `{...}`): exact-match required.
///   • **Prefix + placeholder** (`"object.{label}"` → prefix `"object."`):
///     covers any field that `starts_with(prefix)`.
///   • **Leading placeholder** (template begins with `{`): the prefix
///     is empty and we cannot statically bound the output, so we
///     return "nothing is dangling" for the whole rule rather than
///     risk a false positive. This is an over-approximation of
///     coverage: we'd rather miss a real dangling ref than bother
///     the operator with a wrong warning.
///
/// Returns an empty vec if every field is covered.
pub fn find_dangling_fields(
    predicate_fields: &[String],
    provider_templates: &[String],
) -> Vec<String> {
    let mut literals: Vec<&str> = Vec::new();
    let mut prefixes: Vec<String> = Vec::new();
    for tpl in provider_templates {
        match tpl.find('{') {
            None => literals.push(tpl.as_str()),
            Some(0) => {
                // Leading placeholder — bail out with "nothing is dangling".
                return Vec::new();
            }
            Some(pos) => prefixes.push(tpl[..pos].to_string()),
        }
    }

    predicate_fields
        .iter()
        .filter(|f| {
            let f = f.as_str();
            let literal_hit = literals.contains(&f);
            let prefix_hit = prefixes
                .iter()
                .any(|p| !p.is_empty() && f.starts_with(p.as_str()));
            !literal_hit && !prefix_hit
        })
        .cloned()
        .collect()
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
    ///
    /// Validates the rule before touching the database — any invariant
    /// violation (see [`ReactiveRuleConfig::validate`]) is an error and
    /// nothing is written. This is the single choke point for rule
    /// persistence; both the daemon and mock platform go through here,
    /// so validation is applied uniformly.
    pub fn save_rule(&self, rule: &ReactiveRuleConfig) -> anyhow::Result<()> {
        rule.validate()?;
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

    // ── Circuit breaker tests ──
    //
    // The breaker takes `now: Instant` as a parameter rather than reading the
    // clock, so tests can simulate arbitrary time passage deterministically.

    #[tokio::test]
    async fn breaker_starts_closed() {
        let mut b = ReactiveCircuitBreaker::new(3, Duration::from_secs(300));
        assert!(!b.is_open(Instant::now()));
        assert_eq!(b.consecutive_failures(), 0);
        assert_eq!(b.cool_off_remaining(Instant::now()), None);
    }

    #[tokio::test]
    async fn breaker_opens_exactly_at_threshold() {
        let mut b = ReactiveCircuitBreaker::new(3, Duration::from_secs(300));
        let t0 = Instant::now();

        // Failures 1 and 2 do not trip.
        assert!(!b.record_failure(t0));
        assert!(!b.is_open(t0));
        assert!(!b.record_failure(t0));
        assert!(!b.is_open(t0));

        // Failure 3 trips the breaker, and record_failure returns true to
        // signal the *transition* — callers can log exactly once.
        assert!(b.record_failure(t0));
        assert!(b.is_open(t0));
    }

    #[tokio::test]
    async fn breaker_record_failure_signals_transition_only_once() {
        let mut b = ReactiveCircuitBreaker::new(2, Duration::from_secs(300));
        let t0 = Instant::now();
        assert!(!b.record_failure(t0));
        assert!(b.record_failure(t0)); // Transition: closed → open.
                                       // Further failures while already open must NOT claim to be the
                                       // transition — otherwise operators would get repeat "breaker tripped"
                                       // logs every tick for the whole cool-off window.
        assert!(!b.record_failure(t0));
        assert!(!b.record_failure(t0));
    }

    #[tokio::test]
    async fn breaker_record_success_resets_counter() {
        let mut b = ReactiveCircuitBreaker::new(3, Duration::from_secs(300));
        let t0 = Instant::now();
        b.record_failure(t0);
        b.record_failure(t0);
        assert_eq!(b.consecutive_failures(), 2);

        b.record_success();
        assert_eq!(b.consecutive_failures(), 0);

        // After reset, we need a full threshold of failures again to trip.
        b.record_failure(t0);
        b.record_failure(t0);
        assert!(!b.is_open(t0));
        assert!(b.record_failure(t0));
        assert!(b.is_open(t0));
    }

    #[tokio::test]
    async fn breaker_success_closes_an_open_breaker() {
        // A success during an open cool-off window (e.g. the operator
        // pinged the agent with a healthy job turn that proved upstream
        // is up) should immediately clear both the counter and the
        // tripped_until timestamp.
        let mut b = ReactiveCircuitBreaker::new(2, Duration::from_secs(300));
        let t0 = Instant::now();
        b.record_failure(t0);
        b.record_failure(t0);
        assert!(b.is_open(t0));

        b.record_success();
        assert!(!b.is_open(t0));
        assert_eq!(b.cool_off_remaining(t0), None);
    }

    #[tokio::test]
    async fn breaker_closes_after_cool_off() {
        let mut b = ReactiveCircuitBreaker::new(2, Duration::from_secs(10));
        let t0 = Instant::now();
        b.record_failure(t0);
        b.record_failure(t0);
        assert!(b.is_open(t0));

        // Still open mid-window.
        assert!(b.is_open(t0 + Duration::from_secs(5)));

        // Auto-closes past the window; counter also resets so the next
        // failure has a full threshold to climb before re-tripping.
        let past = t0 + Duration::from_secs(11);
        assert!(!b.is_open(past));
        assert_eq!(b.consecutive_failures(), 0);
    }

    #[tokio::test]
    async fn breaker_cool_off_remaining_reports_accurately() {
        let mut b = ReactiveCircuitBreaker::new(1, Duration::from_secs(60));
        let t0 = Instant::now();
        assert_eq!(b.cool_off_remaining(t0), None);

        b.record_failure(t0);
        let remaining = b.cool_off_remaining(t0).unwrap();
        assert!(remaining >= Duration::from_secs(59));
        assert!(remaining <= Duration::from_secs(60));

        let half = b.cool_off_remaining(t0 + Duration::from_secs(30)).unwrap();
        assert!(half <= Duration::from_secs(30));

        // Past the window, saturating_duration_since returns zero.
        let past = b.cool_off_remaining(t0 + Duration::from_secs(90)).unwrap();
        assert_eq!(past, Duration::ZERO);
    }

    #[tokio::test]
    async fn breaker_can_retrip_after_recovery() {
        // Full cycle: trip → cool off → close → trip again. Confirms that
        // recovery is real, not a one-shot fluke.
        let mut b = ReactiveCircuitBreaker::new(2, Duration::from_secs(10));
        let t0 = Instant::now();
        b.record_failure(t0);
        b.record_failure(t0);
        assert!(b.is_open(t0));

        let t1 = t0 + Duration::from_secs(11);
        assert!(!b.is_open(t1));

        // Fresh pair of failures trips it again.
        assert!(!b.record_failure(t1));
        assert!(b.record_failure(t1));
        assert!(b.is_open(t1));
    }

    #[tokio::test]
    async fn breaker_saturates_counter_does_not_overflow() {
        // Paranoid smoke test: a stuck-failing agent over days must not
        // panic on u32 overflow. `record_failure` uses `saturating_add`,
        // so after arbitrarily many failures the counter stabilises at
        // `u32::MAX` without wrapping or panicking.
        let mut b = ReactiveCircuitBreaker::new(3, Duration::from_secs(1));
        let t0 = Instant::now();
        for _ in 0..10 {
            b.record_failure(t0);
        }
        assert_eq!(b.consecutive_failures(), 10);
        assert!(b.is_open(t0));
    }

    // ── Rule validation tests ──
    //
    // Validation is the boundary that catches bad rules at registration,
    // long before they can cause storms at evaluation time. Each test
    // pins one specific invariant so regressions fail loud.

    fn valid_cfg() -> ReactiveRuleConfig {
        ReactiveRuleConfig {
            id: "a1".to_string(),
            mission_id: "m1".to_string(),
            predicate: "motion.level > 0.05".to_string(),
            debounce_secs: 60,
            arousal_boost: 2.0,
            description: "motion detected".to_string(),
        }
    }

    #[test]
    fn validate_accepts_well_formed_rule() {
        valid_cfg()
            .validate()
            .expect("well-formed rule must validate");
    }

    #[test]
    fn validate_rejects_empty_predicate() {
        // This is the headline bug from the 2026-04-10 post-mortem:
        // `apply_filter("")` returns true on every tick, so an empty
        // predicate rule fires continuously. Must be rejected.
        let mut c = valid_cfg();
        c.predicate = String::new();
        let err = c.validate().unwrap_err().to_string();
        assert!(err.contains("predicate must be non-empty"), "{err}");
    }

    #[test]
    fn validate_rejects_whitespace_only_predicate() {
        // `apply_filter` trims clauses — a predicate of "   \t\n  " has
        // no non-empty clauses, so it also matches every tick. Same bug.
        let mut c = valid_cfg();
        c.predicate = "   \t\n  ".to_string();
        let err = c.validate().unwrap_err().to_string();
        assert!(err.contains("predicate must be non-empty"), "{err}");
    }

    #[test]
    fn validate_rejects_predicate_with_no_comparison_operator() {
        // `apply_filter` silently skips clauses with no recognised
        // operator (`continue`), which means a predicate like "dog" or
        // "sensor_online" matches every tick. Catch this at registration.
        let mut c = valid_cfg();
        c.predicate = "just_a_word".to_string();
        let err = c.validate().unwrap_err().to_string();
        assert!(
            err.contains("comparison operator"),
            "expected operator error, got: {err}"
        );
    }

    #[test]
    fn validate_accepts_all_supported_operators() {
        // Pins the full operator set against the apply_filter syntax.
        // If apply_filter grows a new operator, update predicate_has_operator.
        for pred in ["x = 1", "x != 1", "x > 1", "x < 1", "x >= 1", "x <= 1"] {
            let mut c = valid_cfg();
            c.predicate = pred.to_string();
            c.validate()
                .unwrap_or_else(|e| panic!("predicate {pred:?} should validate: {e}"));
        }
    }

    #[test]
    fn validate_rejects_overlong_predicate() {
        let mut c = valid_cfg();
        // Valid operator at the end, but whole string is way too long.
        c.predicate = format!("{} > 1", "a".repeat(MAX_PREDICATE_LEN));
        let err = c.validate().unwrap_err().to_string();
        assert!(err.contains("exceeds maximum length"), "{err}");
    }

    #[test]
    fn validate_rejects_overlong_description() {
        let mut c = valid_cfg();
        c.description = "x".repeat(MAX_DESCRIPTION_LEN + 1);
        let err = c.validate().unwrap_err().to_string();
        assert!(err.contains("description"), "{err}");
    }

    #[test]
    fn validate_rejects_debounce_zero() {
        // The second route to every-tick firing, independent of
        // predicate shape. Zero is not "as fast as possible" — it is
        // "fire every heartbeat tick regardless of prior fire".
        let mut c = valid_cfg();
        c.debounce_secs = 0;
        let err = c.validate().unwrap_err().to_string();
        assert!(err.contains("debounce_secs must be at least"), "{err}");
    }

    #[test]
    fn validate_rejects_debounce_too_large() {
        let mut c = valid_cfg();
        c.debounce_secs = MAX_DEBOUNCE_SECS + 1;
        let err = c.validate().unwrap_err().to_string();
        assert!(err.contains("at most"), "{err}");
    }

    #[test]
    fn validate_rejects_nan_arousal_boost() {
        let mut c = valid_cfg();
        c.arousal_boost = f64::NAN;
        let err = c.validate().unwrap_err().to_string();
        assert!(err.contains("finite"), "{err}");
    }

    #[test]
    fn validate_rejects_infinite_arousal_boost() {
        let mut c = valid_cfg();
        c.arousal_boost = f64::INFINITY;
        let err = c.validate().unwrap_err().to_string();
        assert!(err.contains("finite"), "{err}");

        let mut c = valid_cfg();
        c.arousal_boost = f64::NEG_INFINITY;
        let err = c.validate().unwrap_err().to_string();
        assert!(err.contains("finite"), "{err}");
    }

    #[test]
    fn validate_rejects_negative_arousal_boost() {
        let mut c = valid_cfg();
        c.arousal_boost = -1.0;
        let err = c.validate().unwrap_err().to_string();
        assert!(err.contains("arousal_boost must be in"), "{err}");
    }

    #[test]
    fn validate_rejects_out_of_range_arousal_boost() {
        let mut c = valid_cfg();
        c.arousal_boost = MAX_AROUSAL_BOOST + 1.0;
        let err = c.validate().unwrap_err().to_string();
        assert!(err.contains("arousal_boost must be in"), "{err}");
    }

    #[test]
    fn validate_rejects_empty_id() {
        let mut c = valid_cfg();
        c.id = String::new();
        let err = c.validate().unwrap_err().to_string();
        assert!(err.contains("rule id"), "{err}");

        let mut c = valid_cfg();
        c.id = "   ".to_string();
        let err = c.validate().unwrap_err().to_string();
        assert!(err.contains("rule id"), "{err}");
    }

    #[test]
    fn validate_rejects_empty_mission_id() {
        let mut c = valid_cfg();
        c.mission_id = String::new();
        let err = c.validate().unwrap_err().to_string();
        assert!(err.contains("mission_id"), "{err}");
    }

    #[test]
    fn save_rule_rejects_invalid_config_without_writing() {
        // End-to-end: validation happens at the SQLite boundary, so a
        // bad rule never reaches the database. Confirm the DB stays
        // empty after a rejected save.
        let dir = tempfile::tempdir().unwrap();
        let store = ReactiveRuleStore::open(&dir.path().join("alerts.db")).unwrap();

        let mut bad = valid_cfg();
        bad.predicate = String::new();
        let err = store.save_rule(&bad).unwrap_err().to_string();
        assert!(err.contains("predicate"), "{err}");

        assert!(
            store.list_rules().unwrap().is_empty(),
            "rejected rule must not be persisted"
        );
    }

    #[test]
    fn save_rule_accepts_valid_config() {
        let dir = tempfile::tempdir().unwrap();
        let store = ReactiveRuleStore::open(&dir.path().join("alerts.db")).unwrap();
        store.save_rule(&valid_cfg()).unwrap();
        assert_eq!(store.list_rules().unwrap().len(), 1);
    }

    // ---------- extract_predicate_fields ----------

    #[test]
    fn extract_fields_handles_simple_equality() {
        let f = extract_predicate_fields("motion.level = 0.5");
        assert_eq!(f, vec!["motion.level"]);
    }

    #[test]
    fn extract_fields_splits_and_clauses() {
        let f =
            extract_predicate_fields("toddler.near_stairs = true AND toddler.confidence > 0.85");
        assert_eq!(f, vec!["toddler.near_stairs", "toddler.confidence"]);
    }

    #[test]
    fn extract_fields_supports_all_operators() {
        // One clause per operator, mirroring apply_filter precedence.
        let f =
            extract_predicate_fields("a != 1 AND b >= 2 AND c <= 3 AND d > 4 AND e < 5 AND f = 6");
        assert_eq!(f, vec!["a", "b", "c", "d", "e", "f"]);
    }

    #[test]
    fn extract_fields_dedupes_repeated_references() {
        let f = extract_predicate_fields("x > 0 AND x < 10 AND y = 1");
        assert_eq!(f, vec!["x", "y"]);
    }

    #[test]
    fn extract_fields_ignores_empty_and_operatorless_clauses() {
        // Empty trailing clause and a bare token with no operator should both
        // be silently skipped — matching apply_filter's behavior.
        let f = extract_predicate_fields("x = 1 AND  AND bogus");
        assert_eq!(f, vec!["x"]);
    }

    #[test]
    fn extract_fields_returns_empty_for_empty_input() {
        assert!(extract_predicate_fields("").is_empty());
        assert!(extract_predicate_fields("   ").is_empty());
    }

    // ---------- find_dangling_fields ----------

    #[test]
    fn dangling_empty_when_all_fields_covered_by_literal() {
        let fields = vec!["motion.level".to_string()];
        let tpls = vec!["motion.level".to_string()];
        assert!(find_dangling_fields(&fields, &tpls).is_empty());
    }

    #[test]
    fn dangling_reports_missing_literal() {
        let fields = vec!["motion.level".to_string()];
        let tpls = vec!["temperature.value".to_string()];
        assert_eq!(
            find_dangling_fields(&fields, &tpls),
            vec!["motion.level".to_string()]
        );
    }

    #[test]
    fn dangling_empty_when_prefix_template_covers_field() {
        // Template "object.{label}" → prefix "object." → covers any
        // field that starts with "object.".
        let fields = vec!["object.person".to_string(), "object.cat".to_string()];
        let tpls = vec!["object.{label}".to_string()];
        assert!(find_dangling_fields(&fields, &tpls).is_empty());
    }

    #[test]
    fn dangling_reports_fields_outside_prefix() {
        let fields = vec!["object.person".to_string(), "motion.level".to_string()];
        let tpls = vec!["object.{label}".to_string()];
        assert_eq!(
            find_dangling_fields(&fields, &tpls),
            vec!["motion.level".to_string()]
        );
    }

    #[test]
    fn dangling_mixes_literal_and_prefix_templates() {
        let fields = vec![
            "temperature.value".to_string(),
            "object.person".to_string(),
            "humidity".to_string(),
        ];
        let tpls = vec![
            "temperature.value".to_string(),
            "object.{label}".to_string(),
        ];
        assert_eq!(
            find_dangling_fields(&fields, &tpls),
            vec!["humidity".to_string()]
        );
    }

    #[test]
    fn dangling_returns_empty_for_leading_placeholder_template() {
        // A template that begins with "{...}" cannot be statically
        // bounded — we return "nothing is dangling" rather than risk a
        // false positive warning.
        let fields = vec!["anything.goes".to_string(), "else.here".to_string()];
        let tpls = vec!["{key}".to_string()];
        assert!(find_dangling_fields(&fields, &tpls).is_empty());
    }

    #[test]
    fn dangling_treats_no_providers_as_all_dangling() {
        let fields = vec!["a".to_string(), "b".to_string()];
        let tpls: Vec<String> = Vec::new();
        assert_eq!(
            find_dangling_fields(&fields, &tpls),
            vec!["a".to_string(), "b".to_string()]
        );
    }
}
