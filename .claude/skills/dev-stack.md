---
name: dev-stack
description: Manage the local bubbaloop development stack (zenohd, bridge, daemon, dashboard, nodes)
---

# Dev Stack Management

Use `scripts/dev.sh` to manage the local development stack.

## Commands

```bash
./scripts/dev.sh start      # Start zenohd + bridge (client mode) + daemon + all registered nodes
./scripts/dev.sh stop       # Stop everything gracefully
./scripts/dev.sh restart    # Full restart (stop + start)
./scripts/dev.sh status     # Show status of all components + memory usage
./scripts/dev.sh dashboard  # Start Vite dev server (generates protos if needed)
./scripts/dev.sh nodes      # Start all registered nodes
./scripts/dev.sh logs camera # Show camera node logs
```

## Typical dev workflow

1. Edit code in `crates/bubbaloop/src/` or `~/bubbaloop-nodes-official/`
2. Build: `cargo build --release` (or `pixi run check` for quick type-check)
3. Restart: `./scripts/dev.sh restart`
4. Dashboard: `./scripts/dev.sh dashboard`
5. Verify: open `http://<tailscale-ip>:5173`

## Critical rules

- **Bridge MUST use `-m client`** — peer mode (default) doesn't route through zenohd, breaking all queryables and cross-client pub/sub. The script handles this automatically.
- **Dashboard protos** — if `src/proto/messages.pb.js` is missing, run `cd dashboard && bash scripts/generate-proto.sh` (the dashboard command does this automatically).
- **Zenoh version alignment** — zenohd, bridge, daemon, and nodes should use compatible zenoh versions. Check with `~/.bubbaloop/bin/zenohd --version` vs `grep zenoh Cargo.lock`.

## After rebuilding the daemon binary

```bash
./scripts/dev.sh restart
```

This stops everything, then starts fresh with the new binary.

## After rebuilding a node (e.g., rtsp-camera)

```bash
cd ~/bubbaloop-nodes-official/rtsp-camera && cargo build --release
./scripts/dev.sh restart   # or just: bubbaloop node stop rtsp-camera && bubbaloop node start rtsp-camera
```

## Checking memory

```bash
./scripts/dev.sh status   # Shows RSS for camera node
watch -n 1 './scripts/dev.sh status'  # Monitor continuously
```

## Remote access

Dashboard is served on `0.0.0.0:5173`. Get the Tailscale IP with:
```bash
tailscale ip -4
```

## Troubleshooting

| Symptom | Cause | Fix |
|---------|-------|-----|
| Dashboard black screen | Zenoh WS connection failed | Check bridge is running: `./scripts/dev.sh status` |
| Nodes widget offline | Bridge in peer mode | Restart with `./scripts/dev.sh restart` (forces client mode) |
| Camera hex instead of video | Schema not loaded | Add a Camera panel (not Raw/JSON), or clear localStorage |
| React hook error | Stale node_modules | `cd dashboard && rm -rf node_modules && npm install` |
| Missing messages.pb.js | Proto not generated | `cd dashboard && bash scripts/generate-proto.sh` |
| Slow build | ARM64 + LTO | Install `mold` + `clang`, use `pixi run check` for iteration |
