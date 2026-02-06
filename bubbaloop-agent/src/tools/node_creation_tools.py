"""Node creation tools - scaffold, write code, add/install/remove nodes.

Safety controls:
- File writes restricted to allowed node directories
- Node names validated (alphanumeric, hyphens, underscores)
- Path traversal blocked
- File size limits enforced
- Protected nodes cannot be removed
"""

import json
import logging
import os
import re
import socket
from pathlib import Path

from .registry import ToolRegistry, ToolDefinition

logger = logging.getLogger(__name__)

# Valid node name: starts with letter/digit, only contains alphanumeric, hyphens, underscores
NODE_NAME_RE = re.compile(r"^[a-zA-Z0-9][a-zA-Z0-9_-]*$")

# Max file size: 50KB (generous for code files, blocks huge blobs)
MAX_FILE_SIZE = 50 * 1024

# Files allowed to be created/written in a node directory
ALLOWED_EXTENSIONS = {
    ".py", ".yaml", ".yml", ".toml", ".proto", ".md", ".txt",
    ".json", ".cfg", ".ini", ".sh", ".rs", ".toml",
}

# Template for a new Python node
PYTHON_NODE_TEMPLATE = {
    "node.yaml": """\
name: {name}
version: "0.1.0"
type: python
description: "{description}"
author: bubbaloop-agent
command: "pixi run run"
""",
    "pixi.toml": """\
[workspace]
name = "{name}"
version = "0.1.0"
channels = ["conda-forge"]
platforms = ["linux-64", "linux-aarch64"]

[tasks]
run = "python main.py -c config.yaml"

[dependencies]
python = ">=3.10"
pyyaml = ">=6.0"

[pypi-dependencies]
eclipse-zenoh = ">=1.7.0"
""",
    "config.yaml": """\
# Topic to publish output to (suffix only, scoped at runtime)
publish_topic: "{name}/output"

# Topic to subscribe to (optional, null to disable)
subscribe_topic: null

# Publishing rate in Hz
rate_hz: 1.0
""",
    "main.py": """\
#!/usr/bin/env python3
\"\"\"{name} node - {description}\"\"\"

import argparse
import json
import logging
import os
import signal
import socket
import sys
import time
from datetime import datetime, timezone
from pathlib import Path

import yaml
import zenoh

logging.basicConfig(
    level=logging.INFO,
    format="%(asctime)s [%(levelname)s] %(message)s",
    datefmt="%Y-%m-%d %H:%M:%S",
)
logger = logging.getLogger(__name__)


class Node:
    def __init__(self, config_path: Path, endpoint: str | None = None):
        if config_path.exists():
            with open(config_path) as f:
                self.config = yaml.safe_load(f) or {{}}
        else:
            self.config = {{"publish_topic": "{name}/output", "rate_hz": 1.0}}

        # Zenoh setup
        zenoh_config = zenoh.Config()
        ep = endpoint or "tcp/127.0.0.1:7447"
        zenoh_config.insert_json5("connect/endpoints", json.dumps([ep]))
        zenoh_config.insert_json5("scouting/multicast/enabled", "false")
        zenoh_config.insert_json5("scouting/gossip/enabled", "false")

        self.session = zenoh.open(zenoh_config)

        # Build scoped topic
        scope = os.environ.get("BUBBALOOP_SCOPE", "local")
        machine = os.environ.get("BUBBALOOP_MACHINE_ID", socket.gethostname())
        suffix = self.config.get("publish_topic", "{name}/output")
        self.full_topic = f"bubbaloop/{{scope}}/{{machine}}/{{suffix}}"
        self.health_topic = f"bubbaloop/{{scope}}/{{machine}}/health/{name}"

        self.publisher = self.session.declare_publisher(self.full_topic)
        logger.info(f"Publishing to: {{self.full_topic}}")

        self.running = True
        self.sequence = 0

    def process(self) -> bytes:
        \"\"\"Generate output data. Override this with your logic.\"\"\"
        output = {{
            "sequence": self.sequence,
            "timestamp": datetime.now(timezone.utc).isoformat(),
            "data": "Hello from {name}",
        }}
        return json.dumps(output).encode()

    def run(self):
        interval = 1.0 / self.config.get("rate_hz", 1.0)
        logger.info("{name} node started (rate: %.1f Hz)", self.config.get("rate_hz", 1.0))

        while self.running:
            output = self.process()
            self.publisher.put(output)

            # Health heartbeat
            self.session.put(self.health_topic, "{name}".encode())

            self.sequence += 1
            time.sleep(interval)

    def stop(self):
        self.running = False

    def close(self):
        self.publisher.undeclare()
        self.session.close()


def main():
    parser = argparse.ArgumentParser(description="{description}")
    parser.add_argument("-c", "--config", type=Path, default=Path("config.yaml"))
    parser.add_argument("-e", "--endpoint", type=str, default=None)
    args = parser.parse_args()

    node = Node(args.config, args.endpoint)

    def handler(signum, frame):
        node.stop()

    signal.signal(signal.SIGINT, handler)
    signal.signal(signal.SIGTERM, handler)

    try:
        node.run()
    finally:
        node.close()


if __name__ == "__main__":
    main()
""",
}


def _validate_node_name(name: str) -> str | None:
    """Validate node name. Returns error string or None if valid."""
    if not name:
        return "Node name cannot be empty."
    if not NODE_NAME_RE.match(name):
        return f"Invalid node name '{name}'. Must start with letter/digit, contain only alphanumeric, hyphens, underscores."
    if len(name) > 50:
        return "Node name too long (max 50 characters)."
    if name in (".", ".."):
        return "Invalid node name."
    return None


def _resolve_node_dir(name: str, allowed_paths: list[str]) -> Path:
    """Resolve where to create/find a node directory."""
    # Default to ~/.bubbaloop/nodes/{name}
    return Path.home() / ".bubbaloop" / "nodes" / name


def _is_path_safe(path: Path, allowed_roots: list[Path]) -> bool:
    """Check that a resolved path is under one of the allowed roots."""
    resolved = path.resolve()
    for root in allowed_roots:
        try:
            resolved.relative_to(root.resolve())
            return True
        except ValueError:
            continue
    return False


def register_node_creation_tools(registry: ToolRegistry, zenoh_bridge, world_model, config: dict):
    """Register node creation and lifecycle tools."""

    protected_nodes = config.get("safety", {}).get("protected_nodes", ["bubbaloop-agent"])
    allowed_node_paths = [
        Path.home() / ".bubbaloop" / "nodes",
        Path.home() / ".bubbaloop" / "configs",
        Path("/tmp/bubbaloop/nodes"),
    ]
    # Add any user-configured paths
    for p in config.get("safety", {}).get("allowed_node_paths", []):
        allowed_node_paths.append(Path(p))

    async def create_node(name: str, description: str = "", node_type: str = "python") -> str:
        """Scaffold a new node with all required files."""
        # Validate name
        err = _validate_node_name(name)
        if err:
            return f"Error: {err}"

        if node_type != "python":
            return "Error: Only Python nodes can be created by the agent currently."

        node_dir = _resolve_node_dir(name, [])
        if node_dir.exists():
            return f"Error: Node directory already exists at {node_dir}. Use write_node_file to modify existing nodes."

        # Create directory and all template files
        node_dir.mkdir(parents=True, exist_ok=True)

        created_files = []
        for filename, template in PYTHON_NODE_TEMPLATE.items():
            content = template.format(name=name, description=description or f"{name} node")
            filepath = node_dir / filename
            filepath.write_text(content)
            created_files.append(filename)

        logger.info(f"Created node '{name}' at {node_dir}")
        return (
            f"Node '{name}' created at {node_dir}\n"
            f"Files: {', '.join(created_files)}\n\n"
            f"Next steps:\n"
            f"1. Edit the code with write_node_file to add your logic\n"
            f"2. Use add_node to register it with the daemon\n"
            f"3. Use build_node then start_node to run it"
        )

    async def write_node_file(node_name: str, file_path: str, content: str) -> str:
        """Write or update a file in a node directory.

        Safety: only writes to allowed node directories, validates extensions,
        enforces size limits, blocks path traversal.
        """
        err = _validate_node_name(node_name)
        if err:
            return f"Error: {err}"

        # Resolve the node directory
        node_dir = _resolve_node_dir(node_name, [])
        if not node_dir.exists():
            return f"Error: Node '{node_name}' not found at {node_dir}. Create it first with create_node."

        # Validate file path (no traversal)
        if ".." in file_path or file_path.startswith("/"):
            return "Error: Path traversal not allowed. Use relative paths within the node directory."

        target = (node_dir / file_path).resolve()

        # Verify target is still under allowed roots
        if not _is_path_safe(target, allowed_node_paths):
            return f"Error: Cannot write outside allowed node directories."

        # Validate extension
        ext = target.suffix.lower()
        if ext and ext not in ALLOWED_EXTENSIONS:
            return f"Error: File extension '{ext}' not allowed. Allowed: {', '.join(sorted(ALLOWED_EXTENSIONS))}"

        # Validate size
        if len(content.encode("utf-8")) > MAX_FILE_SIZE:
            return f"Error: Content too large ({len(content)} chars). Max: {MAX_FILE_SIZE // 1024}KB."

        # Create parent dirs if needed
        target.parent.mkdir(parents=True, exist_ok=True)

        # Write the file
        target.write_text(content)
        logger.info(f"Wrote {len(content)} chars to {target}")
        return f"Written {target.relative_to(node_dir)} ({len(content)} chars)"

    async def read_node_file(node_name: str, file_path: str) -> str:
        """Read a file from a node directory."""
        err = _validate_node_name(node_name)
        if err:
            return f"Error: {err}"

        node_dir = _resolve_node_dir(node_name, [])
        if not node_dir.exists():
            return f"Error: Node '{node_name}' not found at {node_dir}."

        if ".." in file_path or file_path.startswith("/"):
            return "Error: Path traversal not allowed."

        target = node_dir / file_path
        if not target.exists():
            # List available files
            files = [str(f.relative_to(node_dir)) for f in node_dir.rglob("*") if f.is_file()]
            return f"Error: File '{file_path}' not found. Available files:\n" + "\n".join(f"  - {f}" for f in files[:20])

        content = target.read_text()
        if len(content) > MAX_FILE_SIZE:
            content = content[:MAX_FILE_SIZE] + "\n... (truncated)"
        return content

    async def list_node_files(node_name: str) -> str:
        """List all files in a node directory."""
        err = _validate_node_name(node_name)
        if err:
            return f"Error: {err}"

        node_dir = _resolve_node_dir(node_name, [])
        if not node_dir.exists():
            return f"Error: Node '{node_name}' not found at {node_dir}."

        files = []
        for f in sorted(node_dir.rglob("*")):
            if f.is_file():
                rel = f.relative_to(node_dir)
                size = f.stat().st_size
                files.append(f"  {rel} ({size} bytes)")

        return f"Node '{node_name}' at {node_dir}:\n" + "\n".join(files) if files else "No files found."

    async def add_node(node_name: str) -> str:
        """Register a node with the daemon so it can be managed."""
        err = _validate_node_name(node_name)
        if err:
            return f"Error: {err}"

        node_dir = _resolve_node_dir(node_name, [])
        if not node_dir.exists():
            return f"Error: Node '{node_name}' not found at {node_dir}."

        # Check node.yaml exists
        if not (node_dir / "node.yaml").exists():
            return f"Error: No node.yaml found in {node_dir}."

        payload = json.dumps({
            "node_path": str(node_dir),
            "name": node_name,
        })
        result = await zenoh_bridge.query_daemon("nodes/add", payload)
        await world_model.refresh()
        return result

    async def remove_node(node_name: str) -> str:
        """Unregister a node from the daemon (does not delete files)."""
        err = _validate_node_name(node_name)
        if err:
            return f"Error: {err}"

        if node_name in protected_nodes:
            return f"Cannot remove protected node '{node_name}'."

        payload = json.dumps({"command": "remove"})
        result = await zenoh_bridge.query_daemon(f"nodes/{node_name}/command", payload)
        await world_model.refresh()
        return result

    async def install_node(node_name: str) -> str:
        """Install a node as a systemd service (enables daemon management)."""
        err = _validate_node_name(node_name)
        if err:
            return f"Error: {err}"

        payload = json.dumps({"command": "install"})
        result = await zenoh_bridge.query_daemon(f"nodes/{node_name}/command", payload)
        await world_model.refresh()
        return result

    async def uninstall_node(node_name: str) -> str:
        """Remove a node's systemd service."""
        err = _validate_node_name(node_name)
        if err:
            return f"Error: {err}"

        if node_name in protected_nodes:
            return f"Cannot uninstall protected node '{node_name}'."

        payload = json.dumps({"command": "uninstall"})
        result = await zenoh_bridge.query_daemon(f"nodes/{node_name}/command", payload)
        await world_model.refresh()
        return result

    # --- Register all tools ---

    registry.register(ToolDefinition(
        name="create_node",
        description="Scaffold a new bubbaloop node with all required files (node.yaml, main.py, config.yaml, pixi.toml). Creates under ~/.bubbaloop/nodes/.",
        parameters={
            "type": "object",
            "properties": {
                "name": {
                    "type": "string",
                    "description": "Node name (alphanumeric, hyphens, underscores). E.g., 'temperature-logger', 'motion-detector'.",
                },
                "description": {
                    "type": "string",
                    "description": "What this node does. E.g., 'Monitors temperature sensors and publishes readings'.",
                },
                "node_type": {
                    "type": "string",
                    "enum": ["python"],
                    "description": "Node type. Currently only 'python' is supported.",
                },
            },
            "required": ["name", "description"],
        },
        handler=create_node,
        skill="node-management",
    ))

    registry.register(ToolDefinition(
        name="write_node_file",
        description="Write or update a file in a node directory. Use to add custom logic, config, protos, or dependencies. Path must be relative to the node root.",
        parameters={
            "type": "object",
            "properties": {
                "node_name": {
                    "type": "string",
                    "description": "Name of the node to modify.",
                },
                "file_path": {
                    "type": "string",
                    "description": "Relative path within the node (e.g., 'main.py', 'src/logic.py', 'config.yaml').",
                },
                "content": {
                    "type": "string",
                    "description": "Full file content to write.",
                },
            },
            "required": ["node_name", "file_path", "content"],
        },
        handler=write_node_file,
        skill="node-management",
    ))

    registry.register(ToolDefinition(
        name="read_node_file",
        description="Read a file from a node directory. Useful for reviewing code before modifying.",
        parameters={
            "type": "object",
            "properties": {
                "node_name": {
                    "type": "string",
                    "description": "Name of the node.",
                },
                "file_path": {
                    "type": "string",
                    "description": "Relative path within the node (e.g., 'main.py', 'config.yaml').",
                },
            },
            "required": ["node_name", "file_path"],
        },
        handler=read_node_file,
        skill="node-management",
    ))

    registry.register(ToolDefinition(
        name="list_node_files",
        description="List all files in a node directory with sizes.",
        parameters={
            "type": "object",
            "properties": {
                "node_name": {
                    "type": "string",
                    "description": "Name of the node.",
                },
            },
            "required": ["node_name"],
        },
        handler=list_node_files,
        skill="node-management",
    ))

    registry.register(ToolDefinition(
        name="add_node",
        description="Register a node with the daemon (makes it manageable via start/stop/build). Node must exist with a valid node.yaml.",
        parameters={
            "type": "object",
            "properties": {
                "node_name": {
                    "type": "string",
                    "description": "Name of the node to register.",
                },
            },
            "required": ["node_name"],
        },
        handler=add_node,
        skill="node-management",
    ))

    registry.register(ToolDefinition(
        name="remove_node",
        description="Unregister a node from the daemon. Does NOT delete node files, just removes daemon tracking.",
        parameters={
            "type": "object",
            "properties": {
                "node_name": {
                    "type": "string",
                    "description": "Name of the node to remove.",
                },
            },
            "required": ["node_name"],
        },
        handler=remove_node,
        skill="node-management",
    ))

    registry.register(ToolDefinition(
        name="install_node",
        description="Install a node as a systemd service. After installing, the node can be started/stopped via daemon.",
        parameters={
            "type": "object",
            "properties": {
                "node_name": {
                    "type": "string",
                    "description": "Name of the node to install.",
                },
            },
            "required": ["node_name"],
        },
        handler=install_node,
        skill="node-management",
    ))

    registry.register(ToolDefinition(
        name="uninstall_node",
        description="Remove a node's systemd service. Node files are kept, just the service is removed.",
        parameters={
            "type": "object",
            "properties": {
                "node_name": {
                    "type": "string",
                    "description": "Name of the node to uninstall.",
                },
            },
            "required": ["node_name"],
        },
        handler=uninstall_node,
        skill="node-management",
    ))
