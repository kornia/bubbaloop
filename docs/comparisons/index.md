---
description: "Compare Bubbaloop with ROS2, ROS1, Dora-rs, and other robotics frameworks. Find the right tool for your edge AI and IoT project."
---

# Comparisons

How does Bubbaloop compare to other robotics and edge AI frameworks?

| Framework | Best For | Transport | AI Agents | Install Size |
|-----------|----------|-----------|-----------|-------------|
| **[Bubbaloop](https://github.com/kornia/bubbaloop)** | AI-powered hardware orchestration | Zenoh | Built-in (multi-agent, MCP, memory) | ~13 MB |
| **[ROS2](bubbaloop-vs-ros2.md)** | Complex robots (navigation, manipulation) | DDS | Community bridges | 3-6 GB |
| **[ROS1](bubbaloop-vs-ros1.md)** | Legacy robotics (EOL May 2025) | TCPROS | None | 2-4 GB |
| **[Dora-rs](bubbaloop-vs-dora.md)** | ML data pipelines | Apache Arrow | Experimental | Single binary |
| **[ros-z](bubbaloop-vs-rosz.md)** | Pure-Rust ROS 2 on Zenoh | Zenoh | None | Single binary |

## Detailed Comparisons

- [Bubbaloop vs ROS2](bubbaloop-vs-ros2.md) — Full middleware vs focused AI agent runtime
- [Bubbaloop vs ROS1](bubbaloop-vs-ros1.md) — Migrating from end-of-life ROS1
- [Bubbaloop vs Dora-rs](bubbaloop-vs-dora.md) — Two Rust-based frameworks, different goals
- [Bubbaloop vs ros-z](bubbaloop-vs-rosz.md) — Two Zenoh-native Rust frameworks, different layers
