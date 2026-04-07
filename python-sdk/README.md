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
import asyncio
from bubbaloop_sdk import NodeContext
from my_protos_pb2 import SensorData

async def run():
    ctx = await NodeContext.connect()

    # Publisher — encoding set once at declaration
    pub = await ctx.publisher_proto("sensor/data", SensorData)

    while not ctx.is_shutdown():
        msg = SensorData(value=42.0)
        await pub.put(msg)
        await asyncio.sleep(0.1)

    pub.undeclare()
    ctx.close()

asyncio.run(run())
```

### JSON node

```python
import asyncio
from bubbaloop_sdk import NodeContext

async def run():
    ctx = await NodeContext.connect()
    pub = await ctx.publisher_json("weather/current")

    while not ctx.is_shutdown():
        await pub.put({"temperature": 22.5, "humidity": 60})
        await asyncio.sleep(1.0)

    pub.undeclare()
    ctx.close()

asyncio.run(run())
```

### Typed subscriber

```python
import asyncio
from bubbaloop_sdk import NodeContext
from my_protos_pb2 import SensorData

async def run():
    ctx = await NodeContext.connect()
    sub = await ctx.subscriber("sensor/data", SensorData)

    async for msg in sub:
        print(f"value: {msg.value}")

asyncio.run(run())
```

### Schema queryable (protobuf nodes)

```python
from bubbaloop_sdk.schema import declare_schema_queryable
from my_protos_pb2 import SensorData

# Declare once — keeps the queryable alive while the reference is held
schema_qbl = declare_schema_queryable(
    ctx.session, ctx.scope, ctx.machine_id, "my-node", SensorData
)
```

## Configuration

| Environment variable | Default | Description |
|---|---|---|
| `BUBBALOOP_ZENOH_ENDPOINT` | `tcp/127.0.0.1:7447` | Zenoh router endpoint |
| `BUBBALOOP_SCOPE` | `local` | Topic scope |
| `BUBBALOOP_MACHINE_ID` | hostname (sanitized) | Machine identifier |

## Requirements

- Python 3.9+
- `eclipse-zenoh >= 1.7, < 2`
- `protobuf >= 4.0`

## API

### `NodeContext`

| Method | Description |
|---|---|
| `await NodeContext.connect(endpoint=None)` | Connect to Zenoh router |
| `ctx.topic(suffix)` | Build `bubbaloop/{scope}/{machine_id}/{suffix}` |
| `await ctx.publisher_proto(suffix, msg_class)` | Declared protobuf publisher |
| `await ctx.publisher_json(suffix)` | Declared JSON publisher |
| `await ctx.subscriber(suffix, msg_class=None)` | Typed async-iterable subscriber |
| `await ctx.subscriber_raw(key_expr)` | Raw sample subscriber (no topic prefix) |
| `ctx.is_shutdown()` | True after SIGINT/SIGTERM |
| `await ctx.wait_shutdown()` | Suspend until shutdown |
| `ctx.close()` | Close the Zenoh session |

### `ProtoPublisher` / `JsonPublisher`

| Method | Description |
|---|---|
| `await pub.put(msg)` | Publish a message |
| `pub.undeclare()` | Release the Zenoh publisher |

### `TypedSubscriber` / `RawSubscriber`

Both support `async for` iteration.  `RawSubscriber` also exposes `recv()` for
synchronous use and yields `zenoh.Sample` objects directly.
