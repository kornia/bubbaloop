# Heartbeat Agent Loop Design

**Date:** 2026-03-02
**Status:** Approved
**Scope:** Semi-autonomous agent loop with heartbeat, human-in-the-loop approval, and memory integration

## Motivation

Bubbaloop's agent today is 100% reactive — it only acts when a user types in the REPL. The Tier 1 scheduler runs deterministic cron actions (health checks, restarts) but can't reason about ambiguous situations.

OpenClaw and ZeroClaw both implement a **heartbeat pattern** where the agent wakes periodically, checks state, reasons with an LLM, and acts. This is the missing piece that makes bubbaloop a true Hardware AI agent.

## Design

### Heartbeat Loop

```
Every N minutes (default: 5):
  1. Collect current state:
     - Node health (all nodes via list_nodes)
     - Recent events since last heartbeat
     - Active schedules
  2. Search memory for relevant context (FTS5)
  3. Send to Claude: "Here's what changed. What should I do?"
  4. Claude reasons → proposes or executes actions
  5. Log decisions + actions to memory
  6. Sleep until next heartbeat

User input (REPL) interrupts sleep and gets priority.
```

### Human-in-the-Loop Approval

Two modes, configurable per-skill:

| Mode | Behavior | Use for |
|------|----------|---------|
| `auto` | Execute immediately, log decision | Health checks, restarting crashed nodes |
| `propose` | Save proposal, wait for approval | Ambiguous situations, destructive actions |

**Proposal flow:**
1. Claude reasons and proposes an action
2. Proposal saved to `proposals` table in SQLite
3. Notification sent (terminal if REPL active, MCP if external agent)
4. User/agent approves → action executes
5. User/agent rejects → action discarded, logged
6. Timeout → auto-execute or discard (configurable)

### Skill YAML Config

```yaml
# ~/.bubbaloop/skills/watchdog.yaml
name: watchdog
heartbeat: true            # opt-in to heartbeat evaluation
approval: propose          # "auto" | "propose"
approval_timeout: 300      # seconds before timeout action (0 = wait forever)
```

### Cost Model

- 5-min heartbeat = 288 turns/day
- ~90% are "nothing changed" (short response, ~500 tokens)
- Estimated: ~$0.05/day with Haiku for simple turns, ~$0.50/day with Sonnet

Future: model routing (Haiku for "nothing changed", Sonnet for reasoning turns).

## Schema Changes

### New table: `proposals`

```sql
CREATE TABLE IF NOT EXISTS proposals (
    id          TEXT PRIMARY KEY,
    timestamp   TEXT NOT NULL,
    skill       TEXT NOT NULL,
    description TEXT NOT NULL,
    actions     TEXT NOT NULL,       -- JSON array of tool calls
    status      TEXT NOT NULL DEFAULT 'pending',
    decided_by  TEXT,                -- "user" | "mcp" | "timeout"
    decided_at  TEXT
);
CREATE INDEX IF NOT EXISTS idx_proposals_status ON proposals(status);
```

### New MCP Tools

| Tool | RBAC | Description |
|------|------|-------------|
| `list_proposals` | Viewer | List pending action proposals |
| `approve_proposal` | Operator | Approve a pending proposal |
| `reject_proposal` | Operator | Reject a pending proposal |

## Codebase Changes

| File | Change |
|------|--------|
| `agent/mod.rs` | Add heartbeat loop with `tokio::select!` between REPL input and timer |
| `agent/prompt.rs` | Add `build_heartbeat_prompt()` — focused on delta since last check |
| `agent/memory.rs` | Add `proposals` table, CRUD methods, `events_since()` query |
| `mcp/mod.rs` | Add 3 proposal tools (`list_proposals`, `approve_proposal`, `reject_proposal`) |
| `mcp/platform.rs` | Add proposal methods to `PlatformOperations` trait |
| `mcp/rbac.rs` | Register proposal tools at appropriate tiers |
| `skills/mod.rs` | Add `heartbeat`, `approval`, `approval_timeout` fields to `SkillConfig` |

**Estimated scope:** ~200-300 lines of new code. No new dependencies.

## Architecture Diagram

```
┌─────────────────────────────────────────────────────┐
│ Agent Process (bubbaloop agent)                      │
│                                                      │
│  ┌─────────┐    ┌──────────────┐    ┌────────────┐  │
│  │  REPL   │    │  Heartbeat   │    │  Tier 1    │  │
│  │  Input   │    │  Timer (5m)  │    │  Scheduler │  │
│  └────┬────┘    └──────┬───────┘    └─────┬──────┘  │
│       │                │                   │         │
│       └───── tokio::select! ──────────────┘         │
│                    │                                  │
│              ┌─────▼──────┐                          │
│              │ Agent Turn  │                          │
│              │             │                          │
│              │ 1. Collect  │◄── Node Manager          │
│              │ 2. Search   │◄── Memory (FTS5)         │
│              │ 3. Reason   │◄── Claude API             │
│              │ 4. Propose  │──► Proposals table        │
│              │ 5. Execute  │──► Dispatcher (tools)     │
│              │ 6. Log      │──► Memory                 │
│              └─────────────┘                          │
│                                                      │
│  ┌──────────────────────────────────┐                │
│  │ MCP Server (:8088)               │                │
│  │  list_proposals                  │◄── External    │
│  │  approve_proposal                │    agents      │
│  │  reject_proposal                 │                │
│  └──────────────────────────────────┘                │
└─────────────────────────────────────────────────────┘
```

## References

- [OpenClaw Agent Loop](https://docs.openclaw.ai/concepts/agent-loop) — heartbeat scheduler pattern
- [ZeroClaw Architecture](https://zeroclaw.net/) — Rust trait-based autonomous agent
- [NVIDIA ReMEmbR](https://developer.nvidia.com/blog/using-generative-ai-to-enable-robots-to-reason-and-act-with-remembr/) — memory-backed robot reasoning
- [ROS-LLM](https://arxiv.org/abs/2406.19741) — embodied AI with task feedback
- [Agent Memory Survey](https://github.com/Shichun-Liu/Agent-Memory-Paper-List) — comprehensive agent memory research
