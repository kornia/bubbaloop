"""Watcher engine - LLM-driven persistent data stream monitors.

Watchers are NOT hardcoded rules. Each watcher is an ongoing LLM conversation
about a data stream. The LLM decides when to act based on natural language instructions.
"""

import asyncio
import json
import logging
import time
from dataclasses import dataclass, field
from pathlib import Path

from .llm import OpenAICompatProvider, LLMResponse
from .tools import ToolRegistry

logger = logging.getLogger(__name__)


@dataclass
class WatcherEval:
    """Record of a single watcher evaluation."""
    timestamp: float
    data: dict
    assessment: str
    actions_taken: list[str] = field(default_factory=list)


@dataclass
class Watcher:
    """A persistent data stream monitor."""
    name: str
    topics: list[str]
    instruction: str
    sample_interval_sec: int = 30
    max_actions_per_hour: int = 10
    paused: bool = False
    created_at: float = field(default_factory=time.time)
    history: list[WatcherEval] = field(default_factory=list)
    actions_this_hour: int = 0
    actions_hour_start: float = field(default_factory=time.time)

    def format_recent_history(self, n: int = 5) -> str:
        """Format the last N evaluations as text."""
        if not self.history:
            return "No previous evaluations."

        lines = []
        for ev in self.history[-n:]:
            ts = time.strftime("%H:%M:%S", time.localtime(ev.timestamp))
            actions = f" | Actions: {', '.join(ev.actions_taken)}" if ev.actions_taken else ""
            lines.append(f"[{ts}] {ev.assessment}{actions}")
        return "\n".join(lines)

    def can_act(self) -> bool:
        """Check if the watcher is within its action rate limit."""
        now = time.time()
        if now - self.actions_hour_start > 3600:
            self.actions_this_hour = 0
            self.actions_hour_start = now
        return self.actions_this_hour < self.max_actions_per_hour

    def record_action(self):
        """Record that an action was taken."""
        now = time.time()
        if now - self.actions_hour_start > 3600:
            self.actions_this_hour = 0
            self.actions_hour_start = now
        self.actions_this_hour += 1

    def to_dict(self) -> dict:
        """Serialize for persistence."""
        return {
            "name": self.name,
            "topics": self.topics,
            "instruction": self.instruction,
            "sample_interval_sec": self.sample_interval_sec,
            "max_actions_per_hour": self.max_actions_per_hour,
            "paused": self.paused,
            "created_at": self.created_at,
        }

    @classmethod
    def from_dict(cls, data: dict) -> "Watcher":
        return cls(
            name=data["name"],
            topics=data["topics"],
            instruction=data["instruction"],
            sample_interval_sec=data.get("sample_interval_sec", 30),
            max_actions_per_hour=data.get("max_actions_per_hour", 10),
            paused=data.get("paused", False),
            created_at=data.get("created_at", time.time()),
        )


class WatcherEngine:
    """Manages LLM-driven watchers that monitor data streams."""

    def __init__(
        self,
        zenoh_bridge,
        tool_registry: ToolRegistry,
        prompt_builder,
        config: dict,
        data_dir: Path,
    ):
        self.zenoh = zenoh_bridge
        self.tools = tool_registry
        self.prompt_builder = prompt_builder
        self.config = config
        self.data_dir = data_dir
        self.watchers_file = data_dir / "watchers.json"

        self.watchers: dict[str, Watcher] = {}
        self._tasks: dict[str, asyncio.Task] = {}
        self._running = False

        # Watcher evaluation LLM (can be a smaller/cheaper model)
        watcher_config = config.get("watchers", {})
        self._eval_llm = OpenAICompatProvider({
            "base_url": watcher_config.get("eval_base_url", config.get("llm", {}).get("base_url", "http://localhost:11434/v1")),
            "model": watcher_config.get("eval_model", config.get("llm", {}).get("model", "qwen2.5:3b")),
            "api_key_env": config.get("llm", {}).get("api_key_env", ""),
            "max_tokens": 1024,
            "temperature": 0.1,
        })

        self._max_evals_per_min = watcher_config.get("max_evaluations_per_minute", 10)
        self._eval_count_this_min = 0
        self._eval_min_start = time.time()

        # Load persisted watchers
        self._load_watchers()

    def _load_watchers(self):
        """Load watchers from persistence file."""
        if self.watchers_file.exists():
            try:
                data = json.loads(self.watchers_file.read_text())
                for w_data in data:
                    watcher = Watcher.from_dict(w_data)
                    self.watchers[watcher.name] = watcher
                    logger.info(f"Loaded watcher: {watcher.name}")
            except (json.JSONDecodeError, KeyError) as e:
                logger.error(f"Failed to load watchers: {e}")

    def _save_watchers(self):
        """Save watchers to persistence file."""
        data = [w.to_dict() for w in self.watchers.values()]
        self.watchers_file.write_text(json.dumps(data, indent=2))

    async def create_watcher(
        self,
        name: str,
        topics: list[str],
        instruction: str,
        sample_interval_sec: int = 30,
        max_actions_per_hour: int = 10,
    ) -> str:
        """Create a new watcher."""
        if name in self.watchers:
            return f"Watcher '{name}' already exists. Remove it first."

        # Clamp sample interval
        sample_interval_sec = max(10, min(3600, sample_interval_sec))

        watcher = Watcher(
            name=name,
            topics=topics,
            instruction=instruction,
            sample_interval_sec=sample_interval_sec,
            max_actions_per_hour=max_actions_per_hour,
        )
        self.watchers[name] = watcher
        self._save_watchers()

        # Subscribe to all topics
        for topic in topics:
            self.zenoh.subscribe(topic)

        # Start evaluation loop if engine is running
        if self._running:
            self._start_watcher_task(name)

        logger.info(f"Created watcher '{name}': {instruction[:80]}")
        return (
            f"Watcher '{name}' created.\n"
            f"  Topics: {', '.join(topics)}\n"
            f"  Check interval: {sample_interval_sec}s\n"
            f"  Instruction: {instruction}\n"
            f"  Max actions/hour: {max_actions_per_hour}"
        )

    async def remove_watcher(self, name: str) -> str:
        """Remove a watcher."""
        if name not in self.watchers:
            return f"Watcher '{name}' not found."

        # Stop the task
        if name in self._tasks:
            self._tasks[name].cancel()
            del self._tasks[name]

        del self.watchers[name]
        self._save_watchers()
        logger.info(f"Removed watcher '{name}'")
        return f"Watcher '{name}' removed."

    async def pause_watcher(self, name: str, paused: bool) -> str:
        """Pause or resume a watcher."""
        if name not in self.watchers:
            return f"Watcher '{name}' not found."

        self.watchers[name].paused = paused
        self._save_watchers()
        state = "paused" if paused else "resumed"
        logger.info(f"Watcher '{name}' {state}")
        return f"Watcher '{name}' {state}."

    def describe_all(self) -> str:
        """Describe all watchers (for system prompt)."""
        if not self.watchers:
            return "No active watchers."

        lines = []
        for w in self.watchers.values():
            status = "PAUSED" if w.paused else "active"
            lines.append(f"### {w.name} [{status}]")
            lines.append(f"  Topics: {', '.join(w.topics)}")
            lines.append(f"  Interval: {w.sample_interval_sec}s")
            lines.append(f"  Instruction: {w.instruction}")
            if w.history:
                last = w.history[-1]
                ts = time.strftime("%H:%M:%S", time.localtime(last.timestamp))
                lines.append(f"  Last eval [{ts}]: {last.assessment}")
            lines.append(f"  Actions this hour: {w.actions_this_hour}/{w.max_actions_per_hour}")
            lines.append("")
        return "\n".join(lines)

    async def start(self):
        """Start the watcher engine - begins evaluation loops for all watchers."""
        self._running = True
        for name in self.watchers:
            self._start_watcher_task(name)
        logger.info(f"Watcher engine started ({len(self.watchers)} watchers)")

    async def stop(self):
        """Stop all watcher evaluation loops."""
        self._running = False
        for name, task in self._tasks.items():
            task.cancel()
        self._tasks.clear()
        logger.info("Watcher engine stopped")

    def _start_watcher_task(self, name: str):
        """Start the async evaluation loop for a watcher."""
        if name in self._tasks:
            self._tasks[name].cancel()
        self._tasks[name] = asyncio.create_task(self._evaluation_loop(name))

    async def _evaluation_loop(self, name: str):
        """Main evaluation loop for a single watcher."""
        while self._running and name in self.watchers:
            watcher = self.watchers[name]

            if not watcher.paused:
                await self._evaluate_watcher(watcher)

            await asyncio.sleep(watcher.sample_interval_sec)

    async def _evaluate_watcher(self, watcher: Watcher):
        """Run a single evaluation cycle for a watcher."""
        # Rate limit
        now = time.time()
        if now - self._eval_min_start > 60:
            self._eval_count_this_min = 0
            self._eval_min_start = now
        if self._eval_count_this_min >= self._max_evals_per_min:
            logger.debug(f"Rate limit reached, skipping eval for {watcher.name}")
            return
        self._eval_count_this_min += 1

        # 1. Collect latest data from subscribed topics
        data_snapshot = {}
        for topic in watcher.topics:
            sample = self.zenoh.get_latest(topic)
            if sample:
                decoded = self.zenoh.decode_sample(sample, topic)
                data_snapshot[topic] = decoded

        if not data_snapshot:
            # No data yet, skip
            return

        # 2. Build evaluation prompt
        rate_limited = not watcher.can_act()
        rate_note = "\nNOTE: Action rate limit reached for this hour. Observe only." if rate_limited else ""

        prompt = f"""You are monitoring the following data streams.

WATCHER: "{watcher.name}"
YOUR INSTRUCTION: {watcher.instruction}

CURRENT DATA:
{json.dumps(data_snapshot, indent=2, default=str)}

PREVIOUS EVALUATIONS:
{watcher.format_recent_history(5)}
{rate_note}
Based on this data and your instruction, decide:
1. Is any action needed right now? If yes, use your tools.
2. Brief assessment (1-2 sentences) for the log.

If no action needed, just respond with your assessment."""

        # 3. Determine available tools for this watcher
        watcher_tool_names = [
            "list_nodes", "start_node", "stop_node", "restart_node",
            "subscribe_topic", "remember", "publish_message",
        ]
        tool_defs = self.tools.subset_definitions(watcher_tool_names) if not rate_limited else None

        # 4. Call LLM (mini agent loop, max 5 turns)
        messages = [
            {"role": "system", "content": self.prompt_builder.build_watcher_context()},
            {"role": "user", "content": prompt},
        ]

        actions_taken = []
        for _mini_turn in range(5):
            try:
                response = await self._eval_llm.chat(messages, tools=tool_defs)
            except Exception as e:
                logger.error(f"Watcher '{watcher.name}' LLM call failed: {e}")
                break

            if not response.has_tool_calls:
                # Done - log the assessment
                watcher.history.append(WatcherEval(
                    timestamp=time.time(),
                    data=data_snapshot,
                    assessment=response.text or "No assessment",
                    actions_taken=actions_taken,
                ))
                # Keep history bounded
                if len(watcher.history) > 100:
                    watcher.history = watcher.history[-50:]
                break

            # Execute tool calls
            messages.append(response.raw_message)
            for tc in response.tool_calls:
                logger.info(f"Watcher '{watcher.name}' executing: {tc.name}({tc.arguments})")
                result = await self.tools.execute(tc.name, tc.arguments)
                messages.append({
                    "role": "tool",
                    "tool_call_id": tc.id,
                    "content": str(result),
                })
                actions_taken.append(f"{tc.name}({json.dumps(tc.arguments)})")
                watcher.record_action()

        if actions_taken:
            logger.info(f"Watcher '{watcher.name}' took actions: {actions_taken}")
