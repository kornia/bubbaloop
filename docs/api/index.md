# API Reference

Bubbaloop uses Protocol Buffers (protobuf) for message serialization across all components.

## Overview

All messages are defined in `.proto` files located in the `protos/` directory:

```
protos/
└── bubbaloop/
    ├── header.proto      # Common header
    ├── camera.proto      # Camera messages
    └── weather.proto     # Weather messages
```

## Message Types

### Common

| Message | Description | Documentation |
|---------|-------------|---------------|
| `Header` | Common header for all messages | [Header](header.md) |

### Camera

| Message | Description | Documentation |
|---------|-------------|---------------|
| `CompressedImage` | H264/JPEG compressed frames | [Camera](camera.md) |
| `RawImage` | Decoded image data | [Camera](camera.md) |

### Weather

| Message | Description | Documentation |
|---------|-------------|---------------|
| `CurrentWeather` | Current conditions | [Weather](weather.md) |
| `HourlyForecast` | Hourly forecast | [Weather](weather.md) |
| `DailyForecast` | Daily forecast | [Weather](weather.md) |
| `LocationConfig` | Location configuration | [Weather](weather.md) |

## Package Structure

Messages are organized by package:

| Package | Prefix | Purpose |
|---------|--------|---------|
| `bubbaloop.header.v1` | Header | Common metadata |
| `bubbaloop.camera.v1` | Camera | Image data |
| `bubbaloop.weather.v1` | Weather | Weather data |

## Using the API

### Rust

```rust
use bubbaloop_protos::camera::v1::CompressedImage;
use bubbaloop_protos::header::v1::Header;

let image = CompressedImage {
    header: Some(Header {
        acq_time: timestamp_ns,
        pub_time: now_ns,
        sequence: seq,
        frame_id: "camera1".to_string(),
    }),
    format: "h264".to_string(),
    data: h264_bytes,
};

let bytes = image.encode_to_vec();
```

### TypeScript

```typescript
import { CompressedImage } from './proto/camera';
import { Header } from './proto/header';

const image = CompressedImage.decode(messageBytes);
console.log(image.header?.frameId);
console.log(image.format);
```

### Python

```python
from bubbaloop.camera.v1 import CompressedImage
from bubbaloop.header.v1 import Header

image = CompressedImage()
image.ParseFromString(message_bytes)
print(image.header.frame_id)
print(image.format)
```

## Versioning

Message packages use semantic versioning in the package name:

- `bubbaloop.camera.v1` — Version 1 of camera messages

When breaking changes occur:

1. New package version is created (e.g., `v2`)
2. Old version remains for backward compatibility
3. Migration guide provided

## Generated Code

Protobuf code is generated for:

| Language | Location | Generator |
|----------|----------|-----------|
| Rust | `crates/*/src/proto/` | `prost` |
| TypeScript | `dashboard/src/proto/` | `protobuf-ts` |

### Regenerating

To regenerate protobuf code:

```bash
# Rust
pixi run build

# TypeScript
cd dashboard && npm run proto
```

## Wire Format

Messages are serialized using standard protobuf binary format:

- Efficient binary encoding
- Cross-language compatibility
- Forward/backward compatibility for added fields

## Next Steps

- [Header](header.md) — Common message header
- [Camera Messages](camera.md) — Image message types
- [Weather Messages](weather.md) — Weather message types
- [Topics](../concepts/topics.md) — Topic naming conventions
