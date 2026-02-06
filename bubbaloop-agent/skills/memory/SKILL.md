# Memory

Remember things for future conversations. Your memory persists across restarts in MEMORY.md.

## Tools

### remember
Store a piece of information in persistent memory.
- `content` (string): What to remember. Be specific and concise.
- `category` (string, optional): Category like "patterns", "preferences", "issues" (default: "general").
- Returns: Confirmation of what was stored.

### recall
Search your memory for relevant information.
- `query` (string): What to search for.
- Returns: Matching memory entries.

### forget
Remove a piece of information from memory.
- `content` (string): Description of what to forget (matches and removes).
- Returns: Confirmation of what was removed.
