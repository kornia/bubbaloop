---
description: List active missions with their constraints and alerts
---

Call `list_missions` on the `bubbaloop` MCP server.

Render as a table: `mission_id`, `name`, `status`, `agent_id`.

For each mission with `status=running` or `status=paused`, expand inline:
- Constraints from `list_constraints(mission_id=...)`
- Alerts from `list_alerts(mission_id=...)` (note any alert flagged with a dangling world-state ref)

Keep the output dense — one line per constraint, one line per alert. No prose summary unless the user asks.
