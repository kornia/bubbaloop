---
description: Validate a node's manifest, directory structure, and command schema against the bubbaloop contract
argument-hint: <node-path-or-name>
---

Validate node `$ARGUMENTS` against the bubbaloop node contract.

If `$ARGUMENTS` looks like a path (starts with `/` or `.` or contains `/`), run:

```bash
bubbaloop node validate $ARGUMENTS
```

If `$ARGUMENTS` looks like a registered node name, look up its path first via `node_health(node_name="$ARGUMENTS")` (the response includes `path`) and validate that.

Report the result as a compact list:

- `✓ <check>` for passes
- `✗ <check> — <fix>` for failures

Common failures and fixes:

| Failure | Likely cause | Fix |
|---|---|---|
| `node.yaml not found` | Wrong directory passed | Pass the directory containing `node.yaml`, not its parent |
| `name not matching ^[a-zA-Z0-9_/-.]+$` | Invalid characters in name | Rename — use snake_case or kebab-case only |
| `command queryable not declared` | Missing `/command` queryable in main loop | See node-author guide for the skeleton |
| `manifest schema_version too old` | Pre-2026 manifest format | Bump `schema_version` field; see migration table in core-contributing.md |

End with a one-line `→ next:` recommendation. If the node passes, suggest `bubbaloop node add` and `node_install` (or `bubbaloop node install` for the systemd step).

Keep the response under one screen.
