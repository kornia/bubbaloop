# Design alignment notes

This document records how `bubbaloop-mcp` aligns with the conventions used by the official **GitHub MCP server** (`github/github-mcp-server`) and the **Figma Dev Mode MCP server**, and what remains misaligned in the underlying bubbaloop MCP server itself (separate from this plugin wrapper).

## Plugin layer — fully aligned

| Convention | GitHub / Figma | This plugin | Source |
|---|---|---|---|
| Manifest at `.claude-plugin/plugin.json` | ✅ | ✅ | [`.claude-plugin/plugin.json`](../.claude-plugin/plugin.json) |
| Marketplace descriptor for self-hosting | ✅ | ✅ | [`.claude-plugin/marketplace.json`](../.claude-plugin/marketplace.json) |
| MCP server declared in `.mcp.json` (separate from plugin manifest) | ✅ | ✅ | [`.mcp.json`](../.mcp.json) |
| Streamable HTTP transport (not deprecated SSE) | ✅ | ✅ | `type: "streamable-http"` |
| Bearer-token auth via `${ENV_VAR}` interpolation in headers | ✅ | ✅ | `Authorization: Bearer ${BUBBALOOP_MCP_TOKEN}` |
| Curated slash commands under a single namespace | ✅ `/github:*` | ✅ `/bubbaloop:*` | [`commands/`](../commands/) |
| Domain subagent invokable via the Agent tool | ✅ | ✅ `bubbaloop-operator` | [`agents/bubbaloop-operator.md`](../agents/bubbaloop-operator.md) |
| Skill that teaches the model how to use the server | ✅ (GitHub ships one for `gh` CLI) | ✅ `using-bubbaloop` | [`skills/using-bubbaloop/SKILL.md`](../skills/using-bubbaloop/SKILL.md) |
| README explains install + auth + transport choice | ✅ | ✅ | [`README.md`](../README.md) |

## Server layer — partially aligned (work for the bubbaloop repo, not this plugin)

These items live in `crates/bubbaloop/src/mcp/` and are out of scope for the plugin wrapper. Track them separately:

| # | Gap | Where to fix |
|---|---|---|
| ~~1~~ | **(CLOSED in feat/mcp-tool-rename) Tool naming inconsistency.** Mixed `verb_noun` (`list_nodes`), `verb_noun_noun` (`get_node_health`), and noun-first (`query_zenoh`). GitHub uses `noun_verb` namespacing (`issues_get`, `repos_search`). | `crates/bubbaloop/src/mcp/tools.rs` — rename via `#[tool(name = "node_list")]` etc. |
| 2 | **No `ToolAnnotations`.** GitHub sets `readOnlyHint`, `destructiveHint`, `idempotentHint`, `openWorldHint` on every tool so clients can auto-distinguish safe from destructive ops. | `crates/bubbaloop/src/mcp/tools.rs` — add annotations to each `#[tool]` declaration |
| 3 | **No toolset grouping / opt-in.** GitHub MCP exposes `--toolsets repos,issues` and a `dynamic` mode that gates tools behind `enable_toolset`. All 42 bubbaloop tools are always visible, which inflates client context. | `crates/bubbaloop/src/mcp/mod.rs` — gate `list_tools` output on a per-toolset env var or token claim |
| 4 | **RBAC tier hardcoded to Admin.** `let caller_tier = rbac::Tier::Admin;` with `TODO(phase-3)`. | `crates/bubbaloop/src/mcp/mod.rs:143` — encode tier in token format and parse in `call_tool` |
| 5 | **Tools-only — no Resources, no Prompts.** Both GitHub and Figma ship resources (file contents, schemas) and prompts (workflow templates). Bubbaloop's node manifests, schemas, and mission YAMLs are natural resources. | `crates/bubbaloop/src/mcp/mod.rs` — extend `ServerCapabilities::builder()` and implement `list_resources` / `list_prompts` |
| 6 | **No pagination.** `list_tools` returns `next_cursor: None` always; `list_jobs` / `list_proposals` are unpaginated. | `crates/bubbaloop/src/mcp/tools.rs` — accept cursor params, return `next_cursor` |
| 7 | **No `title` field on tools.** GitHub uses both `name` (machine) and `title` (human). | `crates/bubbaloop/src/mcp/tools.rs` |
| 8 | **No OAuth 2.1 flow on hosted MCP.** Only Bearer token from disk. Fine for single-user localhost, blocks "remote bubbaloop". | `crates/bubbaloop/src/mcp/mod.rs` + new module `mcp/oauth.rs` |

## Why we didn't ship the server changes in this plugin

Items 1–8 are **breaking changes** to the existing MCP API surface and need a coordinated decision (especially the naming convention in #1, which renames ~42 tools). The plugin wrapper is purely additive — it doesn't depend on the renames — so it ships now, and the server cleanup happens in a follow-up PR.

## Recommended order for closing server gaps

1. Pick & enforce a tool-naming convention (#1) — biggest discoverability win, blocks compounding cost as external integrations land.
2. Add `ToolAnnotations` (#2) — safety + auto-approval ergonomics, near-zero risk.
3. Implement token → tier mapping (#4) — removes `TODO(phase-3)`, unblocks real multi-user.
4. Toolset grouping (#3) — bigger refactor; do once #1 and #2 are stable.
5. Resources + prompts (#5) — high ergonomic value but additive; defer until the rest stabilises.
6. Pagination (#6), title field (#7) — polish.
7. OAuth (#8) — only when there's a hosted bubbaloop deployment.

## Sources

- [Claude Code plugins documentation](https://code.claude.com/docs/en/plugins.md)
- [Claude Code plugin reference](https://code.claude.com/docs/en/plugins-reference.md)
- [Anthropic official plugin marketplace](https://github.com/anthropics/claude-plugins-official)
- [GitHub MCP server (reference implementation)](https://github.com/github/github-mcp-server)
- [MCP specification — Streamable HTTP transport (March 2026)](https://modelcontextprotocol.io/specification/2025-03-26/basic/transports)
- [MCP server concepts](https://modelcontextprotocol.io/docs/learn/server-concepts)

## Comparable projects

A May 2026 scan of the MCP ecosystem turned up no project with bubbaloop's specific combination (Zenoh-backed multi-agent runtime + per-node manifest contracts + sensor/actuator lifecycle + mission/belief state, all exposed via MCP). The five closest cousins, ranked by structural similarity:

| Project | What it does | What we borrowed |
|---|---|---|
| [`Ranch-Hand-Robotics/rde-mcp-ros-2`](https://github.com/ranch-hand-robotics/rde-mcp-ros-2) | ROS 2 CLI as ~30 MCP tools (node lifecycle, topics, params, lifecycle states, bag record/playback) | Closest in scope. Confirms the design space. |
| [`agentic-community/mcp-gateway-registry`](https://github.com/agentic-community/mcp-gateway-registry) | Gateway aggregating multiple MCP servers + semantic tool `discover` API | Validates our per-node manifest pattern; future direction for `tool_discover("how do I start a recording?")` |
| [`poly-mcp/IoT-Edge-MCP-Server`](https://github.com/poly-mcp/IoT-Edge-MCP-Server) | MQTT/Modbus IoT edge: sensor read, actuator write, alarm lifecycle | Alarm-acknowledge workflow worth borrowing for our reactive alerts |
| [`containers/kubernetes-mcp-server`](https://github.com/containers/kubernetes-mcp-server) (Red Hat) | K8s control via MCP with `--read-only` / `--disable-destructive` flag pattern | **Adopted in this PR** as `BUBBALOOP_MCP_MODE` env var (closes part of gap #4 with a fast win) |
| [`sbroenne/mcp-server-obs`](https://github.com/sbroenne/mcp-server-obs) | OBS Studio recording/streaming control: `obs_recording.Start`, `Stop`, `Pause`, `Resume`, `GetStatus`, `SetFormat`, `SetQuality`, `SetPath` | Direct template for the in-flight bubbaloop-nodes recorder. Use the same command vocabulary in the recorder's manifest. |

### `BUBBALOOP_MCP_MODE` — the Red Hat-pattern fast win

Set the env var to cap which tools the MCP server will accept, regardless of token tier:

| Mode | Effect | Use case |
|---|---|---|
| `readonly` | Only Viewer-tier tools accepted (lists, status, manifests) | Inspection-only deployments, demos, restricted user accounts |
| `safe` | Viewer + Operator tools accepted (start/stop/restart/pause OK; install/remove/build blocked) | Day-2 operations without surface for accidental destruction |
| `admin` (default) | All 44 tools accepted | Single-user localhost dev |

Defense-in-depth: the ceiling is enforced *in addition to* per-tool RBAC. A future per-token tier (see gap #4) would be ANDed with this ceiling, so a mode of `safe` still rejects destructive ops even if a token claims Admin tier.

### Negative finding

LangGraph, AutoGen, and CrewAI **do not expose their own runtime as MCP servers** — they consume MCP, they do not publish it. bubbaloop's `mission_list` / `task_schedule` / `belief_get` MCP surface is genuinely novel and worth highlighting in the eventual blog post.
