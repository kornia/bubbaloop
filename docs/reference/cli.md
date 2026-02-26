# CLI Commands

Reference for the Bubbaloop command-line interface.

## bubbaloop Binary

The `bubbaloop` binary is a single 7MB Rust executable that includes CLI, TUI, and daemon.

### Core Commands

| Command | Description |
|---------|-------------|
| `bubbaloop` | Show help |
| `bubbaloop tui` | Launch interactive TUI |
| `bubbaloop status` | Show service and node status |
| `bubbaloop doctor` | Run system diagnostics |
| `bubbaloop daemon` | Run the daemon (node manager) |

### Node Commands

```bash
bubbaloop node <subcommand>
```

| Subcommand | Description |
|------------|-------------|
| `init <name>` | Create a new node from template |
| `validate [path]` | Validate node.yaml manifest |
| `list` | List all registered nodes |
| `add <source>` | Add node from path, GitHub URL, or shorthand |
| `instance <base> <suffix>` | Create instance of multi-instance node |
| `remove <name>` | Unregister node from daemon |
| `build <name>` | Build the node |
| `clean <name>` | Clean build artifacts |
| `install <name>` | Install as systemd service |
| `uninstall <name>` | Remove systemd service |
| `start <name>` | Start node service |
| `stop <name>` | Stop node service |
| `restart <name>` | Restart node service |
| `logs <name>` | View node logs |
| `enable <name>` | Enable autostart |
| `disable <name>` | Disable autostart |
| `search <query>` | Search marketplace |
| `discover` | Discover nodes on network |

### Marketplace Commands

```bash
bubbaloop marketplace <subcommand>
```

| Subcommand | Description |
|------------|-------------|
| `list` | List node registry sources |
| `add <name> <path>` | Add a source (GitHub repo path) |
| `remove <name>` | Remove a source |
| `enable <name>` | Enable a source |
| `disable <name>` | Disable a source |

**Note**: Marketplace manages *sources* (registries), not nodes. Use `node` commands for node management.

### Debug Commands

```bash
bubbaloop debug <subcommand>
```

| Subcommand | Description |
|------------|-------------|
| `info` | Show Zenoh connection info |
| `topics` | List active Zenoh topics |
| `subscribe <key>` | Subscribe to Zenoh topic |
| `query <key>` | Query Zenoh endpoint |

---

## Command Details

### bubbaloop status

Show current system and node status.

```bash
bubbaloop status [OPTIONS]
```

| Option | Description |
|--------|-------------|
| `-f, --format <format>` | Output format: `table` (default), `json`, `yaml` |

**Examples:**
```bash
bubbaloop status           # Table output
bubbaloop status -f json   # JSON output for scripting
```

### bubbaloop doctor

Run system diagnostics and optionally auto-fix issues.

```bash
bubbaloop doctor [OPTIONS]
```

| Option | Description |
|--------|-------------|
| `-c, --check <check>` | Run specific check only: `zenoh`, `daemon`, `services`, `config` |
| `--json` | Output as JSON |
| `--fix` | Auto-fix common issues |

**Examples:**
```bash
bubbaloop doctor           # Run all checks
bubbaloop doctor -c zenoh  # Check Zenoh only
bubbaloop doctor --fix     # Auto-fix issues
bubbaloop doctor --json    # JSON for parsing
```

**Auto-Fix Actions:**
- Start zenohd if not running
- Start/restart daemon service
- Start bridge service
- Create missing zenoh config
- Create missing sources.json

### bubbaloop daemon

Run the node manager daemon.

```bash
bubbaloop daemon [OPTIONS]
```

| Option | Description |
|--------|-------------|
| `-z, --zenoh <endpoint>` | Zenoh endpoint (default: tcp/127.0.0.1:7447) |
| `--strict` | Exit if another daemon is running |

**Examples:**
```bash
bubbaloop daemon                        # Default
bubbaloop daemon -z tcp/192.168.1.50:7447  # Remote Zenoh
bubbaloop daemon --strict               # Fail if duplicate
```

### bubbaloop node init

Create a new node from template.

```bash
bubbaloop node init <name> [OPTIONS]
```

| Option | Description |
|--------|-------------|
| `-t, --node-type <type>` | Node type: `rust` (default), `python` |
| `-o, --output <path>` | Output directory (default: ./<name>) |
| `-d, --description <desc>` | Node description |
| `--author <name>` | Author name |

**Examples:**
```bash
bubbaloop node init my-sensor                       # Rust node
bubbaloop node init my-sensor --node-type python    # Python node
bubbaloop node init my-sensor -o /path/to/output    # Custom location
```

### bubbaloop node add

Register a node with the daemon.

```bash
bubbaloop node add <source> [OPTIONS]
```

| Option | Description |
|--------|-------------|
| `-o, --output <path>` | Target directory for Git clones |
| `-b, --branch <branch>` | Git branch (default: main) |
| `-s, --subdir <path>` | Subdirectory containing node.yaml |
| `-n, --name <name>` | Instance name override |
| `-c, --config <path>` | Config file path |
| `--build` | Build after adding |
| `--install` | Install as service after adding |

**Source Formats:**
- Local path: `/path/to/node` or `.`
- GitHub URL: `https://github.com/user/repo`
- GitHub shorthand: `user/repo`

**Examples:**
```bash
bubbaloop node add .                                # Current directory
bubbaloop node add /path/to/my-node                 # Local path
bubbaloop node add user/awesome-node                # GitHub shorthand
bubbaloop node add user/repo --subdir nodes/camera  # Subdirectory
bubbaloop node add user/repo --build --install      # Full setup
```

### bubbaloop node instance

Create an instance of a multi-instance node.

```bash
bubbaloop node instance <base> <suffix> [OPTIONS]
```

| Option | Description |
|--------|-------------|
| `-c, --config <path>` | Config file for this instance |
| `--copy-config` | Copy example config from base node |
| `--install` | Install as systemd service |
| `--start` | Start after creating (implies --install) |

**Examples:**
```bash
# Create camera instance with custom config
bubbaloop node instance rtsp-camera terrace --config ~/.bubbaloop/configs/terrace.yaml

# Copy example config from base node
bubbaloop node instance rtsp-camera garden --copy-config

# Full setup: create, install, and start
bubbaloop node instance rtsp-camera entrance --config config.yaml --start
```

### bubbaloop node list

List all registered nodes.

```bash
bubbaloop node list [OPTIONS]
```

| Option | Description |
|--------|-------------|
| `-f, --format <format>` | Output format: `table` (default), `json` |
| `--base` | Show only base nodes (no instances) |
| `--instances` | Show only instances |

**Examples:**
```bash
bubbaloop node list              # All nodes
bubbaloop node list --base       # Base nodes only
bubbaloop node list --instances  # Instances only
bubbaloop node list -f json      # JSON output
```

### bubbaloop node logs

View node logs.

```bash
bubbaloop node logs <name> [OPTIONS]
```

| Option | Description |
|--------|-------------|
| `-f, --follow` | Follow logs in real-time |
| `-n, --lines <n>` | Number of lines to show (default: 50) |

**Examples:**
```bash
bubbaloop node logs my-node       # Last 50 lines
bubbaloop node logs my-node -f    # Follow logs
bubbaloop node logs my-node -n 100  # Last 100 lines
```

---

## Pixi Tasks

For development, use pixi tasks:

```bash
# Build
pixi run build               # cargo build --release

# Run Services
pixi run daemon              # bubbaloop daemon
pixi run tui                 # bubbaloop tui
pixi run dashboard           # React dashboard dev server

# Development
pixi run check               # cargo check
pixi run test                # cargo test
pixi run fmt                 # cargo fmt --all
pixi run clippy              # cargo clippy (enforced warnings)
pixi run lint                # fmt-check + clippy

# Documentation
pixi run docs                # mkdocs serve
pixi run docs-build          # Build static docs

# Orchestration
pixi run up                  # Start all services via process-compose
```

---

## Environment Variables

| Variable | Description | Default |
|----------|-------------|---------|
| `BUBBALOOP_ZENOH_ENDPOINT` | Zenoh router endpoint | `tcp/127.0.0.1:7447` |
| `BUBBALOOP_MACHINE_ID` | Machine identifier | hostname |
| `RUST_LOG` | Log level | `info` |

**Examples:**
```bash
# Remote Zenoh
export BUBBALOOP_ZENOH_ENDPOINT=tcp/192.168.1.50:7447

# Debug logging
RUST_LOG=debug bubbaloop status

# Trace for specific module
RUST_LOG=bubbaloop::daemon=trace bubbaloop daemon
```

---

## JSON Output

All commands support JSON output for scripting and LLM integration:

```bash
# System status
bubbaloop status -f json

# Diagnostics
bubbaloop doctor --json

# Node list
bubbaloop node list -f json
```

**Parse with jq:**
```bash
# Check if system is healthy
bubbaloop doctor --json | jq '.summary.failed == 0'

# Get running nodes
bubbaloop node list -f json | jq '.[] | select(.status == "running") | .name'

# Get failed checks
bubbaloop doctor --json | jq '.checks[] | select(.passed == false)'
```

---

## Common Workflows

### Fresh Install Verification

```bash
bubbaloop doctor --fix       # Auto-fix any issues
bubbaloop status             # Verify services running
```

### Add and Run a Node

```bash
bubbaloop node add user/my-node --build --install
bubbaloop node start my-node
bubbaloop node logs my-node -f
```

### Create Multi-Instance Setup

```bash
bubbaloop node add ~/.bubbaloop/nodes/rtsp-camera
bubbaloop node instance rtsp-camera cam1 --config cam1.yaml --start
bubbaloop node instance rtsp-camera cam2 --config cam2.yaml --start
bubbaloop node list --instances
```

### Debug Connection Issues

```bash
bubbaloop doctor -c zenoh    # Check Zenoh specifically
bubbaloop debug info         # Show Zenoh connection info
bubbaloop debug topics       # List active topics
```

---

## See Also

- [Troubleshooting](troubleshooting.md) — Common issues and solutions
- [Configuration](../getting-started/configuration.md) — Config file reference
- [Skillet Development](../skillet-development.md) — Node development and publishing guide
