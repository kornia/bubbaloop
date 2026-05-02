# bubbaloop-mcp

A Claude Code plugin that wraps the [Bubbaloop](https://github.com/kornia/bubbaloop) daemon's MCP server — control physical sensor nodes (RTSP cameras, environmental sensors, robotics rigs) from any MCP-compatible client.

Modeled on the official GitHub and Figma MCP integrations: a single Streamable-HTTP MCP server, a curated set of slash commands, a domain subagent, and a model-invokable skill that teaches agents how to use the server correctly.

## What you get

- **42 MCP tools** across three RBAC tiers (Viewer, Operator, Admin), exposed by the running `bubbaloop up` daemon.
- **5 slash commands** — `/bubbaloop:status`, `/bubbaloop:nodes`, `/bubbaloop:diagnose <node>`, `/bubbaloop:mission`, `/bubbaloop:stream <node>`.
- **1 subagent** — `bubbaloop-operator`, dispatchable via the `Agent` tool for hands-off node triage.
- **1 skill** — `using-bubbaloop`, auto-loaded into agent context to prevent the two-plane mistake (don't poll streaming data through MCP).

## Install

### Claude Code (from a registered marketplace)

```bash
# Add the bubbaloop marketplace (one-time)
claude /plugin marketplace add github.com/kornia/bubbaloop

# Install the plugin
claude /plugin install bubbaloop
```

### Claude Code (local development)

```bash
git clone https://github.com/kornia/bubbaloop
claude --plugin-dir ./bubbaloop/bubbaloop-mcp
```

### Other MCP clients (Cursor, Cline, Claude Desktop, Copilot CLI, Zed)

Copy the contents of [`.mcp.json`](.mcp.json) into your client's MCP config. The plugin's commands/agents/skills are Claude-Code-specific, but the underlying MCP server works everywhere.

## Prerequisites

1. **Bubbaloop daemon running locally:**
   ```bash
   bubbaloop up
   ```
   This starts the MCP server at `http://127.0.0.1:8088/mcp`.

2. **Bearer token exported:**
   ```bash
   export BUBBALOOP_MCP_TOKEN="$(cat ~/.bubbaloop/mcp-token)"
   ```
   Add this to your shell profile so the plugin can authenticate.

## Transport choice — HTTP vs stdio

The plugin defaults to **Streamable HTTP** (the modern MCP transport — HTTP+SSE is deprecated as of the March 2026 spec) because that's how the bubbaloop daemon serves its tools in production.

If you need a self-contained, no-daemon mode (e.g. CI, a fresh machine without `bubbaloop up`), swap `.mcp.json` for stdio:

```json
{
  "mcpServers": {
    "bubbaloop": {
      "command": "bubbaloop",
      "args": ["mcp", "--stdio"]
    }
  }
}
```

Note: stdio mode is process-scoped — it cannot reach a running daemon's live state (mission scheduler, world state, live providers). Use HTTP for anything beyond ad-hoc inspection.

## Modes

Cap which tools the MCP server will accept by setting `BUBBALOOP_MCP_MODE` before starting the daemon (or stdio process):

```bash
export BUBBALOOP_MCP_MODE=readonly  # only Viewer tools (status, lists, manifests)
export BUBBALOOP_MCP_MODE=safe      # add Operator tools (start/stop, missions, beliefs)
export BUBBALOOP_MCP_MODE=admin     # all 44 tools (default)
```

The ceiling is enforced server-side, independent of token tier — useful for read-only demos, restricted user sessions, or defense-in-depth alongside the future per-token tier mapping.

## Slash commands

| Command | Purpose |
|---|---|
| `/bubbaloop:status` | Daemon + node health summary in one table |
| `/bubbaloop:nodes` | Full node list with attention groupings |
| `/bubbaloop:diagnose <node>` | Health → manifest → logs → recommended action |
| `/bubbaloop:mission` | Active missions with constraints and alerts inlined |
| `/bubbaloop:stream <node>` | Zenoh topic + copy-pasteable subscriber snippet |

## Subagent

`bubbaloop-operator` — dispatchable via the Agent tool when the user asks for hands-off node triage. Enforces:

- Discover before acting.
- Read logs on every failure.
- Admin-tier actions require explicit user confirmation.
- Never poll streaming data through MCP.

## Design alignment

This plugin is intentionally modeled on the conventions of the **official GitHub MCP server** and the **Figma Dev Mode MCP server**. See [`docs/alignment-notes.md`](docs/alignment-notes.md) for a side-by-side comparison and the open gaps in the underlying bubbaloop server (tool naming, `ToolAnnotations`, toolset grouping) that should be closed in subsequent PRs.

## License

Apache-2.0 — same as bubbaloop.
