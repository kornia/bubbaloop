# Installing bubbaloop-mcp

Three install paths, ranked by ease. Pick one.

## Path 1 — Marketplace (once published)

```bash
claude /plugin marketplace add github.com/kornia/bubbaloop
claude /plugin install bubbaloop
```

Plugin auto-loads in every new Claude Code session. Slash commands, subagents, and MCP tools all available.

## Path 2 — Local `--plugin-dir` (works today)

```bash
git clone https://github.com/kornia/bubbaloop
export BUBBALOOP_MCP_TOKEN="$(cat ~/.bubbaloop/mcp-token)"
claude --plugin-dir /home/nvidia/bubbaloop/bubbaloop-mcp
```

When CC starts, it'll prompt to **trust the new MCP server config** — that's the security check for project-scoped MCP servers. Approve `bubbaloop`.

## Path 3 — Project-scoped `.mcp.json` (MCP tools only, no slash commands)

For projects where you only want the bubbaloop MCP tools available — not the full plugin:

```bash
cd /path/to/your/project
cat > .mcp.json << 'EOF'
{
  "mcpServers": {
    "bubbaloop": {
      "type": "streamable-http",
      "url": "http://127.0.0.1:8088/mcp",
      "headers": { "Authorization": "Bearer ${BUBBALOOP_MCP_TOKEN}" }
    }
  }
}
EOF
export BUBBALOOP_MCP_TOKEN="$(cat ~/.bubbaloop/mcp-token)"
claude
```

You get `mcp__bubbaloop__*` tools but not the `/bubbaloop:*` slash commands or the `bubbaloop-operator` subagent.

---

## Prerequisites — daemon must be running

The plugin connects to the bubbaloop daemon's MCP server. Without it, every call returns `Connection refused on :8088`.

```bash
# 1. Start the Zenoh router (one-time per boot)
~/.bubbaloop/bin/zenohd > /tmp/zenohd.log 2>&1 &

# 2. Start the daemon
bubbaloop daemon run > /tmp/bubbaloop_daemon.log 2>&1 &

# 3. Wait for the MCP server to bind
until ss -tlnp 2>/dev/null | grep -q :8088; do sleep 1; done
echo "✓ daemon ready on :8088"
```

Verify:

```bash
bubbaloop status
# Zenoh Router  ✓ running
# Daemon        ✓ running
# Bridge        (optional — only for dashboard WebSocket proxy)
```

---

## Choose a mode (defense in depth)

Set `BUBBALOOP_MCP_MODE` BEFORE starting the daemon. Server-enforced ceiling, independent of token tier:

```bash
export BUBBALOOP_MCP_MODE=readonly  # only Viewer tools — public dashboards, demos
export BUBBALOOP_MCP_MODE=safe      # adds Operator tools — start/stop/restart/missions
export BUBBALOOP_MCP_MODE=admin     # all 44 tools (default) — local development
```

A `safe`-mode daemon will reject `node_install` / `node_remove` / `node_build` / `zenoh_query` / `memory_episodic_clear` even if the caller's token claims admin. Pattern lifted from Red Hat's `kubernetes-mcp-server`.

---

## Verify end-to-end (30 seconds)

After install + daemon start, run any of these:

```
/bubbaloop:doctor              # comprehensive: token, daemon, MCP, Zenoh, nodes, mode
/bubbaloop:status              # one-table view of all nodes
@bubbaloop-operator list nodes # exercise the subagent end-to-end
```

If `/bubbaloop:doctor` returns all green, you're done.

---

## Troubleshooting

| Error | Likely cause | Fix |
|---|---|---|
| `401 Unauthorized` | `BUBBALOOP_MCP_TOKEN` env unset or stale | `export BUBBALOOP_MCP_TOKEN="$(cat ~/.bubbaloop/mcp-token)"` |
| `Connection refused on :8088` | Daemon not running | `bubbaloop daemon run &` |
| `Connection refused on :7447` | Zenoh router not running | `~/.bubbaloop/bin/zenohd &` |
| `tool not found: list_nodes` | Old name pre-rename PR | use `node_list` (see naming map in alignment-notes.md) |
| `Mode-restricted: tool ... requires X tier` | `BUBBALOOP_MCP_MODE` cap rejects this tool | bump to `safe` or `admin`, restart daemon |
| Plugin install hang | Marketplace not registered | `/plugin marketplace add` first |
| Plugin loaded but tools missing in session | Plugin loaded post-startup OR `.mcp.json` not in CWD | `/exit` and re-launch CC from a dir with `.mcp.json` (or with `--plugin-dir`) |
| `Already borrowed` (PyO3 error in node logs) | Re-entrant Zenoh callback in a Python node | not a plugin issue — see node author guide |

---

## Next steps — pick by role

| If you are a… | Start here |
|---|---|
| **Operator** running an existing fleet | `/bubbaloop:status`, `/bubbaloop:diagnose <node>`, the `bubbaloop-operator` subagent |
| **Node author** building a new sensor / actuator / processor | [`docs/node-authoring.md`](docs/node-authoring.md), the `bubbaloop-node-author` subagent, `/bubbaloop:node-new <name>` |
| **Core contributor** extending the daemon / MCP layer / RBAC | [`docs/core-contributing.md`](docs/core-contributing.md), the `bubbaloop-core-contributor` subagent |
| **Just integrating from another MCP client** (Cursor, Cline, Claude Desktop) | Path 3 above + the [`.mcp.json`](.mcp.json) shape |
| **Pairing with Ollama** (Path A native or Path B bridge) | [`docs/ollama-usage.md`](docs/ollama-usage.md) |
