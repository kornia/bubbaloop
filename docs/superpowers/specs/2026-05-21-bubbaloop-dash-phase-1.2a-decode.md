# bubbaloop-dash Phase 1.2a — Decode + Job Runner

**Status:** Draft (brainstorm complete, awaiting user review)
**Date:** 2026-05-21
**Owner:** Edgar Riba
**Builds on:** [Phase 1.1 spec](2026-05-21-bubbaloop-dash-phase-1.1-upload.md) (upload pipeline) + [Phase 1.0 design](2026-05-20-bubbaloop-dash-design.md) (overall architecture, phasing table)
**Predecessor:** Phase 1.1 (upload + library) — shipped via [bubbaloop-dash#2](https://github.com/kornia/bubbaloop-dash/pull/2)
**Successor:** Phase 1.2b — open_clip embedding + LanceDB writes

---

## Executive summary

Phase 1.2a makes uploaded videos *useful for the first time*: after `POST /api/videos/{id}/complete`, the backend automatically extracts JPEG frames from the source video at a configurable sample rate and writes them to disk under `videos/<id>/streams/rgb/`. Nothing is embedded yet (Phase 1.2b adds open_clip + LanceDB), but the user sees their video transition from `uploading → queued → indexing → done` in the library, with progress, retry on interruption, and end-state visibility.

This split is deliberate: 1.2a introduces the job runner architecture and the heavy decoder dependency (`av` / PyAV) without also adding the much heavier `torch` + `open_clip_torch` + `lancedb` deps. Docker image grows from ~345 MB to ~440 MB. 1.2b will land the embedding deps as a separate step (~2 GB image).

**Scope summary:**
- Job runner: single asyncio task in lifespan, `claim → run → mark_indexed` loop, threadpool executor for the blocking decoder.
- PyAV decoder: opens source mp4, samples at SAMPLE_FPS, downscales, encodes JPEG to disk, reports progress.
- State machine: `ready → queued → indexing → done | error | interrupted`. `ready` is collapsed (Phase 1.1's `/complete` now writes `queued` directly).
- Zenoh progress: published on `bubbaloop-dash/jobs/<job_id>/progress`. Frontend polls (Zenoh-subscribe is Phase 1.4 polish).
- Library UI: new badges for the new states + a Retry button on `interrupted` rows.
- Sweeper extension: on server startup, all `indexing` videos get marked `interrupted` (no in-progress runner exists).

**Out of scope (deferred):**
- open_clip embedding (Phase 1.2b)
- LanceDB tables (Phase 1.2b)
- Search REST endpoint (Phase 1.3)
- GPU inference (Phase 1.2b option)
- Frontend Zenoh subscription (Phase 1.4 polish)
- Multi-stream extraction (depth, segmentation) — Phase 2+
- Keyframe-based seeking — Phase 1.4 polish
- Resumable indexing across crashes — Phase 2+
- Per-video sample_fps override — Phase 1.4
- Cancel-running-job endpoint — Phase 1.2b or 1.4

---

## Section 1 — Scope + phase boundary

### Phase 1.2a owns

| Subsystem | Concrete deliverable |
|---|---|
| **Job runner** | `dash.jobs.indexer.indexing_loop` — asyncio task spawned in lifespan, polls SQLite for queued jobs, dispatches to threadpool executor |
| **Decoder** | `dash.decode.pyav_decoder.PyAVDecoder` — synchronous class, opens mp4, samples + writes JPEGs |
| **Schema** | `ALTER TABLE videos ADD COLUMN indexed_frame_count INTEGER`; new index `idx_jobs_kind_status` |
| **Db methods** | `mark_queued`, `claim_next_indexing_job` (BEGIN IMMEDIATE), `update_indexing_progress`, `mark_indexed`, `mark_index_error`, `mark_interrupted`, `retry_interrupted`, `find_interrupted_indexing_at_startup` |
| **Sweeper extension** | `recover_interrupted_at_startup` — once at app start, marks all `status='indexing'` rows as `interrupted` |
| **REST changes** | `POST /api/videos/{id}/complete` now sets `status='queued'` (not `ready`); `GET /api/videos` summaries include `job_id`; new `POST /api/jobs/{id}/retry` |
| **Zenoh progress** | Publish `bubbaloop-dash/jobs/<id>/progress` JSON `{pct, stage, frame_idx, total_estimated}` every N frames |
| **Frontend** | `VideoLibrary` adds badges for `queued | indexing | done | interrupted`; `[Retry]` button on interrupted rows; auto-refresh extends to `uploading | queued | indexing` |
| **Settings** | `SAMPLE_FPS` (default 1.0), `JPG_QUALITY` (default 90), `JPG_MAX_DIM` (default 1280), `INDEXING_PROGRESS_INTERVAL_FRAMES` (default 10) |
| **New dep** | `av` (PyAV ≥13.0,<14.0) |

### Phase 1.2a does NOT own

| Item | Phase that owns it |
|---|---|
| `open_clip_torch`, `torch`, `lancedb` deps | Phase 1.2b |
| LanceDB `videos` + `frames` tables | Phase 1.2b |
| CLIP / DINOv2 / SigLIP embedding | Phase 1.2b |
| GPU / CUDA support | Phase 1.2b option |
| `POST /api/search` endpoint | Phase 1.3 |
| Frontend Zenoh subscription | Phase 1.4 polish |
| Multi-stream extraction (depth, segmentation) | Phase 2+ |
| Keyframe seeking optimization | Phase 1.4 polish |
| Resumable indexing across crashes | Phase 2+ |
| Per-upload SAMPLE_FPS override | Phase 1.4 |
| Cancel-running-indexer endpoint | Phase 1.2b or 1.4 |

### State machine

```
                           POST /complete (Phase 1.1, behavior changes in 1.2a)
              ┌───────────────────────────────────────────────┐
              │                                                ▼
       ┌──────────┐                              ┌──────────────────┐
       │uploading │ (Phase 1.1)                  │     queued       │
       └──────────┘                              └────────┬─────────┘
                                                  runner claims│
                                                              ▼
                                                  ┌──────────────────┐
                                  sweep@startup   │   indexing       │
              ┌───────────────────────────────────│                  │
              │                                   └────┬────┬────┬───┘
              │                                        │    │    │
              ▼                          decoder OK ───┘    │    │
       ┌───────────┐                                        │    │
       │interrupted│                  PyAV open fails ──────┘    │
       └─────┬─────┘                  disk full, etc.             │
             │ POST /api/jobs/{id}/retry                            │
             │ (deletes partial frames, re-queues)                  ▼
             └───────────────────────────────────┐         ┌──────────┐
                                                 ▼         │  error   │
                                          ┌──────────┐     │          │
                                          │   done   │     └──────────┘
                                          │ frames   │
                                          │ on disk  │
                                          └──────────┘

   DELETE /api/videos/{id} from any state → cascade row + dir removal
   "ready" is collapsed — Phase 1.0/1.1 spec used it as a terminal state;
   in 1.2a /complete writes "queued" directly. Migration handles old rows.
```

### Definitions of `done` (matters for 1.2b)

- In 1.2a: "all sampled frames decoded, JPEGs on disk under `videos/<id>/streams/rgb/`, `indexed_frame_count` recorded on the video row."
- In 1.2b: above PLUS "LanceDB `frames` rows written with embeddings for each JPEG."

Pre-1.2b `done` videos won't have embeddings. Phase 1.2b's startup migration either re-indexes them through the new pipeline or leaves them invisible to search. That decision sits in Phase 1.2b's spec; for 1.2a, all that matters is producing well-named JPEGs at deterministic paths.

### Settings additions

| Env var | Default | Purpose |
|---|---|---|
| `SAMPLE_FPS` | `1.0` | Frames per second extracted from source. `1.0` → one frame every second of video. |
| `JPG_QUALITY` | `90` | JPEG encoder quality (1-95). |
| `JPG_MAX_DIM` | `1280` | Longest-side cap (pixels). Larger frames downscale preserving aspect. |
| `INDEXING_PROGRESS_INTERVAL_FRAMES` | `10` | Publish Zenoh progress + bump SQLite `jobs.progress_pct` every N decoded frames. |

Existing 1.1 settings stay unchanged.

---

## Section 2 — Runner architecture + decoder

### Job runner

```python
# backend/src/dash/jobs/indexer.py

@dataclass(frozen=True)
class IndexerJob:
    job_id: str
    video_id: str
    ext: str          # used by Indexer to construct "videos/<video_id>/source.<ext>"

class Indexer:
    """Synchronous frame extractor. Run in threadpool executor, not on event loop."""
    def __init__(self, storage: Storage, settings: Settings) -> None: ...

    def run(self, job: IndexerJob, progress_cb: Callable[[int, int], None]) -> int:
        """Open source via storage, sample frames, write JPEGs. Returns frame count.
        progress_cb(frame_idx, total_estimated) every INDEXING_PROGRESS_INTERVAL_FRAMES.
        Source path is "videos/<video_id>/source.<ext>"; output dir is
        "videos/<video_id>/streams/rgb/"."""

async def indexing_loop(
    db: Db,
    storage: Storage,
    settings: Settings,
    zsess: ZenohSession,
    poll_interval_s: float = 2.0,
) -> None:
    """Long-running coroutine. Cancelled cleanly on shutdown."""
    indexer = Indexer(storage, settings)
    while True:
        try:
            await asyncio.sleep(poll_interval_s)
        except asyncio.CancelledError:
            _log.info("indexer.cancelled")
            raise
        try:
            job = db.claim_next_indexing_job(now_ms=_now_ms())
        except Exception:
            _log.exception("indexer.claim_failed")
            continue
        if job is None:
            continue
        await _run_one(db, indexer, job, zsess)


async def _run_one(db: Db, indexer: Indexer, job: IndexerJob, zsess: ZenohSession) -> None:
    loop = asyncio.get_running_loop()
    def progress_cb(frame_idx: int, total: int) -> None:
        pct = int(100 * frame_idx / max(total, 1))
        db.update_indexing_progress(job.job_id, pct=pct, now_ms=_now_ms())
        publish_progress(zsess, job.job_id, pct, frame_idx, total)
    try:
        frame_count = await loop.run_in_executor(None, indexer.run, job, progress_cb)
        db.mark_indexed(job.video_id, indexed_frame_count=frame_count, now_ms=_now_ms())
        _log.info("indexer.done", job_id=job.job_id, frame_count=frame_count)
    except Exception as exc:
        db.mark_index_error(job.video_id, reason=str(exc), now_ms=_now_ms())
        _log.exception("indexer.failed", job_id=job.job_id)
```

The poll interval is 2 s — fast enough that newly queued videos start indexing within a couple seconds, slow enough to avoid SQLite hammering. The `claim_next_indexing_job` is atomic via `BEGIN IMMEDIATE`, so it's safe to bump `JOB_CONCURRENCY` later (each worker holds its own claim).

`loop.run_in_executor(None, indexer.run, ...)` runs the blocking decoder on the default ThreadPoolExecutor — the GIL is released by libavcodec's C functions, so HTTP handlers and the sweeper stay responsive during decode.

### PyAV decoder

```python
# backend/src/dash/decode/pyav_decoder.py

class PyAVDecoder:
    def __init__(self, settings: Settings) -> None:
        self._sample_fps = settings.sample_fps
        self._jpg_quality = settings.jpg_quality
        self._jpg_max_dim = settings.jpg_max_dim

    def extract(
        self,
        source_path: Path,
        out_dir: Path,
        progress_cb: Callable[[int, int], None],
    ) -> int:
        """Decode source, sample at sample_fps, write JPEGs to out_dir.
        Returns total frames written."""
        out_dir.mkdir(parents=True, exist_ok=True)
        container = av.open(str(source_path))
        try:
            stream = container.streams.video[0]
            source_fps = float(stream.average_rate)
            duration_s = float(container.duration / av.time_base) if container.duration else None
            # Decode every frame; sample every N where N = round(source_fps / sample_fps).
            sample_interval = max(1, round(source_fps / self._sample_fps))
            total_est = int(duration_s * self._sample_fps) if duration_s else 0

            frame_idx = 0
            decoded_count = 0
            for packet in container.demux(stream):
                for frame in packet.decode():
                    if decoded_count % sample_interval == 0:
                        img = frame.to_ndarray(format='rgb24')
                        img = _maybe_downscale(img, self._jpg_max_dim)
                        _write_jpeg(out_dir / f"{frame_idx:06d}.jpg", img, self._jpg_quality)
                        frame_idx += 1
                        if frame_idx % INDEXING_PROGRESS_INTERVAL_FRAMES == 0:
                            progress_cb(frame_idx, total_est)
                    decoded_count += 1
        finally:
            container.close()
        progress_cb(frame_idx, frame_idx)  # final 100%
        return frame_idx


def _maybe_downscale(img: np.ndarray, max_dim: int) -> np.ndarray:
    """Resize so the longest dimension ≤ max_dim, preserving aspect."""
    h, w = img.shape[:2]
    if max(h, w) <= max_dim:
        return img
    if w >= h:
        new_w = max_dim
        new_h = round(h * (max_dim / w))
    else:
        new_h = max_dim
        new_w = round(w * (max_dim / h))
    # PIL.Image.fromarray + .resize + .__array__ — Pillow is a transitive PyAV dep,
    # but if not we add it explicitly.
    from PIL import Image
    return np.asarray(Image.fromarray(img).resize((new_w, new_h), Image.BILINEAR))


def _write_jpeg(path: Path, img: np.ndarray, quality: int) -> None:
    from PIL import Image
    Image.fromarray(img).save(path, format='JPEG', quality=quality)
```

**Sampling notes:**
- We DECODE every frame but only WRITE every `sample_interval`-th frame. PyAV/libavcodec decodes 1080p at 100-200 fps on modern x86, so a 1-hour video takes ~30 seconds of pure decode time on CPU. Acceptable.
- An optimization for Phase 1.4 polish: use `container.seek(target_time)` to skip directly to keyframes, then decode forward to the target frame.
- For variable-fps sources (some phones), `stream.average_rate` is approximate; sampling drifts slightly. Phase 1.4 polish: switch to timestamp-based sampling (`frame.time` vs `next_sample_time`).

**Frame paths:** zero-padded 6-digit ordinal (`000001.jpg`, `000002.jpg`, ...) for natural ls/sort.

**Pillow dep:** PyAV does NOT transitively install Pillow on all platforms. We pin `pillow>=10.0,<12.0` in `pyproject.toml` explicitly. Pillow is small (~3 MB).

---

## Section 3 — Db extensions + state transitions

### Schema migration (additive, safe on existing DBs)

```sql
-- Append to backend/src/dash/db/schema.sql

ALTER TABLE videos ADD COLUMN indexed_frame_count INTEGER;
CREATE INDEX IF NOT EXISTS idx_jobs_kind_status ON jobs(kind, status);
```

`ALTER TABLE ADD COLUMN` is safe on SQLite (no table rewrite). Existing rows get `NULL` for `indexed_frame_count`.

The `idx_jobs_kind_status` index accelerates the runner's poll-for-queued query.

### Migration of pre-1.2a `ready` rows

After running the schema script in `Db.open()`, do a one-time migration:

```python
self._conn.execute(
    "UPDATE videos SET status='queued', updated_at=? WHERE status='ready'",
    (now_ms,),
)
```

Phase 1.1 left finished uploads at `status='ready'`. 1.2a collapses that state — any 1.1-era video is auto-enqueued on first 1.2a startup. Fresh DBs never have `ready` rows.

### New Db methods (all transactional)

```python
# All wrap their UPDATEs in BEGIN/COMMIT/ROLLBACK like create_video does.

def mark_queued(self, video_id: str, now_ms: int) -> None:
    """Phase 1.1's /complete handler now calls THIS (not mark_ready)."""

def claim_next_indexing_job(self, now_ms: int) -> IndexerJob | None:
    """BEGIN IMMEDIATE; SELECT next queued upload job; UPDATE its video + job
    to status='indexing'; COMMIT. Returns IndexerJob or None.
    Single transaction prevents double-claim under concurrency."""

def update_indexing_progress(self, job_id: str, pct: int, now_ms: int) -> None:
    """Bump jobs.progress_pct + videos.updated_at. Called from progress_cb."""

def mark_indexed(self, video_id: str, indexed_frame_count: int, now_ms: int) -> None:
    """indexing → done. Sets indexed_frame_count, jobs.progress_pct=100, finished_at."""

def mark_index_error(self, video_id: str, reason: str, now_ms: int) -> None:
    """indexing → error with reason on jobs.error."""

def mark_interrupted(self, video_id: str, now_ms: int) -> None:
    """indexing → interrupted. Sweeper calls on startup; runner does NOT call this."""

def retry_interrupted(self, video_id: str, now_ms: int) -> None:
    """interrupted → queued. Caller deletes partial frames before calling."""

def find_interrupted_indexing_at_startup(self) -> list[VideoRow]:
    """All status='indexing' videos. Sweeper hands them to mark_interrupted on startup."""
```

`claim_next_indexing_job` shape:

```python
def claim_next_indexing_job(self, now_ms: int) -> IndexerJob | None:
    self._conn.execute("BEGIN IMMEDIATE")
    try:
        row = self._conn.execute(
            "SELECT j.id, j.video_id, v.ext "
            "FROM jobs j JOIN videos v ON v.id = j.video_id "
            "WHERE j.kind='upload' AND v.status='queued' "
            "ORDER BY j.started_at ASC LIMIT 1"
        ).fetchone()
        if row is None:
            self._conn.execute("COMMIT")
            return None
        job_id, video_id, ext = row
        self._conn.execute("UPDATE videos SET status='indexing', updated_at=? WHERE id=?", (now_ms, video_id))
        self._conn.execute("UPDATE jobs SET status='indexing' WHERE id=?", (job_id,))
        self._conn.execute("COMMIT")
        return IndexerJob(job_id=job_id, video_id=video_id, ext=ext)
    except Exception:
        self._conn.execute("ROLLBACK")
        raise
```

`BEGIN IMMEDIATE` acquires SQLite's write lock immediately, preventing two concurrent claimers. Cheap insurance even at concurrency=1.

### Sweeper extension

```python
# backend/src/dash/jobs/sweeper.py — add function

def recover_interrupted_at_startup(db: Db, settings: Settings, now_ms: int) -> int:
    """Called ONCE in lifespan startup, BEFORE indexing_loop launches.
    Marks all status='indexing' videos as 'interrupted' (no process owns them).
    Returns count for logging."""
    rows = db.find_interrupted_indexing_at_startup()
    for v in rows:
        db.mark_interrupted(v.id, now_ms=now_ms)
        _log.warning("indexing.recovered_as_interrupted", video_id=v.id, filename=v.filename)
    return len(rows)
```

The periodic `run_sweep` is UNCHANGED in 1.2a. We deliberately do NOT mark indexing jobs interrupted periodically — a long indexer is not a stuck indexer. Interruption recovery is scoped to "we just started, no process can be working."

---

## Section 4 — REST changes + Zenoh progress + retry endpoint

### `POST /api/videos/{id}/complete` — behavior change

Phase 1.1 set `status='ready'`. Phase 1.2a sets `status='queued'`:

```python
# backend/src/dash/api/videos.py — complete handler diff
- db.mark_ready(video_id, now_ms=_now_ms())
- return CompleteResponse(video_id=video_id, status="ready")
+ db.mark_queued(video_id, now_ms=_now_ms())
+ return CompleteResponse(video_id=video_id, status="queued")
```

The idempotency check also expands — instead of `if video.status == "ready"`, we accept any of `queued | indexing | done` as already-handled.

### `GET /api/videos` summaries — add `job_id`

Frontend retry button needs the job ID per row. Add to `VideoSummary`:

```python
class VideoSummary(BaseModel):
    id: str
    filename: str
    size_bytes: int
    status: str
    created_at: int
    updated_at: int
    completed_at: int | None
    progress_pct: int
    job_id: str | None        # NEW: upload job id (null only if no upload job — shouldn't happen)
    indexed_frame_count: int | None   # NEW: only set when status='done'
```

The list query already joins videos with jobs for `progress_pct`. Add `job_id` to that join.

### New: `POST /api/jobs/{id}/retry`

```python
# backend/src/dash/api/jobs.py — append handler

@router.post(
    "/{job_id}/retry",
    response_model=JobSummary,
    dependencies=[Depends(require_bearer_token)],
)
async def retry_job(job_id: str, request: Request) -> JobSummary:
    db = request.app.state.db
    settings = request.app.state.settings
    j = db.get_job(job_id)
    if j is None:
        raise HTTPException(status_code=404, detail="job_not_found")
    video = db.get_video(j.video_id)
    if video is None or video.status != "interrupted":
        raise HTTPException(
            status_code=409,
            detail=f"video status={video.status if video else 'gone'}, cannot retry",
        )
    # Delete partial frames directory.
    frames_dir = settings.data_dir / "videos" / j.video_id / "streams" / "rgb"
    if frames_dir.exists():
        shutil.rmtree(frames_dir, ignore_errors=True)
    db.retry_interrupted(j.video_id, now_ms=_now_ms())
    j2 = db.get_job(job_id)
    assert j2 is not None
    return JobSummary(**j2.__dict__)
```

### Zenoh progress publishing

Topic: `bubbaloop-dash/jobs/{job_id}/progress`
Payload (JSON, UTF-8): `{"pct": int, "stage": "decoding", "frame_idx": int, "total_estimated": int}`
Frequency: every `INDEXING_PROGRESS_INTERVAL_FRAMES` (default 10) decoded sampled frames + once at start + once at end.

```python
# backend/src/dash/jobs/indexer.py — helper

def publish_progress(zsess: ZenohSession, job_id: str, pct: int, frame_idx: int, total: int) -> None:
    """Fire-and-forget Zenoh publish. Called from runner thread."""
    if zsess.status() != "ok":
        return
    key = f"bubbaloop-dash/jobs/{job_id}/progress"
    payload = json.dumps({
        "pct": pct, "stage": "decoding", "frame_idx": frame_idx, "total_estimated": total,
    }).encode("utf-8")
    try:
        zsess.session().put(key, payload, encoding="application/json")
    except Exception as exc:  # noqa: BLE001
        _log.warning("zenoh.progress_publish_failed", job_id=job_id, error=str(exc))
```

`zenoh.Session.put` is thread-safe. The runner's threadpool worker calls this synchronously — no asyncio bridge needed.

Browser will receive via the `/ws/zenoh` proxy from Phase 1.0 (the multiplex relays any topic the browser subscribes to). Phase 1.2a publishes; frontend Zenoh subscription is Phase 1.4 polish — for now, 5-second polling does the job.

---

## Section 5 — Frontend + error handling

### `VideoLibrary` extensions

Phase 1.1's status set was `uploading | ready | error | abandoned`. Phase 1.2a adds three:

| status | UI rendering | Right-side action |
|---|---|---|
| `queued` | ⏳ queued · *relative time* | `[…]` Delete |
| `indexing` | inline progress bar (from `progress_pct`) | `[…]` Delete |
| `done` | ✓ done · *N frames* · *relative time* | `[…]` Delete |
| `interrupted` | ⚠ interrupted · *relative time* | **`[Retry]`** + `[Delete]` |
| `error` (1.1+1.2a) | ✗ error · *reason* | `[Delete]` |

`done` shows `indexed_frame_count`. `uploading` still renders the upload progress bar from 1.1 (using chunk-bitmap-derived `progress_pct`).

### Auto-refresh extension

```typescript
const hasActiveWork = videos.some((v) =>
  v.status === 'uploading' ||
  v.status === 'queued' ||
  v.status === 'indexing'
);
```

Same 5-second cadence; stops when all videos are terminal.

### Retry button

```tsx
const onRetry = async (videoId: string, jobId: string) => {
  if (!token) return;
  await fetch(`/api/jobs/${jobId}/retry`, {
    method: 'POST',
    headers: { Authorization: `Bearer ${token}` },
  });
  void refresh();
};
```

Library renders the button when `status === 'interrupted'`. The job id comes from the new `job_id` field on `VideoSummary`.

### Error handling per subsystem

| Subsystem | Failure | Behavior |
|---|---|---|
| **Job runner** | DB unavailable on poll | log WARN, sleep, retry next poll (no crash) |
| | `claim_next_indexing_job` raises | log ERROR, sleep, retry |
| | Runner cancelled (lifespan shutdown) | finish current frame, exit cleanly on `CancelledError` |
| **PyAV decoder** | `av.open` raises (corrupt mp4, missing codec) | `mark_index_error(reason=str(exc))`; partial frames stay on disk (sweeper would clean orphan dirs in 1.2 future work) |
| | OOM / segfault inside decoder | Process dies. On restart, `recover_interrupted_at_startup` marks `interrupted`; user retries. |
| | disk full mid-write | `mark_index_error(reason="disk_full")`; leave partials |
| **Zenoh progress publish** | session not ready | silently skip (don't fail the job) |
| | `zenoh.put` raises | log WARN, continue (publish is non-essential) |
| **Sweeper** | `recover_interrupted_at_startup` raises | log ERROR; runner still starts (interrupted videos stay `indexing`; next startup retries) |
| **Retry endpoint** | video not in `interrupted` status | 409 with clear message |
| | `shutil.rmtree` fails (permissions) | log WARN; transition anyway |

### Logging policy

- **INFO**: indexing started, indexing complete (with frame count), retry triggered, interrupted-recovered count at startup.
- **WARN**: PyAV warnings, Zenoh publish failures, sweeper anomalies.
- **ERROR**: decoder errors that move a job to `error` state, lifespan startup failures.

Never log raw frame bytes. Never log file paths in 4xx responses to the client (server-side only).

---

## Section 6 — Testing + phasing

### Tests

**Unit (pytest, fast):**

- `tests/test_pyav_decoder.py`:
  - Fixture: 2-second test mp4 (320x240, 24 fps, single keyframe) generated at test setup via `ffmpeg -f lavfi -i testsrc=duration=2:size=320x240:rate=24 fixture.mp4`. Skip the test if ffmpeg not on PATH. Cache the fixture under `tests/_fixtures/`.
  - Verify: decoder extracts 2 frames at SAMPLE_FPS=1; both are valid JPEGs; dimensions cap at JPG_MAX_DIM; progress_cb invoked at expected times.

- `tests/test_db.py` (extension):
  - `claim_next_indexing_job` returns next queued job AND atomically transitions to indexing (verify in two queries before/after).
  - `find_interrupted_indexing_at_startup` returns only `indexing` rows.
  - All new transitions wrapped in transactions.

- `tests/test_indexer.py` (new):
  - Mock `PyAVDecoder`; verify `claim → run → mark_indexed` happy path.
  - Decoder raises → `mark_index_error` called with reason.

- `tests/test_sweeper.py` (extension):
  - `recover_interrupted_at_startup` finds `indexing` rows and marks them `interrupted`. Does NOT touch `done | error`.

**Integration (pytest with TestClient + lifespan):**

- End-to-end happy path: POST /api/videos → PUT chunks → POST /complete (asserts status=`queued`) → poll until status=`done` → assert `indexed_frame_count > 0` + JPGs exist on disk.
- Retry: insert `interrupted` row directly into Db; create dummy partial frames dir → POST /api/jobs/{id}/retry → assert status=`queued` and frames dir gone.
- Auto-enqueue: POST /complete results in `status='queued'` (regression test for the 1.1→1.2a change).

The runner shares the event loop with TestClient (because the test fixture uses `with TestClient(app)`). Tests use either `await asyncio.sleep(N)` or a poll-until-condition loop with timeout.

**Explicit non-goals for 1.2a tests:**

- No load testing — single-user assumption.
- No fuzz testing of decoder — malformed mp4 → `error`, that's enough.
- No GPU coverage — 1.2a doesn't use GPU.

### Phasing

| Sub-phase | Scope | Ship target |
|---|---|---|
| **1.2a.1** | Schema migration + new Db methods + transactions + unit tests | Day 1 |
| **1.2a.2** | `PyAVDecoder` + decoder unit tests (ffmpeg-generated fixture) | Day 2 |
| **1.2a.3** | `Indexer` + `indexing_loop` + `recover_interrupted_at_startup` + Zenoh progress + lifespan wiring + integration tests | Day 3 |
| **1.2a.4** | `POST /complete` behavior change + `POST /api/jobs/{id}/retry` + `VideoSummary` schema additions (`job_id`, `indexed_frame_count`) + tests | Day 4 |
| **1.2a.5** | Frontend `VideoLibrary` badges + retry button + auto-refresh extension | Day 4 |
| **1.2a.6** | Dockerfile (`av` + `pillow` deps; image grows 345 MB → ~440 MB) + manual smoke (decode a 5s clip end-to-end in browser) + tag `v0.1.2a-alpha` | Day 5 |

5-day target. Same TDD discipline as 1.0 / 1.1.

---

## Section 7 — Open questions / deferrals

| Question | Deferred to | Why |
|---|---|---|
| Frontend Zenoh subscription for sub-second progress | Phase 1.4 polish | 5 s polling is acceptable for v1 |
| Multi-stream extraction (`streams/depth/`, `streams/seg/`) | Phase 2+ | Only RGB needed for CLIP / SLAM / v1; storage layout already supports `streams/<kind>/` |
| Keyframe-based seeking | Phase 1.4 polish | Optimization, not correctness |
| Resumable indexing across crashes | Phase 2+ | Restart-from-scratch acceptable for ≤1-hour videos |
| Per-video SAMPLE_FPS override | Phase 1.4 | Single global FPS suffices for self-hosted single-user |
| Cancel-running-indexer endpoint | Phase 1.2b or 1.4 | Useful for long jobs; not strictly needed for v1 |
| Re-indexing existing `done` videos after SAMPLE_FPS change | Phase 1.4 | Niche; user can delete + re-upload |
| Orphan-frames-directory cleanup in periodic sweep | Phase 1.2b or 1.4 | 1.2b adds the sister case (orphan LanceDB rows), better to design both together |

---

## Glossary updates (additive to Phase 1.0 + 1.1)

- **Indexing** — Phase 1.2's two-pass operation: 1.2a does frame extraction; 1.2b adds embedding. A video moves through `queued → indexing → done` for the combined work.
- **Sample FPS** — frames-per-second extracted from the source video (default 1.0). Independent of source FPS.
- **PyAVDecoder** — synchronous frame extractor; takes a source mp4, writes JPEGs to a directory, reports progress.
- **Indexer** — backend orchestrator combining Db state transitions + PyAVDecoder + Zenoh progress publish.
- **`claim_next_indexing_job`** — atomic Db method that picks a queued job and moves it to indexing in a single transaction.
- **`recover_interrupted_at_startup`** — sweeper helper that marks all `indexing` videos as `interrupted` once at app startup.
- **`done`** — terminal indexed state. In 1.2a means "frames on disk"; in 1.2b will mean "frames on disk + LanceDB rows."
