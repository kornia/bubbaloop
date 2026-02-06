# Bubbaloop Agent - Architecture, Capabilities & Limitations

An LLM-first autonomous agent that serves as the brain of a Physical AI system.
No hardcoded rules, no compiled conditions — the LLM gets real-time system context,
a set of tools, and reasons freely. The tools are the safety boundary.

## How It Works

```
User: "Keep an eye on disk and stop cameras if it gets too full"

  ┌──────────────────────────────────────────────────────────┐
  │                    System Prompt                          │
  │  Built fresh every LLM call from live runtime state:     │
  │  SOUL.md + World Model + Watchers + Captures + Tools +   │
  │  Memory + Safety Rules                                   │
  └────────────────────────┬─────────────────────────────────┘
                           │
  ┌────────────────────────▼─────────────────────────────────┐
  │                    Agent Loop                             │
  │                                                           │
  │  User message                                             │
  │    → LLM reasons (sees nodes, health, disk at 76%)        │
  │    → calls create_watcher(topics=["system-telemetry/..."],│
  │        instruction="stop cameras if disk > 90%")          │
  │    → tool result fed back to LLM                          │
  │    → LLM: "Done. I'll check every 30s."                  │
  └──────────────────────────────────────────────────────────┘
                           │
  ┌────────────────────────▼─────────────────────────────────┐
  │                  Watcher Engine                           │
  │  Every 30s:                                               │
  │    → reads latest telemetry from Zenoh topic buffer       │
  │    → sends data + instruction to small LLM                │
  │    → LLM decides: "87% — getting close, no action yet"   │
  │    → ...                                                  │
  │    → LLM decides: "93% — act now" → calls stop_node()    │
  └──────────────────────────────────────────────────────────┘
```

## Architecture

```
┌─────────────────────────────────────────────────────────────────┐
│ main.py                                                         │
│   Wires everything together. Runs:                              │
│   • Health heartbeat loop (every 5s)                            │
│   • World model refresh (every 30s)                             │
│   • HTTP API server                                             │
│   • Watcher evaluation loops                                    │
│   • Zenoh session                                               │
└─────────────┬───────────────────────────────────────────────────┘
              │
   ┌──────────┼──────────┬──────────────┬──────────────┐
   │          │          │              │              │
   ▼          ▼          ▼              ▼              ▼
┌────────┐ ┌────────┐ ┌───────────┐ ┌──────────┐ ┌──────────┐
│ HTTP   │ │ Agent  │ │ Watcher   │ │ Data     │ │ Zenoh    │
│ API    │ │ Loop   │ │ Engine    │ │ Router   │ │ Bridge   │
│        │ │        │ │           │ │          │ │          │
│/api/   │ │msg→LLM │ │LLM-driven│ │topic→file│ │pub/sub + │
│ chat   │ │→tools  │ │monitors  │ │capture   │ │daemon API│
│ world  │ │→loop   │ │with mini │ │pipelines │ │+ topic   │
│ watch  │ │→respond│ │agent loop│ │          │ │  buffer  │
└────┬───┘ └───┬────┘ └────┬─────┘ └────┬─────┘ └────┬─────┘
     │         │           │            │             │
     │    ┌────▼────┐ ┌────▼─────┐     │             │
     │    │ Prompt  │ │ Tool     │     │             │
     │    │ Builder │ │ Registry │◄────┘             │
     │    │         │ │ 22 tools │                   │
     │    │assembles│ │          │                   │
     │    │system   │ └──────────┘                   │
     │    │prompt   │                                │
     │    └────┬────┘                                │
     │         │                                     │
     │    ┌────▼────┐  ┌──────────┐  ┌──────────┐   │
     │    │ World   │  │ Memory   │  │ LLM      │   │
     │    │ Model   │◄─┤MEMORY.md │  │ Provider │   │
     │    │         │  │+ convos  │  │ OpenAI   │   │
     │    │nodes,   │  │          │  │ compat   │   │
     │    │health   │  └──────────┘  └──────────┘   │
     │    └─────────┘                                │
     │                                               │
     └───────────────────┬───────────────────────────┘
                         │
                    ┌────▼─────┐
                    │  Zenoh   │
                    │  Router  │
                    │ tcp/7447 │
                    └────┬─────┘
                         │
          ┌──────────────┼──────────────┐
          │              │              │
     ┌────▼───┐    ┌────▼────┐   ┌────▼─────┐
     │ Daemon │    │ system- │   │ rtsp-    │
     │ API    │    │telemetry│   │ camera   │
     └────────┘    └─────────┘   └──────────┘
```

## Core Modules

### `main.py` — Entry Point

Wires all components together and runs the main async loop:
- Opens Zenoh session
- Initializes world model, memory, tools, LLM, agent, watchers, data router
- Starts HTTP API server
- Publishes health heartbeats every 5 seconds
- Refreshes world model every 30 seconds
- Publishes AgentEvent protobuf messages

### `src/zenoh_bridge.py` — Zenoh Client

The communication layer to the bubbaloop ecosystem:
- **Topic buffer**: Subscribes to topics, keeps latest N samples per topic in memory
- **Daemon API**: Queries `bubbaloop/{machine_id}/daemon/api/{endpoint}` (no scope in daemon path)
- **Proto decoder**: Attempts protobuf → dict, falls back to JSON → string
- **Scoped topics**: Builds `bubbaloop/{scope}/{machine_id}/{suffix}` from config suffix
- **Scouting disabled**: No multicast, no gossip (required by bubbaloop convention)

### `src/agent.py` — Agent Loop

The core reasoning engine:
1. Builds system prompt from live runtime state
2. Sends user message + conversation history to LLM
3. If LLM returns tool calls → execute them → feed results back → loop
4. If LLM returns text → final response → save to conversation
5. Max 20 turns (configurable) to prevent infinite loops
6. Streams intermediate `[used: tool1, tool2]` status updates

### `src/prompt_builder.py` — Dynamic System Prompt

Assembled fresh on every LLM call from:

| Section | Source | Purpose |
|---------|--------|---------|
| Identity | `SOUL.md` file | Who am I, priorities, personality |
| World Model | Live Zenoh data | Nodes, health, status, topics |
| Active Watchers | Watcher engine | What am I monitoring |
| Active Captures | Data router | What data am I saving |
| Capabilities | Tool registry | Available tools with descriptions |
| Memory | `MEMORY.md` file | Persistent learnings |
| Safety Rules | `config.yaml` | Boundaries and limits |

This means the LLM always sees the current system state — no stale context.

### `src/watcher_engine.py` — LLM-Driven Monitors

The core innovation. A watcher is NOT a compiled rule — it's an ongoing
LLM conversation about a data stream:

```
create_watcher(
  name="disk-protection",
  topics=["system-telemetry/metrics"],
  instruction="Monitor disk usage. If above 85% alert. If above 95% stop cameras.",
  sample_interval_sec=30
)
```

Every 30 seconds:
1. Collect latest data from subscribed topics
2. Build a mini-prompt with data + instruction + recent history
3. Call the eval LLM (can be a smaller/cheaper model)
4. If LLM decides to act → execute tool calls (max 5 turns)
5. Log the assessment to watcher history

**Why this is better than hardcoded rules:**
- "If things look bad" — the LLM understands context
- Compound reasoning works naturally ("disk high AND temperature extreme")
- The LLM sees previous evaluations and adapts
- Works with any data format the LLM can interpret

**Rate limiting**: Per-watcher `max_actions_per_hour` + global `max_evaluations_per_minute`.

### `src/world_model.py` — Live System State

Tracks node states by querying the daemon API:
- Node name, status (running/stopped/failed), health, type, version
- Renders to text for the system prompt
- Handles both string and integer status values from the daemon

### `src/memory.py` — Persistent Memory

- **MEMORY.md**: Human-readable, categorized markdown. The LLM reads it in every
  system prompt and writes to it via the `remember` tool.
- **Conversations**: JSONL files per conversation_id. Only user/assistant messages
  persisted (system messages filtered out).

### `src/data_router.py` — Data Capture

Routes Zenoh topic data to files:
- **Formats**: JSON (jsonl), CSV, raw binary, h264
- **Path validation**: Only writes to allowed directories (configurable)
- **Path traversal protection**: Rejects `..` in paths
- **Max files**: Optional rolling file limit
- **Persistence**: Active captures survive restarts via `captures.json`

### `src/llm/openai_compat.py` — LLM Provider

OpenAI-compatible API client. Works with:
- **Ollama** (tested with qwen3:1.7b and qwen3:8b)
- **vLLM**, **LiteLLM**, **text-generation-inference**
- **OpenAI**, **Claude** (via API proxy), **Gemini** (via OpenAI compat)

Handles qwen3 thinking mode (extracts from `reasoning` field when `content` is empty).

### `src/http_api.py` — HTTP + WebSocket API

| Endpoint | Method | Description |
|----------|--------|-------------|
| `/api/health` | GET | Agent health check |
| `/api/chat` | POST | Synchronous chat (returns when done) |
| `/api/chat/stream` | WS | WebSocket streaming (status updates + response) |
| `/api/world` | GET | Current world state (nodes, daemon health) |
| `/api/watchers` | GET | Active watchers with status |
| `/api/captures` | GET | Active data captures |

### `src/tools/` — 22 Tools Across 6 Skills

**zenoh-core** (3 tools):
- `subscribe_topic` — Subscribe to a topic and get latest data
- `query_topic` — Query any Zenoh key expression
- `publish_message` — Publish to a topic

**node-management** (6 tools):
- `list_nodes` — List all nodes with status/health
- `start_node` / `stop_node` / `restart_node` — Lifecycle control
- `build_node` — Build/rebuild a node
- `get_logs` — Get recent node logs

**watchers** (4 tools):
- `create_watcher` — Create an LLM-driven data monitor
- `list_watchers` / `remove_watcher` / `pause_watcher`

**data-ops** (3 tools):
- `save_stream` — Capture topic data to files
- `stop_capture` / `list_captures`

**memory** (3 tools):
- `remember` / `recall` / `forget`

**system** (3 tools):
- `system_health` — Overall health summary
- `get_world_state` — Comprehensive system state
- `get_machine_info` — Hardware/OS/disk/GPU info

## Safety Boundaries

The LLM can only act through tools. Safety is enforced at the tool level:

| Boundary | Mechanism |
|----------|-----------|
| Protected nodes | `stop_node`/`restart_node` reject names in `safety.protected_nodes` |
| Data paths | `save_stream` validates against `safety.allowed_data_paths` |
| Path traversal | `..` rejected, paths resolved to absolute |
| Agent turns | Max 20 turns per chat request (prevents infinite loops) |
| Watcher rate limit | `max_actions_per_hour` per watcher |
| Eval rate limit | `max_evaluations_per_minute` global cap |
| Network binding | HTTP API binds to `127.0.0.1` only |
| Zenoh scouting | Multicast and gossip disabled |
| Self-protection | Agent cannot stop itself (`bubbaloop-agent` is protected) |

The LLM also receives safety rules in its system prompt — it will often refuse
dangerous requests before even attempting a tool call.

## Configuration

`config.yaml` controls all behavior:

```yaml
llm:
  base_url: "http://localhost:11434/v1"   # Any OpenAI-compatible endpoint
  model: "qwen3:1.7b"                     # Model name
  max_tokens: 4096
  temperature: 0.1

watchers:
  eval_model: "qwen3:1.7b"               # Can use cheaper model for routine checks
  eval_base_url: "http://localhost:11434/v1"
  default_sample_interval_sec: 30
  max_evaluations_per_minute: 10
  max_actions_per_hour: 30

http:
  host: "127.0.0.1"                       # Never 0.0.0.0
  port: 8080

safety:
  max_agent_turns: 20
  allowed_data_paths: ["/data/", "/tmp/bubbaloop/"]
  protected_nodes: [bubbaloop-agent]
```

## File Layout

```
bubbaloop-agent/
├── main.py                  # Entry point
├── node.yaml                # Daemon registration manifest
├── pixi.toml                # Dependencies (eclipse-zenoh, openai, aiohttp)
├── config.yaml              # All configuration
├── build_proto.py           # Compiles .proto → _pb2.py
├── SOUL.md                  # Agent identity (user-editable)
├── protos/
│   ├── header.proto         # Standard bubbaloop header
│   └── agent.proto          # AgentEvent message
├── skills/                  # SKILL.md per skill (LLM reads on-demand)
│   ├── zenoh-core/
│   ├── node-management/
│   ├── watchers/
│   ├── data-ops/
│   ├── memory/
│   └── system/
└── src/
    ├── agent.py             # Agent loop (message → LLM → tools → loop)
    ├── prompt_builder.py    # Dynamic system prompt from runtime state
    ├── watcher_engine.py    # LLM-driven data stream monitors
    ├── data_router.py       # Topic → file capture pipelines
    ├── world_model.py       # Live system state from Zenoh
    ├── memory.py            # MEMORY.md + conversation JSONL
    ├── zenoh_bridge.py      # Zenoh client + topic buffer + daemon API
    ├── http_api.py          # REST + WebSocket API
    ├── llm/
    │   ├── provider.py      # LLMProvider protocol
    │   └── openai_compat.py # OpenAI-compatible implementation
    └── tools/
        ├── registry.py      # Tool discovery + execution
        ├── zenoh_tools.py   # subscribe, query, publish
        ├── node_tools.py    # list, start, stop, restart, build, logs
        ├── watcher_tools.py # create, list, remove, pause
        ├── data_tools.py    # save_stream, stop_capture, list_captures
        ├── memory_tools.py  # remember, recall, forget
        └── system_tools.py  # health, world_state, machine_info

Persistent state (~/.bubbaloop/agent/):
├── MEMORY.md                # Agent's persistent learnings
├── watchers.json            # Active watcher definitions (survive restarts)
├── captures.json            # Active data captures (survive restarts)
└── conversations/           # JSONL transcripts per conversation
```

## Capabilities (Tested)

| Capability | Example | Verified |
|-----------|---------|----------|
| Single tool call | "get logs for system-telemetry" → `get_logs` | Yes |
| Multi-tool in one turn | "check health and machine info" → `system_health` + `get_machine_info` | Yes |
| Multi-turn reasoning | "do a full health check" → 2-3 turns of tool calls | Yes |
| 6 tools in one turn | "remove all 5 watchers and stop capture" → 6 tool calls | Yes |
| Conversation continuity | Follow-up questions in same `conversation_id` | Yes |
| Cross-conversation memory | `remember` in conv A, `recall` in conv B | Yes |
| Watcher creation | "watch disk usage every 30s" → `create_watcher` | Yes |
| Watcher with compound conditions | "CPU > 80% OR memory < 50%" in natural language | Yes |
| Data capture | "save weather to /tmp/bubbaloop/" → `save_stream` | Yes |
| Safety: protected nodes | "stop bubbaloop-agent" → refused | Yes |
| Safety: path validation | "save to /etc/shadow" → rejected by tool | Yes |
| Safety: LLM-level refusal | Dangerous requests refused before tool calls | Yes |
| Conditional reasoning | "if disk > 70%, stop non-essential nodes" → checked, acted | Yes |
| Undo actions | "start it back up" → `start_node` in same conversation | Yes |
| WebSocket streaming | Status updates stream in real-time | Yes |
| Concurrent API requests | 4 GET requests in 10ms | Yes |
| Live daemon integration | Queries real daemon, gets real node list | Yes |
| Proto serialization | AgentEvent heartbeats published via protobuf | Yes |

## Known Limitations

### Model Quality vs Speed Tradeoff

On the Jetson Orin Nano (8GB RAM):

| Model | Speed | Tool Calling | Reasoning Quality |
|-------|-------|-------------|-------------------|
| qwen3:1.7b | ~3-5s/call | Works well | Sometimes hallucinates actions not taken |
| qwen3:8b | ~60-120s/call | Better | Much better reasoning, but impractical |

The 1.7B model occasionally:
- **Claims to have done something it didn't** (e.g., says "removed watchers" but only listed them)
- **Over-reasons** — takes 12 tool-call turns when 2-3 would suffice
- **Misinterprets ambiguous requests** — explicit instructions work much better
- Workaround: Be specific in requests ("use remove_watcher on X" vs "clean up")

### Watcher Evaluation Model

The watcher eval LLM uses the same model as the main agent. On resource-constrained
hardware, this means:
- Each watcher evaluation takes 3-10 seconds
- Many concurrent watchers can cause evaluation backlog
- The global rate limit (`max_evaluations_per_minute: 10`) prevents overload
- For production use, a dedicated smaller model or rule-based pre-filter would help

### No Streaming for Watcher Actions

When a watcher takes an action (e.g., stops a node), there is no push notification
to the user. The action is logged in watcher history and visible via `/api/watchers`,
but there's no WebSocket push for watcher events. Users discover actions on next chat
or API poll.

### Conversation Context Window

- Only the last 20 messages are sent to the LLM per conversation
- Long conversations may lose early context
- The system prompt is rebuilt fresh every call (always current), but conversation
  history gets truncated
- Workaround: Use `remember` to persist important information

### Proto Decoding

- Proto decoding relies on matching topic suffix → proto type name via `config.yaml`
- Unknown topics fall back to JSON parsing, then raw string
- Binary data (camera frames, h264) shown as `<binary data, N bytes>`
- No support for dynamic proto discovery — types must be pre-configured

### Single Machine Scope

- The agent connects to one Zenoh endpoint and one daemon
- Multi-machine fleet management would require either:
  - Running an agent per machine
  - Extending the Zenoh bridge to query multiple machine IDs

### Data Capture Limitations

- CSV format assumes consistent dict keys across samples
- No compression for raw/h264 captures
- Max files enforcement is per-capture, not per-directory
- No automatic cleanup of old capture data

### HTTP API

- No authentication — anyone on localhost can chat with the agent
- No rate limiting on the chat endpoint (each request blocks an LLM call)
- WebSocket doesn't support multiple concurrent conversations per connection

## Deployment

```bash
# 1. Install dependencies
pixi install

# 2. Build protos
pixi run build

# 3. Run directly
pixi run run

# 4. Or register with the daemon
bubbaloop node add /path/to/bubbaloop-agent
bubbaloop node start bubbaloop-agent
```

## Customization

### Change Agent Personality

Edit `SOUL.md`:

```markdown
# Bubbaloop Agent

You are the guardian of a Barcelona rooftop garden installation.

## Your Priorities
1. Protect garden sensors from frost damage
2. Capture camera footage during interesting weather events
3. Alert the owner about any hardware issues
```

### Add a New Tool

1. Create `src/tools/my_tools.py`:
```python
from .registry import ToolRegistry, ToolDefinition

def register_my_tools(registry: ToolRegistry, ...):
    async def my_tool(param: str) -> str:
        return f"did something with {param}"

    registry.register(ToolDefinition(
        name="my_tool",
        description="Does something useful.",
        parameters={
            "type": "object",
            "properties": {
                "param": {"type": "string", "description": "..."},
            },
            "required": ["param"],
        },
        handler=my_tool,
        skill="my-skill",
    ))
```

2. Register in `main.py`:
```python
from src.tools.my_tools import register_my_tools
register_my_tools(tool_registry, ...)
```

3. Optionally add `skills/my-skill/SKILL.md` for documentation.

### Use a Different LLM

Edit `config.yaml`:
```yaml
# OpenAI
llm:
  base_url: "https://api.openai.com/v1"
  api_key_env: "OPENAI_API_KEY"
  model: "gpt-4o-mini"

# Anthropic (via proxy)
llm:
  base_url: "http://localhost:8000/v1"  # litellm proxy
  model: "claude-sonnet-4-20250514"

# Local vLLM
llm:
  base_url: "http://localhost:8000/v1"
  model: "Qwen/Qwen2.5-7B-Instruct"
```
