# Bubbaloop Dashboard

Real-time camera dashboard with H264 decoding via WebCodecs.

## Features

- **H264 decoding** — WebCodecs API (hardware accelerated)
- **Drag-and-drop** — reorder cameras by dragging
- **Maximize/restore** — expand any camera to full width
- **Auto-discovery** — topics discovered automatically
- **Live stats** — FPS, frame count, resolution per camera
- **Metadata panel** — timestamps, latency, sequence numbers
- **HTTPS support** — self-signed cert for remote access
- **Single-port** — Zenoh WebSocket proxied through Vite

## Quick Start

```bash
pixi run bridge     # Terminal 1: zenoh bridge
pixi run multicam   # Terminal 2: camera streams
pixi run dashboard  # Terminal 3: dashboard
```

**Local:** http://localhost:5173
**Remote:** https://\<ip\>:5173 (accept self-signed cert)

## Usage

### Connection

Auto-connects via built-in proxy. Status in header:
- 🟢 Connected | 🟡 Connecting | 🔴 Error (click ↻)

### Camera Controls

| Action | How |
|--------|-----|
| Add | Click "Add Camera" |
| Edit | ✏️ icon |
| Metadata | ⓘ icon |
| Remove | ✕ icon |
| Reorder | Drag grip |
| Maximize | Expand icon |

### Live Stats

Each camera shows: **FPS** · **frames** · **resolution** · **LIVE/INIT**

### Metadata Panel (ⓘ)

Shows format, data size, sequence, frame ID, timestamps, and **latency** (acq → pub).

## Development

```bash
pixi run dashboard          # Dev server
pixi run dashboard-build    # Production build
```

Or with npm:

```bash
cd dashboard && npm install && npm run dev
```

## Architecture

```
RTSP Cameras → multicam (GStreamer) → zenoh-bridge → Dashboard (WebCodecs)
                                         ↓
                              Vite proxies /zenoh → WS:10000
```

## Browser Support

| Browser | Version |
|---------|---------|
| Chrome  | 94+ ✅ |
| Edge    | 94+ ✅ |
| Safari  | 16.4+ ✅ |
| Firefox | ❌ No WebCodecs |

## Troubleshooting

- **"WebSocket disconnected"** — Check `pixi run bridge` is running
- **"Waiting for keyframe"** — Check `pixi run multicam` is running
- **"WebCodecs not supported"** — Use Chrome/Edge/Safari, access via localhost or HTTPS
