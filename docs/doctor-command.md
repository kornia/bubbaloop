# Bubbaloop Doctor Command

The `bubbaloop doctor` command provides comprehensive system diagnostics and health checks for troubleshooting Zenoh router and daemon connectivity issues.

## Quick Start

```bash
# Run all checks
bubbaloop doctor

# Check Zenoh connectivity only
bubbaloop doctor -c zenoh

# Check daemon health only
bubbaloop doctor -c daemon

# Get JSON output (for LLMs/scripts)
bubbaloop doctor --json

# Auto-fix common issues
bubbaloop doctor --fix
```

## What It Checks

### 1. System Services
- **zenohd**: Is the Zenoh router running? Is port 7447 accessible?
- **bubbaloop-daemon**: Is the daemon service active?
- **zenoh-bridge**: Is the WebSocket bridge running? (optional for CLI)

### 2. Zenoh Connectivity
- **Connection**: Can we create a Zenoh session?
- **Queryables**: Can we declare queryables?
- **Query/Reply**: Can we send queries and receive replies?

This section specifically diagnoses the common **"Didn't receive final reply for query: Timeout"** error.

### 3. Daemon Health
- **Health endpoint**: Does `bubbaloop/daemon/api/health` respond?
- **Nodes endpoint**: Does `bubbaloop/daemon/api/nodes` respond?

### 4. Node Subscriptions
- **Topic subscription**: Can we subscribe to `bubbaloop/daemon/nodes`?

## Common Issues Diagnosed

### Error: "Didn't receive final reply for query: Timeout"

**Symptom**: Queries are sent but no replies received within timeout period.

**Doctor Output**:
```
[✗] Zenoh query/reply: query succeeded but no replies received (timeout)
    → Fix: This is the 'Didn't receive final reply for query: Timeout' error.
           Check if zenohd is running and accessible.
```

**Causes**:
1. zenohd not routing queries correctly
2. Queryable not actually listening
3. Network timeout (slow connection)
4. Router configuration issue

**Fix**:
```bash
# Check Zenoh
bubbaloop doctor -c zenoh

# Auto-fix if possible
bubbaloop doctor --fix

# Check zenohd logs
journalctl -u zenohd -f
```

### Error: "Query not found"

**Symptom**: Trying to send reply for a query that already timed out.

**Doctor Output**:
```
[✗] Daemon health: query failed: No reply received (timeout after 5s)
    → Fix: Check if daemon is connected to the same Zenoh router
```

**Causes**:
This is always secondary to timeout issues above. Fix the timeout first.

**Fix**:
```bash
# Check daemon connectivity
bubbaloop doctor -c daemon

# Restart daemon
systemctl --user restart bubbaloop-daemon
```

### Error: Connection Refused

**Symptom**: Cannot connect to Zenoh router.

**Doctor Output**:
```
[✗] zenohd: not running
    → Auto-fixable: Run: zenohd &
[✗] Zenoh connection: failed to connect
[✗] Zenoh port: port 7447 is not accessible
    → Fix: Start zenohd or check firewall settings
```

**Causes**:
1. zenohd not running
2. Firewall blocking port 7447
3. zenohd bound to different interface

**Fix**:
```bash
# Auto-fix
bubbaloop doctor --fix

# Or manually
zenohd &
```

## Command Line Options

### `--json`
Output results as structured JSON for programmatic parsing.

**Example**:
```bash
bubbaloop doctor --json
```

**Output**:
```json
{
  "summary": {
    "total": 9,
    "passed": 7,
    "failed": 2,
    "fixes_applied": 0
  },
  "checks": [
    {
      "check": "zenohd",
      "passed": true,
      "message": "running on port 7447"
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

### `-c, --check <type>`
Run specific checks only. Values: `all`, `zenoh`, `daemon`.

**Examples**:
```bash
# Zenoh connectivity only (faster)
bubbaloop doctor -c zenoh

# Daemon health only
bubbaloop doctor -c daemon

# All checks (default)
bubbaloop doctor -c all
```

### `--fix`
Automatically fix issues that can be resolved.

**Auto-fixable Issues**:
- Start zenohd router
- Start bubbaloop-daemon service
- Restart bubbaloop-daemon service (if failed)
- Start zenoh-bridge service

**Example**:
```bash
bubbaloop doctor --fix
```

**Output**:
```
[1/4] Checking system services...
    → Fixing: Start zenohd router
      ✓ zenohd started successfully

Applied 1 fix
```

## Usage Examples

### For Humans

#### Basic Troubleshooting
```bash
# Something isn't working, run all checks
bubbaloop doctor

# Follow the fix suggestions
# Example: Run: zenohd &
```

#### Quick Zenoh Check
```bash
# Faster than full check if you suspect Zenoh issues
bubbaloop doctor -c zenoh
```

#### Startup Validation
```bash
# Add to startup script
bubbaloop doctor --fix
```

### For LLMs and Scripts

#### Automated Monitoring
```python
import subprocess
import json

result = subprocess.run(
    ["bubbaloop", "doctor", "--json"],
    capture_output=True,
    text=True
)

data = json.loads(result.stdout)

if data["summary"]["failed"] > 0:
    print(f"Found {data['summary']['failed']} issues:")
    for check in data["checks"]:
        if not check["passed"]:
            print(f"- {check['check']}: {check['message']}")
```

#### Pre-deployment Validation
```bash
#!/bin/bash
# Deploy only if doctor passes

if bubbaloop doctor --json | jq -e '.summary.failed == 0' > /dev/null; then
    echo "All checks passed, proceeding"
    ./deploy.sh
else
    echo "Doctor checks failed, aborting"
    exit 1
fi
```

#### Diagnostic Context for LLM
```python
def get_diagnostic_context():
    """Get bubbaloop diagnostics for LLM context"""
    result = subprocess.run(
        ["bubbaloop", "doctor", "--json"],
        capture_output=True,
        text=True
    )

    data = json.loads(result.stdout)

    return {
        "system_status": "healthy" if data["summary"]["failed"] == 0 else "degraded",
        "issues": [
            {
                "component": check["check"],
                "problem": check["message"],
                "solution": check.get("fix", "No automatic fix available")
            }
            for check in data["checks"]
            if not check["passed"]
        ]
    }
```

## Interpreting Results

### All Checks Passed
```
[✓] zenohd: running on port 7447
[✓] Zenoh connection: connected to tcp/127.0.0.1:7447
[✓] Daemon health: ok
...

All checks passed!
```
**Action**: None needed, system is healthy.

### Services Not Running
```
[✗] zenohd: not running
    → Auto-fixable: Run: zenohd &
```
**Action**: Run `bubbaloop doctor --fix` or start service manually.

### Zenoh Routing Issues
```
[✓] Zenoh connection: connected to tcp/127.0.0.1:7447
[✗] Zenoh query/reply: no replies received (timeout)
```
**Action**: This indicates zenohd is running but not routing correctly. Check zenohd configuration and logs.

### Daemon Not Responding
```
[✗] Daemon health: query failed: No reply received (timeout after 5s)
```
**Action**: Check if daemon is connected to same Zenoh router. Verify `BUBBALOOP_ZENOH_ENDPOINT`.

## Integration Examples

### Systemd Pre-start
```ini
[Unit]
Description=Bubbaloop Daemon
After=network.target

[Service]
ExecStartPre=/usr/local/bin/bubbaloop doctor --fix
ExecStart=/usr/local/bin/bubbaloop-daemon
Restart=on-failure

[Install]
WantedBy=default.target
```

### Shell Script Startup
```bash
#!/bin/bash
# startup.sh

echo "Running diagnostics..."
if bubbaloop doctor --fix; then
    echo "System healthy, starting dashboard"
    pixi run dashboard
else
    echo "System checks failed"
    exit 1
fi
```

### CI/CD Pipeline
```yaml
# .github/workflows/test.yml
- name: Run system diagnostics
  run: |
    bubbaloop doctor --json > diagnostics.json
    cat diagnostics.json

- name: Verify health
  run: |
    if ! bubbaloop doctor --json | jq -e '.summary.failed == 0'; then
      echo "Health checks failed"
      exit 1
    fi
```

## Tips

1. **Run before reporting issues**: `bubbaloop doctor --json > diagnostics.json` and attach to issue
2. **Add to startup scripts**: `bubbaloop doctor --fix` as pre-start check
3. **Use JSON for automation**: Parse with `jq`, `python`, or any JSON parser
4. **Check specific components**: Use `-c zenoh` or `-c daemon` to save time
5. **Monitor in production**: Run periodically and alert on failures

## See Also

- [Distributed Deployment Guide](distributed-deployment.md) - Multi-machine setup
- [Debug Commands](../crates/bubbaloop/src/cli/debug.rs) - Low-level Zenoh debugging
- [Daemon Documentation](../crates/bubbaloop-daemon/README.md) - Daemon architecture
