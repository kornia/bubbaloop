"""Persistent memory system - MEMORY.md + conversation JSONL persistence."""

import json
import logging
import re
import time
from pathlib import Path

logger = logging.getLogger(__name__)


class Memory:
    """Persistent memory via MEMORY.md and conversation JSONL files."""

    def __init__(self, data_dir: Path):
        self.data_dir = data_dir
        self.memory_file = data_dir / "MEMORY.md"
        self.conversations_dir = data_dir / "conversations"
        self.conversations_dir.mkdir(parents=True, exist_ok=True)

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

    def remember(self, content: str, category: str = "general") -> str:
        """Add a memory entry to MEMORY.md under the given category."""
        current = self.memory_file.read_text() if self.memory_file.exists() else "# Agent Memory\n\n"

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
