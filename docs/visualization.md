# Visualization

Real-time camera visualization via the React Dashboard.

### Features

- **H264 decoding** via WebCodecs API (hardware accelerated)
- **Drag-and-drop** camera card reordering
- **Maximize/restore** individual camera views
- **Auto-discovery** of available topics
- **Live stats** — FPS, frame count, resolution per camera
- **Metadata panel** — timestamps, latency, sequence numbers
- **HTTPS support** — self-signed cert for remote access
- **Single-port access** — Zenoh WebSocket proxied through Vite

### Quick Start

```bash
# Terminal 1: Start zenoh-bridge
pixi run bridge

# Terminal 2: Start cameras
pixi run multicam

# Terminal 3: Start dashboard
pixi run dashboard
```

**Local:** http://localhost:5173
**Remote:** https://\<ip\>:5173 (accept self-signed cert)

### Setup Details

#### 1. Start the bridge

```bash
pixi run bridge
```

This clones, builds (first time only), and runs `zenoh-bridge-remote-api`.

#### 2. Start Bubbaloop

```bash
pixi run multicam
```

#### 3. Start Dashboard

```bash
pixi run dashboard
```

### Using the Dashboard

#### Connection Status

The dashboard auto-connects via the built-in proxy. Status shown in header:

| Status | Indicator |
|--------|-----------|
| Connected | 🟢 Green dot |
| Connecting | 🟡 Yellow pulsing |
| Error | 🔴 Red (click ↻ to retry) |

#### Camera Controls

| Action | How To |
|--------|--------|
| Add Camera | Click "Add Camera" |
| Edit | Click ✏️ icon |
| Metadata | Click ⓘ icon |
| Remove | Click ✕ icon |
| Reorder | Drag grip handle |
| Maximize | Click expand icon |

#### Live Stats (per camera)

| Stat | Description |
|------|-------------|
| FPS | Frames per second (smoothed) |
| frames | Total received |
| res | Resolution (e.g., 1920×1080) |
| LIVE | Decoder active |

#### Metadata Panel

| Field | Description |
|-------|-------------|
| Format | Video codec (h264) |
| Data Size | Frame size in bytes |
| Sequence | Frame number |
| Frame ID | Camera identifier |
| Acq Time | Acquisition timestamp (ns) |
| Pub Time | Publication timestamp (ns) |
| Latency | Acq → Pub delay (ms) |

### Remote Access

The dashboard serves HTTPS with a self-signed certificate:

1. Access `https://<bubbaloop-ip>:5173` from any device
2. Accept the certificate warning
3. Zenoh WebSocket is proxied through the same port

### Browser Requirements

| Browser | Version | Status |
|---------|---------|--------|
| Chrome  | 94+ | ✅ Recommended |
| Edge    | 94+ | ✅ Supported |
| Safari  | 16.4+ | ✅ Supported |
| Firefox | — | ❌ No WebCodecs |

---

## Troubleshooting

### Dashboard: "WebSocket disconnected"

- Ensure `pixi run bridge` is running
- Check it's listening on port 10000

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
