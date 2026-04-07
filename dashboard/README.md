# Bubbaloop Dashboard

Real-time camera dashboard with H264 decoding via WebCodecs.

## Features

- **H264 decoding** — WebCodecs API (hardware accelerated)
- **Drag-and-drop** — reorder cameras by dragging
- **Maximize/restore** — expand any camera to full width
- **Auto-discovery** — topics discovered automatically via `bubbaloop/global/**`
- **Live stats** — FPS, frame count, resolution per camera
- **Metadata panel** — timestamps, latency, sequence numbers
- **HTTPS support** — self-signed certs or Tailscale serve for remote access
- **Single-port** — Zenoh WebSocket proxied through `serve.mjs`

## Quick Start

### Prerequisites

- **zenohd** running on `tcp/127.0.0.1:7447`
- **zenoh-bridge-remote-api** running in **client mode** on WS port 10001

### 1. Build the dashboard

```bash
cd dashboard
npm install
npm run build
```

### 2. Run the production server

```bash
node serve.mjs [--port 8080] [--bridge-port 10001] [--no-tls]
```

`serve.mjs` serves static files from `dist/` and proxies `/zenoh` WebSocket connections to the Zenoh bridge. This is the recommended way to run in production.

**Options:**
- `--port` — HTTP(S) port (default: 8080)
- `--bridge-port` — Zenoh bridge WS port (default: 10001)
- `--no-tls` — Force plain HTTP even if certs exist in `certs/`

### 3. HTTPS for remote access

**Option A: Tailscale serve (recommended)**

Provides trusted TLS certs with no browser warnings:

```bash
sudo tailscale serve --bg --https=443 http://localhost:8080
# Access at: https://<hostname>.<tailnet>.ts.net/
```

Run `serve.mjs` with `--no-tls` when using Tailscale (Tailscale handles TLS).

**Option B: Self-signed certs**

```bash
mkdir -p certs
openssl req -x509 -newkey rsa:2048 \
  -keyout certs/key.pem -out certs/cert.pem \
  -days 365 -nodes -subj "/CN=localhost" \
  -addext "subjectAltName=IP:<your-ip>,DNS:localhost"
node serve.mjs --port 8080
# Access at: https://<your-ip>:8080/ (accept cert warning)
```

### 4. Zenoh bridge setup

The bridge **must** run in client mode to avoid buffering all topics:

```bash
zenoh-bridge-remote-api --ws-port 10001 -e tcp/127.0.0.1:7447 -m client --no-multicast-scouting
```

**Important:** Peer mode (the default) causes the bridge to eagerly subscribe to all Zenoh topics, including high-bandwidth camera streams. This leads to excessive memory usage (GBs) even when no browser is connected.

### systemd service example

```ini
[Unit]
Description=Bubbaloop Dashboard
After=bubbaloop-bridge.service

[Service]
Type=simple
WorkingDirectory=/path/to/bubbaloop/dashboard
ExecStart=/usr/bin/node serve.mjs --port 8080 --no-tls
Restart=on-failure
RestartSec=5

[Install]
WantedBy=default.target
```

## Development

```bash
npm run dev       # Vite dev server with HMR
npm run build     # Production build
npm run preview   # Preview production build (no WS proxy)
```

Note: `npm run preview` (vite preview) does **not** proxy `/zenoh` — use `node serve.mjs` for production with WebSocket support.

## Architecture

```
RTSP Camera → camera_node (GStreamer/H264) → zenohd (tcp/7447)
                                                ↓
                              zenoh-bridge-remote-api (WS:10001, client mode)
                                                ↓
                              serve.mjs (HTTP:8080, proxies /zenoh → WS:10001)
                                                ↓
                              [optional] tailscale serve (HTTPS:443 → HTTP:8080)
                                                ↓
                                            Browser (WebCodecs H264 decode)
```

## Browser Support

| Browser | Version |
|---------|---------|
| Chrome  | 94+ |
| Edge    | 94+ |
| Safari  | 16.4+ |
| Firefox | Not supported (no WebCodecs) |
| iOS Chrome/Safari | 16.4+ (WebKit) |

## Troubleshooting

- **"WebSocket disconnected"** — Check the bridge is running and `serve.mjs` is proxying `/zenoh`
- **"Waiting for keyframe"** — Check camera node is publishing to `bubbaloop/global/**/compressed`
- **"WebCodecs not supported"** — Use Chrome/Edge/Safari; must access via HTTPS or localhost
- **Bridge using GBs of RAM** — Ensure bridge runs with `-m client`, not the default peer mode
- **No topics in dashboard** — Check bridge is connected to zenohd: `ss -tnp | grep 7447`
