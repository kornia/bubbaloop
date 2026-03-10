# Python Node Examples

Standalone Python nodes that connect to Zenoh and publish sensor data. Each node runs
in **mock mode** by default — no hardware required. Switch to real hardware by editing
`config.yaml`.

## Prerequisites

All nodes require Python 3.10+ and the `zenoh` package. Install with:

```bash
pip install zenoh pyyaml
```

A running Zenoh router is needed to route messages between nodes:

```bash
zenohd
# or via bubbaloop:
bubbaloop daemon start
```

## Nodes

### camera-snapshot

HTTP snapshot camera — polls an IP camera's JPEG endpoint and publishes raw frames.

**Mock mode** (default): generates a minimal 1x1 JPEG every second.
**Real mode**: polls `{url}/snapshot` or `{url}` and publishes the JPEG bytes.

```bash
cd camera-snapshot
pip install -r requirements.txt
python main.py                          # mock mode
python main.py -c config.yaml           # from config (default)
python main.py -e tcp/192.168.1.5:7447  # custom Zenoh endpoint
```

Edit `config.yaml` to switch to a real camera:
```yaml
url: http://192.168.1.10   # replace mock:// with real camera URL
interval_secs: 1.0
```

Published topic: `bubbaloop/{scope}/{machine_id}/{name}/snapshot`
Payload: raw JPEG bytes

---

### serial-sensor

Reads a serial port line-by-line. Parses each line as JSON, then falls back to CSV
(`val1,val2,...` -> `{"values": [...]}`). Publishes parsed JSON to Zenoh.

**Mock mode** (default): generates simulated BME280-style readings (temperature,
humidity, pressure) without any hardware.
**Real mode**: reads from the configured serial port.

```bash
cd serial-sensor
pip install -r requirements.txt
python main.py                           # mock mode (port: mock)
```

Edit `config.yaml` for real hardware:
```yaml
port: /dev/ttyUSB0    # or /dev/ttyACM0 for Arduino
baud_rate: 115200
```

Real serial port requires pyserial:
```bash
pip install pyserial
```

The node accepts any line format your firmware sends:
- JSON: `{"temp": 22.5, "humidity": 60}` -> published as-is
- CSV: `22.5,60,1013` -> `{"values": [22.5, 60.0, 1013.0]}`
- Other: `{"raw": "the line"}` as fallback

Published topic: `bubbaloop/{scope}/{machine_id}/{name}/data`
Payload: JSON bytes

---

### mqtt-bridge

Subscribes to an MQTT topic and republishes each message to Zenoh as a JSON envelope:
```json
{"topic": "home/living_room/temperature", "payload": {...}, "timestamp_ms": 1234567890}
```

**Mock mode** (default): generates fake MQTT messages on a timer.
**Real mode**: connects to a real MQTT broker.

```bash
cd mqtt-bridge
pip install -r requirements.txt
python main.py                           # mock mode
```

Edit `config.yaml` for a real broker:
```yaml
broker: 192.168.1.1     # replace mock with broker IP
broker_port: 1883
mqtt_topic: home/+/temperature
```

Real MQTT requires paho-mqtt:
```bash
pip install paho-mqtt
```

Published topic: `bubbaloop/{scope}/{machine_id}/{name}/data`
Payload: JSON bytes

---

## Common options

All nodes accept:

| Flag | Default | Description |
|------|---------|-------------|
| `-c, --config` | `config.yaml` | Path to config file |
| `-e, --endpoint` | `tcp/127.0.0.1:7447` | Zenoh router endpoint |

Environment variables:

| Variable | Default | Description |
|----------|---------|-------------|
| `BUBBALOOP_SCOPE` | `local` | Topic namespace scope |
| `BUBBALOOP_MACHINE_ID` | `localhost` | Machine identifier |
| `ZENOH_ENDPOINT` | `tcp/127.0.0.1:7447` | Fallback for `-e` flag |

## Watching published data

Use the bubbaloop CLI or zenoh-tools to subscribe:

```bash
# All data from this machine
bubbaloop node logs <name>

# Raw zenoh subscribe (requires z_sub from zenoh-tools)
z_sub --key "bubbaloop/local/**"
```

## Zenoh rules (do not break these)

- `query.key_expr` is a **property**, not a method. Never call `query.key_expr()`.
- Never use `complete=True` on queryables — it blocks wildcard discovery.
- Always use `mode: "client"` when connecting to a Zenoh router.
- Topic paths use underscores for machine IDs with hyphens (enforced in code).
