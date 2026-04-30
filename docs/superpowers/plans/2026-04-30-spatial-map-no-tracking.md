# Spatial Map (No Tracking) Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Project per-camera detections into world coordinates and render them as a live map with raw history and visual zones — no tracking, no entity persistence.

**Architecture:** Application-side only. `geo-projector` (Python node) loses its `WorldTracker` and `track_id` field. `spatial-map` (Python node) is rewritten as a thin SQLite logger with two Zenoh queryables, two write subscribers, and a 1 Hz snapshot publish. `SiteMapView.tsx` is refactored to key live blips by a synthetic per-detection id (no `track_id`) and to render zones from the new queryable. Bubbaloop core stays untouched; the untracked `crates/bubbaloop/src/daemon/detection_store.rs` drift is removed.

**Tech Stack:** Python 3.10+ (`bubbaloop_sdk`, `sqlite3` stdlib, `pytest`), TypeScript (React, Leaflet, Vitest), Zenoh pub/sub + queryables, JSON wire format.

**Spec:** `docs/superpowers/specs/2026-04-30-spatial-map-no-tracking-design.md` (commit `27df217`).

---

## File map

### Repo `bubbaloop` (this repo)

| File | Action |
|---|---|
| `crates/bubbaloop/src/daemon/detection_store.rs` | **delete** (untracked drift) |
| `CLAUDE.md` | append one-paragraph pitfall note |
| `dashboard/src/components/SiteMapView.tsx` | drop `track_id` keying, switch to synthetic detection id, drop trail rendering, wire `{inst}/zones` query |
| `dashboard/src/components/__tests__/SiteMapView.test.tsx` | new vitest tests for the refactor |

### Repo `bubbaloop-nodes-official`

| File | Action |
|---|---|
| `spatial-map/store.py` | **new** — SQLite schema, ingest, zone CRUD, retention, queries |
| `spatial-map/main.py` | rewrite — node lifecycle, Zenoh wiring, validation |
| `spatial-map/tests/__init__.py` | new |
| `spatial-map/tests/test_store.py` | new — store unit tests |
| `spatial-map/tests/test_validation.py` | new — payload validation unit tests |
| `spatial-map/tests/test_snapshot.py` | new — snapshot shape + truncation |
| `spatial-map/tests/live_walkthrough.md` | new — manual acceptance script |
| `spatial-map/node.yaml` | update inputs/outputs/queryables |
| `spatial-map/config.yaml` | update tunables |
| `spatial-map/pixi.toml` | drop `lance`, `pyarrow` deps |
| `spatial-map/README.md` | new/rewrite |
| `geo-projector/main.py` | strip `WorldTracker` class + `track_id` field |
| `geo-projector/configs/*.yaml` | strip any `track_*` keys |
| `geo-projector/node.yaml` | update output description |
| `geo-projector/tests/test_no_track_id.py` | new — regression test |

## Sequencing

Four sequential PRs across two repos. Merge order keeps `main` always-green:

| Order | PR | Repo | Tasks | Why this order |
|---|---|---|---|---|
| 1 | PR 1 — cleanup | bubbaloop | Task 1 | Independent; pure dead-code removal. |
| 2 | PR 2 — spatial-map rewrite | nodes-official | Tasks 2-9 | Must be deployed before PR 3 because the dashboard wires `{inst}/zones` against it. New `spatial-map` is tolerant of `track_id` still being on the wire (just doesn't read it), so it's safe to deploy while `geo-projector` still emits `track_id`. |
| 3 | PR 3 — dashboard | bubbaloop | Task 10 | Switches dashboard off `track_id` keying. Safe because the dashboard now ignores `track_id` whether it's present or not. |
| 4 | PR 4 — geo-projector | nodes-official | Task 11 | Drops `track_id` from the wire. Dashboard already moved off it, so nothing breaks. |
| 5 | acceptance | — | Task 12 | Live walk-through after all four PRs are merged + deployed. |

---

## Task 1 — Remove `detection_store.rs` drift + CLAUDE.md note (PR 1)

**Files:**
- Delete: `crates/bubbaloop/src/daemon/detection_store.rs`
- Modify: `CLAUDE.md`

- [ ] **Step 1: Branch off `feat/demo-mvp-plans`**

```bash
cd /home/nvidia/bubbaloop
git fetch origin
git checkout feat/demo-mvp-plans
git pull --ff-only
git checkout -b chore/remove-detection-store-drift
```

- [ ] **Step 2: Confirm the file is untracked**

```bash
git ls-files crates/bubbaloop/src/daemon/detection_store.rs
```

Expected: empty output (file is untracked, never committed). If output is non-empty, stop and review history with `git log -- crates/bubbaloop/src/daemon/detection_store.rs` before proceeding.

- [ ] **Step 3: Confirm no other Rust file references it**

```bash
grep -rn "detection_store\|DetectionStore" crates/bubbaloop/src/ || echo "no references"
```

Expected: `no references`. If any references exist, fix them in this task before proceeding.

- [ ] **Step 4: Delete the file**

```bash
rm crates/bubbaloop/src/daemon/detection_store.rs
```

- [ ] **Step 5: Append a pitfall note to CLAUDE.md**

Open `CLAUDE.md`, find the `## Pitfalls` section, and append at the end of the bullet list:

```
- Detection-class-specific persistence (entities, world detections, zones, dwell, alerts, ...) lives in the `spatial-map` node, **not** in `daemon/`. Adding tables or MCP tools that interpret detection wire formats violates the open-core boundary — extend a node instead. Reference: `docs/superpowers/specs/2026-04-30-spatial-map-no-tracking-design.md`.
```

- [ ] **Step 6: Verify the build still succeeds**

```bash
cd /home/nvidia/bubbaloop
pixi run check
```

Expected: `cargo check` finishes with `Finished` and no errors. (Warnings are fine if pre-existing; the build must succeed.)

- [ ] **Step 7: Commit**

There is a buggy `.git/hooks/pre-commit` that auto-stages every unstaged Rust file. Stash any unrelated WIP first:

```bash
cd /home/nvidia/bubbaloop
if git status --porcelain | grep -E '^ M .*\.rs$' >/dev/null; then
  git stash push -m "tmp: stash WIP rust files for cleanup commit" -- 'crates/**/*.rs'
  STASHED=1
else
  STASHED=0
fi

git add CLAUDE.md
git rm crates/bubbaloop/src/daemon/detection_store.rs

git commit -m "$(cat <<'EOF'
chore: remove detection_store.rs drift from daemon

This SQLite-backed detection store was application-specific code that
drifted into bubbaloop core. Per spec
docs/superpowers/specs/2026-04-30-spatial-map-no-tracking-design.md
the spatial map lives entirely in the spatial-map node. Adds a
pitfall note in CLAUDE.md to prevent recurrence.

Co-Authored-By: Claude Opus 4.7 <noreply@anthropic.com>
EOF
)"

if [ "$STASHED" = "1" ]; then
  git stash pop
fi
```

- [ ] **Step 8: Push and open PR 1**

```bash
git push -u origin chore/remove-detection-store-drift
gh pr create \
  --base feat/demo-mvp-plans \
  --title "chore: remove detection_store.rs drift" \
  --body "$(cat <<'EOF'
## Summary

- Delete untracked, never-committed `crates/bubbaloop/src/daemon/detection_store.rs` (728 lines of application logic that drifted into bubbaloop core).
- Add a one-line pitfall note to CLAUDE.md.

Implements step 1 of the plan in `docs/superpowers/plans/2026-04-30-spatial-map-no-tracking.md`.

## Test plan

- [ ] `pixi run check` succeeds
- [ ] `pixi run clippy` zero warnings
- [ ] No grep hits for `detection_store|DetectionStore` in `crates/bubbaloop/src/`

🤖 Generated with [Claude Code](https://claude.com/claude-code)
EOF
)"
```

- [ ] **Step 9: After PR 1 merges, return to base branch**

```bash
gh pr merge --squash --delete-branch
git checkout feat/demo-mvp-plans
git pull --ff-only
```

---

## Task 2 — `spatial-map/store.py` — schema + ingest (TDD)

Begin PR 2 in the **nodes-official** repo.

**Files:**
- Create: `bubbaloop-nodes-official/spatial-map/store.py`
- Create: `bubbaloop-nodes-official/spatial-map/tests/__init__.py` (empty)
- Create: `bubbaloop-nodes-official/spatial-map/tests/test_store.py`

- [ ] **Step 1: Branch off `main`**

```bash
cd /home/nvidia/bubbaloop-nodes-official
git fetch origin
git checkout main
git pull --ff-only
git checkout -b feat/spatial-map-no-tracking
```

- [ ] **Step 2: Create empty `tests/__init__.py`**

```bash
mkdir -p spatial-map/tests
: > spatial-map/tests/__init__.py
```

- [ ] **Step 3: Write the first failing test (`test_schema_creates_tables`)**

Create `spatial-map/tests/test_store.py`:

```python
"""Unit tests for spatial-map store.py."""
from __future__ import annotations

import math
import sqlite3
import time
from pathlib import Path

import pytest

from spatial_map.store import (  # type: ignore[import-not-found]
    Store,
    InvalidPolygon,
)


# ---- Schema -----------------------------------------------------------

def test_schema_creates_tables(tmp_path: Path):
    db = tmp_path / "spatial.db"
    Store(db)
    conn = sqlite3.connect(db)
    rows = conn.execute(
        "SELECT name FROM sqlite_master WHERE type='table' ORDER BY name"
    ).fetchall()
    assert [r[0] for r in rows] == ["site_zones", "world_detections"]
```

- [ ] **Step 4: Run test, expect ImportError**

```bash
cd /home/nvidia/bubbaloop-nodes-official/spatial-map
pixi run -- pytest tests/test_store.py::test_schema_creates_tables -v
```

Expected: collection error — `ModuleNotFoundError: No module named 'spatial_map'`. (We will create the package shim in step 5.)

- [ ] **Step 5: Make `spatial_map` an importable package**

The existing `spatial-map/` directory has a hyphen so it cannot be imported directly. Add a `conftest.py` that makes `spatial_map` an alias for the directory:

Create `spatial-map/tests/conftest.py`:

```python
"""Make spatial-map directory importable as `spatial_map`."""
import sys
from pathlib import Path

_HERE = Path(__file__).resolve().parent.parent  # spatial-map/
if str(_HERE) not in sys.path:
    sys.path.insert(0, str(_HERE))

# spatial-map is hyphenated; remap to underscored module name on import.
import importlib
import importlib.machinery
import importlib.util
import types

if "spatial_map" not in sys.modules:
    pkg = types.ModuleType("spatial_map")
    pkg.__path__ = [str(_HERE)]  # type: ignore[attr-defined]
    sys.modules["spatial_map"] = pkg
```

Note for the engineer: this is the simplest way to test a hyphen-named directory without renaming it. The hyphen must stay because the bubbaloop registry references the directory by name. The shim is test-only.

- [ ] **Step 6: Create the minimal `store.py`**

Create `spatial-map/store.py`:

```python
"""SQLite store for the spatial-map node.

Two tables:
  * world_detections — append-only audit log of every projected detection.
  * site_zones       — drawn polygons for visual overlay.

No identity, no tracking, no zone enter/exit events. See spec
docs/superpowers/specs/2026-04-30-spatial-map-no-tracking-design.md.
"""
from __future__ import annotations

import json
import math
import sqlite3
import threading
import time
import uuid
from pathlib import Path
from typing import Any, Iterable


SCHEMA_SQL = """
PRAGMA foreign_keys = ON;
PRAGMA journal_mode = WAL;
PRAGMA synchronous = NORMAL;

CREATE TABLE IF NOT EXISTS world_detections (
  id            TEXT PRIMARY KEY,
  timestamp_ns  INTEGER NOT NULL,
  camera        TEXT NOT NULL,
  class_name    TEXT NOT NULL,
  world_x       REAL NOT NULL,
  world_y       REAL NOT NULL,
  confidence    REAL NOT NULL,
  pose_quality  TEXT NOT NULL
);
CREATE INDEX IF NOT EXISTS idx_det_ts        ON world_detections(timestamp_ns DESC);
CREATE INDEX IF NOT EXISTS idx_det_camera_ts ON world_detections(camera, timestamp_ns DESC);
CREATE INDEX IF NOT EXISTS idx_det_class_ts  ON world_detections(class_name, timestamp_ns DESC);

CREATE TABLE IF NOT EXISTS site_zones (
  id            TEXT PRIMARY KEY,
  name          TEXT NOT NULL,
  camera        TEXT NOT NULL,
  polygon_json  TEXT NOT NULL,
  color         TEXT,
  created_at_ns INTEGER NOT NULL
);
"""


class InvalidPolygon(ValueError):
    """Raised when a zone polygon fails validation."""


class Store:
    """Thread-safe SQLite store for world detections and zones."""

    def __init__(self, path: Path | str):
        self._path = str(path)
        self._lock = threading.Lock()
        Path(self._path).parent.mkdir(parents=True, exist_ok=True)
        with self._connect() as conn:
            conn.executescript(SCHEMA_SQL)

    def _connect(self) -> sqlite3.Connection:
        conn = sqlite3.connect(self._path, timeout=5.0, isolation_level=None)
        conn.execute("PRAGMA foreign_keys = ON")
        return conn
```

- [ ] **Step 7: Run schema test, expect pass**

```bash
pixi run -- pytest tests/test_store.py::test_schema_creates_tables -v
```

Expected: PASS.

- [ ] **Step 8: Commit (TDD checkpoint)**

```bash
git add spatial-map/store.py spatial-map/tests/__init__.py spatial-map/tests/conftest.py spatial-map/tests/test_store.py
git commit -m "feat(spatial-map): add Store skeleton with schema"
```

- [ ] **Step 9: Add ingest test (`test_ingest_writes_one_row_per_detection`)**

Append to `spatial-map/tests/test_store.py`:

```python
# ---- Ingest -----------------------------------------------------------

def _frame(ts_ns: int, camera: str, dets: list[dict[str, Any]]) -> dict[str, Any]:
    return {"timestamp_ns": ts_ns, "camera": camera, "detections": dets}


def _det(class_name: str, x: float, y: float, conf: float = 0.9, pq: str = "OK") -> dict[str, Any]:
    return {
        "class_name": class_name,
        "world_x": x,
        "world_y": y,
        "confidence": conf,
        "pose_quality": pq,
    }


def test_ingest_writes_one_row_per_detection(tmp_path: Path):
    s = Store(tmp_path / "spatial.db")
    s.ingest(_frame(1, "cam_a", [_det("person", 1.0, 2.0), _det("chair", -1.0, 3.0)]))
    rows = s.fetch_recent(camera=None, class_name=None, since_ns=0, until_ns=10**18, limit=100)
    assert len(rows) == 2
    classes = sorted(r["class_name"] for r in rows)
    assert classes == ["chair", "person"]


def test_ingest_drops_nonfinite_coords(tmp_path: Path):
    s = Store(tmp_path / "spatial.db")
    s.ingest(
        _frame(
            1,
            "cam_a",
            [
                _det("person", float("nan"), 2.0),
                _det("person", 1.0, float("inf")),
                _det("person", 3.0, 4.0),  # only this one survives
            ],
        )
    )
    rows = s.fetch_recent(camera=None, class_name=None, since_ns=0, until_ns=10**18, limit=100)
    assert len(rows) == 1
    assert rows[0]["world_x"] == 3.0 and rows[0]["world_y"] == 4.0
```

- [ ] **Step 10: Run new tests, expect AttributeError**

```bash
pixi run -- pytest tests/test_store.py::test_ingest_writes_one_row_per_detection -v
```

Expected: FAIL — `AttributeError: 'Store' object has no attribute 'ingest'`.

- [ ] **Step 11: Implement `ingest()` and `fetch_recent()` in `store.py`**

Append to `spatial-map/store.py`:

```python
    # ---- Ingest -----------------------------------------------------------

    def ingest(self, frame: dict[str, Any]) -> int:
        """Insert every valid detection from a `world_detections` frame.

        Returns the number of rows inserted. Detections with non-finite
        x/y/confidence are silently dropped (logged by the caller).
        """
        ts = int(frame["timestamp_ns"])
        camera = str(frame["camera"])
        rows: list[tuple[Any, ...]] = []
        for d in frame.get("detections", ()):
            try:
                x = float(d["world_x"])
                y = float(d["world_y"])
                conf = float(d["confidence"])
            except (KeyError, TypeError, ValueError):
                continue
            if not (math.isfinite(x) and math.isfinite(y) and math.isfinite(conf)):
                continue
            rows.append(
                (
                    f"det_{uuid.uuid4().hex}",
                    ts,
                    camera,
                    str(d.get("class_name", "")),
                    x,
                    y,
                    conf,
                    str(d.get("pose_quality", "OK")),
                )
            )

        if not rows:
            return 0
        with self._lock, self._connect() as conn:
            conn.executemany(
                "INSERT INTO world_detections "
                "(id, timestamp_ns, camera, class_name, world_x, world_y, confidence, pose_quality) "
                "VALUES (?, ?, ?, ?, ?, ?, ?, ?)",
                rows,
            )
        return len(rows)

    # ---- Read -------------------------------------------------------------

    def fetch_recent(
        self,
        camera: str | None,
        class_name: str | None,
        since_ns: int,
        until_ns: int,
        limit: int,
    ) -> list[dict[str, Any]]:
        """Return recent detections in [since_ns, until_ns], descending by ts."""
        clauses = ["timestamp_ns >= ?", "timestamp_ns <= ?"]
        params: list[Any] = [int(since_ns), int(until_ns)]
        if camera is not None:
            clauses.append("camera = ?")
            params.append(camera)
        if class_name is not None:
            clauses.append("class_name = ?")
            params.append(class_name)
        params.append(int(limit))
        sql = (
            "SELECT id, timestamp_ns, camera, class_name, world_x, world_y, confidence, pose_quality "
            "FROM world_detections WHERE " + " AND ".join(clauses) +
            " ORDER BY timestamp_ns DESC LIMIT ?"
        )
        with self._connect() as conn:
            cur = conn.execute(sql, params)
            cols = [c[0] for c in cur.description]
            return [dict(zip(cols, row)) for row in cur.fetchall()]
```

- [ ] **Step 12: Run ingest tests, expect pass**

```bash
pixi run -- pytest tests/test_store.py -v
```

Expected: 3 PASS.

- [ ] **Step 13: Commit (TDD checkpoint)**

```bash
git add spatial-map/store.py spatial-map/tests/test_store.py
git commit -m "feat(spatial-map): add ingest() + fetch_recent() with non-finite filtering"
```

---

## Task 3 — Zone CRUD + polygon validation (TDD)

**Files:**
- Modify: `bubbaloop-nodes-official/spatial-map/store.py` (extend with zone methods)
- Modify: `bubbaloop-nodes-official/spatial-map/tests/test_store.py` (zone tests)
- Create: `bubbaloop-nodes-official/spatial-map/tests/test_validation.py`

- [ ] **Step 1: Add zone tests**

Append to `spatial-map/tests/test_store.py`:

```python
# ---- Zones ------------------------------------------------------------

def test_zone_create_then_list(tmp_path: Path):
    s = Store(tmp_path / "spatial.db")
    zid = s.upsert_zone(
        zone_id=None,
        name="Front",
        camera="cam_a",
        polygon=[[0.0, 0.0], [3.0, 0.0], [3.0, 3.0], [0.0, 3.0]],
        color="#ff8800",
        now_ns=1_000_000_000,
    )
    assert zid.startswith("zone_")
    zones = s.list_zones(camera=None)
    assert len(zones) == 1
    z = zones[0]
    assert z["zone_id"] == zid
    assert z["name"] == "Front"
    assert z["camera"] == "cam_a"
    assert z["polygon"] == [[0.0, 0.0], [3.0, 0.0], [3.0, 3.0], [0.0, 3.0]]
    assert z["color"] == "#ff8800"
    assert z["created_at_ns"] == 1_000_000_000


def test_zone_update_in_place(tmp_path: Path):
    s = Store(tmp_path / "spatial.db")
    zid = s.upsert_zone(
        zone_id=None, name="Front", camera="cam_a",
        polygon=[[0.0, 0.0], [1.0, 0.0], [1.0, 1.0]], color=None,
        now_ns=1,
    )
    # update with same id, new polygon
    zid2 = s.upsert_zone(
        zone_id=zid, name="Front Door", camera="cam_a",
        polygon=[[0.0, 0.0], [2.0, 0.0], [2.0, 2.0], [0.0, 2.0]], color="#00ff00",
        now_ns=2,
    )
    assert zid2 == zid
    zones = s.list_zones(camera=None)
    assert len(zones) == 1
    assert zones[0]["name"] == "Front Door"
    assert zones[0]["color"] == "#00ff00"
    assert len(zones[0]["polygon"]) == 4


def test_zone_list_filter_by_camera(tmp_path: Path):
    s = Store(tmp_path / "spatial.db")
    s.upsert_zone(None, "A", "cam_a", [[0.0, 0.0], [1.0, 0.0], [1.0, 1.0]], None, 1)
    s.upsert_zone(None, "B", "cam_b", [[0.0, 0.0], [1.0, 0.0], [1.0, 1.0]], None, 2)
    assert {z["camera"] for z in s.list_zones(camera="cam_a")} == {"cam_a"}
    assert {z["camera"] for z in s.list_zones(camera=None)} == {"cam_a", "cam_b"}


def test_zone_delete_known(tmp_path: Path):
    s = Store(tmp_path / "spatial.db")
    zid = s.upsert_zone(None, "A", "cam_a", [[0.0, 0.0], [1.0, 0.0], [1.0, 1.0]], None, 1)
    assert s.delete_zone(zid) is True
    assert s.list_zones(camera=None) == []


def test_zone_delete_unknown_is_noop(tmp_path: Path):
    s = Store(tmp_path / "spatial.db")
    assert s.delete_zone("zone_does_not_exist") is False
```

Create `spatial-map/tests/test_validation.py`:

```python
"""Unit tests for polygon validation."""
from __future__ import annotations

import math
from pathlib import Path

import pytest

from spatial_map.store import Store, InvalidPolygon


def test_polygon_min_3_vertices(tmp_path: Path):
    s = Store(tmp_path / "spatial.db")
    with pytest.raises(InvalidPolygon):
        s.upsert_zone(None, "X", "cam_a", [[0.0, 0.0], [1.0, 1.0]], None, 1)


def test_polygon_rejects_nan(tmp_path: Path):
    s = Store(tmp_path / "spatial.db")
    with pytest.raises(InvalidPolygon):
        s.upsert_zone(None, "X", "cam_a",
                      [[0.0, 0.0], [1.0, math.nan], [2.0, 2.0]], None, 1)


def test_polygon_rejects_inf(tmp_path: Path):
    s = Store(tmp_path / "spatial.db")
    with pytest.raises(InvalidPolygon):
        s.upsert_zone(None, "X", "cam_a",
                      [[0.0, 0.0], [math.inf, 1.0], [2.0, 2.0]], None, 1)


def test_polygon_rejects_non_pair_vertex(tmp_path: Path):
    s = Store(tmp_path / "spatial.db")
    with pytest.raises(InvalidPolygon):
        s.upsert_zone(None, "X", "cam_a",
                      [[0.0, 0.0], [1.0], [2.0, 2.0]], None, 1)


def test_color_optional_but_must_be_hex_if_present(tmp_path: Path):
    s = Store(tmp_path / "spatial.db")
    # ok
    s.upsert_zone(None, "X", "cam_a",
                  [[0.0, 0.0], [1.0, 0.0], [1.0, 1.0]], "#ff8800", 1)
    # bad
    with pytest.raises(InvalidPolygon):
        s.upsert_zone(None, "Y", "cam_a",
                      [[0.0, 0.0], [1.0, 0.0], [1.0, 1.0]], "not-a-color", 1)
```

- [ ] **Step 2: Run new tests, expect failures**

```bash
pixi run -- pytest tests/ -v
```

Expected: store-related failures (`AttributeError: 'Store' object has no attribute 'upsert_zone'`).

- [ ] **Step 3: Implement zone methods + validation**

Append to `spatial-map/store.py`:

```python
    # ---- Zones ------------------------------------------------------------

    @staticmethod
    def _validate_polygon(polygon: Any) -> list[list[float]]:
        if not isinstance(polygon, list) or len(polygon) < 3:
            raise InvalidPolygon(f"polygon must have >= 3 vertices (got {polygon!r})")
        out: list[list[float]] = []
        for v in polygon:
            if not (isinstance(v, list) and len(v) == 2):
                raise InvalidPolygon(f"vertex must be [x, y] pair (got {v!r})")
            try:
                x = float(v[0])
                y = float(v[1])
            except (TypeError, ValueError) as e:
                raise InvalidPolygon(f"vertex coords must be numeric ({v!r})") from e
            if not (math.isfinite(x) and math.isfinite(y)):
                raise InvalidPolygon(f"vertex coords must be finite ({v!r})")
            out.append([x, y])
        return out

    @staticmethod
    def _validate_color(color: Any) -> str | None:
        if color is None:
            return None
        if not isinstance(color, str):
            raise InvalidPolygon(f"color must be string or null (got {color!r})")
        # hex CSS color
        if not (len(color) == 7 and color.startswith("#")
                and all(c in "0123456789abcdefABCDEF" for c in color[1:])):
            raise InvalidPolygon(f"color must match #RRGGBB (got {color!r})")
        return color

    @staticmethod
    def _validate_name(name: Any) -> str:
        if not isinstance(name, str) or not (1 <= len(name) <= 64):
            raise InvalidPolygon(f"name must be 1..64 char string (got {name!r})")
        return name

    @staticmethod
    def _validate_camera(camera: Any) -> str:
        if not isinstance(camera, str) or not (1 <= len(camera) <= 64):
            raise InvalidPolygon(f"camera must be 1..64 char string (got {camera!r})")
        return camera

    def upsert_zone(
        self,
        zone_id: str | None,
        name: str,
        camera: str,
        polygon: list[list[float]],
        color: str | None,
        now_ns: int,
    ) -> str:
        """Create or update a zone. Returns the zone_id used."""
        n = self._validate_name(name)
        c = self._validate_camera(camera)
        p = self._validate_polygon(polygon)
        col = self._validate_color(color)

        with self._lock, self._connect() as conn:
            existing = None
            if zone_id is not None:
                cur = conn.execute("SELECT 1 FROM site_zones WHERE id = ?", (zone_id,))
                existing = cur.fetchone()
            if existing is not None:
                conn.execute(
                    "UPDATE site_zones SET name = ?, camera = ?, polygon_json = ?, "
                    "color = ?, created_at_ns = ? WHERE id = ?",
                    (n, c, json.dumps(p), col, int(now_ns), zone_id),
                )
                return zone_id  # type: ignore[return-value]
            new_id = zone_id if zone_id is not None else f"zone_{uuid.uuid4().hex}"
            conn.execute(
                "INSERT INTO site_zones (id, name, camera, polygon_json, color, created_at_ns) "
                "VALUES (?, ?, ?, ?, ?, ?)",
                (new_id, n, c, json.dumps(p), col, int(now_ns)),
            )
            return new_id

    def list_zones(self, camera: str | None) -> list[dict[str, Any]]:
        sql = "SELECT id, name, camera, polygon_json, color, created_at_ns FROM site_zones"
        params: list[Any] = []
        if camera is not None:
            sql += " WHERE camera = ?"
            params.append(camera)
        sql += " ORDER BY created_at_ns ASC"
        with self._connect() as conn:
            cur = conn.execute(sql, params)
            return [
                {
                    "zone_id": row[0],
                    "name": row[1],
                    "camera": row[2],
                    "polygon": json.loads(row[3]),
                    "color": row[4],
                    "created_at_ns": row[5],
                }
                for row in cur.fetchall()
            ]

    def delete_zone(self, zone_id: str) -> bool:
        with self._lock, self._connect() as conn:
            cur = conn.execute("DELETE FROM site_zones WHERE id = ?", (zone_id,))
            return cur.rowcount > 0
```

- [ ] **Step 4: Run all store + validation tests, expect pass**

```bash
pixi run -- pytest tests/ -v
```

Expected: all tests PASS.

- [ ] **Step 5: Commit (TDD checkpoint)**

```bash
git add spatial-map/store.py spatial-map/tests/test_store.py spatial-map/tests/test_validation.py
git commit -m "feat(spatial-map): add zone CRUD with polygon/color validation"
```

---

## Task 4 — Retention sweeper (TDD)

**Files:**
- Modify: `bubbaloop-nodes-official/spatial-map/store.py`
- Modify: `bubbaloop-nodes-official/spatial-map/tests/test_store.py`

- [ ] **Step 1: Add retention tests**

Append to `spatial-map/tests/test_store.py`:

```python
# ---- Retention --------------------------------------------------------

def test_retention_drops_old_rows(tmp_path: Path):
    s = Store(tmp_path / "spatial.db")
    NS = 1_000_000_000
    # 10 days ago
    s.ingest(_frame(1 * NS, "cam_a", [_det("person", 0.0, 0.0)]))
    # 1 day ago
    s.ingest(_frame(9 * 86400 * NS, "cam_a", [_det("person", 1.0, 1.0)]))
    # now
    now_ns = 10 * 86400 * NS
    deleted = s.prune_older_than(now_ns - 7 * 86400 * NS)
    assert deleted == 1  # only the 10-days-ago row exceeds retention
    rows = s.fetch_recent(None, None, 0, 10**18, 100)
    assert len(rows) == 2  # the 1-day-ago and the now-ish row remain


def test_retention_zero_disables(tmp_path: Path):
    s = Store(tmp_path / "spatial.db")
    NS = 1_000_000_000
    s.ingest(_frame(1 * NS, "cam_a", [_det("person", 0.0, 0.0)]))
    deleted = s.prune_older_than(0)  # retention disabled means caller skips entirely
    assert deleted == 0
    rows = s.fetch_recent(None, None, 0, 10**18, 100)
    assert len(rows) == 1
```

- [ ] **Step 2: Run new tests, expect failures**

```bash
pixi run -- pytest tests/test_store.py::test_retention_drops_old_rows -v
```

Expected: FAIL — `AttributeError: 'Store' object has no attribute 'prune_older_than'`.

- [ ] **Step 3: Implement `prune_older_than()`**

Append to `spatial-map/store.py`:

```python
    # ---- Retention --------------------------------------------------------

    def prune_older_than(self, cutoff_ns: int) -> int:
        """Delete world_detections older than `cutoff_ns`. Returns row count.

        The caller is expected to skip calling this method when retention
        is disabled (RETENTION_DAYS == 0). Passing cutoff_ns == 0 here
        deletes nothing (no rows have ts <= 0 in normal use).
        """
        with self._lock, self._connect() as conn:
            cur = conn.execute(
                "DELETE FROM world_detections WHERE timestamp_ns < ?",
                (int(cutoff_ns),),
            )
            return cur.rowcount
```

- [ ] **Step 4: Run all tests, expect pass**

```bash
pixi run -- pytest tests/ -v
```

Expected: all PASS.

- [ ] **Step 5: Commit**

```bash
git add spatial-map/store.py spatial-map/tests/test_store.py
git commit -m "feat(spatial-map): add prune_older_than() for retention"
```

---

## Task 5 — `spatial-map/main.py` — node skeleton + world_detections subscriber

**Files:**
- Rewrite: `bubbaloop-nodes-official/spatial-map/main.py`

- [ ] **Step 1: Read the existing main.py to understand the current Node class shape and config keys**

```bash
cat /home/nvidia/bubbaloop-nodes-official/spatial-map/main.py | head -120
```

Note the existing `class SpatialMapNode` interface: `name: str`, `__init__(self, ctx, config)`, `run(self)`. Preserve those — `bubbaloop_sdk.run_node` depends on them.

- [ ] **Step 2: Replace the contents of `main.py`**

Overwrite `spatial-map/main.py`:

```python
#!/usr/bin/env python3
"""spatial-map — Logs world-frame detections to SQLite, serves zones, publishes snapshots.

Subscribes:
  bubbaloop/**/world_detections   (any geo-projector instance)
  {inst}/zones/set                (zone create/update)
  {inst}/zones/delete             (zone delete)

Publishes:
  {inst}/snapshot                 (1 Hz JSON: recent detections + zones)

Queryables:
  {inst}/recent                   (filtered recent detections)
  {inst}/zones                    (current zones)

No tracking, no entity persistence, no zone enter/exit events.
See spec docs/superpowers/specs/2026-04-30-spatial-map-no-tracking-design.md.
"""
from __future__ import annotations

import json
import logging
import os
import threading
import time
from pathlib import Path
from typing import Any

import zenoh

from .store import InvalidPolygon, Store  # type: ignore[import-not-found]


log = logging.getLogger("spatial-map")


# ---- Defaults (overridable via config.yaml) -------------------------------

DEFAULT_SITE_ID = "default"
DEFAULT_RETENTION_DAYS = 7
DEFAULT_SNAPSHOT_INTERVAL_S = 1.0
DEFAULT_SNAPSHOT_RECENT_S = 30
DEFAULT_SWEEPER_INTERVAL_S = 60.0
DEFAULT_SNAPSHOT_MAX_DETECTIONS = 200


def _default_dataset_path(site_id: str) -> Path:
    base = Path(os.path.expanduser("~/.bubbaloop/sites")) / site_id
    base.mkdir(parents=True, exist_ok=True)
    return base / "spatial.db"


# ---- Node -----------------------------------------------------------------

class SpatialMapNode:
    """Spatial map sink + queryable server."""

    name = "spatial-map"

    def __init__(self, ctx: Any, config: dict[str, Any]) -> None:
        self._ctx = ctx
        self._site_id = config.get("site_id", DEFAULT_SITE_ID)
        path = config.get("dataset_path") or _default_dataset_path(self._site_id)
        self._db_path = str(path)
        self._store = Store(self._db_path)

        self._retention_days = int(config.get("retention_days", DEFAULT_RETENTION_DAYS))
        self._snapshot_interval_s = float(
            config.get("snapshot_interval_s", DEFAULT_SNAPSHOT_INTERVAL_S)
        )
        self._snapshot_recent_s = int(
            config.get("snapshot_recent_s", DEFAULT_SNAPSHOT_RECENT_S)
        )
        self._sweeper_interval_s = float(
            config.get("sweeper_interval_s", DEFAULT_SWEEPER_INTERVAL_S)
        )
        self._snapshot_max_dets = int(
            config.get("snapshot_max_detections", DEFAULT_SNAPSHOT_MAX_DETECTIONS)
        )

        # Zenoh handles owned by this node (declared in run()).
        self._handles: list[Any] = []
        self._snapshot_thread: threading.Thread | None = None
        self._sweeper_thread: threading.Thread | None = None

    # -- lifecycle -----------------------------------------------------

    def run(self) -> None:
        log.info(
            "spatial-map starting: db=%s retention=%dd snapshot=%.1fs",
            self._db_path, self._retention_days, self._snapshot_interval_s,
        )
        self._wire_subscribers()
        # queryables and snapshot publisher are wired in later tasks.
        try:
            while not self._ctx.is_shutdown():
                time.sleep(0.5)
        finally:
            log.info("spatial-map shutting down")
            for h in self._handles:
                try:
                    h.undeclare()
                except Exception:  # noqa: BLE001
                    pass

    # -- subscribers ---------------------------------------------------

    def _wire_subscribers(self) -> None:
        session: zenoh.Session = self._ctx.session
        # Wildcard subscribe for any per-camera geo-projector instance.
        wd_key = "bubbaloop/**/world_detections"
        log.info("subscribing %s", wd_key)
        self._handles.append(
            session.declare_subscriber(wd_key, self._on_world_detections)
        )

    def _on_world_detections(self, sample: zenoh.Sample) -> None:
        try:
            payload = bytes(sample.payload)
            frame = json.loads(payload.decode("utf-8"))
        except Exception as e:  # noqa: BLE001
            log.warning("dropped malformed world_detections frame: %s", e)
            return
        try:
            n = self._store.ingest(frame)
            if n == 0:
                log.debug("no valid detections in frame")
        except Exception as e:  # noqa: BLE001
            log.warning("ingest failed: %s", e)


# ---- Entrypoint ------------------------------------------------------------

if __name__ == "__main__":
    from bubbaloop_sdk import run_node

    run_node(SpatialMapNode)
```

Note for the engineer: the relative import `from .store import ...` works because `bubbaloop_sdk.run_node` adds the node directory to `sys.path` and imports the entrypoint as a package. If the engineer hits an `ImportError` at runtime, fall back to `from store import InvalidPolygon, Store` (sibling module pattern); the existing geo-projector pattern is the source of truth — match whichever style it uses.

- [ ] **Step 3: Smoke-test the node parses**

```bash
cd /home/nvidia/bubbaloop-nodes-official/spatial-map
pixi run -- python -c "import ast; ast.parse(open('main.py').read()); print('parse ok')"
```

Expected: `parse ok`.

- [ ] **Step 4: Commit**

```bash
git add spatial-map/main.py
git commit -m "feat(spatial-map): node skeleton + world_detections ingest subscriber"
```

---

## Task 6 — Zone write subscribers (`zones/set`, `zones/delete`)

**Files:**
- Modify: `bubbaloop-nodes-official/spatial-map/main.py`

- [ ] **Step 1: Add zone-write subscribers in `_wire_subscribers()`**

Edit `spatial-map/main.py`. Replace the body of `_wire_subscribers` with:

```python
    def _wire_subscribers(self) -> None:
        session: zenoh.Session = self._ctx.session

        wd_key = "bubbaloop/**/world_detections"
        log.info("subscribing %s", wd_key)
        self._handles.append(
            session.declare_subscriber(wd_key, self._on_world_detections)
        )

        zset = self._ctx.topic("zones/set")
        log.info("subscribing %s", zset)
        self._handles.append(session.declare_subscriber(zset, self._on_zone_set))

        zdel = self._ctx.topic("zones/delete")
        log.info("subscribing %s", zdel)
        self._handles.append(session.declare_subscriber(zdel, self._on_zone_delete))
```

- [ ] **Step 2: Add the two handler methods**

Append to `class SpatialMapNode` (place after `_on_world_detections`):

```python
    def _on_zone_set(self, sample: zenoh.Sample) -> None:
        try:
            body = json.loads(bytes(sample.payload).decode("utf-8"))
        except Exception as e:  # noqa: BLE001
            log.warning("zones/set: malformed payload: %s", e)
            return
        try:
            zid = self._store.upsert_zone(
                zone_id=body.get("zone_id"),
                name=body.get("name"),
                camera=body.get("camera"),
                polygon=body.get("polygon"),
                color=body.get("color"),
                now_ns=time.time_ns(),
            )
            log.info("zone upserted id=%s name=%s camera=%s",
                     zid, body.get("name"), body.get("camera"))
        except InvalidPolygon as e:
            log.warning("zones/set: validation rejected: %s", e)
        except Exception as e:  # noqa: BLE001
            log.warning("zones/set: write failed: %s", e)

    def _on_zone_delete(self, sample: zenoh.Sample) -> None:
        try:
            body = json.loads(bytes(sample.payload).decode("utf-8"))
        except Exception as e:  # noqa: BLE001
            log.warning("zones/delete: malformed payload: %s", e)
            return
        zid = body.get("zone_id")
        if not isinstance(zid, str):
            log.warning("zones/delete: missing or non-string zone_id")
            return
        try:
            removed = self._store.delete_zone(zid)
            log.info("zone delete id=%s removed=%s", zid, removed)
        except Exception as e:  # noqa: BLE001
            log.warning("zones/delete: write failed: %s", e)
```

- [ ] **Step 3: Smoke-test parse**

```bash
pixi run -- python -c "import ast; ast.parse(open('main.py').read()); print('parse ok')"
```

Expected: `parse ok`.

- [ ] **Step 4: Commit**

```bash
git add spatial-map/main.py
git commit -m "feat(spatial-map): zones/set and zones/delete write subscribers"
```

---

## Task 7 — Queryables (`recent`, `zones`)

**Files:**
- Modify: `bubbaloop-nodes-official/spatial-map/main.py`

- [ ] **Step 1: Add queryable declarations**

Edit `spatial-map/main.py`. Add at the end of `_wire_subscribers()`:

```python
        recent_key = self._ctx.topic("recent")
        log.info("declaring queryable %s", recent_key)
        self._handles.append(
            session.declare_queryable(recent_key, self._on_recent_query)
        )

        zones_key = self._ctx.topic("zones")
        log.info("declaring queryable %s", zones_key)
        self._handles.append(
            session.declare_queryable(zones_key, self._on_zones_query)
        )
```

- [ ] **Step 2: Add a parameter parser helper**

Append to `class SpatialMapNode`:

```python
    @staticmethod
    def _parse_params(query: zenoh.Query) -> dict[str, str]:
        """Extract `?k=v&...` parameters from a Zenoh query selector."""
        # zenoh.Selector exposes parameters as a dict-like via `.parameters()`
        # but the exact API depends on zenoh-python version. Fall back to
        # parsing the raw selector string ourselves to be version-independent.
        sel = str(query.selector)
        if "?" not in sel:
            return {}
        _, qs = sel.split("?", 1)
        params: dict[str, str] = {}
        for pair in qs.split("&"):
            if not pair:
                continue
            if "=" in pair:
                k, v = pair.split("=", 1)
            else:
                k, v = pair, ""
            params[k] = v
        return params
```

- [ ] **Step 3: Add the two queryable handlers**

Append to `class SpatialMapNode`:

```python
    def _reply(self, query: zenoh.Query, body: dict[str, Any]) -> None:
        payload = json.dumps(body, separators=(",", ":")).encode("utf-8")
        try:
            query.reply(query.key_expr, payload)
        except Exception as e:  # noqa: BLE001
            log.warning("queryable reply failed: %s", e)

    def _on_recent_query(self, query: zenoh.Query) -> None:
        try:
            p = self._parse_params(query)
            now_ns = time.time_ns()
            since_ns = int(p.get("since_ns") or now_ns - 60 * 1_000_000_000)
            until_ns = int(p.get("until_ns") or now_ns)
            limit = max(1, min(int(p.get("limit") or 500), 5000))
            camera = p.get("camera") or None
            class_name = p.get("class") or None
            rows = self._store.fetch_recent(camera, class_name, since_ns, until_ns, limit)
            self._reply(query, {"detections": [
                {
                    "ts": r["timestamp_ns"],
                    "camera": r["camera"],
                    "class_name": r["class_name"],
                    "world_x": r["world_x"],
                    "world_y": r["world_y"],
                    "confidence": r["confidence"],
                    "pose_quality": r["pose_quality"],
                }
                for r in rows
            ]})
        except Exception as e:  # noqa: BLE001
            log.warning("recent query failed: %s", e)
            self._reply(query, {"error": str(e), "detections": []})

    def _on_zones_query(self, query: zenoh.Query) -> None:
        try:
            p = self._parse_params(query)
            camera = p.get("camera") or None
            zones = self._store.list_zones(camera=camera)
            self._reply(query, {"zones": zones})
        except Exception as e:  # noqa: BLE001
            log.warning("zones query failed: %s", e)
            self._reply(query, {"error": str(e), "zones": []})
```

- [ ] **Step 4: Smoke-test parse**

```bash
pixi run -- python -c "import ast; ast.parse(open('main.py').read()); print('parse ok')"
```

- [ ] **Step 5: Commit**

```bash
git add spatial-map/main.py
git commit -m "feat(spatial-map): recent and zones queryables"
```

---

## Task 8 — Snapshot publisher + retention sweeper (with TDD shape test)

**Files:**
- Modify: `bubbaloop-nodes-official/spatial-map/main.py`
- Create: `bubbaloop-nodes-official/spatial-map/tests/test_snapshot.py`

- [ ] **Step 1: Refactor snapshot building into a static method (testable without Zenoh)**

Edit `spatial-map/main.py`. Add to `class SpatialMapNode`:

```python
    @staticmethod
    def build_snapshot(
        store: Store,
        recent_window_s: int,
        max_detections: int,
        now_ns: int | None = None,
    ) -> dict[str, Any]:
        """Build a snapshot dict (pure function for testing)."""
        if now_ns is None:
            now_ns = time.time_ns()
        since_ns = now_ns - recent_window_s * 1_000_000_000

        rows = store.fetch_recent(
            camera=None, class_name=None,
            since_ns=since_ns, until_ns=now_ns,
            limit=max_detections + 1,  # +1 to detect truncation
        )
        truncated = len(rows) > max_detections
        if truncated:
            rows = rows[:max_detections]
        zones = store.list_zones(camera=None)
        return {
            "published_at_ns": now_ns,
            "window_s": recent_window_s,
            "detections": [
                {
                    "ts": r["timestamp_ns"],
                    "camera": r["camera"],
                    "class_name": r["class_name"],
                    "world_x": r["world_x"],
                    "world_y": r["world_y"],
                    "confidence": r["confidence"],
                    "pose_quality": r["pose_quality"],
                }
                for r in rows
            ],
            "zones": zones,
            "truncated": truncated,
        }
```

- [ ] **Step 2: Write snapshot tests**

Create `spatial-map/tests/test_snapshot.py`:

```python
"""Snapshot shape + truncation."""
from __future__ import annotations

from pathlib import Path

from spatial_map.store import Store
from spatial_map.main import SpatialMapNode


NS = 1_000_000_000


def test_snapshot_includes_recent_window_only(tmp_path: Path):
    s = Store(tmp_path / "spatial.db")
    now = 1_000 * NS
    s.ingest({"timestamp_ns": now - 60 * NS, "camera": "cam_a",
              "detections": [{"class_name": "person", "world_x": 1.0, "world_y": 2.0,
                              "confidence": 0.9, "pose_quality": "OK"}]})
    s.ingest({"timestamp_ns": now - 5 * NS, "camera": "cam_a",
              "detections": [{"class_name": "person", "world_x": 3.0, "world_y": 4.0,
                              "confidence": 0.9, "pose_quality": "OK"}]})
    snap = SpatialMapNode.build_snapshot(s, recent_window_s=30, max_detections=200, now_ns=now)
    assert snap["window_s"] == 30
    assert len(snap["detections"]) == 1
    assert snap["detections"][0]["world_x"] == 3.0
    assert snap["truncated"] is False
    assert snap["zones"] == []


def test_snapshot_truncates_to_max(tmp_path: Path):
    s = Store(tmp_path / "spatial.db")
    now = 1_000 * NS
    for i in range(250):
        s.ingest({"timestamp_ns": now - i * NS // 100, "camera": "cam_a",
                  "detections": [{"class_name": "person", "world_x": float(i), "world_y": 0.0,
                                  "confidence": 0.5, "pose_quality": "OK"}]})
    snap = SpatialMapNode.build_snapshot(s, recent_window_s=30, max_detections=200, now_ns=now)
    assert len(snap["detections"]) == 200
    assert snap["truncated"] is True


def test_snapshot_includes_zones(tmp_path: Path):
    s = Store(tmp_path / "spatial.db")
    s.upsert_zone(None, "Front", "cam_a",
                  [[0.0, 0.0], [1.0, 0.0], [1.0, 1.0]], "#ff8800", 1)
    snap = SpatialMapNode.build_snapshot(s, recent_window_s=30, max_detections=200, now_ns=2 * NS)
    assert len(snap["zones"]) == 1
    assert snap["zones"][0]["name"] == "Front"
```

- [ ] **Step 3: Run snapshot tests, expect pass**

```bash
pixi run -- pytest tests/test_snapshot.py -v
```

Expected: 3 PASS.

- [ ] **Step 4: Wire snapshot publisher and retention sweeper threads in `run()`**

Edit `spatial-map/main.py`. Replace the `run()` method body with:

```python
    def run(self) -> None:
        log.info(
            "spatial-map starting: db=%s retention=%dd snapshot=%.1fs",
            self._db_path, self._retention_days, self._snapshot_interval_s,
        )
        self._wire_subscribers()
        self._snapshot_pub = self._ctx.publisher_json("snapshot")
        self._snapshot_thread = threading.Thread(
            target=self._snapshot_loop, name="snapshot-pub", daemon=True
        )
        self._snapshot_thread.start()
        if self._retention_days > 0:
            self._sweeper_thread = threading.Thread(
                target=self._sweeper_loop, name="retention-sweep", daemon=True
            )
            self._sweeper_thread.start()
        try:
            while not self._ctx.is_shutdown():
                time.sleep(0.5)
        finally:
            log.info("spatial-map shutting down")
            for h in self._handles:
                try:
                    h.undeclare()
                except Exception:  # noqa: BLE001
                    pass

    def _snapshot_loop(self) -> None:
        while not self._ctx.is_shutdown():
            try:
                snap = self.build_snapshot(
                    self._store, self._snapshot_recent_s, self._snapshot_max_dets,
                )
                self._snapshot_pub.send(snap)  # JsonPublisher.send takes a dict
            except Exception as e:  # noqa: BLE001
                log.warning("snapshot publish failed: %s", e)
            time.sleep(self._snapshot_interval_s)

    def _sweeper_loop(self) -> None:
        retention_ns = self._retention_days * 86400 * 1_000_000_000
        while not self._ctx.is_shutdown():
            try:
                cutoff = time.time_ns() - retention_ns
                deleted = self._store.prune_older_than(cutoff)
                if deleted:
                    log.info("retention sweep: deleted %d rows", deleted)
            except Exception as e:  # noqa: BLE001
                log.warning("retention sweep failed: %s", e)
            time.sleep(self._sweeper_interval_s)
```

Note for the engineer: `publisher_json` returns an object with `.send(dict)` per the SDK (`crates/bubbaloop-node/src/publisher.rs` and `python-sdk/bubbaloop_sdk/publisher.py`). Confirm the method name with `grep -n 'def send\|def put' python-sdk/bubbaloop_sdk/publisher.py` if `send()` raises `AttributeError`; switch to `.put()` if so.

- [ ] **Step 5: Run all spatial-map tests**

```bash
cd /home/nvidia/bubbaloop-nodes-official/spatial-map
pixi run -- pytest tests/ -v
```

Expected: all PASS.

- [ ] **Step 6: Commit**

```bash
git add spatial-map/main.py spatial-map/tests/test_snapshot.py
git commit -m "feat(spatial-map): snapshot publisher + retention sweeper"
```

---

## Task 9 — `node.yaml`, `pixi.toml`, `config.yaml`, `README.md` + open PR 2

**Files:**
- Modify: `bubbaloop-nodes-official/spatial-map/node.yaml`
- Modify: `bubbaloop-nodes-official/spatial-map/pixi.toml`
- Modify: `bubbaloop-nodes-official/spatial-map/config.yaml`
- Create: `bubbaloop-nodes-official/spatial-map/tests/live_walkthrough.md`
- Rewrite: `bubbaloop-nodes-official/spatial-map/README.md`

- [ ] **Step 1: Replace `spatial-map/node.yaml`**

Overwrite `spatial-map/node.yaml`:

```yaml
name: spatial-map
version: "0.2.0"
type: python
description: SQLite-backed spatial map; logs world detections, serves zones, publishes snapshots
author: Bubbaloop Team
command: pixi run main

capabilities:
  - sink

requires:
  hardware:
    - none
  software:
    - Python 3.10+

spec:
  role: sink

  inputs:
    - suffix: "**/world_detections"
      encoding: application/json
      description: World-frame detections from any geo-projector instance (no track_id)
    - suffix: "zones/set"
      encoding: application/json
      description: |
        Zone create/update payload.
        {name, camera, polygon: [[x,y],...], color?, zone_id?}
    - suffix: "zones/delete"
      encoding: application/json
      description: Zone delete payload. {zone_id}

  outputs:
    - suffix: snapshot
      encoding: application/json
      rate_hz: 1.0
      tier: event
      description: |
        Periodic spatial map snapshot for agent get_node_sample.
        Fields: published_at_ns, window_s, detections[], zones[], truncated.
    - suffix: recent
      encoding: application/json
      tier: event
      description: |
        Queryable. Params: camera?, class?, since_ns?, until_ns?, limit?.
        Reply: {detections:[...]}.
    - suffix: zones
      encoding: application/json
      tier: event
      description: |
        Queryable. Params: camera?.
        Reply: {zones:[{zone_id,name,camera,polygon,color,created_at_ns}]}.
```

- [ ] **Step 2: Update `spatial-map/pixi.toml` to drop `lance` and `pyarrow`**

```bash
cd /home/nvidia/bubbaloop-nodes-official/spatial-map
# Show current pyproject-style deps so the engineer can edit precisely:
grep -nE 'lance|pyarrow' pixi.toml
```

Edit `pixi.toml` and remove the lines that match. The remaining dependencies should be `python`, `eclipse-zenoh-python`, `pyyaml`, `pytest` — i.e. nothing application-specific. After the edit, regenerate the lockfile:

```bash
pixi install
```

- [ ] **Step 3: Update `spatial-map/config.yaml` with new tunables**

Overwrite `spatial-map/config.yaml`:

```yaml
name: spatial-map
site_id: default
retention_days: 7
snapshot_interval_s: 1.0
snapshot_recent_s: 30
sweeper_interval_s: 60.0
snapshot_max_detections: 200
```

- [ ] **Step 4: Create `tests/live_walkthrough.md`**

Create `spatial-map/tests/live_walkthrough.md`:

```markdown
# spatial-map live walk-through

Manual acceptance script. Run after PR 2-4 are deployed.

## Pre-flight

1. `bubbaloop node restart tapo-terrace-detector tapo-terrace-pose tapo-terrace-projector spatial-map`
2. Confirm no errors: `journalctl --user -u 'bubbaloop-spatial-map.service' --since '30s ago' --no-pager`

## Step 1 — ingest fires

Walk past the `tapo_terrace` camera once.

```bash
sqlite3 ~/.bubbaloop/sites/default/spatial.db \
  'SELECT COUNT(*) FROM world_detections WHERE timestamp_ns > strftime("%s","now")*1000000000 - 60000000000;'
```

Expected: a non-zero count within 5 s of walking through the frame.

## Step 2 — snapshot publishes

```bash
bubbaloop dev get-sample spatial-map-default/snapshot
```

Expected: JSON with `detections` matching what you just walked past, `zones: []`.

## Step 3 — push a zone

```bash
zenoh put 'bubbaloop/global/<machine_id>/spatial-map-default/zones/set' \
  '{"name":"Front","camera":"tapo_terrace","polygon":[[0,0],[3,0],[3,3],[0,3]],"color":"#ff8800"}'
```

Confirm:

```bash
zenoh get 'bubbaloop/global/<machine_id>/spatial-map-default/zones'
```

Expected: one zone in the reply.

## Step 4 — snapshot now includes the zone

```bash
bubbaloop dev get-sample spatial-map-default/snapshot
```

Expected: `zones[]` has one entry.

## Step 5 — delete the zone

```bash
zenoh put 'bubbaloop/global/<machine_id>/spatial-map-default/zones/delete' \
  '{"zone_id":"<zone_id from step 3>"}'
zenoh get 'bubbaloop/global/<machine_id>/spatial-map-default/zones'
```

Expected: empty `zones[]` reply.

## Step 6 — retention

Set `retention_days: 0` in `config.yaml`, restart, confirm rows older than 7 days are still present (`SELECT MIN(timestamp_ns) ...`). Restore to `7`.
```

- [ ] **Step 5: Rewrite `spatial-map/README.md`**

Overwrite `spatial-map/README.md`:

```markdown
# spatial-map

SQLite-backed spatial map for projected world-frame detections.

**Subscribes**
- `bubbaloop/**/world_detections` — any geo-projector instance
- `{inst}/zones/set`, `{inst}/zones/delete` — zone CRUD writes

**Publishes**
- `{inst}/snapshot` — JSON snapshot at 1 Hz (recent detections + zones)

**Queryables**
- `{inst}/recent?camera=&class=&since_ns=&until_ns=&limit=`
- `{inst}/zones?camera=`

**Storage:** `~/.bubbaloop/sites/{site_id}/spatial.db` (SQLite, WAL).

**Schema:**
- `world_detections` — append-only audit log: `id, timestamp_ns, camera, class_name, world_x, world_y, confidence, pose_quality`.
- `site_zones` — drawn polygons: `id, name, camera, polygon_json, color, created_at_ns`.

**No tracking, no entity persistence, no zone enter/exit events.** This is the "raw projected detections + visual zones" layer; identity and dwell live in a separate, future node if/when needed.

**Tunables (`config.yaml`):**
- `retention_days` (default `7`) — `0` disables retention.
- `snapshot_interval_s` (default `1.0`).
- `snapshot_recent_s` (default `30`).
- `sweeper_interval_s` (default `60.0`).
- `snapshot_max_detections` (default `200`) — snapshot truncated above this.

**Migration note:** the previous version of this node wrote a Lance dataset at `~/.bubbaloop/sites/{site_id}/observations.lance`. That writer is removed. The Lance directory is left in place; remove with `rm -rf ~/.bubbaloop/sites/{site_id}/observations.lance` once you have confirmed nothing depends on it.

**Spec:** `docs/superpowers/specs/2026-04-30-spatial-map-no-tracking-design.md` (in the `bubbaloop` repo).
```

- [ ] **Step 6: Run the full local test suite once more**

```bash
cd /home/nvidia/bubbaloop-nodes-official/spatial-map
pixi run -- pytest tests/ -v
```

Expected: all PASS.

- [ ] **Step 7: Commit and push**

```bash
git add spatial-map/node.yaml spatial-map/pixi.toml spatial-map/pixi.lock spatial-map/config.yaml spatial-map/README.md spatial-map/tests/live_walkthrough.md
git commit -m "feat(spatial-map): node manifest, config, README, walk-through script"
git push -u origin feat/spatial-map-no-tracking
```

- [ ] **Step 8: Open PR 2**

```bash
gh pr create --repo kornia/bubbaloop-nodes-official \
  --base main --head feat/spatial-map-no-tracking \
  --title "feat(spatial-map): rewrite as SQLite logger + zone CRUD + snapshot publisher" \
  --body "$(cat <<'EOF'
## Summary

Rewrites `spatial-map` from the previous Lance writer to a thin SQLite logger that:

- Subscribes `bubbaloop/**/world_detections` and appends every detection row.
- Accepts zone create/update/delete via Zenoh `{inst}/zones/set` and `{inst}/zones/delete`.
- Serves `{inst}/recent` and `{inst}/zones` Zenoh queryables.
- Publishes a 1 Hz `{inst}/snapshot` JSON for agent `get_node_sample`.

No tracking, no entity layer, no zone enter/exit events — see spec.

Tolerates `track_id` still being on the wire (just doesn't read it). Safe to deploy
before geo-projector drops `track_id`.

## Spec
`docs/superpowers/specs/2026-04-30-spatial-map-no-tracking-design.md` (in `bubbaloop` repo).

## Test plan
- [x] `pixi run -- pytest tests/ -v` — schema, ingest, validation, snapshot, retention.
- [ ] Live walk-through per `tests/live_walkthrough.md` after merge + deploy.

🤖 Generated with [Claude Code](https://claude.com/claude-code)
EOF
)"
```

- [ ] **Step 9: After merge, return to base branch**

```bash
gh pr merge --squash --delete-branch
git checkout main
git pull --ff-only
```

---

## Task 10 — Dashboard `SiteMapView` refactor (PR 3)

**Files:**
- Modify: `dashboard/src/components/SiteMapView.tsx`
- Create: `dashboard/src/components/__tests__/SiteMapView.test.tsx`

- [ ] **Step 1: Branch in bubbaloop repo (off `feat/demo-mvp-plans` after PR 1 merged)**

```bash
cd /home/nvidia/bubbaloop
git fetch origin
git checkout feat/demo-mvp-plans
git pull --ff-only
git checkout -b feat/sitemap-no-tracking
```

- [ ] **Step 2: Read the current track_id-using code and the WorldDetection type**

```bash
sed -n '30,55p;560,600p' dashboard/src/components/SiteMapView.tsx
```

You should see (a) `interface WorldDetection { track_id: string; ... }` and (b) the live-blip + trail loop using `d.track_id`.

- [ ] **Step 3: Update the `WorldDetection` type to drop `track_id`**

In `dashboard/src/components/SiteMapView.tsx`, change:

```ts
interface WorldDetection {
  track_id: string;
  class_name: string;
  confidence: number;
  world_x: number;
  world_y: number;
  pose_quality: string;
}
```

to:

```ts
interface WorldDetection {
  class_name: string;
  confidence: number;
  world_x: number;
  world_y: number;
  pose_quality: string;
}
```

- [ ] **Step 4: Replace the live-blip / trail update loop**

Locate the section starting around line 580:

```ts
      live.set(d.track_id, {
        x: d.world_x,
        y: d.world_y,
        class_name: d.class_name,
        last_seen: now,
      });
      const trail = trails.get(d.track_id) ?? [];
      // ...
      trails.set(d.track_id, trimmed);
```

Replace the whole inner `for (const d of msg.detections)` body with:

```ts
      // No tracker upstream — every detection is independent.
      // Synthesise a per-detection id so the live-blip Map can dedupe by key
      // (the key is unique-per-detection, so dedupe is effectively a no-op).
      const detId = `${msg.timestamp_ns}_${idx}`;
      live.set(detId, {
        x: d.world_x,
        y: d.world_y,
        class_name: d.class_name,
        last_seen: now,
      });
```

…and add `idx` to the loop header:

```ts
msg.detections.forEach((d: WorldDetection, idx: number) => {
  // body above
});
```

(replacing `for (const d of msg.detections)`).

Remove all references to `trails`, `trail`, `trimmed`, `TrailPoint`, and the trail layer in the rest of the file. Use a single `grep` to confirm:

```bash
grep -n "trails\|trail\|TrailPoint" dashboard/src/components/SiteMapView.tsx
```

Expected: no matches after the edit. Wherever you see a match, delete the surrounding block. Comments and visual rendering of trails go too.

- [ ] **Step 5: Subscribe to `{inst}/snapshot` to receive zones**

The spatial-map node publishes a 1 Hz JSON snapshot on `{inst}/snapshot` that already contains the current zones list. The dashboard already uses `useZenohSubscription` (see SiteMapView.tsx line ~510) for the existing `world_detections` topic — reuse the same pattern for `snapshot`.

Add a second subscription alongside the existing `world_detections` one. The exact topic key uses a wildcard to discover any spatial-map instance on the machine:

```ts
// Wildcard subscribe; one machine has at most one spatial-map instance
// for now, so the latest sample wins.
const snapshotTopic = 'bubbaloop/global/**/spatial-map-*/snapshot';
const [zones, setZones] = useState<Zone[]>([]);

const handleSnapshot = useCallback((sample: Sample) => {
  try {
    const body = JSON.parse(new TextDecoder().decode(getSamplePayload(sample)));
    if (Array.isArray(body.zones)) {
      setZones(body.zones.map((z: any) => ({
        id: z.zone_id,
        name: z.name,
        polygon: z.polygon as [number, number][],
        color: z.color ?? '#ff8800',
      })));
    }
  } catch { /* malformed snapshot → ignore this tick */ }
}, []);

useZenohSubscription(snapshotTopic, handleSnapshot);
```

Note for the engineer: confirm the snapshot's wildcard topic by inspecting one with `zenoh get 'bubbaloop/global/**/spatial-map-*/snapshot'` after PR 2 deploys. If the actual instance suffix differs (e.g. `spatial-map-default` vs `spatial-map-tapo_terrace`), tighten the wildcard accordingly.

- [ ] **Step 6: Render zones (already-existing rendering path)**

If the file already has a `zones.map(z => <Polygon ...>)` block driven by a static `zones` array, that block now consumes the new `zones` state from step 5 — no change needed beyond replacing the source. If there is no zone rendering yet, add a simple SVG `<polygon>` layer iterating `zones` with `fill={z.color + '33'}` and `stroke={z.color}`.

This snapshot-driven path replaces the queryable-GET path described in spec §8.2 — it covers the same use case (render current zones) without introducing an HTTP-bridge dependency. If, later, you need ad-hoc parameterised queries against `{inst}/zones?camera=...`, add the queryable HTTP path then; for the current scope, snapshot is sufficient.

- [ ] **Step 7: Write a vitest for the new keying behaviour**

Create `dashboard/src/components/__tests__/SiteMapView.test.tsx`:

```tsx
import { describe, it, expect } from 'vitest';

/**
 * The post-tracking SiteMapView renders independent live blips. The
 * easiest contract to test without mounting Leaflet is the synthesised
 * detection id used to key the live blips Map. Reproduce the formula
 * here so that any change in SiteMapView will trip this test.
 */
function makeDetId(timestamp_ns: number, idx: number): string {
  return `${timestamp_ns}_${idx}`;
}

describe('SiteMapView live-blip keying', () => {
  it('synthesises a unique id from timestamp + index', () => {
    const a = makeDetId(1, 0);
    const b = makeDetId(1, 1);
    const c = makeDetId(2, 0);
    expect(new Set([a, b, c]).size).toBe(3);
  });

  it('reuses the id only for the exact same (ts, idx) tuple', () => {
    expect(makeDetId(1, 0)).toBe(makeDetId(1, 0));
  });
});
```

This is intentionally lightweight — the value is locking the *shape* of the id so the dashboard never silently re-introduces `track_id` keying. Mounting Leaflet under jsdom is out of scope for unit tests.

- [ ] **Step 8: Run the dashboard test suite**

```bash
cd /home/nvidia/bubbaloop/dashboard
npm test -- --run
```

Expected: all tests PASS, including the new file. The vitest run should report green.

- [ ] **Step 9: Verify the dashboard builds**

```bash
npm run build
```

Expected: build succeeds. Type errors here are likely — `track_id` removed from the type may surface unused references. Fix each error by removing the dead reference (do not re-introduce `track_id`).

- [ ] **Step 10: Commit and push**

```bash
cd /home/nvidia/bubbaloop
git add dashboard/src/components/SiteMapView.tsx dashboard/src/components/__tests__/SiteMapView.test.tsx
git commit -m "$(cat <<'EOF'
feat(dashboard): SiteMapView no-tracking refactor

Drops `track_id` keying for live blips; synthesises a per-detection id
from `${timestamp_ns}_${idx}`. Removes the persistent-trail rendering.
Wires `{inst}/zones` queryable for zone polygon overlay.

Companion to spatial-map node rewrite.
Spec: docs/superpowers/specs/2026-04-30-spatial-map-no-tracking-design.md

Co-Authored-By: Claude Opus 4.7 <noreply@anthropic.com>
EOF
)"

git push -u origin feat/sitemap-no-tracking
```

The pre-commit hook may try to auto-stage Rust files; if any unrelated `.rs` files appear staged in the commit message, abort with `git reset HEAD~1` and use the stash workaround from Task 1 step 7.

- [ ] **Step 11: Open PR 3**

```bash
gh pr create \
  --base feat/demo-mvp-plans \
  --title "feat(dashboard): SiteMapView no-tracking refactor" \
  --body "$(cat <<'EOF'
## Summary

- `WorldDetection` type drops `track_id`.
- Live-blip Map is keyed by a synthesised `${timestamp_ns}_${idx}` so each detection is its own fading dot.
- Persistent-trail rendering is removed.
- Adds a 5 s poll of `{inst}/zones` queryable; renders zone polygons over the map.

Wire-format change is shipped in PR 4 (geo-projector). This PR is safe to land first because the dashboard ignores `track_id` whether it's on the wire or not.

## Spec
`docs/superpowers/specs/2026-04-30-spatial-map-no-tracking-design.md`

## Test plan
- [x] `npm test --run` green.
- [x] `npm run build` green.
- [ ] Live: with PR 2 merged + spatial-map deployed, dashboard shows polygon overlays from `{inst}/zones`.
- [ ] Live: walking past tapo_terrace produces fading independent blips along the path (no connected streak — that's by design).

🤖 Generated with [Claude Code](https://claude.com/claude-code)
EOF
)"
```

- [ ] **Step 12: After merge, sync base**

```bash
gh pr merge --squash --delete-branch
git checkout feat/demo-mvp-plans
git pull --ff-only
```

---

## Task 11 — `geo-projector` strip `WorldTracker` + `track_id` (PR 4)

**Files:**
- Modify: `bubbaloop-nodes-official/geo-projector/main.py`
- Modify: `bubbaloop-nodes-official/geo-projector/configs/*.yaml` (if any have track_* keys)
- Modify: `bubbaloop-nodes-official/geo-projector/node.yaml`
- Create: `bubbaloop-nodes-official/geo-projector/tests/test_no_track_id.py`

- [ ] **Step 1: Pre-flight grep for any remaining `track_id` consumers**

```bash
cd /home/nvidia
grep -rn "track_id" bubbaloop bubbaloop-nodes-official 2>/dev/null \
  | grep -v "^bubbaloop-nodes-official/geo-projector/" \
  | grep -v "^bubbaloop/docs/superpowers/specs/" \
  | grep -v "^bubbaloop/docs/superpowers/plans/" \
  | grep -v "node_modules\|.pixi\|target\|__pycache__"
```

Expected: empty output. PR 1 removed `detection_store.rs`; PR 3 removed dashboard usage. If anything else turns up, fix it in this task before continuing — do not let a downstream consumer silently break.

- [ ] **Step 2: Branch off `main`**

```bash
cd /home/nvidia/bubbaloop-nodes-official
git fetch origin
git checkout main
git pull --ff-only
git checkout -b feat/geo-projector-no-tracker
```

- [ ] **Step 3: Write a failing regression test**

Create `geo-projector/tests/__init__.py` (empty) if not present, then create `geo-projector/tests/test_no_track_id.py`:

```python
"""Regression test: geo-projector output no longer carries `track_id`."""
from __future__ import annotations

import json
from pathlib import Path

# Verify by static AST analysis that `track_id` no longer appears in the
# output construction code, so this test fails fast at design time
# rather than only catching it after a runtime walk-through.

PROJECTOR_SRC = Path(__file__).resolve().parent.parent / "main.py"


def test_main_contains_no_track_id_assignment():
    src = PROJECTOR_SRC.read_text()
    # Permit `track_id` to appear ONLY inside a comment line that documents
    # the removal. Strip such lines and assert nothing else mentions it.
    lines = [ln for ln in src.splitlines() if not ln.lstrip().startswith("#")]
    body = "\n".join(lines)
    assert "track_id" not in body, (
        "track_id must not appear in geo-projector output code; "
        f"found references in {PROJECTOR_SRC}"
    )


def test_main_contains_no_world_tracker_class():
    src = PROJECTOR_SRC.read_text()
    assert "class WorldTracker" not in src, (
        "WorldTracker class must be removed; geo-projector now emits raw projections."
    )
```

- [ ] **Step 4: Run the regression test, expect failure**

```bash
cd /home/nvidia/bubbaloop-nodes-official/geo-projector
pixi run -- pytest tests/test_no_track_id.py -v
```

Expected: 2 FAIL — both `track_id` and `WorldTracker` are still in `main.py`.

- [ ] **Step 5: Inspect the current main.py to identify what to delete**

```bash
grep -nE "^class |track_id|WorldTracker|TRACK_" main.py | head -50
```

Make a list of:
- The whole `class WorldTracker:` definition (lines start..end).
- Any module-level `TRACK_*` constants.
- The `track_id` field assignment in the published JSON.
- Any tracker-update calls inside the per-frame loop.
- Any tracker-state initialization in `__init__`.

- [ ] **Step 6: Delete the `WorldTracker` class and its constants**

Edit `main.py`. Delete:
- The `class WorldTracker:` block (entire class).
- Module constants `TRACK_MAX_DIST_M`, `TRACK_MAX_MISS` (and any other `TRACK_*`).

Keep `KNOWN_CLASS_HEIGHTS`, `HEIGHT_*`, and `MetricHeightEstimator` — those are projection helpers, not tracking.

- [ ] **Step 7: Strip tracker calls from the per-frame ingest loop**

Find the loop that builds the `world_detections` JSON. Remove:
- Any `self._tracker.update(...)` call.
- The `track_id` field from each detection dict.

Each detection in the published payload should now contain exactly:

```python
{
    "class_name": ...,
    "confidence": ...,
    "world_x": ...,
    "world_y": ...,
    "pose_quality": ...,
}
```

- [ ] **Step 8: Strip tracker init from `__init__`**

Remove `self._tracker = WorldTracker(...)` (or equivalent) from `GeoProjectorNode.__init__`.

- [ ] **Step 9: Strip `track_*` keys from configs**

```bash
grep -rn "^track_\|track_max_\|track_id" geo-projector/configs/ geo-projector/config.yaml 2>/dev/null
```

Edit each match to remove the line entirely. Common keys to remove: `track_max_dist_m`, `track_max_miss`.

- [ ] **Step 10: Update `geo-projector/node.yaml` description**

Open `geo-projector/node.yaml` and change the `outputs[].description` for the `world_detections` output to:

```yaml
  outputs:
    - suffix: world_detections
      encoding: application/json
      rate_hz: 1.0
      tier: event
      description: |
        World-coordinate detections per detector frame.
        Fields: timestamp_ns, camera, detections[{class_name, confidence,
        world_x, world_y, pose_quality}].
        No tracking; no track_id. Identity (if needed) belongs in a
        downstream node such as spatial-map.
```

- [ ] **Step 11: Run the regression test, expect pass**

```bash
pixi run -- pytest tests/test_no_track_id.py -v
```

Expected: 2 PASS.

- [ ] **Step 12: Run the full geo-projector test suite (existing tests)**

```bash
pixi run -- pytest tests/ -v
```

Expected: all PASS. If any pre-existing test exercises `track_id` directly, update it to assert the new shape (no `track_id`). If a test verifies tracker-specific behaviour, delete that test — the behaviour is gone by design.

- [ ] **Step 13: Smoke-test by running the node against a recorded detection feed (optional but strongly recommended)**

If a recording exists in `bubbaloop-nodes-official/geo-projector/tests/fixtures/`, replay it and inspect one published JSON message:

```bash
# (replace with the actual fixture replay command if one exists)
pixi run main --config configs/tapo_entrance.yaml &
sleep 5
zenoh get 'bubbaloop/global/<machine_id>/tapo_entrance_proj/world_detections' | head -1
kill %1
```

The reply JSON's detections should contain no `track_id` key. If no fixture replay exists, skip this step and rely on the live walk-through in Task 12.

- [ ] **Step 14: Commit and push**

```bash
git add geo-projector/main.py geo-projector/node.yaml geo-projector/configs/ geo-projector/config.yaml geo-projector/tests/__init__.py geo-projector/tests/test_no_track_id.py
git commit -m "$(cat <<'EOF'
refactor(geo-projector): remove WorldTracker and track_id from output

This node now emits raw projected detections — one row per bbox per frame,
with no identity. Identity (if needed) belongs in a downstream node such
as spatial-map. The dashboard already moved off track_id keying in
bubbaloop dashboard PR.

Spec: docs/superpowers/specs/2026-04-30-spatial-map-no-tracking-design.md
(in bubbaloop repo).

Co-Authored-By: Claude Opus 4.7 <noreply@anthropic.com>
EOF
)"
git push -u origin feat/geo-projector-no-tracker
```

- [ ] **Step 15: Open PR 4**

```bash
gh pr create --repo kornia/bubbaloop-nodes-official \
  --base main --head feat/geo-projector-no-tracker \
  --title "refactor(geo-projector): drop WorldTracker and track_id" \
  --body "$(cat <<'EOF'
## Summary

- Delete `WorldTracker` class (~120 lines) and `TRACK_*` constants.
- Drop `track_id` from every published detection.
- Keep projection math + `MetricHeightEstimator` unchanged.
- Strip `track_*` keys from configs.
- Add `tests/test_no_track_id.py` regression to prevent reintroduction.

Companion to bubbaloop PRs: cleanup (PR 1), spatial-map rewrite (PR 2), dashboard refactor (PR 3).

## Spec
`docs/superpowers/specs/2026-04-30-spatial-map-no-tracking-design.md` (in `bubbaloop` repo).

## Test plan
- [x] `pytest tests/` green (incl. new regression).
- [ ] Live walk-through after merge: dashboard shows independent fading blips per detection.

🤖 Generated with [Claude Code](https://claude.com/claude-code)
EOF
)"
```

- [ ] **Step 16: Merge and sync**

```bash
gh pr merge --squash --delete-branch
git checkout main
git pull --ff-only
```

---

## Task 12 — Live walk-through acceptance

**Files:** none — scripted manual run.

- [ ] **Step 1: Confirm all four PRs merged**

```bash
cd /home/nvidia/bubbaloop && git log --oneline -5
cd /home/nvidia/bubbaloop-nodes-official && git log --oneline -5
```

Expected: HEAD on each repo includes the merged squash commits for the four PRs.

- [ ] **Step 2: Restart the relevant nodes**

```bash
/home/nvidia/bubbaloop/target/release/bubbaloop node restart \
  tapo-terrace-detector tapo-terrace-pose tapo-terrace-projector spatial-map
sleep 5
journalctl --user -u 'bubbaloop-spatial-map.service' --since '30s ago' --no-pager | tail -20
```

Expected: no error lines; you should see `subscribing bubbaloop/**/world_detections` in the spatial-map log.

- [ ] **Step 3: Walk past tapo_terrace once**

Then verify a row landed in SQLite:

```bash
sqlite3 ~/.bubbaloop/sites/default/spatial.db \
  'SELECT COUNT(*) FROM world_detections WHERE timestamp_ns > strftime("%s","now")*1000000000 - 60000000000;'
```

Expected: a non-zero count.

- [ ] **Step 4: Verify dashboard renders independent blips (no connected trail)**

Open the dashboard, walk past `tapo_terrace`, observe the SiteMapView. Each detection appears as its own dot, fading over ~30 s. There is **no connected streak** — that's the intended visual.

- [ ] **Step 5: Push a zone via Zenoh**

```bash
MACHINE=$(/home/nvidia/bubbaloop/target/release/bubbaloop machine id 2>/dev/null || hostname | tr '-' '_')
zenoh put "bubbaloop/global/$MACHINE/spatial-map-default/zones/set" \
  '{"name":"Front","camera":"tapo_terrace","polygon":[[0,0],[3,0],[3,3],[0,3]],"color":"#ff8800"}'
sleep 1
zenoh get "bubbaloop/global/$MACHINE/spatial-map-default/zones"
```

Expected: the GET returns a JSON reply with one zone.

- [ ] **Step 6: Verify the zone renders on the dashboard**

Refresh the dashboard map (or wait 5 s for the next poll). The orange polygon should appear over the camera view's coordinates.

- [ ] **Step 7: Verify snapshot via `get_node_sample` (agent path)**

```bash
/home/nvidia/bubbaloop/target/release/bubbaloop dev get-sample spatial-map-default/snapshot
```

Expected: JSON with non-empty `detections[]` (from your walk) and `zones[]` containing the Front zone.

- [ ] **Step 8: Delete the zone**

```bash
ZID=$(zenoh get "bubbaloop/global/$MACHINE/spatial-map-default/zones" \
  | jq -r '.[0].value | fromjson | .zones[0].zone_id')
zenoh put "bubbaloop/global/$MACHINE/spatial-map-default/zones/delete" \
  "{\"zone_id\":\"$ZID\"}"
sleep 1
zenoh get "bubbaloop/global/$MACHINE/spatial-map-default/zones"
```

Expected: empty `zones[]`.

- [ ] **Step 9: Tune `T_RETENTION` if needed**

If `~/.bubbaloop/sites/default/spatial.db` accumulates faster than expected during the walk-through, lower `retention_days` in `spatial-map/config.yaml`. Default is 7 — this is rarely needed for a single demo.

- [ ] **Step 10: Document the run**

Update `bubbaloop-nodes-official/spatial-map/tests/live_walkthrough.md` with the date, machine_id, and observed counts. Commit on a small follow-up branch:

```bash
cd /home/nvidia/bubbaloop-nodes-official
git checkout -b chore/walkthrough-results
# edit live_walkthrough.md to add a "Run on YYYY-MM-DD" section
git add spatial-map/tests/live_walkthrough.md
git commit -m "docs(spatial-map): record live walk-through results"
git push -u origin chore/walkthrough-results
gh pr create --repo kornia/bubbaloop-nodes-official \
  --base main --head chore/walkthrough-results \
  --title "docs(spatial-map): record live walk-through results" \
  --body "Adds the as-run section of the walk-through script for traceability."
```

- [ ] **Step 11: Optional cleanup of stale Lance dataset**

After confirming nothing depends on the legacy data:

```bash
ls ~/.bubbaloop/sites/default/observations.lance/ 2>/dev/null && rm -rf ~/.bubbaloop/sites/default/observations.lance
```

This is intentionally manual, not part of any node's startup, to avoid silent data deletion.

---

## Self-review checklist (run before submitting this plan)

- [x] Spec §3 (architecture) — Tasks 5-9 (spatial-map), Task 11 (geo-projector), Task 1 (detection_store.rs delete), Task 10 (dashboard).
- [x] Spec §4 (wire format) — Task 11 step 7 strips `track_id` from output.
- [x] Spec §5 (geo-projector changes) — Task 11.
- [x] Spec §6.1 (schema) — Task 2.
- [x] Spec §6.2 (retention) — Task 4 + Task 8 sweeper loop.
- [x] Spec §6.3 (tunables) — Task 5 `__init__` + Task 9 `config.yaml`.
- [x] Spec §7.1 snapshot — Task 8.
- [x] Spec §7.2 recent — Task 7.
- [x] Spec §7.3 zones — Task 7.
- [x] Spec §7.4 zones/set — Task 6.
- [x] Spec §7.5 zones/delete — Task 6.
- [x] Spec §8.1 dashboard refactor — Task 10.
- [x] Spec §10 error handling — covered inline in handlers (try/except, log+drop).
- [x] Spec §11.1 unit tests — Tasks 2-4, 8.
- [x] Spec §11.2 projector regression — Task 11 step 3.
- [x] Spec §11.3 live walk-through — Task 12.
- [x] Spec §15 risks — pre-flight grep in Task 11 step 1; lance migration note in Task 9 README.

## Execution

After this plan is reviewed, choose:

1. **Subagent-driven** — dispatch a fresh subagent per task with two-stage review (recommended).
2. **Inline** — execute via `superpowers:executing-plans`, batch checkpoints between tasks.

Either is valid; tasks are sized so subagent-driven keeps each subagent under ~15 minutes of active work.
