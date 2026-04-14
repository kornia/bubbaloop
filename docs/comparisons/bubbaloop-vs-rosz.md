---
description: "Bubbaloop vs ros-z comparison. Two Zenoh-native Rust frameworks: Hardware AI agent runtime vs pure-Rust ROS 2 reimplementation by ZettaScale."
---

# Bubbaloop vs ros-z

Both Bubbaloop and ros-z are Rust-native and built on Zenoh. They share the same transport layer but serve completely different purposes.

---

## At a Glance

| | Bubbaloop | ros-z |
|---|---|---|
| **What it is** | Hardware AI agent runtime | Pure-Rust ROS 2 reimplementation on Zenoh |
| **By** | [Kornia](https://github.com/kornia) | [ZettaScale](https://github.com/ZettaScaleLabs) (the Zenoh team) |
| **Transport** | Zenoh | Zenoh (same protocol) |
| **Goal** | AI agents that control hardware | ROS 2 compatibility without DDS/C++ |
| **Serialization** | Protobuf | CDR (ROS 2 native) + Protobuf |
| **Languages** | Rust, Python, TypeScript | Rust, Python, Go |
| **AI agents** | Built-in (multi-agent, MCP, 4-tier memory) | None |
| **ROS 2 compat** | No (different API) | Yes (wire-compatible, bridge to existing nodes) |
| **Maturity** | Production-focused | Experimental (v0.1.0-rc13) |
| **GitHub** | ~600 stars | ~160 stars |

---

## Different Problems, Same Transport

**Bubbaloop** answers: *"How do I make AI agents orchestrate cameras, sensors, and IoT devices?"*

- Application-layer runtime: daemon, agents, MCP server, telemetry
- Nodes are independent services that publish to Zenoh topics
- Built-in multi-agent reasoning with memory and tool dispatch

**ros-z** answers: *"How do I get ROS 2 semantics without the DDS/C++ baggage?"*

- Middleware-layer reimplementation: topics, services, actions — all in Rust
- Wire-compatible with existing ROS 2 nodes via bridge
- Eliminates DDS discovery complexity, multicast issues, C++ runtime

---

## The ZettaScale Zenoh Ecosystem

ros-z exists alongside two other Zenoh-for-ROS approaches. Understanding all three clarifies where each fits:

| Layer | Project | What it does |
|---|---|---|
| **Bridge** | `zenoh-bridge-ros2dds` | Tunnels existing DDS traffic over Zenoh. No code changes. |
| **RMW** | `rmw_zenoh_cpp` (ros2/rmw_zenoh) | Official ROS 2 middleware plugin. Drop-in DDS replacement. Tier-1 in Kilted. |
| **Native** | `ros-z` | Pure Rust ROS 2 stack on Zenoh. No C++ at all. New API. |
| **Application** | **Bubbaloop** | AI agent runtime on Zenoh. Not ROS 2. Different abstraction. |

Bubbaloop and ros-z occupy different layers. ros-z replaces the ROS 2 communication stack. Bubbaloop replaces the entire approach with an agent-first model.

---

## Feature Comparison

### Communication

Both use Zenoh, so they share the same transport characteristics: microsecond latency, shared memory, TCP/UDP/QUIC, automatic peer discovery, works across NAT.

| | Bubbaloop | ros-z |
|---|---|---|
| **Topic model** | Zenoh key expressions | ROS 2 topics mapped to Zenoh key expressions |
| **Services** | Zenoh queryables | ROS 2 services on Zenoh |
| **Actions** | Not applicable (agent-based) | ROS 2 actions on Zenoh |
| **Schema** | Protobuf FileDescriptorSet | CDR + Protobuf |
| **Interop** | Other Zenoh clients | Existing ROS 2 nodes via bridge |

### What ros-z Is Still Missing

ros-z is experimental (v0.1.0-rc13). Features not yet implemented:

- Parameter server
- Lifecycle nodes
- tf2 (transform library)
- Simulation time
- rosbag2 recording
- `wait_for_service` / `wait_for_action`
- Component nodes

These are core ROS 2 features that many robotics applications depend on. Bubbaloop doesn't need them — it's a different model entirely.

---

## AI Integration

**Bubbaloop:**
- Native multi-agent runtime with Soul, 4-tier memory, adaptive heartbeat
- 42 MCP tools for hardware control and data queries
- Works with Claude, Ollama, and other LLM providers
- Agents reason about hardware, remember across sessions, dispatch tools

**ros-z:**
- No AI integration. It's a communication middleware.
- Any LLM integration would be built on top as a separate node (same as ROS 2)

---

## When to Use Bubbaloop

- You want **AI agents managing hardware** — not just data transport
- You're building camera/sensor/IoT systems, not traditional robots
- You don't need ROS 2 package compatibility
- You want a single binary with built-in dashboard, telemetry, and MCP

## When to Use ros-z

- You want **ROS 2 semantics in pure Rust** — topics, services, actions
- You need to **interoperate with existing ROS 2 nodes** but want to ditch DDS
- You're building a Rust-native robot and want type-safe ROS 2 communication
- You're comfortable with experimental software and can work around missing features

## When to Use rmw_zenoh Instead

If you want Zenoh transport but need the full ROS 2 ecosystem (Nav2, MoveIt2, Gazebo), use the official `rmw_zenoh_cpp` — it's a drop-in middleware replacement that works with all existing ROS 2 packages. ros-z is for teams who want to go further and eliminate the C++ stack entirely.

---

## Can They Work Together?

Yes, naturally. Both speak Zenoh natively. A ros-z node publishing to a Zenoh topic is immediately visible to Bubbaloop's Zenoh session. No bridge needed — they're on the same bus.

A practical architecture: ros-z handles low-level robot communication (motor control, sensor drivers) while Bubbaloop agents handle high-level reasoning (what to do, when, and why).

---

## Summary

**Bubbaloop** is an AI agent platform that uses Zenoh as its data plane. It doesn't try to be ROS 2.

**ros-z** is a pure-Rust ROS 2 reimplementation that uses Zenoh as its transport. It doesn't try to be an AI platform.

They share the same wire protocol and can coexist on the same Zenoh network. One handles the "what should the robot do?" question (Bubbaloop), the other handles the "how do messages flow between robot subsystems?" question (ros-z).
