"""Node management tools - list, start, stop, restart, build, logs via daemon API."""

import json
import logging

from .registry import ToolRegistry, ToolDefinition

logger = logging.getLogger(__name__)


def register_node_tools(registry: ToolRegistry, zenoh_bridge, world_model, config: dict):
    """Register node management tools."""

    protected_nodes = config.get("safety", {}).get("protected_nodes", ["bubbaloop-agent"])

    async def list_nodes() -> str:
        """List all nodes with their status."""
        await world_model.refresh()
        return world_model.to_text()

    async def start_node(name: str) -> str:
        """Start a node."""
        if name in protected_nodes:
            return f"Cannot modify protected node '{name}'."
        payload = json.dumps({"command": "start"})
        result = await zenoh_bridge.query_daemon(f"nodes/{name}/command", payload)
        await world_model.refresh()
        return result

    async def stop_node(name: str) -> str:
        """Stop a node."""
        if name in protected_nodes:
            return f"Cannot modify protected node '{name}'."
        payload = json.dumps({"command": "stop"})
        result = await zenoh_bridge.query_daemon(f"nodes/{name}/command", payload)
        await world_model.refresh()
        return result

    async def restart_node(name: str) -> str:
        """Restart a node."""
        if name in protected_nodes:
            return f"Cannot modify protected node '{name}'."
        payload = json.dumps({"command": "restart"})
        result = await zenoh_bridge.query_daemon(f"nodes/{name}/command", payload)
        await world_model.refresh()
        return result

    async def build_node(name: str) -> str:
        """Build/rebuild a node."""
        payload = json.dumps({"command": "build"})
        result = await zenoh_bridge.query_daemon(f"nodes/{name}/command", payload)
        return result

    async def get_logs(name: str) -> str:
        """Get recent logs from a node."""
        result = await zenoh_bridge.query_daemon(f"nodes/{name}/logs")
        return result

    registry.register(ToolDefinition(
        name="list_nodes",
        description="List all registered nodes with status, health, and description.",
        parameters={"type": "object", "properties": {}},
        handler=list_nodes,
        skill="node-management",
    ))

    registry.register(ToolDefinition(
        name="start_node",
        description="Start a stopped node.",
        parameters={
            "type": "object",
            "properties": {
                "name": {"type": "string", "description": "Node name to start."},
            },
            "required": ["name"],
        },
        handler=start_node,
        skill="node-management",
    ))

    registry.register(ToolDefinition(
        name="stop_node",
        description="Stop a running node. Protected nodes cannot be stopped.",
        parameters={
            "type": "object",
            "properties": {
                "name": {"type": "string", "description": "Node name to stop."},
            },
            "required": ["name"],
        },
        handler=stop_node,
        skill="node-management",
    ))

    registry.register(ToolDefinition(
        name="restart_node",
        description="Restart a node (stop then start). Protected nodes cannot be restarted.",
        parameters={
            "type": "object",
            "properties": {
                "name": {"type": "string", "description": "Node name to restart."},
            },
            "required": ["name"],
        },
        handler=restart_node,
        skill="node-management",
    ))

    registry.register(ToolDefinition(
        name="build_node",
        description="Build/rebuild a node (install deps, compile).",
        parameters={
            "type": "object",
            "properties": {
                "name": {"type": "string", "description": "Node name to build."},
            },
            "required": ["name"],
        },
        handler=build_node,
        skill="node-management",
    ))

    registry.register(ToolDefinition(
        name="get_logs",
        description="Get recent logs from a node.",
        parameters={
            "type": "object",
            "properties": {
                "name": {"type": "string", "description": "Node name to get logs for."},
            },
            "required": ["name"],
        },
        handler=get_logs,
        skill="node-management",
    ))
