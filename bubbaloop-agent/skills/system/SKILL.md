# System

System diagnostics, health checks, and overall world state information.

## Tools

### system_health
Get overall system health: daemon status, node health summary, resource usage.
- No parameters required.
- Returns: Health summary with daemon status, node counts, and any issues.

### get_world_state
Get a comprehensive view of the current system state: all nodes, their status, active watchers, active captures.
- No parameters required.
- Returns: Full world state as structured text.

### get_machine_info
Get information about the machine: hostname, OS, architecture, CPU, memory, disk, GPU.
- No parameters required.
- Returns: Machine information summary.
