# Bubbaloop Troubleshooting Guide

This guide helps you diagnose and fix common issues with Bubbaloop services, the daemon, and the TUI. Start with **Quick Diagnostics** if you're experiencing problems.

---

## Quick Diagnostics

### 1. Check Overall System Status

```bash
# Show comprehensive service status
bubbaloop status

# Output example:
# Services:
#   zenohd                    active
#   bubbaloop-daemon          active
# Daemon: connected
# Nodes:
#   rtsp-camera               running
#   openmeteo                 running
```

### 2. Verify Zenoh Router is Running

```bash
# Check if zenohd is accessible
pgrep -l zenohd

# Expected output: <PID> zenohd

# If not found, start it:
zenohd &
```

### 3. Verify Daemon is Responding

```bash
# Check daemon status
systemctl --user status bubbaloop-daemon

# Check daemon logs for recent errors
journalctl --user -u bubbaloop-daemon -n 20 --no-pager
```

### 4. Verify Zenoh Connectivity

```bash
# From another terminal, test pub/sub
zenoh-cli --connect tcp/127.0.0.1:7447 sub 'bubbaloop/**' &
sleep 1

# In another terminal, publish a test message
zenoh-cli --connect tcp/127.0.0.1:7447 put test/hello "world"

# You should see the message in the first terminal
# Kill the subscriber: fg, then Ctrl+C
```

---

## Architecture Overview

Understanding Bubbaloop's architecture helps diagnose issues:

```
┌─────────────────────────────────────────────────────────────┐
│                      TUI / Dashboard                         │
│                   (Client Mode: peer only)                   │
└──────────────────────────┬──────────────────────────────────┘
                           │ Zenoh queries/subscriptions
                           │ (timeout: 10s)
┌──────────────────────────┴──────────────────────────────────┐
│                        zenohd Router                         │
│            (Central message hub on :7447)                    │
│              (Pub/Sub + Query/Reply both)                    │
└──────────────────────────┬──────────────────────────────────┘
                           │
         ┌─────────────────┼─────────────────┐
         │                 │                 │
    ┌────┴─────┐      ┌───┴────┐      ┌────┴─────┐
    │  Daemon   │      │ Camera │      │  Weather │
    │ (Peer)    │      │ (Peer) │      │  (Peer)  │
    │           │      │        │      │          │
    │ Responds  │      │ Streams│      │ Publishes│
    │ to queries│      │ video  │      │ forecasts│
    │ Publishes │      │ frames │      │          │
    │ node state│      │        │      │          │
    └───────────┘      └────────┘      └──────────┘
```

### Key Communication Patterns

**1. Query/Reply (Request-Response)**
- Used for one-time requests: health checks, node commands
- TUI sends queries to daemon, waits for response
- Default timeout: 10 seconds
- Topics: `bubbaloop/daemon/api/*`

**2. Pub/Sub (Event Stream)**
- Used for continuous updates: node status, video frames
- Daemon publishes state changes, TUI subscribes
- No response needed, asynchronous
- Topics: `bubbaloop/daemon/nodes`, `bubbaloop/nodes/*`

**3. Mode Settings**
- **zenohd (Router)**: Listens for connections, routes all messages
- **TUI/Daemon (Client/Peer)**: Connects to zenohd, can send/receive
- All must connect to the same zenohd instance on the same port

---

## Common Issues & Solutions

### Issue 1: "Daemon: disconnected" in TUI or CLI timeout

**Symptoms:**
- TUI shows "Daemon: disconnected"
- CLI commands hang for 10 seconds then timeout
- Logs show: "Zenoh query failed" or "No valid reply received"

**Root Cause:**
This is usually **NOT** a single-root-cause issue. Multiple things can cause it:
1. zenohd router is not running or crashed
2. Multiple daemon instances fighting over the same port
3. Daemon crashed or hung during initialization
4. Firewall blocking port 7447
5. Wrong `BUBBALOOP_ZENOH_ENDPOINT` environment variable

**Solutions:**

**Step 1: Kill all existing services**
```bash
# Kill any running processes
pkill -f zenohd
pkill -f bubbaloop-daemon
sleep 1
```

**Step 2: Start services in order**
```bash
# Terminal 1: Start Zenoh router
zenohd &
sleep 2

# Terminal 2: Start daemon
# Make sure BUBBALOOP_ZENOH_ENDPOINT is not set (uses default)
unset BUBBALOOP_ZENOH_ENDPOINT
bubbaloop-daemon &
sleep 2

# Terminal 3: Test TUI
bubbaloop
```

**Step 3: Verify each step**
```bash
# After starting zenohd
pgrep zenohd          # Should show PID
netstat -tlnp | grep 7447  # Should show LISTENING

# After starting daemon
ps aux | grep bubbaloop-daemon  # Should show single instance
journalctl --user -u bubbaloop-daemon -n 10
```

**Step 4: Check for duplicate daemons**
```bash
# Look for multiple daemon processes
ps aux | grep bubbaloop-daemon

# If more than one, kill all and restart
pkill -9 -f bubbaloop-daemon
systemctl --user stop bubbaloop-daemon 2>/dev/null
sleep 1
systemctl --user start bubbaloop-daemon
```

**Step 5: Check environment variables**
```bash
# Verify BUBBALOOP_ZENOH_ENDPOINT is correct
echo $BUBBALOOP_ZENOH_ENDPOINT

# If set to wrong IP/port, clear it
unset BUBBALOOP_ZENOH_ENDPOINT

# If you need a specific endpoint, verify it's correct:
# For local development: tcp/127.0.0.1:7447
# For remote: tcp/ACTUAL_IP:7447
```

**Permanent fix (systemd):**
```bash
# Use systemd to auto-restart services in correct order
systemctl --user enable bubbaloop-daemon
systemctl --user restart bubbaloop-daemon

# View status
systemctl --user status bubbaloop-daemon
```

---

### Issue 2: TUI Freezes or UI Hangs

**Symptoms:**
- TUI becomes unresponsive
- Cursor stops moving, keys don't work
- TUI shows spinner indefinitely

**Root Cause:**
The TUI is waiting for a query response that won't arrive (usually due to Issue 1).

**Solutions:**

**Immediate Recovery:**
```bash
# Press Ctrl+C in the TUI terminal to exit (may take a few seconds)
# The TUI will try to gracefully handle shutdown

# Kill if necessary
pkill -f "bubbaloop.*tui"
```

**Permanent Fix:**
Follow **Issue 1** solutions above to ensure daemon is responding.

**Advanced: Enable Debug Logging**
```bash
# Run TUI with debug logs to see what it's waiting for
RUST_LOG=debug bubbaloop tui 2>&1 | tee tui.log

# Check the logs for "Zenoh query failed" or timeout messages
grep -i "timeout\|failed\|error" tui.log
```

---

### Issue 3: Zenoh Connection Refused

**Symptoms:**
```
Error: Failed to connect to zenoh: Connection refused
```

**Root Cause:**
- zenohd is not running
- zenohd is listening on wrong IP (not localhost)
- Port 7447 is blocked by firewall or already in use
- Wrong `BUBBALOOP_ZENOH_ENDPOINT` points to wrong machine

**Solutions:**

**Step 1: Start Zenoh Router**
```bash
# Kill any existing instances
pkill -f zenohd

# Start fresh
zenohd &

# Verify it's listening
sleep 1
netstat -tlnp | grep 7447
# Output should include: tcp 0 0 0.0.0.0:7447
```

**Step 2: Verify Port is Free**
```bash
# Check if port 7447 is already in use
sudo lsof -i :7447

# If something else is using it, either:
# 1. Stop that service
# 2. Change Zenoh to different port (advanced)
```

**Step 3: Verify Endpoint Configuration**
```bash
# For local development (default):
unset BUBBALOOP_ZENOH_ENDPOINT

# For connecting to remote Zenoh router:
export BUBBALOOP_ZENOH_ENDPOINT=tcp/REMOTE_IP:7447
# Then try connecting:
bubbaloop status
```

**Step 4: Check Firewall**
```bash
# On Linux with UFW:
sudo ufw status

# If firewall is active, allow port 7447:
sudo ufw allow 7447

# After allowing, restart daemon
systemctl --user restart bubbaloop-daemon
```

---

### Issue 4: Nodes Not Starting or Disappearing

**Symptoms:**
- Nodes show "not-installed" or "stopped" in TUI
- After reboot, previously running nodes don't auto-start
- Node logs show errors immediately after start

**Root Cause:**
- Service dependencies not met (zenohd/daemon not running first)
- systemd service not installed or broken
- Node configuration issues (missing config file, wrong path)
- Service dependencies specified in `node.yaml` but not started

**Solutions:**

**Step 1: Verify Service Dependencies**
```bash
# Check what services your node depends on
systemctl --user show bubbaloop-NODENAME.service | grep -E "^(Requires|After)="

# Example output:
# Requires=bubbaloop-daemon.service
# After=network.target bubbaloop-daemon.service

# Verify all dependencies are running:
systemctl --user status bubbaloop-daemon
systemctl --user status bubbaloop-zenohd
```

**Step 2: Check Service Installation**
```bash
# List all bubbaloop services
systemctl --user list-units 'bubbaloop-*.service'

# If node service is missing, reinstall it
bubbaloop node install NODENAME

# Or reinstall from TUI: go to node, press 'i' to install
```

**Step 3: Verify Node Configuration**
```bash
# Check if node.yaml exists and is valid
cat crates/bubbaloop-nodes/NODENAME/node.yaml

# Required fields:
# - name
# - version
# - type
# - command (for running), or build (for building)

# Validate with:
bubbaloop node validate crates/bubbaloop-nodes/NODENAME/
```

**Step 4: Check Node Logs**
```bash
# View recent logs for failed start
journalctl --user -u bubbaloop-NODENAME.service -n 30

# If binary is missing:
# error: No such file or directory
# Then rebuild: bubbaloop node build NODENAME

# If config is missing:
# error: config.yaml not found
# Then add config file or check node.yaml command path
```

**Step 5: Rebuild Node**
```bash
# If node has build script in node.yaml:
bubbaloop node build NODENAME

# Check if binary was created:
ls -la crates/bubbaloop-nodes/NODENAME/target/release/

# Reinstall service after rebuild:
bubbaloop node install NODENAME --force
```

---

### Issue 5: Service Dependencies Not Working

**Symptoms:**
- Node has `depends_on: [other-node]` in node.yaml
- Node starts before dependency is ready
- Or node fails to start at all

**Root Cause:**
- Service dependencies not properly installed in systemd
- `depends_on` in node.yaml not being recognized during install
- Dependencies need to be reinstalled

**Solutions:**

**Step 1: Check Dependency Configuration**
```bash
# Verify node.yaml has depends_on field
cat crates/bubbaloop-nodes/NODENAME/node.yaml

# Example:
# depends_on:
#   - rtsp-camera
#   - openmeteo
```

**Step 2: Reinstall Service with Dependencies**
```bash
# When you reinstall, dependencies should be encoded in systemd unit
bubbaloop node install NODENAME --force

# Verify systemd service has dependency:
systemctl --user show bubbaloop-NODENAME.service | grep Requires
# Output should include: Requires=bubbaloop-rtsp-camera.service bubbaloop-openmeteo.service
```

**Step 3: Start in Correct Order**
```bash
# Manual start order (guarantees dependencies are ready):
systemctl --user start bubbaloop-daemon
systemctl --user start bubbaloop-rtsp-camera
systemctl --user start bubbaloop-openmeteo
systemctl --user start bubbaloop-NODENAME

# Or use --all for automatic dependency ordering:
systemctl --user start bubbaloop-rtsp-camera bubbaloop-openmeteo bubbaloop-NODENAME
```

**Step 4: Check Dependency Chain**
```bash
# See full dependency tree
systemctl --user show bubbaloop-NODENAME.service -p Requires
systemctl --user show bubbaloop-NODENAME.service -p After

# All these services must be running for node to start
```

---

### Issue 6: Protobuf Decode Errors

**Symptoms:**
- TUI shows node list but with empty/corrupted data
- Error logs show: "Failed to decode NodeList"
- Dashboard shows garbled text in node panels

**Root Cause:**
- TUI and daemon built with different protobuf schema versions
- Protobuf schema mismatch between versions
- Corrupted message in Zenoh

**Solutions:**

**Step 1: Rebuild Everything**
```bash
# Clean build artifacts
cargo clean
pixi run build

# This recompiles both TUI and daemon with same schema versions
```

**Step 2: Verify Schema Files**
```bash
# Check protobuf schema is up-to-date
cat protos/daemon.proto

# Regenerated Rust code should match
cat crates/bubbaloop/src/schemas.rs | head -50
```

**Step 3: Check Version Compatibility**
```bash
# Verify TUI and daemon are similar versions
bubbaloop -V
bubbaloop-daemon -V

# If major versions differ, rebuild both:
cargo build --release -p bubbaloop -p bubbaloop-daemon
```

---

### Issue 7: Zenoh Topic Routing Issues (Advanced)

**Symptoms:**
- TUI can't find topics published by nodes
- Nodes can't receive commands from TUI
- Multi-machine deployment: remote nodes don't show up

**Root Cause:**
- Zenoh router not routing between clients properly
- Topic names mismatch (typo in subscription vs publication)
- Mode settings wrong (router vs peer vs client)
- Multicast/gossip misconfigured

**Solutions:**

**Step 1: Verify Topic Routing**
```bash
# In one terminal, subscribe to all topics
zenoh-cli --connect tcp/127.0.0.1:7447 sub '@/**' &

# In another, publish a test message
zenoh-cli --connect tcp/127.0.0.1:7447 put bubbaloop/test/hello "world"

# You should see the message in the subscriber
# Output: [RCV] sample:
# bubbaloop/test/hello <- "world"
```

**Step 2: Check Zenoh Router Connectivity**
```bash
# View connected peers
curl http://127.0.0.1:8000/@/router/*/peers 2>/dev/null | python3 -m json.tool

# Should list all connected clients/peers
# If empty or 404, check if zenoh admin API is enabled (default: :8000)
```

**Step 3: Verify Mode Settings**
```bash
# Zenoh router must be in "router" mode
# All others must be in "client" or "peer" mode

# Check current process modes by reviewing systemd unit or startup command:
systemctl --user show bubbaloop-daemon | grep ExecStart

# Should include mode: client or mode: peer, NOT mode: router
```

**Step 4: Test Direct Connectivity**
```bash
# Test if daemon can talk to router
bubbaloop status

# Should succeed and show services
# If fails, check BUBBALOOP_ZENOH_ENDPOINT environment variable
```

---

## Distributed Deployment Issues

For multi-machine setups, see [docs/distributed-deployment.md](distributed-deployment.md) for detailed troubleshooting.

### Common Multi-Machine Problems

**Issue: Jetson devices can't reach central router**

```bash
# On Jetson, verify central router IP is correct
cat ~/.config/zenoh/zenoh.json5 | grep connect

# Should show: connect.endpoints = ["tcp/CENTRAL_IP:7447"]
# Not: tcp/127.0.0.1:7447 (that's local only)

# Test connectivity
ping CENTRAL_IP
```

**Issue: SHM not working across machines**

Shared memory only works locally. For multi-machine:
- Each machine needs its own local zenohd router
- Local nodes connect to local zenohd (via SHM)
- Local zenohd connects to central router (via TCP)
- See distributed-deployment.md for setup

---

## Debug Commands

### Enable Debug Logging

```bash
# Run daemon with debug logs
RUST_LOG=debug systemctl --user restart bubbaloop-daemon
journalctl --user -u bubbaloop-daemon -f

# Run TUI with debug logs
RUST_LOG=debug bubbaloop 2>&1 | tee tui.log

# Run individual node with debug logs
RUST_LOG=debug systemctl --user restart bubbaloop-rtsp-camera
journalctl --user -u bubbaloop-rtsp-camera -f
```

### List All Zenoh Topics

```bash
# Subscribe to all topics and see what's being published
zenoh-cli sub '@/**'

# In another terminal, query daemon
bubbaloop status
```

### Check Zenoh Admin Space

```bash
# Query Zenoh router internals
zenoh-cli get '@/**'

# Output includes:
# - Router info and stats
# - Connected peers
# - Available endpoints
```

### Monitor Service Health

```bash
# Watch daemon status in real-time
watch -n 1 'systemctl --user status bubbaloop-daemon'

# Watch all node services
watch -n 1 'systemctl --user list-units "bubbaloop-*.service" --all'

# Watch daemon logs live
journalctl --user -u bubbaloop-daemon -f

# Filter for errors only
journalctl --user -u bubbaloop-daemon -f | grep -i error
```

### Test Individual Services

```bash
# Test Zenoh router connectivity
zenoh-cli info

# Test daemon health endpoint
zenoh-cli get bubbaloop/daemon/api/health

# Test node list query
zenoh-cli get bubbaloop/daemon/api/nodes

# List all nodes via subscription (live updates)
zenoh-cli sub bubbaloop/daemon/nodes
```

---

## Environment Variables

| Variable | Purpose | Default | Example |
|----------|---------|---------|---------|
| `BUBBALOOP_ZENOH_ENDPOINT` | Zenoh router address | `tcp/127.0.0.1:7447` | `tcp/192.168.1.100:7447` |
| `RUST_LOG` | Log level (trace/debug/info/warn/error) | `info` | `debug` |
| `RUST_BACKTRACE` | Show stack traces on panic | Not set | `1` or `full` |

### Setting Environment Variables

```bash
# Temporarily for current session
export RUST_LOG=debug
bubbaloop status

# Permanently in systemd service
systemctl --user edit bubbaloop-daemon
# Add: Environment="RUST_LOG=debug"

# For TUI running via systemd
systemctl --user edit bubbaloop-tui
# Add: Environment="RUST_LOG=debug"
```

---

## Service Management

### Starting Services

```bash
# Start Zenoh router (required first)
systemctl --user start bubbaloop-zenohd
# Or manually: zenohd &

# Start daemon (depends on zenohd)
systemctl --user start bubbaloop-daemon

# Start individual node
systemctl --user start bubbaloop-rtsp-camera

# Start all services
systemctl --user start bubbaloop-{zenohd,daemon,rtsp-camera,openmeteo}
```

### Stopping Services

```bash
# Stop individual node (daemon continues running)
systemctl --user stop bubbaloop-rtsp-camera

# Stop daemon (also stops all managed nodes)
systemctl --user stop bubbaloop-daemon

# Stop all services
systemctl --user stop bubbaloop-daemon
systemctl --user stop bubbaloop-zenohd
```

### Viewing Logs

```bash
# Real-time logs for one service
journalctl --user -u bubbaloop-daemon -f

# Last 50 lines for a service
journalctl --user -u bubbaloop-daemon -n 50 --no-pager

# Logs from last hour
journalctl --user -u bubbaloop-daemon --since "1 hour ago"

# Search for errors only
journalctl --user -u bubbaloop-daemon -p err

# All systemd logs
journalctl --user -f
```

### Restarting Services

```bash
# Restart daemon (connection will drop momentarily)
systemctl --user restart bubbaloop-daemon

# Restart and verify
systemctl --user restart bubbaloop-daemon && sleep 2
systemctl --user status bubbaloop-daemon
```

---

## Performance Issues

### Issue: High CPU Usage

**Diagnostics:**
```bash
# Check which process is using CPU
top -b -n 1 | head -20

# Or focus on bubbaloop services
ps aux | grep bubbaloop | grep -v grep
```

**Common Causes:**
1. TUI polling too fast (should be 250ms)
2. Node in infinite loop
3. Too many Zenoh subscriptions

**Solutions:**
```bash
# Stop the high-CPU process
pkill -f PROCESSNAME

# If it's a node, check logs
journalctl --user -u bubbaloop-NODENAME -f

# Reduce logging verbosity
RUST_LOG=info systemctl --user restart bubbaloop-NODENAME
```

### Issue: High Memory Usage

**Diagnostics:**
```bash
# Check memory per process
ps aux --sort=-%mem | head -10

# Or specific service
systemctl --user status bubbaloop-rtsp-camera
```

**Common Causes:**
1. Video frame buffer not being released
2. Message queue buildup (no subscribers consuming)
3. Memory leak in long-running node

**Solutions:**
```bash
# Restart the service to clear buffers
systemctl --user restart bubbaloop-rtsp-camera

# If using SHM, check available space
df -h /dev/shm

# If full, clear old SHM buffers
ipcs -m | grep bubbaloop
ipcrm -m SHMID
```

---

## FAQ

### Q: How do I know if everything is working?

A: Run these checks in order:
```bash
# 1. Check overall status
bubbaloop status

# Should show:
# Services: zenohd active, bubbaloop-daemon active
# Daemon: connected
# Nodes: (list of nodes and their status)

# 2. View TUI
bubbaloop
# Should load interactive dashboard

# 3. Check dashboard (if running)
# Open http://localhost:5173 in browser
# Should see live video and node status
```

### Q: The daemon keeps crashing. How do I debug?

A:
```bash
# 1. Check recent logs for error
journalctl --user -u bubbaloop-daemon -n 50 --no-pager

# 2. Look for patterns:
# - "Connection refused" = zenohd not running
# - "Address already in use" = something else on port 7447
# - "Failed to decode" = protobuf mismatch (rebuild needed)

# 3. Enable debug logging and restart
RUST_LOG=debug systemctl --user restart bubbaloop-daemon
journalctl --user -u bubbaloop-daemon -f

# 4. If still crashing, rebuild
cargo clean
pixi run build
systemctl --user restart bubbaloop-daemon
```

### Q: How do I completely reset everything?

A: Use caution with this procedure:
```bash
# 1. Stop all services
systemctl --user stop bubbaloop-{daemon,zenohd} 2>/dev/null
pkill -f zenohd
pkill -f bubbaloop

# 2. Wait for processes to clean up
sleep 3

# 3. Clear any zombie systemd units
systemctl --user daemon-reload

# 4. Clean build artifacts (optional, slow)
cargo clean

# 5. Rebuild
pixi run build

# 6. Start fresh
zenohd &
sleep 2
bubbaloop-daemon &
sleep 2
bubbaloop
```

### Q: How do I run TUI in a non-interactive environment?

A: The TUI requires an interactive terminal with TTY support. If running in CI/CD or via Claude Code:

```bash
# Use CLI instead of TUI
bubbaloop status

# Or run specific commands
bubbaloop node start NODENAME
bubbaloop node logs NODENAME -f

# If you must run TUI, use tmux or screen
tmux new-session -d -s bubbaloop 'bubbaloop'
tmux attach -t bubbaloop
```

### Q: How do I use a remote Zenoh router?

A: Set `BUBBALOOP_ZENOH_ENDPOINT`:
```bash
# For remote server at 192.168.1.50
export BUBBALOOP_ZENOH_ENDPOINT=tcp/192.168.1.50:7447

# Then start daemon/TUI
bubbaloop-daemon &
bubbaloop

# Make it permanent (systemd)
systemctl --user edit bubbaloop-daemon
# Add: Environment="BUBBALOOP_ZENOH_ENDPOINT=tcp/192.168.1.50:7447"

systemctl --user daemon-reload
systemctl --user restart bubbaloop-daemon
```

### Q: Can I run multiple instances of bubbaloop?

A: Yes, with different Zenoh endpoints:
```bash
# Instance 1 (default endpoint)
zenohd -c zenoh1.json5 &
BUBBALOOP_ZENOH_ENDPOINT=tcp/127.0.0.1:7447 bubbaloop-daemon &

# Instance 2 (different endpoint)
zenohd -c zenoh2.json5 &
BUBBALOOP_ZENOH_ENDPOINT=tcp/127.0.0.1:7448 bubbaloop-daemon &

# Or systemd instances
systemctl --user start bubbaloop-daemon@instance1
systemctl --user start bubbaloop-daemon@instance2
```

### Q: How do I check if my node is healthy?

A:
```bash
# 1. Check systemd status
systemctl --user status bubbaloop-NODENAME

# 2. Check logs
journalctl --user -u bubbaloop-NODENAME -f

# 3. Look for it in node list
bubbaloop status

# 4. If node publishes health heartbeats:
zenoh-cli sub 'bubbaloop/nodes/NODENAME/health'
# Should see periodic "ok" messages
```

---

## Getting Help

If you're still stuck:

1. **Collect debug information:**
   ```bash
   # Save comprehensive diagnostic output
   bubbaloop status > /tmp/bubbaloop-status.txt 2>&1
   journalctl --user -u bubbaloop-daemon -n 100 > /tmp/daemon-logs.txt 2>&1
   journalctl --user -u bubbaloop-zenohd -n 100 > /tmp/zenohd-logs.txt 2>&1
   ps aux | grep bubbaloop >> /tmp/bubbaloop-status.txt
   netstat -tlnp | grep 7447 >> /tmp/bubbaloop-status.txt
   ```

2. **Share relevant files:**
   - `/tmp/bubbaloop-status.txt` - Overall status
   - `/tmp/daemon-logs.txt` - Daemon logs
   - Any custom node logs

3. **Include environment details:**
   - OS version: `uname -a`
   - Rust version: `rustc --version`
   - Zenoh version: `zenohd --version`
   - Bubbaloop version: `bubbaloop -V`

4. **Report to GitHub Issues:**
   - https://github.com/kornia/bubbaloop/issues
   - Include diagnostic output above
   - Describe steps to reproduce

---

## See Also

- [docs/distributed-deployment.md](distributed-deployment.md) - Multi-machine setup troubleshooting
- [docs/plugin-development.md](plugin-development.md) - Plugin-specific issues
- [CLAUDE.md](../CLAUDE.md) - Architecture and component overview
