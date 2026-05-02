---
description: List all Bubbaloop nodes with key health signals
---

Call `list_nodes` on the `bubbaloop` MCP server.

Render as a table: `name`, `status`, `health`, `type`, `installed`, `built`.

Below the table, group nodes that need attention:

- **Needs setup:** `installed=false` or `is_built=false`
- **Unhealthy:** `health` is `Degraded` or `Unhealthy`
- **Stopped (was running):** `status=Stopped` with a non-empty `last_heartbeat` recent enough to suggest unexpected exit

If the daemon is unreachable, suggest `bubbaloop up` and stop.
