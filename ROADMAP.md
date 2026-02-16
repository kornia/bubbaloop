# Bubbaloop Platform Roadmap

<!-- LIVING DOCUMENT: Claude and contributors should update checkboxes as work completes.
     Check off items when PRs merge. See ARCHITECTURE.md for design details. -->

> AI-Native Cloud Orchestration for Physical AI

---

## Design DNA

> **"Perhaps only apps that rely on specific hardware sensors will remain."**
> — Peter Steinberger, OpenClaw creator (Feb 2026)

**The Steinberger Test**: Does this make the sensor/hardware layer stronger, or does it add app-layer complexity that AI agents will replace?

**Principles**:
1. Sensor nodes are the product — not the daemon, not the dashboard
2. The daemon is scaffolding — useful today, replaceable by AI agents tomorrow
3. Self-describing nodes are AI-native — discovery without documentation
4. Data access rights are the moat — who controls the sensors controls the value
5. Rust + Zenoh + Protobuf — memory safety, decentralized pub/sub, schema introspection

## Vision

Transform Bubbaloop from a local daemon into a **complete platform** where users can:
1. Install with one command on any machine (computer, robot, Jetson)
2. Login from phone with Google/Apple/GitHub
3. See all their machines in one dashboard
4. Control everything via chat (MCP)
5. Work offline (local-first), sync when connected
6. Plug in new hardware and let Claude figure it out
7. Install nodes from any GitHub repo

---

## Architecture

```
                              CLOUD (Cloudflare + fly.io)
┌──────────────────────────────────────────────────────────────────────────┐
│  ┌──────────────┐  ┌──────────────┐  ┌──────────────┐  ┌──────────────┐  │
│  │ Auth Service │  │   Machine    │  │    Zenoh     │  │     Web      │  │
│  │   (OAuth)    │  │   Registry   │  │    Relay     │  │  Dashboard   │  │
│  │ Google/Apple │  │   (D1/SQL)   │  │  (fly.io)    │  │   (Pages)    │  │
│  │ GitHub login │  │ User→Machine│  │ Remote pub/  │  │ Mobile-first │  │
│  └──────────────┘  └──────────────┘  └──────────────┘  └──────────────┘  │
└────────────────────────────────┬─────────────────────────────────────────┘
                                 │
        ┌────────────────────────┼────────────────────────┐
        ▼                        ▼                        ▼
   ┌─────────┐              ┌─────────┐              ┌─────────┐
   │Machine A│              │Machine B│              │Machine C│
   │ (Jetson)│              │(Desktop)│              │ (Robot) │
   └────┬────┘              └────┬────┘              └────┬────┘
        ▼                        ▼                        ▼

========================= LOCAL MACHINE =================================

┌───────────────────────────────────────────────────────────────────────┐
│                        Bubbaloop Agent                                 │
│  ┌─────────────────────────────────────────────────────────────────┐  │
│  │  Cloud Connector          │  MCP Server (Chat)                  │  │
│  │  - OAuth tokens           │  - Natural language commands        │  │
│  │  - Heartbeat to registry  │  - Tools: start/stop/build/logs     │  │
│  │  - Relay connection       │  - Resources: nodes, metrics        │  │
│  │  - GitHub sync            │  - Prompts: policies                │  │
│  └─────────────────────────────────────────────────────────────────┘  │
│  ┌─────────────────────────────────────────────────────────────────┐  │
│  │  Daemon Core (existing)                                         │  │
│  │  HTTP :8088 │ Zenoh :7447 │ Node Manager │ systemd D-Bus        │  │
│  └─────────────────────────────────────────────────────────────────┘  │
│  ┌─────────────────────────────────────────────────────────────────┐  │
│  │  Local Dashboard :5173 (works offline)                          │  │
│  └─────────────────────────────────────────────────────────────────┘  │
└───────────────────────────────────────────────────────────────────────┘
                                     │
              ┌──────────────────────┼──────────────────────┐
              ▼                      ▼                      ▼
        ┌──────────┐           ┌──────────┐           ┌──────────┐
        │  Camera  │           │ Weather  │           │Inference │
        │   Node   │           │   Node   │           │   Node   │
        └──────────┘           └──────────┘           └──────────┘
```

---

## Implementation Tracks

### Track A: Sensor-Centric Foundation

Building a self-describing, decentralized sensor architecture where nodes are autonomous and AI-discoverable.

#### Phase A1: Contract Enforcement (Complete)

**Goal:** Establish consistent machine ID, scoped topics, and complete API contracts across all components.

**Deliverables:**
- [x] Deduplicate `get_machine_id()` to shared Rust module
- [x] Inject `BUBBALOOP_MACHINE_ID` + `BUBBALOOP_SCOPE` via systemd
- [x] Scope all template topics to `bubbaloop/{scope}/{machine_id}/...`
- [x] Complete JSON API with 6 missing fields (machine_id, health timestamps, etc.)
- [x] Status enum cross-validation tests
- [x] Proto copy at install/create time

**Status:** Merged in PR #33.

---

#### Phase A2: Self-Describing Nodes (Mostly Complete)

**Goal:** Nodes declare their own capabilities via manifest queryables. Dashboard discovers without daemon dependency.

**Deliverables:**
- [x] Define manifest JSON schema with `publishes`, `commands`, `requires_hardware`
- [x] Add manifest queryable to Rust + Python templates
- [x] Add command queryable to Rust + Python templates
- [ ] Add Zenoh liveliness tokens for decentralized presence detection (Python blocked on zenoh-python)
- [x] Dashboard wildcard query `bubbaloop/**/manifest` for discovery
- [ ] Update official nodes: network-monitor, system-telemetry, openmeteo, camera

---

#### Phase A3: Security Hardening

**Goal:** mTLS, per-node ACLs, Python sandboxing, and security audit tooling.

**Deliverables:**
- [ ] Enable Zenoh mTLS for multi-machine deployments
- [ ] Configure per-node ACL rules (sandboxed to own key prefix)
- [ ] Application-level encryption for sensitive sensor data (camera feeds)
- [ ] Python node sandbox: localhost-only, limited key access
- [ ] `bubbaloop doctor` command for security posture audit

---

#### Phase A4: Thin Daemon

**Goal:** Remove daemon as single point of failure for discovery. Nodes serve their own schemas.

**Deliverables:**
- [ ] Remove schema serving from daemon (nodes do it)
- [ ] Remove node state tracking (liveliness + manifests replace it)
- [ ] Dashboard no longer calls daemon API for discovery
- [ ] Daemon retains: install, start, stop, update, compositions

---

### Track B: Cloud Platform

Multi-machine fleet management, OAuth login, cloud sync, and mobile-first dashboard.

#### Phase B1: Installation & Agent (Week 1-2)

**Goal:** One-liner install, agent with cloud connector

```bash
curl -sSL https://get.bubbaloop.com | bash
```

**Deliverables:**
- [ ] `scripts/install.sh` - Platform detection, binary download, systemd setup
- [ ] `crates/bubbaloop-agent/` with cloud connector modules
- [ ] `bubbaloop login` command - OAuth flow
- [ ] Agent registers with cloud on login

---

#### Phase B2: Cloud Infrastructure (Week 2-3)

**Goal:** Auth service, machine registry, Zenoh relay

**Deliverables:**
- [ ] OAuth login (Google, Apple, GitHub)
- [ ] Machine registry (Cloudflare D1)
- [ ] Zenoh relay with auth (fly.io)
- [ ] Cloud workers: auth + API (Cloudflare Workers)

---

#### Phase B3: Cloud Dashboard (Week 3-4)

**Goal:** Dual-mode dashboard (local + cloud), mobile-first

**Deliverables:**
- [ ] Same dashboard works locally AND in cloud
- [ ] OAuth login + machine selector
- [ ] Responsive mobile-first styling
- [ ] Chat panel integration

---

#### Phase B4: MCP Integration (Week 4-5) — In Progress

**Goal:** Natural language control via MCP

**Deliverables:**
- [x] MCP server in daemon (tools: list/start/stop/restart/logs/config/manifest/command/query/discover)
- [x] `.mcp.json` for Claude Code integration
- [ ] Chat panel in dashboard
- [ ] Natural language execution: "Start the camera"
- [ ] Device discovery + auto-install tools

---

#### Phase B4b: OpenClaw Foundation (In Progress)

**Goal:** Make bubbaloop the physical AI layer for OpenClaw and other AI agents.

**Deliverables:**
- [x] MCP server with ~15 generic tools (list_nodes, send_command, etc.)
- [x] Agent rule engine with YAML-based rules
- [x] `list_commands` MCP tool for easy command discovery
- [x] Rule management via MCP (add_rule, remove_rule, update_rule)
- [x] Enriched MCP instructions for AI agent workflow guidance
- [x] Optional `mcp:` section in node.yaml for richer tool descriptions
- [ ] MCP authentication for remote agent access

**Design decision:** Enhanced Option B — daemon-only MCP, no per-node MCP tools. Manifest-driven discovery + generic `send_command` dispatcher. See `.omc/plans/openclaw-foundation.md`.

---

#### Phase B5: GitHub Integration (Week 5-6)

**Goal:** Install nodes from any GitHub repo (`bubbaloop install github.com/user/node`)

**Deliverables:**
- [ ] `install_node_from_url` MCP tool
- [ ] Auto-detect project type (Rust/Python, with/without manifest)
- [ ] Auto-generate `node.yaml` if missing
- [ ] Register systemd service

---

#### Phase B6: Hardware Discovery (Week 6-7)

**Goal:** Plug in hardware, AI discovers and suggests nodes

**Deliverables:**
- [ ] USB device enumeration (udev)
- [ ] Network device scanning (mDNS, IP scan)
- [ ] Camera detection (V4L2, RTSP)
- [ ] Hardware → Node mapping database
- [ ] Auto-suggestion flow via MCP

---

### Phase C: AI-Agent Interface (Convergence)

**Goal:** MCP server exposing node manifests as tools. Natural language sensor discovery and control.

**Deliverables:**
- [ ] MCP server exposes node manifests as tools
- [ ] Natural language sensor discovery: "What cameras are available?"
- [ ] AI-assisted node creation: "Create a node that monitors CPU temperature"
- [ ] Composition orchestration via AI: "Track that person"
- [ ] Context-aware responses with hardware constraints

---

## Open-Core Business Model

| Tier | Price | Machine Limit | Key Features |
|------|-------|---------------|--------------|
| **Free** | $0 | 10 machines | CLI, daemon, TUI, local dashboard, community marketplace, node templates |
| **Startup** | $99/mo | 50 machines | Multi-machine fleet dashboard, cloud time-series sync, basic OTA updates, 48h support |
| **Team** | $499/mo | 500 machines | Canary deployments, enterprise ACLs, mTLS auto-rotation, anomaly detection, 24h support |
| **Enterprise** | Custom | Unlimited | On-premise deployment, dedicated engineer, white-label marketplace, SLA guarantees |

**The Steinberger Boundary**: Sensor nodes and local runtime are free (the product). Fleet operations and cloud services are paid.

**Academic/Research**: Always free (Foxglove model).

---

## Simplified Node Format

### Minimal `node.yaml`
```yaml
name: my-node
type: rust  # or python
```

Everything else auto-detected from `Cargo.toml` or `pyproject.toml`.

### Full `node.yaml`
```yaml
name: my-camera-node
version: "1.2.0"
type: rust
description: "Custom camera integration"

# Optional - auto-detected if omitted
build: "cargo build --release"
command: "./target/release/my_camera_node"

# Hardware hints for auto-discovery
hardware:
  usb_vendor: "046d"
  usb_product: "0825"
  device_type: "camera"

# Zenoh topics (documentation)
topics:
  publishes:
    - "/camera/{name}/compressed"
  subscribes:
    - "/config/{name}"

# MCP tools this node exposes
mcp:
  tools:
    - name: "capture_frame"
      description: "Capture a single frame"
```

---

## Technology Stack

| Component | Technology | Why |
|-----------|------------|-----|
| Agent | Rust + Tokio | Performance, existing codebase |
| Cloud Auth | Cloudflare Workers | Serverless, global edge |
| Database | Cloudflare D1 | Serverless SQLite |
| Zenoh Relay | fly.io | Persistent connections |
| Dashboard | React + Vite + Cloudflare Pages | Fast, free hosting |
| MCP Server | Rust + rmcp | Native Rust SDK |
| Install | Bash + curl | Universal |

---

## What Makes This Novel

1. **MCP-first architecture** - AI control is primary, not bolted-on
2. **Local-first** - Works offline, cloud enhances rather than requires
3. **Phone-first UX** - Control robots from your pocket
4. **One-liner install** - No Docker, no complex setup
5. **Multi-machine from day one** - Designed for fleets
6. **Chat as primary interface** - Natural language, not CLI flags
7. **Self-extending** - Claude can integrate new hardware
8. **GitHub-native nodes** - Install from any repo URL

---

## Priority Order

1. `scripts/install.sh` - One-liner that downloads agent binary
2. `crates/bubbaloop-agent/` - Enhanced daemon with MCP server
3. Cloud auth - Google OAuth on Cloudflare Workers
4. Extend dashboard - Add login + machine selector + chat
5. `install_node_from_url` tool - GitHub cloning + auto-build
6. `discover_devices` tool - USB/network scanning

---

## Claude Code Integration

Once implemented, configure `.mcp.json`:

```json
{
  "mcpServers": {
    "bubbaloop": {
      "type": "sse",
      "url": "http://localhost:8088/mcp/sse"
    }
  }
}
```

Example session:
```
User: List my nodes

Claude: [calls list_nodes]
You have 3 nodes:
- rtsp-camera (running)
- openmeteo (stopped)
- inference (not installed)

User: I just plugged in a USB camera

Claude: [calls discover_devices]
Found: Logitech C920

[calls install_node_from_url("github.com/bubbaloop/v4l2-camera-node")]
Installing... Done.

[calls start_node("v4l2-camera")]
Camera streaming to /camera/usb/compressed
```

---

## Maintaining This Document

**Update when:**
- Phases complete → check off items
- New phases added → document goals and deliverables
- Priorities shift → reorder or add urgency notes

**How to update:**
- Check off items when PRs merge to main
- Mark phases as "In Progress" when work starts
- Add new checkboxes for sub-deliverables as they emerge

**Related files:**
- `ARCHITECTURE.md` — Design details and technical decisions
- `CONTRIBUTING.md` — Contribution workflows and PR guidelines
- `CLAUDE.md` — Coding conventions and constraints
