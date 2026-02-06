# Data Operations

Capture and route data from Zenoh topics to persistent storage. Save streams of data to files for later analysis.

## Tools

### save_stream
Start capturing data from a topic to files.
- `topic` (string): Topic suffix to capture from (e.g., "camera/terrace/compressed").
- `output_path` (string): Directory to save data to (must be in allowed paths).
- `format` (string): Output format - "json", "csv", "raw", or "h264" (default: "json").
- `max_files` (integer, optional): Maximum number of files to keep (default: unlimited).
- Returns: Capture ID and confirmation.

### stop_capture
Stop an active data capture.
- `capture_id` (string): ID of the capture to stop.
- Returns: Confirmation with summary (files written, duration, etc.).

### list_captures
List all active data captures.
- No parameters required.
- Returns: List of captures with ID, topic, output path, format, files written, duration.
