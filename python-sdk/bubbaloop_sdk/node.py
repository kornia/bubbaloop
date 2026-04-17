"""run_node() — SDK entry point for Python nodes.

Mirrors the Rust SDK's run_node::<N>() function. Handles CLI args,
config loading, Zenoh connection, health heartbeat, and shutdown.

Usage::

    # main.py
    from bubbaloop_sdk import run_node
    from my_node import MyNode

    if __name__ == "__main__":
        run_node(MyNode)
"""

import argparse
import logging

import yaml

from .context import NodeContext
from .health import start_health_heartbeat

logging.basicConfig(
    level=logging.INFO,
    format="%(asctime)s [%(levelname)s] %(name)s — %(message)s",
    datefmt="%Y-%m-%dT%H:%M:%S",
)


def run_node(node_class) -> None:
    """Run a node class using the SDK runtime.

    ``node_class`` must implement:
    - ``name: str`` — class attribute (node type name)
    - ``__init__(ctx, config)`` — receives NodeContext and config dict
    - ``run()`` — main loop, should check ``ctx.is_shutdown()`` or ``ctx.wait_shutdown()``
    """
    parser = argparse.ArgumentParser(description=f"Bubbaloop node: {node_class.name}")
    parser.add_argument("-c", "--config", default="config.yaml", help="Config file path")
    parser.add_argument("-e", "--endpoint", default=None, help="Zenoh endpoint")
    args = parser.parse_args()

    with open(args.config) as f:
        config = yaml.safe_load(f) or {}

    instance_name = config.get("name", node_class.name)

    log = logging.getLogger(instance_name)
    log.info("Starting (type=%s, config=%s)", node_class.name, args.config)

    ctx = NodeContext.connect(endpoint=args.endpoint, instance_name=instance_name)

    heartbeat = None
    try:
        node = node_class(ctx, config)
        log.info("Initialized. Running…")
        heartbeat = start_health_heartbeat(ctx.session, ctx.machine_id, instance_name, ctx._shutdown)
        log.info("Health heartbeat: bubbaloop/global/%s/%s/health", ctx.machine_id, instance_name)
        node.run()
    except KeyboardInterrupt:
        pass
    finally:
        ctx._shutdown.set()  # stop heartbeat before closing session
        if heartbeat is not None:
            heartbeat.join(timeout=1.0)
        ctx.close()
        log.info("Shutdown complete")
