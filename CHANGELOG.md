# Changelog

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
