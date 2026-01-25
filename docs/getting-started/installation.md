# Installation

Bubbaloop can be installed in two ways: using pre-built binaries (recommended for users) or building from source (for developers).

## Quick Install (Recommended)

Download and install the latest release:

```bash
curl -sSL https://github.com/kornia/bubbaloop/releases/latest/download/install.sh | bash
```

This installs:

| Component | Location |
|-----------|----------|
| `bubbaloop` | `~/.bubbaloop/bubbaloop` (TUI) |
| `bubbaloop-daemon` | `~/.bubbaloop/bubbaloop-daemon` |

The installer adds `~/.bubbaloop` to your PATH.

### Requirements

- **Node.js 20+** — Required for the TUI
- **Linux** — x86_64 or ARM64 (Ubuntu, Jetson, Raspberry Pi)

Install Node.js if needed:

```bash
# Ubuntu/Debian
curl -fsSL https://deb.nodesource.com/setup_20.x | sudo -E bash -
sudo apt-get install -y nodejs

# Or use nvm
curl -o- https://raw.githubusercontent.com/nvm-sh/nvm/v0.39.0/install.sh | bash
nvm install 20
```

### Verify Installation

```bash
bubbaloop --help
```

## Development Install

For contributors or building from source.

### System Requirements

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

### 1. Install Pixi

[Pixi](https://pixi.sh) is the package manager used by Bubbaloop:

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

### 2. Clone the Repository

```bash
git clone https://github.com/kornia/bubbaloop.git
cd bubbaloop
```

### 3. Install Dependencies

```bash
pixi install
```

This automatically installs:

| Component | Description |
|-----------|-------------|
| Rust toolchain | Compiler for building Bubbaloop |
| GStreamer | Video capture and processing |
| Node.js | Dashboard and TUI runtime |
| protobuf | Protocol buffer compiler |
| pkg-config, cmake | Build tools |

### 4. Build

Build all Rust binaries:

```bash
pixi run build
```

### GStreamer Plugins

The following GStreamer plugins are installed automatically:

- `gstreamer` - Core framework
- `gst-plugins-base` - Basic plugins
- `gst-plugins-good` - RTSP support
- `gst-plugins-bad` - H264 parsing
- `gst-plugins-ugly` - Additional codecs (optional)

## Zenoh Installation

Bubbaloop uses [Zenoh](https://zenoh.io/) for messaging. Install the Zenoh router:

```bash
# Using cargo (if Rust is installed)
cargo install zenoh

# Or download from releases
# https://github.com/eclipse-zenoh/zenoh/releases
```

For browser connectivity, install the WebSocket bridge:

```bash
cargo install zenoh-bridge-remote-api
```

## Verifying Installation

### Binary Install

```bash
# Start the TUI
bubbaloop
```

### Development Install

```bash
# Run all services
pixi run up
```

You should see:

1. Zenoh bridge starting on port 10000
2. Camera capture connecting (or reporting no config)
3. Dashboard available at http://localhost:5173

## Troubleshooting

### "bubbaloop: command not found"

Restart your terminal or add to PATH manually:

```bash
export PATH="$HOME/.bubbaloop:$PATH"
```

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

- [Quickstart](quickstart.md) — Run your first stream
- [Configuration](configuration.md) — Configure cameras and services
