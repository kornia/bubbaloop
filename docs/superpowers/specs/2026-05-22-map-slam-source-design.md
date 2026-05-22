# Map: SLAM-backed top-down view with `SlamMapSource` interface

**Date:** 2026-05-22
**Status:** Draft (awaiting user review)
**Repo target:** bubbaloop-dash
**Branch:** `feat/library-map-redesign` (this work will be a small commit set on top of the existing branch)
**Companion docs:**
- `docs/superpowers/specs/2026-05-21-bubbaloop-dash-phase-1.4-map.md` — the original Map tab spec that this *replaces in part* (floorplan-upload model is dropped; sensor-placement remains)
- `docs/superpowers/research/2026-05-21-library-map-redesign-synthesis.md` — earlier research that explicitly deferred the SLAM era

## Goal

Replace the Map tab's user-uploaded floorplan with a SLAM-derived occupancy grid that comes from the upstream SLAM system's DB service. Dash is a *thin reader* — it never owns the map. Sensor placement (the manual-localization workflow) stays, now in world meters.

## Architecture

Two-mode Map tab. Background is either the SLAM occupancy grid or a meters reference grid. Sensors render on top, always in world meters.

```
            ┌─────────────────────────────────────────────┐
            │  Map tab (frontend)                         │
            │   GET /api/map/slam/meta                    │
            │     ├─ 200 → fetch /api/map/slam → render   │
            │     │         Konva Image as background     │
            │     │         (pixels × resolution = meters)│
            │     └─ 404 → render meters grid             │
            │   Sensors rendered on top, always in meters │
            └─────────────────────────────────────────────┘
                                ▲
                                │ HTTP
                  ┌─────────────┴─────────────────┐
                  │  Dash backend                 │
                  │  /api/map/slam{,/meta}        │
                  │                               │
                  │  SlamMapSource (interface)    │
                  │   ├ LocalFileSource (DEV)     │
                  │   │   reads files from disk   │
                  │   │   to test rendering       │
                  │   └ RemoteSlamDbSource (PROD) │
                  │      HTTP proxy to upstream   │
                  │      (stubbed until SLAM     │
                  │      team publishes contract) │
                  └────────────────────────────────┘
                                ▲
                                │ (config: SLAM_MAP_SOURCE=local|remote)
                  ┌─────────────┴─────────────────┐
                  │  Upstream SLAM DB service     │
                  │  (separate ecosystem)         │
                  └───────────────────────────────┘
```

**Key invariants:**
- Dash is **read-only** against the source. No POST, no DELETE, no PUT.
- Sensor placements live in dash's SQLite, coordinates in **world meters**.
- The same world frame applies to the SLAM map and the sensors. When the SLAM map exists, the YAML `origin` anchors the world frame; when it doesn't, the frame is arbitrary (user-relative).
- Sensors placed before SLAM exists will likely need to be repositioned once a SLAM frame is established. **Accepted caveat.** Documented in the user-facing notes.

## Tech stack

- Backend Python 3.10+, FastAPI + Pydantic + SQLite (existing)
- Frontend React 18 + TS + Vite + Tailwind-less scoped CSS (existing token system), `react-konva` (already installed)
- **New runtime dependency:** `pyyaml >=6.0,<7.0` — the ROS map sidecar is YAML by convention; `LocalFileSource` parses it directly. Cheap dep, well-maintained.
- `RemoteSlamDbSource` may later pull in `httpx` (already in dev deps; promote to runtime when implemented).

## Scope

### In scope
- New backend API: `GET /api/map/slam`, `GET /api/map/slam/meta`
- New backend abstraction: `SlamMapSource` interface + `LocalFileSource` impl + `RemoteSlamDbSource` stub
- Removal of all floorplan-upload code (backend route, frontend component, tests)
- Coordinate-system migration of `sensor_placements`: pixels → meters
- Frontend: `MapTab` renders SLAM background when available, meters-grid otherwise
- Frontend: new `SlamLayer` (Konva Image) + `MetersGrid` components
- Frontend: `useSlamMap()` hook with 10-second polling

### Out of scope (explicit deferrals)
1. **Sensor positions feeding back to SLAM** — landmark priors / fiducial anchors. Revisit when the SLAM team needs that contract.
2. **Multi-map / multi-floor support.** v1 is one map per machine. Future schema can add `map_id`.
3. **Live streaming via Zenoh.** v1 polls `/api/map/slam/meta` every 10 s. Swap to WebSocket only when measured need exists.
4. **`RemoteSlamDbSource` actual implementation.** Stub until the SLAM team publishes their wire format.
5. **"Calibrate to SLAM frame" tool** for existing sensor positions. Accepted re-alignment caveat; user re-places manually.
6. **Floorplan upload.** Removed entirely from this branch.
7. **Authentication scheme for the SLAM DB service.** Will be config-only (env var token) when known.

## Backend

### Data shapes

```python
# backend/src/dash/slam/types.py (NEW)
from pydantic import BaseModel

class SlamMapMeta(BaseModel):
    width: int                # PNG width in pixels
    height: int               # PNG height in pixels
    resolution: float         # meters per pixel (ROS YAML convention)
    origin_x: float           # world meters of the bottom-left pixel
    origin_y: float           # world meters of the bottom-left pixel
    updated_at: int           # ms epoch of last refresh
    source: str               # provenance: "local-file" | "slam-db"
```

### `SlamMapSource` interface

```python
# backend/src/dash/slam/source.py (NEW)
from typing import Protocol
from .types import SlamMapMeta

class SlamMapSource(Protocol):
    async def get_meta(self) -> SlamMapMeta | None: ...
    async def get_image(self) -> bytes | None: ...
```

### `LocalFileSource` implementation

```python
class LocalFileSource:
    def __init__(self, base_path: Path) -> None:
        self._png = base_path / "occupancy.png"
        self._yaml = base_path / "occupancy.yaml"

    async def get_meta(self) -> SlamMapMeta | None:
        if not self._png.is_file() or not self._yaml.is_file():
            return None
        # parse minimal ROS map YAML
        import yaml
        with self._yaml.open() as f:
            y = yaml.safe_load(f) or {}
        # Probe PNG dimensions with Pillow
        from PIL import Image
        with Image.open(self._png) as img:
            width, height = img.size
        return SlamMapMeta(
            width=width,
            height=height,
            resolution=float(y.get("resolution", 0.05)),
            origin_x=float(y["origin"][0]) if "origin" in y else 0.0,
            origin_y=float(y["origin"][1]) if "origin" in y else 0.0,
            updated_at=int(self._png.stat().st_mtime * 1000),
            source="local-file",
        )

    async def get_image(self) -> bytes | None:
        return self._png.read_bytes() if self._png.is_file() else None
```

Source path resolution: `settings.slam_map_local_path` (default `${DATA_DIR}/maps/slam/dev`). Settable via `SLAM_MAP_LOCAL_PATH` env var.

### `RemoteSlamDbSource` (stub)

```python
class RemoteSlamDbSource:
    def __init__(self, endpoint: str, token: str | None) -> None:
        self._endpoint = endpoint
        self._token = token

    async def get_meta(self) -> SlamMapMeta | None:
        # TODO: implemented when SLAM team publishes their service contract.
        # Anticipated shape: GET {endpoint}/maps/current/meta → SlamMapMeta JSON
        return None

    async def get_image(self) -> bytes | None:
        # TODO: GET {endpoint}/maps/current/image → image/png bytes
        return None
```

### Source selection

```python
# backend/src/dash/factory.py — add in startup
source_kind = settings.slam_map_source  # "local" (default) | "remote"
if source_kind == "remote":
    app.state.slam_map_source = RemoteSlamDbSource(
        endpoint=settings.slam_map_db_url,
        token=settings.slam_map_db_token,
    )
else:
    app.state.slam_map_source = LocalFileSource(Path(settings.slam_map_local_path))
```

### Settings additions

```python
# backend/src/dash/config.py
slam_map_source: str = Field(default="local")  # "local" | "remote"
slam_map_local_path: Path = Field(default_factory=lambda: Path("/var/lib/bubbaloop-dash/maps/slam/dev"))
slam_map_db_url: str | None = Field(default=None)
slam_map_db_token: str | None = Field(default=None)
```

### API endpoints

| Endpoint | Behavior |
|---|---|
| `GET /api/map/slam/meta` | 200 + `SlamMapMeta` JSON when source returns meta; **404** with `{"reason": "no_slam_map"}` when source returns `None` |
| `GET /api/map/slam` | 200 + `image/png` body with `Cache-Control: no-cache, must-revalidate` when source has an image; **404** otherwise. ETag based on `updated_at` for cheap polling. |

Both endpoints are gated behind `require_bearer_token` to match sibling map routes.

### `sensor_placements` coord migration

Current columns (`x, y, z REAL`, `range_pixels REAL`). Migration **reinterprets** `x, y, z` as meters instead of pixels, and introduces a new `range REAL` column to replace `range_pixels`:

1. Add column `range REAL` to `sensor_placements` (NULL during transition).
2. Backfill: `UPDATE sensor_placements SET x = x / 20.0, y = y / 20.0, z = z / 20.0, range = range_pixels / 20.0`. (20 px/m is the existing `METER_TO_PIXELS` hack. Numerically a sensor at pixel `(300, 400)` becomes `(15, 20)` meters, which matches the previous 20 px/m render exactly.)
3. Drop `range_pixels` column in a follow-up migration after dust settles. For v1, both `range` and `range_pixels` exist on the row; the API serves only `range`.
4. **Unit convention** (documented at the schema definition + in the API response): `x, y, z, range` are **world meters**. Short names because every consumer is in the same coord space and unit suffixes get noisy fast. The schema comment + Pydantic model field descriptions make units explicit.

```python
# backend/src/dash/db/sqlite.py — schema definition gains a comment line
"""sensor_placements: positions and ranges are in world meters. The world
frame is anchored by the SLAM map's YAML origin when SLAM is present;
otherwise it's user-relative."""
```

```python
# backend/src/dash/api/map.py — Pydantic field descriptions
class PlacementOut(BaseModel):
    x: float = Field(description="world meters")
    y: float = Field(description="world meters")
    z: float = Field(description="world meters")
    yaw: float = Field(description="degrees, 0 = +x (east)")
    range: float = Field(description="world meters")
    # ...
```

Migration applied idempotently on `Db.open()` via `_ensure_*_columns` style helpers (matches the anchor-columns pattern from P1.5).

### Removed backend code

Delete entirely:
- `POST /api/map/floorplan` route
- `GET /api/map/floorplan` route
- `_floorplan_candidates`, `_find_floorplan` helpers
- `_ALLOWED_FORMATS` whitelist
- `FloorplanInfo` Pydantic model
- All `test_map_floorplan.py` tests (replace with the new `test_map_slam.py`)

## Frontend

### File deltas

**Delete:**
- `frontend/src/components/FloorplanUploader.tsx`
- `frontend/src/components/__tests__/FloorplanUploader.test.tsx`

**Modify:**
- `frontend/src/components/MapTab.tsx` — remove floorplan branch + HEAD probe; replace with `useSlamMap()` hook; always render `<MapCanvas>` (no more empty-state component switch)
- `frontend/src/components/MapCanvas.tsx` — accept `background: SlamBackground | MetersGridBackground` prop; render the right Konva layer underneath the existing placements layer. Apply a single scale transform on the Stage so children draw in meter units.
- `frontend/src/components/SensorMarker.tsx`, `FOVCone.tsx`, `RotationHandle.tsx` — accept `range` instead of `range_pixels`. Drop any pixel math; positions and ranges are in world meters per the schema convention.
- `frontend/src/components/NodeSidebar.tsx` — no shape change; drag payload still carries `rangeMeters`.
- `frontend/src/components/__tests__/MapTab.test.tsx` — drop floorplan-upload tests; add background-mode tests.
- `frontend/src/components/__tests__/MapCanvas.test.tsx` — drop floorplan-related test scaffolding; add tests for the two background modes.

**New:**
- `frontend/src/components/SlamLayer.tsx` — `<KonvaImage>` of the occupancy PNG. Sized via `width × resolution` (meters), positioned at `(origin_x, origin_y)`. Pixel-art rendering crisp at integer scales (`imageSmoothingEnabled={false}` if Konva exposes it).
- `frontend/src/components/MetersGrid.tsx` — Konva `<Line>` grid with 1 m spacing and a 5 m major-line accent. Reasonable viewport defaults; doesn't try to be infinite (just covers visible region + overscan).
- `frontend/src/hooks/useSlamMap.ts` — fetches `/api/map/slam/meta`; returns `{ meta, imageUrl } | null`. Polls every 10 s. Image URL is the literal `/api/map/slam` plus a cache-buster from `updated_at`.

### `useSlamMap` hook

```typescript
// frontend/src/hooks/useSlamMap.ts
export type SlamMapMeta = {
  width: number;
  height: number;
  resolution: number;
  origin_x: number;
  origin_y: number;
  updated_at: number;
  source: string;
};

export function useSlamMap(pollMs = 10_000): { meta: SlamMapMeta; imageUrl: string } | null {
  // useState + useEffect with setInterval; returns null when /api/map/slam/meta is 404.
  // imageUrl = `/api/map/slam?ts=${meta.updated_at}` so the browser cache invalidates on changes.
}
```

### Coordinate system at the boundary

The Konva `Stage` applies a single scale transform `scaleX = scaleY = pxPerMeter`. All children draw in **meter units** — no per-component scale math.

- With SLAM background: `pxPerMeter = 1 / meta.resolution`. For a typical 0.05 m/px occupancy grid, `pxPerMeter = 20`. The SlamLayer image is rendered with its native pixel dimensions; the Stage scale converts to meters.
- Without SLAM background: `pxPerMeter = 50` (configurable default). A 1 m grid cell is 50 px on screen.

Sensors, FOV cones, rotation handles use `range`, `x`, `y` directly (values in meters per the schema convention). No conversions.

### Tests

**Backend** — `backend/tests/test_map_slam.py` (NEW):
- `LocalFileSource` returns meta + bytes when files present
- `LocalFileSource` returns `None` when files missing
- `LocalFileSource` malformed YAML → propagate the error or return None (pick one; pick "return None" so a bad YAML doesn't 500 the endpoint)
- `API GET /api/map/slam/meta` → 200 + correct shape with `LocalFileSource`
- `API GET /api/map/slam/meta` → 404 when source has no map
- `API GET /api/map/slam` → 200 + correct image/png headers + ETag
- `API GET /api/map/slam` → 404 when no map
- `RemoteSlamDbSource` always returns None (stub) — verifies the wire shape and the env-var swap mechanism

**Backend** — delete:
- All of `backend/tests/test_map_floorplan.py`

**Backend** — modify:
- `backend/tests/test_sensor_placements.py` — add migration test (old `range_pixels` → new `range`; `x,y,z` reinterpreted as meters); update DTO field tests
- `backend/tests/test_map_placements.py` — update DTO field names

**Frontend** — `frontend/src/components/__tests__/MapTab.test.tsx` (UPDATE):
- Renders `MetersGrid` background when `/api/map/slam/meta` returns 404
- Renders `SlamLayer` background when meta returns 200
- SLAM map image src is `/api/map/slam?ts=<updated_at>`
- Sensor drag fires PATCH with `{x, y}` in meters (e.g., world (5.5, 3) → `{"x": 5.5, "y": 3}`)

**Frontend** — `frontend/src/hooks/__tests__/useSlamMap.test.ts` (NEW):
- Returns null when fetch 404s
- Returns `{meta, imageUrl}` when fetch 200s
- Polls every 10 s — second fetch fires after fake timer advance

## Acceptance criteria

- [ ] `FloorplanUploader` component fully removed from the codebase
- [ ] All `floorplan` backend routes return 404 (or are deleted)
- [ ] `GET /api/map/slam/meta` returns 200 when a sample occupancy PNG + YAML exists at `${SLAM_MAP_LOCAL_PATH}`
- [ ] Map tab renders the SLAM background when the meta endpoint returns 200
- [ ] Map tab renders a meters grid when the meta endpoint returns 404
- [ ] Sensor markers placed on the map are stored with meter-unit coordinates
- [ ] Switching `SLAM_MAP_SOURCE=remote` at runtime swaps to `RemoteSlamDbSource` (which is stubbed → tab falls back to meters grid until the SLAM service is implemented)
- [ ] Existing sensor placements migrate cleanly (pixel-x → meter-x via `/ 20.0`)
- [ ] Backend test suite green (≥ 8 new tests in `test_map_slam.py`, all `test_map_floorplan.py` tests gone)
- [ ] Frontend test suite green (≥ 4 new tests in MapTab + useSlamMap)
- [ ] tsc + ruff + pytest all clean

## Files

### New
- `backend/src/dash/slam/__init__.py`
- `backend/src/dash/slam/types.py`
- `backend/src/dash/slam/source.py`
- `backend/tests/test_map_slam.py`
- `frontend/src/components/SlamLayer.tsx`
- `frontend/src/components/MetersGrid.tsx`
- `frontend/src/hooks/useSlamMap.ts`
- `frontend/src/hooks/__tests__/useSlamMap.test.ts`

### Modified
- `backend/src/dash/config.py` — new settings (`slam_map_source`, `slam_map_local_path`, `slam_map_db_url`, `slam_map_db_token`)
- `backend/src/dash/factory.py` — wire `app.state.slam_map_source` at startup
- `backend/src/dash/api/map.py` — add `/api/map/slam{,/meta}`; remove all floorplan code
- `backend/src/dash/db/sqlite.py` — `_ensure_meter_columns` helper; new field names in CRUD
- `backend/tests/test_sensor_placements.py` — migration test + field-name updates
- `backend/tests/test_map_placements.py` — field-name updates
- `frontend/src/components/MapTab.tsx` — drop floorplan branch, use `useSlamMap`
- `frontend/src/components/MapCanvas.tsx` — accept `background` prop; apply Stage scale transform
- `frontend/src/components/SensorMarker.tsx`, `FOVCone.tsx`, `RotationHandle.tsx` — convert `range_pixels` prop → `range` (meters)
- `frontend/src/components/NodeSidebar.tsx` — minor: drag payload field renames if needed
- `frontend/src/components/__tests__/MapTab.test.tsx`, `MapCanvas.test.tsx` — update to new shape

### Deleted
- `backend/tests/test_map_floorplan.py`
- `frontend/src/components/FloorplanUploader.tsx`
- `frontend/src/components/__tests__/FloorplanUploader.test.tsx`

## Forward-compatibility checklist

When the SLAM team ships their DB service:
1. Set `SLAM_MAP_SOURCE=remote` + `SLAM_MAP_DB_URL` + `SLAM_MAP_DB_TOKEN`
2. Implement `RemoteSlamDbSource.get_meta()` and `get_image()` against the actual contract
3. No frontend changes required — the API surface is stable.
4. Existing sensor positions may need a one-time manual re-placement to align with the SLAM-published world frame (accepted caveat).
