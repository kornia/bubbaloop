# bubbaloop-dash — Design

**Status:** Draft (brainstorm complete, awaiting user review)
**Date:** 2026-05-20
**Owner:** Edgar Riba
**Related specs:**
- (future) Spec 2 — Map tab + sensor placement (Section 1's E + F decomposition)
- (future) Spec 3 — Remote-hosting hardening (Section 1's A decomposition)

---

## Executive summary

`bubbaloop-dash` is a new sibling project to the existing Bubbaloop edge runtime. It owns "everything Bubbaloop knows about *the past* and *uploads*": video upload from iPhone/desktop, frame decode + embedding, LanceDB-backed vector index, REST search, and the dashboard UI itself.

It is a **self-hosted** Python (FastAPI) backend + the existing React/Vite frontend, packaged as a single Docker container. It runs on a user-owned VPS, reachable from anywhere over HTTPS. The browser talks only to `bubbaloop-dash`; `bubbaloop-dash` proxies live Zenoh sensor data from the user's edge daemon (Jetson) over a Tailscale/VPN link.

The bubbaloop edge daemon (single Rust binary, single-binary install) keeps its existing scope: real-time sensor orchestration. It does **not** grow video indexing, LanceDB, or any of the new feature set.

**Out of scope for this spec:** Map canvas, kornia-slam integration, sensor placement, multi-Jetson aggregation, multi-user, world-model integration. Each has a clean hook for later work; none is built in v1.

---

## Section 1 — Identity, scope, name

**Repository name:** `bubbaloop-dash`

**Mission.** A self-hosted server that owns "everything Bubbaloop knows about the past and uploads." Lives on a VPS (user's choice), serves the dashboard UI, accepts iPhone/video uploads, decodes and embeds them into LanceDB, and proxies live Zenoh subscriptions to the browser. Single Docker image. Single-user. Single Bubbaloop fleet (one daemon) in v1.

### In scope (v1)

- Resumable multi-GB video upload (HTTP, chunked PUTs)
- Decode + frame sampling (1 fps default, configurable)
- Embedding inference (open_clip ViT-B/32 → 512-dim, configurable)
- LanceDB writes: `{video_id, frame_idx, ts_ms, embedding[512], jpg_path, meta, source_kind, provenance, embed_model, embed_dim}`
- JPG storage on disk under `${DATA_DIR}/videos/<id>/streams/rgb/`
- REST query surface: vector search, scalar filter, static frame fetch, video CRUD
- Job lifecycle: enqueue, progress (over Zenoh), cancel, retry; persisted in SQLite for crash recovery
- Frontend served as static files (existing `bubbaloop/dashboard/` forked into `bubbaloop-dash/frontend/`)
- Zenoh-over-WebSocket proxy: browser ↔ backend ↔ daemon's `zenohd` (single-origin)
- Single bearer-token auth (`DASHBOARD_TOKEN` env var)
- `segment_mode` upload parameter reserved for future scene-detection

### Out of scope (v1)

- User accounts / OAuth / multi-tenant
- Multi-Jetson aggregation
- Historical telemetry / alert persistence
- Map tab + sensor placement (Spec 2)
- SLAM integration (future)
- Rate limiting, audit log, production hardening beyond TLS + bearer
- World-model integration (Phase 2+)
- Object storage backend (S3/MinIO) — interface in place, impl deferred
- Metrics / tracing / Sentry

---

## Section 2 — Migration approach + repo structure

### Migration approach

This is **not a greenfield project**. It is three moves bundled:

1. **Migrate** (by fork-snapshot) the existing `crates/bubbaloop/dashboard/` (React + Vite + TS) into `bubbaloop-dash/frontend/`.
2. **Add** a new Python backend (`bubbaloop-dash/backend/`) for upload, indexing, LanceDB, REST API, Zenoh-WS proxy.
3. **Keep** the bubbaloop repo's `dashboard/` and `--features dashboard` untouched. Both dashboards coexist; the user switches when ready. A future cleanup spec retires the old one.

The lift is a **fork snapshot**, not a `git subtree split`. Initial commit in `bubbaloop-dash` copies `bubbaloop/dashboard/` at a specific SHA, with the commit message referencing the source SHA so lineage is recoverable. History before that point lives in the bubbaloop repo permanently.

### Repo structure

```
bubbaloop-dash/
├── backend/
│   ├── pyproject.toml             # uv / hatchling, Python 3.11
│   ├── src/dash/
│   │   ├── __init__.py
│   │   ├── app.py                 # FastAPI factory
│   │   ├── config.py              # Settings (pydantic-settings)
│   │   ├── api/
│   │   │   ├── videos.py          # upload, list, delete, get
│   │   │   ├── search.py          # vector + scalar query
│   │   │   ├── frames.py          # static frame fetch
│   │   │   ├── jobs.py            # job status, cancel
│   │   │   └── zenoh_ws.py        # WS proxy to daemon
│   │   ├── jobs/
│   │   │   ├── runner.py          # asyncio queue + SQLite-backed state
│   │   │   ├── index_video.py     # decode → embed → write
│   │   │   └── sweeper.py         # integrity sweeper
│   │   ├── db/
│   │   │   ├── lance.py           # LanceDB connection + schema
│   │   │   └── jobs_sqlite.py     # job state persistence
│   │   ├── decode/
│   │   │   └── pyav.py            # PyAV wrapper, keyframe-aware sampler
│   │   ├── embed/
│   │   │   └── open_clip.py       # model load + batch inference
│   │   ├── storage/
│   │   │   ├── base.py            # Storage Protocol
│   │   │   └── filesystem.py      # local FS impl
│   │   ├── zenoh_proxy/
│   │   │   └── bridge.py          # zenoh-python ↔ FastAPI WebSocket multiplex
│   │   └── schemas/               # generated _pb2.py (committed)
│   ├── tests/
│   └── README.md
├── frontend/                      # forked from bubbaloop/dashboard/@<sha>
│   ├── package.json
│   ├── src/
│   │   ├── components/
│   │   │   ├── MapTab.tsx         # NEW — container for upload/library/search + map canvas placeholder
│   │   │   ├── VideoLibrary.tsx   # NEW
│   │   │   ├── VideoUploader.tsx  # NEW
│   │   │   ├── VideoDetail.tsx    # NEW
│   │   │   ├── FrameSearch.tsx    # NEW
│   │   │   ├── FrameGrid.tsx      # NEW
│   │   │   ├── MapCanvas.tsx      # NEW — placeholder, Spec 2 hooks in
│   │   │   ├── Login.tsx          # NEW
│   │   │   └── ... (existing Dashboard, Mesh, Chat, etc. preserved)
│   │   ├── contexts/
│   │   │   ├── AuthContext.tsx    # NEW
│   │   │   └── ... (existing preserved)
│   │   └── lib/
│   │       ├── api.ts             # NEW — typed REST client with bearer-token attach
│   │       └── zenoh.ts           # existing, endpoint updated to /ws/zenoh
│   └── vite.config.ts             # /api → backend dev proxy
├── proto/                         # vendored from kornia/bubbaloop:crates/bubbaloop-schemas/
│   ├── header.proto               #   (pinned via VERSION; updated by sync-protos.sh)
│   ├── ... (other .proto files)
│   └── VERSION                    # git SHA of bubbaloop repo this dir was synced from
├── scripts/
│   ├── sync-protos.sh             # fetch protos from bubbaloop repo, regen _pb2.py
│   └── dev.sh                     # uvicorn + vite dev side-by-side
├── Dockerfile                     # multi-stage: node→build, python→runtime
├── docker-compose.yml             # single-service production
├── docker-compose.dev.yml         # bind-mount sources, hot reload
├── .github/workflows/ci.yml
├── CLAUDE.md                      # project-specific rules
├── README.md
└── ARCHITECTURE.md                # living doc
```

### Design notes

- **`src/` layout** for the Python backend — standard modern Python, no namespace pollution.
- **Schema sharing:** `scripts/sync-protos.sh` fetches `.proto` files from the bubbaloop repo at a pinned SHA (`proto/VERSION`), regenerates `_pb2.py`. Both protos and generated bindings committed. Bumping requires a deliberate PR. Reason: git submodules cause "I cloned and nothing works" bugs; vendoring + a version pin is the boring, durable pattern.
- **Three storage tiers (LanceDB / SQLite / FS):** each has a real reason. LanceDB is best at vector search and bad at random small writes. SQLite is best at small ACID writes (job state) and bad at vectors. Filesystem is best at multi-MB blobs and bad at queries. Trying to consolidate forces one to do something it's bad at.
- **Dockerfile is multi-stage:** node:20 builds the frontend, python:3.11 image copies `dist/` into static path that FastAPI serves.
- **`pyproject.toml` uses uv** — fast resolver, sane caching, modern lockfile.
- **No Alembic / SQLAlchemy.** SQLite for jobs via raw `sqlite3` (WAL mode). LanceDB native API for frames. Filesystem for blobs.

### What gets retired in `bubbaloop` repo

**Nothing in v1.** `dashboard/` and the `dashboard` cargo feature both stay. A future spec retires them after `bubbaloop-dash` has proven itself.

---

## Section 3 — Architecture, topology, and data flow

### Topology

```
                                  Internet
                                      |
                                      v
   +-------------------------- bubbaloop-dash (VPS) ---------------------------+
   |                                                                          |
   |   :443 (TLS via Caddy / Cloudflare reverse-proxy)                        |
   |          |                                                               |
   |          v                                                               |
   |     +-----------+      +-----------+      +------------------+           |
   |     | FastAPI   |<---->| Job runner|----->| open_clip        |           |
   |     | (uvicorn) |      | (asyncio) |      | (CPU or CUDA)    |           |
   |     +-----+-----+      +-----+-----+      +------------------+           |
   |           |                  |                                            |
   |           |  +---------------+--------+                                   |
   |           |  |               |        |                                   |
   |           v  v               v        v                                   |
   |        +------+         +--------+ +-----------+                          |
   |        |Lance |         | SQLite | | FS blobs  |                          |
   |        | DB   |         |(jobs)  | |(mp4, jpg) |                          |
   |        +------+         +--------+ +-----------+                          |
   |                                                                          |
   |     +----------------------- zenoh client ----------------+               |
   |     | subscribes to bubbaloop/global/** over WSS bridge   |               |
   |     +----------------------------+------------------------+               |
   |                                  |                                       |
   +----------------------------------|---------------------------------------+
                                      |
                                      v
                          Tailscale (or any WAN tunnel)
                                      |
                                      v
   +------------------------ Jetson (LAN) ------------------------+
   |   zenohd (WSS, auth via shared key)                          |
   |        ^                                                      |
   |        | bubbaloop daemon                                      |
   |        +---- nodes (RTSP cams, sensors, ...)                  |
   +--------------------------------------------------------------+

                       browser  <--(WSS)-->  bubbaloop-dash
                                            (NEVER directly to zenohd)
```

### Key architectural choices

- **Single origin** for the browser. Everything is `https://dash.<user>.com/...`. No CORS, no mixed-content, no second WebSocket endpoint. Reduces attack surface and config burden.
- **Zenoh client lives in the backend**, not the browser. Backend opens one persistent connection to the daemon's `zenohd` over Tailscale. Backend exposes `/ws/zenoh` to the browser that **multiplexes** browser subscriptions onto its single upstream connection (ref-counted dedupe).
- **TLS terminates upstream of FastAPI.** Caddy/Traefik/Cloudflare for cert management. FastAPI listens plain HTTP inside Docker.
- **No direct daemon HTTP calls** for now. The new backend doesn't proxy `:8088`. Anything needed from the daemon goes through Zenoh queries — collapses two protocols to one.

### Data flow 1 — Upload + index

```
1. Browser: drag iPhone.mp4 (2.3 GB) into VideoUploader
2. Browser → POST /api/videos { filename, size, content_type, sample_fps?, segment_mode? }
   Backend creates SQLite job row (status=uploading), allocates DATA_DIR/videos/<id>/
   Returns: { video_id, chunk_size, upload_url_template }
3. Browser → PUT /api/videos/<id>/chunks/<n> (binary, repeating, up to 3 concurrent)
   Backend appends to source.mp4, updates SQLite progress
4. Browser → POST /api/videos/<id>/complete { sha256? }
   Backend verifies size; transitions job to status=queued; returns { job_id, queue_position }
5. Job runner picks up:
   a. PyAV opens source.mp4, reads codec/duration/fps; writes row to LanceDB 'videos' table
   b. For each sampled frame (1 fps default):
      - decode → ndarray
      - resize longest-side to 1280, JPEG q=90
      - write to DATA_DIR/videos/<id>/streams/rgb/000001.jpg
      - batch into embedding buffer (size 32)
      - on flush: open_clip.encode_image(batch) → 32 vectors
      - write 32 rows to LanceDB 'frames' table
      - update SQLite progress (pct, last_ts_ms, eta_s)
   c. transition job to status=done
6. Progress publishes to bubbaloop-dash/jobs/<job_id>/progress every ~500ms.
   Browser subscribes via /ws/zenoh proxy — no polling, no SSE.
```

### Data flow 2 — Search

```
1. Browser: user types "package on doorstep"
2. Browser → POST /api/search { text: "...", filters?: {...}, limit: 24 }
3. Backend: open_clip.encode_text(...) → 512-d vector
4. Backend: LanceDB ANN query → top-24 frames by cosine similarity
5. Backend → response:
   { results: [{ video_id, frame_idx, ts_ms, start_sec, end_sec,
                 score, thumbnail_url, meta }] }
6. Browser renders FrameGrid; each cell <img src={thumbnail_url}/> hits
   /api/frames/<video_id>/<frame_idx>.jpg (static file served by FastAPI)
```

### Data flow 3 — Live sensor view

```
1. Browser opens WSS to backend at /ws/zenoh?token=...
2. Browser sends { type: "subscribe", id, key_expr: "bubbaloop/global/<machine>/entrance/data" }
3. Backend (if not already subscribed) opens upstream sub on its zenohd connection
4. Sample arrives → backend pushes { type: "sample", id, key_expr, payload, encoding } to browser
5. Backend ref-counts subscriptions so multiple browser tabs share one upstream sub
6. On disconnect: ref-count → 0 → backend undeclares upstream sub
```

### Forward-compat hooks for future world-model integration

Three cheap additions in v1 prevent future schema migration:

- `videos.kind: str` column — `"upload" | "simulation" | "live_recording"` (default `"upload"`).
- `frames.source_kind: str` + `provenance: dict` columns — origin metadata per frame.
- Storage path `videos/<id>/streams/rgb/*.jpg` (not flat `frames/`) — allows future `depth/`, `segmentation/`, `pose/` sibling streams.

These cost ~5 LoC each. World-model integration designs its full surface in a later spec; v1's job is only to **not forbid** it.

---

## Section 4 — REST API + auth + defaults + LanceDB schema

### Auth (v1)

- Every endpoint except `GET /healthz` requires `Authorization: Bearer <token>`.
- One token, sourced from `DASHBOARD_TOKEN` env var. Generated at first run if unset (printed to logs, persisted to `${DATA_DIR}/.token`).
- Frontend: simple "paste your token" login screen, persists to `localStorage`. No login backend, no sessions, no password hashing — single-tenant doesn't need it.
- WSS auth uses query param (`?token=...`) because browser `WebSocket` constructor cannot send custom headers. Server strips token from request URL before logging.
- TLS terminates upstream (Caddy/Cloudflare). FastAPI listens plain HTTP inside the container.

### Token table (v1: single row, future-shape preserved)

```python
{
    id: str,           # uuid
    kind: str,         # "user" | "device" | "admin"  — only "admin" populated in v1
    scopes: list[str], # ["*"] for v1
    created_at: int,
    revoked_at: int | None,
}
```

The schema accommodates future device tokens (for the bubbaloop daemon to call back into dash, e.g., to register itself or upload its own recordings) without migration. Pattern matches Foxglove / Freedom Robotics / Roboto.

### REST endpoints

```
POST   /api/videos
       body:  { filename, size_bytes, content_type,
                sample_fps?, embed_model?, segment_mode?: "fixed" | "scene" }
       resp:  { video_id, chunk_size, upload_url_template }
       creates SQLite job row (status=uploading), allocates DATA_DIR/videos/<id>/
       note: segment_mode="scene" returns 501 in v1 (Phase 1.5 enables PySceneDetect)

PUT    /api/videos/{id}/chunks/{n}
       body:  binary chunk
       resp:  { received_bytes, etag }
       appends to source.mp4; idempotent on (id, n) — re-PUT same chunk = no-op

POST   /api/videos/{id}/complete
       body:  { sha256? }
       resp:  { job_id, queue_position }
       verifies size; flips job to status=queued; runner picks it up

GET    /api/videos
       query: ?status=&kind=&limit=&cursor=
       resp:  { items: [VideoSummary], next_cursor }

GET    /api/videos/{id}
       resp:  VideoDetail { id, filename, duration_s, fps, codec, frame_count,
                            uploaded_at, status, source_kind, provenance,
                            indexed_frame_count, received_chunks: bitmap }
       received_chunks lets a client resume an interrupted upload.

DELETE /api/videos/{id}
       cascades: source.mp4, frames/, LanceDB rows in `videos` + `frames`,
                 SQLite jobs for this video

POST   /api/search
       body:  { text?: str,
                image: { video_id, frame_idx } | { jpg_b64 } | None,
                filters?: { video_id?, kind?, from_ts?, to_ts? },
                limit: int = 24 }
       resp:  { results: [{ video_id, frame_idx, ts_ms,
                             start_sec, end_sec, score,
                             thumbnail_url, meta }] }
       embeds query (text via encode_text, image via encode_image), runs Lance ANN
       score is float 0..1 (cosine similarity)
       note: v1 (Phase 1.3) supports text queries only. Requests with `image`
             populated return 501 Not Implemented. Image-similarity ships in Phase 1.5.

GET    /api/frames/{video_id}?from_ts=&to_ts=&limit=&cursor=
       resp:  { items: [FrameSummary], next_cursor }
       scalar query, no vector

GET    /api/frames/{video_id}/{frame_idx}.jpg
       resp:  image/jpeg bytes  (or 302 to a presigned URL when Storage backend supports it)
       served from DATA_DIR/videos/<id>/streams/rgb/<idx>.jpg

GET    /api/jobs
       resp:  { items: [JobSummary { id, kind, video_id, status,
                                     progress_pct, started_at, error }] }

GET    /api/jobs/{id}
       resp:  JobDetail
       (also publishes live progress on Zenoh — see below)

DELETE /api/jobs/{id}
       cancels a running/queued job; cleans up partial state

GET    /healthz                     # liveness, no auth
GET    /readyz                      # readiness: zenoh connected, lancedb open

WS     /ws/zenoh?token=...          # multiplexed Zenoh proxy
       protocol matches zenoh-bridge-remote-api subset:
       { type: "subscribe", id, key_expr } | { type: "unsubscribe", id }
                                          → { type: "sample", id, key_expr,
                                              payload, encoding }
```

### Job progress on Zenoh (not SSE)

Each running job publishes to `bubbaloop-dash/jobs/<job_id>/progress` (JSON: `{ pct, stage, last_ts_ms, eta_s }`) every ~500ms. Browser subscribes via the `/ws/zenoh` proxy — same mechanism as live sensor data. **One real-time channel for everything.**

### Defaults

| Concern | Default | Override | Why |
|---|---|---|---|
| Embedding model | `open_clip` ViT-B/32, `laion2b_s34b_b79k` | `EMBED_MODEL`, `EMBED_PRETRAINED` env; per-upload | 512-dim, fast on CPU, ubiquitous baseline. Good text↔image alignment. |
| Sampling rate | 1 fps | `SAMPLE_FPS` env; per-upload | Balances index density with storage; ~5 KB JPG + 2 KB embedding per frame ≈ 25 MB/hour. |
| Segment mode | `fixed` (1 fps) | `segment_mode` param; "scene" returns 501 in v1 | iPhone home video benefits from scene detection; reserve param to add later without API break. |
| Frame format | JPEG q=90, resized longest-side 1280 | `JPG_QUALITY`, `JPG_MAX_DIM` env | Visually lossless at 90; cap prevents iPhone 4K from dominating disk. |
| Decoder | PyAV (in-process FFmpeg) | n/a | Avoids subprocess overhead; well-maintained. |
| Job concurrency | 1 indexing job | `JOB_CONCURRENCY` env | Prevents GPU/CPU contention; trivial to lift. |
| Upload chunk size | 8 MB | client-driven; server caps at 64 MB | Plays well with browser memory and FastAPI request buffer. |
| Storage root | `${DATA_DIR}` (default `/var/lib/bubbaloop-dash`) | `DATA_DIR` env | Single mount point for Docker volume. |
| Job state DB | SQLite WAL at `${DATA_DIR}/state.sqlite` | n/a | Crash recovery; tiny footprint. |
| LanceDB root | `${DATA_DIR}/lance/` | n/a | Two tables: `videos`, `frames`. |
| Listen port | 8000 | `PORT` env | Standard FastAPI default. |

### Storage interface

```python
class Storage(Protocol):
    def write(self, path: str, data: bytes) -> None: ...
    def read(self, path: str) -> bytes: ...
    def url_for(self, path: str) -> str: ...
    def delete(self, path: str) -> None: ...
```

- v1 implementation: `FilesystemStorage` under `DATA_DIR`. `url_for(path)` returns `/api/frames/<id>/<idx>.jpg` (FastAPI route serves from disk).
- Phase 2 implementation: `S3Storage`. `url_for(path)` returns a presigned GET URL; `GET /api/frames/...` becomes a `302` redirect. Same API surface, swappable backend. Pattern matches Foxglove / Rerun / Roboto / V7 Darwin.

### LanceDB schema

```python
videos = {
    id:                   str       # uuid4
    filename:             str
    kind:                 str       # "upload" | "simulation" | "live_recording"
    duration_s:           float
    fps:                  float
    codec:                str
    frame_count:          int       # original (decoded)
    indexed_frame_count:  int       # sampled + embedded
    uploaded_at:          int       # epoch ms
    status:               str       # "queued" | "indexing" | "done" | "error"
    source_kind:          str       # "upload" by default
    provenance:           dict      # {} by default
}

frames = {
    video_id:    str
    frame_idx:   int       # ordinal in the sampled sequence (not source frame #)
    ts_ms:       int       # presentation timestamp in source video
    embedding:   list[float]   # f32[D], cosine-indexed
    jpg_path:    str       # relative to DATA_DIR
    source_kind: str
    provenance:  dict
    embed_model: str       # what model produced this vector
    embed_dim:   int       # for sanity / future multi-model joins
}
```

`embed_model` + `embed_dim` let a future spec add a second index (DINOv2, SigLIP) without losing existing vectors. Pattern inspired by FiftyOne's "brain_key" — named persistent embedding indexes.

---

## Section 5 — Frontend changes

### Big picture

The original ask placed video upload + library inside a new **Map** tab. That framing is right — the Map tab is the *manager* for everything spatial/recorded (SLAM, sensor placement, scene memory in future specs). Videos are inputs to the map, so they live in the same view. v1 ships the Map tab with: real video upload/library/search on the left sidebar, a **placeholder canvas** where the map will eventually render. SLAM and sensor placement stay in Spec 2.

### Tab system

```diff
- type AppView = "dashboard" | "loop" | "chat";
+ type AppView = "dashboard" | "loop" | "chat" | "map";
```

`App.tsx` adds a fourth `view-tab` button (map-pin SVG) between Loop and Chat. No other tab-switching changes.

### Map tab layout

```
+-----------------------------------------------------------+
|  Bubbaloop    [Dash]  [Loop]  [Map *]  [Chat]   conn ●   |
+-----------------------------------------------------------+
|  ┌── Sidebar (~360px) ─────┬── Map canvas ───────────┐   |
|  │                          │                          │   |
|  │  [+ Upload video]        │                          │   |
|  │                          │                          │   |
|  │  [🔍 Search frames___]   │   (placeholder for       │   |
|  │                          │    kornia-slam map —     │   |
|  │  Library                 │    Spec 2)               │   |
|  │  ▸ entrance_2026-05.mp4  │                          │   |
|  │    ●●●●○○ indexing 67%   │                          │   |
|  │  ▸ patio.mp4             │                          │   |
|  │    ✓ done · 1,432 frames │                          │   |
|  │  ▸ kitchen.mp4           │                          │   |
|  │    ✗ failed · retry?     │                          │   |
|  │                          │                          │   |
|  │  Search results ───      │                          │   |
|  │  [thumb][thumb][thumb]   │                          │   |
|  │  [thumb][thumb][thumb]   │                          │   |
|  └──────────────────────────┴──────────────────────────┘   |
+-----------------------------------------------------------+
```

Clicking a video swaps the canvas for `VideoDetail` (frame scrubber + per-video search) until dismissed.

### New components

| File | Purpose | LoC est. |
|---|---|---|
| `frontend/src/components/MapTab.tsx` | Container: sidebar + canvas; manages selected-video state | ~150 |
| `frontend/src/components/VideoUploader.tsx` | Drag-drop / file-picker → chunked PUT loop with progress, retry, cancel | ~250 |
| `frontend/src/components/VideoLibrary.tsx` | `GET /api/videos`, live job status via Zenoh sub on `bubbaloop-dash/jobs/<id>/progress` | ~180 |
| `frontend/src/components/VideoDetail.tsx` | Frame scrubber + per-video search + delete confirm | ~200 |
| `frontend/src/components/FrameSearch.tsx` | Text input → `POST /api/search` → render FrameGrid | ~120 |
| `frontend/src/components/FrameGrid.tsx` | Reusable thumbnail grid | ~80 |
| `frontend/src/components/MapCanvas.tsx` | Placeholder + `<canvas>` Spec 2 hooks into | ~40 |
| `frontend/src/components/Login.tsx` | Paste-bearer-token form | ~80 |
| `frontend/src/contexts/AuthContext.tsx` | Token state + `useAuth()` hook | ~50 |
| `frontend/src/lib/api.ts` | Typed REST client; auto-attach `Authorization: Bearer ${token}` | ~150 |

### Auth integration

1. `main.tsx` wraps `<App />` in `<AuthProvider>`. No token in `localStorage` → render `<Login />`. Token present → render `<App />`.
2. `lib/zenoh.ts` `useZenohSession()` includes the token as a query param on the WSS connection to `/ws/zenoh`.
3. `lib/api.ts` reads from `AuthContext` and attaches to every `fetch()`.

### Zenoh client refactor

- WSS endpoint path: `/zenoh` → `/ws/zenoh`.
- Token attached: `?token=${token}`.
- Backend's multiplexed proxy speaks **the same wire protocol** as `zenoh-bridge-remote-api`. The existing `@eclipse-zenoh/zenoh-ts` library on the frontend doesn't need to change.
- Discovery wildcard expands: `bubbaloop/global/**` → `(bubbaloop/global/** | bubbaloop-dash/**)` so the frontend sees both sensor data and backend job-progress events.

**Fallback**: if matching the zenoh-bridge-remote-api protocol turns out heavy, define a minimal `{subscribe, unsubscribe, sample}` JSON protocol (~100 LoC backend + ~100 LoC small frontend rewrite of `useZenohSubscription`).

### Upload UX

```
1. User drops file (or picks via dialog)
2. UI shows progress bar at 0%, status="uploading"
3. POST /api/videos → { video_id, chunk_size }
4. Loop: read chunk via FileReader, PUT to /api/videos/<id>/chunks/<n>
   - 3 in-flight concurrent PUTs (configurable)
   - On failure: exponential backoff (250ms/500ms/1s/2s/4s), max 5 retries
   - On user cancel: abort all, DELETE /api/videos/<id>
5. After last chunk: POST /api/videos/<id>/complete
6. UI flips to status="queued"; subscribes to bubbaloop-dash/jobs/<job_id>/progress
7. Progress updates the row in the library: uploading → queued → indexing → done|error
```

**Resumability**: on next mount, library calls `GET /api/videos/<id>` for any `uploading` videos to learn which chunks the server has, resumes the gaps.

### What stays unchanged

- `Dashboard.tsx`, `MeshView.tsx`, `ChatView.tsx`, all `Sortable*Card.tsx`, all `*View.tsx`s — preserved as-is.
- All hooks under `hooks/` — unchanged.
- All other contexts under `contexts/` — interfaces preserved.
- Schema decode pipeline (`lib/schemas.ts`, `SchemaRegistry`) — unchanged.

---

## Section 6 — Error handling, testing, observability, phasing

### Error handling

| Subsystem | Failure | Behavior |
|---|---|---|
| **Upload** | Chunk PUT fails (network) | Client retries: 250ms / 500ms / 1s / 2s / 4s, max 5. After max → surface error, manual retry |
| | Mid-upload disconnect | On reconnect, `GET /api/videos/<id>` returns received chunk bitmap; client resumes gaps |
| | Server disk full | 507; client banner |
| | Server killed mid-upload | SQLite stays `uploading`. Startup sweeper marks `abandoned` after 24h |
| | Size mismatch on `/complete` | 422; server deletes partial file |
| **Decode/index** | PyAV can't open mp4 | Job → `error`, message in SQLite row, JPG dir cleaned |
| | open_clip OOM | Retry once with `batch_size //= 2`; second OOM → `error` |
| | LanceDB write fails | Job → `error`; orphan JPGs cleaned by sweeper |
| | Process crash mid-job | Sweeper marks `indexing` jobs `interrupted`; user resumes from last committed frame or restarts |
| | Model download interrupted | Cache marked invalid; next job re-downloads |
| **Search** | Empty query | 400 |
| | LanceDB query > 5s | 504 with "narrow filter" hint |
| | Zero results | 200 with `results: []` (never 404) |
| **Frame fetch** | File missing on disk | 404 + trigger integrity sweep |
| **Zenoh proxy** | zenohd unreachable | Backend reconnect loop with backoff; `readyz` reports `not_ready`; browser banner after 30s of no samples |
| | Browser WS drops | Ref-counted upstream subs cleaned up; no leak |
| **Auth** | Bad/missing/revoked token | 401 + `WWW-Authenticate: Bearer`; token validity cached 60s |

### Integrity sweeper

Single background coroutine, runs every 1h (configurable `SWEEP_INTERVAL_S`):

1. LanceDB `videos` rows whose `source.mp4` missing → mark `error` (`missing_source`).
2. LanceDB `frames` rows whose `jpg_path` missing → delete row.
3. Orphan JPGs (no row referencing them) → delete after 7d grace period.
4. SQLite jobs in `indexing` with no progress update >10min → mark `interrupted`.
5. SQLite jobs in `uploading` older than 24h → mark `abandoned`.

All sweeper actions logged at INFO.

### Testing strategy

**Backend (pytest)**
- API integration tests via FastAPI `TestClient`. Each endpoint: happy path + 1-2 error paths. In-memory LanceDB, tmp filesystem.
- Unit tests for decoder, embedder, storage interface, job runner, sweeper, auth.
- End-to-end smoke (`@pytest.mark.e2e`, opt-in): real Docker, real LanceDB, real open_clip, upload a 5s fixture mp4, verify indexing completes, search returns known content.
- Zenoh proxy test: spawn in-process zenoh-python publisher, subscribe via WSS proxy, assert sample delivery + ref-counted multiplex correctness.

**Frontend (vitest)**
- Component-level: `VideoUploader` chunk loop with mock fetch + abort, `Login`, `AuthContext` persistence, `FrameSearch` rendering.
- Reuse patterns from `dashboard/src/components/__tests__/`.

**Explicit non-goals for v1 testing**
- No Playwright/Cypress E2E. Defer to Phase 1.5 if pain emerges.
- No load testing — single-tenant, concurrent users = 1.
- No chaos/fuzz testing of Zenoh proxy. Add when there's evidence of need.

### CI (GitHub Actions)

```yaml
jobs:
  backend:
    - uv pip install -e ".[dev]"
    - pytest backend/tests/ -m "not e2e"
    - ruff check
    - mypy --strict src/dash/
  frontend:
    - npm ci
    - npm test
    - npm run build
  docker:
    - docker build .   # PR: build-only; main: push to ghcr.io
```

### Observability (minimal v1)

- Structured JSON logs to stdout via `structlog`. Docker log driver picks up.
- Three levels: INFO (job lifecycle, search), WARN (transient errors, retries), ERROR (failures, integrity violations).
- One log per job state transition. One per search. One per upload completion.
- **Not in v1**: Prometheus metrics, OpenTelemetry tracing, Sentry. Add when there's pain to fix, not preemptively.

### Phasing

| Phase | Scope | Ship target |
|---|---|---|
| **1.0** | Repo scaffolding, Dockerfile, dev compose, CI, FastAPI skeleton (`/healthz`, `/readyz`), bearer auth, frontend lift + Login + AuthContext, Zenoh-WSS proxy with multiplex | Week 1 — "log in, see existing dashboard render through new backend" |
| **1.1** | Upload pipeline (`POST/PUT/DELETE /api/videos`), Storage interface + Filesystem impl, SQLite jobs table + sweeper, VideoUploader + VideoLibrary on Map tab | Week 2 — "upload an mp4, see it in the library; no indexing yet" |
| **1.2** | PyAV decoder, open_clip embedder, job runner with Zenoh progress, LanceDB writes | Week 3 — "indexing completes, frames in DB" |
| **1.3** | `POST /api/search` (text query), `GET /api/frames/...`, FrameSearch + FrameGrid | Week 4 — "type query → see results → preview frame" |
| **1.4** | Error UI states, resumable uploads, integrity sweeper, docs (README + deploy guide) | Week 5 — first GitHub release |
| **1.5** | Scene-detection chunking (PySceneDetect), image-similarity search, additional embed models (DINOv2, SigLIP) | Post-v1 |
| **2** | World-model integration, presigned-S3 storage backend, multi-tenant auth, GPU model-server. **Spec 2 (Map + sensor placement) lands.** | Later |

### Open questions / explicit deferrals

| Question | Deferred to | Why deferred |
|---|---|---|
| Production auth (OAuth/sessions/MFA) | Phase 2 | v1 is single-user |
| Multi-Jetson aggregation | Phase 2 | One fleet assumed |
| Object storage (S3/MinIO) | Phase 2 | Storage interface ready; FS enough |
| Scene-detection chunking | Phase 1.5 | API param reserved; impl later |
| Map canvas + SLAM | Spec 2 | Independent project |
| Sensor placement model | Spec 2 | Independent project |
| GPU model server | Phase 2+ | Only needed for heavier world models |
| Metrics + tracing | Phase 2 | Single-instance ops doesn't need them |
| Production rate limits | Phase 2 | Single-tenant has no contention |
| Webhooks for external integrations | Phase 2 | No external consumers in v1 |
| Retire bubbaloop's `dashboard/` + `--features dashboard` | Separate future spec | User asked to keep both running for now |

---

## Appendix A — Research synthesis (industry patterns)

The Section 4 design was validated by three parallel research passes against comparable platforms in adjacent domains. Findings summarized below; full per-platform notes in the research agent transcripts (referenced from the brainstorm session).

### A.1 Robotics data platforms (Foxglove, Rerun, Formant, Freedom Robotics, Roboto)

**Patterns we adopted:**
- Blob storage decoupled from app server (Storage interface).
- Two-step register → transfer pattern (`POST /videos` → chunks → `/complete`).
- Async post-upload processing with explicit job lifecycle.
- Single-origin browser → backend → edge proxy (validated against Foxglove's Remote Access Gateway).

**Patterns we considered and deferred:**
- Customer-owned S3/GCS buckets (Foxglove "Primary Sites") — too much complexity for v1; Storage interface preserves the option.
- Device tokens vs user tokens — schema accommodates, only `admin` populated in v1.

### A.2 CV / annotation platforms (Voxel51 FiftyOne, Encord, Roboflow, V7 Darwin, Labelbox, Supervisely)

**Patterns we adopted:**
- `Authorization: Bearer <token>` (dominant; Encord's SSH-signing is the outlier we reject).
- V7's 4-step upload pattern (register → sign → PUT → confirm), simplified to 3 (register → chunked PUTs → complete) since we don't use presigned URLs in v1.
- FiftyOne's "brain_key" pattern (named persistent indexes) — partially adopted via `embed_model` + `embed_dim` columns; full multi-index per dataset deferred.

**Patterns we considered and rejected:**
- Labelbox's GraphQL-first / API-deprecation-friendly strategy — explicitly rejected (we don't have a SaaS moat to defend).
- Roboflow's "videos decompose into images" model — wrong for our use case (we need video-as-first-class).
- Supervisely's hash-based dedup — interesting but not v1 need.

### A.3 Video search SaaS (Twelve Labs, Marqo, Mixpeek, Pinecone, Vespa, Weaviate, Qdrant)

**Patterns we adopted:**
- Chunk/segment as atomic search unit (per-frame row in `frames` table).
- Float `score 0..1` in results (Twelve Labs' ordinal-rank-only is the outlier).
- Include `start_sec` / `end_sec` / `thumbnail_url` in search response (Twelve Labs, Marqo).
- `segment_mode` parameter reserved at upload time (Mixpeek differentiator).

**Patterns we considered and rejected:**
- Twelve Labs' multipart/form-data for search queries — JSON is more composable.
- URL-pull-only upload (Marqo, Mixpeek, Labelbox) — iPhone uploads can't be served at a public URL.
- Model lock-in at index creation time — we control the pipeline; `embed_model` column lets us re-index.

---

## Appendix B — Glossary

- **bubbaloop** — the Rust edge runtime (existing, unchanged by this spec).
- **bubbaloop-dash** — this project. Python backend + React frontend.
- **daemon** — the bubbaloop edge process, runs on user's Jetson.
- **dash** — short-form colloquial alias for bubbaloop-dash.
- **Map tab** — the new dashboard tab introduced in this spec; manager for video library + (later) SLAM map.
- **Spec 1** — this document. Video upload + indexing pipeline.
- **Spec 2** — future, Map canvas + sensor placement (separate document).
- **Spec 3** — future, remote-hosting hardening (separate document).
- **Phase 1.0–1.4** — the five shippable milestones inside this spec.
- **Phase 1.5** — post-v1 enhancements: scene detection, image-similarity search, more embed models.
- **Phase 2+** — world-model integration, S3 storage, multi-tenant, GPU model server, Spec 2 landing.
