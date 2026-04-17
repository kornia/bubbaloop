# bubbaloop-sdk (Python)

Pure Python wrapper over `zenoh-python`. Synchronous API — no asyncio required.
No compilation needed; install directly from the git repository.

## Install

```bash
pip install git+https://github.com/kornia/bubbaloop.git#subdirectory=python-sdk
```

Or for development:

```bash
cd python-sdk
pip install -e ".[dev]"
```

## Quick start

### Publish JSON

```python
import time
from bubbaloop_sdk import NodeContext

ctx = NodeContext.connect()
pub = ctx.publisher_json("weather/current")

while not ctx.is_shutdown():
    pub.put({"temperature": 22.5, "humidity": 60})
    time.sleep(1.0)

pub.undeclare()
ctx.close()
```

### Publish protobuf

```python
import time
from bubbaloop_sdk import NodeContext
from my_protos_pb2 import SensorData

ctx = NodeContext.connect()

# Publisher — encoding set once at declaration
pub = ctx.publisher_proto("sensor/data", SensorData)

while not ctx.is_shutdown():
    msg = SensorData(value=42.0)
    pub.put(msg)
    time.sleep(0.1)

pub.undeclare()
ctx.close()
```

### Auto-decode subscriber

```python
from bubbaloop_sdk import NodeContext

ctx = NodeContext.connect()
sub = ctx.subscribe("sensor/data")

for msg in sub:   # auto-decoded: proto, dict, or bytes
    print(msg)
```

### Callback subscriber (event-driven, no loop needed)

```python
from bubbaloop_sdk import NodeContext

ctx = NodeContext.connect()

def on_sensor(msg):
    print(f"received: {msg}")  # proto, dict, or bytes depending on encoding

sub = ctx.subscriber_callback("sensor/data", on_sensor)
ctx.wait_shutdown()   # block until SIGINT/SIGTERM
sub.undeclare()
ctx.close()
```

Pass `max_workers` when the handler does slow work (DB writes, HTTP calls) —
the handler runs in a thread pool, freeing Zenoh's internal thread:

```python
sub = ctx.subscriber_callback("sensor/data", on_sensor, max_workers=4)
```

### Queryable (respond to get requests)

```python
import json
from bubbaloop_sdk import NodeContext

ctx = NodeContext.connect()

def on_query(query):
    query.reply(query.key_expr, json.dumps({"status": "ok"}).encode())

qbl = ctx.queryable("status", on_query)
ctx.wait_shutdown()
qbl.undeclare()
ctx.close()
```

Pass `max_workers` when the handler does slow work:

```python
qbl = ctx.queryable("status", on_query, max_workers=4)
qbl.undeclare()   # call when done to release the thread pool
```

## Configuration

| Environment variable | Default | Description |
|---|---|---|
| `BUBBALOOP_ZENOH_ENDPOINT` | `tcp/127.0.0.1:7447` | Zenoh router endpoint |
| `BUBBALOOP_MACHINE_ID` | hostname (sanitized) | Machine identifier |

## API reference

### `NodeContext`

| Method | Description |
|---|---|
| `NodeContext.connect(endpoint=None)` | Connect to Zenoh router |
| `ctx.topic(suffix)` | Build `bubbaloop/global/{machine_id}/{suffix}` |
| `ctx.local_topic(suffix)` | Build `bubbaloop/local/{machine_id}/{suffix}` (SHM-only) |
| `ctx.publisher_proto(suffix, msg_class)` | Declared protobuf publisher |
| `ctx.publisher_json(suffix)` | Declared JSON publisher |
| `ctx.publisher_raw(suffix, local=False)` | Declared raw publisher (no encoding) |
| `ctx.subscribe(suffix, local=False)` | Auto-decode subscriber (proto/json/bytes) |
| `ctx.subscribe_raw(suffix, local=False)` | Raw bytes subscriber |
| `ctx.is_shutdown()` | True after SIGINT/SIGTERM |
| `ctx.wait_shutdown()` | Block until shutdown |
| `ctx.close()` | Close the Zenoh session |

#### Callback subscribers (event-driven)

Handler runs on Zenoh's internal thread by default. Pass `max_workers` to
run the handler in a thread pool instead (for slow work).

| Method | Description |
|---|---|
| `ctx.subscriber_callback(suffix, handler, max_workers=None)` | Auto-decoded message to handler |
| `ctx.subscriber_raw_callback(key_expr, handler, max_workers=None)` | Raw `zenoh.Sample` to handler |

#### Queryables

Do **not** pass `complete=True` — it blocks wildcard queries used by the dashboard.

| Method | Description |
|---|---|
| `ctx.queryable(suffix, handler, max_workers=None)` | Queryable at `topic(suffix)` |
| `ctx.queryable_raw(key_expr, handler, max_workers=None)` | Queryable at literal key expression |

#### Publishers

| Method | Description |
|---|---|
| `pub.put(msg)` | Publish a message |
| `pub.undeclare()` | Release the Zenoh publisher |

### `ProtoSubscriber` / `RawSubscriber`

Both support `for` iteration. `RawSubscriber` yields `bytes` directly.

## Requirements

- Python 3.10+
- `eclipse-zenoh >= 1.7, < 2`
- `protobuf >= 4.0`
