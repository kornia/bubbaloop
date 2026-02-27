# Changelog

## [0.0.6](https://github.com/kornia/bubbaloop/compare/v0.0.5...v0.0.6) (2026-02-27)

### Features

* **mcp:** complete node lifecycle — marketplace install, uninstall, autostart ([041a40c](https://github.com/kornia/bubbaloop/commit/041a40c))
* **mcp:** `install_node` accepts marketplace names (e.g., `"rtsp-camera"`) — downloads precompiled binaries, registers, and creates systemd service
* **mcp:** 4 new tools: `uninstall_node`, `clean_node`, `enable_autostart`, `disable_autostart`
* **mcp:** `install_node` with paths now chains `AddNode` + `Install` (was `AddNode` only)
* **marketplace:** new `marketplace.rs` module — shared precompiled binary download logic (extracted from CLI, reused by MCP)
* **rbac:** new tool-to-tier mappings — `uninstall_node`/`clean_node` → Admin, `enable_autostart`/`disable_autostart` → Operator

### Tests

* 12 new MCP integration tests (47 total, up from 35)
* 300 unit tests passing

## [0.0.5](https://github.com/kornia/bubbaloop/compare/v0.0.4...v0.0.5) (2026-02-27)

### Features

* **mcp:** OpenClaw foundation — MCP-first universal runtime ([1d0c851](https://github.com/kornia/bubbaloop/commit/1d0c851))
* make axum non-optional for MCP HTTP transport ([eb85a3a](https://github.com/kornia/bubbaloop/commit/eb85a3a))

## [0.0.2](https://github.com/kornia/bubbaloop/compare/v0.0.1...v0.0.2) (2026-01-25)


### Bug Fixes

* add --experimental-wasm-modules flag for zenoh-ts WASM support ([259be8c](https://github.com/kornia/bubbaloop/commit/259be8c67fa1a278c61edbd3e035e4919352ebcf))
