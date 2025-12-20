# 🏗️ Architecture

Bubbaloop is designed for efficient multi-camera streaming with minimal CPU overhead.

## System Overview

```
┌─────────────────┐     ┌─────────────────┐     ┌─────────────────┐
│  RTSP Camera 1  │     │  RTSP Camera 2  │     │  RTSP Camera N  │
└────────┬────────┘     └────────┬────────┘     └────────┬────────┘
         │                       │                       │
         ▼                       ▼                       ▼
┌─────────────────────────────────────────────────────────────────┐
│                    GStreamer H264 Capture                       │
│  ┌──────────┐   ┌───────────┐   ┌──────────┐   ┌──────────┐    │
│  │ rtspsrc  │ → │rtph264depay│ → │ h264parse │ → │ appsink  │    │
│  └──────────┘   └───────────┘   └──────────┘   └──────────┘    │
└─────────────────────────────────────────────────────────────────┘
                                   │
                                   ▼
┌─────────────────────────────────────────────────────────────────┐
│                         ROS-Z / Zenoh                           │
│                                                                 │
│   /camera/cam1/compressed  →  Protobuf CompressedImage         │
│   /camera/cam2/compressed  →  Protobuf CompressedImage         │
│   /camera/camN/compressed  →  Protobuf CompressedImage         │
└─────────────────────────────────────────────────────────────────┘
                                   │
                                   ▼
┌─────────────────────────────────────────────────────────────────┐
│                      Foxglove Bridge Node                       │
│                                                                 │
│   Subscribe to /camera/*/compressed                            │
│   Convert to Foxglove CompressedVideo                          │
│   Serve via WebSocket on port 8765                             │
└─────────────────────────────────────────────────────────────────┘
                                   │
                                   ▼
┌─────────────────────────────────────────────────────────────────┐
│                       Foxglove Studio                           │
│                   ws://host:8765                                │
└─────────────────────────────────────────────────────────────────┘
```

## Components

### H264 Stream Capture

Located in `src/h264_capture.rs`, this component:

- Creates a GStreamer pipeline for each camera
- Receives H264 NAL units without decoding (zero CPU overhead)
- Injects SPS/PPS headers before each keyframe for stream compatibility
- Uses zero-copy buffer mapping for efficiency

**GStreamer Pipeline:**

```
rtspsrc location={url} latency={latency}
  ! rtph264depay
  ! h264parse config-interval=-1
  ! video/x-h264,stream-format=byte-stream,alignment=au
  ! appsink emit-signals=true sync=false
```

### RTSP Camera Node

Located in `src/rtsp_camera_node.rs`, each camera node:

- Wraps the H264 capture in a ROS-Z node
- Publishes `CompressedImage` messages via Zenoh
- Handles graceful shutdown on Ctrl+C

### Foxglove Bridge Node

Located in `src/foxglove_node.rs`, this component:

- Subscribes to all camera topics via ROS-Z
- Converts messages to Foxglove's `CompressedVideo` schema
- Serves a WebSocket server on port 8765
- Handles multiple concurrent Foxglove clients

## Message Format

### Protobuf Schema

```protobuf
// protos/camera.proto
message Header {
  uint64 acq_time = 1;   // Acquisition timestamp (nanoseconds)
  uint64 pub_time = 2;   // Publication timestamp (nanoseconds)
  uint32 sequence = 3;   // Frame sequence number
  string frame_id = 4;   // Camera name
}

message CompressedImage {
  Header header = 1;
  string format = 2;     // Always "h264"
  bytes data = 3;        // H264 NAL units (Annex B)
}
```

### Foxglove CompressedVideo

The bridge converts to Foxglove's native schema:

```json
{
  "timestamp": { "sec": 1234567890, "nsec": 123456789 },
  "frame_id": "entrance",
  "format": "h264",
  "data": "<base64 H264 data>"
}
```

## Threading Model

```
┌─────────────────────────────────────────────────────┐
│                    Main Thread                       │
│  - Configuration loading                            │
│  - Ctrl+C signal handling                           │
│  - Task spawning                                    │
└─────────────────────────────────────────────────────┘
         │
         ├──────────────────┬──────────────────┬─────────────────┐
         ▼                  ▼                  ▼                 ▼
┌─────────────────┐ ┌─────────────────┐ ┌─────────────────┐ ┌─────────────┐
│ Camera 1 Task   │ │ Camera 2 Task   │ │ Camera N Task   │ │ Foxglove    │
│ (Tokio spawn)   │ │ (Tokio spawn)   │ │ (Tokio spawn)   │ │ Bridge Task │
│                 │ │                 │ │                 │ │             │
│ GStreamer runs  │ │ GStreamer runs  │ │ GStreamer runs  │ │ WebSocket   │
│ in own thread   │ │ in own thread   │ │ in own thread   │ │ server      │
└─────────────────┘ └─────────────────┘ └─────────────────┘ └─────────────┘
```

## Dependencies

| Crate | Purpose |
|-------|---------|
| `gstreamer` | Video capture pipeline |
| `ros-z` | ROS-compatible pub/sub over Zenoh |
| `zenoh` | Distributed messaging |
| `foxglove` | WebSocket server for Foxglove |
| `prost` | Protobuf serialization |
| `tokio` | Async runtime |

