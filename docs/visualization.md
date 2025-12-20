# Visualization

Bubbaloop supports multiple visualization options for real-time camera streams.

## Option 1: React Dashboard (Recommended)

The React Dashboard provides a modern web-based interface with direct H264 decoding in the browser.

### Features

- Real-time H264 video decoding using WebCodecs API
- Drag-and-drop camera card reordering
- Maximize/restore individual camera views
- Auto-discovery of available topics
- Metadata panel showing protobuf header info
- Persistent camera configuration
- Low-latency streaming via Zenoh

### Quick Start

Open **three terminals** and run:

```bash
# Terminal 1: Start zenoh-bridge-remote-api
./path/to/zenoh-bridge-remote-api --listen tcp/0.0.0.0:7448 --ws-port 10000

# Terminal 2: Start camera capture
pixi run multicam

# Terminal 3: Start dashboard
pixi run dashboard
```

Open http://localhost:5173 in your browser.

### Setup Details

#### 1. Build zenoh-bridge-remote-api (one-time)

The dashboard connects to Zenoh via WebSocket. You need the bridge:

```bash
# Clone and build
git clone https://github.com/eclipse-zenoh/zenoh-ts.git
cd zenoh-ts/zenoh-bridge-remote-api
cargo build --release
```

#### 2. Run zenoh-bridge-remote-api

```bash
./target/release/zenoh-bridge-remote-api --listen tcp/0.0.0.0:7448 --ws-port 10000
```

This starts:

- **TCP:7448** - For Rust clients (multicam)
- **WS:10000** - For browser dashboard

#### 3. Start Bubbaloop

```bash
pixi run multicam
```

#### 4. Start Dashboard

```bash
pixi run dashboard
```

!!! tip "Auto npm install"
    The `pixi run dashboard` command automatically runs `npm install` if needed.

Open http://localhost:5173 in your browser.

### Using the Dashboard

#### Connecting

1. Enter the Zenoh WebSocket endpoint (default: `ws://127.0.0.1:10000`)
2. Click **Connect**

#### Managing Cameras

| Action | How To |
|--------|--------|
| Add Camera | Click "Add Camera" button |
| Edit Camera | Click pencil (✏️) icon |
| Show Metadata | Click info (ⓘ) icon |
| Remove Camera | Click X icon |
| Reorder | Drag by grip handle |
| Maximize | Click expand icon |

#### Metadata Panel

Click the info button to view:

- **Format**: Video codec (h264)
- **Data Size**: Frame size in bytes
- **Sequence**: Frame sequence number
- **Frame ID**: Camera identifier
- **Acq/Pub Time**: Timestamps

### Browser Requirements

| Browser | Minimum Version | Status |
|---------|-----------------|--------|
| Chrome  | 94+ | ✅ Recommended |
| Edge    | 94+ | ✅ Supported |
| Safari  | 16.4+ | ✅ Supported |
| Firefox | - | ❌ Not supported |

!!! warning "Firefox not supported"
    Firefox does not support the WebCodecs API required for H264 decoding.

---

## Option 2: Foxglove Studio

[Foxglove Studio](https://foxglove.dev/studio) provides a full-featured robotics visualization platform.

!!! note "Foxglove integration temporarily disabled"
    The Foxglove WebSocket server is currently commented out in the codebase.
    To use Foxglove, you'll need to re-enable the `FoxgloveNode` in `src/bin/multicam.rs`.

### Setup (if enabled)

#### Desktop App (Recommended)

1. Download from [foxglove.dev/download](https://foxglove.dev/download)
2. Install and launch

#### Web App

Visit [app.foxglove.dev](https://app.foxglove.dev) (requires account)

### Connecting to Bubbaloop

1. Open Foxglove Studio
2. Click **Open connection** (or File → Open connection)
3. Select **Foxglove WebSocket**
4. Enter the URL:

```
ws://localhost:8765
```

!!! tip "Remote connection"
    If Bubbaloop runs on a different machine (e.g., Jetson), use its IP:
    ```
    ws://192.168.1.100:8765
    ```

5. Click **Open**

### Add Video Panels

1. Click the **+** button to add a panel
2. Select **Image** panel
3. In the panel settings, select a topic:
   - `/camera/entrance/compressed`
   - `/camera/backyard/compressed`

Repeat for each camera you want to view.

---

## Architecture Comparison

### React Dashboard

```
RTSP Camera → GStreamer → ros-z → zenoh-bridge → WebSocket → Browser (WebCodecs)
```

**Pros:**

- No desktop app required
- Direct H264 decoding in browser
- Drag-and-drop interface
- Topic auto-discovery
- Metadata inspection

**Cons:**

- Requires WebCodecs support (Chrome/Edge/Safari)
- Limited to video visualization

### Foxglove Studio

```
RTSP Camera → GStreamer → FoxgloveNode → WebSocket → Foxglove Studio
```

**Pros:**

- Full robotics visualization suite
- Supports many message types (images, point clouds, TF, etc.)
- Recording and playback
- Cross-platform desktop app

**Cons:**

- Requires separate application
- Additional WebSocket server overhead

---

## Troubleshooting

### Dashboard: "WebSocket disconnected"

- Ensure `zenoh-bridge-remote-api` is running
- Check it's listening on port 10000 (`--ws-port 10000`)

### Dashboard: "Waiting for keyframe"

- Verify `pixi run multicam` is running
- Check the topic pattern matches
- Look at browser console for subscription logs

### Dashboard: "WebCodecs not supported"

- Use Chrome 94+, Edge 94+, or Safari 16.4+
- Access via `localhost` or HTTPS (secure context required)

### Dashboard: Acq Time shows "N/A"

- Some cameras don't provide timestamps for all frames
- This is normal for certain RTSP sources
