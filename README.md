# Bubbaloop

Orchestration system for Physical AI — multi-camera RTSP streaming with ROS-Z and real-time browser visualization.

## Quick Install

### 1. Install Backend (Zenoh + Daemon)

```bash
curl -sSL https://github.com/kornia/bubbaloop/releases/latest/download/install.sh | bash
```

### 2. Install TUI

```bash
npm install -g @kornia-ai/bubbaloop
```

### 3. Run

```bash
bubbaloop
```

**Requirements:** Linux (x86_64 or ARM64), Node.js 20+

## What Gets Installed

The install script sets up systemd services:

| Service | Description |
|---------|-------------|
| `zenohd` | Zenoh router for pub/sub messaging |
| `zenoh-bridge` | WebSocket bridge for browser access |
| `bubbaloop-daemon` | Node manager for starting/stopping nodes |

The npm package provides the `bubbaloop` CLI for managing nodes.

## Development Setup

For building from source:

```bash
git clone https://github.com/kornia/bubbaloop.git
cd bubbaloop
pixi install
pixi run up
```

This launches the zenoh bridge, camera streams, and dashboard using [process-compose](https://github.com/F1bonacc1/process-compose).

**Dashboard:** http://localhost:5173

### Running services individually

```bash
pixi run daemon      # Start daemon
pixi run bubbaloop   # Start terminal UI
pixi run dashboard   # Start web dashboard
pixi run cameras     # Start camera streams
```

## Service Management

```bash
# View status
systemctl --user status bubbaloop-daemon

# Restart
systemctl --user restart bubbaloop-daemon

# View logs
journalctl --user -u bubbaloop-daemon -f
```

## Documentation

For detailed documentation, run:

```bash
pixi run docs
```

Or see the [docs/](docs/) directory.
