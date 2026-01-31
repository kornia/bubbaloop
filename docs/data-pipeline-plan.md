# Track 2: Data Pipeline & Dashboard Infrastructure

How agent data gets from the Rust agent to the React dashboard for visualization.

## Problem

The agent accumulates valuable operational data (incidents, conversations, actions, standing orders, circuit breaker states). Without visibility into this, the dashboard user has no idea what the agent is doing, why, or whether it's working correctly.

## Design Principles

1. **Zenoh-native**: Agent publishes state via Zenoh pub/sub (same as daemon/nodes)
2. **Protobuf-encoded**: Type-safe, compact, matches existing dashboard patterns
3. **Polling from dashboard**: Use GET queries (same pattern as NodesView) - reliable through WS bridge
4. **Minimal panel first**: One "Agent" panel, expand later
5. **No new backend services**: Agent publishes directly, dashboard subscribes directly

## Data Flow

```
bubbaloop-agent (Rust)
    |
    +-- Publishes every 5s: bubbaloop/{machine_id}/agent/state  (AgentState protobuf)
    +-- Publishes on event:  bubbaloop/{machine_id}/agent/events (AgentEvent protobuf)
    +-- Declares queryable:  bubbaloop/{machine_id}/agent/query  (request-reply)
        |
        v
    zenohd router (:7447)
        |
        v
    zenoh-bridge-remote-api (WS :10001)
        |
        v
    Dashboard (React)
        |
        +-- AgentView panel: polls agent/state every 3s (GET query, same as NodesView)
        +-- Subscribes to agent/events for real-time activity feed
        +-- On-demand queries: incidents, conversations, audit log
```

## Protobuf Schema (`protos/bubbaloop/agent.proto`)

```protobuf
syntax = "proto3";
package bubbaloop.agent.v1;

// Published every 5s on bubbaloop/{machine}/agent/state
message AgentState {
  string name = 1;
  string machine_id = 2;
  AgentStatus status = 3;
  int64 uptime_seconds = 4;
  int64 last_interaction_ms = 5;

  // Channels
  bool telegram_connected = 6;
  bool console_active = 7;

  // Memory stats
  int32 total_incidents = 8;
  int32 total_conversations = 9;
  int32 total_patterns = 10;
  int32 total_strategies = 11;

  // Active state
  repeated StandingOrderSummary standing_orders = 12;
  repeated CircuitBreakerState circuit_breakers = 13;
  repeated RecentAction recent_actions = 14;  // last 20
}

enum AgentStatus {
  AGENT_STATUS_UNKNOWN = 0;
  AGENT_STATUS_IDLE = 1;
  AGENT_STATUS_THINKING = 2;    // Claude API call in progress
  AGENT_STATUS_ACTING = 3;      // Executing tool call
  AGENT_STATUS_ERROR = 4;
}

message StandingOrderSummary {
  string id = 1;
  string description = 2;       // "Keep all cameras running"
  int64 created_ms = 3;
  int32 times_triggered = 4;
}

message CircuitBreakerState {
  string node_name = 1;
  bool is_open = 2;             // true = tripped, not restarting
  int32 failure_count = 3;
  int64 last_failure_ms = 4;
  int64 resets_at_ms = 5;       // when circuit closes again
}

message RecentAction {
  int64 timestamp_ms = 1;
  string user = 2;              // "alice" or "system" (proactive)
  string action = 3;            // "restart_node"
  string target = 4;            // "rtsp-camera"
  bool success = 5;
  string summary = 6;           // "Restarted rtsp-camera after GStreamer failure"
}

// Published on bubbaloop/{machine}/agent/events (per event)
message AgentEvent {
  int64 timestamp_ms = 1;
  AgentEventType event_type = 2;
  string description = 3;
  string user = 4;
  string node_name = 5;         // if applicable
  map<string, string> metadata = 6;
}

enum AgentEventType {
  AGENT_EVENT_UNKNOWN = 0;
  AGENT_EVENT_INTERACTION = 1;    // User asked something
  AGENT_EVENT_TOOL_CALL = 2;     // Agent called a tool
  AGENT_EVENT_INCIDENT = 3;      // Failure detected
  AGENT_EVENT_RESOLUTION = 4;    // Problem resolved
  AGENT_EVENT_ESCALATION = 5;    // Agent escalated to user
  AGENT_EVENT_CIRCUIT_OPEN = 6;  // Circuit breaker tripped
  AGENT_EVENT_ORDER_CREATED = 7; // New standing order
}
```

## Dashboard: AgentView Panel

New file: `dashboard/src/components/AgentView.tsx`

Follows the exact same patterns as `NodesView.tsx`:
- `SortableAgentCard` wrapper for drag-and-drop
- `AgentViewPanel` inner component
- Polls `bubbaloop/*/agent/state` every 3s via Zenoh GET
- Decodes with `decodeAgentState()` from new `proto/agent.ts`

### Panel Layout

```
+---------------------------------------------------+
| * Agent                              [idle] [x]   |  <-- header (draggable)
+---------------------------------------------------+
|                                                   |
|  Status: * Idle    Uptime: 3h 42m                 |
|  Telegram: * Connected   Last interaction: 2m     |
|  Memory: 47 incidents, 12 patterns, 8 strategies  |
|                                                   |
|  -- Standing Orders ----------------------------  |
|  * Keep all cameras running      (triggered 3x)   |
|  * Monitor inference health      (triggered 0x)   |
|                                                   |
|  -- Circuit Breakers ---------------------------  |
|  ! rtsp-camera  OPEN (3 failures, resets 12:30)   |
|                                                   |
|  -- Recent Activity ----------------------------  |
|  12:28  system  restart_node  rtsp-camera  ok     |
|  12:25  system  restart_node  rtsp-camera  ok     |
|  12:22  alice   list_nodes    -            ok     |
|  12:20  system  restart_node  rtsp-camera  ok     |
|  12:15  bob     get_logs      openmeteo    ok     |
|                                                   |
+---------------------------------------------------+
```

### Sections

| Section | Data Source | Update |
|---------|-----------|--------|
| **Status bar** | `AgentState.status`, `uptime_seconds`, `telegram_connected` | 3s poll |
| **Memory stats** | `AgentState.total_incidents/patterns/strategies` | 3s poll |
| **Standing orders** | `AgentState.standing_orders` | 3s poll |
| **Circuit breakers** | `AgentState.circuit_breakers` (only shown when non-empty) | 3s poll |
| **Recent activity** | `AgentState.recent_actions` (last 20) | 3s poll |

## Files to Create/Modify

### New files (Track 2)

| File | Description |
|------|-------------|
| `protos/bubbaloop/agent.proto` | Agent protobuf schema |
| `dashboard/src/proto/agent.ts` | Generated types + decode helpers |
| `dashboard/src/components/AgentView.tsx` | Agent dashboard panel |

### Modifications

| File | Change |
|------|--------|
| `dashboard/src/components/Dashboard.tsx` | Add `'agent'` to PanelType, add AgentView to panel renderer |
| `crates/bubbaloop-agent/src/main.rs` | Spawn Zenoh publisher task for AgentState |

### Agent-side publishing (in Track 1 code)

A `StatePublisher` struct in the agent that:
1. Collects current status, memory stats, standing orders, circuit breaker states
2. Serializes as protobuf `AgentState`
3. Publishes to `bubbaloop/{machine_id}/agent/state` every 5s
4. Publishes `AgentEvent` on every action/incident/escalation

This is a thin layer on top of Track 1's existing data structures - no new storage, just serialization and publishing.

## Implementation Order

1. Define `protos/bubbaloop/agent.proto`
2. Generate TS types in `dashboard/src/proto/agent.ts`
3. Build `AgentView.tsx` panel (can use mock data initially)
4. Register panel type in `Dashboard.tsx`
5. Add `StatePublisher` to agent crate (wires into Track 1's memory/safety/reactive modules)
6. Test end-to-end: agent running -> dashboard shows live state

## Verification

1. Start zenohd + daemon + agent (console mode)
2. Open dashboard, add Agent panel
3. Send agent a message via console -> see "Recent Activity" update
4. Create standing order -> see it appear in panel
5. Trigger circuit breaker (rapid restarts) -> see it in panel
6. Agent status cycles: idle -> thinking -> acting -> idle
