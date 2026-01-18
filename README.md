# Bubbaloop

Multi-camera RTSP streaming with ROS-Z and real-time browser visualization.

Captures H264 streams directly from RTSP cameras (zero decode overhead) and publishes them via Zenoh/ROS-Z for visualization in the React Dashboard.

## Requirements

- [Pixi](https://pixi.sh) for dependency management
- [zenoh-bridge-remote-api](https://github.com/eclipse-zenoh/zenoh-ts) for browser connectivity

## Quick Start

### 1. Install dependencies

```bash
pixi install
```

### 2. Configure cameras

Edit `config.yaml` to add your RTSP cameras:

```yaml
cameras:
  - name: "entrance"
    url: "rtsp://user:pass@192.168.1.10:554/stream"
    latency: 200
  - name: "backyard"
    url: "rtsp://192.168.1.11:554/live"
    latency: 100
```

### 3. Start everything

```bash
pixi run up
```

This launches the zenoh bridge, camera streams, and dashboard using [process-compose](https://github.com/F1bonacc1/process-compose).

**Local:** http://localhost:5173
**Remote:** https://\<ip\>:5173 (accept self-signed cert)

### Running services individually

```bash
pixi run bridge      # Start zenoh bridge (local WebSocket)
pixi run cameras     # Start camera streams (multicast scouting)
pixi run dashboard   # Start web dashboard
pixi run bubbaloop   # Start terminal UI
```

### Remote access (TUI/dashboard from laptop to robot)

```bash
# On robot:
zenohd -c zenoh.json5       # Zenoh router
pixi run cameras            # Cameras (auto-connects to local router)

# On laptop (first-time setup):
pixi run bubbaloop          # Run /server to set robot IP

# On laptop (run services):
pixi run zenohd-client      # Local router → robot
pixi run bubbaloop          # TUI → /connect → /topics
```

See [docs/configuration.md](docs/configuration.md#remote-access-setup) for detailed setup.

## Documentation

For detailed documentation, run:

```bash
pixi run docs
```

Or see the [docs/](docs/) directory.
