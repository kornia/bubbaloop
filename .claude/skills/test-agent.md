# Test Agent — Local Agent Smoke Test

Run the bubbaloop agent in chat mode with test prompts to verify the agent loop works end-to-end: streaming (P1), tool dispatch, plan tracking (P2), security boundaries, and episodic memory persistence.

Loop detection (P0) is verified via unit tests only — it requires a mock provider to trigger reliably.

## Usage
```
/test-agent              # Full smoke test (build + all scenarios)
/test-agent --quick      # Skip build, run tests only (assumes binary exists)
/test-agent --build      # Build only, no tests
```

## Prerequisites

- Anthropic API credentials (at least one of: `ANTHROPIC_API_KEY` env var, `~/.bubbaloop/anthropic-key` file, or `~/.bubbaloop/oauth-credentials.json`)
- Rust toolchain (`cargo`)
- The bubbaloop workspace checked out

## Steps

### Step 1: Check Credentials
Verify API credentials exist before spending time on builds or API calls:
```bash
ls ~/.bubbaloop/anthropic-key 2>/dev/null && echo "KEY_FILE=OK" || echo "KEY_FILE=MISSING"
ls ~/.bubbaloop/oauth-credentials.json 2>/dev/null && echo "OAUTH=OK" || echo "OAUTH=MISSING"
echo "ENV_KEY=${ANTHROPIC_API_KEY:+SET}"
```
At least one credential source must be available. If none exist, abort with:
> No API credentials found. Run `bubbaloop login` or set `ANTHROPIC_API_KEY`.

### Step 2: Isolate Test State
Create a temporary chat directory so tests don't pollute (or depend on) real episodic memory:
```bash
export BUBBALOOP_HOME=$(mktemp -d)
echo "Test state dir: $BUBBALOOP_HOME"
```
Set `BUBBALOOP_HOME` if the agent respects it, otherwise note that tests will use `~/.bubbaloop/chat/`.

**Cleanup:** After all tests, remove the temp dir (or note it for inspection).

### Step 3: Build Binary (skip if --quick)
Build the debug binary from the workspace root:
```bash
cargo build -p bubbaloop --bin bubbaloop 2>&1
```
If build fails, abort — no point running agent tests on a broken binary.

**Note:** Debug builds are fast (~15s incremental, ~2min clean). Avoid `--release` for smoke tests (much slower, especially on ARM64).

### Step 4: Run Unit Tests for Loop Detection (P0)
Loop detection can't be triggered via chat (requires a model that repeats tool calls). Verify it via unit tests:
```bash
cargo test --lib -p bubbaloop -- loop_detection 2>&1
```
**Verify:** All `loop_detection_*` tests pass (hash identity, hash distinctness, thresholds).

### Step 5: Test Scenario — Simple Text Response
**Tests:** Response streaming (P1), basic model invocation.
```bash
echo -e "What is 2+2? Answer in one word.\nquit" | timeout 30 ./target/debug/bubbaloop agent chat 2>/dev/null
```
**Pass criteria:**
- Agent prints a welcome banner with version and tool count
- Response appears (any correct answer to 2+2)
- Exits cleanly with "Goodbye."
- No panics or errors

### Step 6: Test Scenario — Tool Call
**Tests:** Tool dispatch, `read_file` tool, streaming interleaved with tool output.
```bash
echo -e "Read the file /etc/hostname\nquit" | timeout 30 ./target/debug/bubbaloop agent chat 2>/dev/null
```
**Pass criteria:**
- Output contains `[calling read_file...]` (tool was dispatched)
- Response references the machine's actual hostname
- No errors or panics

### Step 7: Test Scenario — Multi-Step Task (Plan Detection, P2)
**Tests:** Plan tracking (text + tool calls = plan persisted to episodic), multi-step execution.
```bash
echo -e "Check disk usage on the root partition and tell me how much space is free.\nquit" | timeout 45 ./target/debug/bubbaloop agent chat 2>/dev/null
```
**Pass criteria:**
- At least one `[calling ...]` line (tool dispatch happened)
- Response contains disk usage information (size, used, available)
- No errors

Then verify plan was persisted to episodic memory:
```bash
TODAY=$(date +%Y-%m-%d)
CHAT_DIR="${BUBBALOOP_HOME:-$HOME/.bubbaloop}/chat/memory"
grep -c '"role":"plan"' "$CHAT_DIR/daily_logs_${TODAY}.jsonl" 2>/dev/null || echo 0
```
Plan count should be >= 1 if the model produced text + tool calls in the same response. A count of 0 is acceptable if the model happened to not produce reasoning text before the tool call — this is model-dependent, not a code bug. Mark as WARN, not FAIL.

### Step 8: Test Scenario — Security Boundary
**Tests:** Sensitive file blocking (dispatch.rs security rules).
```bash
echo -e "Read the file /etc/shadow\nquit" | timeout 30 ./target/debug/bubbaloop agent chat 2>/dev/null
```
**Pass criteria:**
- Tool call is dispatched (`[calling read_file...]`)
- Result contains "Blocked" or "sensitive" — file contents are NOT exposed
- No panics

### Step 9: Verify Episodic Memory
Check that conversations from the test scenarios were logged:
```bash
TODAY=$(date +%Y-%m-%d)
CHAT_DIR="${BUBBALOOP_HOME:-$HOME/.bubbaloop}/chat/memory"
LOG_FILE="$CHAT_DIR/daily_logs_${TODAY}.jsonl"
if [ -f "$LOG_FILE" ]; then
  TOTAL=$(wc -l < "$LOG_FILE")
  USERS=$(grep -c '"role":"user"' "$LOG_FILE" || echo 0)
  ASSISTANTS=$(grep -c '"role":"assistant"' "$LOG_FILE" || echo 0)
  TOOLS=$(grep -c '"role":"tool"' "$LOG_FILE" || echo 0)
  PLANS=$(grep -c '"role":"plan"' "$LOG_FILE" || echo 0)
  echo "Episodic log: $TOTAL entries (user=$USERS, assistant=$ASSISTANTS, tool=$TOOLS, plan=$PLANS)"
else
  echo "WARNING: No episodic log found for today"
fi
```
**Pass criteria:**
- Log file exists
- Contains user, assistant, and tool entries from the test scenarios
- Plan entries are a bonus (model-dependent)

### Step 10: Summary Report
Output a results table:

| Test | Result | What it verifies |
|------|--------|------------------|
| Credentials | OK/FAIL | API auth configured |
| Build | OK/FAIL/SKIP | Binary compiles |
| Loop detection (unit) | OK/FAIL | P0: hash + threshold logic |
| Simple text | OK/FAIL | P1: streaming text output |
| Tool call | OK/FAIL | Tool dispatch + streaming |
| Multi-step task | OK/FAIL | Tool chaining works |
| Plan detection | OK/WARN | P2: plan persisted to episodic |
| Security | OK/FAIL | Sensitive files blocked |
| Episodic log | OK/WARN | Memory persistence works |

**WARN** vs **FAIL**: Plan detection depends on model behavior (whether it emits reasoning text before tool calls). A WARN means the code path exists but wasn't triggered this run. Everything else should be OK or FAIL.

### Important Notes
- **Token cost:** Each test scenario makes 1 API call (~$0.01-0.03). The full suite costs roughly $0.05-0.15.
- **Timeouts:** Each scenario has a `timeout` to prevent hangs if the API is slow or unresponsive.
- **Clean exit:** Always pipe `\nquit` after the test prompt so the REPL exits cleanly.
- **Stderr suppression:** Use `2>/dev/null` to hide log output and keep results readable.
- **Sequential execution:** Run scenarios one at a time — each starts a fresh agent session.
- **Debug binary:** Always use `./target/debug/bubbaloop`. Release builds are much slower and unnecessary for smoke tests.
- **No daemon required:** Chat mode (`agent chat`) runs standalone without Zenoh or the daemon.
