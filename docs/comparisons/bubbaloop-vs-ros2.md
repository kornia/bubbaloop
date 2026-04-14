---
description: "Bubbaloop vs ROS2 comparison. When to use a single-binary Hardware AI agent vs the full ROS2 robotics middleware for edge AI and IoT projects."
---

# Bubbaloop vs ROS2

Both Bubbaloop and ROS2 connect sensors, actuators, and compute on robots and edge devices. They solve different problems at different layers of the stack.

---

## At a Glance

| | Bubbaloop | ROS2 (Jazzy/Kilted) |
|---|---|---|
| **What it is** | Hardware AI agent with built-in runtime | Robotics middleware framework |
| **Architecture** | Single binary (~13 MB) | 2,300+ packages, 3-6 GB installed |
| **Transport** | Zenoh (native) | DDS (Fast DDS default); Zenoh experimental |
| **Languages** | Rust, Python, TypeScript | C++, Python (official); Rust community |
| **AI agents** | Built-in multi-agent runtime, MCP server, 4-tier memory | None native; community bridges (ROSA, RAI) |
| **MCP support** | Native (42 tools, 3-tier RBAC) | Community projects only |
| **Install** | `curl \| bash` — 30 seconds | APT repo + rosdep + colcon — hours |
| **Learning curve** | Minutes to first node | Days to weeks |
| **Ecosystem** | Small, focused | Massive (Nav2, MoveIt2, Gazebo, Isaac ROS) |
| **Jetson support** | Native ARM64 binary | Prebuilt debs (Humble); Docker for newer |
| **Real-time** | No | Possible with RTOS patches |
| **Simulation** | No | Gazebo/Ignition, deep integration |

---

## When to Use Bubbaloop

- You want **AI agents that talk to hardware** — not just data pipelines
- Your project is a focused edge device (Jetson, Raspberry Pi) running cameras and sensors
- You want a single binary you can `scp` and run — no workspace, no sourcing, no DDS config
- You need an **MCP server** for Claude Code or other AI tools to control your hardware
- Your "robot" is really an IoT fleet (cameras, sensors, weather stations) that needs monitoring and orchestration
- You want agent memory, personality, and adaptive behavior out of the box

## When to Use ROS2

- You're building a **multi-subsystem robot** (arms + lidar + navigation + manipulation)
- You need **Nav2**, **MoveIt2**, or the **Isaac ROS** GPU perception stack
- You need **Gazebo simulation** for testing before deploying to hardware
- You need **hardware drivers** — most sensor vendors ship ROS2 packages
- Your team already knows ROS and benefits from the ecosystem
- You need **real-time guarantees** for industrial control
- You need the 2,300+ package ecosystem

---

## Architecture

**Bubbaloop** is an application-layer runtime. One binary includes: daemon, multi-agent runtime, MCP server, node manager, and telemetry watchdog. Nodes are separate binaries that communicate over Zenoh.

**ROS2** is a middleware layer. It provides the communication infrastructure (DDS) and tools, but you assemble your own application from packages. There's no built-in agent, no MCP server, no telemetry watchdog.

---

## Messaging

Both use pub/sub with topics, but the implementations differ:

| | Bubbaloop | ROS2 |
|---|---|---|
| **Protocol** | Zenoh | DDS (multiple vendors) |
| **Discovery** | Zenoh scouting (instant) | DDS multicast (seconds, often blocked by firewalls) |
| **Serialization** | Protobuf | CDR (DDS native) |
| **Shared memory** | Zenoh SHM (all languages) | DDS SHM (C++ only) |
| **Cross-NAT** | Works out of the box | Requires manual DDS peer config |
| **QoS** | Best-effort + reliable | Full DDS QoS profiles |

Notably, ROS2 Kilted (2025) added **experimental Zenoh support** via `rmw_zenoh` — the same protocol Bubbaloop uses natively.

---

## AI and LLM Integration

This is the biggest differentiator.

**Bubbaloop** has a **native multi-agent runtime**:
- Each agent has a Soul (identity + capabilities), 4-tier memory, and adaptive heartbeat
- 42 MCP tools for node lifecycle, data queries, beliefs, missions, constraints
- Agents reason about hardware state, dispatch tools, and remember across sessions
- Works with Claude, Ollama, and other LLM providers

**ROS2** has **no built-in AI support**. Community projects exist:
- [ROSA](https://github.com/nasa-jpl/rosa) (NASA JPL) — LangChain-based agent for ROS inspection
- [RAI](https://github.com/RobotecAI/rai) (RobotecAI) — vendor-agnostic agent framework
- Community MCP servers — thin bridges, not deep integrations

---

## Installation and Setup

**Bubbaloop:**
```bash
curl -sSL https://github.com/kornia/bubbaloop/releases/latest/download/install.sh | bash
bubbaloop up
bubbaloop agent chat "What sensors do I have?"
```

**ROS2:**
```bash
# Add GPG key and apt repository
sudo apt install software-properties-common
sudo add-apt-repository universe
# ... (multiple setup steps)
sudo apt install ros-jazzy-desktop  # 3-6 GB download
source /opt/ros/jazzy/setup.bash
# Create workspace, build with colcon, source overlay...
```

---

## Can They Work Together?

Yes. Since ROS2 Kilted supports Zenoh as an experimental RMW, and Bubbaloop uses Zenoh natively, there's a natural bridge point. You could run ROS2 navigation with `rmw_zenoh` and have Bubbaloop agents interact with the same Zenoh network.

---

## Summary

**Bubbaloop** is for teams who want AI-powered hardware orchestration in a single binary. It's opinionated: Zenoh for messaging, built-in agents with memory, MCP for AI tool integration.

**ROS2** is for teams building complex robots who need the ecosystem: navigation, manipulation, simulation, and thousands of community packages.

They're complementary, not competing — Bubbaloop excels at the AI agent layer, ROS2 excels at the robotics middleware layer.
