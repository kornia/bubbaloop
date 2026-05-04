# bubbaloop-mcp

A Claude Code plugin that wraps the [Bubbaloop](https://github.com/kornia/bubbaloop) daemon's MCP server — control physical sensor nodes (RTSP cameras, environmental sensors, robotics rigs), build new nodes, and contribute to bubbaloop core, all from any MCP-compatible client.

Modeled on the conventions of the official GitHub and Figma MCP integrations: a single Streamable-HTTP MCP server, a curated set of slash commands, three role-specific subagents, and a model-invokable skill that teaches agents how to use the server correctly.

## Get started in 30 seconds

> **Operator** (run / monitor a fleet) → install + `/bubbaloop:status`
> **Node author** (build a sensor / actuator / processor) → [`docs/node-authoring.md`](docs/node-authoring.md) + `/bubbaloop:node-new <name>`
> **Core contributor** (extend daemon / MCP / RBAC) → [`docs/core-contributing.md`](docs/core-contributing.md) + the `bubbaloop-core-contributor` subagent

Full step-by-step in [`INSTALL.md`](INSTALL.md).

## What you get

- **47 MCP tools** across three RBAC tiers (Viewer / Operator / Admin), exposed by the running `bubbaloop daemon run` daemon. Names follow `noun_verb` (`node_list`, `mission_pause`, `belief_update`) and ship with full `ToolAnnotations` (`readOnlyHint`, `destructiveHint`, `idempotentHint`, `openWorldHint`).
- **8 slash commands** under `/bubbaloop:*` covering status, diagnostics, lifecycle, and node authoring.
- **3 subagents** dispatchable via the `Agent` tool — one per audience.
- **1 skill** (`using-bubbaloop`) auto-loaded into agent context to prevent the two-plane mistake (don't poll streaming data through MCP).
- **Mode flag** `BUBBALOOP_MCP_MODE=readonly|safe|admin` for server-side defense in depth.

## Install

See [`INSTALL.md`](INSTALL.md) for the full guide. Quickest path:

```bash
git clone https://github.com/kornia/bubbaloop
export BUBBALOOP_MCP_TOKEN="$(cat ~/.bubbaloop/mcp-token)"
claude --plugin-dir /home/nvidia/bubbaloop/bubbaloop-mcp
```

Then in CC:

```
/bubbaloop:doctor              # verify every layer is green
/bubbaloop:status              # one-table view of the fleet
```

## Slash commands

| Command | Audience | Purpose |
|---|---|---|
| `/bubbaloop:doctor` | All | Diagnose token / daemon / MCP / Zenoh / mode + actionable fixes |
| `/bubbaloop:status` | Operator | Daemon + node health summary in one table |
| `/bubbaloop:nodes` | Operator | Full node list with attention groupings |
| `/bubbaloop:diagnose <node>` | Operator | Health → manifest → logs → recommended action |
| `/bubbaloop:mission` | Operator | Active missions with constraints and alerts inlined |
| `/bubbaloop:stream <node>` | Operator | Zenoh topic + copy-pasteable subscriber snippet |
| `/bubbaloop:node-new <name>` | Node author | Scaffold a new node via the `bubbaloop-node-author` subagent |
| `/bubbaloop:node-validate <path-or-name>` | Node author | Validate manifest + structure against the contract |

## Subagents

Three agents — one per audience — dispatchable via the `Agent` tool:

| Subagent | Audience | When to use |
|---|---|---|
| `bubbaloop-operator` | Operator | Inspect / control / triage running nodes. Enforces discover-before-act, tier discipline, no-MCP-polling. |
| `bubbaloop-node-author` | Node author | Build a new sensor / actuator / processor node. Knows the contract, SDK patterns, wire-format gotchas, validation flow. |
| `bubbaloop-core-contributor` | Core contributor | Extend the daemon / MCP server / RBAC. Knows the crate layout, rmcp 0.15 macro syntax, the renaming gotchas, the alignment-notes roadmap. |

## Skill

`using-bubbaloop` (auto-loaded) — teaches the model the two-plane rule (MCP control / Zenoh data), the discovery sequence, RBAC tiers, auth, and common errors. Single-file: [`skills/using-bubbaloop/SKILL.md`](skills/using-bubbaloop/SKILL.md).

## Modes

Cap which tools the MCP server accepts by setting `BUBBALOOP_MCP_MODE` BEFORE starting the daemon:

```bash
export BUBBALOOP_MCP_MODE=readonly  # only Viewer tools — public dashboards, demos
export BUBBALOOP_MCP_MODE=safe      # adds Operator tools — start/stop/restart, missions
export BUBBALOOP_MCP_MODE=admin     # all 47 tools (default)
```

Server-side enforcement, independent of token tier. Useful for read-only deployments and defense in depth alongside the future per-token tier mapping (gap #4).

## Other MCP clients

The plugin's slash commands / subagents / skill are Claude-Code-specific, but the underlying MCP server works in any client (Cursor, Cline, Claude Desktop, Copilot CLI, Zed). Copy [`.mcp.json`](.mcp.json) into your client's config; export `BUBBALOOP_MCP_TOKEN` before launching.

For Ollama (Path A native via `agents.toml` or Path B via `mcphost`/`Tome`/LangChain): see [`docs/ollama-usage.md`](docs/ollama-usage.md).

## Documentation

| Audience | Doc |
|---|---|
| Everyone | [`INSTALL.md`](INSTALL.md) |
| Operator | The `bubbaloop-operator` subagent prompt + [`skills/using-bubbaloop/SKILL.md`](skills/using-bubbaloop/SKILL.md) |
| Node author | [`docs/node-authoring.md`](docs/node-authoring.md) |
| Core contributor | [`docs/core-contributing.md`](docs/core-contributing.md) |
| Ollama users | [`docs/ollama-usage.md`](docs/ollama-usage.md) |
| Anyone wondering "why this design" | [`docs/alignment-notes.md`](docs/alignment-notes.md) |

## Design alignment

This plugin is intentionally modeled on the official GitHub MCP server and Figma Dev Mode MCP server. See [`docs/alignment-notes.md`](docs/alignment-notes.md) for the full audit, the comparable-projects table (5 closest cousins), and the open gaps in the underlying bubbaloop server tracked as the roadmap.

## License

Apache-2.0 — same as bubbaloop.
