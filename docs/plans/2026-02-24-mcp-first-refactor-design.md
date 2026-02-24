# Bubbaloop MCP-First Universal Runtime — Design Document

**Date:** 2026-02-24
**Status:** Approved (pending implementation planning)
**Authors:** Edgar Riba + Claude
**Reviewers:** Critic (Opus), Architect (Opus), Security Reviewer (Opus)

## Executive Summary

Refactor bubbaloop from a daemon-centric platform into an **MCP-first universal runtime** where any agentic framework (OpenClaw, PicoClaw, Claude, or future tools) can discover and control physical AI nodes via the Model Context Protocol.

**Core bet:** MCP becomes the "HTTP of agents" — a universal protocol every framework supports.

**What changes:** MCP Gateway becomes the primary external interface (~20 tools). Daemon stripped to lifecycle essentials. Security foundation added. Ecosystem tooling for third-party node development.

**What stays:** Rust core, Zenoh data plane, node architecture, systemd lifecycle, protobuf contracts.

## Architecture

### Dual-Plane Model (Control + Data)

```
                    CONTROL PLANE                    DATA PLANE
                    ─────────────                    ──────────
Agent Layer         │ MCP (authenticated)            │ Zenoh (direct)
                    │ Discovery, lifecycle,           │ Streaming sensor
                    │ config, rules, system           │ data at 30fps+
                    │                                │
MCP Gateway ────────┤ ~20 tools                      │
  (axum + rmcp)     │ Bearer token + RBAC            │
                    │ HTTP/StreamableHTTP + stdio     │
                    │                                │
Daemon ─────────────┤ NodeManager (lifecycle)         │ ZenohService (pub/sub)
  (systemd/D-Bus)   │ Health monitor                 │ Agent rule engine
                    │ Registry                       │ Event publishing
                    │                                │
Node Layer ─────────┤ node.yaml + config.yaml        │ Protobuf → Zenoh topics
  (unchanged)       │ systemd services               │ Health heartbeats
                    │                                │ Schema queryables
```

**Key principle:** MCP handles control (discovery, lifecycle, configuration, rules). Zenoh handles data (streaming sensor data). Never route streaming data through MCP.

### Why NOT Streaming Over MCP

MCP is request-response. Camera nodes publish frames at 30fps via Zenoh pub/sub. Attempting to proxy this through MCP (e.g., `subscribe_sensor`) violates MCP semantics and creates backpressure. Instead, the MCP tool `get_stream_info` returns Zenoh connection parameters so agents can subscribe directly.

This dual-plane model is already documented in ARCHITECTURE.md and validated by all three reviewers.

## MCP Gateway Design (~20 Tools)

### Authentication (Phase 0 prerequisite)

```
┌───────────────────────────────────────────────┐
│ axum middleware                                │
│  1. Bearer token check (~/.bubbaloop/mcp-token)│
│  2. RBAC tier: viewer / operator / admin       │
│  3. Rate limiting (tower::RateLimitLayer)      │
│  4. CORS (dashboard origin only)               │
│  5. Audit logging (caller, tool, timestamp)    │
└───────────────────────────────────────────────┘
```

Token auto-generated on first daemon start. Stored in `~/.bubbaloop/mcp-token` with `0600` permissions.

**RBAC tiers:**

| Tier | Tools | Use Case |
|------|-------|----------|
| `viewer` | list_nodes, get_node_detail, get_node_schema, get_stream_info, list_topics, get_system_status, get_machine_info, list_rules, get_events, doctor | Read-only monitoring |
| `operator` | + start_node, stop_node, restart_node, get_node_config, set_node_config, read_sensor, add_rule, remove_rule, test_rule | Day-to-day operations |
| `admin` | + install_node, remove_node, build_node, create_node_instance, set_system_config, query_zenoh | System modification |

### Tool Inventory (~20 tools)

**Discovery (5 tools)**

| Tool | Tier | Description |
|------|------|-------------|
| `list_nodes` | viewer | All registered nodes with status, health, version |
| `get_node_detail` | viewer | Full node info: config, schema, capabilities, health |
| `get_node_schema` | viewer | Protobuf schema as human-readable JSON field descriptions |
| `list_topics` | viewer | Active Zenoh topics with schemas |
| `get_stream_info` | viewer | Returns Zenoh connection params for direct data subscription |

**Lifecycle (5 tools)**

| Tool | Tier | Description |
|------|------|-------------|
| `install_node` | admin | Install from marketplace (download + verify checksum + build + register) |
| `remove_node` | admin | Unregister and remove systemd service |
| `start_node` | operator | Start a node's systemd service |
| `stop_node` | operator | Stop a node's systemd service |
| `restart_node` | operator | Restart a node |

**Data Access (1 tool)**

| Tool | Tier | Description |
|------|------|-------------|
| `read_sensor` | operator | One-shot read of latest value from any node topic (decoded to JSON) |

Note: `subscribe_sensor` and `get_sensor_history` deliberately excluded. Streaming belongs in Zenoh. History requires a time-series store that doesn't exist yet.

**Configuration (3 tools)**

| Tool | Tier | Description |
|------|------|-------------|
| `get_node_config` | operator | Read current node configuration as JSON |
| `set_node_config` | operator | Update node configuration (validates before applying) |
| `set_system_config` | admin | Update daemon config (scope, machine_id, etc.) |

**Automation (5 tools)**

| Tool | Tier | Description |
|------|------|-------------|
| `add_rule` | operator | Create automation rule (trigger + condition + action) |
| `remove_rule` | operator | Delete an automation rule |
| `list_rules` | viewer | List all active rules with status |
| `test_rule` | operator | Dry-run a rule against current sensor data |
| `get_events` | viewer | Recent event log (rule triggers, state changes, health alerts) |

**System (3 tools)**

| Tool | Tier | Description |
|------|------|-------------|
| `get_system_status` | viewer | Overall health: daemon, Zenoh, all nodes, disk/memory |
| `get_machine_info` | viewer | Hardware: CPU, GPU, memory, architecture, OS |
| `doctor` | viewer | Diagnostic: Zenoh connectivity, systemd health, permissions, security config |

**Total: 22 tools** (within the <25 guideline from ARCHITECTURE.md)

### Transports (2 only)

1. **HTTP/StreamableHTTP** (existing) — primary transport, already supports SSE per MCP spec. Binds `127.0.0.1:8088`.
2. **stdio** (new) — `bubbaloop mcp --stdio` subcommand for local agent integration (Claude Code, OpenClaw). Separate process, logs to file not stderr.

No third SSE transport needed — StreamableHTTP already handles it.

### `query_zenoh` Hardening

The existing `query_zenoh` tool gives unrestricted raw Zenoh access. Required changes:
- Validate key expression starts with `bubbaloop/`
- Scope to local machine: `bubbaloop/{scope}/{machine_id}/**` by default
- Deny wildcard-only queries (`**`, `*`)
- Maximum 100 results per query
- Admin tier only

## Daemon Simplification

### What Stays (unchanged)

| Module | File | Responsibility |
|--------|------|---------------|
| NodeManager | `daemon/node_manager.rs` | Lifecycle: start/stop/build/install via systemd |
| Systemd | `daemon/systemd.rs` | D-Bus integration (zbus, no subprocess spawning) |
| Registry | `daemon/registry.rs` | `~/.bubbaloop/nodes.json` management |
| Agent | `agent/mod.rs` + `agent/rules.rs` | Rule engine (stays in daemon, exposed via MCP CRUD) |
| ZenohService | `daemon/zenoh_service.rs` | Pub/sub for dashboard + health monitoring |

### What Gets Deprecated (phased removal)

| Module | File | Replacement | Phase |
|--------|------|-------------|-------|
| Zenoh API | `daemon/zenoh_api.rs` | MCP tools | Remove in Phase 3 |
| TUI | `tui/` (15 files, ~81KB app.rs) | Dashboard + MCP agents | Remove in Phase 3 |
| CLI wrappers | `cli/node.rs` (most subcommands) | MCP tools | Simplify in Phase 3 |

### Rule Engine: Why It Stays in Daemon

The architect and critic reviews converged on this: the rule engine is a real-time reactive system that:
1. Needs persistent Zenoh subscriptions (via `declare_subscriber` callbacks)
2. Benefits from daemon's systemd lifecycle guarantees (long-lived, auto-restart)
3. Has filesystem persistence (`~/.bubbaloop/rules.yaml`) that depends on process stability
4. Would make MCP server stateful if moved there (contradicts gateway pattern)

**Decision:** Keep engine in daemon. MCP tools delegate to `Arc<Agent>` for CRUD (already works today).

### Clean Layer Boundary

Define `trait PlatformOperations` to decouple MCP server from direct `Arc<NodeManager>` dependency:

```rust
trait PlatformOperations: Send + Sync {
    async fn list_nodes(&self) -> Result<Vec<NodeInfo>>;
    async fn execute_command(&self, node: &str, cmd: Command) -> Result<CommandResult>;
    async fn get_node_config(&self, node: &str) -> Result<serde_json::Value>;
    // ... etc
}
```

Both MCP server and any future internal consumer implement against this trait, making the layer separation enforceable at compile time.

## Security Foundation

### Existing Vulnerabilities (fix regardless of refactor)

| Issue | Severity | Location | Fix |
|-------|----------|----------|-----|
| Dashboard binds `0.0.0.0` | HIGH | `bubbaloop_dash.rs:171` | Change to `127.0.0.1` |
| `send_command` broadcasts to all machines via `**` | HIGH | `mcp/mod.rs:205` | Scope to local `{scope}/{machine_id}` |
| `Action::Command` node name unvalidated | HIGH | `agent/rules.rs:222` | Add `validate_node_name()` |
| `Action::Publish` topic unvalidated | CRITICAL | `agent/rules.rs:264` | Add `validate_trigger_pattern()` |
| No checksum for precompiled downloads | CRITICAL | `cli/node.rs:1582` | Add SHA256 verification |
| `journalctl` not called with absolute path | MEDIUM | `node_manager.rs:726` | Use `/usr/bin/journalctl` |

### New Security Requirements

1. **MCP Authentication:** Bearer token middleware on all requests
2. **RBAC Authorization:** 3-tier permission model (viewer/operator/admin)
3. **Rate Limiting:** Per-tool limits, especially on lifecycle operations
4. **CORS:** Restrict browser access to dashboard origin
5. **Audit Logging:** Every MCP tool call logged with caller identity + timestamp
6. **`query_zenoh` scoping:** Validate key expressions, admin-only
7. **Rule action validation:** All fields checked before persistence
8. **Human override auth:** Override messages require caller verification

### Security Checklist

- [ ] MCP server requires authentication on every request
- [ ] Authorization tiers separate read-only from write operations
- [ ] `query_zenoh` key expressions are validated and scoped
- [ ] All rule action fields are validated (node names, publish topics)
- [ ] Precompiled binaries are checksum-verified before execution
- [ ] No services bind to `0.0.0.0` by default
- [ ] Multi-machine configs default to TLS
- [ ] `send_command` is scoped to local machine unless explicitly cross-machine
- [ ] Rate limiting on all destructive MCP operations
- [ ] Audit logging for all MCP tool invocations with caller identity
- [ ] `journalctl` called with absolute path
- [ ] stdio transport requires authentication

## Testing Strategy

### Tier 1: MCP Contract Tests (~100 tests, no Zenoh)

Test every tool's input validation, response schema, and error handling against mock `PlatformOperations` trait impl.

**Mock infrastructure needed:**
- `MockPlatformOps` implementing `trait PlatformOperations` with in-memory state
- `MockAgent` with in-memory rule storage
- No Zenoh session, no systemd, no filesystem

**Example:**
```rust
#[test]
fn test_start_node_invalid_name_returns_error() {
    let mock = MockPlatformOps::new();
    let server = BubbaLoopMcpServer::new_with_ops(mock);
    let result = server.call("start_node", json!({"name": "../etc/passwd"}));
    assert_eq!(result.error_code(), "invalid_node_name");
}
```

**Coverage:** ~5 tests per tool (valid input, invalid input, edge cases, auth tiers).

### Tier 2: Integration Tests (~30 tests, Zenoh + daemon)

Spin up real Zenoh router + daemon + mock nodes in a test harness:

```rust
struct TestHarness {
    zenoh_session: Arc<Session>,
    node_manager: Arc<NodeManager>,
    mcp_client: McpClient,  // rmcp test client
}

impl TestHarness {
    async fn new() -> Self { /* start in-process zenohd, create daemon */ }
    async fn create_mock_node(&self, name: &str) { /* register mock */ }
    async fn mcp(&self, tool: &str, params: Value) -> Value { /* call tool */ }
    async fn wait_healthy(&self, node: &str, timeout: Duration) { /* poll */ }
}
```

**Coverage:** Full lifecycle, data flow, rule engine, config changes, auth enforcement.

### Tier 3: Agent E2E Tests (~10 tests, scripted MCP client)

Scripted sequences that exercise realistic agent workflows without an LLM:

```rust
#[tokio::test]
async fn test_agent_discovers_and_starts_node() {
    let h = TestHarness::new().await;
    h.create_mock_node("camera-front").await;

    let nodes = h.mcp("list_nodes", json!({})).await;
    assert!(nodes["nodes"].as_array().unwrap().len() > 0);

    h.mcp("start_node", json!({"name": "camera-front"})).await;
    h.wait_healthy("camera-front", Duration::from_secs(10)).await;

    let info = h.mcp("get_stream_info", json!({"name": "camera-front"})).await;
    assert!(info["zenoh_topic"].as_str().unwrap().contains("camera-front"));
}
```

### Self-Diagnostic

The `doctor` tool serves as an agentic self-test:
```json
{
  "zenoh": "ok",
  "daemon": "ok",
  "mcp_auth": "enabled",
  "nodes": {"camera": "healthy", "telemetry": "healthy"},
  "tools_available": 22,
  "latency_ms": 12
}
```

Any agent can verify the system works by calling `doctor()`.

## Migration Phases

### Phase 0 — Security Foundation (blocks everything)

**Goal:** Make MCP safe to expand.

- [ ] Add bearer token authentication to MCP server (axum middleware)
- [ ] Implement RBAC authorization tiers (viewer/operator/admin)
- [ ] Fix dashboard `0.0.0.0` → `127.0.0.1`
- [ ] Scope `query_zenoh` key expressions + admin-only
- [ ] Validate rule action fields (`Action::Command` node, `Action::Publish` topic)
- [ ] Add rate limiting on destructive operations
- [ ] Add checksum verification for precompiled binary downloads
- [ ] Fix `send_command` to scope to local machine
- [ ] Add audit logging for MCP tool invocations

**Verification:** Security checklist passes. Existing tests still pass (`pixi run test`).

### Phase 1 — MCP Enhancement

**Goal:** MCP becomes the primary control plane.

- [ ] Define `trait PlatformOperations` for clean layer separation
- [ ] Add 5-7 new MCP tools to reach ~20 total: `get_stream_info`, `search_nodes`, `test_rule`, `get_events`, `get_machine_info`, `create_node_instance`, `build_node`
- [ ] Add stdio transport (`bubbaloop mcp --stdio`)
- [ ] Add MCP contract tests (~100 tests with mock infrastructure)
- [ ] Begin dashboard migration from Zenoh API to MCP/WebSocket

**Verification:** All 22 MCP tools have contract tests. stdio transport works with Claude Code.

### Phase 2 — Dashboard Migration + Test Harness

**Goal:** Dashboard no longer depends on Zenoh API queryables.

- [ ] Complete dashboard migration to MCP + WebSocket
- [ ] Auto-generate JSON schema descriptions from protobuf descriptors
- [ ] Add marketplace MCP tools (`search_nodes` with registry integration)
- [ ] Build integration test harness (TestHarness with in-process zenohd)
- [ ] Add ~30 integration tests
- [ ] Deprecate Zenoh API queryables (log warnings, don't remove)

**Verification:** Dashboard works without Zenoh API. Integration tests pass.

### Phase 3 — Cleanup + Ecosystem

**Goal:** Remove deprecated code. Enable community contributions.

- [ ] Remove deprecated Zenoh API queryables (`zenoh_api.rs`)
- [ ] Remove TUI (or keep behind `--features tui` flag for debugging)
- [ ] Simplify CLI to: `daemon`, `mcp --stdio`, `doctor`, `node list`, `status`
- [ ] Publish `bubbaloop-node-sdk` crate (boilerplate for node authors)
- [ ] Set up `bubbaloop-node-registry` repo (community node catalog)
- [ ] Write developer documentation (Getting Started, Node Dev Guide, MCP Reference)
- [ ] Add agent E2E tests (~10 scripted workflows)

**Verification:** `pixi run clippy` clean. All tests pass. SDK crate compiles independently.

### Phase 4 — Remote Access + ProLink (future, separate initiative)

**Goal:** Multi-machine MCP access and cloud agent integration.

- [ ] Research: MCP-over-Zenoh transport (leverage Zenoh routing + mTLS for auth)
- [ ] Research: ProLink API maturity and integration viability
- [ ] Cloud MCP proxy for fleet management
- [ ] ProLink bridge with mTLS + tool allowlisting + local override
- [ ] Multi-machine integration tests

**Verification:** Agent on machine A can control nodes on machine B via MCP.

## Ecosystem & Contribution Plan

### Repository Structure

```
github.com/kornia/
├── bubbaloop/                    # Core platform (daemon + MCP + CLI)
├── bubbaloop-schemas/            # Protobuf contracts (standalone)
├── bubbaloop-nodes-official/     # Reference nodes (core team maintained)
├── bubbaloop-node-sdk/           # NEW: Crate + templates for node authors
├── bubbaloop-node-registry/      # NEW: Community node catalog (YAML index)
└── bubbaloop-docs/               # NEW: Developer docs, tutorials, examples
```

### Node SDK (`bubbaloop-node-sdk` crate)

A Rust crate that handles all boilerplate so contributors write only business logic:

```rust
use bubbaloop_node_sdk::{Node, NodeConfig, run_node};

#[derive(NodeConfig)]
struct MyConfig {
    #[config(default = 1.0, min = 0.01, max = 100.0)]
    rate_hz: f64,
    #[config(topic)]
    publish_topic: String,
}

struct MyNode { /* ... */ }

impl Node for MyNode {
    type Config = MyConfig;
    type Message = MyProtoMessage;

    async fn process(&mut self) -> Option<Self::Message> {
        // Business logic only — ~50 lines
    }
}

#[tokio::main]
async fn main() { run_node::<MyNode>().await; }
```

The SDK handles: Zenoh session setup, health heartbeats, schema queryable, config parsing + validation, graceful shutdown, CLI flags (`-c`, `-e`), protobuf descriptor serving.

**Result:** Contributors write ~50 lines of business logic instead of ~300 lines of infrastructure.

### Community Node Registry

- `bubbaloop-node-registry/registry.yaml` — index of community nodes
- PR-based submission: add entry with repo URL, description, category, platform support
- CI validates: `node.yaml` exists, builds on ARM64 + x86_64, has tests, passes clippy
- `bubbaloop marketplace search` discovers community nodes via this registry

### Contribution Tiers

| Tier | Barrier | What They Do |
|------|---------|-------------|
| **Node authors** | Low (use SDK) | Build and publish sensor/actuator nodes |
| **SDK contributors** | Medium | Improve the node SDK, add features |
| **Core contributors** | High (review required) | Modify daemon, MCP server, CLI |

### Documentation Plan

| Document | Audience | Content |
|----------|----------|---------|
| Getting Started | Everyone | "Build your first node in 10 minutes" with SDK |
| Node Development Guide | Node authors | SDK usage, testing, publishing, config best practices |
| Architecture Guide | Core contributors | How bubbaloop works internally, design decisions |
| MCP Tool Reference | Agent developers | Every tool with params, examples, auth requirements |
| Security Guide | Operators | Auth setup, multi-machine TLS, hardening |

## Review Summary

### Critic Verdict: REJECT (original) → issues addressed in revision

**Key issues fixed:**
- Phase sequencing corrected (dashboard migrated before Zenoh API removal)
- "Phantom features" removed (marketplace already separate, protobuf→JSON already works)
- Security architecture added (Phase 0)
- ProLink deferred
- Specific tool inventory enumerated

### Architect Verdict: REVISE → corrections applied

**Key corrections applied:**
1. Streaming data stays in Zenoh (dual-plane preserved)
2. Rule engine stays in daemon
3. Two transports, not three
4. 20 tools, not 30
5. Zenoh API removal phased behind dashboard migration
6. `trait PlatformOperations` for clean layer separation
7. MCP-over-Zenoh explored for multi-machine (Phase 4)

### Security Verdict: CRITICAL → remediation plan in Phase 0

**5 critical + 6 high issues identified.** All addressed:
- Phase 0 blocks everything until auth + validation + rate limiting are in place
- Existing bugs (dashboard `0.0.0.0`, `send_command` wildcards, unvalidated rule actions) fixed in Phase 0
- ProLink bridge deferred until security model is proven
- Checksum verification for precompiled downloads

## Open Questions

1. **TUI: feature flag or full removal?** Keep as `--features tui` for SSH debugging scenarios?
2. **Legacy Zenoh key paths:** Remove `bubbaloop/daemon/api/**` (non-machine-scoped) or keep forever?
3. **MCP-over-Zenoh:** Is this the right multi-machine transport, or should we use authenticated tunnels?
4. **Node SDK scope:** Should the SDK support Python nodes too, or Rust-only?
5. **`block_in_place` in rule engine:** Refactor to `tokio::spawn` with bounded channel? (existing perf issue)
