"""Memory tools - remember, recall, forget persistent information."""

import logging

from .registry import ToolRegistry, ToolDefinition

logger = logging.getLogger(__name__)


def register_memory_tools(registry: ToolRegistry, memory):
    """Register memory tools."""

    async def remember(content: str, category: str = "general") -> str:
        """Store information in persistent memory."""
        return memory.remember(content, category)

    async def recall(query: str) -> str:
        """Search memory for relevant information."""
        return memory.recall(query)

    async def forget(content: str) -> str:
        """Remove information from memory."""
        return memory.forget(content)

    registry.register(ToolDefinition(
        name="remember",
        description="Store a piece of information in persistent memory (survives restarts).",
        parameters={
            "type": "object",
            "properties": {
                "content": {
                    "type": "string",
                    "description": "What to remember. Be specific and concise.",
                },
                "category": {
                    "type": "string",
                    "description": "Category: 'patterns', 'preferences', 'issues', 'general'.",
                },
            },
            "required": ["content"],
        },
        handler=remember,
        skill="memory",
    ))

    registry.register(ToolDefinition(
        name="recall",
        description="Search persistent memory for relevant information.",
        parameters={
            "type": "object",
            "properties": {
                "query": {
                    "type": "string",
                    "description": "What to search for.",
                },
            },
            "required": ["query"],
        },
        handler=recall,
        skill="memory",
    ))

    registry.register(ToolDefinition(
        name="forget",
        description="Remove information from persistent memory.",
        parameters={
            "type": "object",
            "properties": {
                "content": {
                    "type": "string",
                    "description": "Description of what to forget (matched and removed).",
                },
            },
            "required": ["content"],
        },
        handler=forget,
        skill="memory",
    ))
