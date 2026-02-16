# Gemini Review — Second AI Opinion

Get a second opinion from Gemini CLI on code changes, architecture decisions, or debugging hypotheses.

## Usage
```
/gemini-review                    # Review all uncommitted changes
/gemini-review --staged           # Review only staged changes
/gemini-review --file <path>      # Review a specific file
/gemini-review --question "<q>"   # Ask Gemini a specific question about the codebase
/gemini-review --diff <base>      # Review diff against a branch (e.g., main)
```

## How It Works

Uses the Gemini CLI (`gemini -p`) in non-interactive headless mode. Pipes context (diffs, files, questions) via stdin and captures the response.

**Important**: Always use `NODE_OPTIONS="--max-old-space-size=4096"` on Jetson to avoid OOM.

## Steps

### Step 1: Gather Context

Based on the mode:

**Default (uncommitted changes)**:
```bash
git diff HEAD -- ':!*.lock' ':!package-lock.json' ':!dist/' ':!target/' > /tmp/gemini-review-diff.txt
echo "Files changed: $(git diff HEAD --name-only | wc -l)"
```

**--staged**:
```bash
git diff --cached -- ':!*.lock' ':!package-lock.json' > /tmp/gemini-review-diff.txt
```

**--file <path>**:
```bash
cat <path> > /tmp/gemini-review-context.txt
```

**--diff <base>**:
```bash
git diff <base>...HEAD -- ':!*.lock' ':!package-lock.json' ':!dist/' ':!target/' > /tmp/gemini-review-diff.txt
```

### Step 2: Build the Prompt

Construct a focused review prompt based on the mode. For diffs:

```
Review this code diff for a Rust + TypeScript robotics project (bubbaloop).
Focus on:
1. Correctness bugs (logic errors, off-by-one, race conditions)
2. API misuse (wrong function signatures, missing error handling)
3. Security issues (injection, unsafe patterns)
4. Zenoh-specific issues (topic naming, session lifecycle, queryable patterns)

For each finding, categorize as:
- CRITICAL: Must fix before merge
- WARNING: Should fix but not blocking
- INFO: Style/improvement suggestion

Be concise. Skip obvious things. Only flag real issues.
```

For questions, pass the question directly with relevant file context.

### Step 3: Call Gemini CLI

```bash
NODE_OPTIONS="--max-old-space-size=4096" cat /tmp/gemini-review-diff.txt | gemini -p "<prompt>" --sandbox false 2>/dev/null
```

If the diff is too large (>50KB), split into chunks by file and review each separately:
```bash
for file in $(git diff HEAD --name-only); do
  echo "=== Reviewing: $file ==="
  git diff HEAD -- "$file" | NODE_OPTIONS="--max-old-space-size=4096" gemini -p "Review this diff for $file. Flag only real bugs, categorize as CRITICAL/WARNING/INFO." --sandbox false 2>/dev/null
done
```

### Step 4: Parse and Present Results

Present Gemini's findings in a structured table:

| # | Severity | File | Finding | Action |
|---|----------|------|---------|--------|
| 1 | CRITICAL | foo.rs | Race condition in... | FIX |
| 2 | WARNING | bar.ts | Missing null check... | FIX |
| 3 | INFO | baz.rs | Could simplify... | DEFER |

### Step 5: Act on Findings

- **CRITICAL**: Fix immediately, delegate to executor agent
- **WARNING**: Fix if quick (<5 min), otherwise note for follow-up
- **INFO**: Skip unless user specifically requests
- **FALSE POSITIVE**: Ignore — Gemini sometimes flags correct patterns as issues

### Tips

- Gemini is good at spotting API misuse and logic errors but sometimes hallucinates Rust borrow checker issues
- For Zenoh-specific questions, provide the zenoh version context (v1.7.x)
- For architecture decisions, include relevant sections from ARCHITECTURE.md as context
- If Gemini CLI hangs (>60s), kill it and retry — it occasionally gets stuck on large inputs
- The `--sandbox false` flag is needed to allow file reads if Gemini needs additional context

### Constraints (Jetson ARM64)

- Always use `NODE_OPTIONS="--max-old-space-size=4096"` to prevent OOM
- Do NOT run Gemini in parallel with cargo builds — too much memory pressure
- Timeout after 120 seconds per review chunk
