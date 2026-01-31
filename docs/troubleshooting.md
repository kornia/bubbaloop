# Bubbaloop Troubleshooting Guide

This guide helps you diagnose and fix common issues with Bubbaloop services, the daemon, nodes, and the TUI. Start with **Quick Diagnostics** if you're experiencing problems.

**Table of Contents:**
1. [Quick Diagnostics](#quick-diagnostics)
2. [Zenoh Issues](#zenoh-issues)
3. [Daemon Issues](#daemon-issues)
4. [Node Issues](#node-issues)
5. [Dashboard Issues](#dashboard-issues)
6. [LLM Troubleshooting Guide](#llm-troubleshooting-guide)
7. [Environment Variables](#environment-variables)
8. [Performance Issues](#performance-issues)
9. [FAQ](#faq)
10. [Getting Help](#getting-help)

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

### 4. Run Doctor Command

```bash
# Comprehensive system health check
bubbaloop doctor

# Quick Zenoh-only check
bubbaloop doctor -c zenoh

# Auto-fix common issues
bubbaloop doctor --fix

# JSON output for programmatic parsing
bubbaloop doctor --json
```

The `bubbaloop doctor` command is your first-line diagnostic tool. It checks:
- System services (zenohd, daemon, bridge)
- Zenoh connectivity and routing
- Daemon health endpoints
- Node subscriptions

### 5. Verify Zenoh Connectivity

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

---

## Zenoh Issues

### "Didn't receive final reply for query: Timeout"

**Symptoms:**
- TUI shows "Daemon: disconnected"
- CLI commands hang for 10 seconds then timeout
- Logs show: "Zenoh query failed" or "No reply received"
- `bubbaloop doctor` shows red X on query/reply check

**Root Causes (in order of likelihood):**
1. Zenoh router not routing queries correctly
2. Queryable handler not responding in time
3. Network timeout (slow connection or high latency)
4. Zenoh router configuration issue
5. Multiple routers on same network interfering

**Solutions:**

**Step 1: Run doctor command**
```bash
bubbaloop doctor -c zenoh
```

Look specifically at the "Zenoh query/reply" check. If it says "query succeeded but no replies received", this confirms the routing issue.

**Step 2: Verify zenohd is running and accessible**
```bash
# Check if process is running
pgrep -l zenohd

# Check if port is listening
netstat -tlnp | grep 7447

# Or test connectivity
zenoh-cli info
```

**Step 3: Restart zenohd**
```bash
# Kill all instances
pkill -9 zenohd
sleep 1

# Start fresh
zenohd &
sleep 2

# Verify it started
pgrep zenohd
netstat -tlnp | grep 7447
```

**Step 4: Check zenohd logs for errors**
```bash
# If running as systemd service
journalctl -u zenohd -n 50 --no-pager

# Look for errors like:
# - "Failed to bind"
# - "Invalid configuration"
# - "Router mode error"
```

**Step 5: Verify daemon is connected to same router**
```bash
# Check what endpoint daemon is connecting to
echo $BUBBALOOP_ZENOH_ENDPOINT

# If set to wrong value, clear it
unset BUBBALOOP_ZENOH_ENDPOINT

# Restart daemon
systemctl --user restart bubbaloop-daemon

# Verify connection
bubbaloop status
```

**Advanced: Increase Query Timeout (for slow networks)**

If you're on a slow network or high-latency connection (e.g., remote server):
```bash
# Run client commands with explicit longer timeout
# Note: This depends on the specific command implementation
RUST_LOG=debug bubbaloop status
```

---

### "Query not found" or "No response from query handler"

**Symptoms:**
- Error message: "Query not found" or "No matching query handlers"
- Only occurs when trying to query daemon
- Zenoh connection itself works fine

**Root Cause:**
The daemon's queryable is not registered or listening. This is secondary to timeout issues above.

**Solutions:**

**Step 1: Verify daemon is actually running**
```bash
systemctl --user status bubbaloop-daemon

# If not running, start it
systemctl --user start bubbaloop-daemon
sleep 2
```

**Step 2: Check daemon health directly**
```bash
# Subscribe to all daemon messages (in one terminal)
zenoh-cli sub 'bubbaloop/daemon/**' &

# In another terminal, trigger daemon activity
bubbaloop status

# You should see daemon messages
```

**Step 3: Restart daemon with debug logging**
```bash
RUST_LOG=debug systemctl --user restart bubbaloop-daemon
sleep 2

# Watch logs
journalctl --user -u bubbaloop-daemon -f

# Try a query (in another terminal)
bubbaloop status

# Look for debug messages about queryable registration
```

**Step 4: Full reset if still failing**
```bash
# Kill daemon
systemctl --user stop bubbaloop-daemon
sleep 1

# Restart zenohd first
pkill -9 zenohd
zenohd &
sleep 2

# Now restart daemon
systemctl --user start bubbaloop-daemon
sleep 2

# Test
bubbaloop status
```

---

### Router Not Starting

**Symptoms:**
- `zenohd: command not found` error
- Port 7447 already in use
- zenohd crashes immediately

**Solutions:**

**If zenohd not installed:**
```bash
# Install via package manager
# Ubuntu/Debian:
sudo apt-get install zenoh-tools

# Or via Rust
cargo install zenoh-tools

# Verify installation
zenohd --version
```

**If port 7447 already in use:**
```bash
# Find what's using the port
sudo lsof -i :7447

# If it's old zenohd process, kill it
pkill -9 zenohd
sleep 1

# Or change zenohd port (advanced, not recommended for production)
zenohd -c /path/to/custom/config.json5
```

**If zenohd crashes immediately:**
```bash
# Try running in foreground to see error
zenohd --log-level debug

# Common errors:
# - "Failed to bind": Port in use or permission denied
# - "Invalid config": Bad configuration file
# - "Thread panicked": Check Zenoh logs or GitHub issues
```

---

### Connection Refused Errors

**Symptoms:**
```
Error: Failed to connect to zenoh: Connection refused
Error: 127.0.0.1:7447: Connection refused
```

**Root Cause:**
1. zenohd not running
2. Connecting to wrong IP/port
3. Firewall blocking the connection
4. Wrong BUBBALOOP_ZENOH_ENDPOINT value

**Solutions:**

**Quick fix:**
```bash
# Start zenohd
zenohd &
sleep 2

# Test connection
bubbaloop status
```

**If still getting connection refused:**
```bash
# Verify what endpoint you're using
echo "Endpoint: $BUBBALOOP_ZENOH_ENDPOINT"

# Test connectivity to the endpoint
nc -zv 127.0.0.1 7447

# If connection refused, zenohd isn't running or listening
zenohd &
sleep 2
nc -zv 127.0.0.1 7447  # Should now succeed
```

**For remote connections:**
```bash
# Don't use 127.0.0.1 for remote servers
export BUBBALOOP_ZENOH_ENDPOINT=tcp/ACTUAL_IP:7447

# Test with actual IP
nc -zv ACTUAL_IP 7447

# If this fails, check firewall
sudo ufw status
sudo ufw allow 7447
```

**Check firewall on Linux:**
```bash
# If using UFW
sudo ufw status
sudo ufw allow 7447

# If using iptables
sudo iptables -L -n | grep 7447
sudo iptables -A INPUT -p tcp --dport 7447 -j ACCEPT

# Reload iptables
sudo iptables-save | sudo iptables-restore
```

---

## Daemon Issues

### Daemon Not Responding

**Symptoms:**
- `bubbaloop status` times out
- TUI shows "Daemon: disconnected"
- `bubbaloop doctor` shows red X for daemon health check

**Symptoms:**
- TUI shows "Daemon: disconnected"
- CLI commands hang for 10 seconds then timeout
- Logs show: "Zenoh query failed" or "No valid reply received"

**Root Causes:**
1. Zenoh router not running or inaccessible
2. Daemon crashed during startup
3. Multiple daemon instances fighting each other
4. Daemon hung waiting for something
5. Wrong BUBBALOOP_ZENOH_ENDPOINT value

**Solutions:**

**Step 1: Run doctor command (fastest diagnosis)**
```bash
bubbaloop doctor --fix
```

This will automatically fix most issues. If it doesn't:

**Step 2: Kill all and restart in order**
```bash
# Kill everything
pkill -9 -f zenohd
pkill -9 -f "bubbaloop daemon"
sleep 2

# Start zenohd first
zenohd &
sleep 3

# Verify it's running
pgrep zenohd
netstat -tlnp | grep 7447  # Should show LISTENING

# Now start daemon
systemctl --user start bubbaloop-daemon
sleep 2

# Verify daemon started
systemctl --user status bubbaloop-daemon
```

**Step 3: Check daemon logs for errors**
```bash
# View recent logs
journalctl --user -u bubbaloop-daemon -n 50 --no-pager

# Look for errors like:
# - "Connection refused" = zenohd not running
# - "Address already in use" = duplicate daemon
# - "Failed to query" = Zenoh routing problem
# - "Thread panicked" = daemon bug

# Watch logs in real-time
journalctl --user -u bubbaloop-daemon -f
```

**Step 4: Verify only one daemon is running**
```bash
# Check for multiple instances
ps aux | grep "bubbaloop daemon" | grep -v grep

# Should show only ONE process. If more, kill all:
pkill -9 -f "bubbaloop daemon"
sleep 2
systemctl --user start bubbaloop-daemon
```

**Step 5: Verify endpoint configuration**
```bash
# Check what endpoint is configured
echo "Endpoint: $BUBBALOOP_ZENOH_ENDPOINT"

# For local development, should be unset or:
# tcp/127.0.0.1:7447

# If set to wrong value, clear it
unset BUBBALOOP_ZENOH_ENDPOINT

# For remote, verify IP is correct:
export BUBBALOOP_ZENOH_ENDPOINT=tcp/ACTUAL_REMOTE_IP:7447

# Restart daemon
systemctl --user restart bubbaloop-daemon
sleep 2

# Test
bubbaloop status
```

---

### Duplicate Daemon Detection (Address Already In Use)

**Symptoms:**
- Error: "Address already in use"
- Multiple `bubbaloop daemon` processes visible in `ps aux`
- Only one responds, others hang

**Root Cause:**
The daemon is running multiple times (usually from crashed restarts or multiple systemd instances).

**Solutions:**

**Immediate fix:**
```bash
# Kill all daemon instances
pkill -9 -f "bubbaloop daemon"
sleep 2

# Ensure systemd knows it's stopped
systemctl --user daemon-reload

# Start fresh
systemctl --user start bubbaloop-daemon
sleep 2

# Verify only one is running
ps aux | grep "bubbaloop daemon" | wc -l  # Should be 2 (process + grep)
```

**Prevent future duplicates:**
```bash
# Don't spawn daemon manually if using systemd
# Use only:
systemctl --user start bubbaloop-daemon

# Or for development, use a different terminal/session:
# Terminal 1:
systemctl --user stop bubbaloop-daemon
unset BUBBALOOP_ZENOH_ENDPOINT
pixi run daemon

# Terminal 2 (different session):
bubbaloop status
```

**Systemd protection:**
```bash
# Check current systemd configuration
systemctl --user show bubbaloop-daemon.service | grep Type=

# Should be "simple" or "notify". If not, edit:
systemctl --user edit bubbaloop-daemon.service

# Add or verify:
# Type=notify
# Restart=on-failure
# RestartSec=2
```

---

### Service Management Errors

**Symptoms:**
- `systemctl --user start bubbaloop-daemon` fails with error
- Service shows "activating" but never reaches "active"
- Service crashes immediately after starting

**Solutions:**

**For "activating" state (stuck):**
```bash
# Force stop the service
systemctl --user kill bubbaloop-daemon

# Reset failed state
systemctl --user reset-failed bubbaloop-daemon

# Retry
systemctl --user start bubbaloop-daemon
sleep 2
systemctl --user status bubbaloop-daemon
```

**For immediate crash:**
```bash
# Check why it's crashing
journalctl --user -u bubbaloop-daemon -n 30 --no-pager

# Common causes:
# 1. "Connection refused" -> zenohd not running
#    Fix: zenohd &
# 2. "Failed to bind" -> port in use
#    Fix: pkill -9 -f "bubbaloop daemon"; pkill zenohd; zenohd &
# 3. "Permission denied" -> systemd hardening issue
#    Fix: See status 218 section below

# After fixing the root cause:
systemctl --user start bubbaloop-daemon
```

**For status 218 (CAPABILITIES) error:**

This occurs when a node or service lacks required Linux capabilities. Typically happens after a fresh reinstall of a node service.

```bash
# Check the actual error
journalctl -u bubbaloop-NODENAME --no-pager | tail -20

# Look for: "status 218" or "Operation not permitted"

# The fix is to reinstall the service (which regenerates proper systemd unit)
bubbaloop node install NODENAME --force

# Or reinstall all nodes
bubbaloop node install rtsp-camera --force
bubbaloop node install openmeteo --force
# etc.

# Then try starting again
systemctl --user start bubbaloop-NODENAME
```

---

## Node Issues

### Node Won't Start

**Symptoms:**
- Node shows "stopped" or "not-installed"
- `systemctl --user start bubbaloop-NODENAME` fails
- Logs show immediate error

**Root Causes:**
1. Service not installed (need to run `bubbaloop node install`)
2. Binary not built (missing executable)
3. Configuration file missing
4. Service dependencies not met
5. Capability/permission issues (status 218)

**Solutions:**

**Step 1: Verify service is installed**
```bash
# List all services
systemctl --user list-units 'bubbaloop-*.service'

# Check if your node service exists
systemctl --user status bubbaloop-NODENAME.service

# If not listed, install it
bubbaloop node install NODENAME
```

**Step 2: Verify node is built**
```bash
# Check if binary exists (in the node's directory)
ls -la ~/.bubbaloop/nodes/NODENAME/target/release/NODENAME

# If not found, build it
bubbaloop node build NODENAME
```

**Step 3: Reinstall service with updated binary**
```bash
# Force reinstall to pick up latest binary
bubbaloop node install NODENAME --force

# Then start
systemctl --user start bubbaloop-NODENAME
systemctl --user status bubbaloop-NODENAME
```

**Step 4: Check node logs**
```bash
# View logs immediately after failure
journalctl --user -u bubbaloop-NODENAME -n 50 --no-pager

# Look for specific errors:
# - "No such file or directory" -> Binary missing, rebuild
# - "config.yaml not found" -> Config missing, create it
# - "Connection refused" -> Zenohd not running
# - "Address already in use" -> Another instance running
```

**Step 5: Check dependencies**
```bash
# Check what this node depends on
systemctl --user show bubbaloop-NODENAME.service | grep Requires

# Verify those services are running
systemctl --user status bubbaloop-daemon
systemctl --user status bubbaloop-OTHER-NODE

# If not, start them first
systemctl --user start bubbaloop-daemon
systemctl --user start bubbaloop-OTHER-NODE
sleep 2

# Then start this node
systemctl --user start bubbaloop-NODENAME
```

---

### Build Failures

**Symptoms:**
- `bubbaloop node build NODENAME` fails
- Cargo compilation errors
- Build timeout

**Solutions:**

**For compilation errors:**
```bash
# Get full error details (from the node's directory)
cd ~/.bubbaloop/nodes/NODENAME
cargo build --release 2>&1 | tail -50

# Common fixes:
# 1. Update dependencies
cargo update

# 2. Clean and rebuild
cargo clean
cargo build --release

# 3. Check for syntax errors
cargo check
```

**For build timeout (>10 minutes):**
```bash
# The daemon will kill builds after 10 minutes
# Kill the hung build
pkill -f "cargo build"

# Check why it's slow:
# - First build? Downloading dependencies can take time
# - Large project? May legitimately need >10 minutes
# - Stuck in compilation? Check top/htop for CPU/memory

# For future builds, increase timeout or pre-warm:
# Pre-download dependencies
cargo metadata --format-version 1 > /dev/null

# Then build
bubbaloop node build NODENAME
```

**For Rust version issues:**
```bash
# Check Rust version
rustc --version

# Update Rust
rustup update

# Update Cargo
cargo update -p <package-name>

# Then rebuild
cargo build --release
```

---

### Start/Stop Timeouts

**Symptoms:**
- `systemctl --user start NODENAME` hangs
- Takes >30 seconds to start/stop
- Process appears to hang

**Solutions:**

**Immediate:**
```bash
# Kill the hung process
systemctl --user kill bubbaloop-NODENAME

# Check what was happening
journalctl --user -u bubbaloop-NODENAME -f

# Look for what it was waiting on:
# - Waiting for Zenoh connection? Make sure zenohd is running
# - Waiting for dependencies? Ensure they started
```

**For startup hanging on Zenoh connection:**
```bash
# Verify Zenoh is running
bubbaloop doctor -c zenoh

# If not responding, restart it
pkill -9 zenohd
zenohd &
sleep 2

# Now try starting node again
systemctl --user start bubbaloop-NODENAME
```

**Increase startup timeout:**
```bash
# Edit service to allow more time
systemctl --user edit bubbaloop-NODENAME.service

# Add or increase:
# TimeoutStartSec=60
# TimeoutStopSec=60

# Then reload and restart
systemctl --user daemon-reload
systemctl --user restart bubbaloop-NODENAME
```

---

### Status 218 (CAPABILITIES) Error

**Symptoms:**
- Service fails with exit code 218
- Error: "Operation not permitted"
- Typically occurs after fresh node installation

**Root Cause:**
The systemd unit was created without proper Linux capability directives, or the node binary requires capabilities not granted.

**Solutions:**

**Automatic fix (recommended):**
```bash
# Reinstall the node service (regenerates systemd unit)
bubbaloop node install NODENAME --force

# Restart
systemctl --user start bubbaloop-NODENAME

# Verify
systemctl --user status bubbaloop-NODENAME
```

**Manual fix (if auto-fix doesn't work):**
```bash
# Check what's in the current systemd unit
systemctl --user show bubbaloop-NODENAME.service | grep -E "^(Protect|Restrict|Capability)"

# If the node needs special capabilities (e.g., for GPIO, network raw sockets):
systemctl --user edit bubbaloop-NODENAME.service

# Add under [Service]:
# NoNewPrivileges=false
# AmbientCapabilities=CAP_NET_RAW CAP_SYS_ADMIN
# (adjust capabilities based on node needs)

# Reload and test
systemctl --user daemon-reload
systemctl --user start bubbaloop-NODENAME
```

**For robotics/hardware nodes:**

If your node accesses hardware (GPIO, I2C, SPI, video devices):
```bash
# Check what permissions the binary needs
ldd ~/.bubbaloop/nodes/NODENAME/target/release/NODENAME

# Edit systemd unit
systemctl --user edit bubbaloop-NODENAME.service

# Add under [Service]:
# PrivateDevices=false
# DevicePolicy=auto

# Reload
systemctl --user daemon-reload
systemctl --user start bubbaloop-NODENAME
```

---

## Dashboard Issues

### WebSocket Connection Failed

**Symptoms:**
- Dashboard page shows "Cannot connect to server"
- Browser console shows WebSocket error
- Port 10001 connection refused

**Root Causes:**
1. zenoh-bridge-remote-api not running
2. Bridge is running but Zenoh not accessible
3. Wrong browser URL or port

**Solutions:**

**Quick fix:**
```bash
# Start the bridge
zenoh-bridge-remote-api --ws-port 10001 -e tcp/127.0.0.1:7447 &
sleep 2

# Verify it's listening
netstat -tlnp | grep 10001

# Refresh browser
# Open http://localhost:10001 or :5173 (depending on setup)
```

**If bridge fails to start:**
```bash
# Check if port is in use
sudo lsof -i :10001

# If in use, kill the old process or change port:
# PORT=10002 zenoh-bridge-remote-api --ws-port 10002 -e tcp/127.0.0.1:7447 &

# Verify bridge can connect to Zenoh
# First make sure zenohd is running:
pgrep zenohd || zenohd &
sleep 2

# Then try bridge again:
zenoh-bridge-remote-api --ws-port 10001 -e tcp/127.0.0.1:7447 &
```

**Check bridge logs:**
```bash
# Run bridge in foreground to see errors
zenoh-bridge-remote-api --ws-port 10001 -e tcp/127.0.0.1:7447 2>&1

# Look for:
# - "Connection refused" -> zenohd not running
# - "Address already in use" -> port 10001 in use
# - "Failed to establish" -> endpoint wrong
```

**For remote dashboard:**
```bash
# If accessing dashboard from another machine:
# Don't use localhost in browser, use actual IP:

# On server:
export SERVER_IP=$(hostname -I | awk '{print $1}')
zenoh-bridge-remote-api --ws-port 10001 -e tcp/127.0.0.1:7447 &

# In browser on client:
# http://SERVER_IP:10001
# Or if using dev server:
# http://SERVER_IP:5173
```

---

### Bridge Not Running

**Symptoms:**
- Dashboard loads but shows "no nodes" or "no data"
- Commands in dashboard don't work
- zenoh-bridge-remote-api not started

**Solutions:**

**Start the bridge:**
```bash
# In a dedicated terminal:
zenoh-bridge-remote-api --ws-port 10001 -e tcp/127.0.0.1:7447 &

# Or via systemd (if available):
systemctl --user start zenoh-bridge

# Verify it started
pgrep zenoh-bridge
netstat -tlnp | grep 10001
```

**Keep bridge running:**
```bash
# Option 1: Use systemd (recommended)
systemctl --user enable zenoh-bridge
systemctl --user start zenoh-bridge

# Option 2: Use tmux/screen
tmux new-session -d -s bridge 'zenoh-bridge-remote-api --ws-port 10001 -e tcp/127.0.0.1:7447'
tmux attach -t bridge

# Option 3: Use systemd-run
systemd-run --user --scope --service-type=simple zenoh-bridge-remote-api --ws-port 10001 -e tcp/127.0.0.1:7447
```

**Troubleshoot bridge connection:**
```bash
# Verify bridge has Zenoh access
zenoh-cli info  # Should succeed

# Verify bridge is routing topics
zenoh-cli sub 'bubbaloop/**' &  # In terminal 1
sleep 1

# In terminal 2, trigger activity:
bubbaloop status

# Terminal 1 should show activity
```

---

## LLM Troubleshooting Guide

This section is specifically for AI assistants (LLMs) using Bubbaloop commands for diagnostics.

### JSON Output for Programmatic Parsing

All diagnostic commands support JSON output, which is ideal for parsing and analysis:

```bash
# Get system status as JSON
bubbaloop status --json

# Get full diagnostics as JSON
bubbaloop doctor --json

# Parse with jq or any JSON parser
bubbaloop doctor --json | jq '.summary.failed'

# Python example
import json
import subprocess

result = subprocess.run(
    ["bubbaloop", "doctor", "--json"],
    capture_output=True,
    text=True
)
diagnostics = json.loads(result.stdout)
print(f"Failed checks: {diagnostics['summary']['failed']}")
```

### Common Diagnostic Patterns

**Check System Health:**
```bash
# Single command that shows everything
bubbaloop doctor --json | jq '{
  healthy: .summary.failed == 0,
  passed: .summary.passed,
  failed: .summary.failed,
  issues: [.checks[] | select(.passed == false) | {check: .check, message: .message}]
}'
```

**Get Status with Details:**
```bash
# Full status in JSON
bubbaloop status --json

# Parse node list
bubbaloop status --json | jq '.nodes[] | {name, status, type: .node_type, version}'
```

**Monitor Daemon Health:**
```bash
# Repeatedly check daemon (useful for monitoring)
while true; do
  bubbaloop doctor --json | jq -r '.checks[] | select(.check == "Daemon health") | .passed'
  sleep 5
done
```

### How to Use `bubbaloop doctor --json`

The JSON output has this structure:
```json
{
  "summary": {
    "total": 9,        # Total checks run
    "passed": 7,       # Checks that passed
    "failed": 2,       # Checks that failed
    "fixes_applied": 0 # Auto-fixes applied with --fix
  },
  "checks": [
    {
      "check": "zenohd",
      "passed": true,
      "message": "running on port 7447",
      "fix": null,      # Only present if failed
      "details": null   # Optional additional info
    },
    {
      "check": "Zenoh query/reply",
      "passed": false,
      "message": "query succeeded but no replies received (timeout)",
      "fix": "Check if zenohd is running and accessible",
      "details": {
        "error_type": "timeout",
        "common_cause": "zenohd not routing queries correctly",
        "timeout_ms": 2000
      }
    }
  ]
}
```

**Parse programmatically:**
```bash
# Check if system is healthy
bubbaloop doctor --json | jq 'if .summary.failed == 0 then "healthy" else "unhealthy" end'

# Get all failed checks with fixes
bubbaloop doctor --json | jq '.checks[] | select(.passed == false) | "\(.check): \(.fix)"'

# Monitor in script
if bubbaloop doctor --json | jq -e '.summary.failed > 0' > /dev/null; then
  echo "System has issues"
  bubbaloop doctor --json | jq '.checks[] | select(.passed == false)'
else
  echo "All checks passed"
fi
```

### Recommended LLM Query Workflow

When asked to diagnose issues:

**1. Gather diagnostics:**
```bash
# Always start here
bubbaloop doctor --json > /tmp/diagnostics.json
bubbaloop status --json > /tmp/status.json
```

**2. Parse results:**
```bash
# Check overall health
jq '.summary' /tmp/diagnostics.json

# Get specific issues
jq '.checks[] | select(.passed == false)' /tmp/diagnostics.json
```

**3. Provide structured response:**
```python
import json

with open('/tmp/diagnostics.json') as f:
    diag = json.load(f)

issues = [c for c in diag['checks'] if not c['passed']]

response = {
    "system_health": "healthy" if diag['summary']['failed'] == 0 else "degraded",
    "issues": [
        {"component": i['check'], "problem": i['message'], "fix": i['fix']}
        for i in issues
    ]
}

print(json.dumps(response, indent=2))
```

### Environment Variables for Diagnostics

Always check and report these:

```bash
# Diagnostic script for LLMs
echo "=== Bubbaloop Diagnostic Report ==="
echo "Timestamp: $(date -u +%Y-%m-%dT%H:%M:%SZ)"
echo ""
echo "Environment:"
echo "  BUBBALOOP_ZENOH_ENDPOINT=${BUBBALOOP_ZENOH_ENDPOINT:-tcp/127.0.0.1:7447}"
echo "  RUST_LOG=${RUST_LOG:-info}"
echo ""
echo "Doctor Output:"
bubbaloop doctor --json | jq '.summary'
echo ""
echo "Failed Checks:"
bubbaloop doctor --json | jq '[.checks[] | select(.passed == false)]'
```


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
| `BUBBALOOP_ZENOH_ENDPOINT` | Zenoh router endpoint | `tcp/127.0.0.1:7447` | `tcp/192.168.1.100:7447` |
| `RUST_LOG` | Log level (trace/debug/info/warn/error) | `info` | `RUST_LOG=debug` |
| `RUST_BACKTRACE` | Show stack traces on panic | Not set | `1` (short) or `full` (detailed) |
| `XDG_CONFIG_HOME` | Config directory | `~/.config` | `/home/user/.config` |
| `XDG_DATA_HOME` | Data directory | `~/.local/share` | `/home/user/.local/share` |

### Setting Environment Variables

**Temporarily (current session only):**
```bash
# For CLI commands
export RUST_LOG=debug
bubbaloop status

# For one command only
RUST_LOG=debug bubbaloop doctor

# For daemon
RUST_LOG=debug systemctl --user restart bubbaloop-daemon
journalctl --user -u bubbaloop-daemon -f
```

**Permanently (systemd):**
```bash
# Edit daemon service
systemctl --user edit bubbaloop-daemon

# Add under [Service]:
# Environment="RUST_LOG=debug"
# Environment="BUBBALOOP_ZENOH_ENDPOINT=tcp/192.168.1.100:7447"

# Reload and restart
systemctl --user daemon-reload
systemctl --user restart bubbaloop-daemon
```

**For shell startup (bash/zsh):**
```bash
# Add to ~/.bashrc or ~/.zshrc:
export RUST_LOG=info
export BUBBALOOP_ZENOH_ENDPOINT=tcp/127.0.0.1:7447

# Then reload
source ~/.bashrc
```

### BUBBALOOP_ZENOH_ENDPOINT Details

**Format:** `protocol/host:port`

**Common values:**
```bash
# Local development (default)
tcp/127.0.0.1:7447

# Local, different port
tcp/127.0.0.1:7448

# Remote server
tcp/192.168.1.100:7447

# With DNS
tcp/zenoh.example.com:7447

# Unset to use default
unset BUBBALOOP_ZENOH_ENDPOINT
```

**Verify current value:**
```bash
echo $BUBBALOOP_ZENOH_ENDPOINT
# If empty, using default: tcp/127.0.0.1:7447
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
bubbaloop daemon &
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
bubbaloop daemon &
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
BUBBALOOP_ZENOH_ENDPOINT=tcp/127.0.0.1:7447 bubbaloop daemon &

# Instance 2 (different endpoint)
zenohd -c zenoh2.json5 &
BUBBALOOP_ZENOH_ENDPOINT=tcp/127.0.0.1:7448 bubbaloop daemon &

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
