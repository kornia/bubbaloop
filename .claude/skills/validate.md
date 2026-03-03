# Validate System

Full system validation workflow for bubbaloop. Runs all test suites, linting, and optionally MCP smoke tests.

## Usage
```
/validate            # Full validation (Rust + MCP integration + clippy)
/validate --quick    # Rust unit tests only
/validate --smoke    # Full validation + MCP stdio smoke test
/validate --gemini   # Full validation + Gemini CLI review
```

## Steps

### Step 1: Rust Compilation Check
Run `pixi run check` to verify the full crate compiles (includes MCP as core dependency).

### Step 2: Clippy Lint
Run `pixi run clippy` and verify zero warnings. The project enforces `-D warnings` with `--all-features`.

### Step 3: Rust Unit Tests
Run `pixi run test` and report the number of passing tests. All tests must pass. Expected: ~325 tests.

### Step 4: MCP Integration Tests (skip if --quick)
Run `cargo test --features test-harness --test integration_mcp` and report results. These test the full MCP protocol stack (tool routing, RBAC, validation) against MockPlatform over duplex transport. Expected: ~35 tests.

### Step 5: MCP Stdio Smoke Test (only if --smoke)
Test the real binary with a live MCP request over stdio:

```bash
(printf '{"jsonrpc":"2.0","method":"initialize","id":1,"params":{"protocolVersion":"2024-11-05","capabilities":{},"clientInfo":{"name":"smoke-test","version":"1.0"}}}\n{"jsonrpc":"2.0","method":"notifications/initialized"}\n{"jsonrpc":"2.0","method":"tools/call","id":2,"params":{"name":"get_machine_info","arguments":{}}}\n'; sleep 10) | timeout 15 ./target/release/bubbaloop mcp --stdio 2>/dev/null
```

Verify:
- Response id=1: initialize response with `protocolVersion` and `capabilities.tools`
- Response id=2: `get_machine_info` returns JSON with `arch`, `hostname`, `os` fields

If the binary doesn't exist, build it first with `cargo build --release` (takes ~9 minutes on ARM64).

### Step 6: Dashboard Tests (skip if --quick)
Run `cd /home/nvidia/bubbaloop/dashboard && npm test` and report the number of passing tests across test files. All tests must pass.

### Step 7: Template Integrity Check
Verify template files exist and have required content:
- `templates/rust-node/node.yaml.template` must contain `{{node_name}}`
- `templates/python-node/node.yaml.template` must contain `{{node_name}}`
- `templates/rust-node/Cargo.toml.template` must NOT have ambiguous git+path for bubbaloop-schemas
- Python template must NOT contain `complete=True` on queryables

### Step 8: Gemini CLI Review (only if --gemini)
Use `NODE_OPTIONS="--max-old-space-size=4096" gemini` to review changed files:
- Pipe each changed file via stdin with a focused review prompt
- Categorize findings as: FIX (real bugs), DEFER (pre-existing/roadmap), or FALSE POSITIVE
- Only fix real bugs; document deferred items

### Step 9: Summary Report
Output a table with:

| Check | Result | Details |
|-------|--------|---------|
| Compile | PASS/FAIL | |
| Clippy | PASS/FAIL | N warnings |
| Unit tests | PASS/FAIL | N tests |
| MCP integration | PASS/FAIL/SKIP | N tests |
| MCP smoke test | PASS/FAIL/SKIP | stdio init + tool call |
| Dashboard tests | PASS/FAIL/SKIP | N tests, M files |
| Templates | PASS/FAIL | |
| Gemini review | N findings | X fixed, Y deferred |

### Important Constraints (Jetson ARM64)
- Do NOT run parallel cargo/pixi commands -- too slow on ARM64
- Run steps sequentially
- `pixi run check` (not `cargo check --lib`) to include all features
- Use `NODE_OPTIONS="--max-old-space-size=4096"` for Gemini CLI to avoid OOM
- Release builds take ~9 minutes -- only build for --smoke tests
