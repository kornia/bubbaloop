---
name: using-bubbaloop
description: Use when interacting with a Bubbaloop deployment via its MCP server — guides the discovery → control → automation workflow and prevents the most common mistakes (MCP polling for streaming data, missing Bearer token, ignoring RBAC tier).
---

# Using bubbaloop

Bubbaloop exposes 42 MCP tools across three RBAC tiers (Viewer / Operator / Admin) for managing sensor nodes on a Zenoh-based pub/sub mesh.

## The two-plane model — the rule that matters most

- **MCP** is the **control plane**: low-rate request/response (rate limit: 100 burst, 1 req/sec sustained). Use it for status, lifecycle, config, scheduling.
- **Zenoh** is the **data plane**: high-rate streaming (1000s msg/sec). Use `get_stream_info` to obtain the topic, then subscribe externally with a Zenoh client (`zenoh-python` or the `zenoh` Rust crate).

**Polling sensor data through repeated MCP calls is the most common mistake.** Don't do it. If you find yourself looping `send_command` or `query_zenoh` to read values, stop and call `get_stream_info`.

## Discovery sequence

For any new node interaction, follow this order:

```
list_nodes → get_node_health → get_node_manifest → list_commands
```

Then act with `send_command` or lifecycle tools. This sequence is cheap, side-effect-free, and prevents acting on stale or hallucinated names.

## RBAC tiers

| Tier | Use for | Examples |
|---|---|---|
| **Viewer** | Read-only inspection | `list_nodes`, `get_node_health`, `get_stream_info`, `list_jobs` |
| **Operator** | Day-to-day control | `start_node`, `stop_node`, `restart_node`, `send_command`, `schedule_task` |
| **Admin** | System modification | `install_node`, `remove_node`, `build_node`, `query_zenoh`, `clear_episodic_memory` |

In single-user localhost mode all authenticated callers receive Admin tier, but you should still treat Admin actions as requiring explicit user confirmation.

## Authentication

The HTTP transport at `http://127.0.0.1:8088/mcp` requires `Authorization: Bearer <token>`. The token lives in `~/.bubbaloop/mcp-token`.

The plugin's `.mcp.json` reads it from the `BUBBALOOP_MCP_TOKEN` environment variable. Set it once:

```bash
export BUBBALOOP_MCP_TOKEN="$(cat ~/.bubbaloop/mcp-token)"
```

If you see `401 Unauthorized`, the env var is unset, stale, or the daemon has rotated the token.

The stdio transport (`bubbaloop mcp --stdio`) needs no auth — process boundary provides implicit trust per the MCP spec.

## Automation

For recurring monitoring (e.g. "every 15 minutes, check all sensor health"), use `schedule_task` with a cron expression and `recurrence=true`. The job runs **inside the daemon** — it doesn't consume MCP rate limit. Track via `list_jobs`.

```
schedule_task(
  prompt="Run health patrol on all sensors and alert on anomalies",
  cron_schedule="*/15 * * * *",
  recurrence=true
)
```

## When NOT to use this plugin

- Streaming raw frames or telemetry → use Zenoh directly.
- Sub-100ms control loops → use Zenoh or write a node.
- Developing a new node → see `docs/plugin-development.md` in the bubbaloop repo, not this plugin.

## Errors you will see and what they mean

| Error text | Meaning | Fix |
|---|---|---|
| `401 Unauthorized` | Bearer token missing or wrong | Re-export `BUBBALOOP_MCP_TOKEN` |
| `Permission denied: tool '…' requires admin tier` | Token has insufficient tier | Use a higher-tier token or run on the daemon host |
| `Error: Node not found: <name>` | Stale name; node was removed or never installed | Re-run `list_nodes` |
| `Error: No response from node (is it running?)` | Node process is down | `get_node_health` then `start_node` |
| `Validation error: …` | Bad parameter shape | Check the tool's JSON schema via `list_tools` |
| `Rate limit exceeded` | You're polling | Switch to Zenoh subscription |
