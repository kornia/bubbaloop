---
description: List active missions with their constraints and alerts
---

Call `mission_list` on the `bubbaloop` MCP server.

Render as a table: `mission_id`, `name`, `status`, `agent_id`.

For each mission with `status=running` or `status=paused`, expand inline:
- Constraints from `constraint_list(mission_id=...)`
- Alerts from `alert_list(mission_id=...)` (note any alert flagged with a dangling world-state ref)

Keep the output dense — one line per constraint, one line per alert. No prose summary unless the user asks.
