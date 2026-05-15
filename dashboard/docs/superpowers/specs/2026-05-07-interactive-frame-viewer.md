# Interactive Frame Viewer Design

**Goal:** Make the carousel image view more informative — clickable detection boxes with depth readout, and a persistent frame metadata + detections sidebar.

**Architecture:** One new backend endpoint + a right-side sidebar in the carousel dialog. No new data model changes.

**Tech Stack:** FastAPI (backend), React + SVG (frontend), existing spatialstore LanceDB (numpy crop on depth_bytes / da3_depth_bytes).

---

## Backend

### `GET /v1/frames/{frame_id}/depth-at-box`
Params: `site_id`, `x1`, `y1`, `x2`, `y2` (image-pixel coordinates, integers).

Reads `depth_bytes` (uint16 PNG, mm, sensor) and `da3_depth_bytes` (uint16 PNG, predicted). Crops each to the box, filters pixels where value == 0 (invalid), returns min/median/max in metres and valid_ratio. Returns null for each source when not present.

```json
{
  "sensor": {"min_m": 0.85, "median_m": 1.12, "max_m": 1.43, "valid_ratio": 0.94},
  "da3":    {"min_m": 0.90, "median_m": 1.08, "max_m": 1.38, "valid_ratio": 1.0}
}
```

---

## Frontend — `LibraryView.tsx`

### Carousel sidebar (new `FrameInfoSidebar` component)
- ~200px fixed-width panel on the right of `carousel-dialog`.
- Image area `flex:1`, sidebar `width:200px flex-shrink:0`.
- Two sections:

**Frame**
Timestamp, camera_id, image size, pose (x/y/z, shown only if non-null), episode_id (shown only if non-null).

**Detections**
One row per detection: coloured dot matching the SVG box colour + label + score.
- Click a row → select it, fire `GET /depth-at-box` for that box, show spinner → replace with `sensor: 1.12m | da3: 1.08m` (or single value if only one source available, or "—" if neither).
- Clicking the image SVG box also selects the corresponding row.
- Selected row gets a highlighted border; selected SVG box gets `strokeWidth:3.5`.

### `ImageOverlay` changes
- Add `selectedIndex: number | null` and `onDetectionClick: (i: number) => void` props.
- Each detection `<g>` gets `pointer-events:all`, `cursor:pointer`, `onClick`.
- SVG root gets `pointer-events:all`.
- Selected box uses thicker stroke + slight fill tint.

### Mobile
Sidebar collapses to a bottom drawer (slides up from above the filmstrip). Toggle button in carousel header.

### ijepa
No sidebar section. Existing badge toggle + heatmap overlay is sufficient. The build is already verified clean.

---

## Out of scope
- Features section (xfeat/ijepa listing) — badges + overlays remain the only UI for those.
- Depth histogram or colour legend — future.
- Depth lookup for keypoints — future.
