# Camera Messages

Camera messages define the format for image data published by camera sensors.

## Package

```
bubbaloop.camera.v1
```

## Messages

### CompressedImage

Compressed video frame (H264, JPEG, etc.)

```protobuf
syntax = "proto3";

package bubbaloop.camera.v1;

import "bubbaloop/header.proto";

// Compressed image (H264, JPEG, etc.)
message CompressedImage {
    bubbaloop.header.v1.Header header = 1;
    string format = 2;      // "h264", "jpeg", etc.
    bytes data = 3;
}
```

#### Fields

| Field | Type | Number | Description |
|-------|------|--------|-------------|
| `header` | Header | 1 | Message metadata ([see Header](header.md)) |
| `format` | string | 2 | Compression format identifier |
| `data` | bytes | 3 | Compressed image/video data |

#### Format Values

| Format | Description | Use Case |
|--------|-------------|----------|
| `"h264"` | H.264/AVC video codec | RTSP streams, video |
| `"jpeg"` | JPEG image | Snapshots, thumbnails |
| `"hevc"` | H.265/HEVC video codec | Future support |

#### H264 Data Format

For H264 format, the `data` field contains:

- **Annex B format** — NAL units with start codes (0x00000001)
- **SPS/PPS included** — Sequence and Picture Parameter Sets before keyframes
- **Complete NAL units** — Each message contains one or more complete NAL units

**NAL unit structure:**

```
[0x00 0x00 0x00 0x01] [NAL Header] [NAL Payload]
```

### RawImage

Raw decoded image data (RGB, grayscale, etc.)

```protobuf
// Raw decoded image (RGB, BGR, grayscale, etc.)
message RawImage {
    bubbaloop.header.v1.Header header = 1;
    uint32 width = 2;
    uint32 height = 3;
    string encoding = 4;    // "rgb8", "bgr8", "mono8", etc.
    uint32 step = 5;        // Row stride in bytes
    bytes data = 6;
}
```

#### Fields

| Field | Type | Number | Description |
|-------|------|--------|-------------|
| `header` | Header | 1 | Message metadata |
| `width` | uint32 | 2 | Image width in pixels |
| `height` | uint32 | 3 | Image height in pixels |
| `encoding` | string | 4 | Pixel encoding format |
| `step` | uint32 | 5 | Row stride in bytes |
| `data` | bytes | 6 | Raw pixel data |

#### Encoding Values

| Encoding | Bytes/Pixel | Description |
|----------|-------------|-------------|
| `"rgb8"` | 3 | 8-bit RGB |
| `"bgr8"` | 3 | 8-bit BGR (OpenCV default) |
| `"rgba8"` | 4 | 8-bit RGBA |
| `"mono8"` | 1 | 8-bit grayscale |
| `"mono16"` | 2 | 16-bit grayscale |

#### Step Calculation

```
step = width * bytes_per_pixel
```

With padding:

```
step = align_to_4(width * bytes_per_pixel)
```

## Topics

### CompressedImage Topics

| Topic | Description |
|-------|-------------|
| `/camera/{name}/compressed` | H264 compressed frames |

**Zenoh key expression:**

```
0/camera%{name}%compressed/**
```

### RawImage Topics (Future)

| Topic | Description |
|-------|-------------|
| `/camera/{name}/image_raw` | Decoded RGB images |

## Usage Examples

### Rust - Publishing

```rust
use bubbaloop_protos::camera::v1::CompressedImage;
use bubbaloop_protos::header::v1::Header;
use prost::Message;

fn publish_frame(
    publisher: &Publisher,
    frame_id: &str,
    sequence: u32,
    h264_data: Vec<u8>,
    acq_time: u64,
) -> Result<()> {
    let header = Header {
        acq_time,
        pub_time: now_ns(),
        sequence,
        frame_id: frame_id.to_string(),
    };

    let image = CompressedImage {
        header: Some(header),
        format: "h264".to_string(),
        data: h264_data,
    };

    let bytes = image.encode_to_vec();
    publisher.put(bytes).await?;
    Ok(())
}
```

### TypeScript - Subscribing

```typescript
import { CompressedImage } from './proto/camera';

subscriber.callback = (sample) => {
    const image = CompressedImage.decode(sample.payload);

    if (image.format === 'h264') {
        // Decode H264 with WebCodecs
        const chunk = new EncodedVideoChunk({
            type: isKeyframe(image.data) ? 'key' : 'delta',
            timestamp: Number(image.header?.acqTime) / 1000, // ns to us
            data: image.data,
        });
        videoDecoder.decode(chunk);
    }
};
```

### Python - Processing

```python
from bubbaloop.camera.v1 import CompressedImage, RawImage
import cv2
import numpy as np

def process_raw_image(raw: RawImage) -> np.ndarray:
    # Convert to numpy array
    data = np.frombuffer(raw.data, dtype=np.uint8)

    if raw.encoding == 'rgb8':
        image = data.reshape((raw.height, raw.width, 3))
    elif raw.encoding == 'mono8':
        image = data.reshape((raw.height, raw.width))
    else:
        raise ValueError(f"Unknown encoding: {raw.encoding}")

    return image
```

## H264 Keyframe Detection

Keyframes (I-frames) are identified by NAL unit type:

```typescript
function isKeyframe(data: Uint8Array): boolean {
    // Find NAL unit type after start code
    for (let i = 0; i < data.length - 4; i++) {
        if (data[i] === 0 && data[i+1] === 0 && data[i+2] === 0 && data[i+3] === 1) {
            const nalType = data[i+4] & 0x1F;
            // IDR slice = keyframe
            if (nalType === 5) return true;
        }
    }
    return false;
}
```

## WebCodecs Integration

The dashboard uses WebCodecs for H264 decoding:

```typescript
const decoder = new VideoDecoder({
    output: (frame) => {
        // Render frame to canvas
        ctx.drawImage(frame, 0, 0);
        frame.close();
    },
    error: (e) => console.error('Decode error:', e),
});

// Configure decoder from SPS/PPS
decoder.configure({
    codec: 'avc1.640028',  // H264 High Profile Level 4.0
    codedWidth: 1920,
    codedHeight: 1080,
    hardwareAcceleration: 'prefer-hardware',
});
```

## Data Size

Typical message sizes:

| Format | Resolution | Size |
|--------|------------|------|
| H264 keyframe | 1080p | 50-200 KB |
| H264 P-frame | 1080p | 5-50 KB |
| JPEG | 1080p | 100-500 KB |
| RGB8 raw | 1080p | ~6 MB |

## Next Steps

- [Header](header.md) — Common header fields
- [Weather Messages](weather.md) — Weather API
- [RTSP Camera](../components/sensors/rtsp-camera.md) — Camera configuration
- [Camera Panel](../dashboard/panels/camera.md) — Dashboard visualization
