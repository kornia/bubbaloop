# Node Management

Manage bubbaloop nodes - the processes that make up your Physical AI system. Nodes handle cameras, sensors, inference, monitoring, and more.

You can both manage existing nodes AND create new ones from scratch.

## Operations Tools

### list_nodes
List all registered nodes with their current status (running, stopped, failed, etc.).
- No parameters required.
- Returns: Table of nodes with name, status, health, version, description.

### start_node
Start a stopped node.
- `name` (string): Node name to start.

### stop_node
Stop a running node.
- `name` (string): Node name to stop.

### restart_node
Restart a node (stop then start).
- `name` (string): Node name to restart.

### build_node
Build/rebuild a node (install dependencies, compile protos).
- `name` (string): Node name to build.

### get_logs
Get recent logs from a node.
- `name` (string): Node name to get logs for.

## Creation Tools

### create_node
Scaffold a brand new Python node with all required files.
- `name` (string): Node name (e.g., "temperature-logger").
- `description` (string): What the node does.
- `node_type` (string, optional): "python" (default, only option for now).
- Creates: node.yaml, main.py, config.yaml, pixi.toml under `~/.bubbaloop/nodes/{name}/`

### write_node_file
Write or update any file in a node directory.
- `node_name` (string): Which node to modify.
- `file_path` (string): Relative path (e.g., "main.py", "src/logic.py").
- `content` (string): Full file content.
- Safety: only writes under node directories, blocks path traversal, max 50KB per file.

### read_node_file
Read a file from a node directory.
- `node_name` (string): Which node.
- `file_path` (string): Relative path to read.

### list_node_files
List all files in a node directory with sizes.
- `node_name` (string): Which node.

## Lifecycle Tools

### add_node
Register a node with the daemon (makes it manageable).
- `node_name` (string): Node to register. Must have node.yaml.

### install_node
Install a node as a systemd service.
- `node_name` (string): Node to install.

### uninstall_node
Remove a node's systemd service (keeps files).
- `node_name` (string): Node to uninstall.

### remove_node
Unregister a node from the daemon (keeps files).
- `node_name` (string): Node to remove.

## Creating a Node: Full Workflow

When a user asks you to create a node, follow this workflow:

1. **Understand the requirement** - What should the node do? What data does it consume/produce?
2. **create_node** - Scaffold with a good name and description
3. **write_node_file** - Customize main.py with the actual logic
4. **write_node_file** - Update config.yaml with proper topics and settings
5. **add_node** - Register with the daemon
6. **build_node** - Install dependencies
7. **install_node** - Create systemd service
8. **start_node** - Start it running
9. **get_logs** - Verify it's working

### Node Architecture

Every Python node follows this pattern:
- Connects to Zenoh pub/sub network
- Publishes data to a scoped topic: `bubbaloop/{scope}/{machine}/{suffix}`
- Publishes health heartbeats every few seconds
- Reads configuration from config.yaml
- Handles SIGINT/SIGTERM for graceful shutdown

### Adding Protobuf Support

If the node needs structured binary messages:
1. Create `protos/` directory with `.proto` files
2. Always include `header.proto` (copy from existing nodes)
3. Add `build_proto.py` script
4. Add `grpcio-tools` and `protobuf` to pixi.toml dependencies
5. Update node.yaml with `build: "pixi run build"`

### Safety Boundaries

- **Protected nodes** cannot be stopped, removed, or uninstalled (e.g., bubbaloop-agent)
- **File writes** are restricted to `~/.bubbaloop/nodes/` and `/tmp/bubbaloop/nodes/`
- **Node names** must be alphanumeric with hyphens/underscores only
- **No path traversal** - files must stay within the node directory
- **File size limit** - 50KB per file to prevent accidental large writes
- **Allowed extensions** - only code/config files (.py, .yaml, .toml, .proto, .md, etc.)
