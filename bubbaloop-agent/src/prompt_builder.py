"""Dynamic system prompt builder - assembles context from runtime state on every LLM call."""

import logging
from pathlib import Path

logger = logging.getLogger(__name__)


class PromptBuilder:
    """Builds system prompts dynamically from runtime state."""

    def __init__(
        self,
        base_dir: Path,
        world_model,
        watcher_engine,
        data_router,
        tool_registry,
        memory,
        config: dict,
    ):
        self.base_dir = base_dir
        self.world_model = world_model
        self.watcher_engine = watcher_engine
        self.data_router = data_router
        self.tool_registry = tool_registry
        self.memory = memory
        self.config = config

    def _load_file(self, filename: str) -> str:
        """Load a markdown file from the base directory."""
        path = self.base_dir / filename
        if path.exists():
            return path.read_text().strip()
        return ""

    def build(self) -> str:
        """Build the complete system prompt from runtime state."""
        sections = []

        # 1. Identity (SOUL.md)
        soul = self._load_file("SOUL.md")
        if soul:
            sections.append(soul)
        else:
            sections.append("# Bubbaloop Agent\nYou are an autonomous agent managing a Physical AI system.")

        # 2. Live world model
        world_text = self.world_model.to_text()
        sections.append(f"""## Current System State
{world_text}""")

        # 3. Active watchers
        watcher_text = self.watcher_engine.describe_all()
        if watcher_text and watcher_text != "No active watchers.":
            sections.append(f"""## Active Watchers
{watcher_text}""")

        # 4. Active data captures
        capture_text = self.data_router.describe_all()
        if capture_text and capture_text != "No active captures.":
            sections.append(f"""## Active Data Captures
{capture_text}""")

        # 5. Available tools
        tool_text = self.tool_registry.describe_all()
        sections.append(f"""## Your Capabilities
You have these tools available:
{tool_text}""")

        # 6. Memory
        memory_text = self.memory.get_all()
        if memory_text:
            sections.append(f"""## Memory (Your Persistent Learnings)
{memory_text}""")

        # 7. Safety boundaries
        safety = self.config.get("safety", {})
        allowed_paths = ", ".join(safety.get("allowed_data_paths", ["/data/", "/tmp/bubbaloop/"]))
        protected = ", ".join(safety.get("protected_nodes", ["bubbaloop-agent"]))
        max_actions = self.config.get("watchers", {}).get("max_actions_per_hour", 30)

        sections.append(f"""## Safety Rules
- You can freely read data and check status (READ operations)
- For actions that change system state (start/stop/restart nodes), explain what you'll do first
- Never stop these protected nodes: {protected}
- Data can only be saved to: {allowed_paths}
- Maximum {max_actions} automated actions per hour per watcher
- Always confirm destructive actions with the user unless in a watcher with clear instructions""")

        return "\n\n".join(sections)

    def build_watcher_context(self) -> str:
        """Build a minimal context for watcher evaluation calls."""
        sections = []

        soul = self._load_file("SOUL.md")
        if soul:
            # Just the first paragraph for watchers (keep it short)
            first_section = soul.split("\n##")[0].strip()
            sections.append(first_section)

        sections.append(f"""## System State (Summary)
{self.world_model.to_text()}""")

        # Watchers get a subset of tools
        sections.append("""## Available Actions
You can use tools to take action when conditions are met.
Be conservative - only act when clearly needed.""")

        return "\n\n".join(sections)
