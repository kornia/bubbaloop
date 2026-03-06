---
hide:
  - navigation
  - toc
template: home.html
---

# Bubbaloop

---

## What you get

<div class="grid cards" markdown>

-   :material-robot:{ .lg .middle } **Agent Runtime**

    ---

    Multi-agent Zenoh gateway. Each agent gets its own Soul, memory, and heartbeat.

    [:octicons-arrow-right-24: Architecture](concepts/architecture.md)

-   :material-wrench:{ .lg .middle } **37 Tools**

    ---

    Node lifecycle, fleet discovery, memory search, telemetry, scheduling, proposals.

    [:octicons-arrow-right-24: Agent Guide](agent-guide.md)

-   :material-database:{ .lg .middle } **3-Tier Memory**

    ---

    RAM for the current turn. NDJSON logs for yesterday. SQLite for the long game.

    [:octicons-arrow-right-24: Memory](concepts/memory.md)

-   :material-account-heart:{ .lg .middle } **Soul System**

    ---

    `identity.md` for personality. `capabilities.toml` for model tuning. Hot-reload.

    [:octicons-arrow-right-24: Configuration](getting-started/configuration.md)

-   :material-chip:{ .lg .middle } **Node SDK**

    ---

    Batteries-included Rust framework. ~50 lines to a working sensor node.

    [:octicons-arrow-right-24: Create a Node](guides/create-your-first-node.md)

-   :material-access-point:{ .lg .middle } **Self-Describing Nodes**

    ---

    Schema, manifest, health, config, command — every node is discoverable by agents.

    [:octicons-arrow-right-24: Messaging](concepts/messaging.md)

-   :material-gauge:{ .lg .middle } **Telemetry Watchdog** <span class="badge badge-new">New</span>

    ---

    CPU, RAM, disk, GPU monitoring. 5 severity levels. Circuit breakers. Runtime-tunable.

    [:octicons-arrow-right-24: Telemetry Guide](guides/telemetry-watchdog.md)

-   :material-monitor-dashboard:{ .lg .middle } **Dashboard**

    ---

    React + WebCodecs. Live camera feeds, telemetry charts, protobuf decode.

    [:octicons-arrow-right-24: Dashboard](dashboard/index.md)

-   :material-server-network:{ .lg .middle } **Fleet Management**

    ---

    Multi-machine orchestration via scoped Zenoh topics. One daemon per machine.

    [:octicons-arrow-right-24: Architecture](concepts/architecture.md)

</div>

---

## Architecture

```mermaid
flowchart TB
    subgraph Clients
        cli["CLI — bubbaloop agent chat"]
        mcp_client["MCP Client — Claude Code"]
        dash["Dashboard — React"]
    end

    subgraph "Daemon (single ~13 MB binary)"
        runtime["Agent Runtime — multi-agent, per-agent memory"]
        mcp["MCP Server — 30 tools, RBAC"]
        nm["Node Manager — lifecycle, health, build"]
        watchdog["Telemetry Watchdog — CPU/RAM/disk/GPU"]
    end

    subgraph Messaging
        zenohd(("zenohd — pub/sub router"))
        bridge["WebSocket Bridge"]
    end

    subgraph "Nodes (self-describing)"
        cam["rtsp-camera"]
        weather["openmeteo"]
        custom["your-node..."]
    end

    cli --> zenohd
    mcp_client --> mcp
    dash --> bridge --> zenohd
    zenohd <--> runtime
    zenohd <--> nm
    zenohd <--> cam
    zenohd <--> weather
    zenohd <--> custom
```

---

## Quick start

```bash
# Install (Linux x86_64 / ARM64)
curl -sSL https://github.com/kornia/bubbaloop/releases/latest/download/install.sh | bash
source ~/.bashrc

# Login + start
bubbaloop login
bubbaloop up

# Talk to your hardware
bubbaloop agent chat
```

First time? The agent introduces itself and asks what you care about. It remembers.

[:octicons-arrow-right-24: Full quickstart guide](getting-started/quickstart.md)

---

## Commands

| Command | What it does |
|---------|-------------|
| `bubbaloop up` | Start daemon (agent runtime + MCP + node manager) |
| `bubbaloop agent chat` | Chat with AI agents via Zenoh |
| `bubbaloop agent list` | List running agents and their models |
| `bubbaloop login` | Authenticate (API key or Claude subscription) |
| `bubbaloop status` | System and node status |
| `bubbaloop doctor --fix` | Diagnostics with auto-repair |
| `bubbaloop node list` | List registered nodes |
| `bubbaloop node add` | Add node from path, GitHub, or marketplace |
| `bubbaloop mcp --stdio` | Run MCP server over stdio |

[:octicons-arrow-right-24: Full CLI reference](reference/cli.md)

---

## Development

```bash
git clone https://github.com/kornia/bubbaloop.git && cd bubbaloop
pixi install && pixi run up
```

Dashboard dev server: [localhost:5173](http://localhost:5173)

---

<div class="grid" markdown>

[:fontawesome-brands-github: **GitHub**](https://github.com/kornia/bubbaloop){ .md-button }
[:fontawesome-brands-discord: **Discord**](https://discord.com/invite/HfnywwpBnD){ .md-button }

</div>
