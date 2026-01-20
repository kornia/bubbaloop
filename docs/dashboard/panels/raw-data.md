# Raw Data Panel

The Raw Data panel displays JSON-formatted data from any topic, allowing you to inspect message contents in real-time.

## Overview

| Property | Value |
|----------|-------|
| Input | Any Protobuf message |
| Display | Formatted JSON |
| Use Case | Debugging, inspection |

## Features

- **Universal viewer** — Works with any message type
- **JSON formatting** — Pretty-printed with syntax highlighting
- **Topic selection** — Choose from discovered topics
- **Live updates** — Real-time message display
- **Metadata** — Message timestamps and sequence numbers

## Adding a Raw Data Panel

### From Topic Discovery

1. Click **Add Panel** (or add any panel type)
2. Click the edit icon
3. Select any topic from the dropdown
4. Click **Save**

### Manual Entry

1. Click **Add Panel**
2. Click the edit icon
3. Enter topic pattern: `0/weather%current/**`
4. Click **Save**

## Panel Interface

### Data Display

The main area shows the decoded message as formatted JSON:

```json
{
  "header": {
    "acqTime": "1705123456789000000",
    "pubTime": "1705123456790000000",
    "sequence": 42,
    "frameId": "weather"
  },
  "latitude": 41.4167,
  "longitude": 1.9667,
  "timezone": "Europe/Madrid",
  "temperature2m": 18.5,
  "relativeHumidity2m": 65,
  ...
}
```

### Controls

| Control | Icon | Description |
|---------|------|-------------|
| Edit | Pencil | Configure topic |
| Info | Circle-i | Show/hide metadata |
| Remove | X | Delete panel |

## Use Cases

### Debugging

View raw message data to debug:

- Message format issues
- Missing or incorrect fields
- Timestamp problems
- Sequence number gaps

### Protocol Development

When developing new components:

- Verify message serialization
- Check field values
- Test new message types

### Data Inspection

Inspect data for analysis:

- Weather data values
- Sensor readings
- System status

## Topic Selection

### Available Topics

The panel can display data from any topic:

| Topic | Message Type |
|-------|--------------|
| `0/camera%name%compressed/**` | CompressedImage |
| `0/weather%current/**` | CurrentWeather |
| `0/weather%hourly/**` | HourlyForecast |
| `0/weather%daily/**` | DailyForecast |

### Custom Topics

Enter any valid Zenoh key expression to subscribe to custom topics.

## Message Decoding

### Protobuf Messages

For known message types, the panel:

1. Receives binary protobuf data
2. Decodes using generated protobuf classes
3. Converts to JSON representation
4. Displays formatted output

### Unknown Messages

For unknown message types:

1. Displays raw binary as hex
2. Shows message size
3. Includes timestamp information

## Data Formats

### Timestamps

Timestamps are displayed as nanoseconds since Unix epoch:

```json
{
  "acqTime": "1705123456789000000"
}
```

To convert: divide by 1,000,000,000 for seconds.

### Binary Data

Binary fields (like `data` in CompressedImage) are truncated:

```json
{
  "data": "<binary: 12345 bytes>"
}
```

### Nested Objects

Nested messages are fully expanded:

```json
{
  "header": {
    "acqTime": "...",
    "pubTime": "...",
    "sequence": 1,
    "frameId": "camera1"
  },
  "entries": [
    { "time": "...", "temperature2m": 20.5 },
    { "time": "...", "temperature2m": 21.0 }
  ]
}
```

## Performance

### Update Rate

- Messages displayed as received
- High-frequency topics may update rapidly
- Consider using specific topics rather than wildcards

### Memory

- Recent messages are kept in memory
- Panel clears on refresh
- Large messages may impact performance

## Troubleshooting

### No data displayed

1. Verify the topic is correct
2. Check that the source is publishing
3. Use `/topics` in TUI to verify activity

### Decoding errors

1. Check message type matches topic
2. Verify protobuf definitions are current
3. Check browser console for errors

### Truncated data

1. Binary fields are intentionally truncated
2. Use specific message type panels for full data
3. Check raw bytes in browser developer tools

## Next Steps

- [Topics](../../concepts/topics.md) — Topic naming conventions
- [API Reference](../../api/index.md) — Message type definitions
- [Dashboard Overview](../index.md) — Other panel types
