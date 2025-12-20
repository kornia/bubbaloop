# Architecture

Bubbaloop is designed for efficient multi-camera streaming with minimal CPU overhead.

## System Overview

```mermaid
flowchart TB
    subgraph cameras [RTSP Cameras]
        cam1[Camera 1]
        cam2[Camera 2]
        camN[Camera N]
    end

    subgraph gstreamer [GStreamer H264 Passthrough]
        direction LR
        rtspsrc[rtspsrc] --> rtph264depay[rtph264depay] --> h264parse[h264parse] --> appsink[appsink]
    end

    subgraph zenoh [ROS-Z / Zenoh]
        topic1["/camera/cam1/compressed"]
        topic2["/camera/cam2/compressed"]
        topicN["/camera/camN/compressed"]
    end

    subgraph foxglove [Foxglove Bridge]
        ws[WebSocket :8765]
    end

    studio[Foxglove Studio]

    cam1 -->|H264| gstreamer
    cam2 -->|H264| gstreamer
    camN -->|H264| gstreamer

    gstreamer -->|CompressedImage| topic1
    gstreamer -->|CompressedImage| topic2
    gstreamer -->|CompressedImage| topicN

    topic1 --> ws
    topic2 --> ws
    topicN --> ws

    ws -->|CompressedVideo| studio
```

## Components

### H264 Stream Capture

Located in `src/h264_capture.rs`, this component:

- Creates a GStreamer pipeline for each camera
- Receives H264 NAL units without decoding (zero CPU overhead)
- Injects SPS/PPS headers before each keyframe for stream compatibility
- Uses zero-copy buffer mapping for efficiency

**GStreamer Pipeline:**

```mermaid
flowchart LR
    A[rtspsrc] -->|RTP| B[rtph264depay]
    B -->|H264 AVC| C[h264parse]
    C -->|"Annex B + SPS/PPS"| D[appsink]

    style A fill:#e1f5fe
    style D fill:#c8e6c9
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