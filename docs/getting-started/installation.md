---
description: "Install Bubbaloop on Jetson, Raspberry Pi, or any Linux device. Pre-built ARM64 and x86_64 binaries, or build from source with Rust."
---

# Installation

Bubbaloop can be installed in two ways: using pre-built binaries (recommended for users) or building from source (for developers).

## Quick Install (Recommended)

Download and install everything with a single command:

```bash
curl -sSL https://github.com/kornia/bubbaloop/releases/latest/download/install.sh | bash
```

This installs:

| Component | Description |
|-----------|-------------|
| `zenohd` | Zenoh router for pub/sub messaging |
| `zenoh-bridge` | WebSocket bridge for browser access |
| `bubbaloop` | Single binary: CLI, daemon, MCP server, agent runtime |

After installation, start a new terminal or source your shell config, then run:

```bash
bubbaloop status
```

### Requirements

- **Linux** — x86_64 or ARM64 (Ubuntu, Jetson, Raspberry Pi)

### Verify Installation

```bash
# Check services are running
systemctl --user status zenohd
systemctl --user status bubbaloop-daemon

# Check status
bubbaloop status
```

### Upgrading

To upgrade to a new version:

```bash
curl -sSL https://github.com/kornia/bubbaloop/releases/latest/download/install.sh | bash
```

The script handles upgrading existing installations.

## Development Install

For contributors or building from source.

### System Requirements

| Platform | Status | Notes |
|----------|--------|-------|
| Ubuntu 22.04+ | Supported | Primary development platform |
| NVIDIA Jetson | Supported | Tested on JetPack 5.x/6.x |
| Raspberry Pi 5 | Supported | Tested on Raspberry Pi 5 (ARM64 Linux) |
| Docker | Supported (development) | Tested in the project devcontainer (non-systemd fallback mode) |
| macOS | Not supported | Future target; no supported install/runtime flow today |
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
| Node.js | Dashboard runtime |
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
pixi run dashboard   # Start web dashboard
```

On Docker or other environments without systemd, the daemon may fall back to a
native process supervisor for development. Treat that mode as development-only:
it is useful for bringing the daemon up, but it does not offer full systemd
behaviour or journalctl-backed logs.

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

These commands are Linux/systemd-specific. They are not the right model for the
development fallback used in Docker or other non-systemd environments.

## Troubleshooting

### "bubbaloop: command not found"

The installer adds `~/.bubbaloop/bin` to your PATH. Restart your terminal or run:

```bash
source ~/.bashrc  # or ~/.zshrc
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
