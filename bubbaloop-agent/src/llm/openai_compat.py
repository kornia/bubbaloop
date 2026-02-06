"""OpenAI-compatible LLM provider. Works with OpenAI, Ollama, vLLM, LiteLLM, etc."""

import json
import logging
import os

from openai import AsyncOpenAI

from .provider import LLMProvider, LLMResponse, ToolCall

logger = logging.getLogger(__name__)


class OpenAICompatProvider:
    """OpenAI-compatible LLM provider."""

    def __init__(self, config: dict):
        self.model = config.get("model", "qwen2.5:7b")
        self.base_url = config.get("base_url", "http://localhost:11434/v1")
        self.max_tokens = config.get("max_tokens", 4096)
        self.temperature = config.get("temperature", 0.1)

        # API key from env var or config
        api_key_env = config.get("api_key_env", "")
        api_key = os.environ.get(api_key_env, "no-key-needed") if api_key_env else "no-key-needed"

        self.client = AsyncOpenAI(
            base_url=self.base_url,
            api_key=api_key,
        )
        logger.info(f"LLM provider: {self.base_url} model={self.model}")

    async def chat(
        self,
        messages: list[dict],
        tools: list[dict] | None = None,
        max_tokens: int | None = None,
        temperature: float | None = None,
    ) -> LLMResponse:
        """Send a chat request to the LLM."""
        kwargs = {
            "model": self.model,
            "messages": messages,
            "max_tokens": max_tokens or self.max_tokens,
            "temperature": temperature if temperature is not None else self.temperature,
        }

        if tools:
            kwargs["tools"] = tools
            kwargs["tool_choice"] = "auto"

        try:
            response = await self.client.chat.completions.create(**kwargs)
        except Exception as e:
            logger.error(f"LLM call failed: {e}")
            return LLMResponse(text=f"LLM error: {e}")

        choice = response.choices[0]
        message = choice.message

        # Handle qwen3 thinking mode: content may be empty while reasoning has the response
        text_content = message.content or ""
        if not text_content and hasattr(message, "reasoning") and message.reasoning:
            # Extract the actual answer from reasoning if model didn't produce content
            text_content = message.reasoning

        # Parse tool calls
        tool_calls = []
        if message.tool_calls:
            for tc in message.tool_calls:
                try:
                    args = json.loads(tc.function.arguments)
                except (json.JSONDecodeError, TypeError):
                    args = {"raw": tc.function.arguments}

                tool_calls.append(ToolCall(
                    id=tc.id,
                    name=tc.function.name,
                    arguments=args,
                ))

        # Build usage info
        usage = {}
        if response.usage:
            usage = {
                "prompt_tokens": response.usage.prompt_tokens,
                "completion_tokens": response.usage.completion_tokens,
                "total_tokens": response.usage.total_tokens,
            }

        return LLMResponse(
            text=text_content,
            tool_calls=tool_calls,
            raw_message={
                "role": "assistant",
                "content": message.content,
                "tool_calls": [
                    {
                        "id": tc.id,
                        "type": "function",
                        "function": {
                            "name": tc.function.name,
                            "arguments": tc.function.arguments,
                        },
                    }
                    for tc in (message.tool_calls or [])
                ] or None,
            },
            usage=usage,
        )
