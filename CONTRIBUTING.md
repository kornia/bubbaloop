# Contributing to Bubbaloop

<!-- LIVING DOCUMENT: Update when workflows change or new skills are added.
     For architecture decisions, see ARCHITECTURE.md. For timeline, see ROADMAP.md. -->

Bubbaloop is built for **agentic engineering** — developers AND AI agents working together. This guide defines workflows that serve sensor nodes, contracts, and tests.

---

## The Anti-Trap Rule

> *"People build increasingly complex toolchains... only to end up building tools instead of genuinely valuable things."*
> — Peter Steinberger, OpenClaw creator

**Every workflow must directly improve sensor nodes, contracts, or tests.**

If a workflow only improves the workflow itself, **delete it**. The sensor nodes are the product. Everything else is scaffolding.

Before adding any new process/skill/automation, ask:
- Does this help build better sensor nodes? ✅
- Does this strengthen data contracts? ✅
- Does this improve test coverage? ✅
- Does this only improve the workflow? ❌ **Delete**

---

## Development Workflows

### 1. Rust Feature: `/ralplan` → `/ralph` → `/validate` → `/validate --gemini` → `/code-review`

1. Plan with Planner + Critic (acceptance criteria + files)
2. Execute with executor agents (parallel file changes)
3. Validate: `pixi run check`, `cargo test`, `npm test`, `clippy`
4. Gemini cross-review (list max 3 bugs only)
5. Architect verifies implementation

**NEVER skip step 3**. Run `pixi run check` after every Rust change.

### 2. Dashboard Feature: `/plan` → `/tdd` (tests first) → designer builds UI → `npm test` → `/debug-dashboard`

1. Define component + test strategy
2. Write tests FIRST (vitest + jsdom) — test behavior, not implementation
3. Designer agent builds UI (React + TypeScript + Tailwind)
4. All 490+ tests must pass (NO test deletion)
5. Live verification (data flows to browser)

**Test-first is non-negotiable**. Dashboard test count only goes up.

**Schema-ready gating**: New view components that decode protobuf MUST use `useSchemaReady()` to gate their `useZenohSubscription` callback. Pass `schemaReady ? handleSample : undefined` to avoid dropping messages during schema loading. See `CameraView.tsx` for reference pattern.

### 3. CLI Command: Check argh conventions → write + test → `cargo test` → `/qa-tester` (tmux) → `clippy`

1. Check argh conventions in CLAUDE.md (`#[derive(FromArgs)]`, `#[argh(subcommand)]`)
2. Write command + unit tests (co-located `#[cfg(test)] mod tests`)
3. Verify 240+ Rust tests pass
4. Interactive CLI testing (verify help text, errors, flags)
5. Zero warnings enforced

**Security**: Node names `[a-zA-Z0-9_-]{1,64}`, no null bytes.

### 4. Cross-Component Contract: Identify surfaces → update proto → Rust → TS → templates → `/validate` → `/validate --gemini`

1. Map proto → Rust → JSON API → TypeScript → templates → UI
2. Update proto + rebuild BOTH descriptor pipelines (`bubbaloop-schemas` AND `bubbaloop`)
3. Update Rust structs (`daemon/zenoh_api.rs`), add serde tests
4. Update dashboard types, add tests in `schema-registry-decode.test.ts`
5. Update node templates if topics/fields change
6. Full system check (240 Rust + 210 dashboard tests)
7. Gemini cross-review

**Critical**: Proto changes require rebuilding both descriptor pipelines.

---

## Agent Tier Guidelines

Start at the lowest tier. Escalate only on failure. Haiku costs ~10x less than Opus.

| Task | Agent Tier | Why |
|------|-----------|-----|
| Find a file/function | `explore` (Haiku) | Fast lookup, no reasoning |
| Fix a type error | `build-fixer-low` (Haiku) | Single-line fix |
| Implement a feature | `executor` (Sonnet) | Standard coding task |
| Write tests | `tdd-guide` (Sonnet) | Needs codebase understanding |
| Debug race condition | `architect` (Opus) | Deep reasoning required |
| Review architecture | `critic` (Opus) | Judgment call, needs experience |
| Quick security scan | `security-reviewer-low` (Haiku) | Pattern matching |
| Full security audit | `security-reviewer` (Opus) | Deep analysis |

**Rule**: Don't waste Opus on lookups. Don't waste Haiku on architecture.

---

## Two-Critic Loop

Use both Claude and Gemini for non-trivial changes:

1. Claude writes code
2. `/validate` — automated checks (fast)
3. `/validate --gemini` — Gemini reviews changed files
4. If Gemini finds **real bug** → Claude fixes → re-validate
5. If Gemini suggests **improvement** → evaluate: does it serve sensor nodes?
   - **Yes** → implement
   - **No** → it's the agentic trap, skip it

**Gemini CLI command** (Jetson-safe):
```bash
NODE_OPTIONS="--max-old-space-size=4096" gemini -p \
  "Review this Rust file briefly. List max 3 actionable bugs or security issues only. Skip style suggestions." \
  < path/to/file.rs
```

Fix real bugs. Skip style bikeshedding.

---

## Validation Checklist

Run these at the right time to avoid wasted cycles:

| Command | When | Why |
|---------|------|-----|
| `pixi run check` | After every Rust change | Fast compilation check |
| `cargo test --lib -p bubbaloop` | Before commits | 240+ Rust tests must pass |
| `cd dashboard && npm test -- --run` | After dashboard changes | 490+ tests, no deletions allowed |
| `pixi run clippy` | Before PRs | Zero warnings enforced (`-D warnings`) |
| `./scripts/validate.sh` | Before final commit | Full system check (10 phases) |
| `./scripts/validate.sh --gemini` | Before PR submission | AI cross-review |

**Jetson constraint**: Do NOT run parallel cargo/pixi commands. ARM64 is too slow. Run sequentially.

---

## Skills Reference

Available skills (see `.claude/skills/` for implementation):

| Skill | Purpose | When to Use |
|-------|---------|-------------|
| `/validate` | Full system validation (Rust + dashboard + clippy + templates) | After any code change, before commits |
| `/validate --quick` | Rust-only validation (skip dashboard) | Quick iteration on daemon code |
| `/validate --gemini` | Full validation + Gemini CLI review | Before PR submission |
| `/debug-dashboard` | Diagnose dashboard data pipeline issues | When panels show no data |

**Planned** (not yet implemented):

| Skill | Purpose | Trigger |
|-------|---------|---------|
| `/test-node` | End-to-end node test: start → publish → dashboard receives | After node template changes |
| `/test-contract` | Verify cross-component contracts (machine ID, topics, API fields) | After schema/API changes |
| `/audit-security` | Run Zenoh ACL + mTLS + input validation checks | Before releases |
| `/upgrade-node` | Graduate Python node to Rust with tests | When node is production-ready |

Add new skills when they directly improve sensor nodes, contracts, or tests.

---

## CLAUDE.md Maintenance

Keep under 100 lines. Update only when:

1. **Conventions change** — Project migrates (e.g., argh → clap)
2. **Pitfall earned** — Real bug in real PR (no hypotheticals)
3. **Architecture change** — New pattern emerges (e.g., Zenoh API rules)
4. **Security constraint** — Vulnerability found

Conventions are immutable unless tech stack changes. Pitfalls are earned, not predicted. Reference skills, don't duplicate.

---

## Code Standards & Commits

See `CLAUDE.md` for full conventions. Critical rules:

**Rust**: `argh` (NOT clap), `log` (NOT tracing), `thiserror`/`anyhow`, `zbus` (NEVER spawn `systemctl`), 100% safe (no `unsafe`)

**Zenoh API**: NEVER `.complete(true)` on queryables (blocks wildcards). Python: `query.key_expr` is property NOT method.

**Security**: `find_curl()` searches `/usr/bin`,`/usr/local/bin`,`/bin` only. Node names `[a-zA-Z0-9_-]{1,64}`. Bind localhost only.

**Commits**: `feat:`, `fix:`, `chore:`, `docs:`, `refactor:`, `test:`. Never commit `target/`, `node_modules/`, `.env`. Always commit `Cargo.lock`, `pixi.lock`, `package-lock.json`.

---

## Pull Request Checklist

**All PRs**:
- [ ] `/validate` passes (240+ Rust, 210+ dashboard tests)
- [ ] `pixi run clippy` zero warnings
- [ ] `/validate --gemini` completed, real bugs fixed
- [ ] `CLAUDE.md` updated if conventions changed
- [ ] PR summary (1-3 bullets) + test plan + references

**Dashboard PRs**: Tests added, none deleted, count ≥ previous

**Contract PRs**: All 5 surfaces updated (proto → Rust → JSON → TS → templates), backward compat verified

---

## Maintaining This Document

**Update when**:
- New workflows established
- New skills added to `.claude/skills/`
- Agent tiers change (cost optimization)
- Validation checks added to `scripts/validate.sh`

**Keep under 250 lines** — AI agents have limited context.

**Related files**:
- `CLAUDE.md` — Conventions, pitfalls, DO/DON'T
- `ARCHITECTURE.md` — Design decisions, Steinberger Principle
- `ROADMAP.md` — Timeline, migration phases
- `.claude/skills/` — Executable workflows
