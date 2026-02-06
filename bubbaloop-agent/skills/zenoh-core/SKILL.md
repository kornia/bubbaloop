# Zenoh Core

Interact with the Zenoh pub/sub network. Read data from topics, publish messages, and query the system.

## Tools

### subscribe_topic
Subscribe to a Zenoh topic and get the latest data.
- `topic` (string): Topic key expression (e.g., "system-telemetry/metrics"). Will be scoped automatically.
- Returns: Latest data from the topic as human-readable text.

### query_topic
Query a Zenoh key expression (used for daemon API calls).
- `key` (string): Full Zenoh key expression to query.
- `payload` (string, optional): JSON payload to send with the query.
- Returns: Query response as text.

### publish_message
Publish a message to a Zenoh topic.
- `topic` (string): Topic to publish to. Will be scoped automatically.
- `data` (string): Message content (JSON string).
- Returns: Confirmation of publish.
