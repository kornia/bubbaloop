# bubbaloop-agent: Agentic AI for IoT/Robotics via Telegram

## Scope

- **Chat**: Telegram only (WhatsApp deferred to future phase)
- **LLM**: Anthropic Claude only - Haiku 4.5 default (~$0.0015/interaction with caching)
- **Scale**: Personal (1-10 users, your team)
- **Language**: Rust (reuses existing `DaemonClient`, single binary, ecosystem-coherent)
- **Estimated cost**: ~$4.50/month for 10 users at 10 interactions/day

## What Makes This "Intelligent" (Not Just a Command Relay)

| Capability | Simple Bot | This Agent |
|-----------|-----------|------------|
| User asks "start camera" | Translates to API call | Same |
| Camera crashes at 3am | Silent. User discovers later. | Detects failure, reasons about cause, restarts, notifies user with diagnosis |
| Camera crashes 5 times in 1 hour | Keeps blindly restarting | Detects pattern, escalates: "Camera keeps crashing. Logs show GStreamer error. Likely hardware issue." |
| "Keep all cameras running" | Not possible | Standing order: monitors continuously, auto-restarts, backs off if repeated failure |
| "Why did inference fail?" | Shows logs | Correlates: "Inference failed 2 min after camera stopped. They share a dependency. Restarting camera first." |
| Same problem happens again | No memory | "This happened last Tuesday too. Last time, restarting zenohd fixed it." |

### Core Intelligence Patterns

1. **Agentic loop**: Multi-turn tool calling (observe -> reason -> act -> verify), not one-shot
2. **Autonomy levels**: Configurable how much the agent does without asking
3. **Proactive monitoring**: Subscribes to Zenoh events, acts on failures without prompting
4. **Standing orders**: Persistent goals ("keep cameras running") that survive restarts
5. **Circuit breakers**: Prevents rapid-cycling (max 3 restarts per 10 minutes)
6. **Memory**: Incident history, learned patterns, user preferences
7. **Escalation**: Knows when to act vs when to ask for help

## Architecture

```
crates/bubbaloop-agent/
├── Cargo.toml
├── node.yaml
├── configs/
│   └── agent.yaml
└── src/
    ├── main.rs                  # Entry point, tokio runtime
    ├── config.rs                # Config schema (serde + yaml)
    │
    ├── channels/                # Communication Layer
    │   ├── mod.rs               # ChatChannel trait
    │   ├── telegram.rs          # teloxide adapter
    │   └── console.rs           # stdin/stdout (dev/testing)
    │
    ├── brain/                   # Brain Layer (Claude Haiku 4.5)
    │   ├── mod.rs
    │   ├── claude.rs            # Anthropic API + agentic tool-calling loop
    │   ├── tools.rs             # Tool registry + JSON schema definitions
    │   └── system_prompt.rs     # Dynamic prompt with live state + standing orders
    │
    ├── memory/                  # Self-Aware Memory (LanceDB + fastembed)
    │   ├── mod.rs               # AgentMemory struct (owns DB + embedding model)
    │   ├── schema.rs            # Arrow/Lance table schemas
    │   ├── incidents.rs         # Incident CRUD + similarity search
    │   ├── patterns.rs          # Learned pattern detection + storage
    │   ├── strategies.rs        # What worked, success rate tracking
    │   └── conversations.rs     # Per-user history with semantic search
    │
    ├── safety/                  # Safety & Autonomy Layer
    │   ├── mod.rs
    │   ├── permissions.rs       # Tiered role model + user store
    │   ├── autonomy.rs          # Autonomy levels + circuit breakers
    │   └── audit.rs             # Append-only JSONL audit log
    │
    ├── reactive/                # Proactive Intelligence
    │   ├── mod.rs
    │   ├── monitor.rs           # Zenoh event subscriber + failure detection
    │   ├── standing_orders.rs   # Persistent goals engine
    │   └── escalation.rs        # When to act vs ask vs escalate
    │
    └── actions/                 # Action Layer (ONLY module with Zenoh access)
        ├── mod.rs               # ActionExecutor (owns DaemonClient)
        └── node_actions.rs      # Tool call -> DaemonClient method mapping
```

### Security Boundary

```
Telegram message (untrusted)
    │
    ▼
channels/telegram.rs       ← Has: bot token only. No Zenoh.
    │ (text, user_id)
    ▼
safety/permissions.rs      ← Checks role vs tool tier. Logs to audit.
    │ (authorized request)
    ▼
brain/claude.rs            ← Has: ANTHROPIC_API_KEY only. No Zenoh.
    │ (ToolCall enum)       ← Returns typed tool calls, not raw strings.
    ▼
actions/mod.rs             ← ONLY module with DaemonClient + Zenoh session.
                            ← Validates params. add_node/build BLOCKED.
```

## Intelligence Architecture

### 1. Agentic Loop (Multi-Turn Reasoning)

The agent doesn't just translate commands - it reasons in a loop:

```
User: "Why is my camera not working?"
    │
    ▼ Claude iteration 1: "I need to check the status"
    → calls get_node_status("rtsp-camera") → "failed"
    │
    ▼ Claude iteration 2: "Failed. Let me check the logs"
    → calls get_node_logs("rtsp-camera") → "GStreamer pipeline error..."
    │
    ▼ Claude iteration 3: "GStreamer error. Let me check dependent nodes"
    → calls list_nodes() → "inference: failed, recorder: failed"
    │
    ▼ Claude iteration 4: "Root cause is camera. Restarting."
    → calls restart_node("rtsp-camera")
    │
    ▼ Claude iteration 5 (end_turn): Full diagnosis to user
```

Implementation: loop until `stop_reason == "end_turn"` (max 10 iterations).
Tool calls within one turn are executed in parallel.

### 2. Autonomy Levels (Controlled Intelligence)

Each action has a configurable autonomy level:

| Level | Behavior | Use For |
|-------|----------|---------|
| **inform** | Detect + notify only | Unknown problems |
| **suggest** | Propose action + inline keyboard [Yes/No] | Destructive ops |
| **act_then_report** | Execute + tell user what happened | Restarts, routine ops |
| **autonomous** | Execute silently, log only | Reads, health checks |

Per-action config in `agent.yaml`:
```yaml
autonomy:
  default_level: suggest
  overrides:
    restart_node: act_then_report
    install_node: suggest
    stop_node: suggest
    list_nodes: autonomous
```

### 3. Circuit Breakers (Safety Limits)

Prevents rapid-cycling a failing node:
- Max 3 restarts per node per 10 minutes
- Exponential backoff: 30s -> 60s -> 120s -> escalate
- After 3 failed restarts -> circuit opens -> alert user
- Circuit resets after 30 minutes of stability

### 4. Proactive Monitoring (Event-Driven)

Agent subscribes to `bubbaloop/daemon/events` and reacts WITHOUT user prompting:

```
Node failure event arrives via Zenoh
    │
    ├─ Check circuit breaker (under limit?)
    ├─ Check standing orders (any goal covering this node?)
    ├─ Check incident memory (seen this before?)
    │
    ▼ Feed context to Claude for reasoning
    │
    ├─ autonomy=act_then_report → restart + notify
    ├─ autonomy=suggest → send Telegram with [Restart] [Ignore] buttons
    └─ escalation triggered → alert with full diagnosis
```

### 5. Standing Orders (Persistent Goals)

Users set goals the agent monitors continuously:

```
User: "Keep all cameras running"
Agent: "Standing order created. I'll auto-restart camera nodes on failure
        (max 3 attempts, then escalate)."
```

Stored in `~/.bubbaloop/agent/standing_orders.json`, checked on every node event.
Manageable via chat: "list orders", "cancel keep-cameras-running".

### 6. Self-Aware Memory (LanceDB + fastembed)

The agent's memory is a local vector database that enables semantic search
over its own history. No external API - embeddings generated on-device.

**Stack**: LanceDB (embedded Rust vector DB) + fastembed-rs (local BGE-small, 384 dims, 33MB)
**Storage**: GCS bucket `gs://<bucket>/agent-memory/` (for central dashboard access)
**Local fallback**: `~/.bubbaloop/agent/memory.lance/` (when offline or for dev)
**RAM**: ~80-130MB total (LanceDB memory-mapped + embedding model)
**GCS cost**: ~$0.006/month for 300MB (negligible)

```yaml
# configs/agent.yaml
memory:
  # Cloud storage (production) - queryable from dashboard
  backend: "gs://bubbaloop-agent-memory/prod"
  # Local storage (development / offline fallback)
  # backend: "~/.bubbaloop/agent/memory.lance"
```

#### Memory Tables

| Table | Purpose | Key Query |
|-------|---------|-----------|
| **incidents** | Past failures + resolutions | "Find similar failures to this GStreamer error" |
| **conversations** | Chat history with embeddings | "What did Alice ask about cameras last week?" |
| **patterns** | Learned recurring patterns | "Does this node fail at this time regularly?" |
| **strategies** | What worked and success rates | "Best fix for network timeouts? (85% success)" |
| **events** | Raw audit trail | "All actions taken in last 24h" |

#### How Self-Awareness Works

```
New incident: "rtsp-camera failed with GStreamer error"
    │
    ▼ Embed description with fastembed (2ms, local)
    │
    ├─ Search incidents table: "Find 5 most similar past failures"
    │  → "3 similar incidents found. Restart fixed 2/3. Third time
    │     needed zenohd restart (different root cause)."
    │
    ├─ Search strategies table: "Best strategy for this problem?"
    │  → "restart_camera strategy: 67% success rate, used 3 times"
    │  → "restart_zenohd strategy: 100% success rate for this pattern"
    │
    ├─ Search patterns table: "Is this a known pattern?"
    │  → "Camera GStreamer failures correlate with high CPU (pattern
    │     confidence: 0.8, seen 5 times)"
    │
    ▼ All context injected into Claude's system prompt
    │
    Claude reasons: "This looks like the CPU-related GStreamer issue
    I've seen 5 times before. Restarting the camera alone only works
    67% of the time. I should check CPU usage first."
```

#### After Resolution: Learning

```rust
// After every incident, record what happened and update strategies
async fn learn_from_outcome(&self, incident: &Incident, result: &Resolution) {
    // 1. Store incident with embedding
    self.memory.insert_incident(incident, result).await;

    // 2. Update strategy success rate
    self.memory.update_strategy_stats(&result.strategy_used, result.success).await;

    // 3. Check if this creates a new pattern (3+ similar incidents)
    let similar = self.memory.find_similar_incidents(&incident.description, 10).await;
    if similar.len() >= 3 {
        self.memory.upsert_pattern(&similar).await;
    }
}
```

### 7. Escalation Logic

```
Failure → Known pattern + high success rate? → Auto-fix
       → First occurrence? → Try obvious fix, monitor
       → 3rd failure in 10 min? → ESCALATE with diagnosis
       → Multiple nodes at once? → ESCALATE (shared dependency likely)
       → Unknown problem? → INFORM, show logs, wait for instructions
```

### 8. Dynamic System Prompt

Rebuilt every interaction with live state:
- Current node statuses (from Zenoh subscription cache)
- Active standing orders
- Recent incidents (last 24h)
- Autonomy rules for this user
- Safety constraints

Static parts (tool definitions, base instructions) use **Anthropic prompt caching**
(`cache_control: {"type": "ephemeral"}`) for 90% cost savings.

## LLM: Claude Haiku 4.5

| Property | Value |
|----------|-------|
| Model | `claude-haiku-4-5-20241022` |
| Input | $1.00/1M tokens |
| Output | $5.00/1M tokens |
| With prompt caching | ~$0.10/1M for cached system prompt (90% savings) |
| Cost per interaction | ~$0.0015 (with caching) |
| Tool-calling quality | Matches Sonnet 4 on function calling |
| Latency | ~400ms TTFT |

**Why Haiku 4.5 over Sonnet**: Same tool-calling reliability at 1/3 the price. For IoT commands (start/stop/status), Haiku is more than sufficient. Upgrade path to Sonnet is a config change.

**Prompt caching**: The system prompt (~500 tokens with tool definitions + node states) is cached via Anthropic's prompt caching. Only the user message and conversation tail are billed at full rate.

**API integration**: Direct `reqwest` HTTP calls to `https://api.anthropic.com/v1/messages` with tool use. No SDK dependency - keeps the binary small and avoids dependency churn.

## Permission Tiers

| Tier | Role | Allowed Tools | Confirmation |
|------|------|---------------|--------------|
| 0 | OBSERVER | list_nodes, get_status, get_logs, system_health | Never |
| 1 | OPERATOR | start_node, stop_node, restart_node | Never (personal scale) |
| 2 | ADMIN | install, uninstall, enable/disable autostart | Yes |
| -- | BLOCKED | add_node, remove_node, build, clean | Never via chat |

Simplified from 4 tiers to 3 for personal scale. `build` and `add_node` remain blocked (they execute arbitrary shell commands).

### User Setup (personal scale)

Config file lists authorized Telegram user IDs and their roles:
```yaml
users:
  - telegram_id: 123456789
    name: "Alice"
    role: admin
  - telegram_id: 987654321
    name: "Bob"
    role: operator
```

## Tool Definitions

Each tool wraps a `DaemonClient` method from `client.rs`:

| Tool | Tier | Claude Schema | DaemonClient Call |
|------|------|--------------|-------------------|
| `list_nodes` | 0 | `{}` (no params) | `list_nodes()` |
| `get_node_status` | 0 | `{name: string}` | Zenoh query `api/nodes/{name}` |
| `get_node_logs` | 0 | `{name: string, lines?: int}` | Zenoh query `api/nodes/{name}/logs` |
| `system_health` | 0 | `{}` | `is_available()` |
| `start_node` | 1 | `{name: string}` | `send_command(name, "start")` |
| `stop_node` | 1 | `{name: string}` | `send_command(name, "stop")` |
| `restart_node` | 1 | `{name: string}` | `send_command(name, "restart")` |
| `install_node` | 2 | `{name: string}` | `send_command(name, "install")` |
| `uninstall_node` | 2 | `{name: string}` | `send_command(name, "uninstall")` |
| `enable_autostart` | 2 | `{name: string}` | `send_command(name, "enable")` |
| `disable_autostart` | 2 | `{name: string}` | `send_command(name, "disable")` |

## Configuration

```yaml
# configs/agent.yaml
zenoh:
  endpoint: "tcp/127.0.0.1:7447"

llm:
  model: "claude-haiku-4-5-20241022"
  max_tokens: 1024
  # API key from ANTHROPIC_API_KEY env var

telegram:
  enabled: true
  # Token from TELEGRAM_BOT_TOKEN env var

console:
  enabled: false  # Enable for local testing

safety:
  audit_log: "~/.bubbaloop/agent/audit.jsonl"

users:
  - telegram_id: 0  # Replace with your Telegram ID
    name: "admin"
    role: admin

memory:
  history_per_user: 30  # conversation turns kept in memory
```

## Dependencies

```toml
[dependencies]
tokio = { version = "1", features = ["full"] }
serde = { version = "1", features = ["derive"] }
serde_json = "1"
serde_yaml = "0.9"
tracing = "0.1"
tracing-subscriber = "0.3"
reqwest = { version = "0.12", features = ["json"] }
teloxide = { version = "0.17", features = ["macros"] }
chrono = { version = "0.4", features = ["serde"] }
uuid = { version = "1", features = ["v4"] }
dirs = "6"
anyhow = "1"
clap = { version = "4", features = ["derive"] }

# Self-aware memory
lancedb = "0.21"                   # Embedded vector DB (Rust-native, no server)
fastembed = "5"                    # Local embeddings (BGE-small, 384 dims, 33MB)
arrow-array = "53"                 # Arrow arrays for LanceDB schema
arrow-schema = "53"                # Arrow schema definitions

# Workspace dependencies (reuse existing bubbaloop crates)
bubbaloop = { path = "../bubbaloop" }
zenoh = { workspace = true }
```

## Implementation Phases

### Phase 1: Agentic Console MVP

**Goal**: Multi-turn reasoning agent via terminal. Not a command relay.

**Files to create** (18 files):
1. `crates/bubbaloop-agent/Cargo.toml` - crate config, add to workspace
2. `crates/bubbaloop-agent/node.yaml` - node manifest
3. `crates/bubbaloop-agent/configs/agent.yaml` - default config
4. `crates/bubbaloop-agent/src/main.rs` - entry point with clap args
5. `crates/bubbaloop-agent/src/config.rs` - YAML config loader
6. `crates/bubbaloop-agent/src/actions/mod.rs` - `ActionExecutor` (owns DaemonClient)
7. `crates/bubbaloop-agent/src/actions/node_actions.rs` - tool call dispatch
8. `crates/bubbaloop-agent/src/brain/mod.rs` - brain module
9. `crates/bubbaloop-agent/src/brain/tools.rs` - tool registry with JSON schemas
10. `crates/bubbaloop-agent/src/brain/claude.rs` - **Agentic loop**: Anthropic API with multi-turn tool calling
11. `crates/bubbaloop-agent/src/brain/system_prompt.rs` - dynamic prompt with live node states
12. `crates/bubbaloop-agent/src/channels/mod.rs` - `ChatChannel` trait
13. `crates/bubbaloop-agent/src/channels/console.rs` - stdin/stdout channel
14. `crates/bubbaloop-agent/src/memory/mod.rs` - AgentMemory (owns LanceDB + fastembed)
15. `crates/bubbaloop-agent/src/memory/schema.rs` - Arrow/Lance table definitions
16. `crates/bubbaloop-agent/src/memory/incidents.rs` - Incident storage + similarity search
17. `crates/bubbaloop-agent/src/memory/conversations.rs` - Per-user chat history
18. `crates/bubbaloop-agent/src/memory/strategies.rs` - Strategy tracking + success rates

**Key: `claude.rs` implements the agentic loop:**
```rust
// Pseudocode for the core loop
loop {
    let response = call_anthropic(&messages).await?;
    match response.stop_reason {
        "end_turn" => return Ok(extract_text(&response)),
        "tool_use" => {
            let tool_calls = extract_tool_uses(&response);
            let results = join_all(tool_calls.map(|tc| execute_tool(tc))).await;
            messages.push(assistant_msg(response.content));
            messages.push(tool_results_msg(results));
        }
    }
}
```

**Verify** (requires zenohd + daemon running):
```bash
ANTHROPIC_API_KEY=sk-... cargo run -p bubbaloop-agent -- -c configs/agent.yaml
> What nodes are running?                    # Single tool call
> Why is the camera not working?             # Multi-step: status → logs → diagnosis
> Show me openmeteo logs and restart it      # Parallel tool calls in one turn
```

### Phase 2: Telegram + Safety + Autonomy

**Goal**: Telegram bot with permission tiers, autonomy levels, and circuit breakers.

**Files to create** (5 files):
1. `crates/bubbaloop-agent/src/channels/telegram.rs` - teloxide bot with inline keyboards
2. `crates/bubbaloop-agent/src/safety/mod.rs` - safety module
3. `crates/bubbaloop-agent/src/safety/permissions.rs` - role checking + user store
4. `crates/bubbaloop-agent/src/safety/autonomy.rs` - autonomy levels + circuit breakers
5. `crates/bubbaloop-agent/src/safety/audit.rs` - JSONL audit logger

**Changes to existing**:
- Add Tier 1 tools (start, stop, restart) to tool registry
- Wire permissions check before tool execution in `actions/mod.rs`
- Wire autonomy level check (inform/suggest/act/autonomous) per tool
- Add circuit breaker state tracking per node
- Telegram inline keyboards for `suggest` mode: [Approve] [Deny]

**Verify**:
1. Create bot via @BotFather, set `TELEGRAM_BOT_TOKEN`
2. Send "start the openmeteo node" -> agent starts it, reports back
3. Send "stop rtsp-camera" -> autonomy=suggest -> inline keyboard appears
4. Rapid-restart test: stop+start camera 4 times quickly -> circuit breaker triggers
5. Check `~/.bubbaloop/agent/audit.jsonl` for complete action log

### Phase 3: Proactive Intelligence

**Goal**: Agent monitors Zenoh events and acts on failures without prompting.

**Files to create** (4 files):
1. `crates/bubbaloop-agent/src/reactive/mod.rs` - reactive module
2. `crates/bubbaloop-agent/src/reactive/monitor.rs` - Zenoh event subscriber
3. `crates/bubbaloop-agent/src/reactive/standing_orders.rs` - persistent goals engine
4. `crates/bubbaloop-agent/src/reactive/escalation.rs` - escalation logic

**Changes to existing**:
- `main.rs`: spawn monitor loop as background tokio task
- `actions/mod.rs`: expose node state cache (from Zenoh subscription)
- `brain/system_prompt.rs`: inject active standing orders into prompt

**New capabilities**:
- Subscribe to `bubbaloop/daemon/events` for real-time failure detection
- When node fails: gather context -> feed to Claude -> act per autonomy level
- Standing orders: "keep cameras running" -> stored, checked on every event
- Incident memory: log every failure + resolution for pattern matching
- Escalation: detect repeated failures, multi-node failures, unknown problems

**Verify**:
1. Set standing order: "keep all cameras running"
2. Stop camera via TUI -> agent detects, restarts, sends Telegram notification
3. Stop camera 4 times in 10 min -> circuit breaker -> escalation message

### Phase 4 (Future): Advanced Intelligence

- `memory/patterns.rs` - Automated pattern detection
- Nightly memory consolidation
- Temporal pattern detection ("camera always fails at 3am")
- Saga workflows for multi-step deployments with rollback
- WhatsApp via Meta Cloud API
- Multi-machine support (machine-scoped Zenoh keys)

## Critical Reference Files

| File | What to Reuse |
|------|---------------|
| `crates/bubbaloop/src/tui/daemon/client.rs` | `DaemonClient` struct - import directly |
| `crates/bubbaloop-daemon/src/zenoh_api.rs` | Command string parsing, JSON request/response formats |
| `protos/bubbaloop/daemon.proto` | NodeState, NodeList, CommandType definitions |
| `Cargo.toml` (workspace root) | Add `crates/bubbaloop-agent` to workspace members |

## Verification (End-to-End)

After Phase 2, the full test is:
1. `zenohd` running
2. `pixi run daemon` running with at least one node registered
3. `cargo run -p bubbaloop-agent` running with Telegram configured
4. Send Telegram message: "What nodes are running?"
5. Agent queries daemon via Zenoh, Claude formats response, sends back via Telegram
6. Send: "Start the openmeteo node"
7. Agent calls `send_command("openmeteo", "start")`, confirms success
8. Verify in TUI or `systemctl --user status bubbaloop-openmeteo`
