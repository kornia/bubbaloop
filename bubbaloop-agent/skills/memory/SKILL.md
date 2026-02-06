# Memory

Remember things for future conversations. Your memory persists across restarts in MEMORY.md.

## Tools

### remember
Store a piece of information in persistent memory.
- `content` (string): What to remember. Be specific and concise.
- `category` (string, optional): Category for organizing memories.
- Returns: Confirmation of what was stored.

### recall
Search your memory for relevant information.
- `query` (string): What to search for.
- Returns: Matching memory entries.

### forget
Remove a piece of information from memory.
- `content` (string): Description of what to forget (matches and removes).
- Returns: Confirmation of what was removed.

## Categories

Use these categories to keep memory organized:

| Category | What to Store |
|----------|--------------|
| `user` | User's name, role, expertise level, timezone, communication style |
| `preferences` | How the user likes things done (alert style, verbosity, workflows) |
| `patterns` | System patterns (crash correlations, peak times, resource usage) |
| `issues` | Problems encountered and their solutions |
| `general` | Anything else worth remembering |

## When to Remember

- User tells you their name or role → `remember` with category "user"
- User corrects your behavior → `remember` with category "preferences"
- You discover a system pattern → `remember` with category "patterns"
- You solve a problem → `remember` with category "issues"
- User expresses a preference → `remember` with category "preferences"
