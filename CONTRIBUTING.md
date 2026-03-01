# 🦐 Contributing to Bubbaloop

<!-- LIVING DOCUMENT: Update when workflows change. See ARCHITECTURE.md for design, ROADMAP.md for timeline. -->

🦐 Bubbaloop is built for **agentic engineering** — developers AND AI agents working together. This guide defines workflows that serve sensor nodes, contracts, and tests.

---

## 🦐 The Anti-Trap Rule

> *"People build increasingly complex toolchains... only to end up building tools instead of genuinely valuable things."*
> — Peter Steinberger

**Every workflow must directly improve sensor nodes, contracts, or tests.**

If a workflow only improves the workflow itself, **delete it**. The sensor nodes are the product. Everything else is scaffolding.

---

## 🦐 Development Workflows

### 1. Rust Feature: Plan → Execute → Validate → Review

1. Plan with acceptance criteria + files
2. Execute with executor agents (parallel file changes)
3. Validate: `pixi run check`, `cargo test`, `pixi run clippy`
4. Architect verifies implementation

**NEVER skip step 3**. Run `pixi run check` after every Rust change.

### 2. Dashboard Feature: Plan → Tests First → Build UI → Verify

1. Define component + test strategy
2. Write tests FIRST (vitest + jsdom)
3. Build UI (React + TypeScript + Tailwind)
4. All tests must pass (NO test deletion)

**Schema-ready gating**: New view components that decode protobuf MUST use `useSchemaReady()` to gate their `useZenohSubscription` callback.

### 3. CLI Command: Check argh conventions → Write + Test → Clippy

1. Check argh conventions in CLAUDE.md (`#[derive(FromArgs)]`, `#[argh(subcommand)]`)
2. Write command + unit tests (co-located `#[cfg(test)] mod tests`)
3. Verify 298+ Rust tests pass
4. Zero clippy warnings enforced

**Security**: Node names `[a-zA-Z0-9_-]{1,64}`, no null bytes.

### 4. Cross-Component Contract: Proto → Rust → TS → Templates → Validate

1. Map proto → Rust → JSON API → TypeScript → templates → UI
2. Update proto + rebuild BOTH descriptor pipelines (`bubbaloop-schemas` AND `bubbaloop`)
3. Update MCP tool handlers in `mcp/mod.rs`, add integration tests
4. Update dashboard types if applicable
5. Full system check (298+ Rust + 47 MCP integration tests)

**Critical**: Proto changes require rebuilding both descriptor pipelines.

---

## 🦐 Agent Tier Guidelines

Start at the lowest tier. Escalate only on failure. Haiku costs ~10x less than Opus.

| Task | Agent Tier | Why |
|------|-----------|-----|
| Find a file/function | `explore` (Haiku) | Fast lookup, no reasoning |
| Fix a type error | `build-fixer-low` (Haiku) | Single-line fix |
| Implement a feature | `executor` (Sonnet) | Standard coding task |
| Write tests | `tdd-guide` (Sonnet) | Needs codebase understanding |
| Debug race condition | `architect` (Opus) | Deep reasoning required |
| Review architecture | `critic` (Opus) | Judgment call, needs experience |

**Rule**: Don't waste Opus on lookups. Don't waste Haiku on architecture.

---

## 🦐 Validation Checklist

| Command | When | Why |
|---------|------|-----|
| `pixi run check` | After every Rust change | Fast compilation check |
| `cargo test --lib -p bubbaloop` | Before commits | 298+ Rust tests |
| `cargo test --features test-harness --test integration_mcp` | After MCP changes | 47 integration tests |
| `pixi run clippy` | Before PRs | Zero warnings (`-D warnings`) |

**Jetson constraint**: Do NOT run parallel cargo/pixi commands. ARM64 is too slow. Run sequentially.

---

## 🦐 Code Standards & Commits

See `CLAUDE.md` for full conventions. Critical rules:

**Rust**: `argh` (NOT clap), `log` (NOT tracing), `thiserror`/`anyhow`, `zbus` (NEVER spawn `systemctl`), 100% safe (no `unsafe`)

**MCP**: All tools use `PlatformOperations` trait. RBAC tiers (Viewer/Operator/Admin). All handlers audit-logged.

**Zenoh API**: NEVER `.complete(true)` on queryables. Python: `query.key_expr` is property NOT method. ALL nodes MUST use `mode: "client"`.

**Security**: `find_curl()` searches `/usr/bin`,`/usr/local/bin`,`/bin` only. Node names `[a-zA-Z0-9_-]{1,64}`. Bind localhost only.

**Commits**: `feat:`, `fix:`, `chore:`, `docs:`, `refactor:`, `test:`. Never commit `target/`, `node_modules/`, `.env`. Always commit `Cargo.lock`, `pixi.lock`.

---

## 🦐 Pull Request Checklist

- [ ] `pixi run check` passes
- [ ] `cargo test --lib -p bubbaloop` (298+ tests)
- [ ] `cargo test --features test-harness --test integration_mcp` (47 tests)
- [ ] `pixi run clippy` zero warnings
- [ ] `CLAUDE.md` updated if conventions changed
- [ ] PR summary (1-3 bullets) + test plan

**Contract PRs**: All surfaces updated (proto → Rust → JSON → TS → templates), backward compat verified.

---

## Maintaining This Document

**Update when**: New workflows established, agent tiers change, validation checks added.

**Keep under 150 lines** — AI agents have limited context.

**Related files**:
- `CLAUDE.md` — Conventions, pitfalls, DO/DON'T
- `ARCHITECTURE.md` — Design decisions, layer model, security
- `ROADMAP.md` — Implementation phases
