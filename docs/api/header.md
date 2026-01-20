# Header

The `Header` message contains common metadata for all Bubbaloop messages.

## Package

```
bubbaloop.header.v1
```

## Definition

```protobuf
syntax = "proto3";

package bubbaloop.header.v1;

// Common header for all messages
message Header {
    uint64 acq_time = 1;    // Acquisition timestamp (nanoseconds)
    uint64 pub_time = 2;    // Publication timestamp (nanoseconds)
    uint32 sequence = 3;    // Sequence number
    string frame_id = 4;    // Frame/source identifier
}
```

## Fields

| Field | Type | Number | Description |
|-------|------|--------|-------------|
| `acq_time` | uint64 | 1 | Acquisition timestamp in nanoseconds since Unix epoch |
| `pub_time` | uint64 | 2 | Publication timestamp in nanoseconds since Unix epoch |
| `sequence` | uint32 | 3 | Monotonically increasing sequence number |
| `frame_id` | string | 4 | Identifier for the data source (camera name, sensor ID, etc.) |

## Field Details

### acq_time

The time when the data was acquired or captured:

- For cameras: When the frame was captured by the sensor
- For weather: When the API data was generated
- **Unit:** Nanoseconds since Unix epoch (1970-01-01 00:00:00 UTC)

**Example:** `1705123456789000000` = 2024-01-13 12:17:36.789 UTC

**Note:** Some sources may not provide acquisition timestamps, in which case this field may be 0.

### pub_time

The time when the message was published to Zenoh:

- Always set by the publishing node
- Used to calculate publication latency
- **Unit:** Nanoseconds since Unix epoch

**Latency calculation:**

```
latency_ms = (pub_time - acq_time) / 1_000_000
```

### sequence

A monotonically increasing counter for ordering:

- Starts at 0 or 1 when the node starts
- Increments for each message published
- Wraps at uint32 max (4,294,967,295)

**Uses:**

- Detect dropped messages (gaps in sequence)
- Order messages if received out of order
- Track message rates

### frame_id

Identifies the source of the data:

- For cameras: Camera name from configuration (e.g., "front_door")
- For weather: Service identifier (e.g., "openmeteo")
- For sensors: Sensor name or ID

## Usage Examples

### Rust

```rust
use bubbaloop_protos::header::v1::Header;
use std::time::{SystemTime, UNIX_EPOCH};

fn create_header(frame_id: &str, sequence: u32, acq_time: u64) -> Header {
    let pub_time = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .as_nanos() as u64;

    Header {
        acq_time,
        pub_time,
        sequence,
        frame_id: frame_id.to_string(),
    }
}
```

### TypeScript

```typescript
import { Header } from './proto/header';

function parseHeader(header: Header | undefined) {
    if (!header) return null;

    const acqTime = new Date(Number(header.acqTime) / 1_000_000);
    const pubTime = new Date(Number(header.pubTime) / 1_000_000);
    const latencyMs = (Number(header.pubTime) - Number(header.acqTime)) / 1_000_000;

    return {
        frameId: header.frameId,
        sequence: header.sequence,
        acqTime,
        pubTime,
        latencyMs,
    };
}
```

### Python

```python
from bubbaloop.header.v1 import Header
from datetime import datetime

def parse_header(header: Header):
    acq_time = datetime.fromtimestamp(header.acq_time / 1e9)
    pub_time = datetime.fromtimestamp(header.pub_time / 1e9)
    latency_ms = (header.pub_time - header.acq_time) / 1e6

    return {
        'frame_id': header.frame_id,
        'sequence': header.sequence,
        'acq_time': acq_time,
        'pub_time': pub_time,
        'latency_ms': latency_ms,
    }
```

## Timestamp Conversion

### Nanoseconds to Milliseconds

```
ms = ns / 1_000_000
```

### Nanoseconds to Seconds

```
seconds = ns / 1_000_000_000
```

### To JavaScript Date

```javascript
const date = new Date(ns / 1_000_000);
```

### To Python datetime

```python
from datetime import datetime
dt = datetime.fromtimestamp(ns / 1e9)
```

## Messages Using Header

All Bubbaloop messages include a Header:

| Message | Package |
|---------|---------|
| `CompressedImage` | bubbaloop.camera.v1 |
| `RawImage` | bubbaloop.camera.v1 |
| `CurrentWeather` | bubbaloop.weather.v1 |
| `HourlyForecast` | bubbaloop.weather.v1 |
| `DailyForecast` | bubbaloop.weather.v1 |

## Next Steps

- [Camera Messages](camera.md) — Image message types
- [Weather Messages](weather.md) — Weather message types
- [Topics](../concepts/topics.md) — Topic naming conventions
