---
description: "Bubbaloop vs Dora-rs comparison. Two Rust-based robotics frameworks with different approaches: Hardware AI agent vs dataflow middleware."
---

# Bubbaloop vs Dora-rs

Bubbaloop and Dora are both Rust-based frameworks for robotics and edge AI. They share DNA (Rust internals, ARM64 focus, simpler-than-ROS2 philosophy) but solve different problems.

---

## At a Glance

| | Bubbaloop | Dora-rs |
|---|---|---|
| **What it is** | Hardware AI agent runtime | Dataflow-oriented robotics middleware |
| **Core model** | Daemon + nodes + AI agents | Declarative dataflow graph (YAML pipeline) |
| **Transport** | Zenoh | Apache Arrow + shared memory + TCP |
| **Serialization** | Protobuf | Apache Arrow (zero-copy cross-language) |
| **Languages** | Rust, Python, TypeScript | Rust, Python, C, C++, WebAssembly |
| **AI agents** | Built-in multi-agent runtime, 42 MCP tools | No agent runtime; dora-llm is experimental |
| **Install** | `curl \| bash` (~13 MB) | `pip install dora-rs-cli` or `cargo install` |
| **Maturity** | Production-focused | Pre-1.0 (v0.5.0, March 2026) |
| **GitHub stars** | ~600 | ~3,200 |
| **Ecosystem** | Small, edge AI focused | Small, ML pipeline focused |
| **Jetson-aware** | Yes (CUDA, JetPack) | ARM64 supported, no Jetson-specific features |
| **ROS2 bridge** | Via Zenoh (rmw_zenoh) | Native dora-ros2-bridge |

---

## Different Design Goals

**Bubbaloop** answers: *"How do I make AI agents that understand and control my hardware?"*

- Single binary: daemon, agent runtime, MCP server, node manager, telemetry
- Agents have Soul (identity), 4-tier memory, and adaptive heartbeat
- Designed for always-on edge deployments with systemd integration
- Built-in dashboard for real-time visualization

**Dora** answers: *"How do I build fast ML data pipelines across languages with minimal overhead?"*

- Declarative YAML graphs that wire node inputs to outputs
- Apache Arrow for zero-copy cross-language data sharing
- Coordinator + daemon model for distributed graph execution
- Optimized for throughput on large payloads (10-17x faster than ROS2 for 40 MB Python transfers)

---

## Messaging

| | Bubbaloop | Dora |
|---|---|---|
| **Protocol** | Zenoh pub/sub + queryables | Apache Arrow over shared memory + TCP |
| **Discovery** | Zenoh scouting (automatic) | Coordinator manages graph topology |
| **Serialization** | Protobuf (schema queryable) | Apache Arrow (zero-copy) |
| **Cross-language zero-copy** | Via Zenoh SHM | Via Apache Arrow (primary feature) |
| **Cross-machine** | Zenoh router peering | TCP between coordinators |
| **Dynamic topology** | Nodes join/leave at runtime | Graph defined statically in YAML |

Dora's Apache Arrow approach shines for **large tensor transfers** between Python and Rust nodes (ML inference pipelines). Bubbaloop's Zenoh approach shines for **dynamic, distributed systems** where nodes come and go.

---

## AI Integration

This is the sharpest difference.

**Bubbaloop:**
- Native multi-agent runtime with per-agent Soul, memory, and heartbeat
- 42 MCP tools for hardware control, data queries, beliefs, missions
- Works with Claude (API + OAuth), Ollama (local), and other providers
- Agents remember across sessions, adapt behavior, and reason about hardware state

**Dora:**
- `dora-llm` — experimental repo for testing LLM nodes
- Node-hub includes some ML nodes (YOLO, MediaPipe, VLM, STT/TTS)
- Qwen2.5 and Qwen2.5-VL nodes added Feb 2025
- No agent runtime, no tool-calling, no memory system
- GSoC 2026 proposal lists "AI agent framework bridge" as a future goal

Dora can *host* an LLM as a node in a pipeline. Bubbaloop *is* an AI agent platform that happens to manage hardware.

---

## Node Development

**Bubbaloop** — SDK-based, service-oriented:
```rust
use bubbaloop_node::{Node, NodeContext, run_node};

struct MySensor;

#[async_trait]
impl Node for MySensor {
    async fn init(ctx: &NodeContext) -> anyhow::Result<Self> { Ok(Self) }
    async fn run(self, ctx: NodeContext) -> anyhow::Result<()> {
        let publisher = ctx.publisher_json("data").await?;
        // ... publish data, handle shutdown
        Ok(())
    }
}
```

**Dora** — Dataflow graph with YAML wiring:
```yaml
nodes:
  - id: camera
    operator:
      python: camera_node.py
    outputs:
      - frames

  - id: detector
    operator:
      python: yolo_node.py
    inputs:
      frames: camera/frames
    outputs:
      - detections
```

Dora's graph model is more declarative — you define the pipeline topology in YAML. Bubbaloop's model is more service-oriented — nodes are independent services that publish to topics.

---

## When to Use Bubbaloop

- You want **AI agents controlling hardware** with memory and reasoning
- You're building an always-on edge deployment (cameras, sensors, IoT)
- You need **MCP integration** with Claude Code or other AI tools
- You want a **single binary runtime** with built-in dashboard and telemetry
- Dynamic topology — nodes joining and leaving at runtime

## When to Use Dora

- You're building an **ML inference pipeline** that chains perception models
- You need **zero-copy cross-language data** (large tensors between Python and Rust)
- You want a **declarative dataflow graph** model (define topology in YAML)
- You need **maximum throughput** for large payloads between nodes
- You want a **lighter alternative to ROS2** for data pipelines

---

## Can They Complement Each Other?

Potentially. Dora excels at high-throughput ML pipelines (camera → detector → tracker → planner). Bubbaloop excels at the agent layer on top (reasoning about what the pipeline sees, deciding actions, remembering context). A Dora pipeline could publish detections to Zenoh, which Bubbaloop agents consume.

This integration doesn't exist today, but both projects use Rust and both support cross-process communication — a bridge is feasible.

---

## Summary

**Bubbaloop** is an AI agent platform for hardware. It answers "what should the robot do?" using LLMs with memory and tools.

**Dora** is a dataflow middleware for ML pipelines. It answers "how do I move data efficiently between perception models?" using Apache Arrow zero-copy.

They target different layers of the robotics stack and could potentially work together.
