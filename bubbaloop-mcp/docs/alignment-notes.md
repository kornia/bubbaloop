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
| 1 | **Tool naming inconsistency.** Mixed `verb_noun` (`list_nodes`), `verb_noun_noun` (`get_node_health`), and noun-first (`query_zenoh`). GitHub uses `noun_verb` namespacing (`issues_get`, `repos_search`). | `crates/bubbaloop/src/mcp/tools.rs` — rename via `#[tool(name = "node_list")]` etc. |
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
