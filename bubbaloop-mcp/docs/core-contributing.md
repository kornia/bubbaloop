# Contributing to bubbaloop core

For developers extending the daemon, MCP server, RBAC, agent runtime, or other internals.

## Repo layout

The single workspace at `/home/nvidia/bubbaloop/` contains one crate today: `crates/bubbaloop/`.

| Module | Purpose |
|---|---|
| `src/mcp/` | MCP server — tool definitions, RBAC, transport, mode ceiling |
| `src/agent/` | Internal LLM agent runtime — multi-agent dispatch, soul, memory |
| `src/daemon/` | systemd integration, node manager, gateway |
| `src/cli/` | CLI subcommands (`bubbaloop status`, `node …`, `daemon …`, `mcp`) |
| `src/api/` | HTTP API — separate from MCP, used by the dashboard |

The plugin lives at `bubbaloop-mcp/` (this directory) and is published independently.

## Adding a new MCP tool — the recipe

See the `bubbaloop-core-contributor` subagent for an interactive walkthrough. Quick recipe:

1. **`tools.rs`** — write the `#[tool(...)]` method. **Always** include `name`, `description`, and `annotations(title, read_only_hint, destructive_hint, idempotent_hint, open_world_hint)`.
2. **`rbac.rs`** — add the tool name to the `required_tier` match. Pick the lowest tier that's safe.
3. **`mock_platform.rs`** — add to the appropriate `rbac_all_*_tools_mapped` test array. **Forgetting this is the #1 PR review comment.**
4. **`mod.rs`** — update the `instructions` block in `get_info()`. Group with the related tool family.
5. **`dispatch.rs`** — if internal agents should call this tool, mirror the registration AND the match-arm dispatch.
6. **`tests/integration_mcp.rs`** — happy-path + validation-failure tests.
7. **`bubbaloop-mcp/docs/agent-guide.md`** — if material, document for agents.

## The renaming lesson

The May 2026 noun_verb rename surfaced two foot-guns worth flagging up front:

### #1 — `cargo check` is not enough

`cargo check --all-targets` proves the code compiles, including tests. The rename PR's `cargo check` passed cleanly but the test ARRAYS in `mock_platform.rs` had stale names. Symptom: `cargo test` failed with:

```
assertion `left == right` failed: Tool 'list_nodes' should be Viewer tier
  left: Admin
 right: Viewer
```

The admin-tier array test passed coincidentally (Admin is the catch-all default for unknown names). Always run `cargo test -p bubbaloop` before claiming a tool-name change is complete.

### #2 — Word-boundary replace renames trait methods too

`\\blist_nodes\\b` matches both `"list_nodes"` (string literal we want to rename) AND `.list_nodes(` (method call on the `PlatformOperations` trait that we DON'T want to rename — function names are unchanged, only the MCP-exposed name changes via `#[tool(name = "...")]`).

When refactoring tool names, do the bulk replace then immediately revert any pattern matching `\\.<NEW_NAME>\\(` or `::<NEW_NAME>\\(` — those are method calls, not MCP names.

## Two registries — internal agent vs external MCP

The internal LLM agents (jean-clawd, qwen) have their OWN tool registry in `crates/bubbaloop/src/agent/dispatch.rs`. When you add an MCP tool that internal agents should also call, mirror it there too — `name`, match arm, handler. The dispatcher routes by name string, so a tool that exists in MCP but not in dispatch.rs is invisible to internal agents (and vice versa).

When in doubt, grep for the OLD name across:

```
crates/bubbaloop/src/mcp/
crates/bubbaloop/src/agent/
crates/bubbaloop/tests/
bubbaloop-mcp/
docs/
```

Every hit is potentially load-bearing.

## RBAC tier rules

- **Viewer**: read-only, no observable side effect. `zenoh_query` is `Admin` despite being read-only because it queries arbitrary keyspace — security boundary trumps technical classification.
- **Operator**: reversible state changes. start/stop/restart, pause/resume, schedule a job, send a node command.
- **Admin**: install/remove/build, config rewrites, broad queries, memory clears.

Rule of thumb: *if this runs without my approval, can I undo it in <60 seconds?* Yes → `Operator`. No → `Admin`.

## ToolAnnotations rules

Set `read_only_hint: true` ONLY for tools with no observable side effect — not even an audit-trail entry on the mutation path.

For destructive tools, ALWAYS set `destructive_hint: true` even if also idempotent.

For tools whose effect depends on caller-supplied open-ended input (`node_command_send`, `zenoh_query`, `node_install` from a URL), set `open_world_hint: true` — clients treat those more cautiously.

## Mode ceiling

`BUBBALOOP_MCP_MODE` (readonly / safe / admin) maps to a `Tier` ceiling enforced server-side BEFORE the existing per-tool RBAC check. New tools inherit this for free — no extra wiring needed.

## Live wire smoke test (after every MCP-server change)

The rename PR taught us the running daemon may be stale even after `cargo check` passes. After rebuilding, re-launch the daemon and run:

```bash
TOKEN="$(cat ~/.bubbaloop/mcp-token)"
INIT=$(curl -sS -i -X POST http://127.0.0.1:8088/mcp \
  -H "Authorization: Bearer ${TOKEN}" \
  -H "Content-Type: application/json" \
  -H "Accept: application/json, text/event-stream" \
  -d '{"jsonrpc":"2.0","id":1,"method":"initialize","params":{"protocolVersion":"2024-11-05","capabilities":{},"clientInfo":{"name":"smoke","version":"0"}}}')
SESSION=$(echo "$INIT" | awk -F': ' 'tolower($1)=="mcp-session-id"{print $2}' | tr -d '\r')

curl -sS -X POST http://127.0.0.1:8088/mcp \
  -H "Authorization: Bearer ${TOKEN}" \
  -H "Mcp-Session-Id: ${SESSION}" \
  -H "Content-Type: application/json" \
  -H "Accept: application/json, text/event-stream" \
  -d '{"jsonrpc":"2.0","method":"notifications/initialized"}' > /dev/null

curl -sS --no-buffer --max-time 12 -X POST http://127.0.0.1:8088/mcp \
  -H "Authorization: Bearer ${TOKEN}" \
  -H "Mcp-Session-Id: ${SESSION}" \
  -H "Content-Type: application/json" \
  -H "Accept: application/json, text/event-stream" \
  -d '{"jsonrpc":"2.0","id":3,"method":"tools/list","params":{}}'
```

Look for your new tool in the response with the right `name`, `description`, and `annotations` block. Done.

## Open gaps (= the roadmap)

`bubbaloop-mcp/docs/alignment-notes.md` tracks open gaps. As of May 2026 the highest-impact open ones:

| Gap | Impact | Cost | One-liner |
|---|---|---|---|
| **#4** per-token tier mapping | High | Medium | encode tier in token format; finishes the RBAC story |
| **#12** dynamic toolset disclosure | High | Medium | borrow GitHub MCP's `enable_toolset` meta-tool — biggest UX win for small models |
| **#13** structured error bodies | High | Low | `{"code","message","retry"}` instead of plain strings |
| **#14** descriptive JSON outputs | Medium | Low | semantic > prescriptive (Framelink lesson) |
| **#11** systemd-install MCP tool | Medium | Low | new MCP tool wrapping `bubbaloop node install` |
| **#5** Resources + Prompts | Medium | Medium | manifests/schemas/missions are natural resources |
| **#3** toolset grouping | High | Medium | requires #12 first |
| **#6** pagination | Low | Low | `next_cursor: None` everywhere today |
| **#8** OAuth 2.1 PKCE | Medium | High | only when hosted bubbaloop ships |

Pick one that matches your interest and time budget.

## Where to ask

- **In-conversation**: dispatch the `bubbaloop-core-contributor` subagent with a specific question.
- **Async**: open a discussion thread in the bubbaloop repo. Issue/PR conventions are the standard ones (`feat:`, `fix:`, `chore:`, `docs:`, `test:`).
