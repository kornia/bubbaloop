# Bubbaloop

Multi-camera RTSP streaming with ROS-Z and real-time browser visualization.

Captures H264 streams directly from RTSP cameras (zero decode overhead) and publishes them via Zenoh/ROS-Z for visualization in the React Dashboard or Foxglove Studio.

## Requirements

- [Pixi](https://pixi.sh) for dependency management
- [zenoh-bridge-remote-api](https://github.com/eclipse-zenoh/zenoh-ts) for browser connectivity

## Quick Start

### 1. Install dependencies

```bash
pixi install
```

### 2. Build and run zenoh-bridge-remote-api (one-time setup)

The bridge provides WebSocket access for browsers to connect to Zenoh:

```bash
# Clone and build
git clone https://github.com/eclipse-zenoh/zenoh-ts.git
cd zenoh-ts/zenoh-bridge-remote-api
cargo build --release

# Run the bridge (in a separate terminal)
./target/release/zenoh-bridge-remote-api --listen tcp/0.0.0.0:7448 --ws-port 10000
```

### 3. Configure cameras

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

### 4. Start the camera streams

```bash
pixi run multicam
```

### 5. Start the dashboard

```bash
pixi run dashboard
```

Open http://localhost:5173 in your browser (Chrome, Edge, or Safari).

## Documentation

For detailed documentation, run:

```bash
pixi run docs
```

Or see the [docs/](docs/) directory.
