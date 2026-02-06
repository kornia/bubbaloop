"""System tools - health, world state, machine info."""

import logging
import os
import platform
import shutil

from .registry import ToolRegistry, ToolDefinition

logger = logging.getLogger(__name__)


def register_system_tools(registry: ToolRegistry, zenoh_bridge, world_model):
    """Register system diagnostic tools."""

    async def system_health() -> str:
        """Get overall system health."""
        await world_model.refresh()

        lines = ["## System Health"]
        lines.append(world_model.to_text())

        # Add active topic data summary
        buffered = zenoh_bridge.get_all_buffered_topics()
        if buffered:
            lines.append(f"\nActive data streams: {len(buffered)}")

        return "\n".join(lines)

    async def get_world_state() -> str:
        """Get comprehensive world state."""
        await world_model.refresh()
        return world_model.to_text()

    async def get_machine_info() -> str:
        """Get machine hardware/OS information."""
        lines = ["## Machine Information"]
        lines.append(f"Hostname: {platform.node()}")
        lines.append(f"OS: {platform.system()} {platform.release()}")
        lines.append(f"Architecture: {platform.machine()}")
        lines.append(f"Python: {platform.python_version()}")

        # CPU info
        cpu_count = os.cpu_count()
        lines.append(f"CPU cores: {cpu_count}")

        # Try to get CPU model
        try:
            with open("/proc/cpuinfo") as f:
                for line in f:
                    if line.startswith("model name"):
                        lines.append(f"CPU: {line.split(':')[1].strip()}")
                        break
        except (FileNotFoundError, PermissionError):
            pass

        # Memory
        try:
            with open("/proc/meminfo") as f:
                for line in f:
                    if line.startswith("MemTotal"):
                        mem_kb = int(line.split()[1])
                        lines.append(f"Memory: {mem_kb / 1024 / 1024:.1f} GB")
                        break
        except (FileNotFoundError, PermissionError):
            pass

        # Disk
        try:
            usage = shutil.disk_usage("/")
            total_gb = usage.total / (1024**3)
            used_gb = usage.used / (1024**3)
            free_gb = usage.free / (1024**3)
            pct = (usage.used / usage.total) * 100
            lines.append(f"Disk: {used_gb:.1f}/{total_gb:.1f} GB ({pct:.1f}% used, {free_gb:.1f} GB free)")
        except Exception:
            pass

        # GPU (Jetson / NVIDIA)
        try:
            with open("/proc/device-tree/model") as f:
                model = f.read().strip().rstrip("\x00")
                lines.append(f"Device: {model}")
        except (FileNotFoundError, PermissionError):
            pass

        try:
            import subprocess
            result = subprocess.run(
                ["nvidia-smi", "--query-gpu=name,memory.total,memory.used,temperature.gpu",
                 "--format=csv,noheader,nounits"],
                capture_output=True, text=True, timeout=5,
            )
            if result.returncode == 0:
                for line in result.stdout.strip().split("\n"):
                    parts = [p.strip() for p in line.split(",")]
                    if len(parts) >= 4:
                        lines.append(f"GPU: {parts[0]} ({parts[2]}/{parts[1]} MB, {parts[3]}°C)")
        except (FileNotFoundError, subprocess.TimeoutExpired):
            pass

        return "\n".join(lines)

    registry.register(ToolDefinition(
        name="system_health",
        description="Get overall system health: daemon status, node health, resource overview.",
        parameters={"type": "object", "properties": {}},
        handler=system_health,
        skill="system",
    ))

    registry.register(ToolDefinition(
        name="get_world_state",
        description="Get comprehensive view of current system: nodes, topics, watchers, captures.",
        parameters={"type": "object", "properties": {}},
        handler=get_world_state,
        skill="system",
    ))

    registry.register(ToolDefinition(
        name="get_machine_info",
        description="Get machine info: hostname, OS, CPU, memory, disk, GPU.",
        parameters={"type": "object", "properties": {}},
        handler=get_machine_info,
        skill="system",
    ))
