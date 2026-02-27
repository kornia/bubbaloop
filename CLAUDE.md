# Bubbaloop

Physical AI orchestration built on Zenoh. Single binary: CLI + daemon + MCP server.

## Living Documents (update these as architecture evolves)

- **ARCHITECTURE.md** — Layer model, node contract, security, open-core boundary
- **ROADMAP.md** — Migration phases (Track A: sensor-centric, Track B: cloud), checkboxes
- **CONTRIBUTING.md** — Agentic workflows, agent tiers, validation, two-critic loop

## Structure

```
crates/bubbaloop/          # Main binary (CLI + daemon + MCP server)
crates/bubbaloop-node-sdk/ # Node SDK (standalone, NOT in workspace — batteries-included framework)
crates/bubbaloop-schemas/  # Protobuf schemas (standalone, NOT in workspace — never add to workspace)
dashboard/                 # React + Vite + TypeScript
```

Key source files in `crates/bubbaloop/src/`:
- `cli/node.rs` (88KB) — node CRUD, install, precompiled binary download
- `daemon/mod.rs` — skill runtime: registry + lifecycle + health + MCP
- `daemon/node_manager.rs` (57KB) — node lifecycle, build queue, health
- `daemon/systemd.rs` (38KB) — D-Bus/zbus integration
- `daemon/registry.rs` — `~/.bubbaloop/nodes.json` management
- `registry.rs` — marketplace fetch/parse/cache, `find_curl()`
- `marketplace.rs` — shared precompiled binary download logic (used by CLI and MCP)
- `mcp/mod.rs` — MCP tools, BubbaLoopMcpServer<P>, ServerHandler impl
- `mcp/platform.rs` — PlatformOperations trait, DaemonPlatform, MockPlatform

### Node SDK (`crates/bubbaloop-node-sdk/`)

Batteries-included framework for writing nodes. Reduces boilerplate from ~300 to ~50 lines.
- `lib.rs` — `Node` trait, `run_node()`, re-exports (zenoh, prost, tokio, anyhow, log)
- `context.rs` — `NodeContext` (session, scope, machine_id, shutdown_rx, `topic()` helper)
- `config.rs` — Generic YAML config loading
- `zenoh_session.rs` — Client-mode Zenoh session (scouting disabled)
- `health.rs` — Background health heartbeat (5s interval)
- `schema.rs` — Schema queryable (FileDescriptorSet serving)
- `shutdown.rs` — SIGINT/SIGTERM signal handling via watch channel

Standalone crate (NOT in workspace). Nodes depend via git:
`bubbaloop-node-sdk = { git = "https://github.com/kornia/bubbaloop.git", branch = "main" }`

Nodes repo: [bubbaloop-nodes-official](https://github.com/kornia/bubbaloop-nodes-official)

## MCP Server (Core)

MCP is core (not feature-gated). 3-tier RBAC, bearer token auth. Run: `bubbaloop mcp --stdio` or daemon HTTP on :8088.

The daemon is a **passive skill runtime** — AI agents (OpenClaw, Claude, etc.) interact exclusively through MCP. No autonomous decision-making.

Key files in `crates/bubbaloop/src/mcp/`: `mod.rs` (tools, BubbaLoopMcpServer), `platform.rs` (PlatformOperations trait), `rbac.rs`, `auth.rs`.

MCP tools: `install_node` accepts marketplace names (e.g., `"rtsp-camera"`), local paths, or GitHub `user/repo`. Full lifecycle: `install_node`, `uninstall_node`, `clean_node`, `enable_autostart`, `disable_autostart`.

Testing: `cargo test --features test-harness --test integration_mcp` (47 tests)

## Build & Verify

```bash
pixi run check     # cargo check (fast — run after every change)
pixi run clippy    # zero warnings enforced (-D warnings)
pixi run test      # cargo test (300 unit tests)
pixi run fmt       # cargo fmt --all
pixi run build     # cargo build --release (slow on ARM64)
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

**MCP:**
- RBAC: Viewer/Operator/Admin tiers, unknown tools default to Admin
- All tool handlers include audit logging: `log::info!("[MCP] tool=...")`
- PlatformOperations trait for clean daemon/MCP separation
- `test-harness` feature enables integration tests with MockPlatform
- Daemon is a passive skill runtime (no agent rule engine, no autonomous decisions)

## DO / DON'T

**DO:** `pixi run check` after changes | `pixi run fmt && pixi run clippy` before commits | tests with every PR | validate user input | `--` in git clone | verify both `bubbaloop-schemas` and `bubbaloop` compile after proto changes

**DON'T:** run `bubbaloop tui` (needs TTY) | edit `target/`/`OUT_DIR` | commit `.env`/credentials/`target/` | add `bubbaloop-schemas` to workspace | combine `git`+`path` in Cargo deps | `git push --force` to main

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

New view components that decode protobuf MUST gate their subscription callback on schema readiness:
```ts
const schemaReady = useSchemaReady();  // from hooks/useSchemaReady
useZenohSubscription(topic, schemaReady ? handleSample : undefined);
```
This prevents the race where Zenoh delivers messages before `fetchSchemas()` completes.
Exception: views with their own fallback decode chain (like JsonView) don't need gating.

## Pitfalls

- OAuth token lacks `workflow` scope — use SSH for workflow files
- ARM64 release builds are slow — use `pixi run check` first
- Proto changes require rebuilding both `bubbaloop-schemas/` and `bubbaloop` (descriptor.bin is compiled in)
- MCP is core — rmcp, schemars, tower_governor are unconditional deps
- TUI is behind `tui` feature flag (not in default build)
- Logs must go to stderr to avoid corrupting TUI
- Zenoh session: MUST use `"client"` mode for router routing; check `BUBBALOOP_ZENOH_ENDPOINT` env var
- Dashboard schema race: subscriptions start before schemas load — always use `useSchemaReady()` gating
