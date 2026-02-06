"""Data operation tools - save streams, stop captures, list captures."""

import logging

from .registry import ToolRegistry, ToolDefinition

logger = logging.getLogger(__name__)


def register_data_tools(registry: ToolRegistry, data_router):
    """Register data capture tools."""

    async def save_stream(
        topic: str,
        output_path: str,
        format: str = "json",
        max_files: int = 0,
    ) -> str:
        """Start capturing data from a topic to files."""
        return await data_router.start_capture(
            topic=topic,
            output_path=output_path,
            format=format,
            max_files=max_files,
        )

    async def stop_capture(capture_id: str) -> str:
        """Stop a data capture."""
        return await data_router.stop_capture(capture_id)

    async def list_captures() -> str:
        """List active data captures."""
        return data_router.describe_all()

    registry.register(ToolDefinition(
        name="save_stream",
        description="Start capturing data from a Zenoh topic to files on disk.",
        parameters={
            "type": "object",
            "properties": {
                "topic": {
                    "type": "string",
                    "description": "Topic suffix to capture (e.g., 'weather/current').",
                },
                "output_path": {
                    "type": "string",
                    "description": "Directory to save files to (must be in allowed paths).",
                },
                "format": {
                    "type": "string",
                    "enum": ["json", "csv", "raw", "h264"],
                    "description": "Output format (default: json).",
                },
                "max_files": {
                    "type": "integer",
                    "description": "Max files to keep (0 = unlimited).",
                },
            },
            "required": ["topic", "output_path"],
        },
        handler=save_stream,
        skill="data-ops",
    ))

    registry.register(ToolDefinition(
        name="stop_capture",
        description="Stop an active data capture.",
        parameters={
            "type": "object",
            "properties": {
                "capture_id": {
                    "type": "string",
                    "description": "ID of the capture to stop.",
                },
            },
            "required": ["capture_id"],
        },
        handler=stop_capture,
        skill="data-ops",
    ))

    registry.register(ToolDefinition(
        name="list_captures",
        description="List all active data captures with details.",
        parameters={"type": "object", "properties": {}},
        handler=list_captures,
        skill="data-ops",
    ))
