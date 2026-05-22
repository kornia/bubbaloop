# Map SLAM-backed top-down view + `SlamMapSource` interface — Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Replace the Map tab's user-uploaded floorplan with a thin reader against an upstream SLAM map source. Dash never owns the map. Sensor placements migrate from pixel coords to world meters.

**Architecture:** Backend exposes `SlamMapSource` Protocol abstracting where the occupancy grid comes from; ships with `LocalFileSource` (reads `occupancy.png`+`occupancy.yaml` from disk for dev) and `RemoteSlamDbSource` (stub for the SLAM team's future DB service). API `GET /api/map/slam{,/meta}` is read-only. Frontend `useSlamMap()` polls every 10s; when meta returns 200 the Map tab renders the occupancy PNG as a Konva Image background, otherwise renders a meters reference grid. Sensors render on top in world meters always.

**Tech Stack:** Backend: FastAPI + Pydantic + SQLite (existing) + **PyYAML** (new dep). Frontend: React 18 + Vite + react-konva (existing).

**Repo:** `/home/nvidia/bubbaloop-dash`
**Branch:** `feat/library-map-redesign` (continues the existing branch)
**Spec:** `docs/superpowers/specs/2026-05-22-map-slam-source-design.md`

---

## File Structure

### Backend — new
- `backend/src/dash/slam/__init__.py` — package marker
- `backend/src/dash/slam/types.py` — `SlamMapMeta` Pydantic model
- `backend/src/dash/slam/source.py` — `SlamMapSource` Protocol + `LocalFileSource` + `RemoteSlamDbSource`
- `backend/tests/test_map_slam.py` — unit + API tests

### Backend — modified
- `backend/pyproject.toml` — add `pyyaml>=6.0,<7.0` to dependencies
- `backend/src/dash/config.py` — 4 new settings (`slam_map_source`, `slam_map_local_path`, `slam_map_db_url`, `slam_map_db_token`)
- `backend/src/dash/factory.py` — wire `app.state.slam_map_source`
- `backend/src/dash/api/map.py` — add `/api/map/slam{,/meta}`; remove all floorplan-upload code; update placement DTOs to use `range` (meters)
- `backend/src/dash/db/sqlite.py` — `_ensure_meter_columns` migration + `SensorPlacementRow.range` field + CRUD method signatures using `range`
- `backend/tests/test_sensor_placements.py` — migration test + field-name updates
- `backend/tests/test_map_placements.py` — DTO field updates
- `backend/tests/test_config.py` — new settings defaults + overrides

### Backend — deleted
- `backend/tests/test_map_floorplan.py`

### Frontend — new
- `frontend/src/hooks/useSlamMap.ts`
- `frontend/src/hooks/__tests__/useSlamMap.test.ts`
- `frontend/src/components/SlamLayer.tsx`
- `frontend/src/components/__tests__/SlamLayer.test.tsx`
- `frontend/src/components/MetersGrid.tsx`
- `frontend/src/components/__tests__/MetersGrid.test.tsx`

### Frontend — modified
- `frontend/src/components/MapTab.tsx` — drop FloorplanUploader; use `useSlamMap`; route background to MapCanvas
- `frontend/src/components/MapCanvas.tsx` — accept `background` prop; apply Stage `scale = pxPerMeter`
- `frontend/src/components/SensorMarker.tsx` — `range_pixels` prop → `range` (meters)
- `frontend/src/components/FOVCone.tsx` — `range_pixels` prop → `range` (meters)
- `frontend/src/components/RotationHandle.tsx` — `range_pixels` prop → `range` (meters)
- `frontend/src/components/__tests__/MapTab.test.tsx` — drop floorplan tests; add background-mode tests
- `frontend/src/components/__tests__/MapCanvas.test.tsx` — drop floorplan tests; add background-mode tests
- `frontend/src/components/__tests__/SensorMarker.test.tsx` — `range` prop
- `frontend/src/components/__tests__/RotationHandle.test.tsx` — `range` prop

### Frontend — deleted
- `frontend/src/components/FloorplanUploader.tsx`
- `frontend/src/components/__tests__/FloorplanUploader.test.tsx`

---

## Task 1: Backend — install PyYAML + scaffold `dash.slam` package

**Files:**
- Modify: `backend/pyproject.toml`
- Create: `backend/src/dash/slam/__init__.py`
- Create: `backend/src/dash/slam/types.py`
- Create: `backend/tests/test_map_slam.py`

- [ ] **Step 1: Write the failing test**

```python
# backend/tests/test_map_slam.py
"""SLAM map source + API tests. v1 ships LocalFileSource for dev/test;
RemoteSlamDbSource is a stub until the SLAM team publishes their wire format."""
from __future__ import annotations

from pathlib import Path

import pytest


def test_slam_map_meta_shape() -> None:
    """SlamMapMeta exposes the fields the API contract documents."""
    from dash.slam.types import SlamMapMeta

    m = SlamMapMeta(
        width=200,
        height=160,
        resolution=0.05,
        origin_x=-5.0,
        origin_y=-4.0,
        updated_at=1716290400000,
        source="local-file",
    )
    assert m.width == 200
    assert m.height == 160
    assert m.resolution == 0.05
    assert m.origin_x == -5.0
    assert m.origin_y == -4.0
    assert m.updated_at == 1716290400000
    assert m.source == "local-file"
```

- [ ] **Step 2: Run test to verify it fails**

Run: `cd /home/nvidia/bubbaloop-dash/backend && .venv/bin/pytest tests/test_map_slam.py::test_slam_map_meta_shape -v`
Expected: `ModuleNotFoundError: No module named 'dash.slam'`

- [ ] **Step 3: Install PyYAML**

Run: `cd /home/nvidia/bubbaloop-dash/backend && .venv/bin/pip install 'pyyaml>=6.0,<7.0'`
Expected: successful install (pyyaml is pure-Python on Jetson aarch64; no compile step).

- [ ] **Step 4: Add PyYAML to pyproject.toml**

Open `backend/pyproject.toml` and add to the `dependencies` list (alphabetical order). The line:

```toml
    "pyyaml>=6.0,<7.0",
```

goes between `"pydantic-settings…"` and `"structlog…"` (wherever that range fits alphabetically among the current deps).

- [ ] **Step 5: Create package marker + types module**

`backend/src/dash/slam/__init__.py`:

```python
"""SLAM map source — read-only adapters over upstream map providers.

Dash is a thin reader against the SLAM system; it never owns the map.
`SlamMapSource` is the interface; `LocalFileSource` reads from disk for
dev/testing, `RemoteSlamDbSource` proxies an upstream DB service in prod
(stubbed until the SLAM team publishes their contract)."""
```

`backend/src/dash/slam/types.py`:

```python
from __future__ import annotations

from pydantic import BaseModel, Field


class SlamMapMeta(BaseModel):
    """Metadata for a SLAM-derived occupancy grid.

    Field semantics follow the ROS Nav2 map.yaml convention:
      - `resolution` is meters per pixel.
      - `origin_x/y` are the world meter coordinates of the bottom-left pixel.
      - Top-left pixel is at (origin_x, origin_y + height * resolution).
    """

    width: int = Field(description="PNG width in pixels")
    height: int = Field(description="PNG height in pixels")
    resolution: float = Field(description="meters per pixel")
    origin_x: float = Field(description="world meters of the bottom-left pixel")
    origin_y: float = Field(description="world meters of the bottom-left pixel")
    updated_at: int = Field(description="ms epoch of last refresh")
    source: str = Field(description="provenance label: 'local-file' or 'slam-db'")
```

- [ ] **Step 6: Run test to verify it passes**

Run: `cd /home/nvidia/bubbaloop-dash/backend && .venv/bin/pytest tests/test_map_slam.py::test_slam_map_meta_shape -v`
Expected: PASS

- [ ] **Step 7: Commit**

```bash
cd /home/nvidia/bubbaloop-dash
git add backend/pyproject.toml backend/src/dash/slam/__init__.py backend/src/dash/slam/types.py backend/tests/test_map_slam.py
git commit -m "feat(slam): scaffold dash.slam package + SlamMapMeta type"
```

---

## Task 2: `SlamMapSource` Protocol + `LocalFileSource`

**Files:**
- Create: `backend/src/dash/slam/source.py`
- Modify: `backend/tests/test_map_slam.py` — add LocalFileSource tests

- [ ] **Step 1: Write the failing test**

Append to `backend/tests/test_map_slam.py`:

```python
import io

import yaml
from PIL import Image


def _write_sample_map(path: Path, *, width: int = 200, height: int = 160) -> None:
    """Helper: write a minimal occupancy.png + occupancy.yaml in ROS Nav2 format."""
    path.mkdir(parents=True, exist_ok=True)
    # Grey-on-grey PNG so file parses successfully.
    img = Image.new("L", (width, height), color=128)
    img.save(path / "occupancy.png", format="PNG")
    yaml_payload = {
        "image": "occupancy.png",
        "resolution": 0.05,
        "origin": [-5.0, -4.0, 0.0],
        "negate": 0,
        "occupied_thresh": 0.65,
        "free_thresh": 0.196,
    }
    (path / "occupancy.yaml").write_text(yaml.safe_dump(yaml_payload))


@pytest.mark.asyncio
async def test_local_file_source_returns_meta_when_files_present(tmp_path: Path) -> None:
    from dash.slam.source import LocalFileSource

    _write_sample_map(tmp_path, width=200, height=160)
    src = LocalFileSource(tmp_path)
    meta = await src.get_meta()
    assert meta is not None
    assert meta.width == 200
    assert meta.height == 160
    assert meta.resolution == 0.05
    assert meta.origin_x == -5.0
    assert meta.origin_y == -4.0
    assert meta.source == "local-file"
    # updated_at is the mtime of the PNG in ms.
    assert meta.updated_at > 0


@pytest.mark.asyncio
async def test_local_file_source_returns_none_when_missing(tmp_path: Path) -> None:
    from dash.slam.source import LocalFileSource

    src = LocalFileSource(tmp_path)  # empty directory
    assert await src.get_meta() is None
    assert await src.get_image() is None


@pytest.mark.asyncio
async def test_local_file_source_returns_image_bytes(tmp_path: Path) -> None:
    from dash.slam.source import LocalFileSource

    _write_sample_map(tmp_path)
    src = LocalFileSource(tmp_path)
    image_bytes = await src.get_image()
    assert image_bytes is not None
    assert image_bytes.startswith(b"\x89PNG")  # PNG magic


@pytest.mark.asyncio
async def test_local_file_source_returns_none_on_malformed_yaml(tmp_path: Path) -> None:
    """A broken sidecar should NOT 500 the endpoint — return None and log."""
    from dash.slam.source import LocalFileSource

    tmp_path.mkdir(parents=True, exist_ok=True)
    Image.new("L", (10, 10)).save(tmp_path / "occupancy.png", format="PNG")
    (tmp_path / "occupancy.yaml").write_text("{not valid yaml: [")
    src = LocalFileSource(tmp_path)
    assert await src.get_meta() is None
```

- [ ] **Step 2: Run test to verify it fails**

Run: `cd /home/nvidia/bubbaloop-dash/backend && .venv/bin/pytest tests/test_map_slam.py -v`
Expected: 4 new tests fail with `ModuleNotFoundError: No module named 'dash.slam.source'`. Existing `test_slam_map_meta_shape` still passes.

- [ ] **Step 3: Implement `source.py`**

Create `backend/src/dash/slam/source.py`:

```python
from __future__ import annotations

import logging
from pathlib import Path
from typing import Protocol

import yaml
from PIL import Image

from dash.slam.types import SlamMapMeta

_log = logging.getLogger(__name__)


class SlamMapSource(Protocol):
    """Read-only adapter over a SLAM map provider.

    Returning `None` means "no map currently available" — callers translate
    that to a 404 at the API layer. Sources never raise for the no-map case;
    raises are reserved for genuine programmer errors (bad type, bad config).
    """

    async def get_meta(self) -> SlamMapMeta | None: ...
    async def get_image(self) -> bytes | None: ...


class LocalFileSource:
    """Reads `occupancy.png` + `occupancy.yaml` from a configured directory.

    The YAML uses the ROS Nav2 map server format. Missing files OR a
    malformed YAML both produce `None` (the dev forgot to drop a map in —
    not an error to propagate to the user)."""

    def __init__(self, base_path: Path) -> None:
        self._png = base_path / "occupancy.png"
        self._yaml = base_path / "occupancy.yaml"

    async def get_meta(self) -> SlamMapMeta | None:
        if not self._png.is_file() or not self._yaml.is_file():
            return None
        try:
            with self._yaml.open() as f:
                y = yaml.safe_load(f) or {}
            if not isinstance(y, dict):
                _log.warning("slam.local_file_source: yaml root is not a dict")
                return None
            with Image.open(self._png) as img:
                width, height = img.size
            origin = y.get("origin", [0.0, 0.0, 0.0])
            return SlamMapMeta(
                width=width,
                height=height,
                resolution=float(y.get("resolution", 0.05)),
                origin_x=float(origin[0]) if len(origin) > 0 else 0.0,
                origin_y=float(origin[1]) if len(origin) > 1 else 0.0,
                updated_at=int(self._png.stat().st_mtime * 1000),
                source="local-file",
            )
        except (yaml.YAMLError, OSError, ValueError, TypeError, IndexError) as exc:
            _log.warning("slam.local_file_source.get_meta failed: %s", exc)
            return None

    async def get_image(self) -> bytes | None:
        if not self._png.is_file():
            return None
        try:
            return self._png.read_bytes()
        except OSError as exc:
            _log.warning("slam.local_file_source.get_image failed: %s", exc)
            return None
```

- [ ] **Step 4: Run tests to verify pass**

Run: `cd /home/nvidia/bubbaloop-dash/backend && .venv/bin/pytest tests/test_map_slam.py -v`
Expected: 5 passed.

- [ ] **Step 5: Commit**

```bash
cd /home/nvidia/bubbaloop-dash
git add backend/src/dash/slam/source.py backend/tests/test_map_slam.py
git commit -m "feat(slam): SlamMapSource Protocol + LocalFileSource impl"
```

---

## Task 3: `RemoteSlamDbSource` stub

**Files:**
- Modify: `backend/src/dash/slam/source.py`
- Modify: `backend/tests/test_map_slam.py`

- [ ] **Step 1: Write the failing test**

Append to `backend/tests/test_map_slam.py`:

```python
@pytest.mark.asyncio
async def test_remote_slam_db_source_stub_returns_none() -> None:
    """Stub returns None until the SLAM team publishes their service contract.
    This test guards the placeholder so a future accidental implementation
    can't ship without updating tests."""
    from dash.slam.source import RemoteSlamDbSource

    src = RemoteSlamDbSource(endpoint="https://example.invalid/slam", token="dummy")
    assert await src.get_meta() is None
    assert await src.get_image() is None
```

- [ ] **Step 2: Run test to verify it fails**

Run: `cd /home/nvidia/bubbaloop-dash/backend && .venv/bin/pytest tests/test_map_slam.py::test_remote_slam_db_source_stub_returns_none -v`
Expected: `ImportError: cannot import name 'RemoteSlamDbSource' from 'dash.slam.source'`

- [ ] **Step 3: Add the stub**

Append to `backend/src/dash/slam/source.py`:

```python
class RemoteSlamDbSource:
    """HTTP proxy against the upstream SLAM team's DB service.

    STUB — always returns None until the SLAM team publishes their service
    contract. When that happens:
      1. Fill in `get_meta()` to GET <endpoint>/maps/current/meta and parse
         the JSON into SlamMapMeta.
      2. Fill in `get_image()` to GET <endpoint>/maps/current/image and
         return the response body bytes.
      3. Bearer-auth via self._token if their contract requires it.
      4. Add httpx as a runtime dep (currently only in dev deps).
    """

    def __init__(self, endpoint: str, token: str | None) -> None:
        self._endpoint = endpoint
        self._token = token

    async def get_meta(self) -> SlamMapMeta | None:
        return None

    async def get_image(self) -> bytes | None:
        return None
```

- [ ] **Step 4: Run test to verify pass**

Run: `cd /home/nvidia/bubbaloop-dash/backend && .venv/bin/pytest tests/test_map_slam.py -v`
Expected: 6 passed.

- [ ] **Step 5: Commit**

```bash
cd /home/nvidia/bubbaloop-dash
git add backend/src/dash/slam/source.py backend/tests/test_map_slam.py
git commit -m "feat(slam): RemoteSlamDbSource stub for the upstream SLAM DB service"
```

---

## Task 4: Settings additions

**Files:**
- Modify: `backend/src/dash/config.py`
- Modify: `backend/tests/test_config.py`

- [ ] **Step 1: Write the failing test**

Append to `backend/tests/test_config.py`:

```python
def test_settings_slam_defaults(tmp_path: Path, monkeypatch: pytest.MonkeyPatch) -> None:
    monkeypatch.setenv("DATA_DIR", str(tmp_path))
    monkeypatch.delenv("SLAM_MAP_SOURCE", raising=False)
    monkeypatch.delenv("SLAM_MAP_LOCAL_PATH", raising=False)
    monkeypatch.delenv("SLAM_MAP_DB_URL", raising=False)
    monkeypatch.delenv("SLAM_MAP_DB_TOKEN", raising=False)
    s = Settings()
    assert s.slam_map_source == "local"
    # Default path is under DATA_DIR.
    assert str(s.slam_map_local_path).endswith("maps/slam/dev")
    assert s.slam_map_db_url is None
    assert s.slam_map_db_token is None


def test_settings_slam_env_overrides(tmp_path: Path, monkeypatch: pytest.MonkeyPatch) -> None:
    monkeypatch.setenv("DATA_DIR", str(tmp_path))
    monkeypatch.setenv("SLAM_MAP_SOURCE", "remote")
    monkeypatch.setenv("SLAM_MAP_LOCAL_PATH", "/tmp/elsewhere")
    monkeypatch.setenv("SLAM_MAP_DB_URL", "https://slam.example/v1")
    monkeypatch.setenv("SLAM_MAP_DB_TOKEN", "secret-xyz")
    s = Settings()
    assert s.slam_map_source == "remote"
    assert str(s.slam_map_local_path) == "/tmp/elsewhere"
    assert s.slam_map_db_url == "https://slam.example/v1"
    assert s.slam_map_db_token == "secret-xyz"
```

- [ ] **Step 2: Run tests to verify failure**

Run: `cd /home/nvidia/bubbaloop-dash/backend && .venv/bin/pytest tests/test_config.py -k slam -v`
Expected: `AttributeError: 'Settings' object has no attribute 'slam_map_source'`.

- [ ] **Step 3: Add the four settings fields**

In `backend/src/dash/config.py` find the `Settings` class and append before the `_empty_str_to_none` validator (next to the other path/option fields):

```python
    # SLAM map source — see backend/src/dash/slam/source.py.
    # "local" reads from slam_map_local_path; "remote" proxies the upstream
    # SLAM DB service (stub until that service exists).
    slam_map_source: str = Field(default="local", pattern="^(local|remote)$")
    slam_map_local_path: Path = Field(
        default_factory=lambda: Path("/var/lib/bubbaloop-dash/maps/slam/dev")
    )
    slam_map_db_url: str | None = Field(default=None)
    slam_map_db_token: str | None = Field(default=None)
```

The `slam_map_local_path` default starts as a fixed absolute path; that matches the prod `data_dir` (`/var/lib/bubbaloop-dash`). For dev mode the env var `SLAM_MAP_LOCAL_PATH` is set by `scripts/dev.sh` (see Task 5).

- [ ] **Step 4: Run tests to verify pass**

Run: `cd /home/nvidia/bubbaloop-dash/backend && .venv/bin/pytest tests/test_config.py -v`
Expected: all pass.

- [ ] **Step 5: Commit**

```bash
cd /home/nvidia/bubbaloop-dash
git add backend/src/dash/config.py backend/tests/test_config.py
git commit -m "feat(config): slam_map_source + slam_map_local_path + slam_map_db_{url,token}"
```

---

## Task 5: Wire `SlamMapSource` into app startup + update `scripts/dev.sh`

**Files:**
- Modify: `backend/src/dash/factory.py`
- Modify: `scripts/dev.sh`

- [ ] **Step 1: Read existing factory startup**

Run: `grep -n "app.state\|lifespan\|on_startup" backend/src/dash/factory.py | head -20`

You'll see existing `app.state.db = ...`, `app.state.storage = ...`, etc. The new line goes alongside these.

- [ ] **Step 2: Wire SlamMapSource into app.state**

In `backend/src/dash/factory.py`, near the other `app.state.*` assignments, add:

```python
from pathlib import Path

from dash.slam.source import LocalFileSource, RemoteSlamDbSource, SlamMapSource

# Inside lifespan / app construction, after settings is bound:
slam_source: SlamMapSource
if settings.slam_map_source == "remote":
    slam_source = RemoteSlamDbSource(
        endpoint=settings.slam_map_db_url or "",
        token=settings.slam_map_db_token,
    )
else:
    slam_source = LocalFileSource(Path(settings.slam_map_local_path))
app.state.slam_map_source = slam_source
```

Insert the `import` lines next to the existing `from dash.<module>` imports (alphabetical). Insert the wiring block immediately after the existing `app.state.db = …` assignment.

- [ ] **Step 3: Verify the app still boots**

Run: `cd /home/nvidia/bubbaloop-dash/backend && .venv/bin/pytest tests/ -q -k "not test_complete and not test_retry_video_re_queues"`
Expected: full suite passes (modulo the two known pre-existing flakies under heavy parallelism — those are explicitly skipped).

- [ ] **Step 4: Update `scripts/dev.sh` to export `SLAM_MAP_LOCAL_PATH`**

Open `scripts/dev.sh` and find the `DEV_DATA_DIR` line near the top. Below it, add:

```bash
SLAM_MAP_LOCAL_PATH="${SLAM_MAP_LOCAL_PATH:-$DEV_DATA_DIR/maps/slam/dev}"
mkdir -p "$SLAM_MAP_LOCAL_PATH"
```

And in the backend launch env block (where `DISABLE_AUTH=1`, `DATA_DIR=`, etc. are exported), add:

```bash
  SLAM_MAP_LOCAL_PATH="$SLAM_MAP_LOCAL_PATH" \
```

- [ ] **Step 5: Commit**

```bash
cd /home/nvidia/bubbaloop-dash
git add backend/src/dash/factory.py scripts/dev.sh
git commit -m "feat(slam): wire SlamMapSource into app.state + dev.sh env"
```

---

## Task 6: API `GET /api/map/slam/meta`

**Files:**
- Modify: `backend/src/dash/api/map.py`
- Modify: `backend/tests/test_map_slam.py`

- [ ] **Step 1: Write the failing tests**

Append to `backend/tests/test_map_slam.py`. The existing `client` fixture in `backend/tests/conftest.py` (or similar) gives a `TestClient` with `app.state.settings.data_dir` already pointed at a tmp dir; we extend that by populating the configured slam path.

```python
def _seed_slam_map_for_client(client, width: int = 200, height: int = 160) -> Path:
    """Write a sample occupancy map at the configured slam_map_local_path
    so the LocalFileSource attached to the client's app picks it up."""
    settings = client.app.state.settings
    target = settings.slam_map_local_path
    _write_sample_map(Path(target), width=width, height=height)
    return Path(target)


def test_api_get_slam_meta_404_when_no_map(client) -> None:
    r = client.get("/api/map/slam/meta")
    assert r.status_code == 404
    body = r.json()
    assert body.get("detail", {}).get("reason") == "no_slam_map" or body.get("detail") == "no_slam_map"


def test_api_get_slam_meta_200_when_map_present(client) -> None:
    _seed_slam_map_for_client(client, width=200, height=160)
    r = client.get("/api/map/slam/meta")
    assert r.status_code == 200
    body = r.json()
    assert body["width"] == 200
    assert body["height"] == 160
    assert body["resolution"] == 0.05
    assert body["origin_x"] == -5.0
    assert body["origin_y"] == -4.0
    assert body["source"] == "local-file"
    assert isinstance(body["updated_at"], int)
```

If the test fixture in `conftest.py` doesn't already point `slam_map_local_path` at a tmp dir, also append a conftest helper that does. The simplest pattern is to amend the existing client fixture if it owns the tmp dir.

Inspect with: `grep -n "slam_map_local_path\|DATA_DIR\|tmp_path" backend/tests/conftest.py` and either reuse the existing tmp_path or scope a new one for slam.

- [ ] **Step 2: Run tests to verify failure**

Run: `cd /home/nvidia/bubbaloop-dash/backend && .venv/bin/pytest tests/test_map_slam.py -k api_get_slam_meta -v`
Expected: 404 for both (route does not exist yet).

- [ ] **Step 3: Implement the meta endpoint**

Open `backend/src/dash/api/map.py`. Add imports (if missing):

```python
from dash.slam.types import SlamMapMeta
```

Append the route (next to the existing placement routes):

```python
@router.get(
    "/slam/meta",
    response_model=SlamMapMeta,
    dependencies=[Depends(require_bearer_token)],
)
async def get_slam_meta(request: Request) -> SlamMapMeta:
    source = request.app.state.slam_map_source
    meta = await source.get_meta()
    if meta is None:
        raise HTTPException(status_code=404, detail={"reason": "no_slam_map"})
    return meta
```

- [ ] **Step 4: Run tests to verify pass**

Run: `cd /home/nvidia/bubbaloop-dash/backend && .venv/bin/pytest tests/test_map_slam.py -k api_get_slam_meta -v`
Expected: 2 passed.

- [ ] **Step 5: Commit**

```bash
cd /home/nvidia/bubbaloop-dash
git add backend/src/dash/api/map.py backend/tests/test_map_slam.py
git commit -m "feat(slam): GET /api/map/slam/meta"
```

---

## Task 7: API `GET /api/map/slam` (image bytes)

**Files:**
- Modify: `backend/src/dash/api/map.py`
- Modify: `backend/tests/test_map_slam.py`

- [ ] **Step 1: Write the failing tests**

Append to `backend/tests/test_map_slam.py`:

```python
def test_api_get_slam_image_404_when_no_map(client) -> None:
    r = client.get("/api/map/slam")
    assert r.status_code == 404


def test_api_get_slam_image_returns_png_bytes(client) -> None:
    _seed_slam_map_for_client(client)
    r = client.get("/api/map/slam")
    assert r.status_code == 200
    assert r.headers["content-type"] == "image/png"
    assert r.content.startswith(b"\x89PNG")
    # ETag based on updated_at for cheap polling.
    assert r.headers.get("etag") is not None
```

- [ ] **Step 2: Run tests to verify failure**

Run: `cd /home/nvidia/bubbaloop-dash/backend && .venv/bin/pytest tests/test_map_slam.py -k api_get_slam_image -v`
Expected: 404 from FastAPI (route missing).

- [ ] **Step 3: Implement the image endpoint**

Add to `backend/src/dash/api/map.py`:

```python
from fastapi.responses import Response as FastAPIResponse


@router.get(
    "/slam",
    dependencies=[Depends(require_bearer_token)],
)
async def get_slam_image(request: Request) -> FastAPIResponse:
    source = request.app.state.slam_map_source
    meta = await source.get_meta()
    if meta is None:
        raise HTTPException(status_code=404, detail={"reason": "no_slam_map"})
    image_bytes = await source.get_image()
    if image_bytes is None:
        raise HTTPException(status_code=404, detail={"reason": "no_slam_map"})
    return FastAPIResponse(
        content=image_bytes,
        media_type="image/png",
        headers={
            "ETag": f'W/"{meta.updated_at}"',
            "Cache-Control": "no-cache, must-revalidate",
        },
    )
```

- [ ] **Step 4: Run tests to verify pass**

Run: `cd /home/nvidia/bubbaloop-dash/backend && .venv/bin/pytest tests/test_map_slam.py -v`
Expected: all tests in the file pass.

- [ ] **Step 5: Commit**

```bash
cd /home/nvidia/bubbaloop-dash
git add backend/src/dash/api/map.py backend/tests/test_map_slam.py
git commit -m "feat(slam): GET /api/map/slam (image bytes with ETag)"
```

---

## Task 8: Migrate `sensor_placements` — add `range` column + backfill in meters

**Files:**
- Modify: `backend/src/dash/db/sqlite.py`
- Modify: `backend/tests/test_sensor_placements.py`

- [ ] **Step 1: Write the failing test**

Append to `backend/tests/test_sensor_placements.py`:

```python
def test_meter_columns_migration_backfills_from_pixels(tmp_path: Path) -> None:
    """When a database created against the v0 schema (range_pixels, no range)
    is opened with the new code, _ensure_meter_columns must:
      - add the `range` column
      - backfill range = range_pixels / 20.0
      - rescale x/y/z to meters by /20.0
    """
    import sqlite3

    db_path = tmp_path / "old.db"
    # Bootstrap a v0 schema row.
    conn = sqlite3.connect(str(db_path))
    conn.execute(
        """CREATE TABLE sensor_placements (
            id TEXT PRIMARY KEY,
            node_id TEXT NOT NULL UNIQUE,
            node_kind TEXT NOT NULL,
            x REAL NOT NULL, y REAL NOT NULL, z REAL NOT NULL DEFAULT 0,
            yaw REAL NOT NULL DEFAULT 0,
            pitch REAL NOT NULL DEFAULT 0,
            fov_degrees REAL NOT NULL DEFAULT 60,
            range_pixels REAL NOT NULL DEFAULT 200,
            created_at INTEGER NOT NULL,
            updated_at INTEGER NOT NULL
        )"""
    )
    conn.execute(
        "INSERT INTO sensor_placements VALUES (?,?,?,?,?,?,?,?,?,?,?,?)",
        ("pid", "n1", "camera", 300.0, 400.0, 0.0, 0.0, 0.0, 60.0, 200.0, 1, 1),
    )
    conn.commit()
    conn.close()

    db = Db.open(db_path)  # triggers _ensure_meter_columns
    row = db.placement_by_id("pid")
    assert row is not None
    # 20 px/m migration converts the visual scale.
    assert row.x == pytest.approx(15.0)
    assert row.y == pytest.approx(20.0)
    assert row.range == pytest.approx(10.0)
```

(Adapt the import of `Db` to match the existing test file's style — likely already `from dash.db.sqlite import Db`.)

- [ ] **Step 2: Run test to verify it fails**

Run: `cd /home/nvidia/bubbaloop-dash/backend && .venv/bin/pytest tests/test_sensor_placements.py::test_meter_columns_migration_backfills_from_pixels -v`
Expected: `AttributeError: 'SensorPlacementRow' object has no attribute 'range'` OR a SQL error because the `range` column doesn't exist.

- [ ] **Step 3: Implement the migration helper**

In `backend/src/dash/db/sqlite.py`, find the existing `_ensure_anchor_columns` method (pattern from Phase 1.5) and add a sibling:

```python
    def _ensure_meter_columns(self) -> None:
        """Add `range` column to sensor_placements; backfill from pixel coords.

        Migration math: 20 px/m (the existing METER_TO_PIXELS hack from the
        frontend). A sensor at pixel (300, 400) with range_pixels=200 becomes
        (15m, 20m, range=10m) — same visual position at the 20 px/m render rate.
        """
        cols = {row[1] for row in self._conn.execute(
            "PRAGMA table_info(sensor_placements)"
        ).fetchall()}
        if "range" not in cols and "range_pixels" in cols:
            self._conn.execute("ALTER TABLE sensor_placements ADD COLUMN range REAL")
            self._conn.execute(
                """UPDATE sensor_placements
                   SET range = range_pixels / 20.0,
                       x = x / 20.0,
                       y = y / 20.0,
                       z = z / 20.0
                """
            )
```

Call this from `Db.open()` right after the schema is loaded, alongside the existing `_ensure_anchor_columns()` call.

- [ ] **Step 4: Run test to verify pass**

Run: `cd /home/nvidia/bubbaloop-dash/backend && .venv/bin/pytest tests/test_sensor_placements.py::test_meter_columns_migration_backfills_from_pixels -v`
Expected: PASS.

- [ ] **Step 5: Commit**

```bash
cd /home/nvidia/bubbaloop-dash
git add backend/src/dash/db/sqlite.py backend/tests/test_sensor_placements.py
git commit -m "feat(db): _ensure_meter_columns migration (pixels → meters at 20 px/m)"
```

---

## Task 9: `SensorPlacementRow` + Db CRUD methods use `range` field

**Files:**
- Modify: `backend/src/dash/db/sqlite.py`
- Modify: `backend/tests/test_sensor_placements.py`

- [ ] **Step 1: Write the failing test**

Append (or modify existing test) in `backend/tests/test_sensor_placements.py`:

```python
def test_create_placement_with_range_meters(db: Db) -> None:
    pid = db.create_placement(
        node_id="cam1",
        node_kind="camera",
        x=5.0, y=3.0, yaw=45.0,
        fov_degrees=60.0,
        range=10.0,            # meters, NOT pixels
        now_ms=1_700_000_000_000,
    )
    row = db.placement_by_id(pid)
    assert row is not None
    assert row.x == 5.0
    assert row.y == 3.0
    assert row.range == 10.0
```

If existing tests use `range_pixels`, update each to `range` here too.

- [ ] **Step 2: Run test to verify failure**

Run: `cd /home/nvidia/bubbaloop-dash/backend && .venv/bin/pytest tests/test_sensor_placements.py::test_create_placement_with_range_meters -v`
Expected: `TypeError: unexpected keyword argument 'range'` OR `AttributeError: 'SensorPlacementRow' object has no attribute 'range'`.

- [ ] **Step 3: Update `SensorPlacementRow` dataclass**

In `backend/src/dash/db/sqlite.py`, find the `SensorPlacementRow` dataclass. Add `range: float` between `range_pixels` and `created_at`:

```python
@dataclass(frozen=True)
class SensorPlacementRow:
    id: str
    node_id: str
    node_kind: str
    x: float
    y: float
    z: float
    yaw: float
    pitch: float
    fov_degrees: float
    range_pixels: float   # legacy — kept until follow-up DROP migration
    range: float          # meters — authoritative
    created_at: int
    updated_at: int
```

- [ ] **Step 4: Update `create_placement` + `update_placement` + read SQL**

In `Db.create_placement`, change the signature parameter from `range_pixels` to `range`. Update the SQL accordingly:

```python
    def create_placement(
        self,
        *,
        node_id: str,
        node_kind: str,
        x: float,
        y: float,
        yaw: float,
        fov_degrees: float,
        range: float,          # meters
        now_ms: int,
        z: float = 0.0,
        pitch: float = 0.0,
    ) -> str:
        pid = uuid.uuid4().hex
        self._conn.execute(
            """INSERT INTO sensor_placements
               (id, node_id, node_kind, x, y, z, yaw, pitch, fov_degrees,
                range_pixels, range, created_at, updated_at)
               VALUES (?,?,?,?,?,?,?,?,?,?,?,?,?)""",
            (pid, node_id, node_kind, x, y, z, yaw, pitch, fov_degrees,
             range * 20.0,  # legacy field — keep in sync until DROP migration
             range, now_ms, now_ms),
        )
        return pid
```

Similarly update `update_placement` to accept a `range: float | None = None` kwarg (or rename `range_pixels` kwarg). Update SELECT queries to read both columns. Update `placement_by_id` and `placement_by_node_id` constructors to pass both.

- [ ] **Step 5: Run tests to verify pass**

Run: `cd /home/nvidia/bubbaloop-dash/backend && .venv/bin/pytest tests/test_sensor_placements.py -v`
Expected: all pass.

- [ ] **Step 6: Commit**

```bash
cd /home/nvidia/bubbaloop-dash
git add backend/src/dash/db/sqlite.py backend/tests/test_sensor_placements.py
git commit -m "feat(db): SensorPlacementRow.range (meters) + CRUD field rename"
```

---

## Task 10: API placement DTOs — `range` (meters) field

**Files:**
- Modify: `backend/src/dash/api/map.py`
- Modify: `backend/tests/test_map_placements.py`

- [ ] **Step 1: Write the failing test**

Update an existing test in `backend/tests/test_map_placements.py` (or add a new one) that POSTs a placement and asserts the response carries `range` (not `range_pixels`):

```python
def test_create_placement_returns_range_in_meters(client) -> None:
    body = {
        "node_id": "cam1",
        "node_kind": "camera",
        "x": 5.0, "y": 3.0, "yaw": 45.0,
        "fov_degrees": 60.0,
        "range": 10.0,
    }
    r = client.post("/api/map/placements", json=body)
    assert r.status_code == 201
    out = r.json()
    assert out["x"] == 5.0
    assert out["y"] == 3.0
    assert out["range"] == 10.0
    assert "range_pixels" not in out  # legacy field hidden from API
```

Also update any existing assertion that referenced `range_pixels` to use `range`.

- [ ] **Step 2: Run test to verify failure**

Run: `cd /home/nvidia/bubbaloop-dash/backend && .venv/bin/pytest tests/test_map_placements.py::test_create_placement_returns_range_in_meters -v`
Expected: `KeyError: 'range'` or 422 (because `range` isn't an accepted field yet).

- [ ] **Step 3: Update Pydantic models**

In `backend/src/dash/api/map.py`, replace `range_pixels` everywhere with `range`. With unit documentation in the Field description:

```python
class PlacementCreate(BaseModel):
    node_id: str = Field(min_length=1, max_length=128)
    node_kind: str = Field(min_length=1, max_length=64)
    x: float = Field(description="world meters")
    y: float = Field(description="world meters")
    yaw: float = Field(description="degrees, 0 = +x (east)")
    fov_degrees: float = Field(gt=0, le=360)
    range: float = Field(gt=0, description="world meters")
    z: float = Field(default=0.0, description="world meters")
    pitch: float = 0.0


class PlacementUpdate(BaseModel):
    x: float | None = None
    y: float | None = None
    z: float | None = None
    yaw: float | None = None
    pitch: float | None = None
    fov_degrees: float | None = Field(default=None, gt=0, le=360)
    range: float | None = Field(default=None, gt=0)


class PlacementOut(BaseModel):
    id: str
    node_id: str
    node_kind: str
    x: float = Field(description="world meters")
    y: float = Field(description="world meters")
    z: float = Field(description="world meters")
    yaw: float
    pitch: float
    fov_degrees: float
    range: float = Field(description="world meters")
    created_at: int
    updated_at: int
```

Update the `_row_to_out` helper to read `row.range` (not `row.range_pixels`). Update `create_placement` route to pass `range=body.range` (not `range_pixels=body.range_pixels * 20`). Update `update_placement` route similarly.

- [ ] **Step 4: Run tests to verify pass**

Run: `cd /home/nvidia/bubbaloop-dash/backend && .venv/bin/pytest tests/test_map_placements.py -v`
Expected: all pass.

- [ ] **Step 5: Commit**

```bash
cd /home/nvidia/bubbaloop-dash
git add backend/src/dash/api/map.py backend/tests/test_map_placements.py
git commit -m "feat(api): placement DTOs use range (meters) instead of range_pixels"
```

---

## Task 11: Remove floorplan-upload backend code + delete tests

**Files:**
- Modify: `backend/src/dash/api/map.py`
- Delete: `backend/tests/test_map_floorplan.py`

- [ ] **Step 1: Inspect what needs to go**

Run: `grep -n "floorplan\|FLOORPLAN\|_ALLOWED_FORMATS\|FloorplanInfo\|_find_floorplan\|_floorplan_candidates" backend/src/dash/api/map.py`

Identify the floorplan-upload route, the GET-floorplan route, helper functions, model class, and constants. They all go.

- [ ] **Step 2: Delete the test file**

```bash
cd /home/nvidia/bubbaloop-dash
git rm backend/tests/test_map_floorplan.py
```

- [ ] **Step 3: Delete the route code**

In `backend/src/dash/api/map.py` remove:
- `FLOORPLAN_MAX_BYTES` constant
- `_ALLOWED_FORMATS` dict
- `FloorplanInfo` Pydantic model
- `_floorplan_candidates`, `_find_floorplan` helpers
- `POST /floorplan` route (`upload_floorplan`)
- `GET /floorplan` route (`get_floorplan`)
- Any associated imports (`UploadFile`, `Image`, `UnidentifiedImageError`, `io`, `shutil`) that are no longer used (verify with grep before removing each)

- [ ] **Step 4: Run full backend suite to confirm no regressions**

Run: `cd /home/nvidia/bubbaloop-dash/backend && .venv/bin/pytest tests/ -q`
Expected: green (modulo pre-existing flaky teardown tests under parallel load — those are not introduced here).

- [ ] **Step 5: Commit**

```bash
cd /home/nvidia/bubbaloop-dash
git add backend/src/dash/api/map.py
git commit -m "refactor(api): remove floorplan-upload code (replaced by SLAM map source)"
```

---

## Task 12: Frontend `useSlamMap` hook

**Files:**
- Create: `frontend/src/hooks/useSlamMap.ts`
- Create: `frontend/src/hooks/__tests__/useSlamMap.test.ts`

- [ ] **Step 1: Write the failing tests**

`frontend/src/hooks/__tests__/useSlamMap.test.ts`:

```ts
import { renderHook, waitFor, act } from '@testing-library/react';
import { describe, it, expect, vi, beforeEach, afterEach } from 'vitest';
import { useSlamMap } from '../useSlamMap';

const META_OK = {
  width: 200, height: 160, resolution: 0.05,
  origin_x: -5.0, origin_y: -4.0,
  updated_at: 1716290400000, source: 'local-file',
};

describe('useSlamMap', () => {
  beforeEach(() => {
    vi.useFakeTimers({ shouldAdvanceTime: true });
  });
  afterEach(() => {
    vi.useRealTimers();
    vi.restoreAllMocks();
  });

  it('returns null when /api/map/slam/meta is 404', async () => {
    global.fetch = vi.fn().mockResolvedValue({
      ok: false, status: 404,
      json: async () => ({ detail: { reason: 'no_slam_map' } }),
    }) as unknown as typeof fetch;
    const { result } = renderHook(() => useSlamMap());
    await waitFor(() => expect((global.fetch as ReturnType<typeof vi.fn>).mock.calls.length).toBeGreaterThan(0));
    expect(result.current).toBeNull();
  });

  it('returns meta + imageUrl on 200', async () => {
    global.fetch = vi.fn().mockResolvedValue({
      ok: true, status: 200,
      json: async () => META_OK,
    }) as unknown as typeof fetch;
    const { result } = renderHook(() => useSlamMap());
    await waitFor(() => expect(result.current).not.toBeNull());
    expect(result.current?.meta.width).toBe(200);
    expect(result.current?.imageUrl).toBe(`/api/map/slam?ts=${META_OK.updated_at}`);
  });

  it('polls /api/map/slam/meta every 10s', async () => {
    const fetchMock = vi.fn().mockResolvedValue({
      ok: true, status: 200, json: async () => META_OK,
    });
    global.fetch = fetchMock as unknown as typeof fetch;
    renderHook(() => useSlamMap(10_000));
    await waitFor(() => expect(fetchMock.mock.calls.length).toBe(1));
    act(() => { vi.advanceTimersByTime(10_000); });
    await waitFor(() => expect(fetchMock.mock.calls.length).toBeGreaterThanOrEqual(2));
  });
});
```

- [ ] **Step 2: Run tests to verify they fail**

Run: `cd /home/nvidia/bubbaloop-dash/frontend && npx vitest run src/hooks/__tests__/useSlamMap.test.ts`
Expected: import error.

- [ ] **Step 3: Implement the hook**

`frontend/src/hooks/useSlamMap.ts`:

```ts
import { useEffect, useState } from 'react';

export type SlamMapMeta = {
  width: number;
  height: number;
  resolution: number;
  origin_x: number;
  origin_y: number;
  updated_at: number;
  source: string;
};

export type SlamMap = {
  meta: SlamMapMeta;
  imageUrl: string;
};

const DEFAULT_POLL_MS = 10_000;

export function useSlamMap(pollMs: number = DEFAULT_POLL_MS): SlamMap | null {
  const [state, setState] = useState<SlamMap | null>(null);

  useEffect(() => {
    let cancelled = false;
    async function probe(): Promise<void> {
      try {
        const r = await fetch('/api/map/slam/meta');
        if (cancelled) return;
        if (!r.ok) {
          setState(null);
          return;
        }
        const meta = (await r.json()) as SlamMapMeta;
        if (cancelled) return;
        setState({
          meta,
          imageUrl: `/api/map/slam?ts=${meta.updated_at}`,
        });
      } catch {
        if (!cancelled) setState(null);
      }
    }
    void probe();
    const id = window.setInterval(() => { void probe(); }, pollMs);
    return () => {
      cancelled = true;
      window.clearInterval(id);
    };
  }, [pollMs]);

  return state;
}
```

- [ ] **Step 4: Run tests to verify they pass**

Run: `cd /home/nvidia/bubbaloop-dash/frontend && npx vitest run src/hooks/__tests__/useSlamMap.test.ts`
Expected: 3 passed.

- [ ] **Step 5: Commit**

```bash
cd /home/nvidia/bubbaloop-dash
git add frontend/src/hooks/useSlamMap.ts frontend/src/hooks/__tests__/useSlamMap.test.ts
git commit -m "feat(slam): useSlamMap hook with 10s polling"
```

---

## Task 13: Frontend `SlamLayer` component

**Files:**
- Create: `frontend/src/components/SlamLayer.tsx`
- Create: `frontend/src/components/__tests__/SlamLayer.test.tsx`

- [ ] **Step 1: Write the failing tests**

`frontend/src/components/__tests__/SlamLayer.test.tsx`:

```tsx
import { render } from '@testing-library/react';
import { describe, it, expect } from 'vitest';
import { Stage, Layer } from 'react-konva';
import { SlamLayer } from '../SlamLayer';

function wrap(node: React.ReactNode) {
  return (
    <Stage width={400} height={300}>
      <Layer>{node}</Layer>
    </Stage>
  );
}

describe('SlamLayer', () => {
  const baseProps = {
    imageUrl: '/api/map/slam?ts=1',
    meta: {
      width: 200, height: 160, resolution: 0.05,
      origin_x: -5.0, origin_y: -4.0,
      updated_at: 1, source: 'local-file',
    },
  };

  it('renders without throwing inside a Konva Stage', () => {
    expect(() => render(wrap(<SlamLayer {...baseProps} />))).not.toThrow();
  });

  it('handles various resolutions + origins', () => {
    for (const [res, ox, oy] of [[0.025, 0, 0], [0.1, -10, 5]] as const) {
      expect(() => render(wrap(<SlamLayer {...baseProps} meta={{ ...baseProps.meta, resolution: res, origin_x: ox, origin_y: oy }} />))).not.toThrow();
    }
  });
});
```

- [ ] **Step 2: Run tests to verify failure**

Run: `cd /home/nvidia/bubbaloop-dash/frontend && npx vitest run src/components/__tests__/SlamLayer.test.tsx`
Expected: import error.

- [ ] **Step 3: Implement `SlamLayer`**

`frontend/src/components/SlamLayer.tsx`:

```tsx
import { Image as KonvaImage } from 'react-konva';
import { useImage } from '../hooks/useImage';
import type { SlamMapMeta } from '../hooks/useSlamMap';

type Props = {
  imageUrl: string;
  meta: SlamMapMeta;
};

/**
 * Renders the SLAM occupancy grid as a Konva Image.
 *
 * The Konva Stage scales meters → screen pixels via `Stage.scale = pxPerMeter`,
 * so this layer draws the image at its native meter dimensions:
 *   - width_in_meters  = meta.width  * meta.resolution
 *   - height_in_meters = meta.height * meta.resolution
 * Positioned at `(origin_x, origin_y)` so the bottom-left pixel lands at the
 * YAML-declared world origin.
 */
export function SlamLayer({ imageUrl, meta }: Props) {
  const img = useImage(imageUrl);
  if (img === undefined) return null;
  const widthMeters = meta.width * meta.resolution;
  const heightMeters = meta.height * meta.resolution;
  return (
    <KonvaImage
      image={img}
      x={meta.origin_x}
      y={meta.origin_y}
      width={widthMeters}
      height={heightMeters}
      listening={false}
    />
  );
}
```

(`useImage` already exists from Phase 1.4; verify it returns `HTMLImageElement | undefined` and accepts a string URL.)

- [ ] **Step 4: Run tests to verify pass**

Run: `cd /home/nvidia/bubbaloop-dash/frontend && npx vitest run src/components/__tests__/SlamLayer.test.tsx`
Expected: 2 passed.

- [ ] **Step 5: Commit**

```bash
cd /home/nvidia/bubbaloop-dash
git add frontend/src/components/SlamLayer.tsx frontend/src/components/__tests__/SlamLayer.test.tsx
git commit -m "feat(slam): SlamLayer Konva component (renders occupancy PNG in meter units)"
```

---

## Task 14: Frontend `MetersGrid` component

**Files:**
- Create: `frontend/src/components/MetersGrid.tsx`
- Create: `frontend/src/components/__tests__/MetersGrid.test.tsx`

- [ ] **Step 1: Write the failing tests**

`frontend/src/components/__tests__/MetersGrid.test.tsx`:

```tsx
import { render } from '@testing-library/react';
import { describe, it, expect } from 'vitest';
import { Stage, Layer } from 'react-konva';
import { MetersGrid } from '../MetersGrid';

function wrap(node: React.ReactNode) {
  return (
    <Stage width={400} height={300}>
      <Layer>{node}</Layer>
    </Stage>
  );
}

describe('MetersGrid', () => {
  it('renders without throwing', () => {
    expect(() =>
      render(wrap(<MetersGrid widthMeters={20} heightMeters={15} />)),
    ).not.toThrow();
  });

  it('handles small and large viewports', () => {
    for (const [w, h] of [[1, 1], [100, 50], [0.5, 0.5]] as const) {
      expect(() =>
        render(wrap(<MetersGrid widthMeters={w} heightMeters={h} />)),
      ).not.toThrow();
    }
  });
});
```

- [ ] **Step 2: Run tests to verify failure**

Run: `cd /home/nvidia/bubbaloop-dash/frontend && npx vitest run src/components/__tests__/MetersGrid.test.tsx`
Expected: import error.

- [ ] **Step 3: Implement `MetersGrid`**

`frontend/src/components/MetersGrid.tsx`:

```tsx
import { Group, Line } from 'react-konva';

type Props = {
  widthMeters: number;
  heightMeters: number;
  /** Spacing of minor grid lines in meters. Defaults to 1m. */
  step?: number;
  /** Spacing of major emphasis lines in meters. Defaults to 5m. */
  majorStep?: number;
};

/**
 * Reference grid drawn in world meters. Used when no SLAM map is available.
 * The grid is centered on world (0, 0) and extends `widthMeters / 2` in each
 * direction along x and `heightMeters / 2` along y.
 *
 * `listening={false}` so taps/clicks pass through to placement layers above.
 */
export function MetersGrid({
  widthMeters,
  heightMeters,
  step = 1,
  majorStep = 5,
}: Props) {
  const halfW = widthMeters / 2;
  const halfH = heightMeters / 2;
  const minorColor = 'rgba(255, 255, 255, 0.04)';
  const majorColor = 'rgba(255, 255, 255, 0.10)';

  const lines: React.ReactNode[] = [];
  // Vertical lines.
  for (let x = -halfW; x <= halfW + 1e-9; x += step) {
    const isMajor = Math.abs(x % majorStep) < 1e-9 || Math.abs((x % majorStep) - majorStep) < 1e-9;
    lines.push(
      <Line
        key={`v-${x}`}
        points={[x, -halfH, x, halfH]}
        stroke={isMajor ? majorColor : minorColor}
        strokeWidth={isMajor ? 0.02 : 0.01}
        listening={false}
      />,
    );
  }
  // Horizontal lines.
  for (let y = -halfH; y <= halfH + 1e-9; y += step) {
    const isMajor = Math.abs(y % majorStep) < 1e-9 || Math.abs((y % majorStep) - majorStep) < 1e-9;
    lines.push(
      <Line
        key={`h-${y}`}
        points={[-halfW, y, halfW, y]}
        stroke={isMajor ? majorColor : minorColor}
        strokeWidth={isMajor ? 0.02 : 0.01}
        listening={false}
      />,
    );
  }

  return <Group listening={false}>{lines}</Group>;
}
```

(Note: stroke widths are in meter units; the Stage scale converts to screen pixels. At `pxPerMeter=50`, `strokeWidth=0.02` becomes 1px — visible but faint.)

- [ ] **Step 4: Run tests to verify pass**

Run: `cd /home/nvidia/bubbaloop-dash/frontend && npx vitest run src/components/__tests__/MetersGrid.test.tsx`
Expected: 2 passed.

- [ ] **Step 5: Commit**

```bash
cd /home/nvidia/bubbaloop-dash
git add frontend/src/components/MetersGrid.tsx frontend/src/components/__tests__/MetersGrid.test.tsx
git commit -m "feat(slam): MetersGrid background for no-SLAM mode"
```

---

## Task 15: `SensorMarker`, `FOVCone`, `RotationHandle` — `range` prop in meters

**Files:**
- Modify: `frontend/src/components/SensorMarker.tsx`
- Modify: `frontend/src/components/FOVCone.tsx`
- Modify: `frontend/src/components/RotationHandle.tsx`
- Modify: `frontend/src/components/__tests__/SensorMarker.test.tsx`
- Modify: `frontend/src/components/__tests__/RotationHandle.test.tsx`

- [ ] **Step 1: Update tests to use `range` prop**

In each test file (`SensorMarker.test.tsx`, `RotationHandle.test.tsx`), find every prop literal `range_pixels=NUMBER` and replace with `range={NUMBER / 20}` (preserving the numeric visual size at 20 px/m — e.g. `range_pixels=200` → `range={10}`).

Use the test file's base props object to keep this DRY.

- [ ] **Step 2: Run tests to verify failure**

Run: `cd /home/nvidia/bubbaloop-dash/frontend && npx vitest run src/components/__tests__/SensorMarker.test.tsx src/components/__tests__/RotationHandle.test.tsx`
Expected: TS / runtime errors on `range` prop because components don't expose it yet.

- [ ] **Step 3: Update the components**

In `frontend/src/components/SensorMarker.tsx`:
- Rename the `rangePixels: number` prop to `range: number`
- Use the renamed value wherever it was used (passed to `FOVCone` + `RotationHandle`)
- Update the FOVCone prop call: `rangePixels={...}` → `range={...}`
- Update the RotationHandle prop call: `rangePixels={...}` → `range={...}`

In `frontend/src/components/FOVCone.tsx`:
- Rename the `rangePixels: number` prop to `range: number`
- The Konva `Wedge` `radius` value is unchanged — the Stage scale handles the meter-to-pixel conversion

In `frontend/src/components/RotationHandle.tsx`:
- Rename the `rangePixels: number` prop to `range: number`
- Update the position math: `coneTipPosition(range, yaw)`

- [ ] **Step 4: Update geometry helper signature (if needed)**

Open `frontend/src/lib/geometry.ts` and verify `coneTipPosition(rangePixels: number, yawDeg: number)` — rename the parameter to `range` for consistency (function doesn't care about units, but the name reads better in meter contexts).

- [ ] **Step 5: Run tests to verify pass**

Run: `cd /home/nvidia/bubbaloop-dash/frontend && npx vitest run src/components/__tests__/SensorMarker.test.tsx src/components/__tests__/RotationHandle.test.tsx`
Expected: pass.

- [ ] **Step 6: Commit**

```bash
cd /home/nvidia/bubbaloop-dash
git add frontend/src/components/SensorMarker.tsx frontend/src/components/FOVCone.tsx frontend/src/components/RotationHandle.tsx frontend/src/components/__tests__/SensorMarker.test.tsx frontend/src/components/__tests__/RotationHandle.test.tsx frontend/src/lib/geometry.ts
git commit -m "feat(map): SensorMarker/FOVCone/RotationHandle take range (meters)"
```

---

## Task 16: `MapCanvas` — `background` prop + Stage scale transform

**Files:**
- Modify: `frontend/src/components/MapCanvas.tsx`
- Modify: `frontend/src/components/__tests__/MapCanvas.test.tsx`

- [ ] **Step 1: Write the failing tests**

Update `frontend/src/components/__tests__/MapCanvas.test.tsx`. Replace floorplan-specific tests with background-mode tests:

```tsx
it('renders MetersGrid when background.kind is meters-grid', () => {
  expect(() => render(
    <MapCanvas
      background={{ kind: 'meters-grid' }}
      stageWidth={400} stageHeight={300}
      placements={[]} selectedId={null}
      onSelect={() => {}} onMove={() => {}} onRotate={() => {}} onPlaceNode={() => {}}
    />,
  )).not.toThrow();
});

it('renders SlamLayer when background.kind is slam', () => {
  const meta = {
    width: 200, height: 160, resolution: 0.05,
    origin_x: -5.0, origin_y: -4.0,
    updated_at: 1, source: 'local-file',
  };
  expect(() => render(
    <MapCanvas
      background={{ kind: 'slam', meta, imageUrl: '/api/map/slam?ts=1' }}
      stageWidth={400} stageHeight={300}
      placements={[]} selectedId={null}
      onSelect={() => {}} onMove={() => {}} onRotate={() => {}} onPlaceNode={() => {}}
    />,
  )).not.toThrow();
});
```

Remove the now-irrelevant `floorplanWidth`/`floorplanHeight`/`floorplanUrl` props from existing tests (replace with the `background` prop).

- [ ] **Step 2: Run tests to verify failure**

Run: `cd /home/nvidia/bubbaloop-dash/frontend && npx vitest run src/components/__tests__/MapCanvas.test.tsx`
Expected: TS / runtime errors on the new `background` prop.

- [ ] **Step 3: Update `MapCanvas`**

In `frontend/src/components/MapCanvas.tsx`:
- Drop `floorplanUrl`, `floorplanWidth`, `floorplanHeight` props
- Add `background: MapBackground` prop where:
  ```ts
  export type MapBackground =
    | { kind: 'slam'; meta: SlamMapMeta; imageUrl: string }
    | { kind: 'meters-grid' };
  ```
- Compute `pxPerMeter`: `1 / meta.resolution` for slam, `50` constant for meters-grid
- Apply Stage scale: `scaleX={pxPerMeter}`, `scaleY={pxPerMeter}`
- Drop the floorplan-image Konva layer; replace with conditional `<SlamLayer>` or `<MetersGrid>` based on `background.kind`
- All sensor placements now draw in meter coords (no conversion); their `range` is read straight from the row

```tsx
import { SlamLayer } from './SlamLayer';
import { MetersGrid } from './MetersGrid';
import type { SlamMapMeta } from '../hooks/useSlamMap';

export type MapBackground =
  | { kind: 'slam'; meta: SlamMapMeta; imageUrl: string }
  | { kind: 'meters-grid' };

export type Placement = {
  id: string;
  node_id: string;
  node_kind: NodeKind;
  x: number;        // meters
  y: number;        // meters
  yaw: number;
  fov_degrees: number;
  range: number;    // meters
};

type Props = {
  background: MapBackground;
  stageWidth: number;
  stageHeight: number;
  placements: Placement[];
  selectedId: string | null;
  onSelect: (id: string | null) => void;
  onMove: (id: string, x: number, y: number) => void;
  onRotate: (id: string, yaw: number) => void;
  onPlaceNode: (payload: NodeDragPayload, x: number, y: number) => void;
};

const NO_SLAM_PX_PER_M = 50;
const NO_SLAM_VIEW_METERS = { w: 40, h: 30 };

export function MapCanvas({ background, stageWidth, stageHeight, placements, selectedId, onSelect, onMove, onRotate, onPlaceNode }: Props) {
  const pxPerMeter =
    background.kind === 'slam' ? 1 / background.meta.resolution : NO_SLAM_PX_PER_M;

  return (
    <div data-testid="map-canvas-drop-zone" /* keep existing drag handlers */>
      <Stage
        width={stageWidth}
        height={stageHeight}
        scaleX={pxPerMeter}
        scaleY={pxPerMeter}
        /* keep pan + wheel-zoom logic; multiply scale by pxPerMeter on apply */
      >
        <Layer>
          {background.kind === 'slam' && (
            <SlamLayer imageUrl={background.imageUrl} meta={background.meta} />
          )}
          {background.kind === 'meters-grid' && (
            <MetersGrid widthMeters={NO_SLAM_VIEW_METERS.w} heightMeters={NO_SLAM_VIEW_METERS.h} />
          )}
        </Layer>
        <Layer>
          {/* existing placement rendering — but read `p.range` instead of `p.range_pixels` */}
        </Layer>
      </Stage>
    </div>
  );
}
```

Adapt the existing pan/zoom math: where previously the screen-to-canvas conversion divided by `scale`, now divide by `scale × pxPerMeter` so drop coords land in meters.

- [ ] **Step 4: Run tests to verify pass**

Run: `cd /home/nvidia/bubbaloop-dash/frontend && npx vitest run src/components/__tests__/MapCanvas.test.tsx`
Expected: pass.

- [ ] **Step 5: Commit**

```bash
cd /home/nvidia/bubbaloop-dash
git add frontend/src/components/MapCanvas.tsx frontend/src/components/__tests__/MapCanvas.test.tsx
git commit -m "feat(map): MapCanvas background prop (slam | meters-grid) + Stage scale = pxPerMeter"
```

---

## Task 17: `MapTab` — drop FloorplanUploader, use `useSlamMap`

**Files:**
- Modify: `frontend/src/components/MapTab.tsx`
- Modify: `frontend/src/components/__tests__/MapTab.test.tsx`

- [ ] **Step 1: Write the failing tests**

Update `frontend/src/components/__tests__/MapTab.test.tsx`:

```tsx
// Remove the existing 'shows FloorplanUploader when no floorplan' test.
// Replace with:

it('renders MetersGrid background when /api/map/slam/meta is 404', async () => {
  setupFetch({
    'GET /api/map/slam/meta': { status: 404 },
    'GET /api/map/placements': { status: 200, body: { placements: [] } },
    'GET /api/videos': { status: 200, body: { items: [], total: 0, next_cursor: null } },
  });
  render(<MapTab />);
  await waitFor(() => screen.getByTestId('map-canvas-drop-zone'));
  // MetersGrid renders inside Konva canvas — no DOM assertion possible;
  // assert that the SlamLayer is NOT rendered by mocking useImage to verify
  // it was never invoked with a slam URL. Simpler: just verify MapCanvas
  // received background.kind === 'meters-grid'. The cleanest assertion is
  // that the drop-zone is present and no slam-related fetch happened.
  const calls = (global.fetch as ReturnType<typeof vi.fn>).mock.calls;
  expect(calls.find(([url]: [string]) => url.startsWith('/api/map/slam') && !url.includes('/meta'))).toBeUndefined();
});

it('renders SlamLayer when /api/map/slam/meta returns 200', async () => {
  setupFetch({
    'GET /api/map/slam/meta': {
      status: 200,
      body: { width: 200, height: 160, resolution: 0.05, origin_x: -5, origin_y: -4, updated_at: 1, source: 'local-file' },
    },
    'GET /api/map/placements': { status: 200, body: { placements: [] } },
    'GET /api/videos': { status: 200, body: { items: [], total: 0, next_cursor: null } },
  });
  render(<MapTab />);
  await waitFor(() => screen.getByTestId('map-canvas-drop-zone'));
  // No DOM-level assertion for the canvas image; verify a slam image fetch fired.
  const calls = (global.fetch as ReturnType<typeof vi.fn>).mock.calls;
  await waitFor(() => {
    expect(calls.find(([url]: [string]) => url.startsWith('/api/map/slam?ts='))).toBeDefined();
  });
});
```

Remove any existing tests that posted to `/api/map/floorplan`.

- [ ] **Step 2: Run tests to verify failure**

Run: `cd /home/nvidia/bubbaloop-dash/frontend && npx vitest run src/components/__tests__/MapTab.test.tsx`
Expected: failures because MapTab still references FloorplanUploader / floorplan API.

- [ ] **Step 3: Update `MapTab`**

In `frontend/src/components/MapTab.tsx`:
- Remove import of `FloorplanUploader`
- Remove `hasFloorplan`, `floorplanUrl`, `probeFloorplan`, `handleUploaded`, and all related state
- Add `import { useSlamMap } from '../hooks/useSlamMap'`
- Compute the `background` prop from `useSlamMap()`:
  ```tsx
  const slamMap = useSlamMap();
  const background = slamMap
    ? { kind: 'slam' as const, meta: slamMap.meta, imageUrl: slamMap.imageUrl }
    : { kind: 'meters-grid' as const };
  ```
- Always render `<MapCanvas background={background} ... />` — no conditional swap based on whether-a-map-exists
- Remove the `METER_TO_PIXELS = 20` constant; `handlePlaceNode` now passes raw meter values to POST
- Update `handlePlaceNode` body construction:
  ```ts
  const body = {
    node_id: payload.nodeId,
    node_kind: payload.nodeKind,
    x, y,                       // already in meters from MapCanvas
    yaw: 0,
    fov_degrees: payload.fovDegrees,
    range: payload.rangeMeters,  // direct, no conversion
  };
  ```
- Remove FloorplanUploader from the JSX

- [ ] **Step 4: Run tests to verify pass**

Run: `cd /home/nvidia/bubbaloop-dash/frontend && npx vitest run src/components/__tests__/MapTab.test.tsx`
Expected: pass.

- [ ] **Step 5: Run full frontend suite + tsc**

Run:
```bash
cd /home/nvidia/bubbaloop-dash/frontend
npm test -- --run
npx tsc --noEmit
```
Expected: green; clean.

- [ ] **Step 6: Commit**

```bash
cd /home/nvidia/bubbaloop-dash
git add frontend/src/components/MapTab.tsx frontend/src/components/__tests__/MapTab.test.tsx
git commit -m "feat(map): MapTab uses useSlamMap; drops FloorplanUploader"
```

---

## Task 18: Delete `FloorplanUploader` + its test

**Files:**
- Delete: `frontend/src/components/FloorplanUploader.tsx`
- Delete: `frontend/src/components/__tests__/FloorplanUploader.test.tsx`

- [ ] **Step 1: Confirm no remaining imports**

Run: `grep -rn "FloorplanUploader" frontend/src/ frontend/tests/ 2>/dev/null`
Expected: no matches (or only the soon-to-be-deleted files themselves).

- [ ] **Step 2: Delete the files**

```bash
cd /home/nvidia/bubbaloop-dash
git rm frontend/src/components/FloorplanUploader.tsx
git rm frontend/src/components/__tests__/FloorplanUploader.test.tsx
```

- [ ] **Step 3: Verify the suite + types**

Run:
```bash
cd /home/nvidia/bubbaloop-dash/frontend
npm test -- --run
npx tsc --noEmit
```
Expected: pass; clean.

- [ ] **Step 4: Commit**

```bash
cd /home/nvidia/bubbaloop-dash
git commit -m "refactor(map): remove FloorplanUploader (replaced by useSlamMap)"
```

---

## Task 19: Manual E2E verification

**Files:** none (manual checklist).

- [ ] **Step 1: Confirm dev stack is running**

If not, run from `/home/nvidia/bubbaloop-dash`:
```bash
./scripts/dev.sh
```
Wait for backend health + Vite ready.

- [ ] **Step 2: Open Map tab — should render MetersGrid background**

Open `https://nvidia-orin00.tail9276e7.ts.net` on phone (or `http://localhost:5174`). Switch to Map tab. Expect:
- A faint 1m grid background (5m emphasis lines)
- No FloorplanUploader card
- NodeSidebar still on the right
- Live sensors from `/api/nodes/live` listed and draggable

- [ ] **Step 3: Drop a sample SLAM map into the dev path**

```bash
SLAM_DEV_PATH=$HOME/.bubbaloop-dash-dev/maps/slam/dev
mkdir -p "$SLAM_DEV_PATH"
cd /home/nvidia/bubbaloop-dash/backend && .venv/bin/python3 <<'PY'
import os
from pathlib import Path
from PIL import Image
import yaml

path = Path(os.environ["HOME"]) / ".bubbaloop-dash-dev" / "maps" / "slam" / "dev"
path.mkdir(parents=True, exist_ok=True)

# Make a recognizable test grid — checkerboard
img = Image.new("L", (200, 160), color=200)
for x in range(0, 200, 20):
    for y in range(0, 160, 20):
        if (x // 20 + y // 20) % 2 == 0:
            for dx in range(20):
                for dy in range(20):
                    if x + dx < 200 and y + dy < 160:
                        img.putpixel((x + dx, y + dy), 80)
img.save(path / "occupancy.png", format="PNG")

(path / "occupancy.yaml").write_text(yaml.safe_dump({
    "image": "occupancy.png",
    "resolution": 0.05,
    "origin": [-5.0, -4.0, 0.0],
    "negate": 0,
    "occupied_thresh": 0.65,
    "free_thresh": 0.196,
}))
print(f"wrote sample SLAM map to {path}")
PY
```

- [ ] **Step 4: Wait up to 10s, then verify SLAM background renders**

The frontend polls every 10s. After at most 10s, the Map tab should show:
- The checkerboard pattern as the canvas background
- 1 pixel = 0.05 m (200×160 px = 10m × 8m)
- The bottom-left of the image at world (−5, −4)

- [ ] **Step 5: Place a sensor and verify meter-unit storage**

Drag a node from the sidebar onto the canvas. Then check the placement in the database:

```bash
sqlite3 ~/.bubbaloop-dash-dev/dash.db 'SELECT node_id, x, y, range FROM sensor_placements ORDER BY created_at DESC LIMIT 1;'
```

Expected: `x`, `y`, `range` are sensible meter values (e.g., 5.0, 3.2, 10.0) — not hundreds.

- [ ] **Step 6: Remove the SLAM map files**

```bash
rm -rf ~/.bubbaloop-dash-dev/maps/slam/dev
```

After ≤ 10s, the Map tab background should revert to MetersGrid. Sensor placement remains visible (positions unchanged).

- [ ] **Step 7: Final regression run**

Run:
```bash
cd /home/nvidia/bubbaloop-dash/backend && .venv/bin/pytest tests/ -q
cd /home/nvidia/bubbaloop-dash/frontend && npm test -- --run
cd /home/nvidia/bubbaloop-dash/frontend && npx tsc --noEmit
```
Expected: all green.

---

## Done criteria

- [ ] All 18 implementation tasks committed (Task 19 is manual verification, no commit)
- [ ] `pyyaml` listed in `backend/pyproject.toml` dependencies
- [ ] `backend/src/dash/slam/` package exists with `types.py` + `source.py`
- [ ] `GET /api/map/slam/meta` and `GET /api/map/slam` both work against `LocalFileSource`
- [ ] `RemoteSlamDbSource` exists as a stub returning `None`
- [ ] `sensor_placements.range` column exists; old rows migrated at `Db.open()`
- [ ] All placement API DTOs use `range` (meters), not `range_pixels`
- [ ] `FloorplanUploader` and its test deleted; no remaining references
- [ ] Frontend `useSlamMap` hook polls `/api/map/slam/meta` every 10s and renders either `SlamLayer` or `MetersGrid`
- [ ] `SensorMarker`/`FOVCone`/`RotationHandle` use the `range` prop
- [ ] Backend test suite green (≥ 9 new tests in `test_map_slam.py` + migration test)
- [ ] Frontend test suite green (≥ 5 new tests across `useSlamMap`, `SlamLayer`, `MetersGrid`, `MapTab`, `MapCanvas`)
- [ ] `npx tsc --noEmit` clean, `ruff` + `pytest` clean
- [ ] Manual E2E: SLAM background renders when files exist, meters grid otherwise, sensor positions stored in meters
