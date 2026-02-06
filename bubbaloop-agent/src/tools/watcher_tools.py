"""Watcher tools - create, list, remove, pause LLM-driven data stream monitors."""

import logging

from .registry import ToolRegistry, ToolDefinition

logger = logging.getLogger(__name__)


def register_watcher_tools(registry: ToolRegistry, watcher_engine):
    """Register watcher management tools."""

    async def create_watcher(
        name: str,
        topics: list[str],
        instruction: str,
        sample_interval_sec: int = 30,
        max_actions_per_hour: int = 10,
    ) -> str:
        """Create a new watcher."""
        return await watcher_engine.create_watcher(
            name=name,
            topics=topics,
            instruction=instruction,
            sample_interval_sec=sample_interval_sec,
            max_actions_per_hour=max_actions_per_hour,
        )

    async def list_watchers() -> str:
        """List all watchers."""
        return watcher_engine.describe_all()

    async def remove_watcher(name: str) -> str:
        """Remove a watcher."""
        return await watcher_engine.remove_watcher(name)

    async def pause_watcher(name: str, paused: bool = True) -> str:
        """Pause or resume a watcher."""
        return await watcher_engine.pause_watcher(name, paused)

    registry.register(ToolDefinition(
        name="create_watcher",
        description="Create a persistent monitor on data streams. The watcher will periodically check data and use LLM reasoning to decide if action is needed.",
        parameters={
            "type": "object",
            "properties": {
                "name": {
                    "type": "string",
                    "description": "Unique watcher name (e.g., 'disk-monitor').",
                },
                "topics": {
                    "type": "array",
                    "items": {"type": "string"},
                    "description": "Topic suffixes to monitor (e.g., ['system-telemetry/metrics']).",
                },
                "instruction": {
                    "type": "string",
                    "description": "Natural language instruction: what to monitor and when to act.",
                },
                "sample_interval_sec": {
                    "type": "integer",
                    "description": "Seconds between checks (default: 30).",
                },
                "max_actions_per_hour": {
                    "type": "integer",
                    "description": "Max automated actions per hour (default: 10).",
                },
            },
            "required": ["name", "topics", "instruction"],
        },
        handler=create_watcher,
        skill="watchers",
    ))

    registry.register(ToolDefinition(
        name="list_watchers",
        description="List all active watchers with status and recent evaluation history.",
        parameters={"type": "object", "properties": {}},
        handler=list_watchers,
        skill="watchers",
    ))

    registry.register(ToolDefinition(
        name="remove_watcher",
        description="Remove a watcher, stopping its monitoring.",
        parameters={
            "type": "object",
            "properties": {
                "name": {"type": "string", "description": "Watcher name to remove."},
            },
            "required": ["name"],
        },
        handler=remove_watcher,
        skill="watchers",
    ))

    registry.register(ToolDefinition(
        name="pause_watcher",
        description="Pause or resume a watcher.",
        parameters={
            "type": "object",
            "properties": {
                "name": {"type": "string", "description": "Watcher name."},
                "paused": {"type": "boolean", "description": "True to pause, false to resume."},
            },
            "required": ["name", "paused"],
        },
        handler=pause_watcher,
        skill="watchers",
    ))
