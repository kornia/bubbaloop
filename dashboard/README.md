# Bubbaloop Dashboard

React-based real-time camera dashboard that subscribes to camera streams via Zenoh and decodes H264 video using the WebCodecs API.

## Features

- **Real-time H264 video decoding** using WebCodecs API (hardware accelerated)
- **Drag-and-drop camera cards** - reorder cameras by dragging
- **Maximize/restore views** - expand any camera to full width
- **Dynamic topic selection** - auto-discover topics or enter manually
- **Metadata panel** - view protobuf header info (format, sequence, timestamps)
- **Persistent configuration** - camera layout and settings saved to localStorage
- **Low-latency streaming** via Zenoh pub/sub

## Quick Start

From the project root:

```bash
# Start zenoh-bridge-remote-api (in terminal 1)
./path/to/zenoh-bridge-remote-api --listen tcp/0.0.0.0:7448 --ws-port 10000

# Start camera streams (in terminal 2)
pixi run multicam

# Start dashboard (in terminal 3)
pixi run dashboard
```

Open http://localhost:5173 in your browser.

## Prerequisites

1. **zenoh-bridge-remote-api**: The dashboard connects to Zenoh via WebSocket. You need to run the `zenoh-bridge-remote-api` which provides WebSocket access to the Zenoh network.

2. **Modern Browser**: WebCodecs API required (Chrome 94+, Edge 94+, Safari 16.4+)

## Setup Details

### Building zenoh-bridge-remote-api

```bash
# Clone zenoh-ts repository (contains the bridge)
git clone https://github.com/eclipse-zenoh/zenoh-ts.git
cd zenoh-ts/zenoh-bridge-remote-api

# Build the bridge
cargo build --release

# Run the bridge
./target/release/zenoh-bridge-remote-api --listen tcp/0.0.0.0:7448 --ws-port 10000
```

This starts:
- TCP listener on port 7448 (for Rust clients like multicam)
- WebSocket server on port 10000 (for the browser dashboard)

### Multicam Configuration

The Rust multicam application connects to the bridge. In `src/bin/multicam.rs`:

```rust
let ctx = Arc::new(
    ZContextBuilder::default()
        .with_json("connect/endpoints", json!(["tcp/127.0.0.1:7448"]))
        .build()?
);
```

## Usage

### Connecting to Zenoh

1. Enter the Zenoh WebSocket endpoint (default: `ws://127.0.0.1:10000`)
2. Click **Connect**

### Managing Cameras

- **Add Camera**: Click the "Add Camera" button
- **Edit Camera**: Click the edit (pencil) icon to change name or topic
- **Show Metadata**: Click the info (ⓘ) icon to view protobuf header details
- **Remove Camera**: Click the X icon to remove a camera
- **Reorder**: Drag cameras by the grip handle to reorder
- **Maximize**: Click the maximize icon to expand a camera to full width

### Topic Selection

When editing a camera, you can:
- **Select from dropdown**: Available topics are auto-discovered
- **Enter manually**: Type a custom topic pattern (e.g., `0/camera%entrance%compressed/**`)

### Metadata Panel

Click the info (ⓘ) button to view:
- **Format**: Video codec format (h264)
- **Data Size**: Frame size in bytes
- **Sequence**: Frame sequence number
- **Frame ID**: Camera identifier
- **Acq Time**: Acquisition timestamp (from GStreamer)
- **Pub Time**: Publication timestamp (Unix epoch)

## Development

Using pixi (recommended):

```bash
pixi run dashboard          # Start dev server with hot reload
pixi run dashboard-build    # Build for production
```

Or using npm directly:

```bash
cd dashboard
npm install
npm run dev      # Start dev server with hot reload
npm run build    # Build for production
npm run preview  # Preview production build
```

## Architecture

```
┌──────────────────┐     ┌─────────────────────────┐     ┌──────────────────┐
│  RTSP Cameras    │────▶│  Bubbaloop (multicam)   │────▶│  zenoh-bridge-   │
│                  │     │  - GStreamer H264       │     │  remote-api      │
└──────────────────┘     │  - ros-z publisher      │     │  - TCP:7448      │
                         └─────────────────────────┘     │  - WS:10000      │
                                                         └────────┬─────────┘
                                                                  │ WebSocket
                                                                  ▼
                                                        ┌──────────────────┐
                                                        │  React Dashboard │
                                                        │  - zenoh-ts      │
                                                        │  - WebCodecs     │
                                                        │  - dnd-kit       │
                                                        └──────────────────┘
```

## Troubleshooting

### "WebSocket disconnected" or Connection Errors

- Ensure `zenoh-bridge-remote-api` is running on port 10000
- Check that the bridge has the correct `--ws-port` flag

### No Video / "Waiting for keyframe"

- Verify multicam is running and connected to the bridge
- Check browser console for Zenoh subscription logs
- Ensure the topic pattern matches (use `**` for wildcards)

### "WebCodecs not supported"

- Use a supported browser: Chrome 94+, Edge 94+, or Safari 16.4+
- Ensure you're accessing via `localhost` or HTTPS (WebCodecs requires secure context)

### Video Stuttering

- The dashboard uses unbounded channels; check network latency
- For high-resolution streams, ensure sufficient CPU/GPU for decoding

### Acq Time shows "N/A"

- Some cameras/streams don't provide timestamps for all frames
- This is normal behavior for certain RTSP sources

## Browser Requirements

| Browser | Minimum Version |
|---------|-----------------|
| Chrome  | 94+             |
| Edge    | 94+             |
| Safari  | 16.4+           |
| Firefox | Not supported (no WebCodecs) |
