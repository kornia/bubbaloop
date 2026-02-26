# Node Marketplace

The Bubbaloop node ecosystem allows anyone to create, share, and install nodes.

## Installing Nodes

### One-Command Install (Recommended)

```bash
bubbaloop node install rtsp-camera
```

This is the simplest path. Here's what happens behind the scenes:

1. **Check local registry** — is `rtsp-camera` already registered with the daemon?
2. **Search marketplace** — fetch `nodes.yaml` from official registries
3. **Try precompiled binary** (fast path) — download architecture-matched binary from GitHub Releases, verify SHA256 checksum
4. **Fall back to source** (slow path) — `git clone` the repository and build with `pixi run build`
5. **Register with daemon** — add to `~/.bubbaloop/nodes.json`
6. **Install systemd service** — create and enable user service

### From GitHub (Manual)

```bash
# Single-node repository
bubbaloop node add user/my-sensor --build --install

# Multi-node repository (use --subdir)
bubbaloop node add kornia/bubbaloop-nodes-official --subdir rtsp-camera --build --install
```

### From Local Path

```bash
bubbaloop node add /path/to/my-sensor --build --install
```

## On-Disk Layout

After installing nodes, your filesystem looks like this:

```
~/.bubbaloop/
├── bin/                            # Platform binaries
│   ├── bubbaloop
│   ├── zenohd
│   └── zenoh-bridge-remote-api
├── nodes/                          # Installed node source/binaries
│   └── bubbaloop-nodes-official/   # Git clone or symlink
│       ├── rtsp-camera/
│       │   ├── node.yaml           # Manifest
│       │   ├── target/release/     # Built binary
│       │   └── configs/            # Example configs
│       ├── openmeteo/
│       ├── system-telemetry/
│       └── network-monitor/
├── configs/                        # Instance config overrides
│   └── camera-entrance.yaml
├── nodes.json                      # Registry: what's registered with daemon
├── sources.json                    # Marketplace source list
└── zenoh.json5                     # Zenoh router config
```

**Precompiled installs** create a minimal directory with just `node.yaml` + `target/release/<binary>`. **Source installs** clone the full repository.

## Official Nodes

Official nodes are maintained at [kornia/bubbaloop-nodes-official](https://github.com/kornia/bubbaloop-nodes-official).

| Node | Type | Description |
|------|------|-------------|
| rtsp-camera | Rust | RTSP camera capture with hardware H264 decode |
| openmeteo | Rust | Open-Meteo weather data publisher |
| system-telemetry | Rust | System metrics (CPU, memory, disk, network, load) |
| network-monitor | Python | Network connectivity monitor (HTTP, DNS, ping) |

Install any official node:

```bash
bubbaloop node install <name>
```

## Multi-Instance Nodes

Some nodes support multiple instances with different configs (e.g., multiple cameras):

```bash
# Create instances with per-camera config
bubbaloop node instance rtsp-camera entrance --config entrance.yaml --start
bubbaloop node instance rtsp-camera terrace --config terrace.yaml --start

# Each instance runs as a separate systemd service
bubbaloop node list --instances
```

## Node Registry

Each multi-node repository contains a `nodes.yaml` file listing available nodes:

```yaml
nodes:
  - name: rtsp-camera
    description: "RTSP camera capture with hardware H264 decode"
    version: "0.1.0"
    type: rust
    category: camera
    tags: [video, rtsp, gstreamer, h264]
    repo: kornia/bubbaloop-nodes-official
    subdir: rtsp-camera
```

## Marketplace Sources

Manage where `bubbaloop node install` searches for nodes:

```bash
bubbaloop marketplace list              # Show configured sources
bubbaloop marketplace add my-registry kornia/bubbaloop-nodes-official
bubbaloop marketplace remove my-registry
```

The default source is `kornia/bubbaloop-nodes-official`.

## Publishing Your Node

1. Create your node: `bubbaloop node init my-sensor --type rust`
2. Implement and test locally
3. Push to GitHub
4. Others install with: `bubbaloop node add your-username/my-sensor --build --install`

### Quality Guidelines

- Include a `node.yaml` manifest with capabilities, publishes, and requires fields
- Include a `README.md` with setup instructions
- Include a `config.yaml` example configuration
- Test that `pixi run build` succeeds from a clean checkout
- Pin dependency versions for reproducible builds (commit `Cargo.lock`)

## Node Search

Search for nodes by name, category, or tag:

```bash
bubbaloop node search camera
bubbaloop node search --category weather
bubbaloop node search --tag gstreamer
```
