# FTS5 Semantic Memory Layer

**Date:** 2026-02-28
**Status:** Approved
**Approach:** Phase 1 of 2 — FTS5 full-text search (no new deps, no unsafe)

## Context

The agent memory system (SQLite) currently uses recency-only retrieval — last 20 messages, last 10 events. This misses relevant past context. Adding semantic search enables the agent to retrieve conversations, events, and skills by meaning.

## Design Decision

After evaluating LanceDB (too heavy for Jetson: +30MB binary, Arrow/DataFusion deps) and comparing with OpenClaw (sqlite-vec) and ZeroClaw (SQLite BLOBs + FTS5), we chose a phased approach:

- **Phase 1 (this PR):** FTS5 full-text search tables in the existing SQLite DB. Zero new deps, zero `unsafe`, zero binary size increase. FTS5 is built into rusqlite's `bundled` feature.
- **Phase 2 (future):** Add `sqlite-vec` (~200KB) for vector search with embeddings. One `unsafe` block for `sqlite3_auto_extension`. Hybrid FTS5+vec0 search using Reciprocal Rank Fusion.

## Architecture

```
SQLite memory.db
├── conversations          (existing — CRUD, pruning)
├── sensor_events          (existing — CRUD, pruning)
├── schedules              (existing — CRUD)
├── fts_conversations      (NEW — FTS5 over conversation content)
├── fts_events             (NEW — FTS5 over event details)
└── fts_skills             (NEW — FTS5 over skill name + body)
```

### Data Flow

1. **Write path:** `log_message()` and `log_event()` dual-write to both the regular table and the corresponding FTS5 table. Skill indexing happens at startup.
2. **Read path:** New `search_conversations(query, limit)`, `search_events(query, limit)`, `search_skills(query, limit)` methods query FTS5 tables with BM25 ranking.
3. **Context injection:** `build_system_prompt()` gains a `relevant_context` parameter — results from semantic search on the current user input, injected alongside recency-based context.

### FTS5 Table Schemas

```sql
-- Conversations: search over message content
CREATE VIRTUAL TABLE IF NOT EXISTS fts_conversations USING fts5(
    id UNINDEXED,
    role UNINDEXED,
    content,
    timestamp UNINDEXED
);

-- Events: search over node_name, event_type, and details
CREATE VIRTUAL TABLE IF NOT EXISTS fts_events USING fts5(
    id UNINDEXED,
    node_name,
    event_type,
    details,
    timestamp UNINDEXED
);

-- Skills: search over skill name and markdown body
CREATE VIRTUAL TABLE IF NOT EXISTS fts_skills USING fts5(
    name,
    driver UNINDEXED,
    body
);
```

### Search API

```rust
// memory.rs — new methods
pub fn search_conversations(&self, query: &str, limit: usize) -> Result<Vec<ConversationRow>>;
pub fn search_events(&self, query: &str, limit: usize) -> Result<Vec<SensorEvent>>;
pub fn search_skills(&self, query: &str, limit: usize) -> Result<Vec<SkillSearchResult>>;

// New struct for skill search results
pub struct SkillSearchResult {
    pub name: String,
    pub driver: String,
    pub body: String,
    pub rank: f64,
}
```

### Retriever Trait (future-proofing for Phase 2)

```rust
pub trait Retriever {
    fn retrieve_conversations(&self, query: &str, limit: usize) -> Result<Vec<ConversationRow>>;
    fn retrieve_events(&self, query: &str, limit: usize) -> Result<Vec<SensorEvent>>;
    fn retrieve_skills(&self, query: &str, limit: usize) -> Result<Vec<SkillSearchResult>>;
}
```

Phase 1: `FtsRetriever` (FTS5 only). Phase 2: `HybridRetriever` (FTS5 + vec0 RRF blend).

### Context Injection Changes

In `mod.rs` REPL loop, before sending to Claude:

```rust
// Retrieve relevant context using FTS5 search on user input
let relevant_convos = memory.search_conversations(input, 5).unwrap_or_default();
let relevant_events = memory.search_events(input, 5).unwrap_or_default();
let relevant_skills = memory.search_skills(input, 3).unwrap_or_default();

// Build system prompt with both recency + relevance context
let system_prompt = build_system_prompt(
    &inventory, &schedules, &recent_events, &skills,
    &relevant_convos, &relevant_events, &relevant_skills,
);
```

### Prompt Changes

Add a new section to the system prompt between recent events and skills:

```
## Relevant Context (from past conversations and events)

[FTS5 search results matching the current user query]
```

## Files to Modify

| File | Changes |
|------|---------|
| `memory.rs` | Add FTS5 table creation in `open()`, dual-write in `log_message`/`log_event`, add `search_*` methods, `index_skills()`, `SkillSearchResult` struct, `Retriever` trait |
| `mod.rs` | Call `index_skills()` at startup, add FTS5 search before each Claude call, pass results to `build_system_prompt()` |
| `prompt.rs` | Add `relevant_context` parameters to `build_system_prompt()`, render relevant results section |

## Tests

- `fts_conversations_search` — insert messages, verify BM25 ranking
- `fts_events_search` — insert events, verify search by node_name and details
- `fts_skills_search` — index skills, verify search by name and body
- `fts_dual_write` — verify log_message writes to both tables
- `fts_empty_query` — verify graceful handling of empty/no-match queries
- Existing tests remain unchanged (FTS5 tables are additive)

## Verification

```bash
pixi run check          # Compilation
pixi run clippy         # Zero warnings
pixi run test           # All unit tests
cargo test --lib -- memory::tests  # Targeted
```

## Phase 2 Preview (not in this PR)

When ready for embeddings:
1. Add `sqlite-vec = "0.1.7-alpha.10"` + `zerocopy` deps
2. One `unsafe` block for `sqlite3_auto_extension(sqlite3_vec_init)`
3. Create `vec0` tables with `distance_metric=cosine`
4. Implement `HybridRetriever` with RRF: 70% vector / 30% FTS5
5. Add embedding computation (candle/ort or API-based)
