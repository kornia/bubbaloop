"""World model - tracks live system state from Zenoh for the system prompt."""

import json
import logging
import time
from dataclasses import dataclass, field

logger = logging.getLogger(__name__)


@dataclass
class NodeInfo:
    """Tracked state of a single node."""
    name: str
    status: str = "unknown"
    health: str = "unknown"
    version: str = ""
    description: str = ""
    node_type: str = ""
    installed: bool = False
    autostart: bool = False
    last_updated: float = 0.0
    machine_id: str = ""


class WorldModel:
    """Live system state assembled from Zenoh data."""

    def __init__(self, zenoh_bridge):
        self.zenoh = zenoh_bridge
        self.nodes: dict[str, NodeInfo] = {}
        self._last_node_refresh: float = 0.0
        self._daemon_healthy: bool | None = None

    async def refresh(self):
        """Refresh the world model from the daemon API."""
        await self._refresh_nodes()
        await self._check_daemon_health()

    async def _refresh_nodes(self):
        """Query daemon for current node list."""
        try:
            response = await self.zenoh.query_daemon("nodes")
            if response and not response.startswith(("No response", "Query failed")):
                data = json.loads(response)
                nodes_data = data if isinstance(data, list) else data.get("nodes", [])

                current_names = set()
                for node_data in nodes_data:
                    name = node_data.get("name", "")
                    if not name:
                        continue
                    current_names.add(name)

                    # Status can be string ("running") or int (2)
                    status_int_map = {
                        0: "unknown", 1: "stopped", 2: "running",
                        3: "failed", 4: "installing", 5: "building",
                        6: "not_installed",
                    }
                    health_int_map = {0: "unknown", 1: "healthy", 2: "unhealthy"}

                    status_val = node_data.get("status", "unknown")
                    if isinstance(status_val, int):
                        status_val = status_int_map.get(status_val, str(status_val))

                    health_val = node_data.get("health_status")
                    if health_val is None:
                        health_val = "unknown"
                    elif isinstance(health_val, int):
                        health_val = health_int_map.get(health_val, str(health_val))

                    self.nodes[name] = NodeInfo(
                        name=name,
                        status=str(status_val),
                        health=str(health_val),
                        version=node_data.get("version", ""),
                        description=node_data.get("description", ""),
                        node_type=node_data.get("node_type", ""),
                        installed=node_data.get("installed", False),
                        autostart=node_data.get("autostart_enabled", False),
                        last_updated=time.time(),
                        machine_id=node_data.get("machine_id", ""),
                    )

                # Remove nodes no longer reported
                for name in list(self.nodes.keys()):
                    if name not in current_names:
                        del self.nodes[name]

                self._last_node_refresh = time.time()
                logger.debug(f"World model refreshed: {len(self.nodes)} nodes")
        except json.JSONDecodeError:
            logger.warning("Failed to parse node list response")
        except Exception as e:
            logger.error(f"Failed to refresh nodes: {e}")

    async def _check_daemon_health(self):
        """Check if the daemon is responding."""
        try:
            response = await self.zenoh.query_daemon("health")
            self._daemon_healthy = response and not response.startswith(("No response", "Query failed"))
        except Exception:
            self._daemon_healthy = False

    def to_text(self) -> str:
        """Render the world model as text for the system prompt."""
        lines = []

        # Daemon status
        if self._daemon_healthy is True:
            lines.append("Daemon: healthy")
        elif self._daemon_healthy is False:
            lines.append("Daemon: NOT RESPONDING")
        else:
            lines.append("Daemon: unknown (not yet checked)")

        lines.append(f"Machine: {self.zenoh.machine_id} | Scope: {self.zenoh.scope}")
        lines.append("")

        if not self.nodes:
            lines.append("No nodes registered.")
            return "\n".join(lines)

        # Node summary
        running = sum(1 for n in self.nodes.values() if n.status == "running")
        stopped = sum(1 for n in self.nodes.values() if n.status == "stopped")
        failed = sum(1 for n in self.nodes.values() if n.status == "failed")
        unhealthy = sum(1 for n in self.nodes.values() if n.health == "unhealthy")

        lines.append(f"Nodes: {len(self.nodes)} total ({running} running, {stopped} stopped, {failed} failed)")
        if unhealthy:
            lines.append(f"WARNING: {unhealthy} node(s) unhealthy")
        lines.append("")

        # Node table
        lines.append(f"{'Name':<25} {'Status':<12} {'Health':<10} {'Type':<8} {'Description'}")
        lines.append("-" * 80)
        for node in sorted(self.nodes.values(), key=lambda n: n.name):
            lines.append(
                f"{node.name:<25} {node.status:<12} {node.health:<10} "
                f"{node.node_type:<8} {node.description[:40]}"
            )

        # Data topics with buffered data
        buffered = self.zenoh.get_all_buffered_topics()
        if buffered:
            lines.append("")
            lines.append("Active data topics (with buffered samples):")
            for topic, count in sorted(buffered.items()):
                lines.append(f"  {topic} ({count} samples)")

        return "\n".join(lines)

    def get_node(self, name: str) -> NodeInfo | None:
        """Get a specific node's info."""
        return self.nodes.get(name)

    def get_running_nodes(self) -> list[NodeInfo]:
        """Get all running nodes."""
        return [n for n in self.nodes.values() if n.status == "running"]
