# Node Management

Manage bubbaloop nodes - the processes that make up your Physical AI system. Nodes handle cameras, sensors, inference, monitoring, and more.

## Tools

### list_nodes
List all registered nodes with their current status (running, stopped, failed, etc.).
- No parameters required.
- Returns: Table of nodes with name, status, health, version, description.

### start_node
Start a stopped node.
- `name` (string): Node name to start.
- Returns: Command result with success/failure.

### stop_node
Stop a running node.
- `name` (string): Node name to stop.
- Returns: Command result with success/failure.

### restart_node
Restart a node (stop then start).
- `name` (string): Node name to restart.
- Returns: Command result with success/failure.

### build_node
Build/rebuild a node (install dependencies, compile).
- `name` (string): Node name to build.
- Returns: Build output with success/failure.

### get_logs
Get recent logs from a node.
- `name` (string): Node name to get logs for.
- Returns: Recent log lines from the node.
