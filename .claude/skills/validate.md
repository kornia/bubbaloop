# Validate System

Full system validation workflow for bubbaloop. Runs all test suites, linting, and optionally consults Gemini CLI for a second review.

## Usage
```
/validate            # Full validation (Rust + dashboard + clippy)
/validate --quick    # Rust only, skip dashboard
/validate --gemini   # Full validation + Gemini CLI review
```

## Steps

### Step 1: Rust Compilation Check
Run `cargo check --lib -p bubbaloop` to verify the library compiles. This is fast and catches most issues.

### Step 2: Rust Test Suite
Run `cargo test --lib -p bubbaloop` and report the number of passing tests. All tests must pass.

### Step 3: Dashboard Tests (skip if --quick)
Run `cd /home/nvidia/bubbaloop/dashboard && npm test` and report the number of passing tests across test files. All tests must pass.

### Step 4: Clippy Lint
Run `pixi run clippy` and verify zero warnings. The project enforces `-D warnings`.

### Step 5: Template Integrity Check
Verify template files exist and have required content:
- `templates/rust-node/node.yaml.template` must contain `{{node_name}}`
- `templates/python-node/node.yaml.template` must contain `{{node_name}}`
- `templates/rust-node/Cargo.toml.template` must NOT have ambiguous git+path for bubbaloop-schemas
- Python template must NOT contain `complete=True` on queryables

### Step 6: Gemini CLI Review (only if --gemini)
Use `NODE_OPTIONS="--max-old-space-size=4096" gemini` to review changed files:
- Pipe each changed file via stdin with a focused review prompt
- Categorize findings as: FIX (real bugs), DEFER (pre-existing/roadmap), or FALSE POSITIVE
- Only fix real bugs; document deferred items

### Step 7: Summary Report
Output a table with:

| Check | Result | Details |
|-------|--------|---------|
| Rust compile | PASS/FAIL | |
| Rust tests | PASS/FAIL | N tests |
| Dashboard tests | PASS/FAIL/SKIP | N tests, M files |
| Clippy | PASS/FAIL | N warnings |
| Templates | PASS/FAIL | |
| Gemini review | N findings | X fixed, Y deferred |

### Important Constraints (Jetson)
- Do NOT run parallel cargo/pixi commands -- too slow on ARM64
- Run steps sequentially
- Use `cargo check --lib -p bubbaloop` (not full binary check, needs dashboard/dist/)
- Use `NODE_OPTIONS="--max-old-space-size=4096"` for Gemini CLI to avoid OOM
