---
description: "Get Bubbaloop running in 30 seconds. Install the binary, start the daemon, and connect your first camera or sensor on Jetson or Raspberry Pi."
---

# Quickstart

Get started with Bubbaloop in minutes.

## Prerequisites

- Linux (Ubuntu 22.04+, Jetson, Raspberry Pi)
- Modern browser (Chrome 94+, Edge 94+, Safari 16.4+) for dashboard

## Step 1: Install Bubbaloop

```bash
curl -sSL https://github.com/kornia/bubbaloop/releases/latest/download/install.sh | bash
source ~/.bashrc
```

This installs Zenoh router, WebSocket bridge, and the bubbaloop daemon as systemd services.

### Verify

```bash
bubbaloop doctor --fix    # Auto-fix any issues
bubbaloop status          # Should show daemon + zenoh running
```

## Step 2: Install a Camera Node

```bash
bubbaloop node install rtsp-camera
```

This downloads the precompiled binary from the marketplace and registers it with the daemon.

## Step 3: Configure and Start

```bash
# Create a config for your camera
mkdir -p ~/.bubbaloop/configs
cat > ~/.bubbaloop/configs/entrance.yaml << 'EOF'
name: entrance
publish_topic: camera/entrance/compressed
url: "rtsp://user:password@192.168.1.100:554/stream1"
latency: 200
decoder: cpu
width: 1280
height: 720
EOF

# Install the node (if not already done) and start it
bubbaloop node install rtsp-camera
bubbaloop node start rtsp-camera
```

## Step 4: Verify

```bash
bubbaloop node list                    # Should show rtsp-camera-entrance running
bubbaloop node logs rtsp-camera-entrance -f   # Live logs
bubbaloop debug subscribe "camera/entrance/compressed"  # See frames
```

## Step 5: Talk to Your Hardware

```bash
# Authenticate with Anthropic (needed for AI agent)
bubbaloop login

# Start nodes and load skills (start the daemon first via systemd or `bubbaloop daemon start`)
bubbaloop up

# Chat with the default agent
bubbaloop agent chat "What sensors do I have?"

# Interactive REPL
bubbaloop agent chat

# List running agents
bubbaloop agent list
```

### First-Run Onboarding

Run the setup wizard before starting the daemon:

```bash
bubbaloop agent setup              # Configure the default agent
bubbaloop agent setup -a my-agent  # Create a named agent
```

The wizard asks you to choose a provider (Claude or Ollama) and model, then prompts for a one-sentence role description and writes `identity.md`. On the first `agent chat` turn, the daemon injects an onboarding interview so the LLM can refine its own identity and write it back — no restart needed.

### Custom Agents (Optional)

Create `~/.bubbaloop/agents.toml` to configure multiple agents:

```toml
[agents.jean-clawd]
enabled = true
default = true
provider = "claude"
model = "claude-sonnet-4-20250514"  # optional: overrides soul/capabilities.toml

[agents.camera-expert]
enabled = true
capabilities = ["camera", "rtsp", "video"]
provider = "ollama"
model = "qwen3.5:9b"
```

Customize per-agent identity in `~/.bubbaloop/agents/{agent-id}/soul/identity.md`.

See [Agent Guide](../agent-guide.md#creating-and-managing-agents) for the full multi-agent setup.

## Step 6: Customize Your Agent

Each agent has a Soul — two files that define personality and behavior:

**Identity** (`~/.bubbaloop/agents/{id}/soul/identity.md`):
```markdown
# Jean-Clawd
I'm a hardware-obsessed shrimp who lives in your Jetson.
I specialize in camera feeds and sensor monitoring.
```

**Capabilities** (`~/.bubbaloop/agents/{id}/soul/capabilities.toml`):
```toml
model_name = "claude-sonnet-4-20250514"
heartbeat_base_interval = 60
compaction_flush_threshold_tokens = 4000
```

Changes take effect on the next turn — no restart needed.

See [Memory](../concepts/memory.md) for how agents remember conversations.

---

See [Installation](installation.md) for detailed requirements.

## Service Management

Services are managed via systemd:

```bash
# View status
systemctl --user status zenohd
systemctl --user status bubbaloop-daemon

# Restart
systemctl --user restart bubbaloop-daemon

# View logs
journalctl --user -u bubbaloop-daemon -f
```

## Dashboard

The web dashboard provides real-time visualization.

### Starting the Dashboard

With development install:

```bash
pixi run dashboard
```

Access at: http://localhost:5173

### Dashboard Features

| Panel | Description |
|-------|-------------|
| Cameras | Live H264 video streams |
| Nodes | Service management |
| Weather | Current conditions and forecasts |
| Raw Data | Browse any Zenoh topic |

## Development Workflow

For contributors building from source:

```bash
git clone https://github.com/kornia/bubbaloop.git
cd bubbaloop
pixi install

# Start everything with process-compose
pixi run up

# Or run services individually:
pixi run daemon      # Bubbaloop daemon
pixi run dashboard   # Web dashboard
```

## Browser Requirements

| Browser | Minimum Version | Status |
|---------|-----------------|--------|
| Chrome  | 94+ | Recommended |
| Edge    | 94+ | Supported |
| Safari  | 16.4+ | Supported |
| Firefox | - | Not supported |

!!! warning "Firefox not supported"
    Firefox does not support the WebCodecs API required for H264 decoding.

## Next Steps

- [Installation](installation.md) — Detailed installation options
- [Configuration](configuration.md) — Component configuration options
- [Architecture](../concepts/architecture.md) — Understand the system design
- [Memory](../concepts/memory.md) — How agents remember
