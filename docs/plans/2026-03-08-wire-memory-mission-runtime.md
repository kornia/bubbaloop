# Wire Memory & Mission Runtime Integration Plan

> **For Claude:** REQUIRED SUB-SKILL: Use superpowers:executing-plans to implement this plan task-by-task.

**Goal:** Connect the v0.0.11 Physical AI Memory & Mission subsystems to the agent runtime so the agent actually sees world state in its prompt, missions are loaded from files, context providers subscribe to Zenoh, and belief decay runs.

**Architecture:** Three focused changes — (1) add `world_state` parameter to prompt builder and call it in `run_agent_turn`, (2) spawn `spawn_belief_decay_task` + `spawn_provider` per agent in `runtime.rs`, (3) spawn `watch_missions_dir` per agent and upsert `.md` files into `MissionStore`.

**Tech Stack:** Rust, tokio, rusqlite (via existing `SemanticStore`/`MissionStore`/`ProviderStore`), existing subsystem entry points — no new dependencies.

---

### Task 1: Wire world state into the system prompt

**Files:**
- Modify: `crates/bubbaloop/src/agent/prompt.rs`
- Modify: `crates/bubbaloop/src/agent/mod.rs`

**Step 1: Add `world_state` param to `build_system_prompt_with_soul_path`**

In `prompt.rs`, change the function signature from:
```rust
pub fn build_system_prompt_with_soul_path(
    soul: &Soul,
    node_inventory: &str,
    active_jobs: &[Job],
    relevant_episodes: &[LogEntry],
    recent_plan: Option<&str>,
    recovered_context: Option<&str>,
    resource_summary: Option<&str>,
    soul_path: Option<&str>,
) -> String {
```
to:
```rust
pub fn build_system_prompt_with_soul_path(
    soul: &Soul,
    node_inventory: &str,
    active_jobs: &[Job],
    relevant_episodes: &[LogEntry],
    recent_plan: Option<&str>,
    recovered_context: Option<&str>,
    resource_summary: Option<&str>,
    soul_path: Option<&str>,
    world_state: &[crate::agent::memory::semantic::WorldStateEntry],
) -> String {
```

Replace the TODO comment (lines 191-192) with:
```rust
    // Inject live world state into every turn — written by context providers,
    // not the LLM. Empty when no context providers are configured.
    let ws_section = format_world_state_section(world_state, 500);
    if !ws_section.is_empty() {
        parts.push(ws_section);
    }
```

Update the `build_system_prompt` wrapper (the 8-arg version) to pass `&[]`:
```rust
pub fn build_system_prompt(
    soul: &Soul,
    node_inventory: &str,
    active_jobs: &[Job],
    relevant_episodes: &[LogEntry],
    recent_plan: Option<&str>,
    recovered_context: Option<&str>,
    resource_summary: Option<&str>,
) -> String {
    build_system_prompt_with_soul_path(
        soul,
        node_inventory,
        active_jobs,
        relevant_episodes,
        recent_plan,
        recovered_context,
        resource_summary,
        None,
        &[],
    )
}
```

**Step 2: Run `pixi run check` to see all call sites that need updating**

```bash
cargo check --lib -p bubbaloop 2>&1 | grep "error\[E"
```

Expected: errors at `build_system_prompt_with_soul_path` call in `mod.rs` and test call sites in `prompt.rs`.

**Step 3: Update `run_agent_turn` in `agent/mod.rs`**

In the block that builds the system prompt (around line 200-242), add world state snapshot inside the existing `backend` lock scope:

```rust
    let (active_jobs, relevant_episodes, recent_plan, recovered_context, world_state) = {
        let backend = memory.backend.lock().await;
        let active_jobs = backend.semantic.pending_jobs().unwrap_or_default();
        let decay_half_life = soul.capabilities.episodic_decay_half_life_days;
        let relevant_episodes = match user_input {
            Some(input) => backend
                .episodic
                .search_with_decay(input, 5, decay_half_life)
                .unwrap_or_default(),
            None => Vec::new(),
        };
        let recent_plan = backend
            .episodic
            .latest_plan()
            .ok()
            .flatten()
            .map(|e| e.content);
        let recovered_context = backend
            .episodic
            .latest_flush()
            .ok()
            .flatten()
            .map(|e| EpisodicLog::strip_flush_prefix(&e.content).to_string());
        let world_state = backend.semantic.world_state_snapshot().unwrap_or_default();
        (
            active_jobs,
            relevant_episodes,
            recent_plan,
            recovered_context,
            world_state,
        )
    };
```

Then pass it to the prompt builder:
```rust
    let system_prompt = prompt::build_system_prompt_with_soul_path(
        soul,
        &inventory,
        &active_jobs,
        &relevant_episodes,
        recent_plan.as_deref(),
        recovered_context.as_deref(),
        resource_summary.as_deref(),
        input.soul_path,
        &world_state,
    );
```

**Step 4: Fix test call sites in `prompt.rs`**

All `build_system_prompt_with_soul_path(...)` calls in tests need `&[]` added as the last argument. The `build_system_prompt(...)` calls do NOT need changing (the wrapper handles it).

Search for them:
```bash
grep -n "build_system_prompt_with_soul_path" crates/bubbaloop/src/agent/prompt.rs
```

For each test call site, add `, &[]` before the closing `)`.

**Step 5: Verify it compiles**

```bash
cargo check --lib -p bubbaloop 2>&1 | tail -3
```
Expected: `Finished`

**Step 6: Run tests**

```bash
cargo test --lib -p bubbaloop -- agent::prompt 2>&1 | tail -5
```
Expected: all pass.

**Step 7: Commit**

```bash
git add crates/bubbaloop/src/agent/prompt.rs crates/bubbaloop/src/agent/mod.rs
git commit -m "feat(agent): inject world state into system prompt every turn"
```

---

### Task 2: Spawn belief decay task per agent

**Files:**
- Modify: `crates/bubbaloop/src/agent/runtime.rs`

**Step 1: Add import at top of `runtime.rs`**

```rust
use crate::daemon::belief_updater::spawn_belief_decay_task;
```

**Step 2: Spawn decay task after `Memory::open`**

In `run_agent_runtime`, after the `Memory::open` block (around line 305), add:

```rust
            // Spawn belief confidence decay — runs hourly, reduces stale belief confidence.
            // db_path = agent_dir/memory.db (same path Memory::open uses)
            let belief_db_path = agent_dir.join("memory.db");
            let belief_decay_shutdown = shutdown_rx.clone();
            tokio::spawn(spawn_belief_decay_task(
                belief_db_path,
                0.9,    // decay_factor: multiply confidence by 0.9 each interval
                3600,   // interval_secs: run hourly
                belief_decay_shutdown,
            ));
```

**Step 3: Verify it compiles**

```bash
cargo check --lib -p bubbaloop 2>&1 | tail -3
```

**Step 4: Run tests**

```bash
cargo test --lib -p bubbaloop -- agent::runtime 2>&1 | tail -5
```

**Step 5: Commit**

```bash
git add crates/bubbaloop/src/agent/runtime.rs
git commit -m "feat(agent): spawn belief decay task per agent at runtime startup"
```

---

### Task 3: Spawn context providers per agent

**Files:**
- Modify: `crates/bubbaloop/src/agent/runtime.rs`

**Step 1: Add imports**

```rust
use crate::daemon::context_provider::{ProviderStore, spawn_provider};
```

**Step 2: Spawn providers after belief decay spawn**

```rust
            // Spawn context providers — daemon background tasks that subscribe to Zenoh
            // topics and write sensor readings into world_state (no LLM involved).
            // Providers are stored in agent_dir/providers.db via configure_context MCP tool.
            let providers_db_path = agent_dir.join("providers.db");
            if providers_db_path.exists() {
                match ProviderStore::open(&providers_db_path) {
                    Ok(store) => match store.list_providers() {
                        Ok(providers) => {
                            log::info!(
                                "[Runtime] Agent '{}': spawning {} context provider(s)",
                                agent_id,
                                providers.len()
                            );
                            for cfg in providers {
                                let semantic_db = agent_dir.join("memory.db");
                                let provider_session = session.clone();
                                let provider_shutdown = shutdown_rx.clone();
                                spawn_provider(cfg, provider_session, semantic_db, provider_shutdown);
                            }
                        }
                        Err(e) => log::warn!(
                            "[Runtime] Agent '{}': failed to list providers: {}",
                            agent_id,
                            e
                        ),
                    },
                    Err(e) => log::warn!(
                        "[Runtime] Agent '{}': failed to open ProviderStore: {}",
                        agent_id,
                        e
                    ),
                }
            }
```

**Step 3: Verify it compiles**

```bash
cargo check --lib -p bubbaloop 2>&1 | tail -3
```

**Step 4: Commit**

```bash
git add crates/bubbaloop/src/agent/runtime.rs
git commit -m "feat(agent): spawn context providers per agent at runtime startup"
```

---

### Task 4: Spawn mission file watcher per agent

**Files:**
- Modify: `crates/bubbaloop/src/agent/runtime.rs`

**Step 1: Add imports**

```rust
use crate::daemon::mission::{Mission, MissionStatus, MissionStore, watch_missions_dir};
```

**Step 2: Spawn mission watcher after context providers**

```rust
            // Spawn mission file watcher — polls agent_dir/missions/ every 5s.
            // New/changed .md files are upserted into MissionStore as Active missions.
            // The filename stem (without .md) becomes the mission ID.
            let missions_dir = agent_dir.join("missions");
            std::fs::create_dir_all(&missions_dir).ok();
            let missions_db_path = agent_dir.join("missions.db");
            let (mission_tx, mut mission_rx) = tokio::sync::mpsc::channel::<String>(16);
            let mission_watcher_shutdown = shutdown_rx.clone();
            tokio::spawn(watch_missions_dir(
                missions_dir.clone(),
                mission_watcher_shutdown,
                mission_tx,
            ));

            // Consume mission IDs from the watcher and upsert into MissionStore
            tokio::spawn(async move {
                while let Some(mission_id) = mission_rx.recv().await {
                    let md_path = missions_dir.join(format!("{}.md", mission_id));
                    let markdown = match std::fs::read_to_string(&md_path) {
                        Ok(s) => s,
                        Err(e) => {
                            log::warn!("[MissionWatcher] Failed to read {}.md: {}", mission_id, e);
                            continue;
                        }
                    };
                    let store = match MissionStore::open(&missions_db_path) {
                        Ok(s) => s,
                        Err(e) => {
                            log::error!("[MissionWatcher] Failed to open MissionStore: {}", e);
                            continue;
                        }
                    };
                    let mission = Mission {
                        id: mission_id.clone(),
                        markdown,
                        status: MissionStatus::Active,
                        expires_at: None,
                        resources: vec![],
                        sub_mission_ids: vec![],
                        depends_on: vec![],
                        compiled_at: std::time::SystemTime::now()
                            .duration_since(std::time::UNIX_EPOCH)
                            .unwrap_or_default()
                            .as_secs() as i64,
                    };
                    // save_mission upserts — existing missions keep their status
                    if let Err(e) = store.save_mission(&mission) {
                        log::error!("[MissionWatcher] Failed to save mission '{}': {}", mission_id, e);
                    } else {
                        log::info!("[MissionWatcher] Loaded mission '{}'", mission_id);
                    }
                }
            });
```

**Step 3: Verify it compiles**

```bash
cargo check --lib -p bubbaloop 2>&1 | tail -3
```

**Step 4: Run all lib tests**

```bash
cargo test --lib -p bubbaloop 2>&1 | tail -5
```
Expected: all pass (the new spawns don't affect existing tests).

**Step 5: Commit**

```bash
git add crates/bubbaloop/src/agent/runtime.rs
git commit -m "feat(agent): spawn mission file watcher per agent at runtime startup"
```

---

### Task 5: Update the tool count in the tools description string

**Files:**
- Modify: `crates/bubbaloop/src/agent/prompt.rs`

**Step 1: Find and update stale tool count**

The prompt currently says "You have 30 tools". Find and update it:

```bash
grep -n "30 tools\|37 tools\|49 tools" crates/bubbaloop/src/agent/prompt.rs
```

Update to reflect reality (49 total: 39 MCP + 10 agent-internal):
```rust
         You have 49 tools across categories:\n\
         - **Node management:** install, build, start, stop, restart, configure, monitor nodes\n\
         - **System:** read and write files, run shell commands\n\
         - **Memory:** search and manage episodic memory, beliefs, world state\n\
         - **Missions:** list, pause, resume, cancel missions\n\
         - **Safety:** register and list constraints\n\
```

**Step 2: Run clippy**

```bash
cargo clippy --lib -p bubbaloop 2>&1 | grep "^error" | head -10
```
Expected: no errors.

**Step 3: Commit**

```bash
git add crates/bubbaloop/src/agent/prompt.rs
git commit -m "chore(agent): update tool count and categories in system prompt"
```

---

### Task 6: End-to-end verification

**Step 1: Run full test suite**

```bash
cargo test --lib -p bubbaloop 2>&1 | tail -5
```
Expected: all pass (≥675).

**Step 2: Run E2E test**

```bash
python3 scripts/e2e-test.py
```
Expected: 24/24 pass.

**Step 3: Run clippy**

```bash
cargo clippy --lib -p bubbaloop 2>&1 | grep "^error"
```
Expected: no output.

**Step 4: Push and update PR**

```bash
git push origin docs/physical-ai-memory-mission
```
