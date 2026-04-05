# Contributing to bubbaloop-sdk (Python)

## Dev environment setup

```bash
cd python-sdk
python3 -m venv .venv
.venv/bin/pip install -e ".[dev]"
```

Dev deps installed: `pytest`, `pytest-asyncio`, `ruff`.

## Running tests

```bash
cd python-sdk
.venv/bin/python -m pytest tests/ -v
```

## Linting (ruff)

Config lives in `python-sdk/pyproject.toml` under `[tool.ruff]`. Same pattern as [Kornia](https://github.com/kornia/kornia).

```bash
cd python-sdk
.venv/bin/python -m ruff check bubbaloop_sdk/ tests/   # check
.venv/bin/python -m ruff check --fix bubbaloop_sdk/ tests/   # auto-fix
.venv/bin/python -m ruff format bubbaloop_sdk/ tests/  # format
```

Key settings:
- `line-length = 120`
- Rules: E/W (pycodestyle), F (Pyflakes), I (isort), B (bugbear), UP (pyupgrade)
- `F821` globally ignored — forward-reference string annotations in `context.py` use lazy imports by design
- Per-file ignores for pre-existing issues in upstream files (`health.py`, `node.py`, `get_sample.py`)

## Known pre-existing issues (upstream files, not touched)

| File | Rule | Reason |
|------|------|--------|
| `health.py` | F401 unused import (`time`) | Pre-existing upstream code |
| `node.py` | F401 unused imports (`os`, `time`) | Pre-existing upstream code |
| `get_sample.py` | B904 raise in except | Pre-existing upstream code |

## Testing notes

Tests in `tests/test_context.py` do **not** open a real Zenoh session.
`_make_context()` helper uses `object.__new__(NodeContext)` + `MagicMock()` session — no router needed.

For async subscriber/queryable tests, `threading.Event` with a 2s timeout is used to verify the handler was dispatched to the thread pool.

## Project structure

```
bubbaloop_sdk/
  __init__.py          # Public API surface
  context.py           # NodeContext — main entry point
  subscriber.py        # TypedSubscriber, RawSubscriber, Callback*, Async*
  publisher.py         # JsonPublisher, ProtoPublisher
  node.py              # run_node() helper
  health.py            # Health heartbeat (used internally by run_node)
  discover.py          # discover_nodes()
  get_sample.py        # get_sample() one-shot helper
  decode_sample.py     # ProtoDecoder
tests/
  test_context.py      # 48 unit tests (no real Zenoh required)
```
