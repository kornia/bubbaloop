# bubbaloop-sdk (Python)

Pure Python wrapper over `zenoh-python`. Synchronous API — no asyncio required.
Mirrors the Rust `bubbaloop-node` SDK surface; nodes written with either SDK are interoperable.

## Structure

```
python-sdk/
  bubbaloop_sdk/
    __init__.py       # Public API — edit when adding new public names
    context.py        # NodeContext: connect(), topic(), publishers, subscribers, queryables
    publisher.py      # JsonPublisher, ProtoPublisher (wraps session.declare_publisher)
    subscriber.py     # TypedSubscriber, RawSubscriber, Callback*, Async*, AsyncQueryable
    node.py           # run_node() — CLI arg parsing + health heartbeat + lifecycle
    health.py         # start_health_heartbeat() — publishes 'ok' every 5s
    discover.py       # discover_nodes() — GET bubbaloop/**/health
    get_sample.py     # get_sample() — one-shot async subscribe-and-wait
    decode_sample.py  # ProtoDecoder — decode zenoh.Sample to protobuf
  tests/
    test_context.py   # 48 unit tests — NO real Zenoh session needed
  pyproject.toml      # Build config, deps, ruff/pytest/coverage
  pixi.toml           # Dev tasks: test, lint, fmt, check
```

## Build & verify

```bash
# With pixi (recommended)
cd python-sdk
pixi run check       # fmt-check + lint (run before every commit)
pixi run test        # 48 unit tests
pixi run test-cov    # tests + coverage report

# With venv (alternative)
cd python-sdk
.venv/bin/python -m ruff check bubbaloop_sdk/ tests/
.venv/bin/python -m pytest tests/ -v
```

## Conventions — MUST follow

**Tooling:**
- `ruff` for lint + format — NOT flake8, black, or isort directly
- Config in `pyproject.toml` under `[tool.ruff]` — do NOT add `.flake8` or `setup.cfg`
- Line length: 120 characters
- `TYPE_CHECKING` guard for cross-module type annotations — NEVER string-quoted forward refs (`"Foo"`)

**Imports:**
- Cross-module type-only imports go under `if TYPE_CHECKING:` at the top of the file
- Lazy runtime imports (inside method bodies) are kept to avoid circular import issues
- `__init__.py` must be updated whenever a new public class is added to `subscriber.py` or `publisher.py`

**Zenoh session:**
- ALWAYS use `mode: "client"` — peer mode does not route through zenohd
- NEVER use `.complete(True)` on queryables — blocks wildcard queries like `bubbaloop/**/schema`
- `query.key_expr` is a **property**, NOT a method — NEVER write `query.key_expr()`
- `query.reply(query.key_expr, payload_bytes)` — correct reply pattern

**Threading — critical:**
- Zenoh uses **one internal thread** for ALL callbacks and queryables on a session
- A slow handler blocks every other subscriber/queryable until it returns
- Use `_async` variants (`subscriber_callback_async`, `queryable_async`) for any handler that does I/O, DB access, or hardware calls
- Shutdown order for `_async` variants: undeclare Zenoh subscriber FIRST, then `executor.shutdown()` — reversing this causes `RuntimeError: cannot schedule new futures after shutdown`

**`undeclare()` discipline:**
- Every subscriber, callback subscriber, and queryable must be undeclared when done
- `AsyncQueryable` and `*Async` subscribers own a `ThreadPoolExecutor` — GC alone is not enough, always call `undeclare()`
- Blocking subscribers (`TypedSubscriber`, `RawSubscriber`) are undeclared via `undeclare()` too

## Testing

Tests do NOT open a real Zenoh session. Use `_make_context()`:

```python
def _make_context(scope, machine_id):
    from bubbaloop_sdk.context import NodeContext
    ctx = object.__new__(NodeContext)
    ctx.session = MagicMock()
    ctx.scope = scope
    ctx.machine_id = machine_id
    ctx.instance_name = machine_id
    ctx._shutdown = threading.Event()
    return ctx
```

For async/threaded tests use `threading.Event` with a 2s timeout — do NOT use `time.sleep`:

```python
event = threading.Event()
def handler(msg):
    received.append(msg)
    event.set()
assert event.wait(timeout=2.0), "handler not called within 2s"
```

## DO / DON'T

**DO:** `pixi run check` before every commit | add tests when adding public methods | update `__init__.py` and its `__all__` for every new public class | call `undeclare()` in tests that create async subscribers or queryables

**DON'T:** use `asyncio` — the SDK is synchronous by design | use `query.key_expr()` with parentheses | use `.complete(True)` on queryables | add string forward references (`"Foo"`) — use `TYPE_CHECKING` instead | suppress lint rules globally when a per-file or code-level fix is possible

## Pitfalls

- `B904` — always `raise Foo from err` inside `except` blocks, never bare `raise Foo(...)`
- `F401` in `__init__.py` is suppressed by ruff config (re-exports are intentional) — do NOT add `# noqa` comments there
- `CallbackSubscriber` and `RawCallbackSubscriber` do NOT own an executor — `undeclare()` only calls `_sub.undeclare()`; the `_async` variants do own an executor and shut it down in `undeclare()`
- `TypedSubscriber` and `RawSubscriber` are iterable (`for msg in sub`) but iteration blocks forever — always prefer `recv(timeout=...)` in shutdown-aware loops
- `run_node()` reads `config.yaml` by default; override with `-c path/config.yaml`. The `name` field in config sets `instance_name` for health/schema topics — collisions happen if two instances share the same name
- Health topic format: `bubbaloop/{scope}/{machine_id}/{instance_name}/health` — ensure consumer patterns match exactly
