# Installation

Detailed installation instructions for Bubbaloop.

## System Requirements

### Supported Platforms

| Platform | Status | Notes |
|----------|--------|-------|
| Ubuntu 22.04+ | Supported | Primary development platform |
| NVIDIA Jetson | Supported | Tested on JetPack 5.x/6.x |
| macOS | Experimental | GStreamer may require additional setup |
| Windows | Not tested | WSL2 may work |

### Hardware Requirements

- **CPU**: Any x86_64 or ARM64 processor
- **RAM**: 2GB minimum, 4GB+ recommended for multiple cameras
- **Network**: Ethernet recommended for multiple camera streams

## Installing Pixi

[Pixi](https://pixi.sh) is the package manager used by Bubbaloop. Install it with:

```bash
curl -fsSL https://pixi.sh/install.sh | sh
```

Restart your terminal or source your shell configuration:

```bash
source ~/.bashrc  # or ~/.zshrc
```

Verify the installation:

```bash
pixi --version
```

## Cloning the Repository

```bash
git clone https://github.com/kornia/bubbaloop.git
cd bubbaloop
```

## Installing Dependencies

```bash
pixi install
```

This automatically installs:

| Component | Description |
|-----------|-------------|
| Rust toolchain | Compiler for building Bubbaloop |
| GStreamer | Video capture and processing |
| Node.js | Dashboard frontend runtime |
| protobuf | Protocol buffer compiler |
| pkg-config, cmake | Build tools |

### GStreamer Plugins

The following GStreamer plugins are required:

- `gstreamer` - Core framework
- `gst-plugins-base` - Basic plugins
- `gst-plugins-good` - RTSP support
- `gst-plugins-bad` - H264 parsing
- `gst-plugins-ugly` - Additional codecs (optional)

Pixi handles all GStreamer dependencies automatically.

## Building

Build all Rust binaries:

```bash
pixi run build
```

The first build may download and compile dependencies.

### Build Components

| Binary | Description |
|--------|-------------|
| `bubbaloop` | Main TUI application |
| `cameras_node` | Camera capture service |
| `openmeteo_node` | Weather data service |

## Zenoh Bridge

The Zenoh WebSocket bridge is built automatically on first run:

```bash
pixi run bridge
```

This clones and compiles `zenoh-bridge-remote-api` from the [zenoh-ts](https://github.com/eclipse-zenoh/zenoh-ts) repository.

## Dashboard Setup

The dashboard dependencies are installed automatically:

```bash
pixi run dashboard
```

This runs `npm install` if needed and starts the development server.

## Verifying Installation

Run all services to verify the installation:

```bash
pixi run up
```

You should see:

1. Zenoh bridge starting on port 10000
2. Camera capture connecting (or reporting no config)
3. Dashboard available at http://localhost:5173

## Troubleshooting

### "pixi: command not found"

Restart your terminal or run:

```bash
source ~/.bashrc
```

### GStreamer errors

Ensure GStreamer plugins are installed:

```bash
pixi run gst-inspect-1.0 rtspsrc
pixi run gst-inspect-1.0 h264parse
```

### Build failures

Clear the build cache and retry:

```bash
pixi run cargo clean
pixi run build
```

### Network issues

If behind a proxy, configure git and cargo:

```bash
git config --global http.proxy http://proxy:port
export HTTPS_PROXY=http://proxy:port
```

## Next Steps

- [Configuration](configuration.md) — Configure cameras and services
- [Quickstart](quickstart.md) — Run your first stream
