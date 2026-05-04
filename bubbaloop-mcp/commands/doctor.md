---
description: Diagnose the bubbaloop install end-to-end — token, daemon, MCP transport, Zenoh router, mode, registered nodes — and tell the user the precise fix for any red status
---

Run a comprehensive bubbaloop install check. Use the `bubbaloop` MCP server where the check is in-band; fall back to Bash for things outside the daemon (env vars, Zenoh router process).

For each check below, output ONE line in this exact format:

```
✓ <check>                           OR
✗ <check> — <one-line fix command>
```

Sequence (each depends on the previous succeeding):

1. **`BUBBALOOP_MCP_TOKEN` env var** — present, non-empty, length plausible (≥ 16 chars). Fix: `export BUBBALOOP_MCP_TOKEN="$(cat ~/.bubbaloop/mcp-token)"`
2. **Token file** — `~/.bubbaloop/mcp-token` exists and is mode 0600. Fix: `bubbaloop daemon run` once to regenerate.
3. **Zenoh router** — port 7447 listening (check via `ss -tlnp 2>/dev/null | grep -q :7447`). Fix: `~/.bubbaloop/bin/zenohd > /tmp/zenohd.log 2>&1 &`
4. **Bubbaloop daemon** — port 8088 listening. Fix: `bubbaloop daemon run > /tmp/bubbaloop_daemon.log 2>&1 &`
5. **MCP `system_status`** — call returns success. Verifies the entire transport+auth path. Fix path depends on error: 401 → re-export token, connection refused → step 4, anything else → tail `/tmp/bubbaloop_daemon.log`.
6. **Tool surface** — `tools/list` returns ≥ 40 tools, names use `noun_verb` (`node_list` not `list_nodes`). If old names → daemon is running a pre-rename binary; rebuild `cargo build --bin bubbaloop` and restart.
7. **`node_list`** — call returns ≥ 1 node.
8. **Failed nodes** — count of nodes with status `Failed`. Surface their names. Fix recommendation: `/bubbaloop:diagnose <name>` for the first one.
9. **NotInstalled nodes** — count + names. Fix: `bubbaloop node install <name>` (NOT the MCP `node_install` tool — see alignment-notes #11).
10. **Mode** — read `BUBBALOOP_MCP_MODE` env var (or note it's unset → admin default). Flag if it's `readonly` or `safe` so the user knows why some tools may be ceiling-restricted.

End with a single-line summary: `<n> passed, <m> failed`.

If any check fails, follow it with the exact `→ next:` fix command on its own line. Don't narrate intermediate calls. Keep the response under one screen.
