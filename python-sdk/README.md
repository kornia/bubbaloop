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
    print(f"value: {msg.value}")
```

### Callback subscriber (event-driven, no loop needed)

```python
from bubbaloop_sdk import NodeContext
from my_protos_pb2 import SensorData

ctx = NodeContext.connect()

def on_sensor(msg: SensorData):
    print(f"received: {msg.value}")

sub = ctx.subscriber_callback("sensor/data", on_sensor, SensorData)
ctx.wait_shutdown()   # block until SIGINT/SIGTERM
sub.undeclare()
ctx.close()
```

Use `subscriber_callback_async` when the handler does slow work (DB writes,
HTTP calls) — it runs the handler in a thread pool and returns immediately,
freeing Zenoh's internal thread:

```python
sub = ctx.subscriber_callback_async("sensor/data", on_sensor, SensorData)
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

Use `queryable_async` when the handler does slow work:

```python
qbl = ctx.queryable_async("status", on_query)
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

Handler is called from Zenoh's internal thread. Keep handlers fast; use `_async`
variants for slow work.

| Method | Description |
|---|---|
| `ctx.subscriber_callback(suffix, handler, msg_class=None)` | Decoded message to handler |
| `ctx.subscriber_raw_callback(key_expr, handler)` | Raw `zenoh.Sample` to handler |
| `ctx.subscriber_callback_async(suffix, handler, msg_class=None, max_workers=4)` | Handler in thread pool |
| `ctx.subscriber_raw_callback_async(key_expr, handler, max_workers=4)` | Raw sample; handler in thread pool |

#### Queryables

Do **not** pass `complete=True` — it blocks wildcard queries used by the dashboard.

| Method | Description |
|---|---|
| `ctx.queryable(suffix, handler)` | Handler at `topic(suffix)` |
| `ctx.queryable_raw(key_expr, handler)` | Handler at literal key expression |
| `ctx.queryable_async(suffix, handler, max_workers=4)` | Handler in thread pool |
| `ctx.queryable_raw_async(key_expr, handler, max_workers=4)` | Raw key; handler in thread pool |

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
