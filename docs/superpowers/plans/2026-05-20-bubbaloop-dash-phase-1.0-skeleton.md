# bubbaloop-dash Phase 1.0 (Skeleton) Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Stand up the `bubbaloop-dash` repository: a FastAPI backend with bearer-token auth and a multiplexed Zenoh-over-WebSocket proxy, the existing Bubbaloop dashboard frontend lifted as a fork-snapshot with a login screen wired in, all packaged as a single Docker container and verified with CI. End state: user runs `docker run`, browses to the URL, pastes a bearer token, and sees the existing Dashboard/Loop/Chat views rendering live Zenoh data through the new backend.

**Architecture:** Single Docker container. Inside: FastAPI (uvicorn) serves both `/api/*` (REST, bearer auth) and `/ws/zenoh` (multiplexed Zenoh proxy via `zenoh-python`) and falls through to static-file serving of the built `frontend/dist/` SPA. The backend keeps one persistent Zenoh client connection to the user's `zenohd` (on the Jetson, reachable via Tailscale). Browser WebSocket subscriptions are ref-counted into shared upstream subs.

**Tech Stack:**
- Backend: Python 3.11, FastAPI, uvicorn, zenoh-python, pydantic-settings, structlog, pytest, httpx (testing), ruff, mypy. Package management via `uv`.
- Frontend: React 18 + TypeScript + Vite (existing, lifted verbatim from `bubbaloop/dashboard/`).
- Container: Multi-stage Dockerfile (node:20 build → python:3.11-slim runtime).
- CI: GitHub Actions (backend tests, frontend tests, Docker build).

**Repository location (host):** `/home/nvidia/bubbaloop-dash/` (sibling of `/home/nvidia/bubbaloop/`).

**Out of scope for this plan (later phases):**
- Video upload pipeline (Phase 1.1)
- Decode + indexing + LanceDB (Phase 1.2)
- Search REST endpoint + search UI (Phase 1.3)
- Polish, error UI, integrity sweeper, docs site, release (Phase 1.4)
- Proto vendoring (deferred to Phase 1.2 when LanceDB schema needs structured types)
- Map tab + sensor placement (separate spec)

---

## Prerequisites (one-time, before Task 1)

The executing engineer must have:
- `python3.11` available on the host (check: `python3.11 --version`)
- `uv` installed (`pip install uv` or `curl -LsSf https://astral.sh/uv/install.sh | sh`)
- `node` ≥ 20 (`node --version`)
- `docker` and `docker compose` (`docker --version`)
- `gh` (GitHub CLI), authenticated (`gh auth status`)
- A reachable Bubbaloop daemon on Tailscale (or any WAN-routable host) running `zenohd`. The Zenoh proxy tests use a synthetic in-process publisher, so a real daemon is NOT required to complete Phase 1.0 — but the manual smoke test at the end is more meaningful with one.

---

## File Structure (target end state after Phase 1.0)

```
/home/nvidia/bubbaloop-dash/                 # new repo, sibling of bubbaloop/
├── .dockerignore
├── .gitignore
├── .python-version                          # "3.11"
├── .source-sha                              # SHA of bubbaloop repo at lift time
├── README.md
├── CLAUDE.md
├── Dockerfile                               # multi-stage
├── docker-compose.yml                       # production
├── docker-compose.dev.yml                   # bind-mount sources
├── .github/workflows/ci.yml
├── backend/
│   ├── pyproject.toml
│   ├── uv.lock
│   ├── README.md
│   ├── src/dash/
│   │   ├── __init__.py
│   │   ├── app.py                           # FastAPI factory
│   │   ├── config.py                        # Settings (pydantic-settings)
│   │   ├── logging.py                       # structlog config
│   │   ├── auth.py                          # bearer-token dependency
│   │   ├── tokens.py                        # first-run token generator
│   │   ├── static.py                        # mount frontend/dist
│   │   ├── api/
│   │   │   ├── __init__.py
│   │   │   ├── health.py                    # /healthz, /readyz
│   │   │   └── zenoh_ws.py                  # /ws/zenoh
│   │   └── zenoh_proxy/
│   │       ├── __init__.py
│   │       ├── session.py                   # zenoh client lifecycle
│   │       ├── multiplex.py                 # ref-counted sub tracker
│   │       └── protocol.py                  # WS message parsing
│   └── tests/
│       ├── __init__.py
│       ├── conftest.py
│       ├── test_health.py
│       ├── test_auth.py
│       ├── test_tokens.py
│       ├── test_zenoh_ws.py
│       └── test_zenoh_multiplex.py
└── frontend/                                # forked from bubbaloop/dashboard/
    ├── (existing files preserved verbatim)
    ├── package.json
    ├── vite.config.ts                       # updated: dev proxy /api + /ws to backend
    ├── src/
    │   ├── App.tsx                          # existing (no Phase 1.0 changes)
    │   ├── main.tsx                         # updated: wrap in AuthProvider
    │   ├── components/
    │   │   ├── Login.tsx                    # NEW
    │   │   └── (existing components preserved)
    │   ├── contexts/
    │   │   ├── AuthContext.tsx              # NEW
    │   │   └── (existing contexts preserved)
    │   └── lib/
    │       ├── api.ts                       # NEW
    │       └── zenoh.ts                     # updated: /zenoh → /ws/zenoh + ?token=
    └── __tests__/                           # existing vitest setup preserved
```

Tasks below assume the executing engineer is at `/home/nvidia/` when running shell commands unless explicitly told otherwise. Each task ends with a commit.

---

## Task 1: Initialize the repository

**Files:**
- Create: `/home/nvidia/bubbaloop-dash/.gitignore`
- Create: `/home/nvidia/bubbaloop-dash/.python-version`
- Create: `/home/nvidia/bubbaloop-dash/.editorconfig`
- Create: `/home/nvidia/bubbaloop-dash/README.md` (stub)
- Create: `/home/nvidia/bubbaloop-dash/.source-sha`

- [ ] **Step 1: Create the directory and `git init`**

```bash
cd /home/nvidia
mkdir bubbaloop-dash
cd bubbaloop-dash
git init -b main
```

Expected: `Initialized empty Git repository in /home/nvidia/bubbaloop-dash/.git/`

- [ ] **Step 2: Record the source SHA of bubbaloop at lift time**

```bash
git -C /home/nvidia/bubbaloop rev-parse HEAD > /home/nvidia/bubbaloop-dash/.source-sha
cat /home/nvidia/bubbaloop-dash/.source-sha
```

Expected: a 40-character SHA printed.

- [ ] **Step 3: Create `.python-version`**

```bash
echo "3.11" > /home/nvidia/bubbaloop-dash/.python-version
```

- [ ] **Step 4: Create `.gitignore`**

File contents (`/home/nvidia/bubbaloop-dash/.gitignore`):

```gitignore
# Python
__pycache__/
*.py[cod]
*$py.class
*.so
.Python
.venv/
venv/
.pytest_cache/
.mypy_cache/
.ruff_cache/
*.egg-info/
.coverage
htmlcov/

# Node
node_modules/
dist/
.vite/

# Editor / OS
.vscode/
.idea/
.DS_Store
*.swp

# Runtime data (volume-mounted in production)
.data/
*.sqlite
*.sqlite-wal
*.sqlite-shm
lance/

# Secrets
.env
.env.local
.token
```

- [ ] **Step 5: Create `.editorconfig`**

File contents (`/home/nvidia/bubbaloop-dash/.editorconfig`):

```ini
root = true

[*]
end_of_line = lf
insert_final_newline = true
charset = utf-8
trim_trailing_whitespace = true

[*.py]
indent_style = space
indent_size = 4

[*.{ts,tsx,js,jsx,json,yaml,yml}]
indent_style = space
indent_size = 2
```

- [ ] **Step 6: Create stub `README.md`**

File contents (`/home/nvidia/bubbaloop-dash/README.md`):

```markdown
# bubbaloop-dash

Self-hosted backend + dashboard for Bubbaloop. Owns video upload, frame indexing, vector search, and serves the Bubbaloop dashboard UI.

See `docs/superpowers/specs/2026-05-20-bubbaloop-dash-design.md` in the [`bubbaloop`](https://github.com/kornia/bubbaloop) repo for the full design.

**Status:** Phase 1.0 (skeleton) — backend + dashboard scaffolding. Video upload + indexing arrive in Phase 1.1+.
```

- [ ] **Step 7: Commit**

```bash
cd /home/nvidia/bubbaloop-dash
git add .
git commit -m "chore: initial repository scaffold

Bare repo skeleton: .gitignore, .python-version, .editorconfig, README stub,
and a .source-sha pinning the bubbaloop repo SHA this work was started from."
```

Expected: one commit landed on `main`.

---

## Task 2: Backend package layout + pyproject.toml

**Files:**
- Create: `/home/nvidia/bubbaloop-dash/backend/pyproject.toml`
- Create: `/home/nvidia/bubbaloop-dash/backend/README.md`
- Create: `/home/nvidia/bubbaloop-dash/backend/src/dash/__init__.py`
- Create: `/home/nvidia/bubbaloop-dash/backend/tests/__init__.py`

- [ ] **Step 1: Create the backend directory tree**

```bash
cd /home/nvidia/bubbaloop-dash
mkdir -p backend/src/dash/api backend/src/dash/zenoh_proxy backend/tests
touch backend/src/dash/__init__.py
touch backend/src/dash/api/__init__.py
touch backend/src/dash/zenoh_proxy/__init__.py
touch backend/tests/__init__.py
```

- [ ] **Step 2: Create `backend/pyproject.toml`**

File contents (`/home/nvidia/bubbaloop-dash/backend/pyproject.toml`):

```toml
[project]
name = "bubbaloop-dash"
version = "0.1.0a0"
description = "Self-hosted backend + dashboard for Bubbaloop"
readme = "README.md"
requires-python = ">=3.11,<3.13"
license = { text = "Apache-2.0" }
authors = [{ name = "Edgar Riba", email = "edgar.riba@gmail.com" }]

dependencies = [
    "fastapi>=0.115,<0.120",
    "uvicorn[standard]>=0.32,<0.40",
    "pydantic>=2.9,<3.0",
    "pydantic-settings>=2.6,<3.0",
    "structlog>=24.4,<26.0",
    "eclipse-zenoh>=1.0,<2.0",
    "websockets>=13.0,<16.0",
]

[project.optional-dependencies]
dev = [
    "pytest>=8.3,<9.0",
    "pytest-asyncio>=0.24,<1.0",
    "httpx>=0.27,<0.30",
    "ruff>=0.7,<1.0",
    "mypy>=1.13,<2.0",
]

[build-system]
requires = ["hatchling"]
build-backend = "hatchling.build"

[tool.hatch.build.targets.wheel]
packages = ["src/dash"]

[tool.pytest.ini_options]
testpaths = ["tests"]
asyncio_mode = "auto"
markers = [
    "e2e: end-to-end tests requiring real Docker/zenohd (deselect with '-m \"not e2e\"')",
]

[tool.ruff]
line-length = 100
target-version = "py311"

[tool.ruff.lint]
select = ["E", "F", "I", "B", "UP", "RUF"]
ignore = ["E501"]

[tool.mypy]
python_version = "3.11"
strict = true
files = ["src/dash"]
```

- [ ] **Step 3: Create `backend/README.md`**

File contents (`/home/nvidia/bubbaloop-dash/backend/README.md`):

```markdown
# bubbaloop-dash backend

FastAPI + zenoh-python service. See repo root `README.md` and the design spec in `bubbaloop/docs/superpowers/specs/`.

## Local dev

```bash
cd backend
uv venv
source .venv/bin/activate
uv pip install -e ".[dev]"
uvicorn dash.app:app --reload --host 127.0.0.1 --port 8000
```

## Tests

```bash
pytest -m "not e2e"
```
```

- [ ] **Step 4: Install dependencies with uv**

```bash
cd /home/nvidia/bubbaloop-dash/backend
uv venv
source .venv/bin/activate
uv pip install -e ".[dev]"
```

Expected: `.venv/` created, dependencies installed without errors. The command emits "Installed N packages in Xs".

- [ ] **Step 5: Verify the install works**

```bash
cd /home/nvidia/bubbaloop-dash/backend
source .venv/bin/activate
python -c "import dash; print(dash.__file__)"
```

Expected: prints path ending in `/backend/src/dash/__init__.py`.

- [ ] **Step 6: Commit**

```bash
cd /home/nvidia/bubbaloop-dash
git add backend/pyproject.toml backend/README.md backend/src/dash/__init__.py backend/src/dash/api/__init__.py backend/src/dash/zenoh_proxy/__init__.py backend/tests/__init__.py
git commit -m "chore(backend): pyproject + package skeleton

Adds backend/ Python package with FastAPI, zenoh-python, pydantic-settings,
structlog dependencies. uv lockfile follows in the next task once first
imports exist (uv lock --check is a release-gating concern, not a Phase 1.0 one)."
```

---

## Task 3: Settings (pydantic-settings)

**Files:**
- Create: `/home/nvidia/bubbaloop-dash/backend/src/dash/config.py`
- Create: `/home/nvidia/bubbaloop-dash/backend/tests/test_config.py`

- [ ] **Step 1: Write the failing test**

File contents (`/home/nvidia/bubbaloop-dash/backend/tests/test_config.py`):

```python
"""Settings parse env vars correctly and apply sensible defaults."""
from pathlib import Path

import pytest

from dash.config import Settings


def test_settings_defaults(tmp_path: Path, monkeypatch: pytest.MonkeyPatch) -> None:
    monkeypatch.setenv("DATA_DIR", str(tmp_path))
    monkeypatch.delenv("DASHBOARD_TOKEN", raising=False)
    s = Settings()
    assert s.data_dir == tmp_path
    assert s.dashboard_token is None
    assert s.port == 8000
    assert s.zenoh_endpoint == "tcp/127.0.0.1:7447"


def test_settings_reads_env(tmp_path: Path, monkeypatch: pytest.MonkeyPatch) -> None:
    monkeypatch.setenv("DATA_DIR", str(tmp_path))
    monkeypatch.setenv("DASHBOARD_TOKEN", "secret-xyz")
    monkeypatch.setenv("PORT", "9000")
    monkeypatch.setenv("ZENOH_ENDPOINT", "tcp/jetson.tail:7447")
    s = Settings()
    assert s.dashboard_token == "secret-xyz"
    assert s.port == 9000
    assert s.zenoh_endpoint == "tcp/jetson.tail:7447"
```

- [ ] **Step 2: Run the test and watch it fail**

```bash
cd /home/nvidia/bubbaloop-dash/backend
source .venv/bin/activate
pytest tests/test_config.py -v
```

Expected: `ModuleNotFoundError: No module named 'dash.config'`.

- [ ] **Step 3: Implement `Settings`**

File contents (`/home/nvidia/bubbaloop-dash/backend/src/dash/config.py`):

```python
"""Application settings sourced from environment variables.

All knobs default to values suitable for a local dev run; production deployments
override via env (typically through Docker/compose).
"""
from pathlib import Path

from pydantic import Field
from pydantic_settings import BaseSettings, SettingsConfigDict


class Settings(BaseSettings):
    """Process-wide configuration. Read once at app startup."""

    model_config = SettingsConfigDict(
        env_file=".env",
        env_file_encoding="utf-8",
        case_sensitive=False,
        extra="ignore",
    )

    data_dir: Path = Field(default=Path("/var/lib/bubbaloop-dash"))
    dashboard_token: str | None = Field(default=None)
    port: int = Field(default=8000, ge=1, le=65535)
    zenoh_endpoint: str = Field(default="tcp/127.0.0.1:7447")
    sweep_interval_s: int = Field(default=3600, ge=60)
    job_concurrency: int = Field(default=1, ge=1)
```

- [ ] **Step 4: Run the test and watch it pass**

```bash
cd /home/nvidia/bubbaloop-dash/backend
pytest tests/test_config.py -v
```

Expected: 2 passed.

- [ ] **Step 5: Commit**

```bash
cd /home/nvidia/bubbaloop-dash
git add backend/src/dash/config.py backend/tests/test_config.py
git commit -m "feat(backend): Settings (pydantic-settings)

Process-wide config sourced from env. Defaults are dev-friendly; production
overrides via Docker env. Knobs: DATA_DIR, DASHBOARD_TOKEN, PORT,
ZENOH_ENDPOINT, SWEEP_INTERVAL_S, JOB_CONCURRENCY."
```

---

## Task 4: Structured logging (structlog)

**Files:**
- Create: `/home/nvidia/bubbaloop-dash/backend/src/dash/logging.py`
- Create: `/home/nvidia/bubbaloop-dash/backend/tests/test_logging.py`

- [ ] **Step 1: Write the failing test**

File contents (`/home/nvidia/bubbaloop-dash/backend/tests/test_logging.py`):

```python
"""Structured logger configures itself once and produces JSON output."""
import json
import logging

import structlog

from dash.logging import configure_logging, get_logger


def test_configure_logging_emits_json(capsys) -> None:
    configure_logging(level="INFO")
    log = get_logger("test")
    log.info("hello", k="v")
    captured = capsys.readouterr()
    # structlog's JSONRenderer goes to stdout via logging module
    line = (captured.out or captured.err).strip().splitlines()[-1]
    payload = json.loads(line)
    assert payload["event"] == "hello"
    assert payload["k"] == "v"
    assert payload["level"] == "info"


def test_get_logger_returns_bound_logger() -> None:
    configure_logging(level="WARNING")
    log = get_logger("test")
    assert isinstance(log, structlog.stdlib.BoundLogger) or hasattr(log, "info")
```

- [ ] **Step 2: Run the test and watch it fail**

```bash
cd /home/nvidia/bubbaloop-dash/backend
pytest tests/test_logging.py -v
```

Expected: `ModuleNotFoundError: No module named 'dash.logging'`.

- [ ] **Step 3: Implement logging**

File contents (`/home/nvidia/bubbaloop-dash/backend/src/dash/logging.py`):

```python
"""Structured logging via structlog.

JSON output to stdout. Three levels in practice: INFO (lifecycle events,
search queries), WARN (transient errors, retries), ERROR (failures, integrity).
Configure once at app startup."""
import logging
import sys
from typing import Any

import structlog


def configure_logging(level: str = "INFO") -> None:
    """Idempotent: safe to call multiple times in tests."""
    logging.basicConfig(
        format="%(message)s",
        stream=sys.stdout,
        level=getattr(logging, level.upper()),
        force=True,
    )
    structlog.configure(
        processors=[
            structlog.contextvars.merge_contextvars,
            structlog.processors.add_log_level,
            structlog.processors.TimeStamper(fmt="iso", utc=True),
            structlog.processors.StackInfoRenderer(),
            structlog.processors.format_exc_info,
            structlog.processors.JSONRenderer(),
        ],
        wrapper_class=structlog.make_filtering_bound_logger(getattr(logging, level.upper())),
        logger_factory=structlog.stdlib.LoggerFactory(),
        cache_logger_on_first_use=True,
    )


def get_logger(name: str) -> Any:
    """Return a logger bound to `name`. Call `configure_logging` first."""
    return structlog.get_logger(name)
```

- [ ] **Step 4: Run the test and watch it pass**

```bash
cd /home/nvidia/bubbaloop-dash/backend
pytest tests/test_logging.py -v
```

Expected: 2 passed.

- [ ] **Step 5: Commit**

```bash
cd /home/nvidia/bubbaloop-dash
git add backend/src/dash/logging.py backend/tests/test_logging.py
git commit -m "feat(backend): structlog JSON logging

Configure once at app startup; INFO/WARN/ERROR to stdout as JSON.
Docker log driver picks them up unchanged."
```

---

## Task 5: FastAPI app factory

**Files:**
- Create: `/home/nvidia/bubbaloop-dash/backend/src/dash/app.py`
- Create: `/home/nvidia/bubbaloop-dash/backend/tests/conftest.py`

- [ ] **Step 1: Create `tests/conftest.py`**

File contents (`/home/nvidia/bubbaloop-dash/backend/tests/conftest.py`):

```python
"""Shared test fixtures."""
from pathlib import Path

import pytest
from fastapi.testclient import TestClient

from dash.app import create_app
from dash.config import Settings


@pytest.fixture
def settings(tmp_path: Path, monkeypatch: pytest.MonkeyPatch) -> Settings:
    monkeypatch.setenv("DATA_DIR", str(tmp_path))
    monkeypatch.setenv("DASHBOARD_TOKEN", "test-token-secret")
    return Settings()


@pytest.fixture
def client(settings: Settings) -> TestClient:
    app = create_app(settings)
    return TestClient(app)
```

- [ ] **Step 2: Write the failing test (rolled into Task 6)**

This task just creates the empty shell — its only test is "the app object imports and instantiates." We'll write that as a single assertion now, then add real route tests in Task 6.

File contents (append to `/home/nvidia/bubbaloop-dash/backend/tests/conftest.py` is already enough — the `client` fixture instantiation will fail if the app doesn't exist).

Quick scaffold test (`/home/nvidia/bubbaloop-dash/backend/tests/test_app.py`):

```python
"""The app factory instantiates."""
from dash.app import create_app
from dash.config import Settings


def test_create_app(tmp_path, monkeypatch) -> None:
    monkeypatch.setenv("DATA_DIR", str(tmp_path))
    s = Settings()
    app = create_app(s)
    assert app.title == "bubbaloop-dash"
```

- [ ] **Step 3: Run the test and watch it fail**

```bash
cd /home/nvidia/bubbaloop-dash/backend
pytest tests/test_app.py -v
```

Expected: `ImportError: cannot import name 'create_app' from 'dash.app'`.

- [ ] **Step 4: Implement the app factory**

File contents (`/home/nvidia/bubbaloop-dash/backend/src/dash/app.py`):

```python
"""FastAPI application factory.

Routers are mounted in `create_app` so tests can construct fresh apps with
overridden settings. Lifespan is wired in later tasks once Zenoh session
management exists."""
from fastapi import FastAPI

from dash.config import Settings
from dash.logging import configure_logging


def create_app(settings: Settings) -> FastAPI:
    configure_logging(level="INFO")
    app = FastAPI(
        title="bubbaloop-dash",
        version="0.1.0a0",
        description="Self-hosted backend + dashboard for Bubbaloop",
    )
    app.state.settings = settings
    return app


# Module-level app for `uvicorn dash.app:app` direct invocation.
app = create_app(Settings())
```

- [ ] **Step 5: Run the test and watch it pass**

```bash
cd /home/nvidia/bubbaloop-dash/backend
pytest tests/test_app.py tests/test_config.py tests/test_logging.py -v
```

Expected: 5 passed total.

- [ ] **Step 6: Commit**

```bash
cd /home/nvidia/bubbaloop-dash
git add backend/src/dash/app.py backend/tests/conftest.py backend/tests/test_app.py
git commit -m "feat(backend): FastAPI app factory + shared test fixtures

create_app(settings) so tests can inject overrides. Module-level `app`
preserves \`uvicorn dash.app:app\` direct-invocation ergonomics."
```

---

## Task 6: /healthz and /readyz endpoints

**Files:**
- Create: `/home/nvidia/bubbaloop-dash/backend/src/dash/api/health.py`
- Create: `/home/nvidia/bubbaloop-dash/backend/tests/test_health.py`
- Modify: `/home/nvidia/bubbaloop-dash/backend/src/dash/app.py` (mount router)

- [ ] **Step 1: Write the failing test**

File contents (`/home/nvidia/bubbaloop-dash/backend/tests/test_health.py`):

```python
"""/healthz is unauthenticated liveness; /readyz reports zenoh + lancedb state."""
from fastapi.testclient import TestClient


def test_healthz_returns_200_without_auth(client: TestClient) -> None:
    r = client.get("/healthz")
    assert r.status_code == 200
    assert r.json() == {"status": "ok"}


def test_readyz_returns_503_when_not_ready(client: TestClient) -> None:
    r = client.get("/readyz")
    # Phase 1.0 stubs both probes as not-yet-implemented → not_ready.
    assert r.status_code == 503
    body = r.json()
    assert body["status"] == "not_ready"
    assert "zenoh" in body["checks"]
    assert "lancedb" in body["checks"]
```

- [ ] **Step 2: Run the test and watch it fail**

```bash
cd /home/nvidia/bubbaloop-dash/backend
pytest tests/test_health.py -v
```

Expected: 2 failed (404 instead of 200/503).

- [ ] **Step 3: Implement the health router**

File contents (`/home/nvidia/bubbaloop-dash/backend/src/dash/api/health.py`):

```python
"""Liveness and readiness endpoints.

/healthz: cheap process-alive check. Never blocks. No auth.
/readyz: aggregates internal readiness probes (Zenoh client up, LanceDB open).
         Returns 503 if any probe reports not-ready. No auth (probes are not
         secret and exposing them simplifies orchestrator integration)."""
from fastapi import APIRouter, Request, Response

router = APIRouter()


@router.get("/healthz")
async def healthz() -> dict[str, str]:
    return {"status": "ok"}


@router.get("/readyz")
async def readyz(request: Request, response: Response) -> dict[str, object]:
    # Each probe is added in a later task (Task 11 wires zenoh).
    # For Phase 1.0 we stub them as not-ready so the endpoint is observably alive.
    checks: dict[str, str] = {
        "zenoh": getattr(request.app.state, "zenoh_status", "not_implemented"),
        "lancedb": getattr(request.app.state, "lancedb_status", "not_implemented"),
    }
    all_ok = all(v == "ok" for v in checks.values())
    if not all_ok:
        response.status_code = 503
    return {"status": "ok" if all_ok else "not_ready", "checks": checks}
```

- [ ] **Step 4: Mount the router in the app factory**

Edit `/home/nvidia/bubbaloop-dash/backend/src/dash/app.py`:

Replace the entire file with:

```python
"""FastAPI application factory."""
from fastapi import FastAPI

from dash.api import health
from dash.config import Settings
from dash.logging import configure_logging


def create_app(settings: Settings) -> FastAPI:
    configure_logging(level="INFO")
    app = FastAPI(
        title="bubbaloop-dash",
        version="0.1.0a0",
        description="Self-hosted backend + dashboard for Bubbaloop",
    )
    app.state.settings = settings
    app.include_router(health.router)
    return app


app = create_app(Settings())
```

- [ ] **Step 5: Run the test and watch it pass**

```bash
cd /home/nvidia/bubbaloop-dash/backend
pytest tests/test_health.py -v
```

Expected: 2 passed.

- [ ] **Step 6: Commit**

```bash
cd /home/nvidia/bubbaloop-dash
git add backend/src/dash/api/health.py backend/src/dash/app.py backend/tests/test_health.py
git commit -m "feat(backend): /healthz and /readyz

/healthz is liveness (always 200). /readyz aggregates probes (zenoh + lancedb)
and returns 503 until both report ok. Probes stub to 'not_implemented' for now;
Task 11 wires the real zenoh probe."
```

---

## Task 7: Bearer-token authentication dependency

**Files:**
- Create: `/home/nvidia/bubbaloop-dash/backend/src/dash/auth.py`
- Create: `/home/nvidia/bubbaloop-dash/backend/tests/test_auth.py`

- [ ] **Step 1: Write the failing tests**

File contents (`/home/nvidia/bubbaloop-dash/backend/tests/test_auth.py`):

```python
"""Bearer token: missing → 401, wrong → 401, correct → passes through."""
from fastapi import Depends, FastAPI
from fastapi.testclient import TestClient

from dash.auth import require_bearer_token
from dash.config import Settings


def _app_with_protected_route(settings: Settings) -> FastAPI:
    app = FastAPI()
    app.state.settings = settings

    @app.get("/protected")
    def protected(_: None = Depends(require_bearer_token)) -> dict[str, str]:
        return {"ok": "yes"}

    return app


def test_missing_token_returns_401(settings: Settings) -> None:
    app = _app_with_protected_route(settings)
    c = TestClient(app)
    r = c.get("/protected")
    assert r.status_code == 401
    assert r.headers.get("www-authenticate") == "Bearer"


def test_wrong_token_returns_401(settings: Settings) -> None:
    app = _app_with_protected_route(settings)
    c = TestClient(app)
    r = c.get("/protected", headers={"Authorization": "Bearer wrong"})
    assert r.status_code == 401


def test_correct_token_passes(settings: Settings) -> None:
    app = _app_with_protected_route(settings)
    c = TestClient(app)
    r = c.get("/protected", headers={"Authorization": f"Bearer {settings.dashboard_token}"})
    assert r.status_code == 200
    assert r.json() == {"ok": "yes"}


def test_malformed_authorization_returns_401(settings: Settings) -> None:
    app = _app_with_protected_route(settings)
    c = TestClient(app)
    r = c.get("/protected", headers={"Authorization": "Token foo"})
    assert r.status_code == 401
```

- [ ] **Step 2: Run the tests and watch them fail**

```bash
cd /home/nvidia/bubbaloop-dash/backend
pytest tests/test_auth.py -v
```

Expected: 4 errors (`ImportError: cannot import name 'require_bearer_token' from 'dash.auth'`).

- [ ] **Step 3: Implement bearer-token dependency**

File contents (`/home/nvidia/bubbaloop-dash/backend/src/dash/auth.py`):

```python
"""Bearer-token authentication.

v1 is single-tenant: one token, sourced from the DASHBOARD_TOKEN env var (or
the .token file written at first run, see dash.tokens). Future multi-token /
multi-user support replaces this dependency; the call sites stay identical."""
from fastapi import HTTPException, Request, status
from fastapi.security import HTTPAuthorizationCredentials, HTTPBearer

_bearer = HTTPBearer(auto_error=False)


async def require_bearer_token(request: Request) -> None:
    """Reject requests without a valid bearer token. Use as `Depends(...)`."""
    credentials: HTTPAuthorizationCredentials | None = await _bearer(request)
    expected: str | None = request.app.state.settings.dashboard_token
    if expected is None:
        raise HTTPException(
            status_code=status.HTTP_500_INTERNAL_SERVER_ERROR,
            detail="DASHBOARD_TOKEN is not configured on the server",
        )
    if credentials is None or credentials.credentials != expected:
        raise HTTPException(
            status_code=status.HTTP_401_UNAUTHORIZED,
            detail="invalid_or_missing_bearer_token",
            headers={"WWW-Authenticate": "Bearer"},
        )
```

- [ ] **Step 4: Run the tests and watch them pass**

```bash
cd /home/nvidia/bubbaloop-dash/backend
pytest tests/test_auth.py -v
```

Expected: 4 passed.

- [ ] **Step 5: Commit**

```bash
cd /home/nvidia/bubbaloop-dash
git add backend/src/dash/auth.py backend/tests/test_auth.py
git commit -m "feat(backend): bearer-token auth dependency

require_bearer_token() raises 401 on missing/wrong token with
WWW-Authenticate: Bearer header. v1 single-tenant: token from
Settings.dashboard_token. Future multi-tenant replaces this dependency;
call sites unchanged."
```

---

## Task 8: First-run token generator

**Files:**
- Create: `/home/nvidia/bubbaloop-dash/backend/src/dash/tokens.py`
- Create: `/home/nvidia/bubbaloop-dash/backend/tests/test_tokens.py`
- Modify: `/home/nvidia/bubbaloop-dash/backend/src/dash/app.py` (call on startup)

- [ ] **Step 1: Write the failing tests**

File contents (`/home/nvidia/bubbaloop-dash/backend/tests/test_tokens.py`):

```python
"""First-run token generation: if DASHBOARD_TOKEN unset and .token file absent,
generate one, persist it, log it. Subsequent runs reuse the persisted token."""
from pathlib import Path

import pytest

from dash.config import Settings
from dash.tokens import ensure_token


def test_generates_and_persists_when_unset(tmp_path: Path, monkeypatch: pytest.MonkeyPatch) -> None:
    monkeypatch.setenv("DATA_DIR", str(tmp_path))
    monkeypatch.delenv("DASHBOARD_TOKEN", raising=False)
    s = Settings()

    token = ensure_token(s)

    assert token is not None
    assert len(token) >= 32
    assert (tmp_path / ".token").read_text().strip() == token
    assert s.dashboard_token == token


def test_reuses_persisted_token_on_second_call(tmp_path: Path, monkeypatch: pytest.MonkeyPatch) -> None:
    monkeypatch.setenv("DATA_DIR", str(tmp_path))
    monkeypatch.delenv("DASHBOARD_TOKEN", raising=False)
    s1 = Settings()
    token1 = ensure_token(s1)

    s2 = Settings()
    token2 = ensure_token(s2)

    assert token1 == token2


def test_env_var_takes_priority(tmp_path: Path, monkeypatch: pytest.MonkeyPatch) -> None:
    monkeypatch.setenv("DATA_DIR", str(tmp_path))
    monkeypatch.setenv("DASHBOARD_TOKEN", "env-wins")
    (tmp_path / ".token").write_text("file-loses")
    s = Settings()

    token = ensure_token(s)

    assert token == "env-wins"
    # Persistent file must NOT be overwritten when env var wins.
    assert (tmp_path / ".token").read_text() == "file-loses"
```

- [ ] **Step 2: Run the tests and watch them fail**

```bash
cd /home/nvidia/bubbaloop-dash/backend
pytest tests/test_tokens.py -v
```

Expected: errors (`ImportError: cannot import name 'ensure_token'`).

- [ ] **Step 3: Implement the token generator**

File contents (`/home/nvidia/bubbaloop-dash/backend/src/dash/tokens.py`):

```python
"""First-run dashboard-token handling.

Priority on startup:
  1. DASHBOARD_TOKEN env var (set by Settings)
  2. ${DATA_DIR}/.token file (persisted from a previous first run)
  3. Generate a fresh 32-byte URL-safe token, persist to .token, log it.

Stamps the resolved token onto Settings.dashboard_token so the auth
dependency sees it on subsequent requests. Run once at app startup."""
import secrets
from pathlib import Path

from dash.config import Settings
from dash.logging import get_logger

_log = get_logger("dash.tokens")


def ensure_token(settings: Settings) -> str:
    """Resolve or generate the dashboard token. Returns the active token."""
    if settings.dashboard_token:
        return settings.dashboard_token

    token_path: Path = settings.data_dir / ".token"
    settings.data_dir.mkdir(parents=True, exist_ok=True)

    if token_path.exists():
        token = token_path.read_text().strip()
        settings.dashboard_token = token
        _log.info("token.loaded_from_file", path=str(token_path))
        return token

    token = secrets.token_urlsafe(32)
    token_path.write_text(token)
    token_path.chmod(0o600)
    settings.dashboard_token = token
    _log.warning(
        "token.generated",
        path=str(token_path),
        message="First-run token generated. Copy this for browser login.",
        token=token,
    )
    return token
```

- [ ] **Step 4: Wire `ensure_token` into the app factory**

Edit `/home/nvidia/bubbaloop-dash/backend/src/dash/app.py` — replace entirely:

```python
"""FastAPI application factory."""
from fastapi import FastAPI

from dash.api import health
from dash.config import Settings
from dash.logging import configure_logging
from dash.tokens import ensure_token


def create_app(settings: Settings) -> FastAPI:
    configure_logging(level="INFO")
    ensure_token(settings)
    app = FastAPI(
        title="bubbaloop-dash",
        version="0.1.0a0",
        description="Self-hosted backend + dashboard for Bubbaloop",
    )
    app.state.settings = settings
    app.include_router(health.router)
    return app


app = create_app(Settings())
```

- [ ] **Step 5: Run all backend tests**

```bash
cd /home/nvidia/bubbaloop-dash/backend
pytest -v
```

Expected: all tests pass (3 tokens + others from earlier tasks).

- [ ] **Step 6: Commit**

```bash
cd /home/nvidia/bubbaloop-dash
git add backend/src/dash/tokens.py backend/src/dash/app.py backend/tests/test_tokens.py
git commit -m "feat(backend): first-run token generation

If DASHBOARD_TOKEN unset and .token absent, generate a 32-byte URL-safe
token, persist to \${DATA_DIR}/.token (0600), and log it once at WARN
level so operators can copy it. Env var > file > generate."
```

---

## Task 9: Zenoh client lifecycle (lifespan)

**Files:**
- Create: `/home/nvidia/bubbaloop-dash/backend/src/dash/zenoh_proxy/session.py`
- Create: `/home/nvidia/bubbaloop-dash/backend/tests/test_zenoh_session.py`
- Modify: `/home/nvidia/bubbaloop-dash/backend/src/dash/app.py` (lifespan + readyz wire-up)

- [ ] **Step 1: Write the failing test**

File contents (`/home/nvidia/bubbaloop-dash/backend/tests/test_zenoh_session.py`):

```python
"""Zenoh session opens on startup, closes cleanly. Failure to connect must NOT
crash the app — readyz reports not_ready and a background reconnect loop runs."""
from pathlib import Path

import pytest
import zenoh

from dash.zenoh_proxy.session import ZenohSession


@pytest.mark.asyncio
async def test_open_close_with_local_router(tmp_path: Path) -> None:
    """Open against a transient in-process router-less peer; verify clean close."""
    sess = ZenohSession(endpoint="tcp/127.0.0.1:17447")
    # No router is listening; expect a connect failure but no exception bubbling up.
    await sess.start()
    # Should NOT raise; status is "not_ready" until a router appears.
    assert sess.status() in ("connecting", "not_ready")
    await sess.stop()
    assert sess.status() == "stopped"


@pytest.mark.asyncio
async def test_inner_session_is_zenoh_session_when_connected(tmp_path: Path) -> None:
    """When connected, .session() returns a real zenoh.Session. Skipped without a router."""
    # Try to open a brief in-process session against the loopback peer.
    # If no router is available, status stays connecting and this assertion is skipped.
    sess = ZenohSession(endpoint="tcp/127.0.0.1:7447")
    await sess.start()
    if sess.status() != "ok":
        pytest.skip("no local zenohd on 7447 — skipping live-connection assertion")
    inner = sess.session()
    assert isinstance(inner, zenoh.Session)
    await sess.stop()
```

- [ ] **Step 2: Run the test and watch it fail**

```bash
cd /home/nvidia/bubbaloop-dash/backend
pytest tests/test_zenoh_session.py -v
```

Expected: `ImportError: cannot import name 'ZenohSession' from 'dash.zenoh_proxy.session'`.

- [ ] **Step 3: Implement `ZenohSession`**

File contents (`/home/nvidia/bubbaloop-dash/backend/src/dash/zenoh_proxy/session.py`):

```python
"""Long-lived zenoh client connection to the user's zenohd.

Opens in `start()`, closes in `stop()`. A background reconnect loop runs while
the session is alive — if the upstream router goes away, status() degrades to
"not_ready" until reconnect succeeds. Never raises on connect failure; instead
records the failure and retries with exponential backoff."""
import asyncio
from typing import Literal

import zenoh

from dash.logging import get_logger

_log = get_logger("dash.zenoh_proxy.session")

Status = Literal["stopped", "connecting", "ok", "not_ready"]

_RECONNECT_BACKOFF_S = [1, 2, 4, 8, 16, 30]  # max 30s between attempts


class ZenohSession:
    """Wraps a `zenoh.Session` with reconnect + lifecycle."""

    def __init__(self, endpoint: str) -> None:
        self._endpoint = endpoint
        self._status: Status = "stopped"
        self._session: zenoh.Session | None = None
        self._task: asyncio.Task[None] | None = None
        self._stop_event = asyncio.Event()

    def status(self) -> Status:
        return self._status

    def session(self) -> zenoh.Session:
        if self._session is None or self._status != "ok":
            raise RuntimeError(f"zenoh session not ready (status={self._status})")
        return self._session

    async def start(self) -> None:
        self._stop_event.clear()
        self._status = "connecting"
        self._task = asyncio.create_task(self._run(), name="zenoh-session-loop")
        # Give the first connect attempt a brief window to settle so the status
        # reflects reality by the time start() returns.
        await asyncio.sleep(0.1)

    async def stop(self) -> None:
        self._stop_event.set()
        if self._task is not None:
            await self._task
            self._task = None
        if self._session is not None:
            self._session.close()
            self._session = None
        self._status = "stopped"

    async def _run(self) -> None:
        backoff_idx = 0
        while not self._stop_event.is_set():
            try:
                cfg = zenoh.Config()
                cfg.insert_json5("mode", '"client"')
                cfg.insert_json5("connect/endpoints", f'["{self._endpoint}"]')
                self._session = zenoh.open(cfg)
                self._status = "ok"
                _log.info("zenoh.connected", endpoint=self._endpoint)
                backoff_idx = 0
                # Hold here until stop() flips the event.
                await self._stop_event.wait()
                return
            except Exception as exc:
                self._status = "not_ready"
                wait = _RECONNECT_BACKOFF_S[min(backoff_idx, len(_RECONNECT_BACKOFF_S) - 1)]
                _log.warning(
                    "zenoh.connect_failed",
                    endpoint=self._endpoint,
                    error=str(exc),
                    retry_in_s=wait,
                )
                backoff_idx += 1
                try:
                    await asyncio.wait_for(self._stop_event.wait(), timeout=wait)
                    return
                except TimeoutError:
                    continue
```

- [ ] **Step 4: Wire lifespan into the app factory + update readyz**

Edit `/home/nvidia/bubbaloop-dash/backend/src/dash/app.py` — replace entirely:

```python
"""FastAPI application factory."""
from contextlib import asynccontextmanager
from typing import AsyncIterator

from fastapi import FastAPI

from dash.api import health
from dash.config import Settings
from dash.logging import configure_logging
from dash.tokens import ensure_token
from dash.zenoh_proxy.session import ZenohSession


def create_app(settings: Settings) -> FastAPI:
    configure_logging(level="INFO")
    ensure_token(settings)

    @asynccontextmanager
    async def lifespan(app: FastAPI) -> AsyncIterator[None]:
        zsess = ZenohSession(endpoint=settings.zenoh_endpoint)
        await zsess.start()
        app.state.zenoh_session = zsess
        try:
            yield
        finally:
            await zsess.stop()

    app = FastAPI(
        title="bubbaloop-dash",
        version="0.1.0a0",
        description="Self-hosted backend + dashboard for Bubbaloop",
        lifespan=lifespan,
    )
    app.state.settings = settings
    app.include_router(health.router)
    return app


app = create_app(Settings())
```

Edit `/home/nvidia/bubbaloop-dash/backend/src/dash/api/health.py` — replace the `readyz` function so it queries the live session:

```python
@router.get("/readyz")
async def readyz(request: Request, response: Response) -> dict[str, object]:
    zsess = getattr(request.app.state, "zenoh_session", None)
    zenoh_status = zsess.status() if zsess is not None else "not_implemented"
    lance_status = "not_implemented"  # wired in Phase 1.2
    checks: dict[str, str] = {"zenoh": zenoh_status, "lancedb": lance_status}
    all_ok = all(v == "ok" for v in checks.values())
    if not all_ok:
        response.status_code = 503
    return {"status": "ok" if all_ok else "not_ready", "checks": checks}
```

- [ ] **Step 5: Update `tests/test_health.py` for the new readyz semantics**

The earlier readyz test expects `not_implemented`. Now the session reports either `connecting` or `not_ready`. Replace the body of `test_readyz_returns_503_when_not_ready`:

```python
def test_readyz_returns_503_when_not_ready(client: TestClient) -> None:
    r = client.get("/readyz")
    assert r.status_code == 503
    body = r.json()
    assert body["status"] == "not_ready"
    assert body["checks"]["zenoh"] in ("connecting", "not_ready", "stopped")
    assert body["checks"]["lancedb"] == "not_implemented"
```

- [ ] **Step 6: Run the tests**

```bash
cd /home/nvidia/bubbaloop-dash/backend
pytest tests/test_zenoh_session.py tests/test_health.py -v
```

Expected: all pass; the live-connection test in test_zenoh_session is skipped if no `zenohd` listens on `:7447`.

- [ ] **Step 7: Commit**

```bash
cd /home/nvidia/bubbaloop-dash
git add backend/src/dash/zenoh_proxy/session.py backend/src/dash/app.py backend/src/dash/api/health.py backend/tests/test_zenoh_session.py backend/tests/test_health.py
git commit -m "feat(backend): zenoh session lifecycle + readyz wire-up

ZenohSession opens at app startup, closes at shutdown. Background reconnect
loop with exponential backoff (1-30s). Never raises on connect failure —
status() reports 'connecting' or 'not_ready' until the router appears.
/readyz returns 503 unless both zenoh and lancedb probes are ok."
```

---

## Task 10: WS protocol parsing (subscribe / unsubscribe / sample)

**Files:**
- Create: `/home/nvidia/bubbaloop-dash/backend/src/dash/zenoh_proxy/protocol.py`
- Create: `/home/nvidia/bubbaloop-dash/backend/tests/test_zenoh_protocol.py`

The browser-side `@eclipse-zenoh/zenoh-ts` library speaks the `zenoh-bridge-remote-api` wire protocol. We implement the minimal subset needed for Dashboard's existing views: `subscribe`, `unsubscribe`, `sample` (incoming). Other message types (put, get, queryable) are NOT in Phase 1.0 and will return an explicit error.

- [ ] **Step 1: Write the failing tests**

File contents (`/home/nvidia/bubbaloop-dash/backend/tests/test_zenoh_protocol.py`):

```python
"""Parse incoming WS messages; serialize outgoing sample frames."""
from dash.zenoh_proxy.protocol import (
    SampleFrame,
    SubscribeMessage,
    UnsubscribeMessage,
    UnsupportedMessage,
    parse_client_message,
    serialize_sample,
)


def test_parse_subscribe() -> None:
    msg = parse_client_message({
        "type": "subscribe",
        "id": "sub-1",
        "key_expr": "bubbaloop/global/host1/**",
    })
    assert isinstance(msg, SubscribeMessage)
    assert msg.id == "sub-1"
    assert msg.key_expr == "bubbaloop/global/host1/**"


def test_parse_unsubscribe() -> None:
    msg = parse_client_message({"type": "unsubscribe", "id": "sub-1"})
    assert isinstance(msg, UnsubscribeMessage)
    assert msg.id == "sub-1"


def test_parse_unsupported_returns_typed_error() -> None:
    msg = parse_client_message({"type": "put", "id": "x", "key_expr": "k", "payload": "v"})
    assert isinstance(msg, UnsupportedMessage)
    assert msg.kind == "put"


def test_parse_malformed_returns_unsupported() -> None:
    msg = parse_client_message({"no": "type"})
    assert isinstance(msg, UnsupportedMessage)


def test_serialize_sample_returns_dict() -> None:
    frame = serialize_sample(
        sub_id="sub-1",
        key_expr="bubbaloop/global/host1/camera/frame",
        payload=b"\x00\x01\x02",
        encoding="application/octet-stream",
    )
    assert frame == {
        "type": "sample",
        "id": "sub-1",
        "key_expr": "bubbaloop/global/host1/camera/frame",
        "payload": "AAEC",  # base64 of \x00\x01\x02
        "encoding": "application/octet-stream",
    }
```

- [ ] **Step 2: Run the tests and watch them fail**

```bash
cd /home/nvidia/bubbaloop-dash/backend
pytest tests/test_zenoh_protocol.py -v
```

Expected: import errors.

- [ ] **Step 3: Implement the protocol module**

File contents (`/home/nvidia/bubbaloop-dash/backend/src/dash/zenoh_proxy/protocol.py`):

```python
"""WS message types and (de)serialization for the multiplexed Zenoh proxy.

Minimal subset of zenoh-bridge-remote-api: subscribe, unsubscribe, sample.
Put/Get/Queryable are NOT supported in Phase 1.0; clients sending them get
an UnsupportedMessage back so the WS handler can reply with an explicit
error frame instead of silently dropping."""
import base64
from dataclasses import dataclass
from typing import Any


@dataclass(frozen=True)
class SubscribeMessage:
    id: str
    key_expr: str


@dataclass(frozen=True)
class UnsubscribeMessage:
    id: str


@dataclass(frozen=True)
class UnsupportedMessage:
    kind: str


ClientMessage = SubscribeMessage | UnsubscribeMessage | UnsupportedMessage


def parse_client_message(payload: dict[str, Any]) -> ClientMessage:
    kind = payload.get("type")
    if kind == "subscribe":
        return SubscribeMessage(id=str(payload["id"]), key_expr=str(payload["key_expr"]))
    if kind == "unsubscribe":
        return UnsubscribeMessage(id=str(payload["id"]))
    return UnsupportedMessage(kind=str(kind) if kind is not None else "unknown")


@dataclass(frozen=True)
class SampleFrame:
    sub_id: str
    key_expr: str
    payload: bytes
    encoding: str


def serialize_sample(sub_id: str, key_expr: str, payload: bytes, encoding: str) -> dict[str, Any]:
    return {
        "type": "sample",
        "id": sub_id,
        "key_expr": key_expr,
        "payload": base64.b64encode(payload).decode("ascii"),
        "encoding": encoding,
    }
```

- [ ] **Step 4: Run the tests and watch them pass**

```bash
cd /home/nvidia/bubbaloop-dash/backend
pytest tests/test_zenoh_protocol.py -v
```

Expected: 5 passed.

- [ ] **Step 5: Commit**

```bash
cd /home/nvidia/bubbaloop-dash
git add backend/src/dash/zenoh_proxy/protocol.py backend/tests/test_zenoh_protocol.py
git commit -m "feat(backend): WS protocol parser for Zenoh proxy

Minimal subset of zenoh-bridge-remote-api: subscribe, unsubscribe, sample.
Unsupported message types (put/get/queryable) produce typed
UnsupportedMessage results so the WS handler can reply with an explicit
error rather than silently dropping."
```

---

## Task 11: Multiplexed subscription tracker

**Files:**
- Create: `/home/nvidia/bubbaloop-dash/backend/src/dash/zenoh_proxy/multiplex.py`
- Create: `/home/nvidia/bubbaloop-dash/backend/tests/test_zenoh_multiplex.py`

The multiplex tracker is the heart of the proxy. It ref-counts upstream subscriptions so N browser tabs subscribing to the same key_expr share one Zenoh subscription on the upstream session.

- [ ] **Step 1: Write the failing tests**

File contents (`/home/nvidia/bubbaloop-dash/backend/tests/test_zenoh_multiplex.py`):

```python
"""Ref-counted subscription manager.

We don't open real Zenoh subs in unit tests; tests inject a fake `Subscriber`
factory and verify lifecycle correctness."""
from dataclasses import dataclass, field

import pytest

from dash.zenoh_proxy.multiplex import SubscriptionRegistry


@dataclass
class FakeSub:
    key_expr: str
    closed: bool = False

    def undeclare(self) -> None:
        self.closed = True


@dataclass
class FakeSession:
    declared: list[FakeSub] = field(default_factory=list)

    def declare_subscriber(self, key_expr: str, handler):  # noqa: ANN001
        sub = FakeSub(key_expr=key_expr)
        self.declared.append(sub)
        return sub


def test_first_subscribe_opens_upstream_sub() -> None:
    sess = FakeSession()
    reg = SubscriptionRegistry(sess)
    reg.subscribe(client_id="c1", sub_id="s1", key_expr="bubbaloop/**", handler=lambda *_: None)
    assert len(sess.declared) == 1
    assert sess.declared[0].key_expr == "bubbaloop/**"


def test_second_subscribe_same_key_does_not_open_new_upstream() -> None:
    sess = FakeSession()
    reg = SubscriptionRegistry(sess)
    reg.subscribe(client_id="c1", sub_id="s1", key_expr="bubbaloop/**", handler=lambda *_: None)
    reg.subscribe(client_id="c2", sub_id="s1", key_expr="bubbaloop/**", handler=lambda *_: None)
    assert len(sess.declared) == 1


def test_unsubscribe_closes_only_when_refcount_zero() -> None:
    sess = FakeSession()
    reg = SubscriptionRegistry(sess)
    reg.subscribe(client_id="c1", sub_id="s1", key_expr="k1", handler=lambda *_: None)
    reg.subscribe(client_id="c2", sub_id="s1", key_expr="k1", handler=lambda *_: None)
    reg.unsubscribe(client_id="c1", sub_id="s1")
    assert sess.declared[0].closed is False
    reg.unsubscribe(client_id="c2", sub_id="s1")
    assert sess.declared[0].closed is True


def test_unsubscribe_unknown_is_noop() -> None:
    sess = FakeSession()
    reg = SubscriptionRegistry(sess)
    reg.unsubscribe(client_id="cZ", sub_id="sZ")  # must not raise


def test_drop_client_unsubscribes_all_its_subs() -> None:
    sess = FakeSession()
    reg = SubscriptionRegistry(sess)
    reg.subscribe(client_id="c1", sub_id="a", key_expr="k1", handler=lambda *_: None)
    reg.subscribe(client_id="c1", sub_id="b", key_expr="k2", handler=lambda *_: None)
    reg.drop_client("c1")
    # Both upstream subs (one per distinct key) are closed.
    assert all(s.closed for s in sess.declared)
```

- [ ] **Step 2: Run the tests and watch them fail**

```bash
cd /home/nvidia/bubbaloop-dash/backend
pytest tests/test_zenoh_multiplex.py -v
```

Expected: import errors.

- [ ] **Step 3: Implement `SubscriptionRegistry`**

File contents (`/home/nvidia/bubbaloop-dash/backend/src/dash/zenoh_proxy/multiplex.py`):

```python
"""Ref-counted Zenoh subscription multiplexer.

Two layers:
  - Per-(key_expr) upstream subscription is shared across all browser clients
    that want it. Ref-count tracks how many distinct (client_id, sub_id) pairs
    are interested; the upstream sub is undeclared when ref-count → 0.
  - Per-(client_id, sub_id) handler is invoked on every sample for the matching
    upstream sub. Multiple sub_ids per client are independent.

This module is intentionally Zenoh-agnostic so it's unit-testable with a fake
session. The real wiring happens in dash.api.zenoh_ws."""
from collections import defaultdict
from dataclasses import dataclass, field
from typing import Any, Callable, Protocol


class _UpstreamSession(Protocol):
    def declare_subscriber(self, key_expr: str, handler: Callable[..., Any]) -> Any: ...


SampleHandler = Callable[[bytes, str, str], None]
"""(payload_bytes, key_expr, encoding_str) -> None"""


@dataclass
class _UpstreamEntry:
    upstream_sub: Any
    clients: dict[tuple[str, str], SampleHandler] = field(default_factory=dict)


class SubscriptionRegistry:
    """Manage ref-counted upstream subs + per-client handlers."""

    def __init__(self, session: _UpstreamSession) -> None:
        self._session = session
        self._by_key: dict[str, _UpstreamEntry] = {}
        # Reverse map: client_id → list of (key_expr, sub_id) pairs they subscribed.
        self._by_client: dict[str, list[tuple[str, str]]] = defaultdict(list)

    def subscribe(
        self,
        client_id: str,
        sub_id: str,
        key_expr: str,
        handler: SampleHandler,
    ) -> None:
        entry = self._by_key.get(key_expr)
        if entry is None:
            def on_sample(sample):  # noqa: ANN001 — zenoh Sample type, not Phase 1.0
                payload = bytes(sample.payload) if hasattr(sample, "payload") else b""
                encoding = str(getattr(sample, "encoding", "application/octet-stream"))
                for h in list(entry.clients.values()) if entry else ():
                    try:
                        h(payload, key_expr, encoding)
                    except Exception:  # noqa: BLE001 — never let one client crash the rest
                        pass
            upstream = self._session.declare_subscriber(key_expr, on_sample)
            entry = _UpstreamEntry(upstream_sub=upstream)
            self._by_key[key_expr] = entry

        entry.clients[(client_id, sub_id)] = handler
        self._by_client[client_id].append((key_expr, sub_id))

    def unsubscribe(self, client_id: str, sub_id: str) -> None:
        for key_expr, entry in list(self._by_key.items()):
            if entry.clients.pop((client_id, sub_id), None) is not None:
                # Remove the reverse-map entry too.
                self._by_client[client_id] = [
                    (k, s) for (k, s) in self._by_client[client_id] if not (k == key_expr and s == sub_id)
                ]
                if not entry.clients:
                    entry.upstream_sub.undeclare()
                    del self._by_key[key_expr]
                return

    def drop_client(self, client_id: str) -> None:
        for key_expr, sub_id in list(self._by_client.get(client_id, [])):
            self.unsubscribe(client_id, sub_id)
        self._by_client.pop(client_id, None)
```

- [ ] **Step 4: Run the tests and watch them pass**

```bash
cd /home/nvidia/bubbaloop-dash/backend
pytest tests/test_zenoh_multiplex.py -v
```

Expected: 5 passed.

- [ ] **Step 5: Commit**

```bash
cd /home/nvidia/bubbaloop-dash
git add backend/src/dash/zenoh_proxy/multiplex.py backend/tests/test_zenoh_multiplex.py
git commit -m "feat(backend): ref-counted subscription registry

SubscriptionRegistry shares one upstream Zenoh sub across N browser clients
that ask for the same key_expr. Upstream sub is undeclared when the last
client drops. drop_client(id) cleans up everything on WS disconnect."
```

---

## Task 12: WS endpoint `/ws/zenoh` (auth + lifecycle)

**Files:**
- Create: `/home/nvidia/bubbaloop-dash/backend/src/dash/api/zenoh_ws.py`
- Create: `/home/nvidia/bubbaloop-dash/backend/tests/test_zenoh_ws.py`
- Modify: `/home/nvidia/bubbaloop-dash/backend/src/dash/app.py` (mount router, expose registry on app.state)

- [ ] **Step 1: Write the failing tests**

File contents (`/home/nvidia/bubbaloop-dash/backend/tests/test_zenoh_ws.py`):

```python
"""WebSocket endpoint /ws/zenoh.

Auth: ?token=... query param (browser WebSocket cannot send custom headers).
Lifecycle: accept on valid token; close 1008 (policy violation) on bad/missing
token. Subscribe/unsubscribe messages mutate the per-app SubscriptionRegistry.
"""
import json

import pytest
from fastapi.testclient import TestClient


def test_ws_rejects_missing_token(client: TestClient) -> None:
    with pytest.raises(Exception):  # noqa: B017 — starlette raises on disconnect
        with client.websocket_connect("/ws/zenoh"):
            pass


def test_ws_rejects_wrong_token(client: TestClient) -> None:
    with pytest.raises(Exception):
        with client.websocket_connect("/ws/zenoh?token=wrong"):
            pass


def test_ws_accepts_correct_token_and_handles_subscribe(client: TestClient, settings) -> None:
    # The app's lifespan started a ZenohSession; in tests it's likely "not_ready",
    # but the WS endpoint accepts the upgrade regardless and replies with an error
    # frame if the underlying session is unavailable for actual subscription.
    with client.websocket_connect(f"/ws/zenoh?token={settings.dashboard_token}") as ws:
        ws.send_text(json.dumps({
            "type": "subscribe",
            "id": "sub-1",
            "key_expr": "bubbaloop/global/test/**",
        }))
        # Expect an ack or an error frame back, but not a disconnect.
        msg = ws.receive_json(timeout=2.0)
        assert msg["type"] in ("ack", "error")
        assert msg["id"] == "sub-1"


def test_ws_unsupported_message_returns_error(client: TestClient, settings) -> None:
    with client.websocket_connect(f"/ws/zenoh?token={settings.dashboard_token}") as ws:
        ws.send_text(json.dumps({"type": "put", "id": "x", "key_expr": "k", "payload": "v"}))
        msg = ws.receive_json(timeout=2.0)
        assert msg["type"] == "error"
        assert "put" in msg["reason"].lower()
```

- [ ] **Step 2: Run the tests and watch them fail**

```bash
cd /home/nvidia/bubbaloop-dash/backend
pytest tests/test_zenoh_ws.py -v
```

Expected: 404 on websocket route (endpoint not yet wired).

- [ ] **Step 3: Implement the WS endpoint**

File contents (`/home/nvidia/bubbaloop-dash/backend/src/dash/api/zenoh_ws.py`):

```python
"""Multiplexed Zenoh-over-WebSocket proxy.

One WS per browser tab. Token auth via ?token=... query param.
Browser sends {subscribe, unsubscribe}; backend forwards Zenoh samples
as {type:"sample", ...} JSON frames. Unsupported message types (put,
get, queryable) reply with an explicit {type:"error", ...} frame and
keep the connection open."""
import asyncio
import json
import uuid

from fastapi import APIRouter, Query, WebSocket, WebSocketDisconnect, status

from dash.logging import get_logger
from dash.zenoh_proxy.multiplex import SubscriptionRegistry
from dash.zenoh_proxy.protocol import (
    SubscribeMessage,
    UnsubscribeMessage,
    UnsupportedMessage,
    parse_client_message,
    serialize_sample,
)

_log = get_logger("dash.api.zenoh_ws")

router = APIRouter()


@router.websocket("/ws/zenoh")
async def zenoh_ws(ws: WebSocket, token: str | None = Query(default=None)) -> None:
    expected = ws.app.state.settings.dashboard_token
    if expected is None or token != expected:
        await ws.close(code=status.WS_1008_POLICY_VIOLATION, reason="invalid_token")
        return

    await ws.accept()
    client_id = uuid.uuid4().hex
    zsess = ws.app.state.zenoh_session
    registry: SubscriptionRegistry | None = None

    # Build the registry lazily once the upstream session is actually ready.
    # If the session is "not_ready", subscribe attempts reply with an error frame
    # but the WS stays open so the client can retry.
    loop = asyncio.get_running_loop()
    pending: asyncio.Queue[dict[str, object]] = asyncio.Queue()

    def push_outgoing(frame: dict[str, object]) -> None:
        loop.call_soon_threadsafe(pending.put_nowait, frame)

    async def sender() -> None:
        try:
            while True:
                frame = await pending.get()
                await ws.send_text(json.dumps(frame))
        except (WebSocketDisconnect, RuntimeError):
            return

    send_task = asyncio.create_task(sender())

    try:
        while True:
            raw = await ws.receive_text()
            try:
                payload = json.loads(raw)
            except json.JSONDecodeError:
                await ws.send_text(json.dumps({"type": "error", "reason": "invalid_json"}))
                continue

            msg = parse_client_message(payload)

            if isinstance(msg, SubscribeMessage):
                if registry is None:
                    if zsess.status() != "ok":
                        await ws.send_text(json.dumps({
                            "type": "error",
                            "id": msg.id,
                            "reason": f"zenoh_session_{zsess.status()}",
                        }))
                        continue
                    registry = SubscriptionRegistry(zsess.session())
                try:
                    def handler(payload_bytes: bytes, key_expr: str, encoding: str) -> None:
                        push_outgoing(serialize_sample(msg.id, key_expr, payload_bytes, encoding))
                    registry.subscribe(client_id, msg.id, msg.key_expr, handler)
                    await ws.send_text(json.dumps({"type": "ack", "id": msg.id, "key_expr": msg.key_expr}))
                except Exception as exc:  # noqa: BLE001
                    await ws.send_text(json.dumps({"type": "error", "id": msg.id, "reason": str(exc)}))

            elif isinstance(msg, UnsubscribeMessage):
                if registry is not None:
                    registry.unsubscribe(client_id, msg.id)
                await ws.send_text(json.dumps({"type": "ack", "id": msg.id, "op": "unsubscribe"}))

            elif isinstance(msg, UnsupportedMessage):
                await ws.send_text(json.dumps({
                    "type": "error",
                    "id": payload.get("id", ""),
                    "reason": f"unsupported_message_type: {msg.kind}",
                }))

    except WebSocketDisconnect:
        pass
    finally:
        send_task.cancel()
        if registry is not None:
            registry.drop_client(client_id)
        _log.info("ws.disconnected", client_id=client_id)
```

- [ ] **Step 4: Mount the router**

Edit `/home/nvidia/bubbaloop-dash/backend/src/dash/app.py` — add the new router import and `include_router` call:

```python
"""FastAPI application factory."""
from contextlib import asynccontextmanager
from typing import AsyncIterator

from fastapi import FastAPI

from dash.api import health, zenoh_ws
from dash.config import Settings
from dash.logging import configure_logging
from dash.tokens import ensure_token
from dash.zenoh_proxy.session import ZenohSession


def create_app(settings: Settings) -> FastAPI:
    configure_logging(level="INFO")
    ensure_token(settings)

    @asynccontextmanager
    async def lifespan(app: FastAPI) -> AsyncIterator[None]:
        zsess = ZenohSession(endpoint=settings.zenoh_endpoint)
        await zsess.start()
        app.state.zenoh_session = zsess
        try:
            yield
        finally:
            await zsess.stop()

    app = FastAPI(
        title="bubbaloop-dash",
        version="0.1.0a0",
        description="Self-hosted backend + dashboard for Bubbaloop",
        lifespan=lifespan,
    )
    app.state.settings = settings
    app.include_router(health.router)
    app.include_router(zenoh_ws.router)
    return app


app = create_app(Settings())
```

- [ ] **Step 5: Run the tests**

```bash
cd /home/nvidia/bubbaloop-dash/backend
pytest tests/test_zenoh_ws.py -v
```

Expected: 4 passed. The first two expect a `WebSocketDisconnect`-style raise from `TestClient` when the server closes with 1008; the third gets an error frame back (since the test app's session isn't connected); the fourth confirms unsupported-message handling.

- [ ] **Step 6: Commit**

```bash
cd /home/nvidia/bubbaloop-dash
git add backend/src/dash/api/zenoh_ws.py backend/src/dash/app.py backend/tests/test_zenoh_ws.py
git commit -m "feat(backend): /ws/zenoh — multiplexed Zenoh-over-WS proxy

One WS per browser tab. Token auth via ?token= query (browsers can't set
WS headers). Subscribe/unsubscribe operate on a per-WS SubscriptionRegistry
ref-counted into upstream subs on the shared Zenoh session. Unsupported
message types reply with an error frame and keep the connection open."
```

---

## Task 13: Snapshot the existing dashboard into frontend/

**Files:**
- Create: `/home/nvidia/bubbaloop-dash/frontend/` (copied from `/home/nvidia/bubbaloop/dashboard/`)

This task is a one-time file copy — no TDD. Frontend tests inherited from the source already exist; we run them at the end to verify the copy is sound.

- [ ] **Step 1: Copy the dashboard tree**

```bash
cd /home/nvidia/bubbaloop-dash
SOURCE_SHA=$(cat .source-sha)
mkdir -p frontend
cp -a /home/nvidia/bubbaloop/dashboard/. frontend/
# Drop directories that don't belong in a fresh repo.
rm -rf frontend/node_modules frontend/dist frontend/.vite
```

- [ ] **Step 2: Verify the copy is clean**

```bash
cd /home/nvidia/bubbaloop-dash/frontend
ls package.json vite.config.ts src/App.tsx src/main.tsx
```

Expected: all four files exist.

- [ ] **Step 3: Install npm dependencies**

```bash
cd /home/nvidia/bubbaloop-dash/frontend
npm ci
```

Expected: `package-lock.json` resolves without errors; `node_modules/` populated.

- [ ] **Step 4: Run the existing tests against the lifted copy**

```bash
cd /home/nvidia/bubbaloop-dash/frontend
npm test
```

Expected: existing test suite passes (or matches whatever it produces in the source repo — same baseline).

- [ ] **Step 5: Build to confirm production bundle works**

```bash
cd /home/nvidia/bubbaloop-dash/frontend
npm run build
ls dist/index.html
```

Expected: `dist/index.html` exists.

- [ ] **Step 6: Commit**

```bash
cd /home/nvidia/bubbaloop-dash
git add frontend/
git commit -m "chore(frontend): snapshot from kornia/bubbaloop dashboard/

Initial frontend lift via cp -a from /home/nvidia/bubbaloop/dashboard/.
Source SHA pinned in .source-sha. node_modules/, dist/, .vite/ excluded.
Tests + build verified against the snapshot."
```

---

## Task 14: Vite dev proxy for `/api` and `/ws`

**Files:**
- Modify: `/home/nvidia/bubbaloop-dash/frontend/vite.config.ts`

In production the FastAPI backend serves the built frontend. In dev, `vite` runs on `:5173` and proxies API + WS calls to the backend on `:8000`. This task wires that proxy so the new auth + Zenoh code can be developed without rebuilding the docker container every cycle.

- [ ] **Step 1: Read the current vite.config.ts**

```bash
cat /home/nvidia/bubbaloop-dash/frontend/vite.config.ts
```

Take note of the existing structure (likely has React plugin, top-level await, WASM plugin).

- [ ] **Step 2: Update vite.config.ts to add the proxy**

Replace the contents of `/home/nvidia/bubbaloop-dash/frontend/vite.config.ts` with the version below. Preserve any plugin lines from the lifted file (the example assumes the canonical Bubbaloop dashboard layout — if the lifted file imports additional plugins, keep those imports).

```typescript
import { defineConfig } from 'vite';
import react from '@vitejs/plugin-react';
import wasm from 'vite-plugin-wasm';
import topLevelAwait from 'vite-plugin-top-level-await';

const BACKEND = process.env.VITE_BACKEND_URL ?? 'http://127.0.0.1:8000';

export default defineConfig({
  plugins: [react(), wasm(), topLevelAwait()],
  server: {
    host: '127.0.0.1',
    port: 5173,
    proxy: {
      '/api': {
        target: BACKEND,
        changeOrigin: true,
      },
      '/ws': {
        target: BACKEND.replace(/^http/, 'ws'),
        ws: true,
        changeOrigin: true,
      },
      '/healthz': { target: BACKEND, changeOrigin: true },
      '/readyz': { target: BACKEND, changeOrigin: true },
    },
  },
});
```

- [ ] **Step 3: Verify the dev server still starts**

```bash
cd /home/nvidia/bubbaloop-dash/frontend
timeout 5 npm run dev || true
```

Expected: vite starts, prints "Local: http://127.0.0.1:5173/", times out after 5s (expected — we don't want a permanent dev server here).

- [ ] **Step 4: Commit**

```bash
cd /home/nvidia/bubbaloop-dash
git add frontend/vite.config.ts
git commit -m "feat(frontend): vite dev proxy for /api, /ws, /healthz, /readyz

Dev server on :5173 proxies API + WS to the backend on :8000 (override
with VITE_BACKEND_URL). Production deploys serve frontend/dist/ statically
from FastAPI, so no proxy needed there."
```

---

## Task 15: Update `lib/zenoh.ts` to point at `/ws/zenoh?token=...`

**Files:**
- Modify: `/home/nvidia/bubbaloop-dash/frontend/src/lib/zenoh.ts`

The existing dashboard derives its Zenoh endpoint from `window.location` and hits `/zenoh` (a path the bubbaloop daemon serves). In the new repo, the backend exposes `/ws/zenoh` and requires a token. This task updates the URL-building helper.

- [ ] **Step 1: Read the current implementation**

```bash
grep -n "zenoh\|endpoint\|wss\|/zenoh" /home/nvidia/bubbaloop-dash/frontend/src/lib/zenoh.ts | head -30
```

Note where `/zenoh` appears and how the endpoint string is built.

- [ ] **Step 2: Locate the endpoint construction**

The likely shape (from the bubbaloop dashboard) is a function that returns `wss://${host}/zenoh`. We need to:
- Change `/zenoh` → `/ws/zenoh`
- Append `?token=${token}` from a token-resolving helper (lives in `lib/auth-token.ts`, created next task)

For now, modify `lib/zenoh.ts` to accept an optional `token` parameter passed into `useZenohSession`:

The existing call site is `App.tsx`:
```tsx
const { session, status, error, reconnect } = useZenohSession(zenohConfig);
```

After this task, the config type gains a `token` field. The hook concatenates it as a query param.

Edit `/home/nvidia/bubbaloop-dash/frontend/src/lib/zenoh.ts`. Find the function that builds the endpoint URL (look for `wss:` or `/zenoh`). Update the path component from `/zenoh` to `/ws/zenoh` and append the token query param when present. If the file exposes a `ZenohConfig` type, add `token?: string` to it.

The minimal change (illustrative — apply to whatever the actual code looks like):

```typescript
export interface ZenohConfig {
  endpoint: string;
  // ... existing fields ...
  token?: string;
}

function buildWsUrl(endpoint: string, token?: string): string {
  // endpoint comes in as e.g. ws://host:5173/zenoh — replace path and append token.
  const url = new URL(endpoint);
  url.pathname = '/ws/zenoh';
  if (token) url.searchParams.set('token', token);
  return url.toString();
}
```

Update the `useZenohSession` hook so wherever it constructs the WS connection, it calls `buildWsUrl(config.endpoint, config.token)` instead of using the raw endpoint.

- [ ] **Step 3: Update `App.tsx`'s `getZenohEndpoint` to default to `/ws/zenoh`**

In `/home/nvidia/bubbaloop-dash/frontend/src/App.tsx`, find:

```typescript
return `${protocol}://${window.location.host}/zenoh`;
```

Replace with:

```typescript
return `${protocol}://${window.location.host}/ws/zenoh`;
```

(The token is appended by `buildWsUrl` when the auth context is wired in Task 18.)

- [ ] **Step 4: Run frontend tests to make sure nothing regressed**

```bash
cd /home/nvidia/bubbaloop-dash/frontend
npm test
```

Expected: existing tests still pass. (Tests do not exercise the live Zenoh path; they should be unaffected by URL changes.)

- [ ] **Step 5: Commit**

```bash
cd /home/nvidia/bubbaloop-dash
git add frontend/src/lib/zenoh.ts frontend/src/App.tsx
git commit -m "feat(frontend): point Zenoh client at /ws/zenoh + token query param

URL path moves from /zenoh (daemon-served) to /ws/zenoh (backend-served).
ZenohConfig gains optional token field; buildWsUrl helper appends ?token=
when set. App.tsx default endpoint updated. Token wiring lands in Task 18
(AuthContext)."
```

---

## Task 16: `lib/api.ts` — typed REST client

**Files:**
- Create: `/home/nvidia/bubbaloop-dash/frontend/src/lib/api.ts`
- Create: `/home/nvidia/bubbaloop-dash/frontend/src/lib/__tests__/api.test.ts`

- [ ] **Step 1: Write the failing test**

File contents (`/home/nvidia/bubbaloop-dash/frontend/src/lib/__tests__/api.test.ts`):

```typescript
import { describe, expect, it, beforeEach, vi } from 'vitest';
import { ApiClient } from '../api';

describe('ApiClient', () => {
  beforeEach(() => {
    vi.restoreAllMocks();
  });

  it('attaches Authorization header from token', async () => {
    const fetchMock = vi.spyOn(globalThis, 'fetch').mockResolvedValue(
      new Response(JSON.stringify({ ok: true }), { status: 200 }),
    );
    const client = new ApiClient({ baseUrl: '', getToken: () => 'tk-123' });
    await client.get('/api/healthz');
    expect(fetchMock).toHaveBeenCalledWith(
      '/api/healthz',
      expect.objectContaining({
        headers: expect.objectContaining({ Authorization: 'Bearer tk-123' }),
      }),
    );
  });

  it('returns parsed JSON on 2xx', async () => {
    vi.spyOn(globalThis, 'fetch').mockResolvedValue(
      new Response(JSON.stringify({ foo: 'bar' }), { status: 200 }),
    );
    const client = new ApiClient({ baseUrl: '', getToken: () => null });
    const body = await client.get<{ foo: string }>('/api/x');
    expect(body).toEqual({ foo: 'bar' });
  });

  it('throws ApiError on non-2xx', async () => {
    vi.spyOn(globalThis, 'fetch').mockResolvedValue(
      new Response(JSON.stringify({ detail: 'nope' }), { status: 401 }),
    );
    const client = new ApiClient({ baseUrl: '', getToken: () => null });
    await expect(client.get('/api/x')).rejects.toThrow(/401/);
  });
});
```

- [ ] **Step 2: Run the test and watch it fail**

```bash
cd /home/nvidia/bubbaloop-dash/frontend
npx vitest run src/lib/__tests__/api.test.ts
```

Expected: import error (file doesn't exist).

- [ ] **Step 3: Implement the REST client**

File contents (`/home/nvidia/bubbaloop-dash/frontend/src/lib/api.ts`):

```typescript
/**
 * Typed REST client. Auto-attaches Authorization: Bearer ${token} when token present.
 * Throws ApiError on non-2xx; returns parsed JSON otherwise.
 */
export interface ApiClientConfig {
  baseUrl: string;
  getToken: () => string | null;
}

export class ApiError extends Error {
  constructor(public readonly status: number, public readonly body: unknown) {
    super(`HTTP ${status}: ${typeof body === 'string' ? body : JSON.stringify(body)}`);
  }
}

export class ApiClient {
  constructor(private readonly config: ApiClientConfig) {}

  async get<T>(path: string): Promise<T> {
    return this.request<T>('GET', path);
  }

  async post<T>(path: string, body?: unknown): Promise<T> {
    return this.request<T>('POST', path, body);
  }

  async put<T>(path: string, body?: BodyInit | unknown): Promise<T> {
    return this.request<T>('PUT', path, body);
  }

  async delete<T>(path: string): Promise<T> {
    return this.request<T>('DELETE', path);
  }

  private async request<T>(
    method: string,
    path: string,
    body?: unknown,
  ): Promise<T> {
    const headers: Record<string, string> = {};
    const token = this.config.getToken();
    if (token) headers.Authorization = `Bearer ${token}`;

    let payload: BodyInit | undefined;
    if (body !== undefined) {
      if (body instanceof ArrayBuffer || body instanceof Blob || body instanceof FormData) {
        payload = body;
      } else {
        headers['Content-Type'] = 'application/json';
        payload = JSON.stringify(body);
      }
    }

    const url = `${this.config.baseUrl}${path}`;
    const resp = await fetch(url, { method, headers, body: payload });

    let parsed: unknown;
    const text = await resp.text();
    try {
      parsed = text ? JSON.parse(text) : null;
    } catch {
      parsed = text;
    }

    if (!resp.ok) throw new ApiError(resp.status, parsed);
    return parsed as T;
  }
}
```

- [ ] **Step 4: Run the test and watch it pass**

```bash
cd /home/nvidia/bubbaloop-dash/frontend
npx vitest run src/lib/__tests__/api.test.ts
```

Expected: 3 passed.

- [ ] **Step 5: Commit**

```bash
cd /home/nvidia/bubbaloop-dash
git add frontend/src/lib/api.ts frontend/src/lib/__tests__/api.test.ts
git commit -m "feat(frontend): typed REST client with bearer-token attach

ApiClient.get/post/put/delete with auto-Authorization header from a
token-getter callback. ApiError throws on non-2xx with status + parsed body.
Handles ArrayBuffer/Blob/FormData bodies without JSON-stringifying."
```

---

## Task 17: AuthContext + AuthProvider

**Files:**
- Create: `/home/nvidia/bubbaloop-dash/frontend/src/contexts/AuthContext.tsx`
- Create: `/home/nvidia/bubbaloop-dash/frontend/src/contexts/__tests__/AuthContext.test.tsx`

- [ ] **Step 1: Write the failing test**

File contents (`/home/nvidia/bubbaloop-dash/frontend/src/contexts/__tests__/AuthContext.test.tsx`):

```typescript
import { describe, expect, it, beforeEach } from 'vitest';
import { render, screen, act } from '@testing-library/react';
import { AuthProvider, useAuth } from '../AuthContext';

function Probe() {
  const { token, setToken, clearToken } = useAuth();
  return (
    <div>
      <span data-testid="token">{token ?? 'none'}</span>
      <button onClick={() => setToken('xyz')}>set</button>
      <button onClick={() => clearToken()}>clear</button>
    </div>
  );
}

describe('AuthContext', () => {
  beforeEach(() => {
    localStorage.clear();
  });

  it('starts with no token if localStorage empty', () => {
    render(<AuthProvider><Probe /></AuthProvider>);
    expect(screen.getByTestId('token').textContent).toBe('none');
  });

  it('loads token from localStorage on mount', () => {
    localStorage.setItem('bubbaloop_dash_token', 'persisted-tk');
    render(<AuthProvider><Probe /></AuthProvider>);
    expect(screen.getByTestId('token').textContent).toBe('persisted-tk');
  });

  it('setToken updates state and persists to localStorage', async () => {
    render(<AuthProvider><Probe /></AuthProvider>);
    await act(async () => { screen.getByText('set').click(); });
    expect(screen.getByTestId('token').textContent).toBe('xyz');
    expect(localStorage.getItem('bubbaloop_dash_token')).toBe('xyz');
  });

  it('clearToken removes state and localStorage entry', async () => {
    localStorage.setItem('bubbaloop_dash_token', 'old');
    render(<AuthProvider><Probe /></AuthProvider>);
    await act(async () => { screen.getByText('clear').click(); });
    expect(screen.getByTestId('token').textContent).toBe('none');
    expect(localStorage.getItem('bubbaloop_dash_token')).toBeNull();
  });
});
```

- [ ] **Step 2: Run the test and watch it fail**

```bash
cd /home/nvidia/bubbaloop-dash/frontend
npx vitest run src/contexts/__tests__/AuthContext.test.tsx
```

Expected: import error.

- [ ] **Step 3: Implement `AuthContext`**

File contents (`/home/nvidia/bubbaloop-dash/frontend/src/contexts/AuthContext.tsx`):

```tsx
import { createContext, useCallback, useContext, useEffect, useState, ReactNode } from 'react';

const STORAGE_KEY = 'bubbaloop_dash_token';

interface AuthState {
  token: string | null;
  setToken: (t: string) => void;
  clearToken: () => void;
}

const AuthContext = createContext<AuthState | null>(null);

export function AuthProvider({ children }: { children: ReactNode }) {
  const [token, setTokenState] = useState<string | null>(() => {
    try {
      return localStorage.getItem(STORAGE_KEY);
    } catch {
      return null;
    }
  });

  useEffect(() => {
    if (token === null) return;
    try {
      localStorage.setItem(STORAGE_KEY, token);
    } catch {
      // localStorage unavailable (e.g. private mode in some browsers) — silently degrade.
    }
  }, [token]);

  const setToken = useCallback((t: string) => setTokenState(t), []);
  const clearToken = useCallback(() => {
    setTokenState(null);
    try {
      localStorage.removeItem(STORAGE_KEY);
    } catch {
      // see above
    }
  }, []);

  return (
    <AuthContext.Provider value={{ token, setToken, clearToken }}>
      {children}
    </AuthContext.Provider>
  );
}

export function useAuth(): AuthState {
  const ctx = useContext(AuthContext);
  if (!ctx) throw new Error('useAuth must be used inside <AuthProvider>');
  return ctx;
}
```

- [ ] **Step 4: Run the test and watch it pass**

```bash
cd /home/nvidia/bubbaloop-dash/frontend
npx vitest run src/contexts/__tests__/AuthContext.test.tsx
```

Expected: 4 passed.

- [ ] **Step 5: Commit**

```bash
cd /home/nvidia/bubbaloop-dash
git add frontend/src/contexts/AuthContext.tsx frontend/src/contexts/__tests__/AuthContext.test.tsx
git commit -m "feat(frontend): AuthContext + AuthProvider

Token state with localStorage persistence (key: bubbaloop_dash_token).
useAuth() exposes { token, setToken, clearToken }. Silent degradation
when localStorage is unavailable (e.g. some private-mode browsers)."
```

---

## Task 18: Login screen + main.tsx gate

**Files:**
- Create: `/home/nvidia/bubbaloop-dash/frontend/src/components/Login.tsx`
- Create: `/home/nvidia/bubbaloop-dash/frontend/src/components/__tests__/Login.test.tsx`
- Modify: `/home/nvidia/bubbaloop-dash/frontend/src/main.tsx`

- [ ] **Step 1: Write the failing test**

File contents (`/home/nvidia/bubbaloop-dash/frontend/src/components/__tests__/Login.test.tsx`):

```typescript
import { describe, expect, it, beforeEach } from 'vitest';
import { render, screen, fireEvent } from '@testing-library/react';
import { AuthProvider, useAuth } from '../../contexts/AuthContext';
import { Login } from '../Login';

function Display() {
  const { token } = useAuth();
  return <div data-testid="t">{token ?? 'none'}</div>;
}

describe('Login', () => {
  beforeEach(() => localStorage.clear());

  it('sets the token via input + submit', () => {
    render(
      <AuthProvider>
        <Login />
        <Display />
      </AuthProvider>,
    );
    const input = screen.getByPlaceholderText(/paste your bearer token/i) as HTMLInputElement;
    fireEvent.change(input, { target: { value: 'tk-abc' } });
    fireEvent.click(screen.getByRole('button', { name: /sign in/i }));
    expect(screen.getByTestId('t').textContent).toBe('tk-abc');
  });

  it('disables submit on empty input', () => {
    render(<AuthProvider><Login /></AuthProvider>);
    const button = screen.getByRole('button', { name: /sign in/i });
    expect(button).toBeDisabled();
  });
});
```

- [ ] **Step 2: Run the test and watch it fail**

```bash
cd /home/nvidia/bubbaloop-dash/frontend
npx vitest run src/components/__tests__/Login.test.tsx
```

Expected: import error.

- [ ] **Step 3: Implement the Login component**

File contents (`/home/nvidia/bubbaloop-dash/frontend/src/components/Login.tsx`):

```tsx
import { FormEvent, useState } from 'react';
import { useAuth } from '../contexts/AuthContext';

export function Login() {
  const { setToken } = useAuth();
  const [value, setValue] = useState('');

  function onSubmit(e: FormEvent) {
    e.preventDefault();
    const trimmed = value.trim();
    if (trimmed) setToken(trimmed);
  }

  return (
    <div className="login">
      <div className="login-card">
        <h1>bubbaloop-dash</h1>
        <p>Paste your dashboard bearer token to continue.</p>
        <form onSubmit={onSubmit}>
          <input
            type="password"
            placeholder="Paste your bearer token"
            value={value}
            onChange={(e) => setValue(e.target.value)}
            autoFocus
            autoComplete="off"
          />
          <button type="submit" disabled={value.trim().length === 0}>
            Sign in
          </button>
        </form>
        <p className="hint">
          Token is generated on first server run. Check the backend logs for the line
          tagged <code>token.generated</code>, or set <code>DASHBOARD_TOKEN</code>
          in the server env.
        </p>
      </div>

      <style>{`
        .login { display: flex; align-items: center; justify-content: center; height: 100vh; background: var(--bg-primary, #0d1117); }
        .login-card { background: var(--bg-secondary, #161b22); padding: 32px; border-radius: 12px; max-width: 420px; width: 100%; }
        .login-card h1 { background: var(--accent-gradient, linear-gradient(45deg, #3d5afe, #00e5ff)); -webkit-background-clip: text; -webkit-text-fill-color: transparent; background-clip: text; font-size: 24px; margin: 0 0 8px; }
        .login-card p { color: var(--text-secondary, #8b949e); font-size: 14px; margin: 0 0 16px; }
        .login-card form { display: flex; gap: 8px; }
        .login-card input { flex: 1; padding: 10px 12px; background: var(--bg-tertiary, #21262d); border: 1px solid var(--border-color, #30363d); border-radius: 6px; color: var(--text-primary, #c9d1d9); font-family: 'JetBrains Mono', monospace; font-size: 13px; }
        .login-card button { padding: 10px 16px; background: var(--accent-primary, #3d5afe); color: white; border: none; border-radius: 6px; cursor: pointer; font-weight: 500; }
        .login-card button:disabled { opacity: 0.4; cursor: not-allowed; }
        .login-card .hint { font-size: 12px; color: var(--text-muted, #6e7681); margin-top: 16px; }
        .login-card .hint code { background: var(--bg-tertiary, #21262d); padding: 1px 4px; border-radius: 3px; }
      `}</style>
    </div>
  );
}
```

- [ ] **Step 4: Run the component test**

```bash
cd /home/nvidia/bubbaloop-dash/frontend
npx vitest run src/components/__tests__/Login.test.tsx
```

Expected: 2 passed.

- [ ] **Step 5: Gate `<App />` in `main.tsx` on token presence**

Edit `/home/nvidia/bubbaloop-dash/frontend/src/main.tsx`. Find the existing render (looks like `ReactDOM.createRoot(...).render(<App />)`). Wrap it with `<AuthProvider>` and add the token gate:

```tsx
import React from 'react';
import ReactDOM from 'react-dom/client';
import App from './App';
import { Login } from './components/Login';
import { AuthProvider, useAuth } from './contexts/AuthContext';
import './index.css';

function Gate() {
  const { token } = useAuth();
  return token ? <App /> : <Login />;
}

ReactDOM.createRoot(document.getElementById('root')!).render(
  <React.StrictMode>
    <AuthProvider>
      <Gate />
    </AuthProvider>
  </React.StrictMode>,
);
```

(Adapt the existing imports as needed — keep any extra setup lines from the source file.)

- [ ] **Step 6: Update `App.tsx` to consume the auth token for Zenoh**

In `App.tsx`, where `zenohConfig` is built, pull the token from `useAuth` and pass it through:

```tsx
import { useAuth } from './contexts/AuthContext';

// inside App():
const { token } = useAuth();
const zenohConfig = useMemo(
  () => ({ endpoint: ZENOH_ENDPOINT, token: token ?? undefined }),
  [token],
);
```

(The `ZenohConfig` type was extended in Task 15 to accept an optional `token`.)

- [ ] **Step 7: Run all frontend tests**

```bash
cd /home/nvidia/bubbaloop-dash/frontend
npm test
```

Expected: all tests pass (including the new Login + AuthContext tests, plus the existing ones).

- [ ] **Step 8: Commit**

```bash
cd /home/nvidia/bubbaloop-dash
git add frontend/src/components/Login.tsx frontend/src/components/__tests__/Login.test.tsx frontend/src/main.tsx frontend/src/App.tsx
git commit -m "feat(frontend): Login screen + main.tsx token gate

main.tsx wraps the tree in AuthProvider and renders <Login /> when no
token is present, <App /> otherwise. App.tsx now pulls the token from
useAuth() and threads it into the Zenoh WS connection."
```

---

## Task 19: Static-file serving from FastAPI

**Files:**
- Create: `/home/nvidia/bubbaloop-dash/backend/src/dash/static.py`
- Modify: `/home/nvidia/bubbaloop-dash/backend/src/dash/app.py`
- Create: `/home/nvidia/bubbaloop-dash/backend/tests/test_static.py`

The Docker image (Task 20) bakes the built frontend into `/srv/dash-frontend/`. The backend mounts that directory as static files, serving `index.html` for all unmatched paths so the SPA's client-side router works (no real router in v1, but the structure is in place).

- [ ] **Step 1: Write the failing test**

File contents (`/home/nvidia/bubbaloop-dash/backend/tests/test_static.py`):

```python
"""Static file serving falls through to index.html for SPA routes."""
from pathlib import Path

import pytest
from fastapi.testclient import TestClient

from dash.app import create_app
from dash.config import Settings


def _prep_static(tmp_path: Path, monkeypatch: pytest.MonkeyPatch) -> tuple[TestClient, Path]:
    static_dir = tmp_path / "static"
    static_dir.mkdir()
    (static_dir / "index.html").write_text("<!doctype html><title>dash</title>")
    (static_dir / "assets").mkdir()
    (static_dir / "assets" / "app.js").write_text("console.log('app')")

    monkeypatch.setenv("DATA_DIR", str(tmp_path))
    monkeypatch.setenv("DASHBOARD_TOKEN", "tk")
    monkeypatch.setenv("STATIC_DIR", str(static_dir))

    settings = Settings()
    return TestClient(create_app(settings)), static_dir


def test_serves_index_html_at_root(tmp_path: Path, monkeypatch: pytest.MonkeyPatch) -> None:
    client, _ = _prep_static(tmp_path, monkeypatch)
    r = client.get("/")
    assert r.status_code == 200
    assert "<title>dash</title>" in r.text


def test_serves_real_asset(tmp_path: Path, monkeypatch: pytest.MonkeyPatch) -> None:
    client, _ = _prep_static(tmp_path, monkeypatch)
    r = client.get("/assets/app.js")
    assert r.status_code == 200
    assert "console.log" in r.text


def test_unknown_route_falls_through_to_index(tmp_path: Path, monkeypatch: pytest.MonkeyPatch) -> None:
    client, _ = _prep_static(tmp_path, monkeypatch)
    r = client.get("/some/spa/route")
    assert r.status_code == 200
    assert "<title>dash</title>" in r.text


def test_api_route_does_not_fall_through(tmp_path: Path, monkeypatch: pytest.MonkeyPatch) -> None:
    client, _ = _prep_static(tmp_path, monkeypatch)
    r = client.get("/healthz")  # real API route — must not return index.html
    assert r.status_code == 200
    assert r.json() == {"status": "ok"}
```

- [ ] **Step 2: Run the test and watch it fail**

```bash
cd /home/nvidia/bubbaloop-dash/backend
pytest tests/test_static.py -v
```

Expected: SPA routes return 404 (no fallback yet).

- [ ] **Step 3: Extend Settings with `static_dir`**

Edit `/home/nvidia/bubbaloop-dash/backend/src/dash/config.py` — replace the file entirely so we add the field AND a validator that treats empty-string env values as `None` (docker-compose.dev.yml sets `STATIC_DIR=""` to disable static serving in dev):

```python
"""Application settings sourced from environment variables.

All knobs default to values suitable for a local dev run; production deployments
override via env (typically through Docker/compose).
"""
from pathlib import Path

from pydantic import Field, field_validator
from pydantic_settings import BaseSettings, SettingsConfigDict


class Settings(BaseSettings):
    """Process-wide configuration. Read once at app startup."""

    model_config = SettingsConfigDict(
        env_file=".env",
        env_file_encoding="utf-8",
        case_sensitive=False,
        extra="ignore",
    )

    data_dir: Path = Field(default=Path("/var/lib/bubbaloop-dash"))
    dashboard_token: str | None = Field(default=None)
    port: int = Field(default=8000, ge=1, le=65535)
    zenoh_endpoint: str = Field(default="tcp/127.0.0.1:7447")
    sweep_interval_s: int = Field(default=3600, ge=60)
    job_concurrency: int = Field(default=1, ge=1)
    static_dir: Path | None = Field(default=None, description="Path to built frontend; None/empty disables static serving.")

    @field_validator("static_dir", mode="before")
    @classmethod
    def _empty_str_to_none(cls, v: object) -> object:
        if v in (None, "", "none", "None"):
            return None
        return v
```

- [ ] **Step 4: Implement the static mount**

File contents (`/home/nvidia/bubbaloop-dash/backend/src/dash/static.py`):

```python
"""Mount built frontend on the FastAPI app with SPA fallback.

Routes hit in this order:
  1. Real API routes (FastAPI's normal router)
  2. Static files under STATIC_DIR (e.g. /assets/app.js, /favicon.ico)
  3. Fallback to STATIC_DIR/index.html for any other GET (SPA route)

If STATIC_DIR is not set, static serving is disabled entirely — the API
runs alone (useful for dev where vite serves the frontend on :5173)."""
from pathlib import Path

from fastapi import FastAPI, Request
from fastapi.responses import FileResponse, Response
from fastapi.staticfiles import StaticFiles


def mount_static(app: FastAPI, static_dir: Path | None) -> None:
    if static_dir is None or not static_dir.exists():
        return

    index = static_dir / "index.html"
    if not index.exists():
        return

    # /assets, /favicon.ico, etc. served by StaticFiles. Mounted last so API routes win.
    app.mount("/_dash_static", StaticFiles(directory=str(static_dir)), name="dash_static")

    @app.middleware("http")
    async def _spa_fallback(request: Request, call_next):  # noqa: ANN001
        response: Response = await call_next(request)
        if (
            response.status_code == 404
            and request.method == "GET"
            and not request.url.path.startswith("/api/")
            and not request.url.path.startswith("/ws/")
            and not request.url.path.startswith("/healthz")
            and not request.url.path.startswith("/readyz")
        ):
            # Try the literal file first.
            candidate = static_dir / request.url.path.lstrip("/")
            if candidate.is_file():
                return FileResponse(str(candidate))
            return FileResponse(str(index))
        return response
```

- [ ] **Step 5: Wire `mount_static` into the app factory**

Edit `/home/nvidia/bubbaloop-dash/backend/src/dash/app.py` — replace the file:

```python
"""FastAPI application factory."""
from contextlib import asynccontextmanager
from typing import AsyncIterator

from fastapi import FastAPI

from dash.api import health, zenoh_ws
from dash.config import Settings
from dash.logging import configure_logging
from dash.static import mount_static
from dash.tokens import ensure_token
from dash.zenoh_proxy.session import ZenohSession


def create_app(settings: Settings) -> FastAPI:
    configure_logging(level="INFO")
    ensure_token(settings)

    @asynccontextmanager
    async def lifespan(app: FastAPI) -> AsyncIterator[None]:
        zsess = ZenohSession(endpoint=settings.zenoh_endpoint)
        await zsess.start()
        app.state.zenoh_session = zsess
        try:
            yield
        finally:
            await zsess.stop()

    app = FastAPI(
        title="bubbaloop-dash",
        version="0.1.0a0",
        description="Self-hosted backend + dashboard for Bubbaloop",
        lifespan=lifespan,
    )
    app.state.settings = settings
    app.include_router(health.router)
    app.include_router(zenoh_ws.router)
    mount_static(app, settings.static_dir)
    return app


app = create_app(Settings())
```

- [ ] **Step 6: Run the tests**

```bash
cd /home/nvidia/bubbaloop-dash/backend
pytest tests/test_static.py -v
```

Expected: 4 passed.

- [ ] **Step 7: Commit**

```bash
cd /home/nvidia/bubbaloop-dash
git add backend/src/dash/static.py backend/src/dash/app.py backend/src/dash/config.py backend/tests/test_static.py
git commit -m "feat(backend): static-file serving with SPA fallback

STATIC_DIR env (Settings.static_dir) points at built frontend/dist.
Real API routes (/api/, /ws/, /healthz, /readyz) win on conflict; unknown
GETs fall through to index.html so the SPA can route client-side. Setting
STATIC_DIR=unset disables static serving (used in dev when vite runs on :5173)."
```

---

## Task 20: Multi-stage Dockerfile

**Files:**
- Create: `/home/nvidia/bubbaloop-dash/Dockerfile`
- Create: `/home/nvidia/bubbaloop-dash/.dockerignore`

- [ ] **Step 1: Create `.dockerignore`**

File contents (`/home/nvidia/bubbaloop-dash/.dockerignore`):

```
# Python
**/__pycache__
**/.venv
**/.pytest_cache
**/.mypy_cache
**/.ruff_cache
**/*.egg-info

# Node
**/node_modules
**/dist
**/.vite

# Editor / OS
**/.vscode
**/.idea
**/.DS_Store

# Git
.git
.gitignore

# Runtime data
.data
*.sqlite*
lance/
.token
.env
```

- [ ] **Step 2: Create `Dockerfile`**

File contents (`/home/nvidia/bubbaloop-dash/Dockerfile`):

```dockerfile
# --- Stage 1: build the frontend ---
FROM node:20-slim AS frontend-build

WORKDIR /build
COPY frontend/package.json frontend/package-lock.json ./
RUN npm ci

COPY frontend/ ./
RUN npm run build
# Output: /build/dist


# --- Stage 2: install backend + assemble runtime image ---
FROM python:3.11-slim AS runtime

ENV PYTHONUNBUFFERED=1 \
    PIP_NO_CACHE_DIR=1 \
    DATA_DIR=/var/lib/bubbaloop-dash \
    STATIC_DIR=/srv/dash-frontend \
    PORT=8000

# uv for fast installs
RUN apt-get update \
 && apt-get install -y --no-install-recommends curl ca-certificates \
 && curl -LsSf https://astral.sh/uv/install.sh | sh \
 && cp /root/.local/bin/uv /usr/local/bin/uv \
 && apt-get purge -y curl \
 && rm -rf /var/lib/apt/lists/*

WORKDIR /srv/dash-backend
COPY backend/pyproject.toml ./
COPY backend/src/ ./src/
RUN uv pip install --system -e .

COPY --from=frontend-build /build/dist /srv/dash-frontend

RUN mkdir -p /var/lib/bubbaloop-dash
VOLUME ["/var/lib/bubbaloop-dash"]

EXPOSE 8000
CMD ["uvicorn", "dash.app:app", "--host", "0.0.0.0", "--port", "8000"]
```

- [ ] **Step 3: Build the image**

```bash
cd /home/nvidia/bubbaloop-dash
docker build -t bubbaloop-dash:dev .
```

Expected: image builds without error. Size on disk roughly ~250–400 MB depending on Python wheel sizes (uvicorn, fastapi, zenoh native).

- [ ] **Step 4: Smoke-test the built image (no zenohd required)**

```bash
docker run --rm -d --name bdash-smoke -p 18000:8000 \
  -e DASHBOARD_TOKEN=smoketest -e ZENOH_ENDPOINT=tcp/127.0.0.1:7447 \
  bubbaloop-dash:dev
sleep 2
curl -sf http://127.0.0.1:18000/healthz
curl -si http://127.0.0.1:18000/readyz | head -1
curl -sf http://127.0.0.1:18000/ | grep -i "dash\|html" | head -3
docker stop bdash-smoke
```

Expected output:
- `/healthz` returns `{"status":"ok"}`
- `/readyz` returns `HTTP/1.1 503 Service Unavailable` (no upstream zenohd reachable in this test)
- `/` returns the lifted dashboard's `index.html`

- [ ] **Step 5: Commit**

```bash
cd /home/nvidia/bubbaloop-dash
git add Dockerfile .dockerignore
git commit -m "feat(docker): multi-stage Dockerfile

Stage 1: node:20-slim builds the frontend (npm ci + npm run build → /build/dist).
Stage 2: python:3.11-slim installs the backend via uv, copies frontend dist
into /srv/dash-frontend, mounts /var/lib/bubbaloop-dash as a volume.
Single image, single CMD: uvicorn dash.app:app."
```

---

## Task 21: docker-compose files

**Files:**
- Create: `/home/nvidia/bubbaloop-dash/docker-compose.yml`
- Create: `/home/nvidia/bubbaloop-dash/docker-compose.dev.yml`

- [ ] **Step 1: Create production `docker-compose.yml`**

File contents (`/home/nvidia/bubbaloop-dash/docker-compose.yml`):

```yaml
services:
  dash:
    image: bubbaloop-dash:latest
    restart: unless-stopped
    ports:
      - "127.0.0.1:8000:8000"
    environment:
      DASHBOARD_TOKEN: ${DASHBOARD_TOKEN:-}
      ZENOH_ENDPOINT: ${ZENOH_ENDPOINT:-tcp/127.0.0.1:7447}
      DATA_DIR: /var/lib/bubbaloop-dash
      STATIC_DIR: /srv/dash-frontend
    volumes:
      - dash_data:/var/lib/bubbaloop-dash
    healthcheck:
      test: ["CMD", "python", "-c", "import urllib.request, sys; sys.exit(0 if urllib.request.urlopen('http://localhost:8000/healthz').status == 200 else 1)"]
      interval: 30s
      timeout: 5s
      retries: 3

volumes:
  dash_data:
```

- [ ] **Step 2: Create dev `docker-compose.dev.yml`**

File contents (`/home/nvidia/bubbaloop-dash/docker-compose.dev.yml`):

```yaml
# Bind-mount sources for hot reload + run uvicorn with --reload.
# Frontend is NOT served by the backend in dev — run `npm run dev` separately
# on the host so vite can hot-reload TSX.

services:
  dash:
    build: .
    image: bubbaloop-dash:dev
    restart: "no"
    ports:
      - "127.0.0.1:8000:8000"
    environment:
      DASHBOARD_TOKEN: ${DASHBOARD_TOKEN:-dev-token}
      ZENOH_ENDPOINT: ${ZENOH_ENDPOINT:-tcp/127.0.0.1:7447}
      DATA_DIR: /var/lib/bubbaloop-dash
      STATIC_DIR: ""  # disable static serving — vite owns the frontend in dev
    volumes:
      - ./backend/src:/srv/dash-backend/src:ro
      - dash_data_dev:/var/lib/bubbaloop-dash
    command: ["uvicorn", "dash.app:app", "--host", "0.0.0.0", "--port", "8000", "--reload"]

volumes:
  dash_data_dev:
```

- [ ] **Step 3: Smoke-test compose**

```bash
cd /home/nvidia/bubbaloop-dash
docker compose -f docker-compose.yml config > /dev/null
docker compose -f docker-compose.dev.yml config > /dev/null
echo "compose files validate"
```

Expected: both `config` invocations exit 0; "compose files validate" prints.

- [ ] **Step 4: Commit**

```bash
cd /home/nvidia/bubbaloop-dash
git add docker-compose.yml docker-compose.dev.yml
git commit -m "feat(docker): production + dev compose files

docker-compose.yml: bind to 127.0.0.1:8000 (reverse-proxy terminates TLS
upstream), persistent dash_data volume, healthcheck via /healthz.
docker-compose.dev.yml: bind-mount backend/src for --reload; disable
STATIC_DIR so vite (run separately on the host) owns the frontend."
```

---

## Task 22: GitHub Actions CI

**Files:**
- Create: `/home/nvidia/bubbaloop-dash/.github/workflows/ci.yml`

- [ ] **Step 1: Create the CI workflow**

```bash
mkdir -p /home/nvidia/bubbaloop-dash/.github/workflows
```

File contents (`/home/nvidia/bubbaloop-dash/.github/workflows/ci.yml`):

```yaml
name: CI

on:
  push:
    branches: [main]
  pull_request:
    branches: [main]

jobs:
  backend:
    runs-on: ubuntu-latest
    defaults:
      run:
        working-directory: backend
    steps:
      - uses: actions/checkout@v4

      - uses: actions/setup-python@v5
        with:
          python-version: "3.11"

      - name: Install uv
        run: pip install uv

      - name: Install backend (dev)
        run: uv pip install --system -e ".[dev]"

      - name: Ruff
        run: ruff check src tests

      - name: Mypy
        run: mypy --strict src/dash

      - name: Pytest (unit only)
        run: pytest -m "not e2e" -v

  frontend:
    runs-on: ubuntu-latest
    defaults:
      run:
        working-directory: frontend
    steps:
      - uses: actions/checkout@v4

      - uses: actions/setup-node@v4
        with:
          node-version: "20"
          cache: "npm"
          cache-dependency-path: frontend/package-lock.json

      - name: Install
        run: npm ci

      - name: Test
        run: npm test

      - name: Build
        run: npm run build

  docker:
    runs-on: ubuntu-latest
    needs: [backend, frontend]
    steps:
      - uses: actions/checkout@v4

      - name: Build image
        run: docker build -t bubbaloop-dash:ci .

      - name: Smoke run
        run: |
          docker run --rm -d --name ci-smoke -p 18000:8000 \
            -e DASHBOARD_TOKEN=ci-tk -e ZENOH_ENDPOINT=tcp/127.0.0.1:7447 \
            bubbaloop-dash:ci
          for i in 1 2 3 4 5; do
            curl -sf http://127.0.0.1:18000/healthz && break
            sleep 1
          done
          curl -sf http://127.0.0.1:18000/healthz | grep -q '"ok"'
          docker stop ci-smoke
```

- [ ] **Step 2: Validate the workflow syntax locally (best effort)**

```bash
cd /home/nvidia/bubbaloop-dash
python -c "import yaml; yaml.safe_load(open('.github/workflows/ci.yml'))"
echo "YAML parses"
```

Expected: "YAML parses" prints, no exception.

- [ ] **Step 3: Commit**

```bash
cd /home/nvidia/bubbaloop-dash
git add .github/workflows/ci.yml
git commit -m "ci: GitHub Actions — backend pytest+ruff+mypy, frontend tests+build, docker smoke

Three jobs in parallel:
- backend: uv install, ruff, mypy --strict, pytest (unit, no e2e)
- frontend: npm ci, npm test, npm run build
- docker: build the image, run it, hit /healthz"
```

---

## Task 23: CLAUDE.md + README polish

**Files:**
- Create: `/home/nvidia/bubbaloop-dash/CLAUDE.md`
- Modify: `/home/nvidia/bubbaloop-dash/README.md`

- [ ] **Step 1: Create CLAUDE.md**

File contents (`/home/nvidia/bubbaloop-dash/CLAUDE.md`):

```markdown
# bubbaloop-dash

Self-hosted backend + dashboard for Bubbaloop. Python (FastAPI) backend, React/TS frontend, single Docker image.

See full design: [`bubbaloop`](https://github.com/kornia/bubbaloop) repo → `docs/superpowers/specs/2026-05-20-bubbaloop-dash-design.md`.

## Structure

- `backend/` — Python 3.11, FastAPI, zenoh-python. Package management via `uv`. Tests via `pytest`.
- `frontend/` — React 18, TypeScript, Vite. Forked from `bubbaloop/dashboard/` (source SHA in `.source-sha`).
- `Dockerfile` — multi-stage (node:20 build → python:3.11-slim runtime).
- `proto/` — vendored protobuf schemas from `bubbaloop-schemas/`. Not present in Phase 1.0; arrives in Phase 1.2 when LanceDB schema needs structured types.

## Local dev

```bash
# Backend
cd backend
uv venv
source .venv/bin/activate
uv pip install -e ".[dev]"
DASHBOARD_TOKEN=dev-token uvicorn dash.app:app --reload

# Frontend (separate terminal)
cd frontend
npm ci
npm run dev
# open http://127.0.0.1:5173
```

## Tests

```bash
# Backend
cd backend
pytest -m "not e2e"

# Frontend
cd frontend
npm test
```

## Conventions

- `uv` for Python deps. `pyproject.toml` is canonical; no `requirements.txt`.
- `ruff` + `mypy --strict` for backend lint/types. All warnings fixed before commit.
- Logging via `structlog` to stdout (JSON). No `print()`.
- Bearer-token auth on every endpoint except `/healthz`. New routes MUST `Depends(require_bearer_token)`.
- New tests for new code. TDD where practical.
- Conventional commits: `feat:`, `fix:`, `chore:`, `docs:`, `refactor:`, `test:`.

## Don't

- Commit `.token`, `.env`, `*.sqlite*`, `lance/`, `dist/`, `node_modules/`.
- Add features not in the current phase. Out-of-phase work needs its own spec.
- Modify `proto/` files by hand — they're vendored. Use `scripts/sync-protos.sh` (lands in Phase 1.2).
```

- [ ] **Step 2: Expand README.md**

Replace `/home/nvidia/bubbaloop-dash/README.md` with:

```markdown
# bubbaloop-dash

Self-hosted backend + dashboard for [Bubbaloop](https://github.com/kornia/bubbaloop). Owns video upload, frame indexing, vector search, and serves the Bubbaloop dashboard UI.

**Status:** Phase 1.0 — skeleton. Login screen + existing Bubbaloop dashboard renders through the new backend. Video upload, indexing, and search arrive in Phases 1.1–1.4.

## Quick start (Docker)

```bash
# Build (or pull from ghcr.io once published)
docker build -t bubbaloop-dash:latest .

# Run
docker run --rm -d \
  --name bubbaloop-dash \
  -p 127.0.0.1:8000:8000 \
  -e DASHBOARD_TOKEN=$(openssl rand -hex 32) \
  -e ZENOH_ENDPOINT=tcp/your-jetson.tailnet.ts.net:7447 \
  -v bubbaloop_dash_data:/var/lib/bubbaloop-dash \
  bubbaloop-dash:latest

# Or via compose
DASHBOARD_TOKEN=$(openssl rand -hex 32) \
  ZENOH_ENDPOINT=tcp/your-jetson.tailnet.ts.net:7447 \
  docker compose up -d
```

Then point a reverse proxy (Caddy / Cloudflare Tunnel / Traefik) at `127.0.0.1:8000` with TLS, and browse to it. Paste the token at the login screen.

## Configuration

| Env var | Default | Purpose |
|---|---|---|
| `DASHBOARD_TOKEN` | _(generated)_ | Bearer token clients send in `Authorization: Bearer ...`. If unset, generated on first run and persisted to `${DATA_DIR}/.token`. |
| `ZENOH_ENDPOINT` | `tcp/127.0.0.1:7447` | TCP endpoint of the user's `zenohd` (Bubbaloop daemon). Typically a Tailscale/VPN address. |
| `DATA_DIR` | `/var/lib/bubbaloop-dash` | Persistent state (token file, jobs DB later, LanceDB later, uploaded blobs later). |
| `STATIC_DIR` | `/srv/dash-frontend` | Path to built frontend. Set to empty string to disable static serving. |
| `PORT` | `8000` | TCP port FastAPI listens on inside the container. |
| `SWEEP_INTERVAL_S` | `3600` | Integrity sweeper interval (only relevant Phase 1.1+). |
| `JOB_CONCURRENCY` | `1` | Max concurrent indexing jobs (only relevant Phase 1.2+). |

## Develop locally

See [`CLAUDE.md`](./CLAUDE.md) for backend + frontend dev workflows, conventions, and what not to do.

## Design

Full design lives in the [`bubbaloop`](https://github.com/kornia/bubbaloop) repo at `docs/superpowers/specs/2026-05-20-bubbaloop-dash-design.md`. The implementation plan for this phase: `docs/superpowers/plans/2026-05-20-bubbaloop-dash-phase-1.0-skeleton.md`.
```

- [ ] **Step 3: Commit**

```bash
cd /home/nvidia/bubbaloop-dash
git add CLAUDE.md README.md
git commit -m "docs: CLAUDE.md + expanded README

CLAUDE.md mirrors bubbaloop's style: structure, dev commands, conventions,
don'ts. README adds quick-start (docker + compose), env var table, links
to design + plan in the bubbaloop repo."
```

---

## Task 24: Create the GitHub repository + push

**Files:** (no new local files)

- [ ] **Step 1: Confirm `gh` is authenticated**

```bash
gh auth status
```

Expected: prints "Logged in to github.com as <user>".

- [ ] **Step 2: Create the remote repository**

```bash
cd /home/nvidia/bubbaloop-dash
gh repo create kornia/bubbaloop-dash \
  --public \
  --description "Self-hosted backend + dashboard for Bubbaloop" \
  --source . \
  --remote origin \
  --push
```

Expected: prints the new repo URL and pushes the local `main` branch.

If the `kornia` org is wrong for the executing user, substitute the right namespace (e.g. `--source . --remote origin` without `kornia/` prefix to create under the user's own namespace, then `gh repo edit --add-topic ...` as needed).

- [ ] **Step 3: Confirm the push**

```bash
gh repo view kornia/bubbaloop-dash --json url,defaultBranchRef
```

Expected: JSON with `"url": "https://github.com/kornia/bubbaloop-dash"` and `"defaultBranchRef": {"name": "main"}`.

- [ ] **Step 4: Check that CI runs**

```bash
gh run list --repo kornia/bubbaloop-dash --limit 5
```

Expected: at least one workflow run kicked off (status `in_progress` or `queued`).

- [ ] **Step 5: No commit needed — push already happened in Step 2.**

---

## Task 25: End-to-end manual smoke test

**Files:** (no new files — verification only)

This is a manual verification step. It is not automated, but it is the success criterion for Phase 1.0.

- [ ] **Step 1: Run the image with a real Zenoh endpoint**

```bash
# DASHBOARD_TOKEN: pick or generate one; remember it for the login.
# ZENOH_ENDPOINT: the Tailscale address of your Bubbaloop daemon's zenohd.
docker run --rm --name dash-smoke -p 8000:8000 \
  -e DASHBOARD_TOKEN=manual-smoke-tk \
  -e ZENOH_ENDPOINT=tcp/your-jetson.tailnet.ts.net:7447 \
  -v dash_data:/var/lib/bubbaloop-dash \
  bubbaloop-dash:latest
```

- [ ] **Step 2: From a separate shell, verify `/healthz` and `/readyz`**

```bash
curl -sf http://127.0.0.1:8000/healthz
curl -si http://127.0.0.1:8000/readyz | head -5
```

Expected:
- `/healthz` → `{"status":"ok"}`
- `/readyz` → `HTTP/1.1 200 OK` if your daemon's zenohd is reachable, else 503 with `"checks": {"zenoh": "connecting" | "not_ready"}`

- [ ] **Step 3: Open the dashboard in a browser**

Navigate to `http://127.0.0.1:8000/` in a browser.

Expected:
- The Login screen renders with a token input.
- Paste `manual-smoke-tk` (or whatever you set for `DASHBOARD_TOKEN`); click Sign in.
- After sign-in, the existing dashboard (Dashboard / Loop / Chat tabs) loads.
- The connection-status indicator in the top-right shows the WSS endpoint `ws://127.0.0.1:8000/ws/zenoh` and a green "Connected" dot.
- If your daemon is publishing camera frames, opening the Dashboard tab and selecting your camera should render live video.

- [ ] **Step 4: Stop the container**

```bash
docker stop dash-smoke
```

- [ ] **Step 5: Record the smoke-test outcome in a brief release-notes commit**

```bash
cd /home/nvidia/bubbaloop-dash
cat > SMOKE_NOTES.md << 'EOF'
# Phase 1.0 manual smoke test

- [x] /healthz returns 200 with {"status":"ok"}
- [x] /readyz returns 200 when zenohd is reachable, 503 otherwise
- [x] Login screen renders, accepts a token, transitions to App
- [x] Existing Dashboard / Loop / Chat views render
- [x] WSS proxy delivers live samples (if a daemon is publishing)

Tested with bubbaloop-dash:latest, ZENOH_ENDPOINT pointing at a Tailscale-reachable Jetson.
EOF
git add SMOKE_NOTES.md
git commit -m "docs: Phase 1.0 manual smoke notes"
```

- [ ] **Step 6: Tag the release**

```bash
cd /home/nvidia/bubbaloop-dash
git tag -a v0.1.0-alpha -m "Phase 1.0 — skeleton: backend, auth, Zenoh-WS proxy, frontend lift, Docker, CI"
git push origin v0.1.0-alpha
```

Expected: tag pushed; visible at `https://github.com/kornia/bubbaloop-dash/releases/tag/v0.1.0-alpha`.

---

## Self-review checklist (run before declaring done)

1. **Spec coverage:** Skim spec Section 6 phase 1.0 row. Every bullet there must be implemented by a task above.
   - Repo scaffolding → Tasks 1–2 ✓
   - Dockerfile + dev compose → Tasks 20–21 ✓
   - CI → Task 22 ✓
   - FastAPI skeleton + /healthz + /readyz → Tasks 5–6 ✓
   - Bearer auth → Tasks 7–8 ✓
   - Frontend lift + Login + AuthContext → Tasks 13, 15–18 ✓
   - Zenoh-WSS proxy with multiplex → Tasks 9–12 ✓
   - End-state ("log in, see existing dashboard render") → Task 25 manual smoke ✓

2. **Placeholders:** Search the plan for `TBD`, `TODO`, `fill in`, `similar to`. **None present.** Every step contains the actual code/command an engineer needs.

3. **Type consistency:** `ZenohSession`, `SubscriptionRegistry`, `ApiClient`, `AuthContext` names are stable across all tasks that reference them. `dashboard_token` field on `Settings` is used identically by `auth.py` and `tokens.py`. `STATIC_DIR` env var name matches `static_dir` field consistently. ✓

4. **Out-of-phase scope:** No tasks touch video upload, LanceDB, search endpoints, frame storage, or PyAV — those are Phases 1.1–1.3. ✓

If any of the above turns up an issue during execution, fix the plan inline (don't try to remember it) and continue.

---

## Execution handoff

Plan complete and saved to `docs/superpowers/plans/2026-05-20-bubbaloop-dash-phase-1.0-skeleton.md`. Two execution options:

**1. Subagent-Driven (recommended)** — fresh subagent per task, review between tasks, fast iteration. Best when you want me to drive execution end-to-end with checkpoints you can interrupt or redirect at each task boundary.

**2. Inline Execution** — execute tasks in this session using `executing-plans`, batch execution with checkpoints. Best when you want a contiguous session log and don't mind a long single conversation.

**Which approach?**
