"""Agent loop - the core reasoning engine. Message → LLM → tools → loop."""

import asyncio
import logging
import uuid
from typing import AsyncIterator

from .llm import LLMProvider, LLMResponse
from .tools import ToolRegistry
from .prompt_builder import PromptBuilder
from .memory import Memory

logger = logging.getLogger(__name__)


class BubbalooAgent:
    """The core agent: receives messages, reasons with tools, responds."""

    def __init__(
        self,
        llm: LLMProvider,
        tools: ToolRegistry,
        prompt_builder: PromptBuilder,
        memory: Memory,
        config: dict,
    ):
        self.llm = llm
        self.tools = tools
        self.prompt_builder = prompt_builder
        self.memory = memory
        self.max_turns = config.get("safety", {}).get("max_agent_turns", 20)

    async def handle_message(
        self,
        message: str,
        conversation_id: str | None = None,
    ) -> AsyncIterator[str]:
        """Process a user message through the agent loop.

        Yields intermediate status updates and final response.
        """
        if not conversation_id:
            conversation_id = str(uuid.uuid4())[:8]

        # 1. Build system prompt from runtime state
        system_prompt = self.prompt_builder.build()

        # 2. Load conversation history
        conv = self.memory.get_conversation(conversation_id)
        conv.append({"role": "user", "content": message})

        # 3. Build messages for LLM
        messages = [{"role": "system", "content": system_prompt}]
        # Only include recent conversation history to stay within context
        messages.extend(conv[-20:])

        # 4. Agent loop
        tool_defs = self.tools.all_definitions()

        for turn in range(self.max_turns):
            logger.info(f"Agent turn {turn + 1}/{self.max_turns}")

            # Call LLM
            response = await self.llm.chat(messages, tools=tool_defs if tool_defs else None)

            if not response.has_tool_calls:
                # Final response - LLM is done reasoning
                final_text = response.text or "(No response)"
                self.memory.append_to_conversation(
                    conversation_id,
                    {"role": "user", "content": message},
                )
                self.memory.append_to_conversation(
                    conversation_id,
                    {"role": "assistant", "content": final_text},
                )
                yield final_text
                return

            # Add assistant message with tool calls
            messages.append(response.raw_message)

            # Execute each tool call
            tool_names = []
            for tc in response.tool_calls:
                tool_names.append(tc.name)
                logger.info(f"Executing tool: {tc.name}({tc.arguments})")

                result = await self.tools.execute(tc.name, tc.arguments)

                messages.append({
                    "role": "tool",
                    "tool_call_id": tc.id,
                    "content": str(result),
                })

            # Stream intermediate status
            yield f"[used: {', '.join(tool_names)}]"

        # Hit max turns
        yield "[Reached max reasoning turns. Please try a simpler request.]"

    async def handle_message_sync(
        self,
        message: str,
        conversation_id: str | None = None,
    ) -> str:
        """Process a message and return the final response (non-streaming)."""
        result_parts = []
        async for part in self.handle_message(message, conversation_id):
            result_parts.append(part)
        final = result_parts[-1] if result_parts else "(No response)"

        # Trigger background reflection (don't block the response)
        asyncio.create_task(self._reflect(message, final))

        return final

    async def _reflect(self, user_message: str, agent_response: str):
        """Post-conversation reflection: let the LLM decide what to remember.

        Runs in background after each conversation. Uses a short LLM call
        to extract any learnings worth persisting to memory.
        """
        try:
            # Skip reflection for very short exchanges
            if len(user_message) < 10 and len(agent_response) < 50:
                return

            existing_memory = self.memory.get_all()
            reflection_prompt = f"""Review this interaction and decide if anything is worth remembering for future conversations.

USER: {user_message}

YOUR RESPONSE: {agent_response[:500]}

EXISTING MEMORY:
{existing_memory[:500] if existing_memory else "(empty)"}

If you learn something new and useful about the user (their name, preferences, expertise level, what they care about, their communication style) or about the system (a pattern, a recurring issue, a fix that worked), call the `remember` tool.

Categories: "user" for user info, "patterns" for system patterns, "preferences" for user workflow preferences, "issues" for problems/fixes.

If nothing new or useful was revealed, just respond with "Nothing to remember." and do NOT call any tools."""

            # Short reflection - max 3 turns, just enough to call remember if needed
            messages = [
                {"role": "system", "content": "You are reflecting on a conversation to extract learnings. Be selective - only remember genuinely useful insights, not trivial details."},
                {"role": "user", "content": reflection_prompt},
            ]

            # Only give it the remember tool
            remember_tool = None
            for td in self.tools.all_definitions():
                if td["function"]["name"] == "remember":
                    remember_tool = [td]
                    break

            for _ in range(3):
                response = await self.llm.chat(messages, tools=remember_tool)
                if not response.has_tool_calls:
                    break
                messages.append(response.raw_message)
                for tc in response.tool_calls:
                    result = await self.tools.execute(tc.name, tc.arguments)
                    messages.append({
                        "role": "tool",
                        "tool_call_id": tc.id,
                        "content": str(result),
                    })
                    logger.info(f"Reflection remembered: {tc.arguments}")

        except Exception as e:
            logger.debug(f"Reflection failed (non-critical): {e}")
