# Distributed Bubbaloop Deployment Guide

This guide explains how to deploy Bubbaloop across multiple machines (e.g., Jetson devices) with a central orchestration point.

## Architecture Overview

```
                                  ┌─────────────────────┐
                                  │   Central Server    │
                                  │  (Dashboard Host)   │
                                  │                     │
                                  │  ┌───────────────┐  │
┌─────────────────────────────────┤──│   zenohd      │──├─────────────────────────────────┐
│                                 │  │ (central)     │  │                                 │
│    Tailscale / LAN              │  └───────────────┘  │    Tailscale / LAN              │
│                                 │         │          │                                 │
│                                 │         │ WS:10000 │                                 │
│                                 │  ┌──────┴───────┐  │                                 │
│                                 │  │  Dashboard   │  │                                 │
│                                 │  │  (Browser)   │  │                                 │
│                                 │  └──────────────┘  │                                 │
│                                 └─────────────────────┘                                 │
│                                          │                                              │
│              TCP:7447                    │                   TCP:7447                   │
│                                          │                                              │
▼                                          ▼                                              ▼
┌─────────────────────┐          ┌─────────────────────┐          ┌─────────────────────┐
│     Jetson #1       │          │     Jetson #2       │          │     Jetson #N       │
│                     │          │                     │          │                     │
│  ┌───────────────┐  │          │  ┌───────────────┐  │          │  ┌───────────────┐  │
│  │   zenohd      │◄─┼──────────┼─►│   zenohd      │◄─┼──────────┼─►│   zenohd      │  │
│  │ (local)       │  │  gossip  │  │ (local)       │  │  gossip  │  │ (local)       │  │
│  └───────┬───────┘  │          │  └───────┬───────┘  │          │  └───────┬───────┘  │
│          │ SHM      │          │          │ SHM      │          │          │ SHM      │
│  ┌───────┴───────┐  │          │  ┌───────┴───────┐  │          │  ┌───────┴───────┐  │
│  │ rtsp-camera   │  │          │  │ rtsp-camera   │  │          │  │ rtsp-camera   │  │
│  │ daemon        │  │          │  │ daemon        │  │          │  │ daemon        │  │
│  │ openmeteo     │  │          │  │ openmeteo     │  │          │  │ other nodes   │  │
│  └───────────────┘  │          │  └───────────────┘  │          │  └───────────────┘  │
└─────────────────────┘          └─────────────────────┘          └─────────────────────┘
```

## Key Concepts

### Why Local Routers?

Each Jetson runs its own local zenohd router for several reasons:

1. **Shared Memory (SHM)** - Camera frames (6MB per 1080p frame) transfer via SHM between local processes only. Without a local router, all data would go over the network.

2. **Resilience** - Local nodes continue communicating if VPN/network is temporarily down.

3. **Graceful Degradation** - If central router is unreachable, local services still function.

4. **Reduced Latency** - Local communication is 10-100x faster than network.

### Scouting Settings

| Setting | Central Router | Jetson Router | Nodes |
|---------|----------------|---------------|-------|
| Mode | `router` | `router` | `peer` or `client` |
| Multicast | `false` | `false` | `false` |
| Gossip | `true` | `true` | `false` |
| Listen | `0.0.0.0:7447` | `127.0.0.1:7447` | N/A |
| Connect | N/A | `<central>:7447` | `127.0.0.1:7447` |

## Deployment Steps

### 1. Central Server Setup

The central server hosts the zenohd router and dashboard.

```bash
# Copy the central router config
cp configs/zenoh/central-router.json5 /etc/zenoh/zenoh.json5

# Start the router
zenohd -c /etc/zenoh/zenoh.json5

# Start the dashboard
cd dashboard && npm run dev
```

For production, create a systemd service:

```ini
# /etc/systemd/system/zenohd.service
[Unit]
Description=Zenoh Central Router
After=network.target

[Service]
ExecStart=/usr/bin/zenohd -c /etc/zenoh/zenoh.json5
Restart=always

[Install]
WantedBy=multi-user.target
```

### 2. Jetson Device Setup

On each Jetson device:

```bash
# 1. Copy and edit the Jetson router config
cp configs/zenoh/jetson-router.json5 ~/.config/zenoh/zenoh.json5

# 2. Edit to add your central router IP
nano ~/.config/zenoh/zenoh.json5
# Change: connect.endpoints = ["tcp/CENTRAL_ROUTER_IP:7447"]

# 3. Start the local router
zenohd -c ~/.config/zenoh/zenoh.json5

# 4. Start the daemon
BUBBALOOP_ZENOH_ENDPOINT=tcp/127.0.0.1:7447 bubbaloop daemon

# 5. Start nodes (they connect to local router automatically)
bubbaloop node start rtsp-camera
```

### 3. Using Tailscale

If devices are on different networks, use Tailscale for secure connectivity:

```bash
# On central server
tailscale up
tailscale ip -4  # Note the IP, e.g., 100.64.1.1

# On each Jetson
tailscale up
# Edit config to use Tailscale IP
nano ~/.config/zenoh/zenoh.json5
# connect.endpoints = ["tcp/100.64.1.1:7447"]
```

Benefits of Tailscale:
- WireGuard encryption (no TLS needed in Zenoh)
- NAT traversal
- Device authentication via SSO
- No port forwarding required

## Configuration Files

### Central Router (`configs/zenoh/central-router.json5`)

```json5
{
  mode: "router",
  listen: {
    endpoints: ["tcp/0.0.0.0:7447"],
  },
  scouting: {
    multicast: { enabled: false },
    gossip: { enabled: true, autoconnect: "router" },
  },
  plugins: {
    remote_api: { websocket_port: 10000 },
  },
}
```

### Jetson Router (`configs/zenoh/jetson-router.json5`)

```json5
{
  mode: "router",
  listen: {
    endpoints: ["tcp/127.0.0.1:7447"],
  },
  connect: {
    endpoints: ["tcp/CENTRAL_ROUTER_IP:7447"],
    retry: {
      period_init_ms: 1000,
      period_max_ms: 60000,
      period_increase_factor: 2.0,
    },
  },
  scouting: {
    multicast: { enabled: false },
    gossip: { enabled: true, autoconnect: "router" },
  },
  transport: {
    shared_memory: { enabled: true },
  },
}
```

## Environment Variables

| Variable | Description | Default |
|----------|-------------|---------|
| `BUBBALOOP_ZENOH_ENDPOINT` | Zenoh router endpoint | `tcp/127.0.0.1:7447` |
| `RUST_LOG` | Log level | `info` |

## Verifying Connectivity

### Check Router Status

```bash
# On central router - list connected peers
curl http://localhost:8000/@/router/*/peers

# Check Zenoh admin space
zenoh-cli --connect tcp/127.0.0.1:7447 get '@/**'
```

### Test Cross-Machine Communication

```bash
# On Jetson #1 - publish
zenoh-cli --connect tcp/127.0.0.1:7447 put test/hello "from jetson1"

# On Jetson #2 - subscribe
zenoh-cli --connect tcp/127.0.0.1:7447 sub 'test/**'
```

## Troubleshooting

### Connection Refused

```
Error: Failed to connect to zenoh: Connection refused
```

1. Check router is running: `pgrep zenohd`
2. Check port is open: `netstat -tlnp | grep 7447`
3. Check firewall: `sudo ufw status`

### Routers Not Connecting

If local routers can't reach central:

1. Verify network connectivity: `ping CENTRAL_IP`
2. Check Tailscale status: `tailscale status`
3. Check connect endpoint in config

### SHM Not Working

Shared memory only works for processes on the same machine connecting to the same local router.

1. Verify nodes connect to local `127.0.0.1:7447`
2. Check SHM is enabled in router config
3. Verify `/dev/shm` has space: `df -h /dev/shm`

## Security Considerations

1. **Use Tailscale** - Provides WireGuard encryption and authentication
2. **Disable Multicast** - Prevents accidental discovery across networks
3. **Listen on Localhost** - Local routers only listen locally
4. **Central Router Firewall** - Only allow Tailscale IPs to port 7447

## Resource Usage

| Component | RAM | CPU | Notes |
|-----------|-----|-----|-------|
| zenohd (router) | 15-25 MB | <5% | Minimal overhead |
| zenohd + SHM | +256 MB | - | For camera frames |
| bubbaloop daemon | 20-30 MB | <5% | Node management |

## Next Steps

1. Set up central router on a stable server
2. Configure each Jetson with local router pointing to central
3. Verify connectivity with `zenoh-cli`
4. Deploy nodes and confirm dashboard sees all devices
