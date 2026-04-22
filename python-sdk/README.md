# bubbaloop-sdk (Python)

Pure Python wrapper over `zenoh-python` with the same API surface as the Rust Node SDK.
No compilation required — installable directly from the git repository.

> **Parity invariant:** This SDK is a peer of `crates/bubbaloop-node/` (Rust), not a layer on top of it. Every publisher/subscriber/context method added to one SDK MUST have an equivalent in the other in the same PR. Where `zenoh-python` can't surface a knob that Rust exposes, the Python side collapses it to the simplest equivalent that preserves wire behavior. Example: Rust `publisher_cbor_shm(suffix, slot_count, slot_size)` ↔ Python `publisher_cbor(suffix, local=True)` — same `application/cbor` encoding and `CongestionControl.Block` on the wire; slot sizing is implicit because `zenoh-python` doesn't expose `ShmProvider`.

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

### CBOR node (recommended)

CBOR is self-describing — field names travel as map keys, no schema registration needed.

```python
import time
from bubbaloop_sdk import NodeContext

ctx = NodeContext.connect()

# Publisher — encoding set once at declaration
pub = ctx.publisher_cbor("sensor/data")

while not ctx.is_shutdown():
    pub.put({"value": 42.0, "unit": "celsius"})
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

### CBOR subscriber

```python
from bubbaloop_sdk import NodeContext

ctx = NodeContext.connect()
sub = ctx.subscribe("sensor/data")   # auto-decodes CBOR, JSON, or returns raw bytes

for msg in sub:
    # CBOR dict → SimpleNamespace (attribute access)
    print(f"value: {msg.value}")
```

### Raw subscriber (SHM frames)

```python
from bubbaloop_sdk import NodeContext

ctx = NodeContext.connect()
sub = ctx.subscribe_raw("camera/raw", local=True)

for raw_bytes in sub:
    import torch
    tensor = torch.frombuffer(raw_bytes, dtype=torch.uint8)
```

## Configuration

| Environment variable | Default | Description |
|---|---|---|
| `BUBBALOOP_ZENOH_ENDPOINT` | `tcp/127.0.0.1:7447` | Zenoh router endpoint |
| `BUBBALOOP_MACHINE_ID` | hostname (sanitized) | Machine identifier |

## Requirements

- Python 3.9+
- `eclipse-zenoh >= 1.7, < 2`
- `cbor2 >= 5.0`

## API

### `NodeContext`

| Method | Description |
|---|---|
| `NodeContext.connect(endpoint=None)` | Connect to Zenoh router |
| `ctx.topic(suffix)` | Build `bubbaloop/global/{machine_id}/{suffix}` |
| `ctx.local_topic(suffix)` | Build `bubbaloop/local/{machine_id}/{suffix}` (SHM-only) |
| `ctx.publisher_cbor(suffix)` | Declared CBOR publisher |
| `ctx.publisher_json(suffix)` | Declared JSON publisher |
| `ctx.publisher_raw(suffix, local=False)` | Declared raw-bytes publisher |
| `ctx.subscribe(suffix, local=False)` | CBOR/JSON/raw subscriber (iterable) |
| `ctx.subscribe_raw(suffix, local=False)` | Raw bytes subscriber (iterable) |
| `ctx.is_shutdown()` | True after SIGINT/SIGTERM |
| `ctx.wait_shutdown()` | Block until shutdown |
| `ctx.close()` | Close the Zenoh session |

### `CborPublisher` / `JsonPublisher` / `RawPublisher`

| Method | Description |
|---|---|
| `pub.put(value)` | Publish a value |
| `pub.undeclare()` | Release the Zenoh publisher |

### `CborSubscriber` / `RawSubscriber`

Both support `for` iteration. Decoding:

- `CborSubscriber`: CBOR → SimpleNamespace, JSON → dict, other → bytes
- `RawSubscriber`: always returns raw `bytes`
