# 🎥 Bubbaloop

**Multi-camera RTSP streaming with ROS-Z and Foxglove.**

Bubbaloop captures H264 streams directly from RTSP cameras with **zero decode overhead** and publishes them via Zenoh/ROS-Z for real-time visualization in Foxglove Studio.

## ✨ Features

- 🚀 **Zero-copy H264 passthrough** — No decoding overhead, direct stream forwarding
- 📡 **Multi-camera support** — Stream from multiple RTSP cameras simultaneously  
- 🔌 **Zenoh/ROS-Z integration** — Publish camera streams as ROS-compatible topics
- 📊 **Foxglove visualization** — Real-time video streaming to Foxglove Studio
- ⚙️ **Simple YAML config** — Easy camera configuration

## 🏗️ Tech Stack

| Component | Technology |
|-----------|------------|
| Language | Rust 🦀 |
| Video Capture | GStreamer |
| Messaging | Zenoh / ROS-Z |
| Visualization | Foxglove WebSocket |
| Package Manager | Pixi |

## 🚀 Quick Start

```bash
# Install pixi
curl -fsSL https://pixi.sh/install.sh | sh

# Clone and enter the project
git clone https://github.com/kornia/bubbaloop.git
cd bubbaloop

# Install dependencies
pixi install

# Configure cameras
vim config.yaml

# Run
pixi run multicam
```

Then connect Foxglove Studio to `ws://<host>:8765` to view your camera streams.

## 👥 Community

- 💬 [Discord Server](https://discord.com/invite/HfnywwpBnD)
- 📦 [GitHub Repository](https://github.com/kornia/bubbaloop)

