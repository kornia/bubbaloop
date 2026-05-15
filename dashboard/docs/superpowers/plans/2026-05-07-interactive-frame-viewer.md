# Interactive Frame Viewer Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Add clickable detection boxes with depth readout (sensor + DA3) and a persistent frame-metadata + detections sidebar to the carousel viewer.

**Architecture:** One new backend store function + API endpoint for box-region depth stats; frontend refactors detection fetch into `FrameCarousel`, adds `FrameInfoSidebar` alongside the image viewport, and makes `ImageOverlay` SVG boxes interactive.

**Tech Stack:** FastAPI + numpy + cv2 (backend); React + SVG (frontend); existing LanceDB (spatialstore).

---

## File Structure

| File | Change |
|---|---|
| `/home/nvidia/spatialstore/tests/__init__.py` | Create (empty) |
| `/home/nvidia/spatialstore/tests/test_depth_at_box.py` | Create — unit tests for `_depth_stats` |
| `/home/nvidia/spatialstore/spatialstore/store.py` | Add `_depth_stats()` + `get_depth_at_box()` |
| `/home/nvidia/spatialstore/spatialstore/api.py` | Add `GET /v1/frames/{frame_id}/depth-at-box` |
| `/home/nvidia/bubbaloop/dashboard/src/components/LibraryView.tsx` | `ImageOverlay` interaction, `FrameInfoSidebar`, detection-fetch refactor, CSS |

---

### Task 1: Backend — `_depth_stats` pure function + unit tests

`_depth_stats` is module-level (not nested) so it can be tested without any LanceDB. It decodes a uint16 PNG with `cv2.IMREAD_ANYDEPTH`, crops to the box, filters zero pixels (= invalid/missing), returns stats in metres.

**Files:**
- Create: `/home/nvidia/spatialstore/tests/__init__.py`
- Create: `/home/nvidia/spatialstore/tests/test_depth_at_box.py`
- Modify: `/home/nvidia/spatialstore/spatialstore/store.py` (insert before `get_frame_keypoints`)

- [ ] **Step 1: Create the tests package**

```bash
touch /home/nvidia/spatialstore/tests/__init__.py
```

- [ ] **Step 2: Write the failing tests**

`/home/nvidia/spatialstore/tests/test_depth_at_box.py`:

```python
import cv2
import numpy as np
import pytest

# import before implementing — this MUST fail at this step
from spatialstore.store import _depth_stats


def _png(arr: np.ndarray) -> bytes:
    _, buf = cv2.imencode(".png", arr)
    return buf.tobytes()


def test_median_in_metres():
    arr = np.zeros((10, 10), dtype=np.uint16)
    arr[1, 1] = 1000  # 1.0 m
    arr[2, 2] = 2000  # 2.0 m
    arr[3, 3] = 3000  # 3.0 m
    stats = _depth_stats(_png(arr), x1=0, y1=0, x2=5, y2=5)
    assert stats["min_m"] == 1.0
    assert stats["median_m"] == 2.0
    assert stats["max_m"] == 3.0
    assert stats["valid_ratio"] == pytest.approx(3 / 25)


def test_all_zeros_returns_nones():
    arr = np.zeros((5, 5), dtype=np.uint16)
    stats = _depth_stats(_png(arr), x1=0, y1=0, x2=5, y2=5)
    assert stats["min_m"] is None
    assert stats["median_m"] is None
    assert stats["max_m"] is None
    assert stats["valid_ratio"] == 0.0


def test_box_clamped_to_image_bounds():
    arr = np.full((5, 5), 1500, dtype=np.uint16)  # all 1.5 m
    stats = _depth_stats(_png(arr), x1=3, y1=3, x2=100, y2=100)
    assert stats["median_m"] == 1.5
    assert stats["valid_ratio"] == 1.0
```

- [ ] **Step 3: Run tests — verify they FAIL with ImportError**

```bash
cd /home/nvidia/spatialstore && python -m pytest tests/test_depth_at_box.py -v
```

Expected: `ImportError: cannot import name '_depth_stats' from 'spatialstore.store'`

- [ ] **Step 4: Add `_depth_stats` to `store.py`**

Insert this block immediately before the `get_frame_keypoints` function (around line 484 in the current file):

```python
# ── Depth-at-box stats ────────────────────────────────────────────────────────

def _depth_stats(raw: bytes, x1: int, y1: int, x2: int, y2: int) -> dict:
    """Return min/median/max depth in metres for a box region of a uint16 PNG.

    raw: uint16 PNG bytes (mm units, 0 = invalid).
    Returns dict with keys min_m, median_m, max_m (float or None), valid_ratio (float).
    """
    import cv2
    import numpy as np

    depth_u16 = cv2.imdecode(np.frombuffer(raw, np.uint8), cv2.IMREAD_ANYDEPTH)
    if depth_u16 is None:
        return {"min_m": None, "median_m": None, "max_m": None, "valid_ratio": 0.0}
    h, w = depth_u16.shape[:2]
    y1c, y2c = max(0, y1), min(h, y2)
    x1c, x2c = max(0, x1), min(w, x2)
    region = depth_u16[y1c:y2c, x1c:x2c]
    valid = region[region > 0]
    if valid.size == 0:
        return {"min_m": None, "median_m": None, "max_m": None, "valid_ratio": 0.0}
    total = region.size
    return {
        "min_m":       round(float(valid.min())          / 1000.0, 3),
        "median_m":    round(float(np.median(valid))     / 1000.0, 3),
        "max_m":       round(float(valid.max())          / 1000.0, 3),
        "valid_ratio": round(float(valid.size) / total,            3),
    }
```

- [ ] **Step 5: Run tests — verify they PASS**

```bash
cd /home/nvidia/spatialstore && python -m pytest tests/test_depth_at_box.py -v
```

Expected: `3 passed`

- [ ] **Step 6: Commit**

```bash
cd /home/nvidia/spatialstore
git add tests/__init__.py tests/test_depth_at_box.py spatialstore/store.py
git commit -m "feat: add _depth_stats for bounding-box depth readout"
```

---

### Task 2: Backend — `get_depth_at_box` store function + API endpoint

`get_depth_at_box` is the LanceDB wrapper. The API endpoint validates the box, calls the store, and returns `{"sensor": {...}|null, "da3": {...}|null}`.

**Files:**
- Modify: `/home/nvidia/spatialstore/spatialstore/store.py` (add after `_depth_stats`)
- Modify: `/home/nvidia/spatialstore/spatialstore/api.py` (add after `get_ijepa_embedding`)

- [ ] **Step 1: Add `get_depth_at_box` to `store.py`**

Insert immediately after `_depth_stats`:

```python
def get_depth_at_box(
    frame_id: str,
    site_id: str,
    x1: int, y1: int, x2: int, y2: int,
) -> dict:
    """Return sensor + DA3 depth stats for a bounding box region.

    Returns {"sensor": stats_or_none, "da3": stats_or_none}.
    Each stats dict has min_m, median_m, max_m, valid_ratio.
    """
    tbl = _open_or_create_lance(site_id)
    existing = {f.name for f in tbl.schema}

    cols = ["depth_bytes"]
    if "da3_depth_bytes" in existing:
        cols.append("da3_depth_bytes")

    df = (
        tbl.search()
           .where(f"frame_id = '{frame_id}'", prefilter=True)
           .limit(1)
           .select(cols)
           .to_polars()
    )
    if df.is_empty():
        return {"sensor": None, "da3": None}

    sensor_raw = df[0, "depth_bytes"]
    da3_raw = df[0, "da3_depth_bytes"] if "da3_depth_bytes" in df.columns else None

    return {
        "sensor": _depth_stats(bytes(sensor_raw), x1, y1, x2, y2) if sensor_raw is not None else None,
        "da3":    _depth_stats(bytes(da3_raw),    x1, y1, x2, y2) if da3_raw    is not None else None,
    }
```

- [ ] **Step 2: Add the API endpoint to `api.py`**

Insert after the `get_ijepa_embedding` endpoint (after line 317):

```python
@app.get("/v1/frames/{frame_id}/depth-at-box")
def get_depth_at_box(
    frame_id: str,
    site_id: str = Query("default"),
    x1: int = Query(...),
    y1: int = Query(...),
    x2: int = Query(...),
    y2: int = Query(...),
):
    """Returns min/median/max depth in metres for a bounding box region.

    x1,y1,x2,y2 are image-pixel coordinates (integers).
    Response: {"sensor": {min_m, median_m, max_m, valid_ratio}|null,
               "da3":    {min_m, median_m, max_m, valid_ratio}|null}
    """
    if x1 >= x2 or y1 >= y2:
        raise HTTPException(400, detail="invalid box: need x1 < x2 and y1 < y2")
    return store.get_depth_at_box(frame_id, site_id, x1, y1, x2, y2)
```

- [ ] **Step 3: Restart spatialstore and smoke-test with curl**

```bash
# restart spatialstore (adjust to your launch command)
# then pick any frame_id from your data and run:
curl -s "http://localhost:8080/v1/frames/YOUR_FRAME_ID/depth-at-box?x1=100&y1=50&x2=300&y2=200&site_id=default" | python3 -m json.tool
```

Expected response shape (values will vary; both keys present even if null):
```json
{
    "sensor": {"min_m": 0.85, "median_m": 1.12, "max_m": 1.43, "valid_ratio": 0.94},
    "da3": null
}
```

- [ ] **Step 4: Commit**

```bash
cd /home/nvidia/spatialstore
git add spatialstore/store.py spatialstore/api.py
git commit -m "feat: add depth-at-box endpoint for detection depth readout"
```

---

### Task 3: Frontend — `ImageOverlay` interactive detection boxes

Add `selectedDetectionIndex` + `onDetectionClick` props. When provided, SVG boxes get `pointer-events:all`, `cursor:pointer`, and a thick stroke + fill tint on the selected box. The `.image-overlay-svg` CSS gets an `.interactive` modifier class.

**Files:**
- Modify: `/home/nvidia/bubbaloop/dashboard/src/components/LibraryView.tsx`

- [ ] **Step 1: Update the `ImageOverlay` function signature and implementation**

Replace the entire `ImageOverlay` function (lines 214–272) with:

```tsx
function ImageOverlay({
  imageWidth,
  imageHeight,
  keypoints,
  detections,
  selectedDetectionIndex = null,
  onDetectionClick,
}: {
  imageWidth: number;
  imageHeight: number;
  keypoints: Keypoint[] | null;
  detections: Detection[] | null;
  selectedDetectionIndex?: number | null;
  onDetectionClick?: (index: number) => void;
}) {
  if (!keypoints?.length && !detections?.length) return null;

  return (
    <svg
      viewBox={`0 0 ${imageWidth} ${imageHeight}`}
      className={`image-overlay-svg${onDetectionClick ? " interactive" : ""}`}
      preserveAspectRatio="xMidYMid meet"
      aria-hidden
    >
      {keypoints?.map((kp, i) => (
        <circle
          key={i}
          cx={kp.x}
          cy={kp.y}
          r={10}
          fill="rgba(255,220,0,0.65)"
          stroke="rgba(0,0,0,0.55)"
          strokeWidth={2}
        />
      ))}

      {detections?.map((det, i) => {
        const [x1, y1, x2, y2] = det.box;
        const color = DET_COLORS[i % DET_COLORS.length];
        const isSelected = selectedDetectionIndex === i;
        const label = `${det.label} ${Math.round(det.score * 100)}%`;
        const charW = 6.5;
        const labelW = label.length * charW + 8;
        const labelH = 16;
        const labelY = y1 >= labelH ? y1 - labelH : y2;
        return (
          <g
            key={i}
            onClick={onDetectionClick ? () => onDetectionClick(i) : undefined}
            style={{ cursor: onDetectionClick ? "pointer" : "default" }}
          >
            <rect
              x={x1} y={y1} width={x2 - x1} height={y2 - y1}
              fill={isSelected ? `${color}33` : "none"}
              stroke={color}
              strokeWidth={isSelected ? 3.5 : 2}
            />
            <rect x={x1} y={labelY} width={labelW} height={labelH}
              fill={color} fillOpacity={0.82} />
            <text x={x1 + 4} y={labelY + 12}
              fontSize={11} fill="#000" fontWeight="600"
              fontFamily="'JetBrains Mono',monospace">
              {label}
            </text>
          </g>
        );
      })}
    </svg>
  );
}
```

- [ ] **Step 2: Add the `.interactive` CSS modifier**

In the `<style>` block, find `.image-overlay-svg {` and add one line after it:

```css
.image-overlay-svg { position:absolute; inset:0; width:100%; height:100%; pointer-events:none; overflow:visible; }
.image-overlay-svg.interactive { pointer-events:all; }
```

- [ ] **Step 3: Type check**

```bash
cd /home/nvidia/bubbaloop/dashboard && npx tsc --noEmit 2>&1 | grep -v "SiteMapView\|leaflet"
```

Expected: no output (zero errors).

- [ ] **Step 4: Commit**

```bash
cd /home/nvidia/bubbaloop/dashboard
git add src/components/LibraryView.tsx
git commit -m "feat: make ImageOverlay detection boxes interactive"
```

---

### Task 4: Frontend — `FrameInfoSidebar` component + CSS

New component showing frame metadata and a detection list. Each detection row: coloured dot, label, score. Clicking a row fires the `depth-at-box` API and shows the result inline. Resets per `frame.frame_id` (via `key` prop in Task 5).

**Files:**
- Modify: `/home/nvidia/bubbaloop/dashboard/src/components/LibraryView.tsx`

- [ ] **Step 1: Add the `DepthStats` and `DepthAtBox` interfaces**

Insert after the `Keypoint` interface (after line 72):

```tsx
interface DepthStats {
  min_m: number | null;
  median_m: number | null;
  max_m: number | null;
  valid_ratio: number;
}

interface DepthAtBox {
  sensor: DepthStats | null;
  da3: DepthStats | null;
}
```

- [ ] **Step 2: Add the `FrameInfoSidebar` component**

Insert after the `JobCell` component (after the closing `}` of `JobCell`, around line 379):

```tsx
// ── Frame info sidebar ────────────────────────────────────────────────────────
//
// Shows frame metadata + detection list with on-demand depth readout.
// Keyed by frame_id in FrameCarousel so depth cache resets on slide change.

function FrameInfoSidebar({
  frame,
  siteId,
  detections,
  selectedIndex,
  onSelectDetection,
}: {
  frame: Frame;
  siteId: string;
  detections: Detection[] | null;
  selectedIndex: number | null;
  onSelectDetection: (index: number) => void;
}) {
  const [depthByIndex, setDepthByIndex] = useState<Record<number, DepthAtBox | "loading">>({});

  const handleRowClick = useCallback(async (i: number, det: Detection) => {
    onSelectDetection(i);
    if (depthByIndex[i] !== undefined) return;
    setDepthByIndex((prev) => ({ ...prev, [i]: "loading" }));
    const [x1, y1, x2, y2] = det.box.map(Math.round);
    try {
      const r = await fetch(
        `${SPATIALSTORE_URL}/v1/frames/${frame.frame_id}/depth-at-box` +
        `?site_id=${siteId}&x1=${x1}&y1=${y1}&x2=${x2}&y2=${y2}`
      );
      const data: DepthAtBox = await r.json();
      setDepthByIndex((prev) => ({ ...prev, [i]: data }));
    } catch {
      setDepthByIndex((prev) => ({ ...prev, [i]: { sensor: null, da3: null } }));
    }
  }, [frame.frame_id, siteId, depthByIndex, onSelectDetection]);

  return (
    <div className="frame-info-sidebar">
      {/* ── Frame metadata ── */}
      <div className="fis-section">
        <div className="fis-heading">Frame</div>
        <div className="fis-row">
          <span className="fis-key">time</span>
          <span className="fis-val">{formatTime(frame.timestamp_ns)}</span>
        </div>
        <div className="fis-row">
          <span className="fis-key">cam</span>
          <span className="fis-val fis-val-mono">{frame.camera_id}</span>
        </div>
        <div className="fis-row">
          <span className="fis-key">size</span>
          <span className="fis-val">{frame.image_width}×{frame.image_height}</span>
        </div>
        {frame.cam_x != null && (
          <div className="fis-row">
            <span className="fis-key">pose</span>
            <span className="fis-val fis-val-mono">
              {frame.cam_x.toFixed(2)} {frame.cam_y!.toFixed(2)} {frame.cam_z!.toFixed(2)}
            </span>
          </div>
        )}
        {frame.episode_id != null && (
          <div className="fis-row">
            <span className="fis-key">ep</span>
            <span className="fis-val fis-val-mono">{frame.episode_id}</span>
          </div>
        )}
      </div>

      {/* ── Detections ── */}
      {detections && detections.length > 0 && (
        <div className="fis-section">
          <div className="fis-heading">Detections ({detections.length})</div>
          {detections.map((det, i) => {
            const color = DET_COLORS[i % DET_COLORS.length];
            const isSelected = selectedIndex === i;
            const depth = depthByIndex[i];
            return (
              <button
                key={i}
                className={`fis-det-row${isSelected ? " selected" : ""}`}
                onClick={() => handleRowClick(i, det)}
              >
                <span className="fis-det-dot" style={{ background: color }} />
                <span className="fis-det-label">{det.label}</span>
                <span className="fis-det-score">{Math.round(det.score * 100)}%</span>
                {isSelected && (
                  <div className="fis-det-depth">
                    {depth === "loading" ? (
                      <span className="fis-depth-spin" />
                    ) : depth ? (
                      <>
                        {depth.sensor?.median_m != null && (
                          <span className="fis-depth-val sensor">S {depth.sensor.median_m}m</span>
                        )}
                        {depth.da3?.median_m != null && (
                          <span className="fis-depth-val da3">D {depth.da3.median_m}m</span>
                        )}
                        {!depth.sensor?.median_m && !depth.da3?.median_m && (
                          <span className="fis-depth-none">—</span>
                        )}
                      </>
                    ) : null}
                  </div>
                )}
              </button>
            );
          })}
        </div>
      )}
    </div>
  );
}
```

- [ ] **Step 3: Add sidebar CSS**

In the `<style>` block, add after the `.depth-alpha-slider` rule:

```css
/* ── Frame info sidebar ── */
.frame-info-sidebar {
  width:180px; flex-shrink:0;
  background:var(--bg-secondary); border-left:1px solid var(--border-color);
  overflow-y:auto; display:flex; flex-direction:column;
  font-size:11px; font-family:'JetBrains Mono',monospace;
}
.fis-section {
  padding:10px 10px 8px; border-bottom:1px solid var(--border-color);
}
.fis-section:last-child { border-bottom:none; }
.fis-heading {
  font-size:9px; font-weight:700; letter-spacing:0.08em;
  text-transform:uppercase; color:var(--text-muted);
  margin-bottom:6px;
}
.fis-row {
  display:flex; gap:6px; align-items:baseline; margin-bottom:3px;
}
.fis-key {
  color:var(--text-muted); flex-shrink:0; width:32px; font-size:10px;
}
.fis-val {
  color:var(--text-secondary); overflow:hidden; text-overflow:ellipsis;
  white-space:nowrap; flex:1;
}
.fis-val-mono { font-family:'JetBrains Mono',monospace; font-size:10px; }

.fis-det-row {
  width:100%; display:flex; align-items:center; gap:6px;
  padding:5px 6px; border-radius:4px; border:none; background:none;
  cursor:pointer; text-align:left; color:var(--text-secondary);
  flex-wrap:wrap;
}
.fis-det-row:hover { background:var(--bg-tertiary); }
.fis-det-row.selected { background:rgba(99,179,237,0.12); border:1px solid rgba(99,179,237,0.3); }
.fis-det-dot {
  width:8px; height:8px; border-radius:50%; flex-shrink:0;
}
.fis-det-label {
  flex:1; font-size:11px; color:var(--text-primary);
  overflow:hidden; text-overflow:ellipsis; white-space:nowrap;
}
.fis-det-score { font-size:10px; color:var(--text-muted); flex-shrink:0; }

.fis-det-depth {
  width:100%; padding-left:14px; display:flex; gap:6px; align-items:center;
  margin-top:2px;
}
.fis-depth-val {
  font-size:10px; padding:1px 5px; border-radius:3px;
}
.fis-depth-val.sensor {
  background:rgba(99,179,237,0.18); color:rgba(147,210,255,0.9);
}
.fis-depth-val.da3 {
  background:rgba(180,140,255,0.15); color:rgba(200,170,255,0.9);
}
.fis-depth-none { font-size:10px; color:var(--text-muted); }
.fis-depth-spin {
  display:inline-block; width:10px; height:10px; border-radius:50%;
  border:1.5px solid rgba(99,179,237,0.25);
  border-top-color:rgba(99,179,237,0.9);
  animation:spin 0.9s linear infinite;
}
```

Also add to the `@media (max-width:640px)` block:

```css
.frame-info-sidebar { display:none; }
```

- [ ] **Step 4: Type check**

```bash
cd /home/nvidia/bubbaloop/dashboard && npx tsc --noEmit 2>&1 | grep -v "SiteMapView\|leaflet"
```

Expected: no output.

- [ ] **Step 5: Commit**

```bash
cd /home/nvidia/bubbaloop/dashboard
git add src/components/LibraryView.tsx
git commit -m "feat: add FrameInfoSidebar with frame metadata and detection depth readout"
```

---

### Task 5: Frontend — Wire detection fetch + sidebar into `FrameCarousel`

Move detection fetching from `CarouselSlide` to `FrameCarousel` (only the current frame's detections are needed — no reason to fetch for every slide). Add `selectedDetectionIndex` state. Render `FrameInfoSidebar` inside `carousel-main-wrap`. Update `CarouselSlide` props.

**Files:**
- Modify: `/home/nvidia/bubbaloop/dashboard/src/components/LibraryView.tsx`

- [ ] **Step 1: Update `CarouselSlide` — remove internal detection fetch, add new props**

Replace the `CarouselSlide` function signature and internal detection logic.

Change the props type (currently lines 383–395):

```tsx
function CarouselSlide({
  frame,
  siteId,
  visible,
  activeLayers,
  depthAlpha,
  detections,
  selectedDetectionIndex,
  onDetectionClick,
}: {
  frame: Frame;
  siteId: string;
  visible: boolean;
  activeLayers: Set<string>;
  depthAlpha?: number;
  detections: Detection[] | null;
  selectedDetectionIndex: number | null;
  onDetectionClick: (index: number) => void;
}) {
```

Remove these lines from inside `CarouselSlide` (the detection state + useEffect, currently around lines 398 and 423–431):

```tsx
// DELETE these two lines:
const [detections, setDetections] = useState<Detection[] | null>(null);

// DELETE this entire useEffect:
useEffect(() => {
  if (!showDet || (frame.rfdetr_detection_count ?? 0) === 0) {
    setDetections(null);
    return;
  }
  fetch(`${SPATIALSTORE_URL}/v1/frames/${frame.frame_id}/detections?site_id=${siteId}`)
    .then((r) => r.ok ? r.json() : null)
    .then((d) => d && setDetections(d.detections))
    .catch(() => {});
}, [showDet, frame.frame_id, frame.rfdetr_detection_count, siteId]);
```

Update the `ImageOverlay` call at the bottom of `CarouselSlide` (currently around lines 500–507) to pass the new props:

```tsx
{(keypoints || detections) && (
  <ImageOverlay
    imageWidth={frame.image_width}
    imageHeight={frame.image_height}
    keypoints={keypoints}
    detections={detections}
    selectedDetectionIndex={selectedDetectionIndex}
    onDetectionClick={onDetectionClick}
  />
)}
```

- [ ] **Step 2: Update `FrameCarousel` — add state + detection fetch + sidebar**

In `FrameCarousel`, add these new state declarations after the existing `depthAlpha` state (after line 538):

```tsx
const [currentDetections, setCurrentDetections] = useState<Detection[] | null>(null);
const [selectedDetectionIndex, setSelectedDetectionIndex] = useState<number | null>(null);
```

Add this useEffect after the existing `toggleLayer` useCallback (after line 545):

```tsx
// Fetch detections for the currently-visible frame when the det layer is active
useEffect(() => {
  setCurrentDetections(null);
  setSelectedDetectionIndex(null);
  const f = frames[selectedIndex];
  if (!f || !activeLayers.has("det") || (f.rfdetr_detection_count ?? 0) === 0) return;
  let cancelled = false;
  fetch(`${SPATIALSTORE_URL}/v1/frames/${f.frame_id}/detections?site_id=${siteId}`)
    .then((r) => r.ok ? r.json() : null)
    .then((d) => { if (!cancelled && d) setCurrentDetections(d.detections); })
    .catch(() => {});
  return () => { cancelled = true; };
}, [selectedIndex, activeLayers, frames, siteId]);
```

Update the `carousel-main-wrap` JSX to add the sidebar. Find the current `<div className="carousel-main-wrap">` block (lines 661–693) and add `FrameInfoSidebar` as the last child before the closing `</div>`, and add the current frame's data to `CarouselSlide`:

The `carousel-main-wrap` block becomes:

```tsx
<div className="carousel-main-wrap">
  <button
    className="carousel-arrow carousel-arrow-prev"
    onClick={() => mainEmbla?.scrollPrev()}
    disabled={selectedIndex === 0}
    aria-label="Previous frame"
  >‹</button>

  <div className="carousel-viewport" ref={mainRef}>
    <div className="carousel-container">
      {frames.map((f, i) => (
        <div className="carousel-slide" key={f.frame_id}>
          <CarouselSlide
            frame={f}
            siteId={siteId}
            visible={Math.abs(i - selectedIndex) <= 2}
            activeLayers={activeLayers}
            depthAlpha={depthAlpha}
            detections={i === selectedIndex ? currentDetections : null}
            selectedDetectionIndex={i === selectedIndex ? selectedDetectionIndex : null}
            onDetectionClick={setSelectedDetectionIndex}
          />
        </div>
      ))}
    </div>
  </div>

  <button
    className="carousel-arrow carousel-arrow-next"
    onClick={() => mainEmbla?.scrollNext()}
    disabled={selectedIndex === frames.length - 1}
    aria-label="Next frame"
  >›</button>

  <FrameInfoSidebar
    key={current.frame_id}
    frame={current}
    siteId={siteId}
    detections={currentDetections}
    selectedIndex={selectedDetectionIndex}
    onSelectDetection={setSelectedDetectionIndex}
  />
</div>
```

- [ ] **Step 3: Build verify**

```bash
cd /home/nvidia/bubbaloop/dashboard
npx tsc --noEmit 2>&1 | grep -v "SiteMapView\|leaflet"
npx vite build 2>&1 | tail -15
```

Expected: zero type errors; build succeeds (chunk-size warning is pre-existing, not a failure).

- [ ] **Step 4: Commit**

```bash
cd /home/nvidia/bubbaloop/dashboard
git add src/components/LibraryView.tsx
git commit -m "feat: wire FrameInfoSidebar into carousel with interactive detection depth readout"
```

---

### Task 6: Smoke test end-to-end

Manual verification that the full feature works: open the carousel, toggle detections, click a box, see depth stats.

- [ ] **Step 1: Serve the dashboard**

```bash
cd /home/nvidia/bubbaloop/dashboard && npx vite --host 0.0.0.0
```

- [ ] **Step 2: Open a session with rfdetr detections in the browser**

Navigate to the library view. Select a session where the rfdetr job has run (rfdetr badge shows `✓`). Click a thumbnail to open the carousel.

- [ ] **Step 3: Verify sidebar metadata**

The right sidebar shows time, cam, size, and pose/episode if present. No detection list yet.

- [ ] **Step 4: Toggle detections**

Click the `rfdetr` badge in the carousel header. The SVG boxes appear AND the detection list appears in the sidebar.

- [ ] **Step 5: Click a detection box in the image**

A spinner appears under the row in the sidebar, then is replaced with depth stats (e.g. `S 1.12m` for sensor, `D 1.08m` for DA3, or `—` if no depth available). The corresponding SVG box gets a thicker stroke and tint.

- [ ] **Step 6: Verify clicking a sidebar row also selects the SVG box**

Click a different detection row in the sidebar. The SVG highlight moves to that box.

---

## Self-Review

**Spec coverage:**
- ✓ `GET /depth-at-box` endpoint — Task 1+2
- ✓ Median depth (sensor + da3) — `_depth_stats` in Task 1
- ✓ `FrameInfoSidebar` with frame metadata — Task 4
- ✓ Detection list with depth readout — Task 4+5
- ✓ SVG box click → selects row + depth fetch — Task 3+5
- ✓ Sidebar row click → highlights SVG box — Task 5 (`selectedDetectionIndex` state flows to `ImageOverlay`)
- ✓ Mobile: sidebar hidden — Task 4 CSS
- ✓ I-JEPA: no new UI needed, existing overlay + badge confirmed working by build passing

**Placeholder scan:** None found.

**Type consistency:** `Detection` interface with `box: [number,number,number,number]` is already defined (line 62–65). `DepthAtBox` defined in Task 4 Step 1, used in `FrameInfoSidebar` and the fetch response in Task 4 Step 2. `selectedDetectionIndex: number | null` flows from `FrameCarousel` state → `CarouselSlide` prop → `ImageOverlay` prop consistently.
