# bubbaloop-sdk (Python)

Pure Python wrapper over `zenoh-python` with the same API surface as the Rust Node SDK.
No compilation required — installable directly from the git repository.

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

### Protobuf node

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

### JSON node

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

### Proto subscriber

```python
from bubbaloop_sdk import NodeContext
from my_protos_pb2 import SensorData

ctx = NodeContext.connect()
sub = ctx.subscriber("sensor/data", SensorData)

for msg in sub:
    print(f"value: {msg.value}")
```

### Schema queryable (protobuf nodes)

```python
from bubbaloop_sdk.schema import declare_schema_queryable
from my_protos_pb2 import SensorData

# Declare once — keeps the queryable alive while the reference is held
schema_qbl = declare_schema_queryable(
    ctx.session, ctx.machine_id, "my-node", SensorData
)
```

## Configuration

| Environment variable | Default | Description |
|---|---|---|
| `BUBBALOOP_ZENOH_ENDPOINT` | `tcp/127.0.0.1:7447` | Zenoh router endpoint |
| `BUBBALOOP_MACHINE_ID` | hostname (sanitized) | Machine identifier |

## Requirements

- Python 3.9+
- `eclipse-zenoh >= 1.7, < 2`
- `protobuf >= 4.0`

## API

### `NodeContext`

| Method | Description |
|---|---|
| `NodeContext.connect(endpoint=None)` | Connect to Zenoh router |
| `ctx.topic(suffix)` | Build `bubbaloop/global/{machine_id}/{suffix}` |
| `ctx.local_topic(suffix)` | Build `bubbaloop/local/{machine_id}/{suffix}` (SHM-only) |
| `ctx.publisher_proto(suffix, msg_class)` | Declared protobuf publisher |
| `ctx.publisher_json(suffix)` | Declared JSON publisher |
| `ctx.subscriber(suffix, msg_class=None)` | Proto subscriber (iterable) |
| `ctx.subscriber_raw(key_expr)` | Raw sample subscriber (no topic prefix) |
| `ctx.is_shutdown()` | True after SIGINT/SIGTERM |
| `ctx.wait_shutdown()` | Block until shutdown |
| `ctx.close()` | Close the Zenoh session |

### `ProtoPublisher` / `JsonPublisher`

| Method | Description |
|---|---|
| `pub.put(msg)` | Publish a message |
| `pub.undeclare()` | Release the Zenoh publisher |

### `ProtoSubscriber` / `RawSubscriber`

Both support `for` iteration.  `RawSubscriber` also exposes `recv()` for
direct use and yields `zenoh.Sample` objects directly.
