# Physical AI Memory & Mission System — Implementation Plan

> **For Claude:** REQUIRED SUB-SKILL: Use superpowers:executing-plans to implement this plan task-by-task.

**Goal:** Implement the four-tier sensor-grounded memory model, LLM-compiled reactive mission DAG,
context provider system, and daemon-enforced safety layer as designed in
`docs/plans/2026-03-08-physical-ai-memory-mission-design.md`.

**Architecture:** The system extends the existing 3-tier memory (short-term, episodic, semantic)
with a new Tier 0 world state table written continuously by daemon-side context providers and
read by the agent on every turn. Missions are markdown files compiled by a setup turn into
persisted SQLite config; the daemon evaluates the mission DAG, constraints, and reactive rules
without any LLM involvement after setup.

**Tech Stack:** Rust, Tokio, Zenoh 1.x, rusqlite (already linked), argh CLI, rmcp MCP server,
`log` + `env_logger`, `thiserror` + `anyhow`, `serde` + `serde_json`, `chrono`.

---

## Expert Review Board (framing)

This plan was reviewed against the following failure taxonomy, as a DeepMind/NASA joint team
would demand before touching production safety-adjacent code:

| Reviewer | Primary concern | Encoded in plan as |
|----------|-----------------|--------------------|
| **Robotics safety** (NASA JPL) | No LLM call in safety path. Constraint violations must be synchronous. | Phase 4 implemented before Phase 5 (DAG). Constraints validated before Zenoh publish. |
| **ML systems** (DeepMind) | World state must be consistent. No torn reads across multiple keys. | SQLite WAL + single writer per key. Snapshot is a single `BEGIN IMMEDIATE` transaction. |
| **Distributed systems** | Federated gossip must be idempotent. Clock skew handled. | `last_seen_at` is source timestamp, not receiver time. Remote keys prefixed. |
| **Formal methods** | Every module has stated invariants. Tests prove invariants hold. | Each task begins with invariant declaration. Tests target invariants directly. |
| **Embedded systems** | Binary size budget enforced. No heap allocation in hot path. | Size checked after every phase. Stack-allocated predicate evaluator. |

---

## Formal Invariants (system-wide)

Before writing code, understand these invariants. Every test should target at least one:

```
I1: World state entries are never read while being written
    → enforced by SQLite WAL + single tokio task per provider

I2: Constraint violations are rejected before any Zenoh publish
    → enforced by constraint_engine::validate() wrapping all actuator tool calls

I3: A mission in "active" status always has at least one active context provider
    → enforced by Mission::activate() atomically creating provider + updating status

I4: Belief confidence is monotonically non-increasing over time (absent new observations)
    → enforced by decay job running before each agent turn

I5: Agent turn never receives stale world state without explicit staleness warning
    → enforced by snapshot() marking entries where now - last_seen_at > max_age_secs

I6: Resource locks are always released on mission completion, failure, or expiry
    → enforced by MissionRuntime::finalize() called in Drop impl

I7: Setup turns never contribute to episodic memory
    → enforced by run_setup_turn() using a separate Memory instance with no backend
```

---

## Phases Overview

```
Phase 0: DB foundations          (world_state + beliefs + causal chain columns)
Phase 1: Context providers       (ZenohContextProvider + configure_context tool)
Phase 2: Mission model           (file watcher + setup turn + mission CRUD tools)
Phase 3: Reactive pre-filter     (rule engine + register_alert + arousal integration)
Phase 4: Safety layer            (constraint engine + resource locks + fallback actions)
Phase 5: Mission DAG             (sub-missions + dependency resolution + micro-turns)
Phase 6: Belief system           (update loop + decay + contradiction detection)
Phase 7: Federated agents        (gossip protocol + remote world state merge)
```

**Do phases in order. Each phase depends on the previous. Never skip ahead.**

---

## Phase 0: Database Foundations

*No new behaviour. Only schema changes. Zero risk. Must complete before any other phase.*

### Task 0.1: Add world_state table to SemanticStore

**Invariant targeted:** I1, I5

**Files:**
- Modify: `crates/bubbaloop/src/agent/memory/semantic.rs`

**Step 1: Write the failing test first**

Add to the `#[cfg(test)] mod tests` block at the bottom of `semantic.rs`:

```rust
#[test]
fn world_state_table_exists_and_roundtrips() {
    let dir = tempfile::tempdir().unwrap();
    let db = dir.path().join("test.db");
    let store = SemanticStore::open(&db).unwrap();

    store.upsert_world_state("dog.location", "kitchen", 0.97,
        Some("bubbaloop/home/j1/vision/detections"), Some("vision-node"), 300).unwrap();

    let snap = store.world_state_snapshot().unwrap();
    assert_eq!(snap.len(), 1);
    assert_eq!(snap[0].key, "dog.location");
    assert_eq!(snap[0].value, "kitchen");
    assert!((snap[0].confidence - 0.97).abs() < 0.001);
    assert!(!snap[0].stale); // just inserted, cannot be stale
}

#[test]
fn world_state_staleness_flagged() {
    let dir = tempfile::tempdir().unwrap();
    let db = dir.path().join("test.db");
    let store = SemanticStore::open(&db).unwrap();

    // Insert with last_seen_at = 1 hour ago, max_age = 10 secs
    let old_ts = crate::agent::memory::now_epoch_secs() as i64 - 3600;
    store.upsert_world_state_at("robot.position", "[0,0]", 1.0, None, None, 10, old_ts).unwrap();

    let snap = store.world_state_snapshot().unwrap();
    assert_eq!(snap.len(), 1);
    assert!(snap[0].stale, "entry must be flagged stale");
}
```

**Step 2: Run tests to confirm they fail**
```bash
cargo test --lib -p bubbaloop world_state 2>&1 | grep -E "FAILED|error"
```
Expected: compile errors — `upsert_world_state` doesn't exist yet.

**Step 3: Add WorldStateEntry struct and table creation**

In `semantic.rs`, after the `Proposal` struct, add:

```rust
/// A single world state entry — a key-value snapshot of physical reality.
#[derive(Debug, Clone, serde::Serialize)]
pub struct WorldStateEntry {
    pub key: String,
    pub value: String,
    pub confidence: f64,
    pub source_topic: Option<String>,
    pub source_node: Option<String>,
    pub last_seen_at: i64,
    pub max_age_secs: i64,
    /// True if now - last_seen_at > max_age_secs
    pub stale: bool,
}
```

In `SemanticStore::open()`, after the jobs/proposals table creation, add:

```rust
conn.execute_batch("
    CREATE TABLE IF NOT EXISTS world_state (
        key           TEXT PRIMARY KEY,
        value         TEXT NOT NULL,
        confidence    REAL NOT NULL DEFAULT 1.0,
        source_topic  TEXT,
        source_node   TEXT,
        last_seen_at  INTEGER NOT NULL,
        max_age_secs  INTEGER NOT NULL DEFAULT 300,
        mission_id    TEXT,
        created_at    INTEGER NOT NULL
    );
    CREATE INDEX IF NOT EXISTS idx_ws_last_seen ON world_state(last_seen_at);
")?;
```

**Step 4: Implement upsert_world_state and world_state_snapshot**

```rust
pub fn upsert_world_state(
    &self,
    key: &str,
    value: &str,
    confidence: f64,
    source_topic: Option<&str>,
    source_node: Option<&str>,
    max_age_secs: i64,
) -> super::Result<()> {
    let now = crate::agent::memory::now_epoch_secs() as i64;
    self.upsert_world_state_at(key, value, confidence, source_topic, source_node, max_age_secs, now)
}

pub fn upsert_world_state_at(
    &self,
    key: &str,
    value: &str,
    confidence: f64,
    source_topic: Option<&str>,
    source_node: Option<&str>,
    max_age_secs: i64,
    last_seen_at: i64,
) -> super::Result<()> {
    let now = crate::agent::memory::now_epoch_secs() as i64;
    self.conn.execute(
        "INSERT INTO world_state (key, value, confidence, source_topic, source_node,
            last_seen_at, max_age_secs, created_at)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8)
         ON CONFLICT(key) DO UPDATE SET
            value = excluded.value,
            confidence = excluded.confidence,
            source_topic = excluded.source_topic,
            source_node = excluded.source_node,
            last_seen_at = excluded.last_seen_at,
            max_age_secs = excluded.max_age_secs",
        rusqlite::params![key, value, confidence, source_topic, source_node,
            last_seen_at, max_age_secs, now],
    )?;
    Ok(())
}

/// Snapshot all world state entries. Marks stale entries. Single transaction (I1).
pub fn world_state_snapshot(&self) -> super::Result<Vec<WorldStateEntry>> {
    let now = crate::agent::memory::now_epoch_secs() as i64;
    let mut stmt = self.conn.prepare(
        "SELECT key, value, confidence, source_topic, source_node,
                last_seen_at, max_age_secs
         FROM world_state ORDER BY key"
    )?;
    let entries = stmt.query_map([], |row| {
        let last_seen_at: i64 = row.get(5)?;
        let max_age_secs: i64 = row.get(6)?;
        Ok(WorldStateEntry {
            key: row.get(0)?,
            value: row.get(1)?,
            confidence: row.get(2)?,
            source_topic: row.get(3)?,
            source_node: row.get(4)?,
            last_seen_at,
            max_age_secs,
            stale: (now - last_seen_at) > max_age_secs,
        })
    })?.collect::<rusqlite::Result<Vec<_>>>()?;
    Ok(entries)
}
```

**Step 5: Run tests**
```bash
cargo test --lib -p bubbaloop world_state 2>&1 | grep -E "ok|FAILED"
```
Expected: `test ... ok` for both tests.

**Step 6: Commit**
```bash
git add crates/bubbaloop/src/agent/memory/semantic.rs
git commit -m "feat(memory): add world_state table with staleness detection"
```

---

### Task 0.2: Add beliefs table to SemanticStore

**Invariant targeted:** I4

**Files:**
- Modify: `crates/bubbaloop/src/agent/memory/semantic.rs`

**Step 1: Write failing tests**

```rust
#[test]
fn belief_upsert_and_confirm() {
    let dir = tempfile::tempdir().unwrap();
    let store = SemanticStore::open(&dir.path().join("test.db")).unwrap();

    store.upsert_belief("dog", "eats_at", "08:00,18:00", 0.5, "sensor").unwrap();
    store.confirm_belief("dog", "eats_at").unwrap();

    let b = store.get_belief("dog", "eats_at").unwrap().unwrap();
    assert_eq!(b.confirmation_count, 2); // 1 from upsert + 1 from confirm
    assert!(b.confidence > 0.5); // confidence increases on confirmation
}

#[test]
fn belief_contradiction_reduces_confidence() {
    let dir = tempfile::tempdir().unwrap();
    let store = SemanticStore::open(&dir.path().join("test.db")).unwrap();

    store.upsert_belief("dog", "eats_at", "18:00", 0.9, "sensor").unwrap();
    store.contradict_belief("dog", "eats_at").unwrap();
    store.contradict_belief("dog", "eats_at").unwrap();

    let b = store.get_belief("dog", "eats_at").unwrap().unwrap();
    assert!(b.confidence < 0.9);
    assert_eq!(b.contradiction_count, 2);
}
```

**Step 2: Run to confirm failure**
```bash
cargo test --lib -p bubbaloop belief 2>&1 | grep -E "FAILED|error"
```

**Step 3: Add Belief struct + table schema**

```rust
#[derive(Debug, Clone, serde::Serialize)]
pub struct Belief {
    pub id: String,
    pub subject: String,
    pub predicate: String,
    pub value: String,
    pub confidence: f64,
    pub source: String,            // "sensor" | "agent" | "user"
    pub first_observed: i64,
    pub last_confirmed: i64,
    pub confirmation_count: i32,
    pub contradiction_count: i32,
    pub notes: Option<String>,
}
```

In `SemanticStore::open()`, add:

```rust
conn.execute_batch("
    CREATE TABLE IF NOT EXISTS beliefs (
        id                  TEXT PRIMARY KEY,
        subject             TEXT NOT NULL,
        predicate           TEXT NOT NULL,
        value               TEXT NOT NULL,
        confidence          REAL NOT NULL DEFAULT 1.0,
        source              TEXT NOT NULL,
        first_observed      INTEGER NOT NULL,
        last_confirmed      INTEGER NOT NULL,
        confirmation_count  INTEGER NOT NULL DEFAULT 1,
        contradiction_count INTEGER NOT NULL DEFAULT 0,
        notes               TEXT,
        UNIQUE(subject, predicate)
    );
")?;
```

**Step 4: Implement belief CRUD**

```rust
pub fn upsert_belief(&self, subject: &str, predicate: &str, value: &str,
    confidence: f64, source: &str) -> super::Result<()> {
    let now = crate::agent::memory::now_epoch_secs() as i64;
    let id = format!("{}-{}", subject, predicate);
    self.conn.execute(
        "INSERT INTO beliefs (id, subject, predicate, value, confidence, source,
            first_observed, last_confirmed, confirmation_count, contradiction_count)
         VALUES (?1,?2,?3,?4,?5,?6,?7,?7,1,0)
         ON CONFLICT(subject, predicate) DO UPDATE SET
            value = excluded.value,
            confidence = MIN(1.0, (confidence + excluded.confidence) / 2.0),
            last_confirmed = excluded.last_confirmed,
            confirmation_count = confirmation_count + 1",
        rusqlite::params![id, subject, predicate, value, confidence, source, now],
    )?;
    Ok(())
}

pub fn confirm_belief(&self, subject: &str, predicate: &str) -> super::Result<()> {
    let now = crate::agent::memory::now_epoch_secs() as i64;
    self.conn.execute(
        "UPDATE beliefs SET
            confirmation_count = confirmation_count + 1,
            confidence = MIN(1.0, confidence + 0.05),
            last_confirmed = ?1
         WHERE subject = ?2 AND predicate = ?3",
        rusqlite::params![now, subject, predicate],
    )?;
    Ok(())
}

pub fn contradict_belief(&self, subject: &str, predicate: &str) -> super::Result<()> {
    self.conn.execute(
        "UPDATE beliefs SET
            contradiction_count = contradiction_count + 1,
            confidence = MAX(0.0, confidence - 0.15)
         WHERE subject = ?1 AND predicate = ?2",
        rusqlite::params![subject, predicate],
    )?;
    Ok(())
}

pub fn get_belief(&self, subject: &str, predicate: &str) -> super::Result<Option<Belief>> {
    let mut stmt = self.conn.prepare(
        "SELECT id,subject,predicate,value,confidence,source,first_observed,
                last_confirmed,confirmation_count,contradiction_count,notes
         FROM beliefs WHERE subject=?1 AND predicate=?2"
    )?;
    let mut rows = stmt.query(rusqlite::params![subject, predicate])?;
    if let Some(row) = rows.next()? {
        Ok(Some(Belief {
            id: row.get(0)?, subject: row.get(1)?, predicate: row.get(2)?,
            value: row.get(3)?, confidence: row.get(4)?, source: row.get(5)?,
            first_observed: row.get(6)?, last_confirmed: row.get(7)?,
            confirmation_count: row.get(8)?, contradiction_count: row.get(9)?,
            notes: row.get(10)?,
        }))
    } else {
        Ok(None)
    }
}

pub fn list_beliefs(&self) -> super::Result<Vec<Belief>> {
    let mut stmt = self.conn.prepare(
        "SELECT id,subject,predicate,value,confidence,source,first_observed,
                last_confirmed,confirmation_count,contradiction_count,notes
         FROM beliefs ORDER BY confidence DESC"
    )?;
    let beliefs = stmt.query_map([], |row| Ok(Belief {
        id: row.get(0)?, subject: row.get(1)?, predicate: row.get(2)?,
        value: row.get(3)?, confidence: row.get(4)?, source: row.get(5)?,
        first_observed: row.get(6)?, last_confirmed: row.get(7)?,
        confirmation_count: row.get(8)?, contradiction_count: row.get(9)?,
        notes: row.get(10)?,
    }))?.collect::<rusqlite::Result<Vec<_>>>()?;
    Ok(beliefs)
}
```

**Step 5: Run tests**
```bash
cargo test --lib -p bubbaloop belief 2>&1 | grep -E "ok|FAILED"
```

**Step 6: Commit**
```bash
git add crates/bubbaloop/src/agent/memory/semantic.rs
git commit -m "feat(memory): add beliefs table with confidence tracking"
```

---

### Task 0.3: Add salience and causal chain fields to EpisodicLog

**Files:**
- Modify: `crates/bubbaloop/src/agent/memory/episodic.rs`

**Step 1: Write failing test**

```rust
#[test]
fn causal_chain_roundtrips() {
    let dir = tempfile::tempdir().unwrap();
    let log = EpisodicLog::new(&dir.path().join("mem"), &dir.path().join("test.db")).unwrap();

    let cause_id = "evt-001".to_string();
    log.append_with_cause("system", "Motor overheated", Some(&cause_id), 0.9).unwrap();
    log.append_with_cause("assistant", "Reducing speed to 30%", Some(&cause_id), 0.7).unwrap();

    let chain = log.causal_chain(&cause_id).unwrap();
    assert_eq!(chain.len(), 2);
    assert_eq!(chain[0].role, "system");
    assert_eq!(chain[1].role, "assistant");
}
```

**Step 2: Run to confirm failure**
```bash
cargo test --lib -p bubbaloop causal_chain 2>&1 | grep -E "FAILED|error"
```

**Step 3: Extend LogEntry struct and FTS5 table**

In `episodic.rs`, extend `LogEntry`:

```rust
pub struct LogEntry {
    pub timestamp: String,
    pub role: String,
    pub content: String,
    pub job_id: Option<String>,
    pub flush: Option<bool>,
    // New fields
    pub id: Option<String>,        // UUID for causal chain linking
    pub cause_id: Option<String>,  // ID of the event that caused this entry
    pub salience: Option<f32>,     // 0.0-1.0 importance (None = not scored)
    pub mission_id: Option<String>,
}
```

Add migration in `EpisodicLog::new()` after the table creation:

```rust
// Add new columns if they don't exist (migration-safe ADD COLUMN)
for col in &["id TEXT", "cause_id TEXT", "salience REAL", "mission_id TEXT"] {
    let _ = conn.execute(&format!(
        "ALTER TABLE fts_episodic ADD COLUMN {}", col), []);
    // Ignore errors — column already exists
}
```

**Step 4: Add append_with_cause and causal_chain**

```rust
pub fn append_with_cause(&self, role: &str, content: &str,
    cause_id: Option<&str>, salience: f32) -> super::Result<String> {
    let id = uuid_v4(); // implement as: format!("{:x}", rand::random::<u64>())
    let entry = LogEntry {
        id: Some(id.clone()),
        timestamp: crate::agent::memory::now_rfc3339(),
        role: role.to_string(),
        content: content.to_string(),
        cause_id: cause_id.map(String::from),
        salience: Some(salience),
        job_id: None, flush: None, mission_id: None,
    };
    self.append(&entry)?;
    Ok(id)
}

pub fn causal_chain(&self, cause_id: &str) -> super::Result<Vec<LogEntry>> {
    let mut stmt = self.conn.prepare(
        "SELECT content, id, role, timestamp, job_id, cause_id, salience
         FROM fts_episodic WHERE cause_id = ?1 ORDER BY timestamp"
    )?;
    let entries = stmt.query_map(rusqlite::params![cause_id], |row| {
        Ok(LogEntry {
            content: row.get(0)?, id: row.get(1)?, role: row.get(2)?,
            timestamp: row.get(3)?, job_id: row.get(4)?,
            cause_id: row.get(5)?, salience: row.get(6)?,
            flush: None, mission_id: None,
        })
    })?.collect::<rusqlite::Result<Vec<_>>>()?;
    Ok(entries)
}

fn uuid_v4() -> String {
    use std::time::{SystemTime, UNIX_EPOCH};
    let t = SystemTime::now().duration_since(UNIX_EPOCH).unwrap_or_default();
    format!("{:x}-{:x}", t.as_nanos(), t.subsec_nanos() ^ std::process::id())
}
```

**Step 5: Run tests**
```bash
cargo test --lib -p bubbaloop causal_chain 2>&1 | grep -E "ok|FAILED"
cargo test --lib -p bubbaloop 2>&1 | tail -3
```
Expected: all existing tests still pass.

**Step 6: Commit**
```bash
git add crates/bubbaloop/src/agent/memory/episodic.rs
git commit -m "feat(memory): add causal chain fields and salience to episodic log"
```

---

## Phase 1: Context Providers

*Context providers are daemon background tasks that write to the world state.
No agent turn involvement. No LLM.*

### Task 1.1: ProviderConfig struct and missions.db

**Invariant targeted:** I3

**Files:**
- Create: `crates/bubbaloop/src/daemon/context_provider.rs`
- Modify: `crates/bubbaloop/src/daemon/mod.rs` (add mod declaration)

**Step 1: Write failing test**

Create `crates/bubbaloop/src/daemon/context_provider.rs`:

```rust
//! Context providers — daemon background tasks that populate Tier 0 world state.

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::tempdir;

    #[test]
    fn provider_config_roundtrips_to_db() {
        let dir = tempdir().unwrap();
        let store = ProviderStore::open(&dir.path().join("missions.db")).unwrap();

        let cfg = ProviderConfig {
            id: "test-provider".to_string(),
            mission_id: "dog-monitor".to_string(),
            topic_pattern: "bubbaloop/**/vision/detections".to_string(),
            world_state_key_template: "{label}.location".to_string(),
            value_field: "location".to_string(),
            filter: Some("label=dog".to_string()),
            min_interval_secs: 30,
            max_age_secs: 300,
            confidence_field: Some("confidence".to_string()),
            token_budget: 50,
        };

        store.save_provider(&cfg).unwrap();
        let loaded = store.list_providers().unwrap();
        assert_eq!(loaded.len(), 1);
        assert_eq!(loaded[0].id, "test-provider");
        assert_eq!(loaded[0].min_interval_secs, 30);
    }

    #[test]
    fn delete_provider_removes_it() {
        let dir = tempdir().unwrap();
        let store = ProviderStore::open(&dir.path().join("missions.db")).unwrap();

        let cfg = ProviderConfig {
            id: "p1".to_string(),
            mission_id: "m1".to_string(),
            topic_pattern: "**/test".to_string(),
            world_state_key_template: "test.key".to_string(),
            value_field: "value".to_string(),
            filter: None,
            min_interval_secs: 5,
            max_age_secs: 60,
            confidence_field: None,
            token_budget: 20,
        };
        store.save_provider(&cfg).unwrap();
        store.delete_provider("p1").unwrap();
        assert!(store.list_providers().unwrap().is_empty());
    }
}
```

**Step 2: Run to confirm failure**
```bash
cargo test --lib -p bubbaloop provider_config 2>&1 | grep -E "FAILED|error"
```

**Step 3: Implement ProviderConfig and ProviderStore**

```rust
use rusqlite::{params, Connection};
use std::path::Path;
use anyhow::Result;

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct ProviderConfig {
    pub id: String,
    pub mission_id: String,
    pub topic_pattern: String,
    pub world_state_key_template: String, // e.g. "{label}.location"
    pub value_field: String,              // JSON path in published message
    pub filter: Option<String>,           // e.g. "label=dog AND confidence>0.85"
    pub min_interval_secs: u32,
    pub max_age_secs: u32,
    pub confidence_field: Option<String>,
    pub token_budget: u32,
}

pub struct ProviderStore {
    conn: Connection,
}

impl ProviderStore {
    pub fn open(path: &Path) -> Result<Self> {
        if let Some(p) = path.parent() { std::fs::create_dir_all(p)?; }
        let conn = Connection::open(path)?;
        conn.query_row("PRAGMA journal_mode=WAL", [], |_| Ok(()))?;
        conn.execute_batch("
            CREATE TABLE IF NOT EXISTS context_providers (
                id                        TEXT PRIMARY KEY,
                mission_id                TEXT NOT NULL,
                topic_pattern             TEXT NOT NULL,
                world_state_key_template  TEXT NOT NULL,
                value_field               TEXT NOT NULL,
                filter                    TEXT,
                min_interval_secs         INTEGER NOT NULL DEFAULT 30,
                max_age_secs              INTEGER NOT NULL DEFAULT 300,
                confidence_field          TEXT,
                token_budget              INTEGER NOT NULL DEFAULT 50,
                created_at                INTEGER NOT NULL DEFAULT (strftime('%s','now'))
            );
        ")?;
        Ok(Self { conn })
    }

    pub fn save_provider(&self, cfg: &ProviderConfig) -> Result<()> {
        self.conn.execute(
            "INSERT OR REPLACE INTO context_providers
             (id,mission_id,topic_pattern,world_state_key_template,value_field,
              filter,min_interval_secs,max_age_secs,confidence_field,token_budget)
             VALUES (?1,?2,?3,?4,?5,?6,?7,?8,?9,?10)",
            params![cfg.id, cfg.mission_id, cfg.topic_pattern, cfg.world_state_key_template,
                cfg.value_field, cfg.filter, cfg.min_interval_secs, cfg.max_age_secs,
                cfg.confidence_field, cfg.token_budget],
        )?;
        Ok(())
    }

    pub fn list_providers(&self) -> Result<Vec<ProviderConfig>> {
        let mut stmt = self.conn.prepare(
            "SELECT id,mission_id,topic_pattern,world_state_key_template,value_field,
                    filter,min_interval_secs,max_age_secs,confidence_field,token_budget
             FROM context_providers"
        )?;
        let cfgs = stmt.query_map([], |row| Ok(ProviderConfig {
            id: row.get(0)?, mission_id: row.get(1)?,
            topic_pattern: row.get(2)?, world_state_key_template: row.get(3)?,
            value_field: row.get(4)?, filter: row.get(5)?,
            min_interval_secs: row.get(6)?, max_age_secs: row.get(7)?,
            confidence_field: row.get(8)?, token_budget: row.get(9)?,
        }))?.collect::<rusqlite::Result<Vec<_>>>()?;
        Ok(cfgs)
    }

    pub fn delete_provider(&self, id: &str) -> Result<()> {
        self.conn.execute("DELETE FROM context_providers WHERE id=?1", params![id])?;
        Ok(())
    }

    pub fn providers_for_mission(&self, mission_id: &str) -> Result<Vec<ProviderConfig>> {
        let mut stmt = self.conn.prepare(
            "SELECT id,mission_id,topic_pattern,world_state_key_template,value_field,
                    filter,min_interval_secs,max_age_secs,confidence_field,token_budget
             FROM context_providers WHERE mission_id=?1"
        )?;
        let cfgs = stmt.query_map(params![mission_id], |row| Ok(ProviderConfig {
            id: row.get(0)?, mission_id: row.get(1)?,
            topic_pattern: row.get(2)?, world_state_key_template: row.get(3)?,
            value_field: row.get(4)?, filter: row.get(5)?,
            min_interval_secs: row.get(6)?, max_age_secs: row.get(7)?,
            confidence_field: row.get(8)?, token_budget: row.get(9)?,
        }))?.collect::<rusqlite::Result<Vec<_>>>()?;
        Ok(cfgs)
    }
}
```

**Step 4: Add `pub mod context_provider;` to `daemon/mod.rs`**

Find the existing `mod` declarations in `daemon/mod.rs` and add:
```rust
pub mod context_provider;
```

**Step 5: Run tests**
```bash
cargo test --lib -p bubbaloop provider_config 2>&1 | grep -E "ok|FAILED"
cargo test --lib -p bubbaloop 2>&1 | tail -3
```

**Step 6: Commit**
```bash
git add crates/bubbaloop/src/daemon/context_provider.rs crates/bubbaloop/src/daemon/mod.rs
git commit -m "feat(daemon): add context provider config store (missions.db)"
```

---

### Task 1.2: ZenohContextProvider background task

**Invariant targeted:** I1 (single writer per key via tokio task per provider)

**Files:**
- Modify: `crates/bubbaloop/src/daemon/context_provider.rs`

**Step 1: Write integration test (no Zenoh — unit test the filter logic)**

```rust
#[test]
fn filter_matches_correctly() {
    assert!(apply_filter("label=dog AND confidence>0.85",
        &serde_json::json!({"label":"dog","confidence":0.97})));
    assert!(!apply_filter("label=dog AND confidence>0.85",
        &serde_json::json!({"label":"cat","confidence":0.97})));
    assert!(!apply_filter("label=dog AND confidence>0.85",
        &serde_json::json!({"label":"dog","confidence":0.50})));
}

#[test]
fn key_template_substitution() {
    let key = resolve_key_template("{label}.location",
        &serde_json::json!({"label":"dog","location":"kitchen"}));
    assert_eq!(key, "dog.location");
}

#[test]
fn value_extraction_from_json_path() {
    let val = extract_field("location",
        &serde_json::json!({"label":"dog","location":"kitchen","confidence":0.97}));
    assert_eq!(val, Some("kitchen".to_string()));
}
```

**Step 2: Run to confirm failure**
```bash
cargo test --lib -p bubbaloop filter_matches 2>&1 | grep -E "FAILED|error"
```

**Step 3: Implement filter, key template, and field extraction**

```rust
/// Minimal filter evaluator for "field=value AND field2>number" expressions.
/// Supports: =, !=, >, <, >=, <= operators. AND conjunction only.
/// Returns true if all conditions match. Unknown fields → false.
pub fn apply_filter(filter: &str, sample: &serde_json::Value) -> bool {
    filter.split(" AND ").all(|cond| {
        let cond = cond.trim();
        for op in &[">=", "<=", "!=", ">", "<", "="] {
            if let Some(pos) = cond.find(op) {
                let field = cond[..pos].trim();
                let expected = cond[pos + op.len()..].trim().trim_matches('"');
                let actual = sample.get(field).map(|v| match v {
                    serde_json::Value::String(s) => s.clone(),
                    other => other.to_string(),
                });
                return match (actual, op) {
                    (None, _) => false,
                    (Some(a), &"=") => a == expected,
                    (Some(a), &"!=") => a != expected,
                    (Some(a), &">") => a.parse::<f64>().ok()
                        .zip(expected.parse::<f64>().ok())
                        .map(|(av, ev)| av > ev).unwrap_or(false),
                    (Some(a), &"<") => a.parse::<f64>().ok()
                        .zip(expected.parse::<f64>().ok())
                        .map(|(av, ev)| av < ev).unwrap_or(false),
                    (Some(a), &">=") => a.parse::<f64>().ok()
                        .zip(expected.parse::<f64>().ok())
                        .map(|(av, ev)| av >= ev).unwrap_or(false),
                    (Some(a), &"<=") => a.parse::<f64>().ok()
                        .zip(expected.parse::<f64>().ok())
                        .map(|(av, ev)| av <= ev).unwrap_or(false),
                    _ => false,
                };
            }
        }
        false
    })
}

/// Replace {field} placeholders in template with values from sample.
pub fn resolve_key_template(template: &str, sample: &serde_json::Value) -> String {
    let mut result = template.to_string();
    if let Some(obj) = sample.as_object() {
        for (k, v) in obj {
            let placeholder = format!("{{{}}}", k);
            let val = match v {
                serde_json::Value::String(s) => s.clone(),
                other => other.to_string().trim_matches('"').to_string(),
            };
            result = result.replace(&placeholder, &val);
        }
    }
    result
}

/// Extract a field value from a JSON sample as a string.
pub fn extract_field(field: &str, sample: &serde_json::Value) -> Option<String> {
    sample.get(field).map(|v| match v {
        serde_json::Value::String(s) => s.clone(),
        other => other.to_string().trim_matches('"').to_string(),
    })
}
```

**Step 4: Implement ZenohContextProvider spawner**

```rust
use std::sync::Arc;
use tokio::sync::watch;
use crate::agent::memory::semantic::SemanticStore;

/// Spawn a background task for a single ProviderConfig.
/// Returns a drop guard — dropping it sends shutdown signal.
pub fn spawn_provider(
    cfg: ProviderConfig,
    session: Arc<zenoh::Session>,
    memory_backend: Arc<tokio::sync::Mutex<crate::agent::memory::MemoryBackend>>,
    mut shutdown: watch::Receiver<()>,
) -> tokio::task::JoinHandle<()> {
    tokio::spawn(async move {
        log::info!("[ContextProvider] Starting provider '{}' on '{}'", cfg.id, cfg.topic_pattern);

        let subscriber = match session.declare_subscriber(&cfg.topic_pattern).await {
            Ok(s) => s,
            Err(e) => {
                log::error!("[ContextProvider] Failed to subscribe '{}': {}", cfg.topic_pattern, e);
                return;
            }
        };

        let mut last_update: std::collections::HashMap<String, u64> = Default::default();

        loop {
            tokio::select! {
                _ = shutdown.changed() => {
                    log::info!("[ContextProvider] Shutting down provider '{}'", cfg.id);
                    break;
                }
                sample = subscriber.recv_async() => {
                    let Ok(sample) = sample else { break; };

                    // Parse payload as JSON
                    let payload = match std::str::from_utf8(&sample.payload().to_bytes()) {
                        Ok(s) => s.to_string(),
                        Err(_) => continue,
                    };
                    let json: serde_json::Value = match serde_json::from_str(&payload) {
                        Ok(v) => v,
                        Err(_) => continue,
                    };

                    // Apply filter
                    if let Some(ref f) = cfg.filter {
                        if !apply_filter(f, &json) { continue; }
                    }

                    // Rate limit
                    let now = crate::agent::memory::now_epoch_secs();
                    let key = resolve_key_template(&cfg.world_state_key_template, &json);
                    let last = last_update.get(&key).copied().unwrap_or(0);
                    if now - last < cfg.min_interval_secs as u64 { continue; }
                    last_update.insert(key.clone(), now);

                    // Extract value and confidence
                    let value = match extract_field(&cfg.value_field, &json) {
                        Some(v) => v,
                        None => continue,
                    };
                    let confidence = cfg.confidence_field.as_deref()
                        .and_then(|f| extract_field(f, &json))
                        .and_then(|s| s.parse::<f64>().ok())
                        .unwrap_or(1.0);

                    // Write to world state
                    let backend = memory_backend.lock().await;
                    if let Err(e) = backend.semantic.upsert_world_state(
                        &key, &value, confidence,
                        Some(sample.key_expr().as_str()),
                        None,
                        cfg.max_age_secs as i64,
                    ) {
                        log::warn!("[ContextProvider] Write failed for '{}': {}", key, e);
                    }
                }
            }
        }
    })
}
```

**Step 5: Run all tests**
```bash
cargo test --lib -p bubbaloop 2>&1 | tail -5
```

**Step 6: Commit**
```bash
git add crates/bubbaloop/src/daemon/context_provider.rs
git commit -m "feat(daemon): implement ZenohContextProvider with filter, template, decimation"
```

---

### Task 1.3: configure_context MCP tool

**Files:**
- Modify: `crates/bubbaloop/src/mcp/tools.rs` (add tool handler)
- Modify: `crates/bubbaloop/src/mcp/mod.rs` (register tool)
- Modify: `crates/bubbaloop/src/mcp/platform.rs` (add ConfigureContext to platform trait)

**Step 1: Write test against MockPlatform**

In `mcp/mock_platform.rs` tests or `integration_mcp`, add:

```rust
#[tokio::test]
async fn configure_context_tool_validates_input() {
    let platform = MockPlatform::new();
    // Empty topic_pattern should fail validation
    let result = platform.configure_context(ConfigureContextParams {
        mission_id: "test".to_string(),
        topic_pattern: "".to_string(),       // invalid
        world_state_key_template: "x".to_string(),
        value_field: "v".to_string(),
        filter: None,
        min_interval_secs: Some(30),
        max_age_secs: Some(300),
        confidence_field: None,
        token_budget: Some(50),
    }).await;
    assert!(result.is_err());
}
```

**Step 2: Add ConfigureContextParams to platform.rs**

```rust
#[derive(Debug, serde::Deserialize, schemars::JsonSchema)]
pub struct ConfigureContextParams {
    pub mission_id: String,
    pub topic_pattern: String,
    pub world_state_key_template: String,
    pub value_field: String,
    pub filter: Option<String>,
    pub min_interval_secs: Option<u32>,
    pub max_age_secs: Option<u32>,
    pub confidence_field: Option<String>,
    pub token_budget: Option<u32>,
}
```

Add to `PlatformOperations` trait:
```rust
async fn configure_context(&self, params: ConfigureContextParams)
    -> anyhow::Result<String>;
```

**Step 3: Implement in DaemonPlatform**

```rust
async fn configure_context(&self, params: ConfigureContextParams) -> anyhow::Result<String> {
    if params.topic_pattern.is_empty() {
        anyhow::bail!("topic_pattern cannot be empty");
    }
    if params.world_state_key_template.is_empty() {
        anyhow::bail!("world_state_key_template cannot be empty");
    }
    crate::validation::validate_query_key_expr(&params.topic_pattern)
        .map_err(|e| anyhow::anyhow!("invalid topic_pattern: {e}"))?;

    let id = format!("{}-{}", params.mission_id,
        crate::agent::memory::now_epoch_secs());
    let cfg = crate::daemon::context_provider::ProviderConfig {
        id: id.clone(),
        mission_id: params.mission_id,
        topic_pattern: params.topic_pattern,
        world_state_key_template: params.world_state_key_template,
        value_field: params.value_field,
        filter: params.filter,
        min_interval_secs: params.min_interval_secs.unwrap_or(30),
        max_age_secs: params.max_age_secs.unwrap_or(300),
        confidence_field: params.confidence_field,
        token_budget: params.token_budget.unwrap_or(50),
    };

    self.provider_store.save_provider(&cfg)?;
    // TODO Phase 1 tail: signal daemon to spawn the new provider task
    log::info!("[MCP] tool=configure_context id={}", id);
    Ok(format!("Context provider '{}' registered. Will start on next daemon tick.", id))
}
```

**Step 4: Run tests**
```bash
cargo test --lib -p bubbaloop 2>&1 | tail -3
pixi run clippy 2>&1 | tail -5
```

**Step 5: Commit**
```bash
git add crates/bubbaloop/src/mcp/ crates/bubbaloop/src/daemon/context_provider.rs
git commit -m "feat(mcp): add configure_context tool with validation"
```

---

### Task 1.4: Inject Tier 0 into agent prompt

**Files:**
- Modify: `crates/bubbaloop/src/agent/prompt.rs`

**Step 1: Write failing test**

```rust
#[test]
fn world_state_snapshot_injected_into_prompt() {
    let entries = vec![
        WorldStateEntry {
            key: "dog.location".to_string(),
            value: "kitchen".to_string(),
            confidence: 0.97,
            source_topic: None, source_node: None,
            last_seen_at: now_epoch_secs() as i64,
            max_age_secs: 300,
            stale: false,
        },
        WorldStateEntry {
            key: "robot.position".to_string(),
            value: "[0.3, 0.1]".to_string(),
            confidence: 1.0,
            source_topic: None, source_node: None,
            last_seen_at: now_epoch_secs() as i64 - 3600,
            max_age_secs: 10,
            stale: true,
        },
    ];
    let prompt = format_world_state_section(&entries, 200);
    assert!(prompt.contains("dog.location"));
    assert!(prompt.contains("kitchen"));
    assert!(prompt.contains("STALE") || prompt.contains("⚠"));
    // Token budget: must not exceed ~200 tokens
    assert!(prompt.len() < 1200); // rough char estimate
}
```

**Step 2: Implement format_world_state_section in prompt.rs**

```rust
pub fn format_world_state_section(entries: &[WorldStateEntry], token_budget: usize) -> String {
    if entries.is_empty() { return String::new(); }

    let mut lines = vec!["WORLD STATE:".to_string()];
    let mut approx_tokens = 3usize;

    // Safety-critical (stale) always first
    let (stale, fresh): (Vec<_>, Vec<_>) = entries.iter().partition(|e| e.stale);

    for entry in stale.iter().chain(fresh.iter()) {
        let line = if entry.stale {
            format!("  ⚠ {} = {} [STALE: conf={:.2}]",
                entry.key, entry.value, entry.confidence)
        } else {
            format!("  {} = {} [conf={:.2}]",
                entry.key, entry.value, entry.confidence)
        };
        let line_tokens = line.len() / 4 + 1; // rough estimate
        if approx_tokens + line_tokens > token_budget { break; }
        approx_tokens += line_tokens;
        lines.push(line);
    }

    lines.join("\n")
}
```

**Step 3: Wire into build_system_prompt_with_soul_path**

Find where the system prompt is assembled in `agent/prompt.rs` and add the world state section.
The world state is fetched via `memory.backend.lock().await.semantic.world_state_snapshot()`.

**Step 4: Run tests**
```bash
cargo test --lib -p bubbaloop world_state_snapshot 2>&1 | grep -E "ok|FAILED"
cargo test --lib -p bubbaloop 2>&1 | tail -3
```

**Step 5: Commit**
```bash
git add crates/bubbaloop/src/agent/prompt.rs
git commit -m "feat(agent): inject Tier 0 world state into system prompt with staleness warnings"
```

---

## Phase 2: Mission Model

### Task 2.1: Mission struct and missions.db

**Files:**
- Create: `crates/bubbaloop/src/daemon/mission.rs`
- Modify: `crates/bubbaloop/src/daemon/mod.rs`

**Step 1: Write failing tests**

```rust
#[test]
fn mission_store_roundtrips() {
    let dir = tempdir().unwrap();
    let store = MissionStore::open(&dir.path().join("missions.db")).unwrap();

    let m = Mission {
        id: "dog-monitor".to_string(),
        markdown: "Watch the kitchen camera.".to_string(),
        status: MissionStatus::Active,
        expires_at: None,
        resources: vec![],
        sub_mission_ids: vec![],
        depends_on: vec![],
        compiled_at: now_epoch_secs() as i64,
    };
    store.save_mission(&m).unwrap();

    let loaded = store.get_mission("dog-monitor").unwrap().unwrap();
    assert_eq!(loaded.id, "dog-monitor");
    assert_eq!(loaded.status, MissionStatus::Active);
}

#[test]
fn mission_expiry_detection() {
    let dir = tempdir().unwrap();
    let store = MissionStore::open(&dir.path().join("missions.db")).unwrap();

    let past = now_epoch_secs() as i64 - 10;
    let m = Mission {
        id: "temp-mission".to_string(),
        markdown: "Short mission".to_string(),
        status: MissionStatus::Active,
        expires_at: Some(past), // already expired
        resources: vec![],
        sub_mission_ids: vec![],
        depends_on: vec![],
        compiled_at: now_epoch_secs() as i64,
    };
    store.save_mission(&m).unwrap();
    let expired = store.expired_missions().unwrap();
    assert_eq!(expired.len(), 1);
    assert_eq!(expired[0].id, "temp-mission");
}
```

**Step 2: Implement MissionStore**

Follow the exact same pattern as `ProviderStore` — `Connection::open`, `execute_batch` for schema,
`save_mission`, `get_mission`, `list_missions`, `update_status`, `expired_missions`.

Mission table:
```sql
CREATE TABLE IF NOT EXISTS missions (
    id              TEXT PRIMARY KEY,
    markdown        TEXT NOT NULL,
    status          TEXT NOT NULL DEFAULT 'active',
    expires_at      INTEGER,
    resources       TEXT NOT NULL DEFAULT '[]',   -- JSON array of strings
    sub_mission_ids TEXT NOT NULL DEFAULT '[]',
    depends_on      TEXT NOT NULL DEFAULT '[]',
    compiled_at     INTEGER NOT NULL
);
```

**Step 3: Run tests, commit**
```bash
cargo test --lib -p bubbaloop mission_store 2>&1 | grep -E "ok|FAILED"
git add crates/bubbaloop/src/daemon/mission.rs crates/bubbaloop/src/daemon/mod.rs
git commit -m "feat(daemon): add mission model with expiry detection"
```

---

### Task 2.2: Mission file watcher

**Files:**
- Modify: `crates/bubbaloop/src/daemon/mission.rs`

The watcher polls `~/.bubbaloop/agents/{id}/missions/` on a 5-second interval (no inotify dep —
avoid new crate dependency, use polling for simplicity):

```rust
pub async fn watch_missions_dir(
    missions_dir: PathBuf,
    store: Arc<MissionStore>,
    setup_tx: tokio::sync::mpsc::Sender<String>, // sends mission_id to trigger setup turn
    mut shutdown: watch::Receiver<()>,
) {
    let mut known: HashMap<String, std::time::SystemTime> = HashMap::new();
    let mut interval = tokio::time::interval(std::time::Duration::from_secs(5));

    loop {
        tokio::select! {
            _ = shutdown.changed() => break,
            _ = interval.tick() => {
                let Ok(entries) = std::fs::read_dir(&missions_dir) else { continue };
                for entry in entries.flatten() {
                    let path = entry.path();
                    let Some(ext) = path.extension() else { continue };
                    if ext != "md" { continue }
                    let Ok(meta) = path.metadata() else { continue };
                    let Ok(mtime) = meta.modified() else { continue };
                    let id = path.file_stem().unwrap_or_default()
                        .to_string_lossy().to_string();
                    let changed = known.get(&id).map(|&t| t != mtime).unwrap_or(true);
                    if changed {
                        known.insert(id.clone(), mtime);
                        log::info!("[MissionWatcher] Detected change: {}", id);
                        let _ = setup_tx.send(id).await;
                    }
                }
            }
        }
    }
}
```

**Test:** Write a test that creates a temp dir with a `.md` file, runs the watcher briefly,
and confirms the setup channel receives the mission ID.

**Commit:**
```bash
git commit -m "feat(daemon): add mission file watcher with 5s polling"
```

---

### Task 2.3: Mission lifecycle MCP tools

Add these four tools following the exact same pattern as existing tools in `mcp/tools.rs`:

- `list_missions` (Viewer) — returns JSON list of missions + status
- `pause_mission` (Operator) — sets status to "paused"
- `resume_mission` (Operator) — sets status to "active"
- `cancel_mission` (Operator) — sets status to "cancelled", tears down providers

Each tool: write test → run to fail → implement → run to pass → commit separately.

**Commit per tool:**
```bash
git commit -m "feat(mcp): add list_missions tool"
git commit -m "feat(mcp): add pause_mission / resume_mission tools"
git commit -m "feat(mcp): add cancel_mission tool with provider teardown"
```

---

## Phase 3: Reactive Pre-filter

### Task 3.1: Rule engine and register_alert tool

**Files:**
- Create: `crates/bubbaloop/src/daemon/reactive.rs`

**Invariant targeted:** Reactive layer never acts — only adjusts arousal.

**Step 1: Write tests for predicate evaluation**

```rust
#[test]
fn sql_predicate_evaluates_against_world_state() {
    let mut ws = HashMap::new();
    ws.insert("toddler.near_stairs", "true");
    ws.insert("toddler.confidence", "0.91");

    assert!(eval_predicate(
        "toddler.near_stairs = 'true' AND toddler.confidence > 0.85", &ws));
    assert!(!eval_predicate(
        "toddler.near_stairs = 'true' AND toddler.confidence > 0.95", &ws));
}

#[test]
fn rule_respects_debounce() {
    let rule = ReactiveRule {
        id: "r1".to_string(), mission_id: "m1".to_string(),
        predicate: "x = '1'".to_string(),
        debounce_secs: 60, arousal_boost: 2.0,
        description: "test rule".to_string(),
        last_fired_at: AtomicI64::new(now_epoch_secs() as i64 - 10), // fired 10s ago
    };
    let mut ws = HashMap::new();
    ws.insert("x", "1");
    // Debounce = 60s, last fired 10s ago → should NOT fire
    assert!(!rule.should_fire(&ws));
}
```

**Step 2: Implement ReactiveRule and evaluator**

Use the same `apply_filter` logic from `context_provider.rs` — reuse, don't duplicate.

```rust
pub struct ReactiveRule {
    pub id: String,
    pub mission_id: String,
    pub predicate: String,
    pub debounce_secs: u32,
    pub arousal_boost: f64,
    pub description: String,
    pub last_fired_at: std::sync::atomic::AtomicI64,
}

impl ReactiveRule {
    pub fn should_fire(&self, world_state: &HashMap<&str, &str>) -> bool {
        let now = crate::agent::memory::now_epoch_secs() as i64;
        let last = self.last_fired_at.load(std::sync::atomic::Ordering::Relaxed);
        if now - last < self.debounce_secs as i64 { return false; }
        eval_predicate(&self.predicate, world_state)
    }

    pub fn fire(&self) -> f64 {
        let now = crate::agent::memory::now_epoch_secs() as i64;
        self.last_fired_at.store(now, std::sync::atomic::Ordering::Relaxed);
        self.arousal_boost
    }
}
```

**Step 3: Wire into heartbeat tick**

In `agent/heartbeat.rs`, add a new `ArousalSource::ReactiveRuleFired` variant.
In the daemon's heartbeat collection step, evaluate all active rules against the current
world state snapshot. For each rule that fires, add the arousal boost.

**Step 4: Add `register_alert` MCP tool**

Saves a `ReactiveRule` to `missions.db`. Follows same pattern as `configure_context`.

**Step 5: Run all tests, commit**
```bash
cargo test --lib -p bubbaloop 2>&1 | tail -3
git commit -m "feat(daemon): reactive pre-filter with rule engine and register_alert tool"
```

---

## Phase 4: Safety Layer

*This phase must be complete before Phase 5 (DAG). Safety cannot depend on planning code.*

### Task 4.1: Constraint engine

**Files:**
- Create: `crates/bubbaloop/src/daemon/constraints.rs`

**Invariant targeted:** I2 (violations synchronously rejected before Zenoh publish)

**Step 1: Write tests — constraints are the most safety-critical code**

```rust
#[test]
fn workspace_constraint_rejects_out_of_bounds() {
    let c = Constraint::Workspace { x: (-0.8, 0.8), y: (-0.8, 0.8), z: (0.0, 1.2) };
    assert!(c.validate_goal(&[0.5, 0.3, 0.8]).is_ok());
    assert!(c.validate_goal(&[0.95, 0.0, 0.5]).is_err()); // x=0.95 > 0.8
    assert!(c.validate_goal(&[0.0, 0.0, -0.1]).is_err()); // z=-0.1 < 0.0
}

#[test]
fn velocity_constraint_rejects_excess_speed() {
    let c = Constraint::MaxVelocity(0.3);
    assert!(c.validate_velocity(0.2).is_ok());
    assert!(c.validate_velocity(0.5).is_err());
}

#[test]
fn constraint_engine_validates_all_active_constraints() {
    let engine = ConstraintEngine::new(vec![
        Constraint::Workspace { x: (-1.0, 1.0), y: (-1.0, 1.0), z: (0.0, 2.0) },
        Constraint::MaxVelocity(0.5),
    ]);
    // Valid goal
    assert!(engine.validate_position_goal(&[0.3, 0.2, 0.5], Some(0.4)).is_ok());
    // Out-of-workspace goal
    assert!(engine.validate_position_goal(&[1.5, 0.0, 0.5], Some(0.4)).is_err());
    // Valid position, excessive velocity
    assert!(engine.validate_position_goal(&[0.3, 0.2, 0.5], Some(0.8)).is_err());
}
```

**Step 2: Implement ConstraintEngine**

```rust
#[derive(Debug, Clone)]
pub enum Constraint {
    Workspace { x: (f64, f64), y: (f64, f64), z: (f64, f64) },
    MaxVelocity(f64),
    ForbiddenZone { center: [f64; 3], radius: f64 },
    MaxForce(f64),
}

impl Constraint {
    pub fn validate_goal(&self, position: &[f64]) -> anyhow::Result<()> {
        match self {
            Constraint::Workspace { x, y, z } => {
                let check = |v: f64, (lo, hi): (f64, f64), axis: &str| {
                    if v < lo || v > hi {
                        anyhow::bail!("workspace.{} limit [{},{}] exceeded ({})", axis, lo, hi, v)
                    }
                    Ok(())
                };
                check(position.first().copied().unwrap_or(0.0), *x, "x")?;
                check(position.get(1).copied().unwrap_or(0.0), *y, "y")?;
                check(position.get(2).copied().unwrap_or(0.0), *z, "z")?;
                Ok(())
            }
            Constraint::ForbiddenZone { center, radius } => {
                let dist = (0..3).map(|i| {
                    let d = position.get(i).copied().unwrap_or(0.0) - center[i];
                    d * d
                }).sum::<f64>().sqrt();
                if dist < *radius {
                    anyhow::bail!("position inside forbidden zone (dist={:.3} < r={:.3})", dist, radius)
                }
                Ok(())
            }
            _ => Ok(()), // velocity/force checked separately
        }
    }

    pub fn validate_velocity(&self, speed: f64) -> anyhow::Result<()> {
        if let Constraint::MaxVelocity(max) = self {
            if speed > *max {
                anyhow::bail!("velocity {:.3} exceeds max {:.3}", speed, max)
            }
        }
        Ok(())
    }
}

pub struct ConstraintEngine {
    constraints: Vec<Constraint>,
}

impl ConstraintEngine {
    pub fn new(constraints: Vec<Constraint>) -> Self { Self { constraints } }

    pub fn validate_position_goal(&self, pos: &[f64], velocity: Option<f64>) -> anyhow::Result<()> {
        for c in &self.constraints {
            c.validate_goal(pos)?;
            if let Some(v) = velocity { c.validate_velocity(v)?; }
        }
        Ok(())
    }
}
```

**Step 3: Run tests — ALL must pass**
```bash
cargo test --lib -p bubbaloop constraint 2>&1 | grep -E "ok|FAILED"
```
If ANY constraint test fails: **stop. Do not proceed to Phase 5 until all pass.**

**Step 4: Commit**
```bash
git commit -m "feat(safety): constraint engine with workspace/velocity/forbidden-zone validation"
```

---

### Task 4.2: Resource locking

**Files:**
- Modify: `crates/bubbaloop/src/daemon/constraints.rs`

```rust
#[test]
fn resource_lock_exclusive_acquisition() {
    let registry = ResourceRegistry::new();
    assert!(registry.acquire("robot_arm", "mission-a").is_ok());
    assert!(registry.acquire("robot_arm", "mission-b").is_err()); // already locked
    registry.release("robot_arm", "mission-a").unwrap();
    assert!(registry.acquire("robot_arm", "mission-b").is_ok()); // now available
}

#[test]
fn resource_lock_idempotent_release() {
    let registry = ResourceRegistry::new();
    registry.acquire("robot_arm", "m1").unwrap();
    registry.release("robot_arm", "m1").unwrap();
    registry.release("robot_arm", "m1").unwrap(); // should not panic
}
```

Implement `ResourceRegistry` using `Arc<Mutex<HashMap<String, String>>>`.

**Invariant I6:** Add `Drop` impl to a `ResourceGuard` that calls `registry.release()`.

```bash
git commit -m "feat(safety): resource registry with exclusive locking and drop-guard"
```

---

### Task 4.3: register_constraint MCP tool + compiled fallback actions

Add `register_constraint` (Admin RBAC) MCP tool.
Add `CompiledFallback` enum with `StopVla`, `PauseAllMissions`, `AlertAgent` variants.
Store fallback config in `missions.db`.

Test: constraint registered via tool → stored in DB → retrievable on daemon restart.

```bash
git commit -m "feat(mcp): add register_constraint tool and compiled fallback actions"
```

---

## Phase 5: Mission DAG

### Task 5.1: Sub-mission lifecycle

**Files:**
- Modify: `crates/bubbaloop/src/daemon/mission.rs`

```rust
#[test]
fn sub_mission_activates_when_parent_active_and_no_depends_on() {
    // Sub-mission with no depends_on → activates immediately when parent active
}

#[test]
fn sub_mission_waits_for_dependency_completion() {
    // sub-002 depends_on [sub-001] → stays pending until sub-001 completed
}

#[test]
fn dag_evaluator_returns_ready_missions() {
    // Given a DAG with 3 nodes where 2 depends on 1, only 1 is ready initially
}
```

Implement `DagEvaluator::ready_missions()` — returns missions whose `depends_on` are all
in `completed` status. Called on every daemon heartbeat tick.

```bash
git commit -m "feat(daemon): mission DAG evaluator with dependency resolution"
```

---

### Task 5.2: Micro-turns for plan validation

**Files:**
- Modify: `crates/bubbaloop/src/agent/mod.rs`

A micro-turn is a minimal `run_agent_turn()` call with:
- No conversation history (empty `short_term`)
- No episodic recall
- World state only + sub-mission description
- Hard token limit: 200 tokens in, 50 out
- Response parsed for `VALID` or `INVALID [reason]`

```rust
pub async fn run_micro_turn(
    sub_mission_description: &str,
    world_state: &str,
    provider: &dyn ModelProvider,
) -> anyhow::Result<bool> {
    let prompt = format!(
        "Current world state:\n{}\n\nAbout to execute: {}\n\
         Is this still valid? Reply exactly: VALID or INVALID [reason]",
        world_state, sub_mission_description
    );
    // Single LLM call, no tools, no history
    // Parse response: starts with "VALID" → true, else false
}
```

Test: mock provider returns "VALID" → `true`. Returns "INVALID obstacle detected" → `false`.

```bash
git commit -m "feat(agent): micro-turn for sub-mission plan validation"
```

---

## Phase 6: Belief System

### Task 6.1: Belief update loop (daemon background)

**Files:**
- Create: `crates/bubbaloop/src/daemon/belief_updater.rs`

The belief updater runs on every context provider write. When world state key `X.Y` is updated,
it checks if there's a belief `(X, Y)` and calls `confirm_belief` or `contradict_belief` based
on whether the new value matches the existing belief.

```rust
#[test]
fn observation_confirms_matching_belief() {
    // belief: dog eats_at "08:00,18:00", confidence 0.8
    // observation: dog.eats_at = "08:00" → partial match → confirm
}

#[test]
fn observation_contradicts_mismatched_belief() {
    // belief: dog eats_at "18:00", confidence 0.9
    // observation: dog.last_seen_at_bowl = "never_after_5pm" × 10 days → contradict
}
```

```bash
git commit -m "feat(daemon): belief update loop wired to context provider writes"
```

---

### Task 6.2: get_belief / update_belief MCP tools

Follow existing tool pattern. `get_belief(subject, predicate)` → returns Belief JSON.
`update_belief(subject, predicate, value, confidence, notes)` → manual agent/user assertion.

```bash
git commit -m "feat(mcp): add get_belief and update_belief tools"
```

---

## Phase 7: Federated Agents

*This phase is optional for v1 but specified here for completeness.*

### Task 7.1: World state gossip publisher

Each agent publishes its world state snapshot to:
`bubbaloop/{scope}/{machine_id}/agent/{agent_id}/world_state`

Publish interval: configurable (default 30s). Payload: JSON diff of changed entries since
last publish.

### Task 7.2: Remote world state subscriber

Daemon subscribes to `bubbaloop/**/agent/*/world_state` (wildcard, other machines).
Merges remote entries into local world state with `remote:{machine_id}.` key prefix.
Remote entries have shorter `max_age_secs` (source machine's freshness estimate).

### Task 7.3: Quorum config and enforcement

`QuorumConfig` stored in `missions.db`. Evaluated by reactive pre-filter: rule fires only
when N machines have confirmed the condition within the agreement window.

---

## Verification Checklist (NASA-style, before any PR merge)

Run these in order. **Any failure blocks merge.**

```bash
# 1. All invariants tested
cargo test --lib -p bubbaloop 2>&1 | grep -E "^test result"
# Expected: N passed; 0 failed

# 2. Zero clippy warnings
cargo clippy --lib -p bubbaloop -- -D warnings 2>&1 | tail -3
# Expected: Finished (no warnings)

# 3. Binary size within budget
cargo build --release -p bubbaloop 2>&1 && \
ls -lh target/release/bubbaloop | awk '{print $5}'
# Expected: ≤ 14 MB

# 4. Safety invariant I2 (constraint engine never panics on adversarial input)
cargo test --lib -p bubbaloop constraint 2>&1 | grep -E "ok|FAILED"
# Expected: all ok

# 5. Integration tests (if Zenoh available)
cargo test --features test-harness --test integration_mcp 2>&1 | tail -3

# 6. Format check
pixi run fmt-check
```

---

## Implementation Order Summary

```
Phase 0  (DB)            → Task 0.1 → 0.2 → 0.3
Phase 1  (Providers)     → Task 1.1 → 1.2 → 1.3 → 1.4
Phase 2  (Missions)      → Task 2.1 → 2.2 → 2.3
Phase 3  (Reactive)      → Task 3.1
Phase 4  (Safety) ⚠️     → Task 4.1 → 4.2 → 4.3   ← MUST complete before Phase 5
Phase 5  (DAG)           → Task 5.1 → 5.2
Phase 6  (Beliefs)       → Task 6.1 → 6.2
Phase 7  (Federation)    → Task 7.1 → 7.2 → 7.3   ← optional v1
```

**Estimated new test count:** ~80 unit tests across all phases.
**Estimated new code:** ~3,000–4,000 lines Rust.
**Estimated binary size addition:** ~200 KB.
**New crate dependencies:** None.
