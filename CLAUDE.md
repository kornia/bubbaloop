# 🦐 Bubbaloop

Physical AI orchestration built on Zenoh. Single binary: CLI + daemon + MCP server.

## Living Documents (update these as architecture evolves)

- **ARCHITECTURE.md** — Layer model, node contract, security, open-core boundary
- **ROADMAP.md** — Implementation phases (YAML skills, agent, SQLite memory, scheduling)
- **CONTRIBUTING.md** — Agentic workflows, agent tiers, validation, two-critic loop
- **docs/concepts/dataflow.md** — Provenance envelope, manifest queryable, `bubbaloop dataflow` CLI + MCP tool (how the SDK answers "what's wired, what's flowing")

## Structure

```
crates/bubbaloop/           # Main binary (CLI + daemon + MCP server)
crates/bubbaloop-node/      # Node SDK (standalone, NOT in workspace — batteries-included framework)
crates/bubbaloop-node-build/ # Build helper for nodes (wraps prost-build, standalone)
crates/bubbaloop-schemas/   # Protobuf schemas (standalone, NOT in workspace — never add to workspace)
dashboard/                  # React + Vite + TypeScript
```

Key source files in `crates/bubbaloop/src/`:
- `cli/login.rs` — login/logout/status + `has_claude_credentials()` (env var → OAuth → key file)
- `cli/agent_setup.rs` — `bubbaloop agent setup`: interactive provider/model/identity wizard (no daemon needed)
- `cli/agent_client.rs` — Thin Zenoh CLI client for agent chat/list (pub/sub, no LLM)
- `agent/mod.rs` — Agent core: EventSink trait, run_agent_turn(), AgentTurnInput (soul_path triggers onboarding)
- `agent/gateway.rs` — Zenoh gateway wire format (AgentMessage, AgentEvent, topic builders)
- `agent/runtime.rs` — Multi-agent runtime: AgentsConfig, AgentRuntime, ZenohSink, agent_loop; `agent_directory()` (pub)
- `agent/prompt.rs` — System prompt builder; `build_system_prompt_with_soul_path()` injects onboarding prompt for new agents
- `agent/soul.rs` — Soul struct, first-run onboarding, notify hot-reload (`~/.bubbaloop/soul/`)
- `agent/provider/mod.rs` — ModelProvider trait, Message, ContentBlock, ToolDefinition, StreamEvent
- `agent/provider/claude.rs` — Claude API client with dual auth (API key + OAuth bearer token)
- `agent/provider/ollama.rs` — Ollama local LLM client with tool calling (`/api/chat`)
- `agent/memory/` — 4-tier: world state (live SQLite) + short-term (RAM) + episodic (NDJSON) + semantic (SQLite)
- `agent/heartbeat.rs` — Adaptive heartbeat: arousal + decay + state collection
- `agent/dispatch.rs` — Internal MCP tool dispatch (42 MCP + agent-internal tools, includes telemetry + beliefs + constraints + missions)
- `cli/node/mod.rs` — node CRUD, validation, list/add/remove
- `cli/node/install.rs` — install, precompiled binary download, GitHub clone
- `cli/node/lifecycle.rs` — start, stop, restart, logs
- `cli/node/build.rs` — build node
- `daemon/mod.rs` — skill runtime: registry + lifecycle + health + MCP + telemetry watchdog
- `daemon/telemetry/` — Resource watchdog: sampler (sysinfo), circuit breaker, SQLite storage, hot-reload config
- `daemon/node_manager.rs` (57KB) — node lifecycle, build queue, health
- `daemon/systemd.rs` (38KB) — D-Bus/zbus integration
- `daemon/registry.rs` — `~/.bubbaloop/nodes.json` management
- `registry.rs` — marketplace fetch/parse/cache, `find_curl()`
- `marketplace.rs` — shared precompiled binary download logic (used by CLI and MCP)
- `mcp/mod.rs` — MCP tools, BubbaLoopMcpServer<P>, ServerHandler impl
- `mcp/platform.rs` — PlatformOperations trait, DaemonPlatform, MockPlatform

### Node SDK (`crates/bubbaloop-node/`)

Batteries-included framework for writing nodes. Reduces boilerplate from ~300 to ~50 lines.
- `lib.rs` — `Node` trait, `run_node()`, re-exports (zenoh, prost, tokio, anyhow, log, serde_json)
- `context.rs` — `NodeContext` (session, machine_id, **instance_name**, shutdown_rx, `topic()`, `local_topic()`, `publisher_proto()`, `publisher_json()`, `publisher_raw()`, `publisher_raw_proto()`, `subscriber()`, `subscriber_raw()`)
- `publisher.rs` — `ProtoPublisher<T>` (APPLICATION_PROTOBUF + schema suffix), `JsonPublisher` (APPLICATION_JSON), `RawPublisher` (ZBytes, optional encoding, SHM congestion control)
- `subscriber.rs` — `TypedSubscriber<T>` (auto-decode, 256-slot FIFO), `RawSubscriber` (raw ZBytes, 4-slot FIFO)
- `proto_decoder.rs` — `ProtoDecoder` (dynamic protobuf decode via SchemaRegistry, caches DescriptorPool)
- `discover.rs` — `discover_nodes()`, `NodeInfo` (discovers nodes via health heartbeats)
- `get_sample.rs` — `get_sample()` (single-shot pull without maintaining subscription)
- `config.rs` — YAML config loading; `extract_name()` reads `name` field for per-instance topics
- `zenoh_session.rs` — Client-mode Zenoh session (scouting disabled)
- `health.rs` — Background health heartbeat (5s interval) to `{instance_name}/health`
- `schema.rs` — Schema queryable at `{instance_name}/schema` (FileDescriptorSet serving)
- `shutdown.rs` — SIGINT/SIGTERM signal handling via watch channel

Standalone crates (NOT in workspace). Nodes depend via git:
```toml
[dependencies]
bubbaloop-node = { git = "https://github.com/kornia/bubbaloop.git", branch = "main" }
[build-dependencies]
bubbaloop-node-build = { git = "https://github.com/kornia/bubbaloop.git", branch = "main" }
```

**Rust ↔ Python SDK parity (MUST hold):** `crates/bubbaloop-node/` (Rust) and `python-sdk/` (Python) are peer APIs, not layered — every publisher/subscriber/context method added to one MUST have an equivalent in the other in the same PR (or a linked tracking issue). Names should match where possible; where the underlying binding can't surface a knob, the Python side collapses it to the simplest equivalent that preserves wire behavior. Example: Rust `publisher_cbor_shm(suffix, slot_count, slot_size)` ↔ Python `publisher_cbor(suffix, local=True)` — `zenoh-python` doesn't expose `ShmProvider` so slot sizing is implicit, but encoding (`application/cbor`) and congestion control (`Block`) are identical on the wire.

**Multi-instance support:** SDK reads `name` from config YAML at startup. Health and schema topics use this name, so `tapo_entrance` and `tapo_terrace` instances of the same binary get separate topics: `bubbaloop/global/host/tapo_entrance/health` vs `bubbaloop/global/host/tapo_terrace/health`.

**Topic key spaces:** Two fixed key spaces replace the old `{scope}`:
- `ctx.topic(suffix)` → `bubbaloop/global/{machine_id}/{suffix}` (network-visible, dashboard, CLI)
- `ctx.local_topic(suffix)` → `bubbaloop/local/{machine_id}/{suffix}` (SHM-only, same-machine, never crosses WebSocket bridge)

### Node Build Helper (`crates/bubbaloop-node-build/`)

Companion build crate (tonic-build pattern). Node `build.rs` is one line:
```rust
fn main() -> Result<(), Box<dyn std::error::Error>> {
    bubbaloop_node_build::compile_protos(&["protos/my_node.proto"])
}
```
Automatically: embeds `header.proto` (no local copy needed), maps `.bubbaloop.header.v1` → `::bubbaloop_node::schemas::header::v1`, writes `descriptor.bin` for schema queryable registration.

Python SDK: `python-sdk/` — synchronous Python wrapper over zenoh-python (no asyncio). Exports: `NodeContext`, `run_node`, `JsonPublisher`, `ProtoPublisher`, `RawPublisher`, `ProtoSubscriber` (auto-decode via SchemaRegistry), `RawSubscriber`, `ProtoDecoder`, `discover_nodes`, `get_sample`. Install:
`pip install git+https://github.com/kornia/bubbaloop.git#subdirectory=python-sdk`

Nodes repo: [bubbaloop-nodes-official](https://github.com/kornia/bubbaloop-nodes-official)

## MCP Server (Core)

MCP is core (not feature-gated). 3-tier RBAC, bearer token auth. Run: `bubbaloop mcp --stdio` or daemon HTTP on :8088.

The daemon runs the **agent runtime** (multi-agent Zenoh gateway) alongside the MCP server. Agents are configured via `~/.bubbaloop/agents.toml`, each with per-agent Soul and Memory in `~/.bubbaloop/agents/{id}/`. The CLI (`bubbaloop agent chat`) is a thin Zenoh client — all LLM processing is daemon-side.

Key files in `crates/bubbaloop/src/mcp/`: `mod.rs` (tools, BubbaLoopMcpServer), `platform.rs` (PlatformOperations trait), `rbac.rs`, `auth.rs`.

MCP tools: `install_node` accepts marketplace names (e.g., `"rtsp-camera"`), local paths, or GitHub `user/repo`. Full lifecycle: `install_node`, `uninstall_node`, `clean_node`, `enable_autostart`, `disable_autostart`.

Testing: `cargo test --features test-harness --test integration_mcp` (47 tests)

## Build & Verify

```bash
pixi run check     # cargo check (fast — run after every change)
pixi run clippy    # zero warnings enforced (-D warnings)
pixi run test      # cargo test (2400+ unit tests)
pixi run fmt       # cargo fmt --all
pixi run build     # cargo build --release (slow on ARM64; install mold+clang to speed up)
cargo test --features test-harness --test integration_mcp  # 47 integration tests
```

## Conventions — MUST follow

**Use these (not alternatives):**
- `argh` for CLI — NOT clap (`#[derive(FromArgs)]`, `#[argh(subcommand)]`)
- `log` + `env_logger` — NOT tracing (`log::info!()`, logs to stderr)
- `thiserror` for library errors, `anyhow` for CLI — module-specific `type Result<T>`
- `zbus` for systemd — NEVER spawn `systemctl` as subprocess
- `println!()` only for CLI user output — all other output via `log` macros
- `tempfile::tempdir()` for filesystem tests, co-located `#[cfg(test)] mod tests`
- 100% safe Rust — no `unsafe`

**Async patterns:**
- tokio with `watch::channel(())` shutdown pattern
- Zenoh queries: 3 retries, 1s timeout, `QueryTarget::BestMatching`

**Security:**
- `find_curl()` in `marketplace.rs` and `registry.rs` searches `/usr/bin`, `/usr/local/bin`, `/bin` only (no PATH)
- Build commands: allowlist prefixes only (cargo, pixi, npm, make, python, pip)
- Node names: 1-64 chars, `[a-zA-Z0-9_-]`, no null bytes
- Git clone: always use `--` separator
- Bind localhost only, never `0.0.0.0`
- `kill <numeric_pids>` allowed (agent lifecycle cleanup); `kill 0/1/-1`, `killall`, `pkill` blocked

**MCP:**
- RBAC: Viewer/Operator/Admin tiers, unknown tools default to Admin
- All tool handlers include audit logging: `log::info!("[MCP] tool=...")`
- PlatformOperations trait for clean daemon/MCP separation
- `test-harness` feature enables integration tests with MockPlatform
- Daemon hosts agent runtime (multi-agent Zenoh gateway) + MCP server

### Topic naming (auto-scoped)

Every `ctx.topic(suffix)` / `ctx.local_topic(suffix)` call auto-prefixes with `instance_name`:

```
bubbaloop/{global|local}/{machine_id}/{instance_name}/{suffix}
```

- **Publishers** (`publisher_*(suffix)`) auto-scope: `ctx.publisher_json("data")` → `bubbaloop/global/{machine_id}/{instance_name}/data`. Use `publisher_*_absolute(suffix)` only for explicit cross-node control topics (e.g. daemon broadcast channels).
- **Subscribers** (`subscribe(absolute_suffix)`) are **absolute by default** — pass the upstream node's full topic suffix including its instance name, e.g. `ctx.subscribe("tapo_terrace_embedder/embeddings")`.
- **Asymmetry rationale**: publishers own their outputs (auto-scope makes sense), subscribers consume upstream (absolute avoids silent self-scoping bugs).
- **Three roles** (`spec.role:` in `node.yaml v2`): `source` (raw sensor), `processor` (derived signal), `sink` (actuator/store). Processors inherit their source's instance prefix: `tapo_terrace_embedder` derives from `tapo_terrace`.
- **Instance-naming convention**: `<source>[_<purpose>]` — e.g. `tapo_terrace`, `tapo_terrace_embedder`, `tapo_terrace_detector`.
- **`bubbaloop-nodes-official` layout** (future restructure, not yet done): group directories by role — `sources/`, `processors/`, `sinks/`.
- Every node served by the SDK exposes `{instance}/manifest` (CBOR queryable) listing its live `inputs`/`outputs` (each with per-topic `{ever_fired, still_live, declared_at_ns}` liveness) + `role`; `bubbaloop dataflow` (and the `dataflow` MCP tool) reconstruct the full DAG from these — no config grepping required. Default edge inference requires `still_live && ever_fired`; pass `include_declared_but_unused=true` to see wired-but-idle pipes.

## DO / DON'T

**DO:** `pixi run check` after changes | `pixi run fmt && pixi run clippy` before commits | tests with every PR | validate user input | `--` in git clone | verify both `bubbaloop-schemas` and `bubbaloop` compile after proto changes

**DON'T:** edit `target/`/`OUT_DIR` | commit `.env`/credentials/`target/` | add `bubbaloop-schemas` to workspace | combine `git`+`path` in Cargo deps | `git push --force` to main | **commit directly to main** (always use a branch + PR)

## Zenoh API Rules (templates & nodes)

- **ALL nodes MUST use `mode: "client"`** — peer mode doesn't route through zenohd router
- **NEVER** use `.complete(true)` on queryables — blocks wildcard queries like `bubbaloop/**/schema`
- **Python**: `query.key_expr` is a **property** NOT a method — NEVER use `query.key_expr()`
- **Python**: `query.reply(query.key_expr, payload_bytes)` — correct reply pattern
- **Rust**: Do NOT use `.complete(true)` — use `.await?` directly
- **Validation**: Run `./scripts/validate.sh` to catch these errors automatically

## Commits

Conventional: `feat:`, `fix:`, `chore:`, `docs:`, `refactor:`, `test:`
Never commit: `target/`, `node_modules/`, `dist/`, `.pixi/`
Always commit: `Cargo.lock`, `pixi.lock`, `package-lock.json`

## Dashboard View Components

### Encoding-First Decode (preferred)

New view components should read `sample.encoding()` as the primary decode signal:
```ts
import { getEncodingInfo } from '../lib/zenoh';

const handleSample = (sample: Sample) => {
  const encoding = getEncodingInfo(sample);
  // APPLICATION_JSON (5) → snakeToCamel(JSON.parse()) — nodes publish snake_case, dashboard normalizes
  // APPLICATION_PROTOBUF (13) → SchemaRegistry decode (with on-demand schema fetch)
  // ZENOH_BYTES (0) or unknown → sniff fallback
};
```

### Legacy Gating (backward compat for old nodes)

For views that must support old nodes publishing without encoding:
```ts
const schemaReady = useSchemaReady();  // from hooks/useSchemaReady
useZenohSubscription(topic, schemaReady ? handleSample : undefined);
```
This prevents the race where Zenoh delivers messages before schemas load.
Exception: views using encoding-first decode don't need gating.

## Pitfalls

- OAuth token lacks `workflow` scope — use SSH for workflow files
- ARM64 release builds are slow — use `pixi run check` first
- Proto changes require rebuilding both `bubbaloop-schemas/` and `bubbaloop` (descriptor.bin is compiled in)
- MCP is core — rmcp, schemars, tower_governor are unconditional deps
- TUI was removed in v0.0.6, re-added in v0.0.11 as ratatui chat REPL (`agent chat`). Single-message mode stays plain stdout.
- `dashboard` feature is opt-in (not default) — use `--features dashboard` to build the web UI
- Binary size profile: `panic=abort` + `lto=true` + `opt-level=z` + `strip=symbols` + `rusqlite bundled` (not bundled-full)
- mold linker config ready in `.cargo/config.toml` — activate with `sudo apt install mold clang`
- Logs must go to stderr (convention: never pollute stdout)
- Zenoh session: MUST use `"client"` mode for router routing; check `BUBBALOOP_ZENOH_ENDPOINT` env var
- Dashboard schema race: for old nodes without encoding, use `useSchemaReady()` gating. New nodes with encoding decode on first sample.
- Daemon wire format: ALL daemon messages are JSON (not protobuf). Dashboard uses `JSON.parse()` for daemon responses.
- Zenoh default encoding: samples published without explicit encoding get `ZENOH_BYTES` (id=0). Dashboard treats this as "no signal" → sniff fallback.
- OAuth tokens require Claude CLI identity headers (user-agent, x-app, anthropic-beta) — see `agent/provider/claude.rs::OAUTH_BETA_HEADERS`
- Agent robustness constants in `agent/mod.rs`: `TURN_TIMEOUT_SECS=120`, `TOOL_CALL_TIMEOUT_SECS=30`, `MAX_TOOL_RESULT_CHARS=4096` — change these to tune agent behavior
- `run_turn_loop()` is the inner async fn wrapped by `tokio::time::timeout` — keep turn-level logic there, finalization (Done event, trim) in `run_agent_turn()`
- Agent ID vs display name: agent ID (`agents.toml` key) is the routing/filesystem key — immutable. The display name lives in `identity.md` (hot-reloaded). Edit `identity.md` to rename an agent without touching the ID.
- `agent setup` onboarding flow: writes `.needs-onboarding` marker in `~/.bubbaloop/agents/{id}/`. On first chat turn, daemon injects `NEW_AGENT_IDENTITY` prompt; LLM writes `identity.md`; daemon clears marker. `needs_onboarding` bool is cached in `agent_loop` to avoid hot-path `Path::exists()` calls.
- `agents.toml` `model` field overrides `Soul.capabilities.model_name` per-agent. Omit to use soul default.
- Topic key spaces: `global` = network-visible (dashboard, CLI, remote machines), `local` = SHM-only (same-machine, never crosses WebSocket bridge). The old `{scope}` (local/staging/prod) was removed — all topics now use `bubbaloop/global/...` or `bubbaloop/local/...`.
- `RawPublisher` with `local=true` uses `CongestionControl::Block` — required for SHM so frames aren't silently dropped. Without it, slow consumers lose frames.
- `RawSubscriber` uses a 4-slot FIFO (not 256 like `TypedSubscriber`) — older frames are intentionally dropped when the consumer is slow. This is correct for video frames.
- Python `ProtoSubscriber` auto-decodes via `SchemaRegistry` (queries `bubbaloop/**/schema`, 2s timeout). No `_pb2` imports needed — schema is fetched from the publishing node at runtime.
- Python SDK subscribers use `_BaseSubscriber` for shared iterator protocol. Only override `recv()` in subclasses.
