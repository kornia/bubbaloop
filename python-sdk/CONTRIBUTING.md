# Contributing to bubbaloop-sdk (Python)

## Dev environment

### With pixi (recommended)

```bash
cd python-sdk
pixi install   # creates env, installs all deps including dev extras
pixi run test
pixi run lint
pixi run fmt
```

Available tasks:

| Task | Description |
|---|---|
| `pixi run test` | Run test suite |
| `pixi run test-cov` | Run tests with coverage report |
| `pixi run lint` | Check for lint errors (ruff) |
| `pixi run lint-fix` | Auto-fix lint errors |
| `pixi run fmt` | Format code |
| `pixi run fmt-check` | Check formatting without changing files |
| `pixi run check` | Run fmt-check + lint (CI equivalent) |

### With plain venv

```bash
cd python-sdk
python3 -m venv .venv
.venv/bin/pip install -e ".[dev]"
.venv/bin/pytest tests/ -v
.venv/bin/ruff check bubbaloop_sdk/ tests/
```

## Linting (ruff)

Config lives in `python-sdk/pyproject.toml` under `[tool.ruff]`.
Follows the same pattern as [kornia/kornia](https://github.com/kornia/kornia).

Rules enabled: E/W (pycodestyle), F (Pyflakes), I (isort), B (bugbear),
UP (pyupgrade), C4 (comprehensions), RUF (ruff-specific).

Line length: 120 characters.

## Lint suppressions

| File | Rule | Reason |
|---|---|---|
| `*/__init__.py` | F401, F403 | Re-exports allowed |
| `tests/*` | S101, D | Assert and missing docstrings allowed in tests |

## Testing

Tests in `tests/test_context.py` do **not** open a real Zenoh session.
`_make_context()` uses `object.__new__(NodeContext)` + `MagicMock()` — no router needed.

For async subscriber/queryable tests, `threading.Event` with a 2s timeout
verifies that handlers are dispatched to the thread pool correctly.

## Project structure

```text
python-sdk/
  pyproject.toml        # Build config, deps, ruff/pytest/coverage config
  pixi.toml             # Pixi tasks (test, lint, fmt, check)
  README.md             # User-facing API docs
  bubbaloop_sdk/
    __init__.py         # Public API surface
    context.py          # NodeContext — main entry point
    subscriber.py       # ProtoSubscriber, RawSubscriber, CallbackSubscriber, RawCallbackSubscriber, Queryable
    publisher.py        # JsonPublisher, ProtoPublisher
    node.py             # run_node() helper
    health.py           # Health heartbeat (used internally by run_node)
    discover.py         # discover_nodes()
    get_sample.py       # get_sample() one-shot helper
    decode_sample.py    # ProtoDecoder
  tests/
    test_context.py     # 68 unit tests (no real Zenoh required)
```
