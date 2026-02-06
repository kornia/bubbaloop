# Watchers

Create persistent monitors on data streams. Watchers periodically check data from Zenoh topics and use LLM reasoning to decide if action is needed. They run autonomously in the background.

## Tools

### create_watcher
Create a new watcher that monitors data streams and acts on your instruction.
- `name` (string): Unique watcher name (e.g., "disk-monitor").
- `topics` (list of strings): Topic suffixes to subscribe to (e.g., ["system-telemetry/metrics"]).
- `sample_interval_sec` (integer): How often to check data (default: 30).
- `instruction` (string): Natural language instruction for what to monitor and when to act.
- `max_actions_per_hour` (integer, optional): Rate limit on actions (default: 10).
- Returns: Confirmation with watcher details.

### list_watchers
List all active watchers with their status and recent evaluation history.
- No parameters required.
- Returns: List of watchers with name, topics, instruction, last evaluation, actions taken.

### remove_watcher
Remove a watcher, stopping its monitoring.
- `name` (string): Name of watcher to remove.
- Returns: Confirmation of removal.

### pause_watcher
Pause or resume a watcher.
- `name` (string): Name of watcher to pause/resume.
- `paused` (boolean): True to pause, false to resume.
- Returns: Confirmation of pause/resume.
