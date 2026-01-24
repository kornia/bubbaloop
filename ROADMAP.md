# Bubbaloop Platform Roadmap

> AI-Native Cloud Orchestration for Physical AI

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

## Implementation Phases

### Phase 1: Installation & Agent (Week 1-2)

**Goal:** One-liner install, agent with cloud connector

```bash
curl -sSL https://get.bubbaloop.com | bash
```

**Deliverables:**
- [ ] `scripts/install.sh` - Platform detection, binary download, systemd setup
- [ ] `crates/bubbaloop-agent/` - Enhanced daemon with cloud modules
- [ ] `bubbaloop login` command - Opens OAuth flow
- [ ] Agent registers with cloud on login

**Files to create:**
```
scripts/install.sh
crates/bubbaloop-agent/
├── Cargo.toml
├── src/
│   ├── main.rs
│   ├── cloud/
│   │   ├── mod.rs
│   │   ├── connector.rs
│   │   ├── auth.rs
│   │   └── registry.rs
```

---

### Phase 2: Cloud Infrastructure (Week 2-3)

**Goal:** Auth service, machine registry, Zenoh relay

**Deliverables:**
- [ ] OAuth login works (Google first, then Apple/GitHub)
- [ ] Machines appear in cloud registry
- [ ] Zenoh relay accepts authenticated connections

**Infrastructure:**
```
cloud/
├── auth-worker/          # Cloudflare Worker - OAuth
├── api-worker/           # Cloudflare Worker - Machine registry
└── zenoh-relay/          # fly.io - Zenoh router with auth
```

**Database schema:**
```sql
CREATE TABLE users (id, email, name, oauth_provider);
CREATE TABLE machines (id, user_id, name, hostname, platform, last_seen, is_online);
```

---

### Phase 3: Cloud Dashboard (Week 3-4)

**Goal:** Extend existing dashboard with cloud features (mobile-first)

**Key Design: Dual-Mode Dashboard**
- **Local mode** (default): No auth, direct Zenoh to localhost
- **Cloud mode**: OAuth required, Zenoh via relay, machine selector

**Deliverables:**
- [ ] Same dashboard works locally AND in cloud
- [ ] Login from phone works (cloud mode)
- [ ] Machine selector when multiple machines
- [ ] Responsive/mobile-first styling

**Files to modify in `dashboard/`:**
```
dashboard/src/
├── components/
│   ├── Login.tsx             # NEW
│   ├── MachineSelector.tsx   # NEW
│   ├── ChatPanel.tsx         # NEW
│   └── AuthGuard.tsx         # NEW
├── lib/
│   ├── auth.ts               # NEW
│   ├── cloud-zenoh.ts        # NEW
│   └── zenoh.ts              # MODIFY
├── App.tsx                   # MODIFY
└── main.tsx                  # MODIFY
```

---

### Phase 4: MCP Integration (Week 4-5)

**Goal:** Natural language control via chat

**MCP Tools:**
| Tool | Description |
|------|-------------|
| `list_nodes` | List nodes on a machine |
| `start_node` | Start a node |
| `stop_node` | Stop a node |
| `restart_node` | Restart a node |
| `build_node` | Build a node |
| `get_logs` | Get node logs |
| `read_config` | Read node config |
| `update_config` | Update config + restart |
| `discover_devices` | Scan USB/network for devices |
| `install_node_from_url` | Install from GitHub URL |
| `configure_node` | Update node config for hardware |

**Deliverables:**
- [ ] MCP server in agent (Streamable HTTP)
- [ ] Chat panel in dashboard
- [ ] "Start the camera" understood and executed
- [ ] Context-aware responses

**Files to create:**
```
crates/bubbaloop-agent/src/mcp/
├── mod.rs
├── server.rs
├── tools.rs
├── resources.rs
└── prompts.rs
```

---

### Phase 5: GitHub Integration (Week 5-6)

**Goal:** Install nodes from any GitHub repo

```bash
bubbaloop install github.com/user/my-camera-node
```

**Supported formats:**
| Format | Detection | Install Method |
|--------|-----------|----------------|
| Rust with `node.yaml` | `Cargo.toml` + `node.yaml` | `cargo build --release` |
| Rust without manifest | `Cargo.toml` only | Generate manifest, build |
| Python with `pyproject.toml` | `pyproject.toml` | `pip install` |
| Python with `requirements.txt` | `requirements.txt` | Create venv, pip install |

**Deliverables:**
- [ ] `install_node_from_url` MCP tool
- [ ] Auto-detect project type (Rust/Python)
- [ ] Auto-generate `node.yaml` if missing
- [ ] Register and install systemd service

**Files to create:**
```
crates/bubbaloop-agent/src/installer/
├── mod.rs
├── github.rs
├── rust_builder.rs
├── python_builder.rs
└── manifest.rs
```

---

### Phase 6: Hardware Discovery (Week 6-7)

**Goal:** Self-extending platform - plug in hardware, Claude figures it out

**Deliverables:**
- [ ] USB device enumeration (udev)
- [ ] Network device scanning (mDNS, IP scan)
- [ ] Camera detection (V4L2, RTSP probe)
- [ ] Hardware → Node mapping database
- [ ] Auto-suggestion when new device detected

**Files to create:**
```
crates/bubbaloop-agent/src/discovery/
├── mod.rs
├── usb.rs
├── network.rs
└── v4l2.rs
```

**Example flow:**
```
User: "I just plugged in a USB camera"

Claude: [calls discover_devices]
Found: Logitech C920 (046d:082d)

[calls search_nodes("logitech c920")]
Found: github.com/bubbaloop/v4l2-camera-node

Would you like me to install and configure it?
```

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
