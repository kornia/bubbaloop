---
description: Scaffold a new bubbaloop node via the bubbaloop-node-author subagent — guided through manifest, command queryable, validation, and registration
argument-hint: <node-name> [rust|python]
---

Dispatch the `bubbaloop-node-author` subagent to scaffold a new node named `$ARGUMENTS`.

The subagent will:

1. Confirm intended **role** (sensor / actuator / processor / gateway) and language (rust default, python alternative).
2. Run `bubbaloop node init <name> --type <lang>` to create the directory tree.
3. Walk through the **manifest** (`node.yaml`) shape — capabilities, commands, topics, hardware requirements.
4. Show the **command queryable handler skeleton** that correctly parses the `{command, params}` wire envelope (the most common new-node footgun — see node-author guide).
5. Recommend the **`verb_noun` command-naming convention** (matches OBS MCP server, improves small-model accuracy by 13–15%).
6. Run `bubbaloop node validate` to confirm the manifest is well-formed.
7. Walk through `bubbaloop node add` → `bubbaloop node install` → `node_start` via MCP to test end-to-end.

If you already have an in-progress node directory, point that out — the subagent can adapt to existing layouts rather than scaffolding from scratch.
