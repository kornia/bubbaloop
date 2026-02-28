# Changelog

## [0.0.7](https://github.com/kornia/bubbaloop/compare/v0.0.6...v0.0.7) (2026-03-01)

### Features

* **cli:** migrate CLI from Zenoh to REST API ([c797ffa](https://github.com/kornia/bubbaloop/commit/c797ffa))
* **cli:** add `bubbaloop login` and `logout` commands for API key setup ([315179a](https://github.com/kornia/bubbaloop/commit/315179a))
* **cli:** add `--skip-validation` flag to login command ([d5054cf](https://github.com/kornia/bubbaloop/commit/d5054cf))
* **cli:** make `bubbaloop up` create per-skill node instances ([0171643](https://github.com/kornia/bubbaloop/commit/0171643))
* **agent:** add Claude API client with tool_use support ([61ddbf6](https://github.com/kornia/bubbaloop/commit/61ddbf6))
* **agent:** add agent orchestrator, system prompt builder, and CLI command ([bfe4752](https://github.com/kornia/bubbaloop/commit/bfe4752))
* **agent:** add internal MCP tool dispatcher ([8e8dda0](https://github.com/kornia/bubbaloop/commit/8e8dda0))
* **agent:** wire scheduler into agent and register skill schedules ([c6a44e7](https://github.com/kornia/bubbaloop/commit/c6a44e7))
* **memory:** add SQLite memory module with conversations, events, schedules ([93a7478](https://github.com/kornia/bubbaloop/commit/93a7478))
* **memory:** add FTS5 full-text search to agent memory system ([ab704e3](https://github.com/kornia/bubbaloop/commit/ab704e3))
* **scheduler:** add cron-based scheduler with Tier 1 built-in actions ([0215227](https://github.com/kornia/bubbaloop/commit/0215227))
* **skills:** add YAML skills system and `bubbaloop up` command ([0cc7657](https://github.com/kornia/bubbaloop/commit/0cc7657))

### Improvements

* improve agent UX, error handling, and API key resolution ([f74fd91](https://github.com/kornia/bubbaloop/commit/f74fd91))
* remove deprecation warnings from debug commands ([bbb5205](https://github.com/kornia/bubbaloop/commit/bbb5205))

### Docs

* updated CLAUDE.md, ROADMAP.md, ARCHITECTURE.md for login command and agent phases

## [0.0.6](https://github.com/kornia/bubbaloop/compare/v0.0.5...v0.0.6) (2026-02-27)

### Features

* **mcp:** complete node lifecycle — marketplace install, uninstall, autostart ([041a40c](https://github.com/kornia/bubbaloop/commit/041a40c))
* **mcp:** `install_node` accepts marketplace names (e.g., `"rtsp-camera"`) — downloads precompiled binaries, registers, and creates systemd service
* **mcp:** 4 new tools: `uninstall_node`, `clean_node`, `enable_autostart`, `disable_autostart`
* **mcp:** `install_node` with paths now chains `AddNode` + `Install` (was `AddNode` only)
* **marketplace:** new `marketplace.rs` module — shared precompiled binary download logic (extracted from CLI, reused by MCP)
* **rbac:** new tool-to-tier mappings — `uninstall_node`/`clean_node` → Admin, `enable_autostart`/`disable_autostart` → Operator

### Refactors

* **tui:** remove TUI module entirely (ratatui/crossterm) — codebase simplified from ~20K to ~14K lines ([f3b54cb](https://github.com/kornia/bubbaloop/commit/f3b54cb))
* **cli:** split monolithic `cli/node.rs` (2,655 lines) into 4 submodules: `mod.rs`, `install.rs`, `lifecycle.rs`, `build.rs`
* **cleanup:** remove dead code, unused imports, stale feature flags

### Docs

* Hardware AI Agent design document (`docs/plans/2026-02-27-hardware-ai-agent-design.md`)
* Rewritten ROADMAP with 5-phase implementation plan (YAML skills → Agent → SQLite → Scheduling → Polish)
* Updated ARCHITECTURE.md, CONTRIBUTING.md, CLAUDE.md to match new vision

### Tests

* 12 new MCP integration tests (47 total, up from 35)
* 298 unit tests passing

## [0.0.5](https://github.com/kornia/bubbaloop/compare/v0.0.4...v0.0.5) (2026-02-27)

### Features

* **mcp:** OpenClaw foundation — MCP-first universal runtime ([1d0c851](https://github.com/kornia/bubbaloop/commit/1d0c851))
* make axum non-optional for MCP HTTP transport ([eb85a3a](https://github.com/kornia/bubbaloop/commit/eb85a3a))

## [0.0.4](https://github.com/kornia/bubbaloop/compare/v0.0.3...v0.0.4) (2026-02-05)

### Bug Fixes

* correct JSON field name in marketplace sources ([6149d4d](https://github.com/kornia/bubbaloop/commit/6149d4d))

### Chores

* bump version to 0.0.4 ([0eb04d4](https://github.com/kornia/bubbaloop/commit/0eb04d4))

## [0.0.3](https://github.com/kornia/bubbaloop/compare/v0.0.2...v0.0.3) (2026-02-05)

### Bug Fixes

* improve install script and doctor command for smooth onboarding ([62019bc](https://github.com/kornia/bubbaloop/commit/62019bc))

## [0.0.2](https://github.com/kornia/bubbaloop/compare/v0.0.1...v0.0.2) (2026-02-05)

### Features

* make nodes standalone with expanded bubbaloop-schemas ([724b643](https://github.com/kornia/bubbaloop/commit/724b643))
* standalone nodes, marketplace clarity, and schema consolidation ([307fdd9](https://github.com/kornia/bubbaloop/commit/307fdd9))
* **dashboard:** multi-machine fleet orchestration ([e7cd324](https://github.com/kornia/bubbaloop/commit/e7cd324))
* add bubbaloop-dash standalone dashboard server ([a70bf25](https://github.com/kornia/bubbaloop/commit/a70bf25))
* add `bubbaloop launch` command for multi-instance node support ([7255c0a](https://github.com/kornia/bubbaloop/commit/7255c0a))

### Bug Fixes

* add --experimental-wasm-modules flag for zenoh-ts WASM support ([259be8c](https://github.com/kornia/bubbaloop/commit/259be8c67fa1a278c61edbd3e035e4919352ebcf))
* add missing machine-scoped nodes queryable and remove dead HTTP proxy ([c819528](https://github.com/kornia/bubbaloop/commit/c819528))
* **dashboard:** camera topic auto-detection, build warnings, and proto header ([9d1fa8f](https://github.com/kornia/bubbaloop/commit/9d1fa8f))

### Chores

* remove release-please workflow ([a4476d9](https://github.com/kornia/bubbaloop/commit/a4476d9))
* remove old daemon crate, add schemas crate, update docs and security ([af1d5b8](https://github.com/kornia/bubbaloop/commit/af1d5b8))
* resolve 3 dependabot security vulnerabilities ([d4878e6](https://github.com/kornia/bubbaloop/commit/d4878e6))

## [0.0.1](https://github.com/kornia/bubbaloop/releases/tag/v0.0.1) (2026-01-31)

### Features

* merge daemon into CLI, single 7.4MB binary + ratatui TUI ([268f8ff](https://github.com/kornia/bubbaloop/commit/268f8ff))
