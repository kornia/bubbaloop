#!/usr/bin/env python3
"""Bubbaloop Agent - LLM-first autonomous system brain.

An autonomous agent that lets the LLM figure everything out.
No hardcoded rules, no AST condition compilers, no rigid policy schemas.
The LLM gets real-time system context, a set of tools, and reasons freely.
"""

import argparse
import asyncio
import logging
import os
import signal
import socket
import sys
import time
from datetime import datetime, timezone
from pathlib import Path

import yaml

logging.basicConfig(
    level=logging.INFO,
    format="%(asctime)s [%(levelname)s] %(name)s: %(message)s",
    datefmt="%Y-%m-%d %H:%M:%S",
)
logger = logging.getLogger("bubbaloop-agent")

# Base directory (where main.py lives)
BASE_DIR = Path(__file__).parent

# Agent data directory (persistent state)
DATA_DIR = Path.home() / ".bubbaloop" / "agent"


def load_config(config_path: Path) -> dict:
    """Load and validate configuration."""
    if not config_path.exists():
        logger.warning(f"Config not found at {config_path}, using defaults")
        return {}

    with open(config_path) as f:
        config = yaml.safe_load(f) or {}

    return config


async def main_async(config: dict, endpoint: str | None):
    """Async entry point - initializes all components and runs the agent."""
    # Ensure data directory exists
    DATA_DIR.mkdir(parents=True, exist_ok=True)

    # Override zenoh endpoint if provided via CLI
    if endpoint:
        config.setdefault("zenoh", {})["endpoint"] = endpoint

    # --- Initialize components ---

    # 1. Zenoh bridge
    from src.zenoh_bridge import ZenohBridge
    zenoh_bridge = ZenohBridge(config)
    zenoh_bridge.open()

    # 2. World model
    from src.world_model import WorldModel
    world_model = WorldModel(zenoh_bridge)
    await world_model.refresh()

    # 3. Memory
    from src.memory import Memory
    memory = Memory(DATA_DIR)

    # 4. Tool registry
    from src.tools.registry import ToolRegistry
    tool_registry = ToolRegistry()

    # 5. Data router
    from src.data_router import DataRouter
    data_router = DataRouter(zenoh_bridge, config, DATA_DIR)

    # 6. Watcher engine (needs prompt_builder, created below)
    # Placeholder - will set prompt_builder after creation
    from src.watcher_engine import WatcherEngine

    # 7. Prompt builder
    from src.prompt_builder import PromptBuilder

    # Create watcher engine first (prompt_builder will be set after)
    watcher_engine = WatcherEngine(
        zenoh_bridge=zenoh_bridge,
        tool_registry=tool_registry,
        prompt_builder=None,  # Set below
        config=config,
        data_dir=DATA_DIR,
    )

    prompt_builder = PromptBuilder(
        base_dir=BASE_DIR,
        world_model=world_model,
        watcher_engine=watcher_engine,
        data_router=data_router,
        tool_registry=tool_registry,
        memory=memory,
        config=config,
    )

    # Now set prompt_builder on watcher_engine
    watcher_engine.prompt_builder = prompt_builder

    # 8. Register all tools
    from src.tools.zenoh_tools import register_zenoh_tools
    from src.tools.node_tools import register_node_tools
    from src.tools.watcher_tools import register_watcher_tools
    from src.tools.data_tools import register_data_tools
    from src.tools.memory_tools import register_memory_tools
    from src.tools.system_tools import register_system_tools

    register_zenoh_tools(tool_registry, zenoh_bridge)
    register_node_tools(tool_registry, zenoh_bridge, world_model, config)
    register_watcher_tools(tool_registry, watcher_engine)
    register_data_tools(tool_registry, data_router)
    register_memory_tools(tool_registry, memory)
    register_system_tools(tool_registry, zenoh_bridge, world_model)

    logger.info(f"Registered {len(tool_registry.list_tools())} tools: {', '.join(tool_registry.list_tools())}")

    # 9. LLM provider
    from src.llm.openai_compat import OpenAICompatProvider
    llm = OpenAICompatProvider(config.get("llm", {}))

    # 10. Agent
    from src.agent import BubbalooAgent
    agent = BubbalooAgent(
        llm=llm,
        tools=tool_registry,
        prompt_builder=prompt_builder,
        memory=memory,
        config=config,
    )

    # 11. HTTP API
    from src.http_api import HttpApi
    http_api = HttpApi(agent, watcher_engine, world_model, data_router, config)

    # --- Start everything ---

    # Resume persisted captures
    data_router.start_persisted_captures()

    # Start watcher engine
    await watcher_engine.start()

    # Start HTTP API
    http_runner = await http_api.start()

    # Health heartbeat setup
    scope = os.environ.get("BUBBALOOP_SCOPE", "local")
    machine_id = os.environ.get("BUBBALOOP_MACHINE_ID", socket.gethostname())

    # Publish agent events via protobuf (if available)
    publish_topic = config.get("publish_topic", "bubbaloop-agent/events")
    full_topic = f"bubbaloop/{scope}/{machine_id}/{publish_topic}"

    try:
        import agent_pb2
        import header_pb2
        has_protos = True
        logger.info("Proto modules available - publishing structured events")
    except ImportError:
        has_protos = False
        logger.info("Proto modules not available (run 'pixi run build') - events will be plain text")

    logger.info("=" * 60)
    logger.info("Bubbaloop Agent started")
    logger.info(f"  Zenoh: {config.get('zenoh', {}).get('endpoint', 'tcp/127.0.0.1:7447')}")
    logger.info(f"  LLM: {config.get('llm', {}).get('base_url', 'http://localhost:11434/v1')} ({config.get('llm', {}).get('model', 'qwen2.5:7b')})")
    logger.info(f"  HTTP: {http_api.host}:{http_api.port}")
    logger.info(f"  Data dir: {DATA_DIR}")
    logger.info(f"  Watchers: {len(watcher_engine.watchers)} loaded")
    logger.info(f"  Captures: {len([c for c in data_router.captures.values() if c.active])} active")
    logger.info("=" * 60)

    # --- Main loop: health heartbeat + world model refresh ---
    sequence = 0
    heartbeat_interval = 5  # seconds
    refresh_interval = 30  # seconds
    last_refresh = 0

    try:
        while True:
            # Health heartbeat
            zenoh_bridge.publish_health("bubbaloop-agent")

            # Periodic world model refresh
            now = time.time()
            if now - last_refresh > refresh_interval:
                await world_model.refresh()
                last_refresh = now

            # Publish agent event (heartbeat)
            if has_protos:
                now_ns = int(datetime.now(timezone.utc).timestamp() * 1e9)
                event = agent_pb2.AgentEvent()
                event.header.CopyFrom(header_pb2.Header(
                    acq_time=now_ns,
                    pub_time=now_ns,
                    sequence=sequence,
                    frame_id="bubbaloop-agent",
                    machine_id=machine_id,
                    scope=scope,
                ))
                event.event_type = agent_pb2.EVENT_TYPE_CHAT
                event.summary = "heartbeat"
                try:
                    zenoh_bridge.session.put(full_topic, event.SerializeToString())
                except Exception:
                    pass

            sequence += 1
            await asyncio.sleep(heartbeat_interval)

    except asyncio.CancelledError:
        pass
    finally:
        logger.info("Shutting down...")
        await watcher_engine.stop()
        await http_runner.cleanup()
        zenoh_bridge.close()
        logger.info("Bubbaloop Agent stopped")


def main():
    parser = argparse.ArgumentParser(description="Bubbaloop Agent - LLM-first autonomous system brain")
    parser.add_argument(
        "-c", "--config",
        type=Path,
        default=BASE_DIR / "config.yaml",
        help="Path to configuration file",
    )
    parser.add_argument(
        "-e", "--endpoint",
        type=str,
        default=None,
        help="Zenoh endpoint (e.g., tcp/127.0.0.1:7447)",
    )
    parser.add_argument(
        "-v", "--verbose",
        action="store_true",
        help="Enable debug logging",
    )
    args = parser.parse_args()

    if args.verbose:
        logging.getLogger().setLevel(logging.DEBUG)

    config = load_config(args.config)

    # Setup shutdown
    loop = asyncio.new_event_loop()
    asyncio.set_event_loop(loop)

    def signal_handler(signum, frame):
        logger.info("Shutdown signal received")
        for task in asyncio.all_tasks(loop):
            task.cancel()

    signal.signal(signal.SIGINT, signal_handler)
    signal.signal(signal.SIGTERM, signal_handler)

    try:
        loop.run_until_complete(main_async(config, args.endpoint))
    except KeyboardInterrupt:
        pass
    finally:
        loop.close()


if __name__ == "__main__":
    main()
