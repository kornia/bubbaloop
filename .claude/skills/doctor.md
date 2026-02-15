# Doctor — System Health Check

Comprehensive health check and troubleshooter for the bubbaloop system. Verifies the full pipeline: services → ports → bridge → proxy → zenoh routing → schemas → decode. Use this BEFORE debugging individual issues.

## Usage
```
/doctor              # Full system health check
/doctor --fix        # Auto-fix common issues (restart services, regenerate configs)
/doctor --services   # Check services only
```

## Checks

### Step 1: Service Health
Check all required services are running:
```bash
systemctl --user list-units 'bubbaloop-*' --no-pager --plain
```

Required services:
- `bubbaloop-zenohd` — Zenoh router (port 7447)
- `bubbaloop-bridge` — WebSocket bridge (port 10001)
- `bubbaloop-daemon` — Daemon (Zenoh API)
- Node services — at least one node should be running

If `--fix`: restart any failed services with `systemctl --user restart <service>`.

### Step 2: Port Connectivity
Verify all ports are listening:
```bash
ss -tlnp | grep -E '(7447|10001|5173)'
```

Required:
- `7447` — zenohd (localhost only)
- `10001` — zenoh-bridge-remote-api (WebSocket)
- `5173` — Vite dashboard (or production server)

### Step 3: Duplicate Process Detection
Check for duplicate bridge or router processes (common after manual restarts):
```bash
echo "Bridge processes: $(pgrep -c -f 'zenoh-bridge-remote' 2>/dev/null || echo 0) (expected: 1)"
echo "Zenohd processes: $(pgrep -c zenohd 2>/dev/null || echo 0) (expected: 1)"
```

If duplicates exist:
- If `--fix`: Kill stale processes and restart services
- Duplicates can cause port conflicts and connection instability

### Step 4: Service File Consistency
Check that all node service files have required env vars:
```bash
for f in ~/.config/systemd/user/bubbaloop-*.service; do
  name=$(basename "$f" .service)
  has_machine_id=$(grep -c 'BUBBALOOP_MACHINE_ID' "$f" 2>/dev/null || echo 0)
  has_scope=$(grep -c 'BUBBALOOP_SCOPE' "$f" 2>/dev/null || echo 0)
  echo "$name: MACHINE_ID=$has_machine_id SCOPE=$has_scope"
done
```

Node services MUST have `BUBBALOOP_MACHINE_ID` and `BUBBALOOP_SCOPE`. If missing:
- If `--fix`: add them (get machine_id from `hostname | tr '-' '_'`, scope defaults to `local`)
- Otherwise: warn and show the fix command

### Step 5: Zenoh Router TCP Connections
Verify all components are connected to the router:
```bash
ss -tnp | grep 7447 | awk '{print $5, $6}' | sort
```

Expected connections (each node has 1-2 TCP connections to :7447):
- `zenoh-bridge-re` — bridge to router
- `system_telemetr` — system-telemetry node
- `rtsp_camera_nod` — camera node
- `openmeteo_node` — weather node
- `python` — network-monitor (Python node)
- `bubbaloop` — daemon

If a node is "active running" but has no TCP connection to :7447, the node may be stuck.

### Step 6: WebSocket Connectivity Chain
Test the FULL WebSocket path (bridge → Vite proxy):
```bash
# Direct to bridge
python3 -c "
import websocket, ssl, time
ws = websocket.WebSocket()
try:
    ws.connect('ws://127.0.0.1:10001', timeout=3)
    print('Direct bridge: OK')
    ws.close()
except Exception as e:
    print(f'Direct bridge: FAIL - {e}')

# Through Vite HTTPS proxy
ws2 = websocket.WebSocket(sslopt={'cert_reqs': ssl.CERT_NONE})
try:
    ws2.connect('wss://localhost:5173/zenoh', timeout=5)
    time.sleep(1)
    if ws2.connected:
        print('Vite WSS proxy: OK (stable 1s)')
    else:
        print('Vite WSS proxy: FAIL (dropped)')
    ws2.close()
except Exception as e:
    print(f'Vite WSS proxy: FAIL - {e}')
"
```

Common failures:
- **Direct fails**: Bridge not running or port conflict from duplicate process
- **Proxy fails, direct OK**: Vite dev server not running or proxy config broken
- **Both OK but dashboard disconnects**: Check zenoh-ts vs bridge version match

### Step 7: Version Compatibility
Check zenoh-ts and bridge versions match:
```bash
echo "Bridge: $(/home/nvidia/.bubbaloop/bin/zenoh-bridge-remote-api --version 2>&1 | grep -oP 'v[\d.]+')"
echo "zenoh-ts: $(cat /home/nvidia/bubbaloop/dashboard/node_modules/@eclipse-zenoh/zenoh-ts/package.json | grep '"version"' | grep -oP '[\d.]+')"
```

Versions MUST match (both v1.7.x). Mismatched versions cause protocol errors and immediate disconnects.

### Step 8: Topic Publishing
Verify nodes are actually publishing data:
```bash
for svc in $(systemctl --user list-units 'bubbaloop-*' --no-pager --plain | grep 'active running' | awk '{print $1}' | grep -v 'zenohd\|bridge\|daemon\|dashboard'); do
  echo "=== $svc ==="
  systemctl --user status "$svc" --no-pager | tail -5
done
```

Look for:
- system-telemetry: "Publishing metrics" at regular intervals
- openmeteo: "Current weather" messages with temperature
- rtsp-camera: "Compressed task started" and frame activity
- network-monitor: data publishing

If logs show ONLY startup messages but no ongoing publishing, the node may be stalled.

### Step 9: ros-z Topic Compatibility
Check for hyphens in topic names AND machine IDs (ros-z rejects hyphens in path components):
```bash
# Check config files for hyphenated topics
for cfg in ~/.bubbaloop/nodes/*/config.yaml ~/.bubbaloop/nodes/*/*/config.yaml; do
  [ -f "$cfg" ] || continue
  if grep -qP 'publish_topic:.*-' "$cfg" 2>/dev/null; then
    echo "WARNING: $cfg has hyphenated topic (ros-z will reject)"
  fi
done

# Check service files for unhyphenated machine IDs
for f in ~/.config/systemd/user/bubbaloop-*.service; do
  mid=$(grep 'BUBBALOOP_MACHINE_ID=' "$f" 2>/dev/null | grep -oP '=\K.*')
  if [ -n "$mid" ] && echo "$mid" | grep -q '-'; then
    echo "WARNING: $f has hyphenated MACHINE_ID=$mid (ros-z will reject)"
  fi
done
```

If `--fix`: replace hyphens with underscores in topic names and machine IDs.

### Step 10: Schema Key Consistency
Check that each node's schema queryable key prefix matches its data topic prefix:
```bash
for svc in $(systemctl --user list-units 'bubbaloop-*' --no-pager --plain | grep 'active running' | awk '{print $1}' | grep -v 'zenohd\|bridge\|daemon\|dashboard'); do
  logs=$(systemctl --user status "$svc" --no-pager 2>/dev/null)
  # Extract data topic and schema key from logs
  data_topic=$(echo "$logs" | grep -oP "(?:Publishing to|Data topic|pub_build; topic=)\K[^\s]+|(?:topic=)[^\s]+" | head -1)
  schema_key=$(echo "$logs" | grep -oP "(?:Schema queryable|schema_key): \K[^\s]+" | head -1)
  if [ -n "$data_topic" ] && [ -n "$schema_key" ]; then
    # Extract the node-name segment from each
    data_seg=$(echo "$data_topic" | awk -F/ '{print $4}')
    schema_seg=$(echo "$schema_key" | awk -F/ '{print $4}')
    if [ "$data_seg" != "$schema_seg" ]; then
      echo "MISMATCH in $svc: data='$data_seg' vs schema='$schema_seg'"
      echo "  Data topic:  $data_topic"
      echo "  Schema key:  $schema_key"
    fi
  fi
done
```

Data topics use underscores (ros-z requirement) but schema queryables might use hyphens.
This mismatch prevents `discoverSchemaForTopic()` from mapping topics to schemas.
If `--fix`: update node code to use consistent naming (prefer underscores for both).

### Step 11: Schema Queryable Verification
Check that nodes serve schema queryables. For Rust nodes:
```bash
for node_dir in ~/.bubbaloop/nodes/*/; do
  [ -d "$node_dir/src" ] || continue
  name=$(basename "$node_dir")
  has_descriptor=$(grep -rl 'DESCRIPTOR' "$node_dir/src/" 2>/dev/null | head -1)
  has_queryable=$(grep -rl 'schema.*queryable\|declare_queryable.*schema' "$node_dir/src/" 2>/dev/null | head -1)
  echo "$name: DESCRIPTOR=${has_descriptor:+YES} QUERYABLE=${has_queryable:+YES}"
done
# For Python nodes:
for node_dir in ~/.bubbaloop/nodes/*/; do
  [ -f "$node_dir/main.py" ] || continue
  name=$(basename "$node_dir")
  has_descriptor=$(grep -l 'descriptor\|DESCRIPTOR\|descriptor.bin' "$node_dir/"*.py 2>/dev/null | head -1)
  has_queryable=$(grep -l 'declare_queryable\|schema' "$node_dir/"*.py 2>/dev/null | head -1)
  echo "$name (Python): DESCRIPTOR=${has_descriptor:+YES} QUERYABLE=${has_queryable:+YES}"
done
```

Nodes without schema queryable won't have their protobuf messages decoded in the dashboard.

### Step 12: Camera RTSP Source
If camera node is running, verify RTSP source connectivity:
```bash
for cfg in ~/.bubbaloop/nodes/*/configs/*.yaml ~/.bubbaloop/nodes/*/*/configs/*.yaml; do
  [ -f "$cfg" ] || continue
  url=$(grep -oP 'url:\s*"\K[^"]+' "$cfg" 2>/dev/null)
  if [ -n "$url" ]; then
    host=$(echo "$url" | grep -oP '(?<=@)[^:]+|(?<=//)[^:@]+')
    if [ -n "$host" ]; then
      echo -n "Camera $(basename $cfg .yaml) → $host: "
      timeout 3 ping -c 1 -W 2 "$host" 2>/dev/null | grep -oP '[\d.]+ms' || echo "UNREACHABLE"
    fi
  fi
done
```

High latency (>500ms) or unreachable source means no camera frames will be published.

### Step 13: Journal Storage
Check if journald is storing logs (needed for debugging):
```bash
journalctl --user --disk-usage 2>&1
```

If "No journal files were found":
- Logs exist only in volatile memory (systemctl status can still show recent lines)
- For persistent logs: `sudo mkdir -p /var/log/journal && sudo systemd-tmpfiles --create --prefix /var/log/journal`
- Then restart journald: `sudo systemctl restart systemd-journald`

### Step 14: Dashboard HTTPS
Verify the dashboard is accessible:
```bash
curl -sk -o /dev/null -w "HTTP %{http_code}" https://localhost:5173/ && echo " OK" || echo " FAIL"
```

If empty or error: restart Vite dev server:
```bash
cd /home/nvidia/bubbaloop/dashboard && npx vite --host &
```

### Step 15: Summary Report
Output a table:

| Component | Status | Details |
|-----------|--------|---------|
| zenohd | OK/FAIL | port 7447 |
| bridge | OK/FAIL | port 10001, no duplicates |
| daemon | OK/FAIL | |
| dashboard | OK/FAIL | port 5173, HTTPS |
| WS direct | OK/FAIL | ws://127.0.0.1:10001 |
| WS proxy | OK/FAIL | wss://localhost:5173/zenoh |
| version match | OK/FAIL | bridge vs zenoh-ts |
| router connections | N clients | list node names |
| nodes | N running | list names |
| service files | OK/WARN | missing env vars |
| topic names | OK/WARN | hyphen issues |
| schema keys | OK/WARN | data vs schema mismatch |
| schema queryables | N/M nodes | which missing |
| camera source | OK/WARN | latency or unreachable |
| journal storage | OK/WARN | persistent or volatile |

### Common Fixes (--fix mode)
1. **Service not running**: `systemctl --user restart <service>`
2. **Duplicate processes**: Kill stale PIDs, restart services
3. **Missing MACHINE_ID**: Add `Environment=BUBBALOOP_MACHINE_ID=$(hostname | tr '-' '_')` to service file
4. **Missing SCOPE**: Add `Environment=BUBBALOOP_SCOPE=local` to service file
5. **Hyphenated topics/machine IDs**: Edit config.yaml and service files to use underscores
6. **Schema key mismatch**: Update node code to use consistent underscore naming for both data topics and schema keys
7. **Bridge disconnecting**: Kill duplicates, restart bridge, check version match
8. **Dashboard empty**: Refresh browser after bridge/Vite restart
9. **Schema not decoded**: Node needs DESCRIPTOR constant + schema queryable (see ARCHITECTURE.md)
10. **No journal storage**: Create `/var/log/journal/` and restart journald
11. **Camera no frames**: Check RTSP source connectivity, verify URL in config.yaml
