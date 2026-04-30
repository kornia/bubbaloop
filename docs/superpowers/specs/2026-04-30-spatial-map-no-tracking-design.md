# Spatial Map (no tracking) — Design Spec

**Date:** 2026-04-30
**Status:** Draft (pending user review)
**Scope:** application-side only (`bubbaloop-nodes-official/`); zero changes to bubbaloop core

## 1. Goal

Project per-camera object detections into world coordinates and render them as a live map with raw history and visual zones. **No tracking, no identity, no entity persistence.** Each detection is an independent point in `(camera, time, class, x, y, confidence)` space.

## 2. Non-goals (explicit)

- **Tracking of any kind.** No `track_id`, no `entity_id`, no nearest-neighbour matching, no Kalman filters.
- **Persistent objects.** "The chair is here even when we don't see it" is not a claim this system makes. If no detection fires this second, nothing about that object exists in the live view.
- **Zone enter/exit events.** Zones are visual overlays only — drawn polygons rendered on the dashboard. No occupancy semantics, no dwell, no alerts.
- **Multi-camera fusion.** Each camera projects to its own local floor frame. Detections carry `camera`; the map shows union-of-cameras, no identity tying observations across views.
- **Bubbaloop core changes.** Daemon, MCP, SDK untouched. No new MCP tools.
- **Lance dataset writes.** The existing `spatial-map` node's Lance writer is removed.

## 3. Architecture

```
camera-object-detector ──► detections (JSON) ─┐
camera-pose             ──► pose       (JSON) ─┤
                                               ▼
                                        geo-projector  (per camera)
                                               │
                                               ▼ world_detections (JSON)
                                          ┌────┴─────┐
                                          ▼          ▼
                                  SiteMapView    spatial-map (node)
                                  (live blips)        │
                                                      ├─► SQLite log
                                                      ├─► snapshot publish (1 Hz)
                                                      └─► queryables: recent, zones
```

Two application changes, one cleanup:

| Component | Change |
|---|---|
| `geo-projector` | strip `WorldTracker` (NN tracker) and `track_id` field; keep projection + height estimator |
| `spatial-map` | rewrite: replace Lance writer with thin SQLite logger + zone CRUD + snapshot publisher |
| `crates/bubbaloop/src/daemon/detection_store.rs` | **delete** (untracked, 728 lines of drift) |

## 4. Wire format: `world_detections`

Single canonical JSON shape, published by `geo-projector` per detector frame:

```json
{
  "timestamp_ns": 1782151200000000000,
  "camera": "tapo_terrace",
  "detections": [
    {
      "class_name": "person",
      "confidence": 0.91,
      "world_x": 1.2,
      "world_y": 3.4,
      "pose_quality": "OK"
    }
  ]
}
```

**Removed fields** (vs. current): `track_id`. **No new fields.**

`world_x` / `world_y` are meters in **camera-local floor frame**, origin at the camera's vertical axis projection on the floor. Multi-camera fusion (deferred) would later add a global frame.

`pose_quality` reflects the upstream pose source — `"OK"` for static-calib pose, future SLAM backends may emit `"DEGRADED"` etc. `spatial-map` does not interpret it; it only logs and forwards.

## 5. `geo-projector` changes

| What | Action |
|---|---|
| `WorldTracker` class (~120 lines) | **Delete.** No replacement. |
| `track_id` assignment in main loop | **Delete.** Output detections in the order the detector emits them. |
| `MetricHeightEstimator` | **Keep.** It depends only on bbox geometry + class height table, not on tracking. |
| Projection math (intrinsics → ray → floor plane intersection) | **Keep unchanged.** |
| `node.yaml` `outputs[].description` | Update: drop "tracked objects" wording, change to "world-coordinate detections per frame". |
| `configs/*.yaml` | Strip any `track_*` parameters. |

**Net diff:** roughly `-130 lines / +0 lines` in `geo-projector/main.py`.

## 6. `spatial-map` node — new shape

A single-purpose Python node. Three responsibilities:

1. **Log:** subscribe `bubbaloop/**/world_detections`, append every detection row to SQLite.
2. **Zones:** accept zone CRUD via Zenoh subscribers; serve zone listing via queryable.
3. **Snapshot:** republish a small "what's happening right now" JSON on `{inst}/snapshot` at 1 Hz so the agent can read it via existing `get_node_sample`.

### 6.1 Storage — `~/.bubbaloop/sites/{site_id}/spatial.db`

```sql
PRAGMA foreign_keys = ON;
PRAGMA journal_mode = WAL;
PRAGMA synchronous = NORMAL;

-- 1. Raw detection log (every projected detection, immutable)
CREATE TABLE world_detections (
  id            TEXT PRIMARY KEY,        -- 'det_<uuid>'
  timestamp_ns  INTEGER NOT NULL,
  camera        TEXT NOT NULL,
  class_name    TEXT NOT NULL,
  world_x       REAL NOT NULL,
  world_y       REAL NOT NULL,
  confidence    REAL NOT NULL,
  pose_quality  TEXT NOT NULL
);
CREATE INDEX idx_det_ts            ON world_detections(timestamp_ns DESC);
CREATE INDEX idx_det_camera_ts     ON world_detections(camera, timestamp_ns DESC);
CREATE INDEX idx_det_class_ts      ON world_detections(class_name, timestamp_ns DESC);

-- 2. Visual zones (drawn polygons, no occupancy semantics)
CREATE TABLE site_zones (
  id            TEXT PRIMARY KEY,        -- 'zone_<uuid>'
  name          TEXT NOT NULL,
  camera        TEXT NOT NULL,
  polygon_json  TEXT NOT NULL,           -- '[[x,y],...]' meters, same frame as detections
  color         TEXT,                    -- optional CSS color
  created_at_ns INTEGER NOT NULL
);
```

Two tables. No `entities`, no `entity_id`, no `zone_events`.

### 6.2 Retention

A background sweeper (`threading.Thread`, 1-minute interval) deletes rows from `world_detections` older than `RETENTION_DAYS * 86400 * 1e9` ns. Default `RETENTION_DAYS = 7`. Configurable; `0` disables retention.

### 6.3 Tunables (node config YAML)

| Tunable | Default | Purpose |
|---|---|---|
| `RETENTION_DAYS` | 7 | Days of `world_detections` history kept; 0 disables |
| `SNAPSHOT_INTERVAL_S` | 1.0 | Period of `{inst}/snapshot` publish |
| `SNAPSHOT_RECENT_S` | 30 | Window of detections included in snapshot |
| `SWEEPER_INTERVAL_S` | 60 | Retention sweep period |
| `SITE_ID` | `"default"` | Subdirectory under `~/.bubbaloop/sites/` |

## 7. Zenoh interface

| Suffix | Direction | Encoding | Purpose |
|---|---|---|---|
| `bubbaloop/**/world_detections` | subscribe | application/json | Ingest from any `geo-projector` instance |
| `{inst}/snapshot` | publish (1 Hz) | application/json | Live state for agent `get_node_sample` |
| `{inst}/recent` | queryable | application/json | Filtered recent detections |
| `{inst}/zones` | queryable | application/json | List zones |
| `{inst}/zones/set` | subscribe | application/json | Create or update a zone |
| `{inst}/zones/delete` | subscribe | application/json | Delete a zone |

### 7.1 `{inst}/snapshot` (published)

```json
{
  "published_at_ns": 1782151230000000000,
  "window_s": 30,
  "detections": [
    {"ts": 1782151229000000000, "camera":"tapo_terrace",
     "class_name":"person","confidence":0.91,
     "world_x":1.2,"world_y":3.4,"pose_quality":"OK"}
  ],
  "zones": [
    {"zone_id":"zone_001","name":"Front","camera":"tapo_terrace",
     "polygon":[[0,0],[3,0],[3,3],[0,3]],"color":"#ff8800"}
  ]
}
```

Size budget: 50 KB. If exceeded, truncate `detections` to the most-recent 200 and emit `"truncated": true`.

### 7.2 `{inst}/recent` (queryable)

**Params:** `camera?`, `class?`, `since_ns?` (default `now − 60s`), `until_ns?` (default `now`), `limit?` (default `500`).

**Reply:**
```json
{"detections":[
  {"ts":..., "camera":..., "class_name":..., "confidence":...,
   "world_x":..., "world_y":..., "pose_quality":...}
]}
```

Implementation: indexed `SELECT ... FROM world_detections WHERE ... ORDER BY timestamp_ns DESC LIMIT :limit`.

### 7.3 `{inst}/zones` (queryable)

**Params:** `camera?`.

**Reply:**
```json
{"zones":[{"zone_id":"zone_001","name":"Front","camera":"tapo_terrace",
           "polygon":[[0,0],[3,0],[3,3],[0,3]],
           "color":"#ff8800","created_at_ns":...}]}
```

### 7.4 `{inst}/zones/set` (subscriber)

**Payload:**
```json
{"name":"Front","camera":"tapo_terrace",
 "polygon":[[0,0],[3,0],[3,3],[0,3]],
 "color":"#ff8800",
 "zone_id":"zone_001"}     // optional; present → update, absent → create
```

**Validation:**
- `name`: non-empty string ≤ 64 chars.
- `camera`: non-empty string ≤ 64 chars.
- `polygon`: array of ≥ 3 `[x, y]` pairs, all finite floats (no NaN / Inf).
- `color`: optional, `^#[0-9a-fA-F]{6}$` if provided.

On validation failure: log warning, drop. No reply (Zenoh subscribers are fire-and-forget).

### 7.5 `{inst}/zones/delete` (subscriber)

**Payload:** `{"zone_id":"zone_001"}`. Silent no-op if `zone_id` is unknown.

## 8. Dashboard contract (consumed in M4, not built here)

`SiteMapView.tsx` already subscribes to `world_detections` directly via Zenoh — that wiring stays. **However, the current dashboard logic (lines 582–594) keys live blips and trails by `d.track_id`.** Removing `track_id` from the wire format breaks that path. The dashboard must change in lockstep with this spec.

### 8.1 Required dashboard changes (in this spec's PR)

| Current | New |
|---|---|
| `live.set(d.track_id, …)` — dedupes live blips per track | `const detId = ${msg.timestamp_ns}_${idx}`; `live.set(detId, …)` — every detection is its own blip, no dedup |
| `trails.set(d.track_id, [pts…])` — multi-blip streak per track | **Removed.** Trails feature is dropped. |

Visual consequence: a person walking through the camera view used to render as a connected "streak" of fading dots (because the tracker glued them). Now it renders as **independent blips** along the path, each with its own 30 s fade. This is honest — there is no tracking — and visually communicates the boundary.

### 8.2 New data sources this spec adds

| Need | Where it comes from |
|---|---|
| Heatmap over arbitrary past window | New: HTTP/Zenoh GET `{inst}/recent?since_ns=...&until_ns=...` |
| Zone polygons rendered on the map | New: GET `{inst}/zones` once + refresh on observed `{inst}/zones/set` |
| User draws a zone in the dashboard | New: PUT (Zenoh) `{inst}/zones/set` with the polygon |

Wiring these into `SiteMapView.tsx` happens in the same PR as the `track_id` removal — the alternative is a known-broken dashboard, which we won't ship. Style polish (animations, scrubber, etc.) stays as M4 work.

## 9. Agent path

The agent reaches the spatial map by calling existing `get_node_sample(topic="{inst}/snapshot")`. That returns the latest published snapshot JSON. The agent can answer:

- "What's been detected on the terrace in the last 30 seconds?" → from `detections[]` filtered by `camera`.
- "How many people are on site right now?" → count of `class_name == "person"` in `detections[]`.
- "What zones are defined?" → from `zones[]`.

The agent **cannot** ask:
- "Trail of a specific entity" — there are no entities.
- "Who entered Front zone in the last hour" — no enter/exit events; only "detections inside the polygon" (which the agent could compute itself if given polygons + detections, but this spec does not provide that as a tool).
- Time-travel queries beyond the snapshot window — would require a `query_node` MCP tool (separate, future, generic spec).

## 10. Error handling

| Failure | Behavior |
|---|---|
| Malformed `world_detections` JSON | Log warning, drop frame. |
| Detection with non-finite x/y/confidence | Log warning, drop that detection (rest of frame still ingested). |
| SQLite `database is locked` | Auto-retry under `busy_timeout=5000ms`; beyond that, log error and drop frame. |
| Disk full | Log fatal, exit; supervisor restarts node. |
| Zone CRUD validation failure | Log warning, drop write. |
| Queryable handler exception | Reply with `{"error": "<message>"}` JSON; do not crash. |
| Snapshot publish exception | Log warning, skip this tick; next tick continues. |
| Retention sweep exception | Log warning, retry next interval. |

## 11. Testing

### 11.1 Unit (`bubbaloop-nodes-official/spatial-map/tests/`)

`pytest`, real in-memory SQLite (no mocks).

- `test_ingest_writes_row_per_detection` — frame with 3 detections → 3 rows in `world_detections`.
- `test_ingest_drops_nonfinite_coords` — detection with `world_x=NaN` is skipped; siblings still written.
- `test_zone_set_creates_then_updates` — POST without `zone_id` creates; same name + provided `zone_id` updates in place.
- `test_zone_set_validates_polygon_min_vertices` — < 3 vertices → drop.
- `test_zone_set_validates_polygon_finite` — NaN / Inf coords → drop.
- `test_zone_delete_removes_row` — delete known zone → row gone; unknown zone is no-op.
- `test_recent_query_filters_by_camera_and_class` — populate, query, assert filter & ordering.
- `test_recent_query_respects_time_window` — `since_ns` / `until_ns` filter applied.
- `test_snapshot_includes_recent_window_only` — write rows older than `SNAPSHOT_RECENT_S` → not in snapshot.
- `test_snapshot_size_budget_truncates` — 1000 recent detections → snapshot truncated to 200, `truncated:true`.
- `test_retention_drops_old_rows` — write rows with `ts = now - 8 days`; sweep with `RETENTION_DAYS=7` → rows gone.
- `test_retention_zero_disables` — `RETENTION_DAYS=0` → no rows deleted.

### 11.2 `geo-projector` regression test

- `test_projector_no_track_id_in_output` — feed synthetic pose + detection, assert output JSON has no `track_id` key, has `world_x` / `world_y`.

### 11.3 Live acceptance (manual, scripted in `tests/live_walkthrough.md`)

1. Restart `tapo-terrace-detector`, `tapo-terrace-pose`, `tapo-terrace-projector`, `spatial-map`.
2. Walk past `tapo_terrace`. Verify a dot appears in `SiteMapView` and a row lands in `~/.bubbaloop/sites/default/spatial.db` (`SELECT COUNT(*) FROM world_detections`).
3. Push a zone via `zenoh put '{inst}/zones/set' '{"name":"Front","camera":"tapo_terrace","polygon":[[0,0],[3,0],[3,3],[0,3]]}'`. Verify `{inst}/zones` returns it; verify dashboard renders the polygon overlay.
4. Wait `SNAPSHOT_INTERVAL_S + 1`; verify `bubbaloop` agent calling `get_node_sample("spatial-map-default/snapshot")` receives a JSON with the polygon and recent detection.
5. Delete zone via `zenoh put '{inst}/zones/delete' '{"zone_id":"zone_001"}'`. Verify `{inst}/zones` reply is empty.

### 11.4 Performance (loose targets)

- 60 detections / s sustained ingest, no SQLite write backlog.
- Snapshot publish < 30 ms with 200 recent detections.
- `{inst}/recent` reply < 100 ms over 100 k-row history.

## 12. Files touched

| Repo | File | Action | ~lines |
|---|---|---|---|
| `bubbaloop-nodes-official` | `geo-projector/main.py` | strip `WorldTracker`, drop `track_id` | -130 / +0 |
| `bubbaloop-nodes-official` | `geo-projector/node.yaml` | update output description | ±3 |
| `bubbaloop-nodes-official` | `geo-projector/configs/*.yaml` | strip any `track_*` keys | ±5 |
| `bubbaloop-nodes-official` | `spatial-map/main.py` | rewrite: SQLite logger + zone CRUD + snapshot | ~400 |
| `bubbaloop-nodes-official` | `spatial-map/store.py` | new (schema + queries) | ~150 |
| `bubbaloop-nodes-official` | `spatial-map/node.yaml` | update inputs / outputs / queryables | ±25 |
| `bubbaloop-nodes-official` | `spatial-map/pixi.toml` | drop `lance`, `pyarrow` | -2 |
| `bubbaloop-nodes-official` | `spatial-map/tests/*` | new test suite | ~250 |
| `bubbaloop-nodes-official` | `spatial-map/README.md` | update | ~50 |
| `bubbaloop` | `dashboard/src/components/SiteMapView.tsx` | drop `track_id`-keyed live map + trail map; key live blips by synthetic per-detection id; wire `{inst}/zones` and `{inst}/recent` queries | ~-80 / +120 |
| `bubbaloop` | `crates/bubbaloop/src/daemon/detection_store.rs` | **delete (untracked)** | -728 |
| `bubbaloop` | `CLAUDE.md` | one-paragraph note in Pitfalls about open-core boundary | +5 |

**Net core change:** -728 lines (only the never-committed drift file).
**Net node change:** ~+550 lines in `spatial-map`, ~-130 lines in `geo-projector`.

## 13. Implementation phasing (input for writing-plans)

1. **`detection_store.rs` removal + CLAUDE.md note** — `git rm`, paragraph in Pitfalls. (~0.25 day)
2. **`geo-projector` cleanup** — strip tracker, regression test, smoke against live camera. (~0.75 day)
3. **`spatial-map` storage layer** — `store.py`, schema, retention sweeper, unit tests. (~1 day)
4. **`spatial-map` zone CRUD** — `{inst}/zones/set`, `/delete`, `/zones` queryable, validation, tests. (~1 day)
5. **`spatial-map` recent queryable + snapshot publisher** — `{inst}/recent`, `{inst}/snapshot`, size budget, tests. (~1 day)
6. **Dashboard `SiteMapView` refactor** — drop track_id-keyed maps, synth detection id, wire `{inst}/zones` + `{inst}/recent`. Goes in the same PR as step 2 to avoid a known-broken intermediate state on `main`. (~1 day)
7. **Live walk-through** — acceptance script + tuning. (~0.5 day)

≈ **5.5 dev days.** No core changes.

## 14. Explicit deferrals

- **Tracking / identity** — could return as a separate spec if the demo ever wants "is that the same person?". Today's answer: no.
- **Heatmap aggregation queryable** — `{inst}/heatmap?grid_m=` for server-side bucketing. Today the dashboard's 30 s rolling buffer suffices; for longer windows the dashboard can call `{inst}/recent` and bucket client-side. Add later if performance demands it.
- **Multi-camera fusion (M2)** — out of scope; each camera publishes its own local floor frame.
- **Zone enter/exit events / dwell / alerts** — out of scope per Q2 = "visual zones only".
- **Lance nightly export** — not currently needed; SQLite is the source of truth.
- **Agent parameterised queries beyond snapshot** — would require a generic core `query_node` MCP tool; deferred.

## 15. Risks & mitigations

| Risk | Mitigation |
|---|---|
| Downstream consumers of `track_id` we missed | Pre-flight: `grep -rn 'track_id' bubbaloop bubbaloop-nodes-official dashboard/`. **Already known consumers:** `SiteMapView.tsx` lines 582–594 (handled in §8.1); `crates/bubbaloop/src/daemon/detection_store.rs` (deleted in this spec); `dashboard/src/components/Dashboard.tsx` subscribes the topic but does not read `track_id`. Anything found by the pre-flight grep that is not in this list goes into the PR's blocker list. |
| SQLite write backlog under heavy detection load | Keep ingest single-threaded with a buffered `Cursor.executemany()` per frame; performance test under 60 det/s in §11.4. |
| `spatial.db` grows unbounded if `RETENTION_DAYS=0` and forgotten | Document the sharp edge in README; default is 7. |
| Schema change to `world_detections` breaks dashboard live-blip and trail rendering | Mitigation is the dashboard refactor in §8.1, shipped in the same PR. Trails feature is intentionally dropped, not silently broken. |
| User has an existing `observations.lance` directory from the prior spatial-map version | Implementation plan logs a one-time warning on node start if the lance dir exists; no automated migration, no automated delete. README documents safe `rm -rf ~/.bubbaloop/sites/{site_id}/observations.lance` once the user is satisfied no data is needed. |

## Appendix A — Why no tracking is the right call here

- The demo's questions are about **what is happening now in space**, not **who is the same as before**. Without an appearance model (OSNet / re-id), any tracking layer is a guess that sometimes lies. Honest live detections are more trustworthy than dishonest persistent IDs.
- Tracking can be added later as an *additional* node that subscribes to `world_detections` and publishes `world_tracks`. The schema here doesn't preclude that — it just doesn't pay for it today.
- Without server-side `track_id`, each detection becomes an independent fading blip on the map. Closely-spaced detections along a trajectory still read as motion to the human eye, but the system makes no formal continuity claim and renders no connected trail.

## Appendix B — Why two SQLite tables, not one

`world_detections` is high-write, low-row-size, append-only.
`site_zones` is rare-write, larger row size (polygon JSON), point-lookup.
Mixing them would force every write transaction to touch the bigger schema and would complicate retention (we don't want to delete zones with detections). Two tables is the obvious shape and SQLite handles it without effort.
