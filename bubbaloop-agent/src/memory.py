"""Persistent memory system - MEMORY.md + conversation JSONL persistence."""

import json
import logging
import re
import time
from pathlib import Path

logger = logging.getLogger(__name__)


class Memory:
    """Persistent memory via MEMORY.md and conversation JSONL files."""

    # Patterns that indicate an attempt to inject safety overrides into memory.
    # These are checked case-insensitively against remember() content.
    BLOCKED_PATTERNS = [
        r"ignore\s+(all\s+)?(safety|rules|instructions|boundaries|constraints)",
        r"override\s+(safety|rules|config|protected)",
        r"disregard\s+(previous|safety|rules|all)",
        r"you\s+(can|should|must)\s+(now\s+)?(ignore|bypass|skip|override)",
        r"new\s+(rule|instruction|policy)\s*:",
        r"forget\s+(all\s+)?(safety|rules|boundaries)",
        r"protected.?nodes?\s*[:=]",
        r"allowed.?paths?\s*[:=]",
        r"max.?actions?\s*[:=]",
        r"system\s*prompt",
        r"you\s+are\s+now\s+",
        r"from\s+now\s+on\s+you",
    ]

    # Max total memory size (characters). Prevents prompt flooding.
    MAX_MEMORY_SIZE = 5000

    # Max entries per category
    MAX_ENTRIES_PER_CATEGORY = 20

    def __init__(self, data_dir: Path):
        self.data_dir = data_dir
        self.memory_file = data_dir / "MEMORY.md"
        self.conversations_dir = data_dir / "conversations"
        self.conversations_dir.mkdir(parents=True, exist_ok=True)

        # Compile blocked patterns once
        self._blocked_re = [re.compile(p, re.IGNORECASE) for p in self.BLOCKED_PATTERNS]

        # Ensure MEMORY.md exists
        if not self.memory_file.exists():
            self.memory_file.write_text("# Agent Memory\n\n")

    def get_all(self) -> str:
        """Get the full contents of MEMORY.md."""
        if self.memory_file.exists():
            content = self.memory_file.read_text().strip()
            if content == "# Agent Memory":
                return ""  # Empty memory
            return content
        return ""

    def _is_blocked_content(self, content: str) -> str | None:
        """Check if content attempts to inject safety overrides.

        Returns the matched pattern description or None if clean.
        """
        for pattern in self._blocked_re:
            if pattern.search(content):
                logger.warning(f"Blocked memory write (safety override attempt): {content[:80]}...")
                return pattern.pattern
        return None

    def _count_category_entries(self, text: str, category: str) -> int:
        """Count how many entries exist in a category."""
        category_header = f"## {category.title()}"
        lines = text.split("\n")
        count = 0
        in_section = False
        for line in lines:
            if line.strip() == category_header:
                in_section = True
            elif line.startswith("## "):
                in_section = False
            elif in_section and line.strip().startswith("- "):
                count += 1
        return count

    def remember(self, content: str, category: str = "general") -> str:
        """Add a memory entry to MEMORY.md under the given category."""

        # Safety: block content that tries to override rules
        blocked = self._is_blocked_content(content)
        if blocked:
            return "Cannot store this memory: content appears to modify safety rules or system instructions."

        # Safety: block attempts to use categories that sound like system override
        blocked_categories = {"safety", "rules", "system", "config", "prompt", "instructions"}
        if category.lower() in blocked_categories:
            return f"Cannot use reserved category '{category}'. Use: user, preferences, patterns, issues, or general."

        current = self.memory_file.read_text() if self.memory_file.exists() else "# Agent Memory\n\n"

        # Safety: check total memory size
        if len(current) >= self.MAX_MEMORY_SIZE:
            return f"Memory is full ({len(current)} chars, max {self.MAX_MEMORY_SIZE}). Use `forget` to remove old entries first."

        # Safety: check entries per category
        if self._count_category_entries(current, category) >= self.MAX_ENTRIES_PER_CATEGORY:
            return f"Category '{category}' has {self.MAX_ENTRIES_PER_CATEGORY} entries (max). Use `forget` to remove old entries first."

        # Find or create the category section
        category_header = f"## {category.title()}"
        if category_header in current:
            # Append to existing section
            lines = current.split("\n")
            insert_idx = None
            for i, line in enumerate(lines):
                if line.strip() == category_header:
                    # Find the end of this section (next ## or end of file)
                    for j in range(i + 1, len(lines)):
                        if lines[j].startswith("## "):
                            insert_idx = j
                            break
                    if insert_idx is None:
                        insert_idx = len(lines)
                    break

            if insert_idx is not None:
                lines.insert(insert_idx, f"- {content}")
                current = "\n".join(lines)
        else:
            # Create new section
            current = current.rstrip() + f"\n\n{category_header}\n- {content}\n"

        self.memory_file.write_text(current)
        logger.info(f"Memory stored: [{category}] {content[:50]}...")
        return f"Remembered under '{category}': {content}"

    def recall(self, query: str) -> str:
        """Search memory for entries matching the query."""
        current = self.get_all()
        if not current:
            return "No memories stored yet."

        # Simple keyword matching
        query_words = set(query.lower().split())
        lines = current.split("\n")
        matches = []

        current_section = "general"
        for line in lines:
            if line.startswith("## "):
                current_section = line[3:].strip()
            elif line.strip().startswith("- "):
                entry = line.strip()[2:]
                entry_words = set(entry.lower().split())
                overlap = query_words & entry_words
                if overlap:
                    matches.append(f"[{current_section}] {entry}")

        if not matches:
            # Return full memory if no specific matches
            return f"No specific matches for '{query}'. Full memory:\n{current}"

        return "Matching memories:\n" + "\n".join(f"- {m}" for m in matches)

    def forget(self, content: str) -> str:
        """Remove memory entries matching the description."""
        current = self.memory_file.read_text() if self.memory_file.exists() else ""
        if not current:
            return "No memories to forget."

        lines = current.split("\n")
        removed = []
        new_lines = []
        query_words = set(content.lower().split())

        for line in lines:
            if line.strip().startswith("- "):
                entry = line.strip()[2:]
                entry_words = set(entry.lower().split())
                overlap = query_words & entry_words
                # Remove if more than half the query words match
                if len(overlap) > len(query_words) / 2:
                    removed.append(entry)
                    continue
            new_lines.append(line)

        if removed:
            self.memory_file.write_text("\n".join(new_lines))
            return f"Forgot {len(removed)} entries:\n" + "\n".join(f"- {r}" for r in removed)
        return f"No memories matching '{content}' found."

    def get_conversation(self, conversation_id: str) -> list[dict]:
        """Load a conversation from JSONL file."""
        conv_file = self.conversations_dir / f"{conversation_id}.jsonl"
        if not conv_file.exists():
            return []

        messages = []
        for line in conv_file.read_text().strip().split("\n"):
            if line.strip():
                try:
                    messages.append(json.loads(line))
                except json.JSONDecodeError:
                    continue
        return messages

    def save_conversation(self, conversation_id: str, messages: list[dict]):
        """Save a conversation to JSONL file."""
        conv_file = self.conversations_dir / f"{conversation_id}.jsonl"
        with open(conv_file, "w") as f:
            for msg in messages:
                # Only save user and assistant messages (not system)
                if msg.get("role") in ("user", "assistant"):
                    f.write(json.dumps(msg, default=str) + "\n")

    def append_to_conversation(self, conversation_id: str, message: dict):
        """Append a single message to a conversation."""
        if message.get("role") not in ("user", "assistant"):
            return
        conv_file = self.conversations_dir / f"{conversation_id}.jsonl"
        with open(conv_file, "a") as f:
            f.write(json.dumps(message, default=str) + "\n")

    def is_first_run(self) -> bool:
        """Check if this is a fresh agent with no user interaction history."""
        # No meaningful memory yet
        content = self.get_all()
        has_memory = content and content != "# Agent Memory"

        # No past conversations
        conv_files = list(self.conversations_dir.glob("*.jsonl"))
        has_conversations = len(conv_files) > 0

        return not has_memory and not has_conversations

    def get_user_section(self) -> str:
        """Extract user-specific memories (category: user/preferences)."""
        content = self.get_all()
        if not content:
            return ""
        lines = content.split("\n")
        user_lines = []
        in_user_section = False
        for line in lines:
            if line.strip() in ("## User", "## Preferences"):
                in_user_section = True
                continue
            elif line.startswith("## "):
                in_user_section = False
            elif in_user_section and line.strip():
                user_lines.append(line.strip())
        return "\n".join(user_lines)

    def conversation_count(self) -> int:
        """Return the number of past conversations."""
        return len(list(self.conversations_dir.glob("*.jsonl")))
