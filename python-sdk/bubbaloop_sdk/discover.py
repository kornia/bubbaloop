"""Node discovery via health heartbeats.

Every node publishes ``"ok"`` to ``bubbaloop/{scope}/{machine_id}/{node_name}/health``
every 5 seconds. Subscribing to ``bubbaloop/**/health`` for slightly longer than
one heartbeat interval collects all live nodes.

Example::

    import zenoh
    from bubbaloop_sdk import discover_nodes

    session = zenoh.open(zenoh.Config())
    nodes = discover_nodes(session)
    for node in nodes:
        print(node.base_topic)   # bubbaloop/local/nvidia_orin00/tapo_terrace
        print(node.schema_topic) # bubbaloop/local/nvidia_orin00/tapo_terrace/schema
"""

from __future__ import annotations

import threading
import time
from dataclasses import dataclass

import zenoh


@dataclass(frozen=True)
class NodeInfo:
    """Identity of a live bubbaloop node discovered via health heartbeat."""

    scope: str
    machine_id: str
    node_name: str

    @property
    def base_topic(self) -> str:
        return f"bubbaloop/{self.scope}/{self.machine_id}/{self.node_name}"

    @property
    def schema_topic(self) -> str:
        return f"{self.base_topic}/schema"

    def topic(self, resource: str) -> str:
        """Build a full topic for a node resource, e.g. node.topic('compressed')."""
        return f"{self.base_topic}/{resource}"

    def __str__(self) -> str:
        return self.base_topic


def discover_nodes(session: zenoh.Session, timeout: float = 6.5) -> list[NodeInfo]:
    """Return all live nodes by collecting health heartbeats.

    Args:
        session: Open Zenoh session.
        timeout: How long to listen (seconds). Default 6.5s covers one full
                 heartbeat cycle (nodes publish every 5s) plus margin.

    Returns:
        Deduplicated list of :class:`NodeInfo` for every node that published
        a health beat during the window.
    """
    seen: set[NodeInfo] = set()
    lock = threading.Lock()

    def _on_sample(sample: zenoh.Sample) -> None:
        node = _parse_health_key(str(sample.key_expr))
        if node is not None:
            with lock:
                seen.add(node)

    sub = session.declare_subscriber("bubbaloop/**/health", _on_sample)
    try:
        time.sleep(timeout)
    finally:
        sub.undeclare()

    return sorted(seen, key=lambda n: n.base_topic)


def _parse_health_key(key: str) -> NodeInfo | None:
    """Parse ``bubbaloop/{scope}/{machine_id}/{node_name}/health`` → NodeInfo."""
    parts = key.split("/")
    # expected: ["bubbaloop", scope, machine_id, node_name, "health"]
    if len(parts) != 5 or parts[0] != "bubbaloop" or parts[4] != "health":
        return None
    _, scope, machine_id, node_name, _ = parts
    return NodeInfo(scope=scope, machine_id=machine_id, node_name=node_name)
