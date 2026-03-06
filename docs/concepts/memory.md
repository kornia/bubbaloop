# 3-Tier Memory

RAM for now. Logs for yesterday. SQLite for the long game.

---

## Architecture

```
+------------------------------------------+
| Tier 1: Short-term (RAM)                 |
|   Vec<Message> — current turn context    |
|   system prompt + user + tool + assistant|
+------------------+-----------------------+
                   | flush on compaction
                   v
+------------------------------------------+
| Tier 2: Episodic (NDJSON + FTS5)         |
|   daily_logs_YYYY-MM-DD.jsonl            |
|   BM25 search with temporal decay        |
+------------------+-----------------------+
                   | indexed by
                   v
+------------------------------------------+
| Tier 3: Semantic (SQLite)                |
|   jobs table    — scheduled tasks        |
|   proposals table — approval queue       |
|   fts_episodic  — FTS5 search index      |
+------------------------------------------+
```

All three tiers are managed by the `Memory` struct in
`crates/bubbaloop/src/agent/memory/mod.rs`.

---

## Tier 1: Short-term (RAM)

```rust
pub struct Memory {
    pub short_term: Vec<Message>,
    pub backend: Arc<tokio::sync::Mutex<MemoryBackend>>,
}
```

`short_term` holds the active conversation for the current turn. It contains:

- The system prompt (injected at generation time, not stored here)
- User messages
- Assistant responses (text + tool use blocks)
- Tool results

Between turns the agent trims `short_term` to the last 40 messages
(`MAX_CONVERSATION_MESSAGES = 40`). After a job completes,
`clear_short_term()` wipes it entirely.

When context usage approaches the 200k-token window limit
(`compaction_flush_threshold_tokens` from the agent's Soul), a silent
pre-compaction flush turn fires. The model is asked to summarize active
proposals, node health, and unresolved decisions. That summary is written
to episodic and recovered on the next turn. See the Context Compaction
section below.

---

## Tier 2: Episodic (NDJSON + FTS5)

Daily append-only log files. Every user message, assistant response, tool
result, and planning step is written here as a JSON line.

**File location:**
```
~/.bubbaloop/agents/{agent-id}/memory/daily_logs_YYYY-MM-DD.jsonl
```

**Entry format:**
```json
{"timestamp":"2026-03-06T14:22:01Z","role":"assistant","content":"Restarted rtsp-camera node.","job_id":"job-abc123"}
```

Roles: `user`, `assistant`, `tool`, `system`, `plan`.

The `plan` role is special — it fires when the model outputs both text
(reasoning) and tool calls in the same response. That text is the plan.

**Search:**

Each append dual-writes to an FTS5 virtual table (`fts_episodic`) inside
`memory.db`. Queries use BM25 ranking via SQLite's `rank` column.

```sql
SELECT content, role, timestamp FROM fts_episodic
WHERE fts_episodic MATCH 'camera restart'
ORDER BY rank LIMIT 5;
```

**Temporal decay:**

`search_with_decay(query, limit, half_life_days)` applies exponential
decay to demote old matches:

```
effective_rank = rank * e^(-age_days * ln2 / half_life_days)
```

Set `half_life_days = 0` to disable decay (falls back to plain BM25).

**Retention:**

At startup, `prune_old_logs(retention_days)` deletes NDJSON files older
than `retention_days` and removes their FTS5 entries. Set `retention_days
= 0` to keep everything.

**Forget:**

`forget(query, reason)` hard-deletes matching FTS5 entries and writes an
audit trail to `memory/.deleted/forgotten_YYYY-MM-DD_HH-MM-SS.jsonl`.
The NDJSON source files are not touched (append-only principle).

---

## Tier 3: Semantic (SQLite)

```
~/.bubbaloop/agents/{agent-id}/memory.db
```

Permissions: `0600` (owner-read/write only).

WAL mode enabled. Busy timeout: 5000ms.

### Tables

**`jobs`** — scheduled and one-off agent tasks:

| Column           | Type    | Description                                    |
|------------------|---------|------------------------------------------------|
| `id`             | TEXT PK | UUID                                           |
| `cron_schedule`  | TEXT    | Optional cron expression for recurring jobs   |
| `next_run_at`    | INTEGER | Epoch seconds (when to next fire)             |
| `prompt_payload` | TEXT    | Instruction to execute                        |
| `status`         | TEXT    | `pending`, `running`, `completed`, `failed`, `dead_letter` |
| `recurrence`     | BOOLEAN | Recurring if true                             |
| `retry_count`    | INTEGER | Consecutive failure count                     |
| `last_error`     | TEXT    | Most recent error message                     |

Retry logic: exponential backoff (`30s * 2^retry_count`). After
`max_retries` consecutive failures the job is parked as `dead_letter`.

**`proposals`** — human-in-the-loop approval queue:

| Column        | Type    | Description                                    |
|---------------|---------|------------------------------------------------|
| `id`          | TEXT PK | UUID                                           |
| `skill`       | TEXT    | Action category                               |
| `description` | TEXT    | Human-readable summary                        |
| `actions`     | TEXT    | JSON array of tool calls to execute           |
| `status`      | TEXT    | `pending`, `approved`, `rejected`, `expired`  |
| `decided_by`  | TEXT    | `user`, `mcp`, or `timeout`                   |
| `decided_at`  | TEXT    | ISO 8601 timestamp of decision                |

**`fts_episodic`** — FTS5 virtual table (search index for Tier 2).

---

## Per-Agent Isolation

Each agent gets its own memory directory and SQLite database. No shared
state between agents.

```
~/.bubbaloop/agents/
├── jean-clawd/
│   ├── soul/
│   │   ├── identity.md
│   │   └── capabilities.toml
│   ├── memory/
│   │   ├── daily_logs_2026-03-05.jsonl
│   │   ├── daily_logs_2026-03-06.jsonl
│   │   └── .deleted/
│   │       └── forgotten_2026-03-06_09-15-00.jsonl
│   └── memory.db
└── camera-expert/
    ├── soul/
    ├── memory/
    └── memory.db
```

`Memory::open(base)` creates `{base}/memory/` for NDJSON files and
`{base}/memory.db` for the SQLite database. Both are created on first
open if they do not exist.

---

## Context Compaction

When `last_input_tokens > DEFAULT_CONTEXT_WINDOW - flush_threshold`:

1. Agent appends a flush instruction to `short_term` asking the model to
   summarize key state (proposals, node health, unresolved decisions).
2. Model response is checked for substance (`len >= 50`, no "nothing to
   persist" phrases).
3. Substantive response is written to episodic as a `CONTEXT_FLUSH:`
   entry (`role = "system"`, `flush = true`).
4. On the next turn, `latest_flush()` retrieves this entry and injects
   it into the system prompt as recovered context.

This prevents total context loss during long-running conversations.
The flush does not count toward `max_turns`.

---

## Memory Tools

| Tool              | Tier    | Description                                    |
|-------------------|---------|------------------------------------------------|
| `memory_search`   | 2       | BM25 search over episodic logs                |
| `memory_forget`   | 2       | Remove entries from FTS5 (with audit trail)   |
| `schedule_task`   | 3       | Create a scheduled job in SQLite              |
| `create_proposal` | 3       | Submit a proposal for human approval          |

---

## Example

```
> What did we discuss about the camera yesterday?

[jean-clawd]
  [calling memory_search...]
Yesterday we set up the front-door RTSP camera at 192.168.1.100 with
the rtsp-camera node. The stream was healthy after restart at 14:22 UTC.
```

The agent calls `memory_search` with your query, gets BM25 results from
the FTS5 index (with temporal decay applied), and synthesizes the answer.

---

## See Also

- [Architecture](architecture.md) — layer model and node contract
- [Agent Guide](../agent-guide.md) — configuring agents and Soul
