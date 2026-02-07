# Bubbaloop

AI-native orchestration for Physical AI, built on Zenoh.

## Quick Start

```bash
zenohd &                                                          # 1. Zenoh router
zenoh-bridge-remote-api --ws-port 10001 -e tcp/127.0.0.1:7447 &  # 2. WebSocket bridge
pixi run daemon                                                   # 3. Daemon
pixi run dashboard                                                # 4. Dashboard (another terminal)
```

## Architecture

```
Dashboard / TUI / MCP  →  zenoh-bridge-remote (WS :10001)  →  zenohd (:7447)
                                                                    │
                    ┌───────────────────┬───────────────────────────┤
              bubbaloop daemon    rtsp-camera    openmeteo    system-telemetry ...
```

- **zenohd**: Central router, all peers connect on `:7447`
- **zenoh-bridge-remote-api**: WebSocket bridge for browser dashboard
- **bubbaloop daemon**: Node lifecycle manager (systemd/D-Bus)
- **Nodes**: Standalone processes (Rust or Python) that publish/subscribe via Zenoh

### Directory Structure

```
crates/
├── bubbaloop/           # Single binary: CLI + TUI + daemon
└── bubbaloop-schemas/   # Protobuf schemas (standalone, not in workspace)
dashboard/               # React dashboard (Vite + TypeScript)
configs/                 # Zenoh configuration files
docs/                    # MkDocs documentation site
```

Official nodes live in [bubbaloop-nodes-official](https://github.com/kornia/bubbaloop-nodes-official).

### Source Code Map (`crates/bubbaloop/src/`)

```
bin/bubbaloop.rs         # Entry point: CLI arg dispatch
lib.rs                   # Library root: protobuf schemas, descriptor utilities
cli/
  node.rs                # Node CRUD: init, add, instance, build, start, stop
  doctor.rs              # System diagnostics
  debug.rs               # Zenoh debugging (topics, subscribe, query)
  marketplace.rs         # Marketplace source management
  status.rs              # Non-interactive status display
daemon/
  node_manager.rs        # Core: node lifecycle, build queue, health monitoring
  registry.rs            # Node registry: ~/.bubbaloop/nodes.json
  systemd.rs             # systemd integration via D-Bus (zbus)
  zenoh_api.rs           # Zenoh queryable handlers
  zenoh_service.rs       # Pub/sub for state broadcasting
tui/                     # ratatui terminal UI
```

## Pixi Tasks

```bash
pixi run up              # Launch all services via process-compose
pixi run build           # cargo build --release
pixi run daemon          # Run daemon
pixi run tui             # Run TUI
pixi run dashboard       # Run React dashboard dev server

pixi run check           # cargo check (fast)
pixi run test            # cargo test
pixi run fmt             # cargo fmt --all
pixi run clippy          # cargo clippy (with -D warnings)
pixi run lint            # fmt-check + clippy

pixi run dashboard-install   # npm install
pixi run dashboard-proto     # Generate protobuf TS bindings
pixi run dashboard-build     # Production build
```

## CLI Quick Reference

```bash
bubbaloop status [-f json]              # Service status
bubbaloop doctor [--fix]                # Diagnostics
bubbaloop daemon [-z <endpoint>]        # Run daemon
bubbaloop node init|add|list|build|install|start|stop|logs|remove <name>
bubbaloop node instance <base> <suffix> # Multi-instance node
bubbaloop debug topics|subscribe|query  # Zenoh debugging
bubbaloop marketplace list|add|remove   # Node sources
```

## Zenoh Topics

```
bubbaloop/{scope}/{machine}/daemon/api/health|nodes|command|schemas  # Daemon API (query/reply)
bubbaloop/{scope}/{machine}/daemon/nodes                             # Node state (pub/sub)
bubbaloop/{scope}/{machine}/health/{node-name}                       # Health heartbeats
bubbaloop/{scope}/{machine}/{node-name}/schema                       # Node schema (query/reply)
bubbaloop/{scope}/{machine}/camera/{name}/compressed                 # Camera frames
bubbaloop/{scope}/{machine}/weather/current|hourly|daily             # Weather data
```

## Protobuf Schemas

Source of truth: `crates/bubbaloop-schemas/protos/`. Key types:

| Proto | Key Types |
|-------|-----------|
| `header.proto` | `Header` (timestamps, frame_id, seq, machine_id, scope) |
| `camera.proto` | `CompressedImage`, `RawImage` |
| `weather.proto` | `CurrentWeather`, `HourlyForecast`, `DailyForecast` |
| `daemon.proto` | `NodeState`, `NodeStatus`, `NodeCommand` |
| `system_telemetry.proto` | `SystemMetrics`, `CpuMetrics`, etc. |
| `network_monitor.proto` | `NetworkStatus`, `HealthCheck` |

`bubbaloop-schemas` is standalone (not in workspace). Both the main crate and nodes depend on it.
Nodes also carry local copies of their protos in `protos/` for `descriptor.bin` generation.

## Key Dependencies

| Crate | Purpose |
|-------|---------|
| `zenoh` 1.7 | Pub/sub messaging, query/reply |
| `ros-z` (git) | ROS 2 compatibility layer over Zenoh |
| `prost` / `prost-build` 0.14 | Protobuf serialization and codegen |
| `ratatui` 0.29 | Terminal UI |
| `zbus` | D-Bus client for systemd |
| `tokio` 1.0 (full) | Async runtime |

## Agent Guidelines

### Coding Style

- **Rust edition 2021**, async/await with tokio
- **Error handling**: `thiserror` for libraries, `anyhow` for applications
- **Naming**: snake_case files/functions, CamelCase types, SCREAMING_SNAKE constants
- **Logging**: `log::info!` etc. for operational messages; `println!` only for CLI user output
- **Tests**: co-located `#[cfg(test)] mod tests` blocks, `tempfile` for filesystem tests

### DO

- Run `pixi run check` after Rust changes (fast compile check)
- Run `pixi run fmt && pixi run clippy` before commits
- Include tests with every PR
- Validate node names, build commands, and user input
- Use `--` separator in git clone commands
- When adding proto changes, verify both `bubbaloop-schemas` and `bubbaloop` compile

### DON'T

- Don't run `bubbaloop tui` from Claude Code (needs interactive TTY)
- Don't edit files in `target/` or `OUT_DIR`
- Don't commit `.env`, credentials, or `target/`
- Don't add `bubbaloop-schemas` to the workspace (intentionally standalone)
- Don't combine `git` and `path` in Cargo dependency specs
- Don't `git push --force` to main

### Verification Workflow

1. `pixi run check` — fast compile check
2. `pixi run clippy` — lint (zero warnings enforced)
3. `pixi run test` — run tests
4. `pixi run fmt` — format check

### Commit Style

Conventional commits: `feat:`, `fix:`, `chore:`, `docs:`, `refactor:`, `test:`

**Never commit:** `target/`, `node_modules/`, `dist/`, `.pixi/`, `*.pb.js`, `*.pb.d.ts`, `*.pyc`
**Always commit:** `Cargo.lock`, `pixi.lock`, `package-lock.json`
