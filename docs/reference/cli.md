# CLI Commands

Reference for Bubbaloop command-line tools and pixi tasks.

## Pixi Tasks

### Main Commands

| Command | Description |
|---------|-------------|
| `pixi run up` | Start all services (bridge, cameras, dashboard) |
| `pixi run build` | Build all Rust binaries |
| `pixi run docs` | Serve documentation locally |

### Service Commands

| Command | Description |
|---------|-------------|
| `pixi run bridge` | Start Zenoh WebSocket bridge |
| `pixi run cameras` | Start RTSP camera capture |
| `pixi run weather` | Start OpenMeteo weather service |
| `pixi run dashboard` | Start React dashboard |
| `pixi run bubbaloop` | Start TUI application |

### Development Commands

| Command | Description |
|---------|-------------|
| `pixi run zenohd` | Start Zenoh router (server mode) |
| `pixi run zenohd-client` | Start Zenoh router (client mode) |

## Camera Node CLI

### Usage

```bash
pixi run cameras [OPTIONS]
```

### Options

| Option | Short | Description | Default |
|--------|-------|-------------|---------|
| `--config` | `-c` | Path to configuration file | `config.yaml` |
| `--zenoh-endpoint` | `-z` | Zenoh router endpoint | `tcp/127.0.0.1:7447` |

### Examples

```bash
# Default configuration
pixi run cameras

# Custom config file
pixi run cameras -- -c /path/to/cameras.yaml

# Connect to remote Zenoh
pixi run cameras -- -z tcp/192.168.1.100:7447
```

### Environment Variables

| Variable | Description |
|----------|-------------|
| `ZENOH_ENDPOINT` | Override Zenoh endpoint |
| `RUST_LOG` | Logging level (info, debug, trace) |

```bash
RUST_LOG=debug pixi run cameras
```

## Weather Node CLI

### Usage

```bash
pixi run weather [OPTIONS]
```

### Options

| Option | Short | Description | Default |
|--------|-------|-------------|---------|
| `--config` | `-c` | Path to configuration file | Auto-discovery |
| `--zenoh-endpoint` | `-z` | Zenoh router endpoint | `tcp/127.0.0.1:7447` |

### Examples

```bash
# Auto-discover location
pixi run weather

# Explicit configuration
pixi run weather -- -c crates/openmeteo/configs/config.yaml

# Connect to remote Zenoh
pixi run weather -- -z tcp/192.168.1.100:7447
```

## TUI Application

### Usage

```bash
pixi run bubbaloop
```

### TUI Commands

| Command | Description |
|---------|-------------|
| `/help` | Show help message |
| `/connect` | Connect to Zenoh WebSocket |
| `/disconnect` | Disconnect from Zenoh |
| `/topics` | List active topics with statistics |
| `/server` | Configure remote server endpoint |
| `/quit` | Exit the application |

### Server Configuration

The `/server` command configures the Zenoh endpoint for remote access:

```
/server
> tcp/192.168.1.100:7447
```

This generates `~/.bubbaloop/zenoh.cli.json5` automatically.

### Topic Monitoring

The `/topics` command shows:

- Topic name
- Message frequency (Hz)
- Message count
- Last message time

```
/topics
/camera/front_door/compressed  25.0 Hz  1234 msgs
/weather/current                0.03 Hz   42 msgs
```

## Zenoh Router

### Server Mode

```bash
pixi run zenohd
# or: zenohd -c zenoh.json5
```

Configuration (`zenoh.json5`):

```json5
{
  mode: "router",
  listen: {
    endpoints: ["tcp/0.0.0.0:7447"],
  },
  plugins: {
    remote_api: {
      websocket_port: 10000,
    },
  },
}
```

### Client Mode

```bash
pixi run zenohd-client
# or: zenohd -c ~/.bubbaloop/zenoh.cli.json5
```

Connects to a remote Zenoh router and provides local WebSocket access.

## Process Compose

The `pixi run up` command uses process-compose to manage services.

### Process Compose UI

When running `pixi run up`, you can:

| Key | Action |
|-----|--------|
| `↑/↓` | Navigate processes |
| `Enter` | View process logs |
| `q` | Quit (stops all services) |
| `r` | Restart selected process |

### Configuration

Process definitions are in `process-compose.yaml`:

```yaml
processes:
  bridge:
    command: pixi run bridge
    readiness_probe:
      http_get:
        port: 10000

  cameras:
    command: pixi run cameras
    depends_on:
      bridge:
        condition: process_healthy

  dashboard:
    command: pixi run dashboard
```

## Dashboard Development

### Start Development Server

```bash
pixi run dashboard
# or
cd dashboard && npm run dev
```

### Build for Production

```bash
cd dashboard
npm run build
```

### Regenerate Protobuf

```bash
cd dashboard
npm run proto
```

## Logging

### Log Levels

Set via `RUST_LOG` environment variable:

| Level | Description |
|-------|-------------|
| `error` | Errors only |
| `warn` | Warnings and errors |
| `info` | General information (default) |
| `debug` | Detailed debugging |
| `trace` | Very verbose tracing |

### Examples

```bash
# Debug logging for camera node
RUST_LOG=debug pixi run cameras

# Trace logging for specific module
RUST_LOG=bubbaloop::h264_capture=trace pixi run cameras

# Multiple modules
RUST_LOG=info,zenoh=debug pixi run cameras
```

## Common Workflows

### Local Development

```bash
# Start all services
pixi run up
```

### Distributed Setup

**Server (robot):**

```bash
# Terminal 1: Zenoh router
zenohd -c zenoh.json5

# Terminal 2: Services
pixi run cameras
pixi run weather
```

**Client (laptop):**

```bash
# Terminal 1: Configure and start local router
pixi run bubbaloop  # Use /server to set robot IP
pixi run zenohd-client

# Terminal 2: Dashboard
pixi run dashboard
```

### Debugging

```bash
# Check topic activity
pixi run bubbaloop
# Then: /connect → /topics

# Verbose camera logging
RUST_LOG=debug pixi run cameras

# Check Zenoh connectivity
pixi run bubbaloop
# Then: /connect → check status
```

## Next Steps

- [Configuration](../getting-started/configuration.md) — Configuration file reference
- [Troubleshooting](troubleshooting.md) — Common issues and solutions
- [Architecture](../concepts/architecture.md) — System design
