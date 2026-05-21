# bubbaloop-dash Phase 1.1 — Upload + Library Design

**Status:** Draft (brainstorm complete, awaiting user review)
**Date:** 2026-05-21
**Owner:** Edgar Riba
**Builds on:** [Phase 1.0 design](2026-05-20-bubbaloop-dash-design.md) Section 4 (phasing table) + `2026-05-20-bubbaloop-dash-phase-1.0-skeleton.md` plan
**Predecessor:** Phase 1.0 (skeleton) — shipped at `kornia/bubbaloop-dash@v0.1.0-alpha`
**Successor:** Phase 1.2 (decode + index)

---

## Executive summary

Phase 1.1 makes the dashboard *useful for the first time*: users can upload mp4 clips from their browser into the bubbaloop-dash backend, see them appear in a library, and delete them. Nothing decodes the videos yet — they sit on disk waiting for Phase 1.2's indexer. The deliverable is end-to-end: drag a file onto the Map tab → see it appear in the sidebar library → server stores it durably → integrity sweeper protects against half-finished uploads.

Phase 1.1 introduces the **Map tab** (placeholder canvas with the video library in the sidebar), the **filesystem-backed storage layer** (with an abstraction that lets Phase 2+ swap to S3 without API changes), and the **SQLite jobs/videos schema** that Phase 1.2's indexer will build on.

**Out of scope (explicit):** no cross-session resume (an interrupted upload after a browser refresh = delete-and-retry, not auto-recover), no multi-file batching, no decode/embed/index, no search, no map canvas, no thumbnails.

---

## Section 1 — Scope + phase boundary

### Phase 1.1 owns

| Subsystem | Concrete deliverable |
|---|---|
| **REST API** | `POST /api/videos`, `PUT /api/videos/{id}/chunks/{n}`, `POST /api/videos/{id}/complete`, `GET /api/videos`, `GET /api/videos/{id}`, `DELETE /api/videos/{id}`, `GET /api/jobs`, `GET /api/jobs/{id}`, `DELETE /api/jobs/{id}` |
| **Storage** | `Storage` Protocol in `dash.storage.base` + `FilesystemStorage` impl in `dash.storage.filesystem`. Pure file ops, no S3 yet. |
| **Persistence** | SQLite WAL at `${DATA_DIR}/state.sqlite`. Schema: `videos`, `upload_progress`, `jobs` tables. Connection opened in lifespan, closed on shutdown. |
| **Sweeper** | Background `asyncio` task, runs every `SWEEP_INTERVAL_S` (default 3600s). Marks `uploading` videos with no chunk in 24h as `abandoned`; deletes orphan source files. |
| **Map tab** | Add `"map"` to `AppView`, render `MapTab.tsx` (sidebar + canvas placeholder). `MapCanvas.tsx` is a "SLAM coming soon" div. |
| **Upload UI** | `VideoUploader.tsx` (drag-or-pick, single file at a time, in-session retry, cancel). |
| **Library UI** | `VideoLibrary.tsx` (list of videos with status badges, progress bar for in-progress, delete confirm). |

### Phase 1.1 does NOT own (deferred to 1.2+)

| Out-of-scope item | Phase that owns it |
|---|---|
| PyAV decoder | Phase 1.2 |
| open_clip embedder | Phase 1.2 |
| Background job runner (picks up `ready` videos, decodes, embeds, writes LanceDB) | Phase 1.2 |
| Job states beyond `uploading / ready / error / abandoned` (`queued`, `indexing`, `done`, `interrupted`) | Phase 1.2 |
| LanceDB `videos` + `frames` writes (duration, codec, fps, embeddings) | Phase 1.2 |
| Video probe (ffprobe / PyAV.open) for duration/codec metadata in library | Phase 1.2 |
| Frame thumbnails in the library | Phase 1.2 |
| Search UI (FrameSearch, FrameGrid) | Phase 1.3 |
| Map canvas (kornia-slam, sensor placement) | Spec 2 |
| Cross-session upload resume (browser refresh recovery) | Phase 1.4 polish, possibly never (delete-and-retry is fine) |
| Multi-file batch upload | Phase 1.4 polish, possibly never |
| Image-similarity search input (jpg_b64 / video_id+frame_idx) | Phase 1.5 |

### Job state machine (Phase 1.1 only)

```
            ┌───────────────┐
   POST     │               │   POST /complete OK
  /videos ──▶  uploading    │─────────────────────▶  ready
            │               │
            └─┬───────────┬─┘
              │           │   POST /complete fails (size mismatch)
              │           │   OR PUT chunk fails (disk full)
              │           └────────────────────────▶  error
              │
              │   sweeper: last_chunk_at > 24h ago
              └────────────────────────────────────▶  abandoned

   DELETE /api/videos/{id} from any state → row + files removed
```

Phase 1.2 will add: `ready → queued → indexing → done | error | interrupted`.

### Map tab placement

- `AppView` becomes: `"dashboard" | "loop" | "map" | "chat"` (insert between Loop and Chat).
- Tab icon: map-pin SVG (stroke-only, matches existing tab style).
- Layout: 360px sidebar (Library + Uploader) + flex-1 canvas (placeholder).

---

## Section 2 — Storage layer + SQLite schema

### Storage abstraction

```python
# backend/src/dash/storage/base.py
from typing import Protocol

class Storage(Protocol):
    def write_chunk(self, path: str, offset: int, data: bytes) -> int:
        """Atomically write `data` at byte `offset` of `path`. Returns bytes written.
        Creates the file (and parent dirs) if missing. Safe for concurrent writes to
        non-overlapping offsets within the same file."""
    def read(self, path: str) -> bytes: ...
    def url_for(self, path: str) -> str:
        """Returns a URL the browser can fetch the file from. For FilesystemStorage,
        an /api/blob/<path> route; for future S3Storage, a presigned GET URL."""
    def delete(self, path: str) -> None:
        """Idempotent — deleting a missing path is not an error."""
    def size(self, path: str) -> int:
        """Returns the current size in bytes of `path`. Raises FileNotFoundError if missing."""
    def exists(self, path: str) -> bool: ...
```

### FilesystemStorage

```python
# backend/src/dash/storage/filesystem.py
import os
from pathlib import Path

class FilesystemStorage:
    def __init__(self, root: Path) -> None:
        self._root = root.resolve()

    def _abs(self, path: str) -> Path:
        # Path normalization + traversal protection.
        candidate = (self._root / path).resolve()
        if not candidate.is_relative_to(self._root):
            raise ValueError(f"path escapes storage root: {path}")
        return candidate

    def write_chunk(self, path: str, offset: int, data: bytes) -> int:
        abs_path = self._abs(path)
        abs_path.parent.mkdir(parents=True, exist_ok=True)
        fd = os.open(abs_path, os.O_WRONLY | os.O_CREAT, 0o644)
        try:
            return os.pwrite(fd, data, offset)
        finally:
            os.close(fd)

    # read / url_for / delete / size / exists trivial.
```

`os.pwrite` is atomic per-call within the kernel for non-overlapping byte ranges — two concurrent `write_chunk` calls to disjoint offsets of the same file are safe without explicit locking.

`url_for` returns `/api/blob/<path>`. A new tiny route serves blobs from the filesystem (auth-protected). For Phase 1.1 this is mainly used for the future `<img>` thumbnail tag in 1.2+; in 1.1 the route exists but no UI consumes it.

### Storage layout

```
${DATA_DIR}/
├── state.sqlite                 # SQLite WAL
├── state.sqlite-wal
├── state.sqlite-shm
├── .token                       # Phase 1.0
└── videos/
    └── <video_id_uuid>/
        └── source.<ext>         # original bytes; ext sanitized from upload filename
```

**Extension sanitization:** uppercase the filename, take substring after the last `.`, lowercase, allow only `[a-z0-9]{1,8}`. Default to `bin` if invalid. We don't care about real format validation — PyAV in Phase 1.2 reads the container from content, not extension.

### SQLite schema

```sql
-- backend/src/dash/db/schema.sql

CREATE TABLE IF NOT EXISTS videos (
    id              TEXT PRIMARY KEY,            -- uuid4 hex
    filename        TEXT NOT NULL,                -- user-displayable original
    content_type    TEXT NOT NULL,                -- 'video/mp4', etc.
    ext             TEXT NOT NULL,                -- sanitized extension used on disk
    size_bytes      INTEGER NOT NULL,             -- total size declared at POST /api/videos
    chunk_size      INTEGER NOT NULL,             -- server-decided, e.g., 8 MB
    total_chunks    INTEGER NOT NULL,             -- ceil(size_bytes / chunk_size)
    status          TEXT NOT NULL,                -- 'uploading' | 'ready' | 'error' | 'abandoned'
    error           TEXT,                         -- nullable; reason if status='error'
    created_at      INTEGER NOT NULL,             -- epoch ms
    updated_at      INTEGER NOT NULL,             -- epoch ms (last status transition or chunk)
    completed_at    INTEGER                       -- epoch ms; set when status → 'ready'
);

CREATE TABLE IF NOT EXISTS upload_progress (
    video_id        TEXT PRIMARY KEY REFERENCES videos(id) ON DELETE CASCADE,
    received_chunks BLOB NOT NULL,                -- packed bitmap, ceil(total_chunks/8) bytes
    received_bytes  INTEGER NOT NULL DEFAULT 0,   -- sum of declared chunk sizes received
    last_chunk_at   INTEGER NOT NULL              -- epoch ms; sweeper uses this for 'abandoned'
);

CREATE TABLE IF NOT EXISTS jobs (
    id              TEXT PRIMARY KEY,             -- uuid4 hex
    video_id        TEXT NOT NULL REFERENCES videos(id) ON DELETE CASCADE,
    kind            TEXT NOT NULL,                -- 'upload' (1.1); 'indexing' (1.2)
    status          TEXT NOT NULL,                -- mirrors videos.status for kind='upload'
    progress_pct    INTEGER NOT NULL DEFAULT 0,
    error           TEXT,
    started_at      INTEGER NOT NULL,
    finished_at     INTEGER                       -- nullable until terminal state
);

CREATE INDEX IF NOT EXISTS idx_videos_status ON videos(status);
CREATE INDEX IF NOT EXISTS idx_videos_updated ON videos(updated_at DESC);
CREATE INDEX IF NOT EXISTS idx_jobs_video ON jobs(video_id);
CREATE INDEX IF NOT EXISTS idx_jobs_status ON jobs(status);
```

### Chunk bitmap encoding

- **On disk** (BLOB column): packed bits, `ceil(total_chunks / 8)` bytes. Bit `i` set = chunk `i` received. For a 3 GB / 8 MB-chunk video → 375 chunks → 47 bytes.
- **NOT exposed in API** (Phase 1.1): client doesn't get the bitmap because we're not supporting cross-session resume.
- **Used server-side** to validate completeness at `POST /complete` — all bits must be set. Catches "client claimed done but missed a middle chunk" bugs.

### Cross-session resume — explicit non-goal

The browser's `File` object does not survive page reload. To "resume" after reload we'd need the user to re-pick the same file, and the server to validate it's the same file (size + content hash prefix). For Phase 1.1 we don't do this — an interrupted upload becomes an `abandoned` video after 24h; user deletes and re-uploads. **This may never become a requirement.** Most real-world uploads finish in one session; the sweeper handles the cleanup.

### Database connection

```python
# backend/src/dash/db/sqlite.py
class Db:
    def __init__(self, conn: sqlite3.Connection): self._conn = conn

    @classmethod
    def open(cls, path: Path) -> "Db":
        conn = sqlite3.connect(path, check_same_thread=False, isolation_level=None)
        conn.execute("PRAGMA journal_mode = WAL")
        conn.execute("PRAGMA foreign_keys = ON")
        conn.execute("PRAGMA busy_timeout = 5000")
        conn.executescript(SCHEMA_SQL)
        return cls(conn)

    def create_video(self, ...) -> str: ...
    def record_chunk(self, video_id: str, chunk_idx: int, bytes_written: int) -> None: ...
    def mark_ready(self, video_id: str) -> None: ...
    def mark_error(self, video_id: str, reason: str) -> None: ...
    def mark_abandoned(self, video_id: str) -> None: ...
    def list_videos(self, status_filter: str | None) -> list[VideoSummary]: ...
    def get_video(self, video_id: str) -> VideoDetail | None: ...
    def get_upload_progress(self, video_id: str) -> UploadProgress | None: ...
    def delete_video(self, video_id: str) -> None: ...
    def list_jobs(self, status_filter: str | None) -> list[JobSummary]: ...
    def get_job(self, job_id: str) -> JobDetail | None: ...
```

Connection opened in `factory.py` lifespan, stamped on `app.state.db`. Single connection per app process; SQLite serializes writes via WAL anyway, and we don't have enough write traffic to warrant a pool.

`check_same_thread=False` because FastAPI handlers run on the same event loop but may dispatch DB operations from threadpool executors. `isolation_level=None` (autocommit); explicit transactions in `Db` methods that need atomicity.

---

## Section 3 — REST handlers + sweeper

### Endpoint contracts

#### `POST /api/videos`
**Body:**
```json
{
  "filename": "entrance_2026-05-21.mp4",
  "size_bytes": 3221225472,
  "content_type": "video/mp4"
}
```
**Response 200:**
```json
{
  "video_id": "1a2b3c4d...",
  "chunk_size": 8388608,
  "total_chunks": 384
}
```
**Behavior:** validate `size_bytes` ≤ `MAX_UPLOAD_BYTES` (default 16 GB, env override). Validate `content_type` starts with `video/`. Generate uuid. Sanitize extension from filename. Insert `videos` row with `status='uploading'`. Insert `upload_progress` row with empty bitmap. Insert `jobs` row with `kind='upload'`, `status='uploading'`. Allocate `${DATA_DIR}/videos/<id>/` directory. Returns server-decided `chunk_size` (8 MB).

#### `PUT /api/videos/{id}/chunks/{n}`
**Headers:** `Content-Length: <bytes>`
**Body:** raw binary chunk bytes
**Response 204:** no body
**Behavior:** lookup video; if status != `uploading`, return 409. Validate `n` < `total_chunks`. Validate chunk size:
- if `n < total_chunks - 1`: must equal `chunk_size`
- if `n == total_chunks - 1`: must equal `size_bytes - n*chunk_size` (the remainder)

Write to `storage.write_chunk(f"videos/{id}/source.{ext}", offset=n*chunk_size, data=body)`. Update `upload_progress`: set bit `n` in bitmap, increment `received_bytes` (only if bit was previously unset — idempotent re-uploads of same chunk don't double-count). Update `last_chunk_at`. Update `jobs.progress_pct = floor(100 * popcount(bitmap) / total_chunks)`.

#### `POST /api/videos/{id}/complete`
**Body:** `{}` (reserved for future `sha256` integrity field)
**Response 200:**
```json
{
  "video_id": "1a2b3c4d...",
  "status": "ready"
}
```
**Behavior:** lookup video. Read `upload_progress`. If any bit unset → return 422 with detail `"missing_chunks: [3, 17, 22]"` (first 10). If `storage.size(path) != size_bytes` → return 422 `"size_mismatch"`, mark video `error`, delete partial file. Otherwise: set `videos.status='ready'`, `completed_at=now`, `jobs.status='ready'`, `jobs.finished_at=now`, `jobs.progress_pct=100`.

#### `GET /api/videos`
**Query:** `?status=ready,uploading&limit=100&cursor=<opaque>`
**Response 200:**
```json
{
  "items": [
    {
      "id": "1a2b...",
      "filename": "entrance.mp4",
      "size_bytes": 3221225472,
      "status": "ready",
      "created_at": 1747800000000,
      "updated_at": 1747800120000,
      "completed_at": 1747800120000,
      "progress_pct": 100
    }
  ],
  "next_cursor": null
}
```
**Behavior:** ORDER BY `updated_at DESC`, status-filter optional, default limit 100, cursor-based pagination (cursor = base64-encoded `updated_at`+`id` tuple). Phase 1.1 has no real pagination need (assumed dozens of videos max) but the shape is stable.

#### `GET /api/videos/{id}`
**Response 200:** as above row, plus:
```json
{
  "error": null,
  "_links": {
    "blob": "/api/blob/videos/1a2b.../source.mp4",
    "delete": "/api/videos/1a2b..."
  }
}
```
**404** if missing.

#### `DELETE /api/videos/{id}`
**Response 204** on success.
**Behavior:** cascade. Delete `videos` row (FK CASCADE handles `upload_progress` and `jobs`). Delete `${DATA_DIR}/videos/<id>/` directory recursively. Idempotent (404 if already gone is fine, but we return 204 either way).

#### `GET /api/jobs`
**Query:** `?status=&kind=&limit=100`
**Response 200:**
```json
{
  "items": [
    {
      "id": "j1a2b...",
      "video_id": "1a2b...",
      "kind": "upload",
      "status": "ready",
      "progress_pct": 100,
      "started_at": 1747800000000,
      "finished_at": 1747800120000
    }
  ]
}
```

#### `GET /api/jobs/{id}`
Single job detail; same shape as a job item.

#### `DELETE /api/jobs/{id}`
**Phase 1.1 behavior:** for `kind='upload'`, equivalent to `DELETE /api/videos/{job.video_id}`. The upload IS the work. Phase 1.2 makes this more meaningful (cancel a running indexer).

#### `GET /api/blob/{path...}`
**Response:** `image/jpeg` or `video/mp4` bytes, streamed via `FileResponse`. Path traversal protected (uses `Storage.url_for` mapping). Phase 1.1 has no real consumer; Phase 1.2 uses for thumbnails.

### Auth

All endpoints `Depends(require_bearer_token)`. The blob route serves bytes; do not skip auth here.

### Sweeper

```python
# backend/src/dash/jobs/sweeper.py
async def sweeper_loop(db: Db, storage: Storage, interval_s: int, abandoned_after_s: int = 86400) -> None:
    while True:
        await asyncio.sleep(interval_s)
        try:
            run_sweep(db, storage, abandoned_after_s=abandoned_after_s)
        except Exception:
            _log.exception("sweeper.failed")

def run_sweep(db: Db, storage: Storage, abandoned_after_s: int) -> SweepReport:
    now = epoch_ms()
    cutoff = now - abandoned_after_s * 1000
    # 1. Mark stale uploads abandoned.
    stale = db.find_stale_uploads(cutoff)        # status='uploading' AND last_chunk_at < cutoff
    for v in stale:
        db.mark_abandoned(v.id)
        _log.warning("sweep.abandoned", video_id=v.id, last_chunk_at=v.last_chunk_at)

    # 2. Delete orphan source files (no matching videos row).
    orphans = list_orphan_files(db, storage)
    for path in orphans:
        storage.delete(path)
        _log.info("sweep.orphan_deleted", path=path)

    return SweepReport(abandoned=len(stale), orphans_deleted=len(orphans))
```

Launched from `factory.py` lifespan as a tokio-style background task (`asyncio.create_task`) alongside the Zenoh session. Cancelled on shutdown. `run_sweep` is its own function so we can unit-test it without the loop.

---

## Section 4 — Frontend (Map tab, VideoUploader, VideoLibrary)

### App.tsx tab addition

```diff
- type AppView = "dashboard" | "loop" | "chat";
+ type AppView = "dashboard" | "loop" | "map" | "chat";
```

Three lines: add the map-pin SVG button between Loop and Chat; route `currentView === "map"` to `<MapTab />`; pass through `availableTopics` / `session` for future use (Spec 2's sensor placement).

### MapTab.tsx

```tsx
export function MapTab() {
  const [selectedVideoId, setSelectedVideoId] = useState<string | null>(null);
  return (
    <div className="map-tab">
      <aside className="map-sidebar">
        <VideoUploader onUploaded={(id) => setSelectedVideoId(id)} />
        <VideoLibrary
          onSelect={setSelectedVideoId}
          selectedId={selectedVideoId}
        />
      </aside>
      <main className="map-canvas">
        <MapCanvas />  {/* placeholder for Spec 2 */}
      </main>
    </div>
  );
}
```

Sidebar fixed-width 360px on desktop, full-width drawer on mobile (< 768px). `MapCanvas` is a 40-line placeholder with "SLAM coming soon" + an empty `<canvas>` element Spec 2 hooks into.

### VideoUploader.tsx

```
+-- VideoUploader ----------------------------------+
|                                                   |
|  ┌─────────────────────────────────────────────┐ |
|  │           Drop a video here or              │ |
|  │           [ Browse... ]                     │ |
|  └─────────────────────────────────────────────┘ |
|                                                   |
|  -- when uploading: --                            |
|  ┌─────────────────────────────────────────────┐ |
|  │ entrance.mp4 (3.2 GB)                       │ |
|  │ ██████████░░░░░░░░░░ 47%   12 MB/s          │ |
|  │ [ Cancel ]                                  │ |
|  └─────────────────────────────────────────────┘ |
+---------------------------------------------------+
```

**Behavior (single-file, in-session retry only):**

1. User drops or picks a file. `<input type="file" accept="video/*">`.
2. POST `/api/videos { filename, size_bytes, content_type }` → `{ video_id, chunk_size, total_chunks }`.
3. Loop: read chunks via `file.slice(offset, offset + chunk_size).arrayBuffer()`; PUT to `/api/videos/<id>/chunks/<n>`. Maintain a `chunkConcurrency = 3` semaphore (e.g., simple counter + queue).
4. Per-chunk: exponential backoff retry on network failure (250, 500, 1000, 2000, 4000 ms); max 5 attempts. On final failure, abort the whole upload (DELETE /api/videos/<id>) and surface error.
5. On user `[Cancel]`: AbortController on in-flight fetches; DELETE the video; reset UI.
6. After last chunk: POST `/api/videos/<id>/complete`. On 2xx, call `onUploaded(video_id)`; reset UI to drop-zone.
7. While uploading, the drop-zone is disabled (can't start a second concurrent upload).

**No cross-session recovery.** If the user closes the tab mid-upload, the partial mp4 sits server-side until the sweeper marks it `abandoned`. Library will show it as "uploading" with stale `last_chunk_at`; user deletes and re-uploads from a fresh session.

**Speed indicator:** rolling 5-second window average of `received_bytes / elapsed_ms`. Display as `MB/s`. Useful UX, no protocol cost.

**State (single component):**
```typescript
type UploaderState =
  | { kind: "idle" }
  | { kind: "starting"; file: File }
  | { kind: "uploading"; file: File; videoId: string; receivedBytes: number; totalBytes: number; speedMBps: number; abortController: AbortController }
  | { kind: "completing"; file: File; videoId: string }
  | { kind: "error"; reason: string };
```

### VideoLibrary.tsx

```
+-- VideoLibrary -----------------------------------+
| Library                                  [⟳]      |
|                                                   |
|  ┌─────────────────────────────────────────────┐ |
|  │ ▸ entrance_2026-05-21.mp4   3.2 GB          │ |
|  │   ✓ ready  ·  just now                      │ |
|  │                                       […]   │ |
|  └─────────────────────────────────────────────┘ |
|  ┌─────────────────────────────────────────────┐ |
|  │ ▸ patio.mp4                 142 MB          │ |
|  │   ██████████░░░░ uploading 47% · 8 MB/s     │ |
|  │                                       [×]   │ |
|  └─────────────────────────────────────────────┘ |
|  ┌─────────────────────────────────────────────┐ |
|  │ ▸ kitchen.mp4              1.8 GB           │ |
|  │   ⚠ interrupted · last chunk 18m ago        │ |
|  │                                  [Delete]   │ |
|  └─────────────────────────────────────────────┘ |
|  ┌─────────────────────────────────────────────┐ |
|  │ ▸ broken.mp4               220 MB           │ |
|  │   ✗ error: size mismatch          [Delete]  │ |
|  └─────────────────────────────────────────────┘ |
+---------------------------------------------------+
```

**Per-row visual logic:**

| `status` | Badge | Right-side action |
|---|---|---|
| `ready` | ✓ ready · *<relative_time>* | `[…]` menu → Delete |
| `uploading` (currently active in this tab) | progress bar + speed | `[×]` Cancel |
| `uploading` (no chunk in >10min, NOT currently this tab) | ⚠ interrupted · last chunk *<time>* ago | `[Delete]` |
| `abandoned` | abandoned · *<time>* ago | `[Delete]` |
| `error` | ✗ error: *<reason>* | `[Delete]` |

The "interrupted" badge is **derived in the frontend** from `updated_at` (server has no separate "interrupted" state; sweeper turns these into `abandoned` after 24h). Threshold: `now - updated_at > 10 * 60_000 && status == "uploading"`.

**Data fetch:**

- Initial mount: `GET /api/videos?limit=100`.
- Refresh: manual `[⟳]` button; auto-refresh every 5s while any video has `status='uploading'`; pause auto-refresh when none.
- After own VideoUploader completes: refresh immediately.
- After own delete: optimistic remove + refresh.

**No Zenoh subscription for library in 1.1.** The Phase 1.0 spec floats `bubbaloop-dash/jobs/<id>/progress` over Zenoh for job progress; that becomes useful in Phase 1.2 when the runner publishes real-time decode/embed progress. For uploads, the client knows its own progress directly (no network round-trip needed). Cross-tab live updates are a Phase 1.4 polish.

**Delete confirm:** `window.confirm()` for v1. Custom modal in Phase 1.4 if it gets noisy.

---

## Section 5 — Error handling

### Per-subsystem failure modes

| Subsystem | Failure | Behavior |
|---|---|---|
| **POST /videos** | `size_bytes > MAX_UPLOAD_BYTES` | 413 Payload Too Large + reason in body |
| | `content_type` not `video/*` | 415 Unsupported Media Type |
| | Disk full at directory creation | 507 Insufficient Storage |
| **PUT chunks/{n}** | Video not found | 404 |
| | Video status != `uploading` | 409 Conflict + body `{"detail": "status=ready, cannot accept chunks"}` |
| | Chunk index out of range | 422 Unprocessable Entity |
| | Wrong chunk size | 422 |
| | Storage write fails | 500 + error logged; client retries |
| | Duplicate chunk (bit already set) | 204 No Content (idempotent) |
| **POST /complete** | Missing chunks | 422 with `{"missing": [3, 17, 22]}` (first 10) |
| | Size mismatch | 422; mark video `error`; delete partial file |
| | Video already `ready` | 200 (idempotent — return same shape) |
| | Video `error` or `abandoned` | 409 + body explains terminal state |
| **DELETE /videos/{id}** | Not found | 204 (idempotent) |
| | Storage delete partially fails | 500; row stays; sweeper eventually cleans orphan files |
| **Sweeper** | DB error during sweep | log ERROR, continue (don't crash the loop) |
| | Storage error on orphan delete | log WARN, retry next sweep |
| **Frontend uploader** | Network failure on chunk | Exponential backoff (250/500/1000/2000/4000 ms), max 5 retries per chunk; on final failure abort+DELETE |
| | User clicks Cancel | AbortController fires; DELETE /api/videos/<id> |
| | Browser closes tab mid-upload | Server-side partial stays; sweeper marks `abandoned` after 24h |
| | POST /complete fails | Surface error in uploader UI; library shows the video as `error` |

### Logging policy

INFO: video created, upload completed, sweep run summary, video deleted.
WARN: 4xx responses (auth failures, bad chunk size, status conflict), sweeper marks-abandoned, sweeper deletes-orphan.
ERROR: 5xx responses (disk full, storage write fail, DB errors), unhandled exceptions.

Never log the raw bearer token, never log full file paths in 4xx responses to the client (server-side log is fine).

---

## Section 6 — Testing + phasing

### Backend (pytest)

**Unit tests:**
- `Db` methods: create_video, record_chunk (bitmap set semantics, idempotency), mark_ready / mark_error / mark_abandoned, list_videos / list_jobs, delete_video (cascade).
- `FilesystemStorage`: write_chunk (concurrent disjoint offsets), read, delete, size, exists, path-traversal protection.
- Sweeper `run_sweep`: stale-upload marking, orphan-file deletion. Use freezegun or a `clock` injection.
- Chunk bitmap helpers: pack/unpack, popcount, set/clear bit.

**API integration tests (FastAPI TestClient + tmp DATA_DIR):**
- Happy path: POST /videos → PUTs all chunks → POST /complete → GET /videos shows it → DELETE removes it.
- POST /videos: 413 oversize, 415 wrong content-type.
- PUT chunks: 404 unknown video, 409 wrong status, 422 out-of-range index, 422 wrong chunk size, 204 on duplicate chunk.
- POST /complete: 422 missing chunks (response includes `missing` field), 422 size mismatch (file deleted, status → error), 200 idempotent on already-ready.
- DELETE: 204 idempotent on missing.
- GET /jobs: returns the upload job.
- Sweeper: invoke `run_sweep` directly with monkeypatched clock; assert stale rows → abandoned, orphan files deleted.

**No E2E in Phase 1.1.** A real-browser smoke test of the uploader belongs in Phase 1.4 (when there's enough UI complexity to warrant Playwright).

### Frontend (vitest)

- `VideoUploader`: mock fetch + AbortController; assert chunk PUT loop sends correct ranges; assert error path triggers DELETE; assert Cancel button aborts. Use a small fixture `File` (e.g., 64 KB total, 8 KB chunks).
- `VideoLibrary`: mock `/api/videos` response; render rows; assert status-badge logic ("interrupted" derived from updated_at); assert delete confirm flow.
- `MapTab`: smoke test that both sub-components mount and don't crash.

### Phasing (within Phase 1.1)

| Sub-phase | Scope | Ship target |
|---|---|---|
| **1.1.a** | Storage interface + FilesystemStorage + DB schema + `Db` wrapper + sweeper module + unit tests | Day 1-2 |
| **1.1.b** | `POST /videos`, `PUT chunks`, `POST /complete`, `GET /videos`, `GET /videos/{id}`, `DELETE /videos/{id}` + integration tests | Day 3 |
| **1.1.c** | `GET /jobs`, `GET /jobs/{id}`, `DELETE /jobs/{id}`, `GET /blob/{path}` + tests | Day 3 |
| **1.1.d** | Sweeper wired into lifespan; integration test that startup spawns + shutdown cancels | Day 4 |
| **1.1.e** | Map tab in App.tsx + MapTab.tsx + MapCanvas.tsx placeholder | Day 4 |
| **1.1.f** | VideoUploader.tsx + component tests | Day 5 |
| **1.1.g** | VideoLibrary.tsx + component tests | Day 5 |
| **1.1.h** | Manual E2E (drop a real iPhone clip, verify it lands, library renders, delete works); tag `v0.1.1-alpha` | Day 5 |

5-day target; same TDD discipline as Phase 1.0 (failing test → impl → passing → commit per sub-task).

---

## Open questions / explicit deferrals

| Question | Deferred to | Why |
|---|---|---|
| Cross-session upload resume (file re-pick + content validation) | Phase 1.4 or never | Most uploads finish in one session; delete-and-retry is acceptable for v1 |
| Multi-file batch upload | Phase 1.4 or never | Single-file scales fine for self-hosted single-user; queue UI is real work |
| Video probe (duration / codec / resolution in library) | Phase 1.2 | Requires PyAV; lands when decoder lands |
| Frame thumbnails in library | Phase 1.2 | Requires decoder + storage for thumbnails |
| Custom modal for delete confirm | Phase 1.4 | `window.confirm()` is fine for v1 |
| Cross-tab live library updates (via Zenoh) | Phase 1.4 | Useful once multiple users / devices; not v1 |
| Pagination of /api/videos beyond cursor scaffolding | Phase 2 | Self-hosted single-user has < 100 videos |
| Object storage (S3/MinIO) backend | Phase 2 | Storage Protocol is ready; impl swap on demand |
| Resumable per-chunk SHA validation | Phase 2 | Detect corruption-in-transit; current size-check is sufficient |
| Rate limiting on uploads | Phase 2 | Single-user has no contention |

---

## Glossary updates (additive to Phase 1.0)

- **Map tab** — fourth top-level tab introduced in this phase. Container for video library (1.1), search (1.3), and eventual SLAM map (Spec 2).
- **VideoUploader** — single-file drag-or-pick component; in-session retry only.
- **VideoLibrary** — list of all videos with per-row status + actions.
- **Upload progress bitmap** — packed-bits BLOB in SQLite recording which chunks the server has received; used for `/complete` validation; not exposed in API.
- **Interrupted (UI-only)** — derived state shown in library when `status='uploading'` and `updated_at > 10min ago`. No backend column; sweeper turns these into `abandoned` after 24h.
- **Sweeper** — background asyncio task running every hour; marks stale uploads abandoned, deletes orphan source files.
- **Storage Protocol** — Python Protocol abstracting blob storage. v1.1 impl is FilesystemStorage; Phase 2+ adds S3Storage with the same interface.
