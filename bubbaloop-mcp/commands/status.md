---
description: Show Bubbaloop daemon and node health summary
---

Use the `bubbaloop` MCP server.

1. Call `system_status` for a top-line view (machine_id, node counts, daemon health).
2. Call `node_list` for the per-node breakdown.

Render the result as a single compact table with columns: `name`, `status`, `health`, `type`, `installed`, `built`.

If any node reports `Failed` or `Unhealthy`:
- Call `node_logs(node_name=...)` for that node only
- Surface the last error line beneath the table

Keep the response under one screen. No narration of intermediate calls.
