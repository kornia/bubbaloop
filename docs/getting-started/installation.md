# Installation

Bubbaloop can be installed in two ways: using pre-built binaries (recommended for users) or building from source (for developers).

## Quick Install (Recommended)

### Step 1: Install Backend Services

Download and install Zenoh + daemon:

```bash
curl -sSL https://github.com/kornia/bubbaloop/releases/latest/download/install.sh | bash
```

This installs and starts as systemd services:

| Component | Description |
|-----------|-------------|
| `zenohd` | Zenoh router for pub/sub messaging |
| `zenoh-bridge` | WebSocket bridge for browser access |
| `bubbaloop-daemon` | Node manager for starting/stopping nodes |

### Step 2: Install TUI

Install the terminal UI via npm:

```bash
npm install -g @kornia-ai/bubbaloop
```

### Step 3: Run

```bash
bubbaloop
```

### Requirements

- **Linux** — x86_64 or ARM64 (Ubuntu, Jetson, Raspberry Pi)
- **Node.js 20+** — Required for the TUI

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
# Check services are running
systemctl --user status zenohd
systemctl --user status bubbaloop-daemon

# Run TUI
bubbaloop
```

### Upgrading

To upgrade to a new version:

```bash
# Upgrade backend
curl -sSL https://github.com/kornia/bubbaloop/releases/latest/download/install.sh | bash

# Upgrade TUI
npm update -g @kornia-ai/bubbaloop
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

### 5. Run

```bash
# Start all services
pixi run up

# Or run individually
pixi run daemon      # Start daemon
pixi run bubbaloop   # Start TUI
pixi run dashboard   # Start web dashboard
```

## Service Management

The install script sets up systemd user services:

```bash
# View status
systemctl --user status zenohd
systemctl --user status zenoh-bridge
systemctl --user status bubbaloop-daemon

# Restart services
systemctl --user restart bubbaloop-daemon

# View logs
journalctl --user -u bubbaloop-daemon -f

# Stop all services
systemctl --user stop bubbaloop-daemon zenoh-bridge zenohd
```

## Troubleshooting

### "bubbaloop: command not found"

Restart your terminal or ensure npm global bin is in PATH:

```bash
export PATH="$(npm config get prefix)/bin:$PATH"
```

### Services not starting

Check if systemd user services are enabled:

```bash
systemctl --user list-unit-files | grep bubbaloop
loginctl enable-linger $USER
```

### "pixi: command not found"

Restart your terminal or run:

```bash
source ~/.bashrc
```

### Build failures

Clear the build cache and retry:

```bash
pixi run cargo clean
pixi run build
```

## Next Steps

- [Quickstart](quickstart.md) — Run your first stream
- [Configuration](configuration.md) — Configure cameras and services
