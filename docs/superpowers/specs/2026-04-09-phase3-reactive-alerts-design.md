# Phase 3: Reactive Alert Wiring — Design Spec

## Goal

Connect the existing reactive alert system so that when world state matches a registered alert rule, the agent's arousal is boosted and it wakes up faster. Minimal change — one file, ~30 lines.

## Background

All components already exist but are disconnected:

| Component | File | Status |
|---|---|---|
| `ReactiveRuleStore` (alerts.db) | `daemon/reactive.rs` | Complete |
| `evaluate_rules(rules, world_state) → f64` | `daemon/reactive.rs` | Complete |
| `ArousalState::add_external_boost(boost)` | `agent/heartbeat.rs` | Complete |
| `spawn_provider` (world state writer) | `daemon/context_provider.rs` | Complete |
| `register_alert` MCP tool | `mcp/daemon_platform.rs` | Complete |
| **Wiring in `agent_loop`** | `agent/runtime.rs` | **Missing** |

## Design

### Single file change: `agent/runtime.rs`

#### 1. Load rules at startup (~line 715)

After existing agent_loop setup, load reactive rules from `alerts.db`:

```rust
let alerts_db_path = agent_dir.join("alerts.db");
let reactive_rules: Vec<ReactiveRule> = ReactiveRuleStore::open(&alerts_db_path)
    .and_then(|s| s.list_rules())
    .map(|configs| configs.into_iter().map(Into::into).collect())
    .unwrap_or_default();
let mut tick_count: u64 = 0;
```

#### 2. Evaluate on heartbeat tick (~line 811)

In the `poll_jobs` block, before `arousal.update(&state)`:

```rust
// Phase 3: evaluate reactive rules against world state
tick_count += 1;

// Refresh rules from DB every 10 ticks to pick up new register_alert calls
if tick_count % 10 == 0 {
    if let Ok(store) = ReactiveRuleStore::open(&alerts_db_path) {
        if let Ok(configs) = store.list_rules() {
            // Preserve existing rules with their debounce state,
            // only add genuinely new rules
            // (simplified: full reload is acceptable since debounce
            //  uses wall-clock comparison, not in-memory state)
            reactive_rules = configs.into_iter().map(Into::into).collect();
        }
    }
}

let ws_entries = {
    let backend = memory.backend.lock().await;
    backend.semantic.world_state_snapshot().unwrap_or_default()
};
let ws_map: std::collections::HashMap<&str, &str> = ws_entries.iter()
    .map(|e| (e.key.as_str(), e.value.as_str()))
    .collect();
if !ws_map.is_empty() && !reactive_rules.is_empty() {
    let boost = evaluate_rules(&reactive_rules, &ws_map);
    if boost > 0.0 {
        arousal.add_external_boost(boost);
        log::info!("[agent:{}] reactive alert boost: {:.2}", agent_id, boost);
    }
}
```

### What does NOT change

- `reactive.rs` — untouched
- `heartbeat.rs` — untouched
- `context_provider.rs` — untouched
- `prompt.rs` — untouched (no triggered alerts section, saves tokens)
- `daemon_platform.rs` — untouched

### Debounce handling

`ReactiveRule::should_fire()` uses `AtomicU64` for `last_fired_at`. Since we reload rules every 10 ticks, debounce state resets on reload. This is acceptable because:
- Debounce is typically 30-60s, tick interval is 5-60s
- Reload happens every 10 ticks = 50-600s apart
- Worst case: one extra alert fire after reload, then debounce kicks in again

For stricter debounce, a future improvement could persist `last_fired_at` in SQLite.

### Flow

```
sensor node → publishes data to Zenoh
configure_context → spawn_provider subscribes, writes to world_state (SQLite)
heartbeat tick → reads world_state snapshot
              → evaluate_rules checks predicates
              → boost > 0 → add_external_boost
              → next tick interval shrinks (60s → 5s)
              → agent turn runs, sees world state, reasons
```

### End-to-end usage

```bash
# 1. Wire motion score into world state
bubbaloop agent chat "configure_context for motion/score topic"

# 2. Register alert rule
bubbaloop agent chat "register_alert when motion_score > 0.03"

# 3. Motion happens → world state updated → rule fires → agent wakes
```

## Testing

- `cargo check` — compile verification
- `cargo test` — existing tests pass (change is additive, no-op when alerts.db is empty)
- Manual: register alert via MCP, trigger motion, verify agent wakes within one tick

## Scope

One file, ~30 lines of new code. No schema changes. No new dependencies. No new async tasks. Fully backward compatible — empty `alerts.db` or missing file means zero boost, no change in behavior.
