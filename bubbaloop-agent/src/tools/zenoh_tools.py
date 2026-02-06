"""Zenoh core tools - subscribe, query, publish via Zenoh network."""

import json
import logging

from .registry import ToolRegistry, ToolDefinition

logger = logging.getLogger(__name__)


def register_zenoh_tools(registry: ToolRegistry, zenoh_bridge):
    """Register Zenoh interaction tools."""

    async def subscribe_topic(topic: str) -> str:
        """Subscribe to a topic and return its latest data."""
        try:
            # Ensure we're subscribed
            zenoh_bridge.subscribe(topic)

            # Get latest data
            sample = zenoh_bridge.get_latest(topic)
            if sample is None:
                return f"Subscribed to '{topic}' but no data received yet. Data will be available on next check."

            decoded = zenoh_bridge.decode_sample(sample, topic)
            if isinstance(decoded, dict):
                return json.dumps(decoded, indent=2, default=str)
            return str(decoded)
        except Exception as e:
            return f"Error subscribing to '{topic}': {e}"

    async def query_topic(key: str, payload: str = "") -> str:
        """Query a Zenoh key expression."""
        return await zenoh_bridge.query(key, payload if payload else None)

    async def publish_message(topic: str, data: str) -> str:
        """Publish a message to a topic."""
        try:
            zenoh_bridge.publish(topic, data.encode("utf-8"))
            return f"Published to '{topic}'"
        except Exception as e:
            return f"Error publishing to '{topic}': {e}"

    registry.register(ToolDefinition(
        name="subscribe_topic",
        description="Subscribe to a Zenoh topic and get its latest data. Topic is a suffix like 'system-telemetry/metrics'.",
        parameters={
            "type": "object",
            "properties": {
                "topic": {
                    "type": "string",
                    "description": "Topic suffix (e.g., 'system-telemetry/metrics'). Will be scoped automatically.",
                },
            },
            "required": ["topic"],
        },
        handler=subscribe_topic,
        skill="zenoh-core",
    ))

    registry.register(ToolDefinition(
        name="query_topic",
        description="Query a Zenoh key expression (for daemon API or one-shot reads). Use full key for daemon API, suffix for scoped topics.",
        parameters={
            "type": "object",
            "properties": {
                "key": {
                    "type": "string",
                    "description": "Zenoh key expression to query.",
                },
                "payload": {
                    "type": "string",
                    "description": "Optional JSON payload to send with query.",
                },
            },
            "required": ["key"],
        },
        handler=query_topic,
        skill="zenoh-core",
    ))

    registry.register(ToolDefinition(
        name="publish_message",
        description="Publish a message to a Zenoh topic.",
        parameters={
            "type": "object",
            "properties": {
                "topic": {
                    "type": "string",
                    "description": "Topic suffix to publish to.",
                },
                "data": {
                    "type": "string",
                    "description": "Message content (string or JSON).",
                },
            },
            "required": ["topic", "data"],
        },
        handler=publish_message,
        skill="zenoh-core",
    ))
