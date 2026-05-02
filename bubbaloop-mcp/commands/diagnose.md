---
description: Diagnose a node — health, manifest, logs, recommended action
argument-hint: <node-name>
---

Diagnose node `$ARGUMENTS` using the `bubbaloop` MCP server:

1. `get_node_health(node_name="$ARGUMENTS")` — current state, uptime, last heartbeat
2. `get_node_manifest(node_name="$ARGUMENTS")` — declared capabilities, topics, commands
3. `get_node_logs(node_name="$ARGUMENTS")` — last 50 lines

Decision rules:

- **Status not Running** → cite the failure cause from logs and suggest `start_node` (or `build_node` if `is_built=false`).
- **Running but Degraded/Unhealthy** → call `get_stream_info` and check whether the data plane is firing; cross-reference recent log lines.
- **Running and Healthy** → confirm and stop; no action needed.

End with a one-line recommendation labelled `→ action:`.
