# Bubbaloop

Physical AI orchestration built on Zenoh. Single binary: CLI + TUI + daemon.

## Structure

```
crates/bubbaloop/          # Main binary (CLI + TUI + daemon)
crates/bubbaloop-schemas/  # Protobuf schemas (standalone, NOT in workspace — never add to workspace)
dashboard/                 # React + Vite + TypeScript
```

Key source files in `crates/bubbaloop/src/`:
- `cli/node.rs` (88KB) — node CRUD, install, precompiled binary download
- `daemon/node_manager.rs` (57KB) — node lifecycle, build queue, health
- `daemon/systemd.rs` (38KB) — D-Bus/zbus integration
- `daemon/registry.rs` — `~/.bubbaloop/nodes.json` management
- `registry.rs` — marketplace fetch/parse/cache, `find_curl()`

Nodes repo: [bubbaloop-nodes-official](https://github.com/kornia/bubbaloop-nodes-official)

## Build & Verify

```bash
pixi run check     # cargo check (fast — run after every change)
pixi run clippy    # zero warnings enforced (-D warnings)
pixi run test      # cargo test
pixi run fmt       # cargo fmt --all
pixi run build     # cargo build --release (slow on ARM64)
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
- `find_curl()` searches `/usr/bin`, `/usr/local/bin`, `/bin` only (no PATH)
- Build commands: allowlist prefixes only (cargo, pixi, npm, make, python, pip)
- Node names: 1-64 chars, `[a-zA-Z0-9_-]`, no null bytes
- Git clone: always use `--` separator
- Bind localhost only, never `0.0.0.0`

## DO / DON'T

**DO:** `pixi run check` after changes | `pixi run fmt && pixi run clippy` before commits | tests with every PR | validate user input | `--` in git clone | verify both `bubbaloop-schemas` and `bubbaloop` compile after proto changes

**DON'T:** run `bubbaloop tui` (needs TTY) | edit `target/`/`OUT_DIR` | commit `.env`/credentials/`target/` | add `bubbaloop-schemas` to workspace | combine `git`+`path` in Cargo deps | `git push --force` to main

## Commits

Conventional: `feat:`, `fix:`, `chore:`, `docs:`, `refactor:`, `test:`
Never commit: `target/`, `node_modules/`, `dist/`, `.pixi/`
Always commit: `Cargo.lock`, `pixi.lock`, `package-lock.json`

## Pitfalls

- OAuth token lacks `workflow` scope — use SSH for workflow files
- ARM64 release builds are slow — use `pixi run check` first
- Proto changes require rebuilding both `bubbaloop-schemas/` and `bubbaloop` (descriptor.bin is compiled in)
- Dashboard deps (`axum`, `rust-embed`) are behind `dashboard` feature flag
- Logs must go to stderr to avoid corrupting TUI
- Zenoh session: `"peer"` mode, check `BUBBALOOP_ZENOH_ENDPOINT` env var
