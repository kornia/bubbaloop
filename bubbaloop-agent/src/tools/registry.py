"""Tool registry - discovers, registers, describes, and executes tools."""

import logging
from dataclasses import dataclass, field
from typing import Any, Callable, Awaitable

logger = logging.getLogger(__name__)


@dataclass
class ToolDefinition:
    """A tool that the LLM can call."""
    name: str
    description: str
    parameters: dict  # JSON Schema for parameters
    handler: Callable[..., Awaitable[str]]
    skill: str = ""  # Which skill this belongs to


class ToolRegistry:
    """Registry for all available tools."""

    def __init__(self):
        self._tools: dict[str, ToolDefinition] = {}

    def register(self, tool: ToolDefinition):
        """Register a tool."""
        if tool.name in self._tools:
            logger.warning(f"Tool '{tool.name}' already registered, overwriting")
        self._tools[tool.name] = tool
        logger.debug(f"Registered tool: {tool.name} (skill: {tool.skill})")

    def get(self, name: str) -> ToolDefinition | None:
        """Get a tool by name."""
        return self._tools.get(name)

    def all_definitions(self) -> list[dict]:
        """Get all tool definitions in OpenAI function calling format."""
        return [
            {
                "type": "function",
                "function": {
                    "name": tool.name,
                    "description": tool.description,
                    "parameters": tool.parameters,
                },
            }
            for tool in self._tools.values()
        ]

    def subset_definitions(self, names: list[str]) -> list[dict]:
        """Get tool definitions for a subset of tools."""
        return [
            {
                "type": "function",
                "function": {
                    "name": tool.name,
                    "description": tool.description,
                    "parameters": tool.parameters,
                },
            }
            for name in names
            if (tool := self._tools.get(name))
        ]

    async def execute(self, name: str, arguments: dict) -> str:
        """Execute a tool by name with arguments."""
        tool = self._tools.get(name)
        if not tool:
            return f"Error: Unknown tool '{name}'"

        try:
            result = await tool.handler(**arguments)
            return str(result)
        except TypeError as e:
            return f"Error calling {name}: invalid arguments - {e}"
        except Exception as e:
            logger.error(f"Tool {name} failed: {e}", exc_info=True)
            return f"Error: {name} failed - {e}"

    def list_tools(self) -> list[str]:
        """List all registered tool names."""
        return list(self._tools.keys())

    def describe_all(self) -> str:
        """Get a human-readable description of all tools (for system prompt)."""
        by_skill: dict[str, list[ToolDefinition]] = {}
        for tool in self._tools.values():
            skill = tool.skill or "general"
            by_skill.setdefault(skill, []).append(tool)

        lines = []
        for skill, tools in sorted(by_skill.items()):
            lines.append(f"\n### {skill}")
            for tool in tools:
                params = tool.parameters.get("properties", {})
                param_names = ", ".join(params.keys())
                lines.append(f"- **{tool.name}**({param_names}): {tool.description}")
        return "\n".join(lines)
