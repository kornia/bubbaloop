# Debug Dashboard

Live debugging workflow for the bubbaloop dashboard. Diagnoses why panels aren't showing data by checking the full pipeline: Zenoh topics → daemon → WebSocket bridge → dashboard components.

## Usage
```
/debug-dashboard              # Full diagnostic
/debug-dashboard camera       # Debug specific panel
/debug-dashboard raw           # Debug JSON/raw panel
/debug-dashboard stats         # Debug stats panel
```

## Diagnostic Pipeline

### Step 1: Check Services Running
Verify all required services are up:
```bash
pgrep -a zenohd              # Zenoh router
pgrep -a zenoh-bridge        # WebSocket bridge (port 10001)
pgrep -a bubbaloop           # Daemon
pgrep -a vite                # Dashboard dev server
```

### Step 2: Check Zenoh Topics
Query what topics have active publishers:
```bash
# List all active Zenoh key expressions
timeout 3 bubbaloop topics 2>/dev/null || echo "No topics command available"
```

### Step 3: Check Node Status
```bash
bubbaloop status 2>/dev/null || bubbaloop list 2>/dev/null
```

### Step 4: Check WebSocket Bridge
The dashboard connects via WebSocket to `ws://localhost:10001`. Verify bridge is listening:
```bash
ss -tlnp | grep 10001
```

### Step 5: Check Browser Console
Ask user to open browser DevTools (F12) → Console tab and report:
- Any red errors
- WebSocket connection status messages
- Schema registry loading messages

### Step 6: Check Dashboard Subscriptions
Read the subscription manager and zenoh connection code:
- `dashboard/src/lib/zenoh.ts` — connection setup
- `dashboard/src/lib/subscription-manager.ts` — topic subscriptions
- `dashboard/src/contexts/ZenohSubscriptionContext.tsx` — what topics are subscribed

### Step 7: Check Component Data Flow
For each panel type, trace the data flow:

**Camera Panel**:
- Topic: `bubbaloop/**/camera/**` or node-specific camera topics
- Component: `dashboard/src/components/CameraView.tsx`
- Needs: H264/MJPEG video frames on the topic

**Raw/JSON Panel**:
- Topic: any topic selected by user
- Component: `dashboard/src/components/JsonView.tsx`
- Decode chain: JSON → SchemaRegistry (protobuf) → built-in decoders → plain text → hex
- Check: SchemaRegistry loaded? Topic has data?

**Stats Panel**:
- Topic: `bubbaloop/**/stats` or telemetry topics
- Component: `dashboard/src/components/StatsView.tsx`

**System Telemetry**:
- Topic: machine telemetry topics
- Component: `dashboard/src/components/SystemTelemetryView.tsx`

### Step 8: Fix Common Issues
1. **No data on any panel**: WebSocket bridge not connecting → check port 10001
2. **Schema registry empty**: Daemon not serving schemas at `*/daemon/api/schemas`
3. **Camera black**: No camera node running, or wrong video format
4. **JSON panel empty**: No publisher on selected topic
5. **Stats missing**: No telemetry node running
6. **Too many daemon items**: Filter or simplify the nodes list display

### Jetson-Specific
- Dashboard at https://localhost:5173/ (SSL required for WebCodecs)
- Bridge at ws://localhost:10001
- Daemon binds localhost only
