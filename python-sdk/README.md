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

ctx.close()
```

### Publish protobuf

```python
import time
from bubbaloop_sdk import NodeContext
from my_protos_pb2 import SensorData

ctx = NodeContext.connect()
pub = ctx.publisher_proto("sensor/data", SensorData)

while not ctx.is_shutdown():
    pub.put(SensorData(value=42.0))
    time.sleep(0.1)

ctx.close()
```

### Blocking subscriber (poll loop)

```python
from bubbaloop_sdk import NodeContext
from my_protos_pb2 import SensorData

ctx = NodeContext.connect()
sub = ctx.subscriber("sensor/data", SensorData)

while not ctx.is_shutdown():
    msg = sub.recv(timeout=5.0)   # returns None on timeout
    if msg is not None:
        print(f"value: {msg.value}")

ctx.close()
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
| `BUBBALOOP_SCOPE` | `local` | Topic scope |
| `BUBBALOOP_MACHINE_ID` | hostname (sanitized) | Machine identifier |

## API reference

### `NodeContext`

| Method | Returns | Description |
|---|---|---|
| `NodeContext.connect(endpoint=None, instance_name=None)` | `NodeContext` | Connect to Zenoh router |
| `ctx.topic(suffix)` | `str` | Build `bubbaloop/{scope}/{machine_id}/{suffix}` |
| `ctx.is_shutdown()` | `bool` | True after SIGINT/SIGTERM |
| `ctx.wait_shutdown()` | — | Block until SIGINT/SIGTERM |
| `ctx.close()` | — | Close the Zenoh session |

#### Publishers

| Method | Returns | Description |
|---|---|---|
| `ctx.publisher_json(suffix)` | `JsonPublisher` | JSON publisher at `topic(suffix)` |
| `ctx.publisher_proto(suffix, msg_class=None)` | `ProtoPublisher` | Protobuf publisher at `topic(suffix)` |

#### Blocking subscribers (poll with `recv`)

| Method | Returns | Description |
|---|---|---|
| `ctx.subscriber(suffix, msg_class=None)` | `TypedSubscriber` | Queue-backed subscriber; `recv(timeout)` returns `None` on timeout |
| `ctx.subscriber_raw(key_expr)` | `RawSubscriber` | Same but yields raw `zenoh.Sample`; uses literal key expression |

#### Callback subscribers (event-driven)

Handler is called from Zenoh's internal thread. Keep handlers fast; use `_async`
variants for slow work.

| Method | Returns | Description |
|---|---|---|
| `ctx.subscriber_callback(suffix, handler, msg_class=None)` | `CallbackSubscriber` | Decoded message passed to handler |
| `ctx.subscriber_raw_callback(key_expr, handler)` | `RawCallbackSubscriber` | Raw `zenoh.Sample` passed to handler; literal key expression |
| `ctx.subscriber_callback_async(suffix, handler, msg_class=None, max_workers=4)` | `CallbackSubscriberAsync` | Handler runs in thread pool |
| `ctx.subscriber_raw_callback_async(key_expr, handler, max_workers=4)` | `RawCallbackSubscriberAsync` | Raw sample; handler in thread pool |

#### Queryables

Do **not** pass `complete=True` — it blocks wildcard queries used by the dashboard.

| Method | Returns | Description |
|---|---|---|
| `ctx.queryable(suffix, handler)` | `zenoh.Queryable` | Handler at `topic(suffix)`; called from Zenoh thread |
| `ctx.queryable_raw(key_expr, handler)` | `zenoh.Queryable` | Handler at literal key expression |
| `ctx.queryable_async(suffix, handler, max_workers=4)` | `AsyncQueryable` | Handler in thread pool; call `undeclare()` to release |
| `ctx.queryable_raw_async(key_expr, handler, max_workers=4)` | `AsyncQueryable` | Raw key; handler in thread pool |

### Publishers

| Method | Description |
|---|---|
| `pub.put(msg)` | Publish a message (bytes, proto message, or dict for JSON) |

### Blocking subscribers

| Method | Description |
|---|---|
| `sub.recv(timeout=None)` | Return next message or `None` on timeout |
| `sub.undeclare()` | Stop receiving samples |
| `for msg in sub` | Iterate (blocks indefinitely) |

### Callback subscribers / AsyncQueryable

| Method | Description |
|---|---|
| `sub.undeclare()` | Undeclare subscriber and shut down thread pool (async variants) |

## Requirements

- Python 3.9+
- `eclipse-zenoh >= 1.7, < 2`
- `protobuf >= 4.0`
