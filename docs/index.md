# 🎥 Bubbaloop

**Multi-camera RTSP streaming with ROS-Z and real-time browser visualization.**

Bubbaloop captures H264 streams directly from RTSP cameras with **zero decode overhead** and publishes them via Zenoh/ROS-Z for real-time visualization in the React Dashboard.

## ✨ Features

- 🚀 **Zero-copy H264 passthrough** — No decoding overhead, direct stream forwarding
- 📡 **Multi-camera support** — Stream from multiple RTSP cameras simultaneously
- 🔌 **Zenoh/ROS-Z integration** — Publish camera streams as ROS-compatible topics
- 🌐 **React Dashboard** — Real-time browser visualization with WebCodecs, live FPS/latency stats
- 🔒 **Remote access** — HTTPS with self-signed cert, single-port deployment
- ⚙️ **Simple YAML config** — Easy camera configuration

## 🏗️ Tech Stack

| Component | Technology |
|-----------|------------|
| Language | Rust 🦀 |
| Video Capture | GStreamer |
| Messaging | Zenoh / ROS-Z |
| Dashboard | React + WebCodecs |
| Package Manager | Pixi |

## 🚀 Quick Start

```bash
# Install dependencies
pixi install

# Terminal 1: Start zenoh bridge
pixi run bridge

# Terminal 2: Start camera capture
pixi run multicam

# Terminal 3: Start dashboard
pixi run dashboard
```

Open http://localhost:5173 in Chrome, Edge, or Safari.

See [Quickstart](quickstart.md) for detailed setup instructions.

## 📦 Available Commands

| Command | Description |
|---------|-------------|
| `pixi run multicam` | Start camera capture and Zenoh publishing |
| `pixi run dashboard` | Start React dashboard (auto npm install) |
| `pixi run build` | Build Rust binaries |
| `pixi run docs` | Serve documentation locally |

## 👥 Community

- 💬 [Discord Server](https://discord.com/invite/HfnywwpBnD)
- 📦 [GitHub Repository](https://github.com/kornia/bubbaloop)
