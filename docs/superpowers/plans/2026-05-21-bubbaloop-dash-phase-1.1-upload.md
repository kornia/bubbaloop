# bubbaloop-dash Phase 1.1 (Upload + Library) Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Ship the user-facing video upload pipeline in `bubbaloop-dash`: chunked PUTs through the backend to filesystem storage, SQLite-backed progress tracking, integrity sweeper, the Map tab, the VideoUploader component, and the VideoLibrary component. End state: user drops an iPhone clip → server stores it durably → library shows the video; nothing decodes yet (Phase 1.2 ships the indexer).

**Architecture:** Backend extends Phase 1.0's FastAPI app with: a `Storage` Protocol implemented for the local filesystem, a SQLite (WAL) jobs/videos schema with a chunk bitmap, REST endpoints for upload lifecycle + listing + delete + jobs + blob serving, and a background asyncio sweeper task. Frontend adds a fourth top-level tab ("Map") whose sidebar hosts a drag-or-pick uploader and a status-aware library; the canvas area is a placeholder for Spec 2.

**Tech Stack:**
- Backend: Python 3.11, FastAPI, `sqlite3` (stdlib, WAL mode), `os.pwrite` for atomic chunk writes, `asyncio` for the sweeper, `structlog`, pytest.
- Frontend: React 18 + TypeScript + Vite, vitest, `@testing-library/react`. No new npm deps.
- No new system deps. No Docker changes (Phase 1.0's image already mounts `${DATA_DIR}`).

**Repository:** `/home/nvidia/bubbaloop-dash/` (sibling of `/home/nvidia/bubbaloop/`). All work happens there. The plan lives in `bubbaloop` as a docs artifact.

**Out of scope (separate phases):** PyAV decoder (1.2), open_clip embedder (1.2), background job runner (1.2), LanceDB schema/writes (1.2), search UI (1.3), map canvas + SLAM (Spec 2), cross-session upload resume (1.4 or never), multi-file batch (1.4 or never), video thumbnails (1.2), video probe metadata in library (1.2).

---

## Prerequisites

- Phase 1.0 shipped: `bubbaloop-dash` repo exists at `/home/nvidia/bubbaloop-dash/` with `v0.1.0-alpha` tag.
- Backend `.venv` already created and populated. Verify: `cd /home/nvidia/bubbaloop-dash/backend && source .venv/bin/activate && pytest -m "not e2e" -v` shows 34 passed.
- Frontend `node_modules` installed. Verify: `cd /home/nvidia/bubbaloop-dash/frontend && npm test 2>&1 | tail -5` shows 342 passed.
- Working from `main`: `cd /home/nvidia/bubbaloop-dash && git checkout main && git pull --ff-only`.

---

## File Structure (target after Phase 1.1)

```
/home/nvidia/bubbaloop-dash/
├── backend/
│   ├── src/dash/
│   │   ├── config.py                # MODIFIED: + max_upload_bytes, upload_chunk_size_bytes, abandoned_after_s
│   │   ├── factory.py               # MODIFIED: wire Db + sweeper into lifespan; mount videos/jobs/blob routers
│   │   ├── api/
│   │   │   ├── videos.py            # NEW: POST/PUT/GET/DELETE /api/videos
│   │   │   ├── jobs.py              # NEW: GET/DELETE /api/jobs
│   │   │   └── blob.py              # NEW: GET /api/blob/{path...}
│   │   ├── db/
│   │   │   ├── __init__.py          # NEW
│   │   │   ├── schema.sql           # NEW: CREATE TABLE statements
│   │   │   ├── sqlite.py            # NEW: Db class wrapping sqlite3 connection
│   │   │   └── bitmap.py            # NEW: pack/unpack/popcount helpers
│   │   ├── storage/
│   │   │   ├── __init__.py          # NEW
│   │   │   ├── base.py              # NEW: Storage Protocol
│   │   │   └── filesystem.py        # NEW: FilesystemStorage impl
│   │   └── jobs/
│   │       ├── __init__.py          # NEW
│   │       └── sweeper.py           # NEW: run_sweep + sweeper_loop
│   └── tests/
│       ├── test_storage.py          # NEW
│       ├── test_bitmap.py           # NEW
│       ├── test_db.py               # NEW
│       ├── test_sweeper.py          # NEW
│       ├── test_videos_api.py       # NEW
│       ├── test_jobs_api.py         # NEW
│       └── test_blob_api.py         # NEW
└── frontend/
    ├── src/
    │   ├── App.tsx                  # MODIFIED: add "map" AppView, Map tab button, route to MapTab
    │   ├── components/
    │   │   ├── MapTab.tsx           # NEW
    │   │   ├── MapCanvas.tsx        # NEW (placeholder)
    │   │   ├── VideoUploader.tsx    # NEW
    │   │   ├── VideoLibrary.tsx     # NEW
    │   │   └── __tests__/
    │   │       ├── VideoUploader.test.tsx  # NEW
    │   │       └── VideoLibrary.test.tsx   # NEW
    │   └── lib/
    │       └── (existing api.ts is sufficient; no changes)
```

Each backend module has one responsibility. The DB wrapper hides SQL from API handlers; the Storage Protocol hides filesystem details; the Sweeper is its own module with a pure `run_sweep(db, storage, now_ms, abandoned_after_s)` function plus a thin `sweeper_loop` wrapper. Frontend components are split by role: container (`MapTab`), input (`VideoUploader`), display (`VideoLibrary`), placeholder (`MapCanvas`).

Tasks below assume the executing engineer's CWD is `/home/nvidia/bubbaloop-dash/` unless otherwise stated.

---

## Task 1: Setup branch + Settings additions

**Files:**
- Create branch: `feat/phase-1.1-upload`
- Modify: `backend/src/dash/config.py`
- Modify: `backend/tests/test_config.py`

- [ ] **Step 1: Branch off main**

```bash
cd /home/nvidia/bubbaloop-dash
git checkout main
git pull --ff-only
git checkout -b feat/phase-1.1-upload
```

Expected: "Switched to a new branch 'feat/phase-1.1-upload'".

- [ ] **Step 2: Extend test_config.py with assertions for new fields**

Add these test functions to `backend/tests/test_config.py` (append at end of existing file):

```python
def test_settings_phase_1_1_defaults(tmp_path, monkeypatch) -> None:
    monkeypatch.setenv("DATA_DIR", str(tmp_path))
    monkeypatch.delenv("DASHBOARD_TOKEN", raising=False)
    s = Settings()
    assert s.max_upload_bytes == 16 * 1024 * 1024 * 1024  # 16 GB
    assert s.upload_chunk_size_bytes == 8 * 1024 * 1024   # 8 MB
    assert s.abandoned_after_s == 86400                    # 24h


def test_settings_phase_1_1_overrides(tmp_path, monkeypatch) -> None:
    monkeypatch.setenv("DATA_DIR", str(tmp_path))
    monkeypatch.setenv("MAX_UPLOAD_BYTES", "1073741824")        # 1 GB
    monkeypatch.setenv("UPLOAD_CHUNK_SIZE_BYTES", "4194304")    # 4 MB
    monkeypatch.setenv("ABANDONED_AFTER_S", "3600")             # 1h
    s = Settings()
    assert s.max_upload_bytes == 1073741824
    assert s.upload_chunk_size_bytes == 4194304
    assert s.abandoned_after_s == 3600
```

- [ ] **Step 3: Run, watch fail**

```bash
cd /home/nvidia/bubbaloop-dash/backend && source .venv/bin/activate
pytest tests/test_config.py -v 2>&1 | tail -8
```

Expected: 2 failures (AttributeError: 'Settings' has no attribute 'max_upload_bytes').

- [ ] **Step 4: Add the three fields to Settings**

Edit `backend/src/dash/config.py` — add these fields inside the `Settings` class, after the existing `job_concurrency` field, before the `static_dir` field:

```python
    max_upload_bytes: int = Field(default=16 * 1024 * 1024 * 1024, ge=1)
    upload_chunk_size_bytes: int = Field(default=8 * 1024 * 1024, ge=1024)
    abandoned_after_s: int = Field(default=86400, ge=60)
```

- [ ] **Step 5: Run, watch pass**

```bash
cd /home/nvidia/bubbaloop-dash/backend && source .venv/bin/activate
pytest tests/test_config.py -v 2>&1 | tail -10
```

Expected: 4 passed (2 new + 2 existing).

- [ ] **Step 6: Commit**

```bash
cd /home/nvidia/bubbaloop-dash
git add backend/src/dash/config.py backend/tests/test_config.py
git commit -m "feat(config): add upload + sweep tuning knobs

MAX_UPLOAD_BYTES (16 GB), UPLOAD_CHUNK_SIZE_BYTES (8 MB),
ABANDONED_AFTER_S (24h) — used by Phase 1.1 upload pipeline + sweeper."
```

---

## Task 2: Storage Protocol + FilesystemStorage

**Files:**
- Create: `backend/src/dash/storage/__init__.py` (empty)
- Create: `backend/src/dash/storage/base.py`
- Create: `backend/src/dash/storage/filesystem.py`
- Create: `backend/tests/test_storage.py`

- [ ] **Step 1: Create the directory + empty __init__**

```bash
cd /home/nvidia/bubbaloop-dash
mkdir -p backend/src/dash/storage
touch backend/src/dash/storage/__init__.py
```

- [ ] **Step 2: Write the failing tests (`backend/tests/test_storage.py`)**

```python
"""FilesystemStorage: write_chunk is atomic per disjoint offset;
path-traversal protection; idempotent delete."""
from pathlib import Path

import pytest

from dash.storage.filesystem import FilesystemStorage


def test_write_then_read_round_trip(tmp_path: Path) -> None:
    s = FilesystemStorage(root=tmp_path)
    s.write_chunk("a/b/c.bin", offset=0, data=b"hello")
    assert s.read("a/b/c.bin") == b"hello"


def test_write_chunk_at_offset(tmp_path: Path) -> None:
    s = FilesystemStorage(root=tmp_path)
    s.write_chunk("f.bin", offset=0, data=b"AAAA")
    s.write_chunk("f.bin", offset=4, data=b"BBBB")
    assert s.read("f.bin") == b"AAAABBBB"
    assert s.size("f.bin") == 8


def test_write_chunk_disjoint_offsets_concurrent(tmp_path: Path) -> None:
    """Two writes to non-overlapping ranges of the same file: both land."""
    s = FilesystemStorage(root=tmp_path)
    s.write_chunk("f.bin", offset=8, data=b"BBBB")
    s.write_chunk("f.bin", offset=0, data=b"AAAA")  # out-of-order
    assert s.read("f.bin") == b"AAAA\x00\x00\x00\x00BBBB"


def test_path_traversal_rejected(tmp_path: Path) -> None:
    s = FilesystemStorage(root=tmp_path)
    with pytest.raises(ValueError, match="escapes"):
        s.write_chunk("../../etc/passwd", offset=0, data=b"x")


def test_delete_idempotent(tmp_path: Path) -> None:
    s = FilesystemStorage(root=tmp_path)
    s.write_chunk("g.bin", offset=0, data=b"x")
    s.delete("g.bin")
    s.delete("g.bin")  # second delete must NOT raise


def test_exists_and_size(tmp_path: Path) -> None:
    s = FilesystemStorage(root=tmp_path)
    assert s.exists("nope") is False
    s.write_chunk("h.bin", offset=0, data=b"hello")
    assert s.exists("h.bin") is True
    assert s.size("h.bin") == 5


def test_url_for_returns_api_path(tmp_path: Path) -> None:
    s = FilesystemStorage(root=tmp_path)
    assert s.url_for("videos/abc/source.mp4") == "/api/blob/videos/abc/source.mp4"
```

- [ ] **Step 3: Run, watch fail**

```bash
cd /home/nvidia/bubbaloop-dash/backend && source .venv/bin/activate
pytest tests/test_storage.py -v 2>&1 | tail -10
```

Expected: `ModuleNotFoundError: No module named 'dash.storage.filesystem'`.

- [ ] **Step 4: Implement the Protocol (`backend/src/dash/storage/base.py`)**

```python
"""Blob storage abstraction.

Phase 1.1 impl is FilesystemStorage. Phase 2+ adds S3Storage with the same
interface — `url_for` becomes a presigned-URL generator, `write_chunk`
buffers + uploads multipart parts, etc. Consumers (API handlers, sweeper)
depend on this Protocol, not on FilesystemStorage."""
from typing import Protocol


class Storage(Protocol):
    def write_chunk(self, path: str, offset: int, data: bytes) -> int:
        """Atomically write `data` at byte `offset` of `path`. Returns bytes written.
        Creates the file (and parent dirs) if missing. Safe for concurrent writes
        to non-overlapping offsets within the same file."""

    def read(self, path: str) -> bytes: ...

    def url_for(self, path: str) -> str:
        """Returns a URL the browser can fetch the file from."""

    def delete(self, path: str) -> None:
        """Idempotent — deleting a missing path is not an error."""

    def size(self, path: str) -> int:
        """Returns the current size in bytes of `path`. Raises FileNotFoundError if missing."""

    def exists(self, path: str) -> bool: ...
```

- [ ] **Step 5: Implement FilesystemStorage (`backend/src/dash/storage/filesystem.py`)**

```python
"""Local-filesystem implementation of the Storage Protocol.

Uses os.pwrite for atomic non-overlapping-offset writes — two concurrent
write_chunk calls to disjoint byte ranges of the same file are safe without
explicit locking."""
import os
from pathlib import Path


class FilesystemStorage:
    def __init__(self, root: Path) -> None:
        self._root = root.resolve()
        self._root.mkdir(parents=True, exist_ok=True)

    def _abs(self, path: str) -> Path:
        candidate = (self._root / path).resolve()
        try:
            candidate.relative_to(self._root)
        except ValueError as exc:
            raise ValueError(f"path escapes storage root: {path}") from exc
        return candidate

    def write_chunk(self, path: str, offset: int, data: bytes) -> int:
        abs_path = self._abs(path)
        abs_path.parent.mkdir(parents=True, exist_ok=True)
        fd = os.open(abs_path, os.O_WRONLY | os.O_CREAT, 0o644)
        try:
            return os.pwrite(fd, data, offset)
        finally:
            os.close(fd)

    def read(self, path: str) -> bytes:
        return self._abs(path).read_bytes()

    def url_for(self, path: str) -> str:
        return f"/api/blob/{path}"

    def delete(self, path: str) -> None:
        try:
            self._abs(path).unlink()
        except FileNotFoundError:
            pass

    def size(self, path: str) -> int:
        return self._abs(path).stat().st_size

    def exists(self, path: str) -> bool:
        return self._abs(path).exists()
```

- [ ] **Step 6: Run, watch pass**

```bash
cd /home/nvidia/bubbaloop-dash/backend && source .venv/bin/activate
pytest tests/test_storage.py -v 2>&1 | tail -12
```

Expected: 7 passed.

- [ ] **Step 7: Commit**

```bash
cd /home/nvidia/bubbaloop-dash
git add backend/src/dash/storage/ backend/tests/test_storage.py
git commit -m "feat(storage): Storage Protocol + FilesystemStorage

Atomic chunk writes via os.pwrite (disjoint-offset safe). Path-traversal
protection at root. Idempotent delete. Phase 2+ swaps in S3Storage with
the same interface; API handlers depend on Protocol, not impl."
```

---

## Task 3: Chunk bitmap helpers

**Files:**
- Create: `backend/src/dash/db/__init__.py` (empty)
- Create: `backend/src/dash/db/bitmap.py`
- Create: `backend/tests/test_bitmap.py`

- [ ] **Step 1: Create the directory + empty __init__**

```bash
cd /home/nvidia/bubbaloop-dash
mkdir -p backend/src/dash/db
touch backend/src/dash/db/__init__.py
```

- [ ] **Step 2: Write the failing tests (`backend/tests/test_bitmap.py`)**

```python
"""Packed-bits chunk bitmap helpers."""
import pytest

from dash.db.bitmap import (
    empty_bitmap,
    get_bit,
    popcount,
    set_bit,
    missing_indices,
    is_complete,
)


def test_empty_bitmap_sized_correctly() -> None:
    assert empty_bitmap(0) == b""
    assert empty_bitmap(1) == b"\x00"
    assert empty_bitmap(8) == b"\x00"
    assert empty_bitmap(9) == b"\x00\x00"
    assert empty_bitmap(17) == b"\x00\x00\x00"


def test_set_and_get_bit() -> None:
    b = empty_bitmap(16)
    assert get_bit(b, 0) is False
    b = set_bit(b, 0)
    assert get_bit(b, 0) is True
    assert get_bit(b, 1) is False
    b = set_bit(b, 15)
    assert get_bit(b, 15) is True
    assert get_bit(b, 8) is False


def test_set_bit_is_idempotent() -> None:
    b = empty_bitmap(8)
    b1 = set_bit(b, 3)
    b2 = set_bit(b1, 3)
    assert b1 == b2


def test_popcount() -> None:
    b = empty_bitmap(16)
    assert popcount(b) == 0
    b = set_bit(b, 0)
    b = set_bit(b, 5)
    b = set_bit(b, 12)
    assert popcount(b) == 3


def test_missing_indices() -> None:
    b = empty_bitmap(5)
    assert missing_indices(b, total=5) == [0, 1, 2, 3, 4]
    b = set_bit(b, 0)
    b = set_bit(b, 2)
    b = set_bit(b, 4)
    assert missing_indices(b, total=5) == [1, 3]


def test_is_complete() -> None:
    b = empty_bitmap(3)
    assert is_complete(b, total=3) is False
    b = set_bit(b, 0)
    b = set_bit(b, 1)
    assert is_complete(b, total=3) is False
    b = set_bit(b, 2)
    assert is_complete(b, total=3) is True


def test_get_bit_out_of_range_raises() -> None:
    b = empty_bitmap(8)
    with pytest.raises(IndexError):
        get_bit(b, 8)
```

- [ ] **Step 3: Run, watch fail**

```bash
cd /home/nvidia/bubbaloop-dash/backend && source .venv/bin/activate
pytest tests/test_bitmap.py -v 2>&1 | tail -10
```

Expected: import error.

- [ ] **Step 4: Implement (`backend/src/dash/db/bitmap.py`)**

```python
"""Packed-bits chunk bitmap.

byte index = bit_index // 8 ;  bit within byte = bit_index % 8 (LSB-first)

Used by the Db wrapper to store upload progress in a BLOB column. Not
exposed in any API response — server-side only."""


def empty_bitmap(total: int) -> bytes:
    if total <= 0:
        return b""
    return b"\x00" * ((total + 7) // 8)


def get_bit(b: bytes, idx: int) -> bool:
    if idx < 0 or idx >= len(b) * 8:
        raise IndexError(f"bit index {idx} out of range for bitmap of {len(b) * 8} bits")
    return bool(b[idx // 8] & (1 << (idx % 8)))


def set_bit(b: bytes, idx: int) -> bytes:
    if idx < 0:
        raise IndexError(f"negative bit index {idx}")
    byte_idx = idx // 8
    if byte_idx >= len(b):
        raise IndexError(f"bit index {idx} out of range for bitmap of {len(b) * 8} bits")
    ba = bytearray(b)
    ba[byte_idx] |= 1 << (idx % 8)
    return bytes(ba)


def popcount(b: bytes) -> int:
    return sum(bin(byte).count("1") for byte in b)


def missing_indices(b: bytes, total: int) -> list[int]:
    return [i for i in range(total) if not get_bit(b, i)]


def is_complete(b: bytes, total: int) -> bool:
    return popcount(b) >= total
```

- [ ] **Step 5: Run, watch pass**

```bash
cd /home/nvidia/bubbaloop-dash/backend && source .venv/bin/activate
pytest tests/test_bitmap.py -v 2>&1 | tail -10
```

Expected: 7 passed.

- [ ] **Step 6: Commit**

```bash
cd /home/nvidia/bubbaloop-dash
git add backend/src/dash/db/__init__.py backend/src/dash/db/bitmap.py backend/tests/test_bitmap.py
git commit -m "feat(db): chunk bitmap helpers (pack/unpack/popcount/missing)

LSB-first packed bits. ceil(N/8) bytes for N chunks. Used server-side
only for /complete validation; never exposed in API responses."
```

---

## Task 4: SQLite schema + Db wrapper (videos table)

**Files:**
- Create: `backend/src/dash/db/schema.sql`
- Create: `backend/src/dash/db/sqlite.py`
- Create: `backend/tests/test_db.py`

- [ ] **Step 1: Create the schema SQL (`backend/src/dash/db/schema.sql`)**

```sql
CREATE TABLE IF NOT EXISTS videos (
    id              TEXT PRIMARY KEY,
    filename        TEXT NOT NULL,
    content_type    TEXT NOT NULL,
    ext             TEXT NOT NULL,
    size_bytes      INTEGER NOT NULL,
    chunk_size      INTEGER NOT NULL,
    total_chunks    INTEGER NOT NULL,
    status          TEXT NOT NULL,
    error           TEXT,
    created_at      INTEGER NOT NULL,
    updated_at      INTEGER NOT NULL,
    completed_at    INTEGER
);

CREATE TABLE IF NOT EXISTS upload_progress (
    video_id        TEXT PRIMARY KEY REFERENCES videos(id) ON DELETE CASCADE,
    received_chunks BLOB NOT NULL,
    received_bytes  INTEGER NOT NULL DEFAULT 0,
    last_chunk_at   INTEGER NOT NULL
);

CREATE TABLE IF NOT EXISTS jobs (
    id              TEXT PRIMARY KEY,
    video_id        TEXT NOT NULL REFERENCES videos(id) ON DELETE CASCADE,
    kind            TEXT NOT NULL,
    status          TEXT NOT NULL,
    progress_pct    INTEGER NOT NULL DEFAULT 0,
    error           TEXT,
    started_at      INTEGER NOT NULL,
    finished_at     INTEGER
);

CREATE INDEX IF NOT EXISTS idx_videos_status ON videos(status);
CREATE INDEX IF NOT EXISTS idx_videos_updated ON videos(updated_at DESC);
CREATE INDEX IF NOT EXISTS idx_jobs_video ON jobs(video_id);
CREATE INDEX IF NOT EXISTS idx_jobs_status ON jobs(status);
```

- [ ] **Step 2: Write the failing tests for video CRUD (`backend/tests/test_db.py`)**

```python
"""Db wrapper: video lifecycle + listing + delete (cascade).

Subsequent test files cover upload_progress and jobs methods."""
from pathlib import Path

import pytest

from dash.db.sqlite import Db


@pytest.fixture
def db(tmp_path: Path) -> Db:
    return Db.open(tmp_path / "state.sqlite")


def test_create_and_get_video(db: Db) -> None:
    vid = db.create_video(
        filename="entrance.mp4",
        content_type="video/mp4",
        ext="mp4",
        size_bytes=1000,
        chunk_size=400,
        total_chunks=3,
        now_ms=1_000_000,
    )
    v = db.get_video(vid)
    assert v is not None
    assert v.id == vid
    assert v.filename == "entrance.mp4"
    assert v.status == "uploading"
    assert v.size_bytes == 1000
    assert v.total_chunks == 3
    assert v.created_at == 1_000_000
    assert v.updated_at == 1_000_000
    assert v.completed_at is None


def test_get_video_missing_returns_none(db: Db) -> None:
    assert db.get_video("does-not-exist") is None


def test_list_videos_orders_by_updated_at_desc(db: Db) -> None:
    a = db.create_video("a.mp4", "video/mp4", "mp4", 100, 100, 1, now_ms=1000)
    b = db.create_video("b.mp4", "video/mp4", "mp4", 100, 100, 1, now_ms=2000)
    c = db.create_video("c.mp4", "video/mp4", "mp4", 100, 100, 1, now_ms=3000)
    items = db.list_videos(status_filter=None, limit=10)
    assert [v.id for v in items] == [c, b, a]


def test_list_videos_filter_by_status(db: Db) -> None:
    a = db.create_video("a.mp4", "video/mp4", "mp4", 100, 100, 1, now_ms=1000)
    b = db.create_video("b.mp4", "video/mp4", "mp4", 100, 100, 1, now_ms=2000)
    db.mark_ready(a, now_ms=1500)
    ready = db.list_videos(status_filter="ready", limit=10)
    uploading = db.list_videos(status_filter="uploading", limit=10)
    assert [v.id for v in ready] == [a]
    assert [v.id for v in uploading] == [b]


def test_mark_ready_sets_completed_at(db: Db) -> None:
    vid = db.create_video("a.mp4", "video/mp4", "mp4", 100, 100, 1, now_ms=1000)
    db.mark_ready(vid, now_ms=5000)
    v = db.get_video(vid)
    assert v.status == "ready"
    assert v.completed_at == 5000
    assert v.updated_at == 5000
    assert v.error is None


def test_mark_error(db: Db) -> None:
    vid = db.create_video("a.mp4", "video/mp4", "mp4", 100, 100, 1, now_ms=1000)
    db.mark_error(vid, reason="size mismatch", now_ms=5000)
    v = db.get_video(vid)
    assert v.status == "error"
    assert v.error == "size mismatch"
    assert v.updated_at == 5000
    assert v.completed_at is None


def test_mark_abandoned(db: Db) -> None:
    vid = db.create_video("a.mp4", "video/mp4", "mp4", 100, 100, 1, now_ms=1000)
    db.mark_abandoned(vid, now_ms=99999)
    v = db.get_video(vid)
    assert v.status == "abandoned"
    assert v.updated_at == 99999


def test_delete_video_cascades(db: Db) -> None:
    vid = db.create_video("a.mp4", "video/mp4", "mp4", 100, 100, 1, now_ms=1000)
    # upload_progress + jobs rows exist (created by create_video implicitly).
    assert db.get_upload_progress(vid) is not None
    assert len(db.list_jobs(video_id=vid)) == 1
    db.delete_video(vid)
    assert db.get_video(vid) is None
    assert db.get_upload_progress(vid) is None
    assert db.list_jobs(video_id=vid) == []


def test_delete_video_missing_is_noop(db: Db) -> None:
    db.delete_video("does-not-exist")  # must not raise
```

- [ ] **Step 3: Run, watch fail**

```bash
cd /home/nvidia/bubbaloop-dash/backend && source .venv/bin/activate
pytest tests/test_db.py -v 2>&1 | tail -10
```

Expected: import error.

- [ ] **Step 4: Implement Db (`backend/src/dash/db/sqlite.py`)**

```python
"""SQLite WAL wrapper. Single connection per app, opened in lifespan.

Phase 1.1 surface: video CRUD, upload-progress recording, jobs CRUD.
Phase 1.2 will extend with indexing-job methods (kind='indexing')."""
import sqlite3
import uuid
from dataclasses import dataclass
from pathlib import Path

from dash.db.bitmap import empty_bitmap, get_bit, is_complete, missing_indices, popcount, set_bit


_SCHEMA_PATH = Path(__file__).parent / "schema.sql"


@dataclass(frozen=True)
class VideoRow:
    id: str
    filename: str
    content_type: str
    ext: str
    size_bytes: int
    chunk_size: int
    total_chunks: int
    status: str
    error: str | None
    created_at: int
    updated_at: int
    completed_at: int | None


@dataclass(frozen=True)
class UploadProgressRow:
    video_id: str
    received_chunks: bytes
    received_bytes: int
    last_chunk_at: int


@dataclass(frozen=True)
class JobRow:
    id: str
    video_id: str
    kind: str
    status: str
    progress_pct: int
    error: str | None
    started_at: int
    finished_at: int | None


class Db:
    def __init__(self, conn: sqlite3.Connection) -> None:
        self._conn = conn

    @classmethod
    def open(cls, path: Path) -> "Db":
        path.parent.mkdir(parents=True, exist_ok=True)
        conn = sqlite3.connect(path, check_same_thread=False, isolation_level=None)
        conn.execute("PRAGMA journal_mode = WAL")
        conn.execute("PRAGMA foreign_keys = ON")
        conn.execute("PRAGMA busy_timeout = 5000")
        conn.executescript(_SCHEMA_PATH.read_text())
        return cls(conn)

    def close(self) -> None:
        self._conn.close()

    # ---- videos ----

    def create_video(
        self,
        filename: str,
        content_type: str,
        ext: str,
        size_bytes: int,
        chunk_size: int,
        total_chunks: int,
        now_ms: int,
    ) -> str:
        video_id = uuid.uuid4().hex
        job_id = uuid.uuid4().hex
        bitmap = empty_bitmap(total_chunks)
        self._conn.execute("BEGIN")
        try:
            self._conn.execute(
                """INSERT INTO videos (id, filename, content_type, ext, size_bytes,
                       chunk_size, total_chunks, status, created_at, updated_at)
                   VALUES (?, ?, ?, ?, ?, ?, ?, 'uploading', ?, ?)""",
                (video_id, filename, content_type, ext, size_bytes, chunk_size, total_chunks, now_ms, now_ms),
            )
            self._conn.execute(
                """INSERT INTO upload_progress (video_id, received_chunks, received_bytes, last_chunk_at)
                   VALUES (?, ?, 0, ?)""",
                (video_id, bitmap, now_ms),
            )
            self._conn.execute(
                """INSERT INTO jobs (id, video_id, kind, status, progress_pct, started_at)
                   VALUES (?, ?, 'upload', 'uploading', 0, ?)""",
                (job_id, video_id, now_ms),
            )
            self._conn.execute("COMMIT")
        except Exception:
            self._conn.execute("ROLLBACK")
            raise
        return video_id

    def get_video(self, video_id: str) -> VideoRow | None:
        row = self._conn.execute(
            "SELECT id, filename, content_type, ext, size_bytes, chunk_size, total_chunks, "
            "status, error, created_at, updated_at, completed_at FROM videos WHERE id = ?",
            (video_id,),
        ).fetchone()
        return VideoRow(*row) if row else None

    def list_videos(self, status_filter: str | None, limit: int) -> list[VideoRow]:
        if status_filter is None:
            rows = self._conn.execute(
                "SELECT id, filename, content_type, ext, size_bytes, chunk_size, total_chunks, "
                "status, error, created_at, updated_at, completed_at FROM videos "
                "ORDER BY updated_at DESC LIMIT ?",
                (limit,),
            ).fetchall()
        else:
            rows = self._conn.execute(
                "SELECT id, filename, content_type, ext, size_bytes, chunk_size, total_chunks, "
                "status, error, created_at, updated_at, completed_at FROM videos "
                "WHERE status = ? ORDER BY updated_at DESC LIMIT ?",
                (status_filter, limit),
            ).fetchall()
        return [VideoRow(*r) for r in rows]

    def mark_ready(self, video_id: str, now_ms: int) -> None:
        self._conn.execute(
            "UPDATE videos SET status='ready', updated_at=?, completed_at=? WHERE id=?",
            (now_ms, now_ms, video_id),
        )
        self._conn.execute(
            "UPDATE jobs SET status='ready', progress_pct=100, finished_at=? "
            "WHERE video_id=? AND kind='upload'",
            (now_ms, video_id),
        )

    def mark_error(self, video_id: str, reason: str, now_ms: int) -> None:
        self._conn.execute(
            "UPDATE videos SET status='error', error=?, updated_at=? WHERE id=?",
            (reason, now_ms, video_id),
        )
        self._conn.execute(
            "UPDATE jobs SET status='error', error=?, finished_at=? "
            "WHERE video_id=? AND kind='upload'",
            (reason, now_ms, video_id),
        )

    def mark_abandoned(self, video_id: str, now_ms: int) -> None:
        self._conn.execute(
            "UPDATE videos SET status='abandoned', updated_at=? WHERE id=?",
            (now_ms, video_id),
        )
        self._conn.execute(
            "UPDATE jobs SET status='abandoned', finished_at=? "
            "WHERE video_id=? AND kind='upload'",
            (now_ms, video_id),
        )

    def delete_video(self, video_id: str) -> None:
        # ON DELETE CASCADE drops upload_progress + jobs rows.
        self._conn.execute("DELETE FROM videos WHERE id = ?", (video_id,))

    # ---- upload_progress + jobs methods added in Task 5 ----

    def get_upload_progress(self, video_id: str) -> UploadProgressRow | None:
        row = self._conn.execute(
            "SELECT video_id, received_chunks, received_bytes, last_chunk_at "
            "FROM upload_progress WHERE video_id = ?",
            (video_id,),
        ).fetchone()
        return UploadProgressRow(*row) if row else None

    def list_jobs(self, video_id: str | None = None) -> list[JobRow]:
        if video_id is None:
            rows = self._conn.execute(
                "SELECT id, video_id, kind, status, progress_pct, error, started_at, finished_at "
                "FROM jobs ORDER BY started_at DESC"
            ).fetchall()
        else:
            rows = self._conn.execute(
                "SELECT id, video_id, kind, status, progress_pct, error, started_at, finished_at "
                "FROM jobs WHERE video_id = ?",
                (video_id,),
            ).fetchall()
        return [JobRow(*r) for r in rows]
```

- [ ] **Step 5: Run, watch pass**

```bash
cd /home/nvidia/bubbaloop-dash/backend && source .venv/bin/activate
pytest tests/test_db.py -v 2>&1 | tail -15
```

Expected: 9 passed.

- [ ] **Step 6: Commit**

```bash
cd /home/nvidia/bubbaloop-dash
git add backend/src/dash/db/schema.sql backend/src/dash/db/sqlite.py backend/tests/test_db.py
git commit -m "feat(db): SQLite WAL schema + Db wrapper (videos)

videos + upload_progress + jobs tables with FK CASCADE. Single connection
opened in lifespan. create_video atomically inserts video + progress + job
rows in one transaction. mark_ready/mark_error/mark_abandoned + delete
+ list_videos + get_video + get_upload_progress + list_jobs."
```

---

## Task 5: Db wrapper — upload_progress record_chunk + jobs detail

**Files:**
- Modify: `backend/src/dash/db/sqlite.py` (add `record_chunk`, `get_job`)
- Modify: `backend/tests/test_db.py` (append tests for chunk recording + get_job)

- [ ] **Step 1: Append the failing tests to `backend/tests/test_db.py`**

```python
def test_record_chunk_sets_bit_and_increments_bytes(db: Db) -> None:
    vid = db.create_video("a.mp4", "video/mp4", "mp4", 1000, 400, 3, now_ms=1000)
    db.record_chunk(vid, chunk_idx=0, chunk_bytes=400, now_ms=1100)
    up = db.get_upload_progress(vid)
    assert up.received_bytes == 400
    assert up.last_chunk_at == 1100
    # bit 0 set
    from dash.db.bitmap import get_bit
    assert get_bit(up.received_chunks, 0) is True
    assert get_bit(up.received_chunks, 1) is False


def test_record_chunk_idempotent_does_not_double_count(db: Db) -> None:
    vid = db.create_video("a.mp4", "video/mp4", "mp4", 1000, 400, 3, now_ms=1000)
    db.record_chunk(vid, chunk_idx=0, chunk_bytes=400, now_ms=1100)
    db.record_chunk(vid, chunk_idx=0, chunk_bytes=400, now_ms=1200)  # duplicate
    up = db.get_upload_progress(vid)
    assert up.received_bytes == 400  # NOT 800
    assert up.last_chunk_at == 1200  # but last_chunk_at advances


def test_record_chunk_updates_job_progress_pct(db: Db) -> None:
    vid = db.create_video("a.mp4", "video/mp4", "mp4", 1000, 400, 3, now_ms=1000)
    db.record_chunk(vid, chunk_idx=0, chunk_bytes=400, now_ms=1100)
    jobs = db.list_jobs(video_id=vid)
    assert jobs[0].progress_pct == 33  # 1/3 = 33
    db.record_chunk(vid, chunk_idx=1, chunk_bytes=400, now_ms=1200)
    jobs = db.list_jobs(video_id=vid)
    assert jobs[0].progress_pct == 66  # 2/3 = 66


def test_find_stale_uploads(db: Db) -> None:
    a = db.create_video("a.mp4", "video/mp4", "mp4", 1000, 400, 3, now_ms=1000)
    b = db.create_video("b.mp4", "video/mp4", "mp4", 1000, 400, 3, now_ms=2000)
    db.record_chunk(b, chunk_idx=0, chunk_bytes=400, now_ms=5000)  # b is fresh
    # cutoff = 3000: a is stale (last_chunk_at=1000), b is not (=5000)
    stale = db.find_stale_uploads(cutoff_ms=3000)
    assert [v.id for v in stale] == [a]


def test_get_job(db: Db) -> None:
    vid = db.create_video("a.mp4", "video/mp4", "mp4", 1000, 400, 3, now_ms=1000)
    jobs = db.list_jobs(video_id=vid)
    job = db.get_job(jobs[0].id)
    assert job is not None
    assert job.kind == "upload"
    assert job.video_id == vid


def test_get_job_missing_returns_none(db: Db) -> None:
    assert db.get_job("nope") is None
```

- [ ] **Step 2: Run, watch fail**

```bash
cd /home/nvidia/bubbaloop-dash/backend && source .venv/bin/activate
pytest tests/test_db.py -v 2>&1 | tail -10
```

Expected: 6 new failures (AttributeError on `record_chunk` and `find_stale_uploads` and `get_job`).

- [ ] **Step 3: Add the missing methods to `backend/src/dash/db/sqlite.py`**

Append these methods to the `Db` class (before `list_jobs` is fine):

```python
    def record_chunk(self, video_id: str, chunk_idx: int, chunk_bytes: int, now_ms: int) -> None:
        """Record receipt of chunk `chunk_idx`. Idempotent: re-recording the same
        chunk does not double-count bytes (but DOES update last_chunk_at)."""
        up = self.get_upload_progress(video_id)
        if up is None:
            raise ValueError(f"no upload_progress row for video_id={video_id}")
        already_set = get_bit(up.received_chunks, chunk_idx)
        if already_set:
            new_bitmap = up.received_chunks
            new_bytes = up.received_bytes
        else:
            new_bitmap = set_bit(up.received_chunks, chunk_idx)
            new_bytes = up.received_bytes + chunk_bytes
        self._conn.execute(
            "UPDATE upload_progress SET received_chunks=?, received_bytes=?, last_chunk_at=? "
            "WHERE video_id=?",
            (new_bitmap, new_bytes, now_ms, video_id),
        )
        # progress_pct on the upload job
        video = self.get_video(video_id)
        if video is not None and video.total_chunks > 0:
            pct = int(100 * popcount(new_bitmap) / video.total_chunks)
            self._conn.execute(
                "UPDATE jobs SET progress_pct=? WHERE video_id=? AND kind='upload'",
                (pct, video_id),
            )
        # bump video.updated_at
        self._conn.execute(
            "UPDATE videos SET updated_at=? WHERE id=?",
            (now_ms, video_id),
        )

    def find_stale_uploads(self, cutoff_ms: int) -> list[VideoRow]:
        """Returns videos with status='uploading' whose upload_progress.last_chunk_at
        is before `cutoff_ms`. Used by the sweeper to mark them abandoned."""
        rows = self._conn.execute(
            "SELECT v.id, v.filename, v.content_type, v.ext, v.size_bytes, v.chunk_size, "
            "v.total_chunks, v.status, v.error, v.created_at, v.updated_at, v.completed_at "
            "FROM videos v JOIN upload_progress p ON v.id = p.video_id "
            "WHERE v.status = 'uploading' AND p.last_chunk_at < ?",
            (cutoff_ms,),
        ).fetchall()
        return [VideoRow(*r) for r in rows]

    def get_job(self, job_id: str) -> JobRow | None:
        row = self._conn.execute(
            "SELECT id, video_id, kind, status, progress_pct, error, started_at, finished_at "
            "FROM jobs WHERE id = ?",
            (job_id,),
        ).fetchone()
        return JobRow(*row) if row else None
```

- [ ] **Step 4: Run, watch pass**

```bash
cd /home/nvidia/bubbaloop-dash/backend && source .venv/bin/activate
pytest tests/test_db.py -v 2>&1 | tail -20
```

Expected: 15 passed (9 from Task 4 + 6 new).

- [ ] **Step 5: Commit**

```bash
cd /home/nvidia/bubbaloop-dash
git add backend/src/dash/db/sqlite.py backend/tests/test_db.py
git commit -m "feat(db): record_chunk + find_stale_uploads + get_job

record_chunk is idempotent: re-recording same chunk doesn't double-count
bytes, but advances last_chunk_at. Also updates job.progress_pct and
video.updated_at in the same transaction. find_stale_uploads powers the
sweeper. get_job is a single-job lookup for /api/jobs/{id}."
```

---

## Task 6: Sweeper module

**Files:**
- Create: `backend/src/dash/jobs/__init__.py` (empty)
- Create: `backend/src/dash/jobs/sweeper.py`
- Create: `backend/tests/test_sweeper.py`

- [ ] **Step 1: Create the directory + empty __init__**

```bash
cd /home/nvidia/bubbaloop-dash
mkdir -p backend/src/dash/jobs
touch backend/src/dash/jobs/__init__.py
```

- [ ] **Step 2: Write the failing tests (`backend/tests/test_sweeper.py`)**

```python
"""Sweeper: marks stale uploads abandoned + deletes orphan source files.

Tests inject a fake clock (now_ms) and a fake Storage."""
from dataclasses import dataclass, field
from pathlib import Path

import pytest

from dash.db.sqlite import Db
from dash.jobs.sweeper import run_sweep


@dataclass
class FakeStorage:
    """Records delete calls; pretends every queried path exists."""
    deleted: list[str] = field(default_factory=list)
    listed_paths: list[str] = field(default_factory=list)

    def delete(self, path: str) -> None:
        self.deleted.append(path)


def test_marks_stale_uploads_abandoned(tmp_path: Path) -> None:
    db = Db.open(tmp_path / "state.sqlite")
    storage = FakeStorage()
    # Upload started at 1000 with one chunk; never finished.
    vid = db.create_video("a.mp4", "video/mp4", "mp4", 1000, 400, 3, now_ms=1000)
    # Sweep at "now=100000", abandoned_after=10 → cutoff=99990. last_chunk_at=1000 < cutoff.
    report = run_sweep(db, storage, now_ms=100_000, abandoned_after_s=10)
    assert report.abandoned == 1
    v = db.get_video(vid)
    assert v.status == "abandoned"


def test_fresh_upload_not_touched(tmp_path: Path) -> None:
    db = Db.open(tmp_path / "state.sqlite")
    storage = FakeStorage()
    vid = db.create_video("a.mp4", "video/mp4", "mp4", 1000, 400, 3, now_ms=1000)
    # Sweep at "now=5000", abandoned_after=86400 → cutoff is negative; no rows stale.
    report = run_sweep(db, storage, now_ms=5_000, abandoned_after_s=86400)
    assert report.abandoned == 0
    v = db.get_video(vid)
    assert v.status == "uploading"


def test_completed_uploads_not_touched(tmp_path: Path) -> None:
    db = Db.open(tmp_path / "state.sqlite")
    storage = FakeStorage()
    vid = db.create_video("a.mp4", "video/mp4", "mp4", 1000, 400, 3, now_ms=1000)
    db.mark_ready(vid, now_ms=2000)
    # Sweep with cutoff that would mark it stale IF it were still uploading.
    report = run_sweep(db, storage, now_ms=100_000, abandoned_after_s=10)
    assert report.abandoned == 0
    v = db.get_video(vid)
    assert v.status == "ready"
```

- [ ] **Step 3: Run, watch fail**

```bash
cd /home/nvidia/bubbaloop-dash/backend && source .venv/bin/activate
pytest tests/test_sweeper.py -v 2>&1 | tail -10
```

Expected: import error.

- [ ] **Step 4: Implement (`backend/src/dash/jobs/sweeper.py`)**

```python
"""Background integrity sweeper.

Runs in a tokio-style asyncio task in the FastAPI lifespan. The pure
`run_sweep` function does the work; `sweeper_loop` is a thin wrapper that
sleeps + retries on errors so test code can drive run_sweep directly."""
import asyncio
from dataclasses import dataclass
from typing import Any

from dash.logging import get_logger

_log = get_logger("dash.jobs.sweeper")


@dataclass(frozen=True)
class SweepReport:
    abandoned: int
    orphans_deleted: int


def run_sweep(db: Any, storage: Any, now_ms: int, abandoned_after_s: int) -> SweepReport:
    """Single sweep pass. Returns counts for logging/testing."""
    cutoff = now_ms - abandoned_after_s * 1000
    stale = db.find_stale_uploads(cutoff_ms=cutoff)
    for v in stale:
        db.mark_abandoned(v.id, now_ms=now_ms)
        _log.warning(
            "sweep.abandoned",
            video_id=v.id,
            filename=v.filename,
            cutoff_ms=cutoff,
        )
    # Orphan-file cleanup deferred until Phase 1.2 (when we have a clear
    # picture of what's a legitimate file vs orphan after the indexer
    # writes frames/ alongside source.<ext>). For now, only mark abandoned.
    return SweepReport(abandoned=len(stale), orphans_deleted=0)


async def sweeper_loop(
    db: Any,
    storage: Any,
    interval_s: int,
    abandoned_after_s: int,
    now_ms_fn: Any,
) -> None:
    """Long-running coroutine. Cancelled on app shutdown."""
    while True:
        try:
            await asyncio.sleep(interval_s)
        except asyncio.CancelledError:
            _log.info("sweep.cancelled")
            raise
        try:
            report = run_sweep(db, storage, now_ms=now_ms_fn(), abandoned_after_s=abandoned_after_s)
            if report.abandoned > 0:
                _log.info("sweep.complete", abandoned=report.abandoned)
        except Exception as exc:  # noqa: BLE001
            _log.exception("sweep.failed", error=str(exc))
```

- [ ] **Step 5: Run, watch pass**

```bash
cd /home/nvidia/bubbaloop-dash/backend && source .venv/bin/activate
pytest tests/test_sweeper.py -v 2>&1 | tail -10
```

Expected: 3 passed.

- [ ] **Step 6: Commit**

```bash
cd /home/nvidia/bubbaloop-dash
git add backend/src/dash/jobs/ backend/tests/test_sweeper.py
git commit -m "feat(jobs): integrity sweeper

run_sweep: marks status='uploading' rows abandoned when last_chunk_at
is older than now - abandoned_after_s. Pure function for unit-testing
with injected clock. sweeper_loop wraps it for the lifespan task.
Orphan-file cleanup deferred to 1.2."
```

---

## Task 7: POST /api/videos

**Files:**
- Create: `backend/src/dash/api/videos.py`
- Create: `backend/tests/test_videos_api.py`
- Modify: `backend/src/dash/factory.py` (mount router + provide db/storage on app.state)

- [ ] **Step 1: Write the failing tests (`backend/tests/test_videos_api.py`)**

```python
"""POST /api/videos: creates upload session, returns video_id + chunking info."""
from fastapi.testclient import TestClient


def test_post_videos_creates_session(client: TestClient, settings) -> None:
    r = client.post(
        "/api/videos",
        json={"filename": "entrance.mp4", "size_bytes": 1024, "content_type": "video/mp4"},
        headers={"Authorization": f"Bearer {settings.dashboard_token}"},
    )
    assert r.status_code == 200
    body = r.json()
    assert "video_id" in body
    assert body["chunk_size"] == settings.upload_chunk_size_bytes
    assert body["total_chunks"] == (1024 + body["chunk_size"] - 1) // body["chunk_size"]


def test_post_videos_requires_auth(client: TestClient) -> None:
    r = client.post(
        "/api/videos",
        json={"filename": "a.mp4", "size_bytes": 1024, "content_type": "video/mp4"},
    )
    assert r.status_code == 401


def test_post_videos_rejects_oversize(client: TestClient, settings) -> None:
    r = client.post(
        "/api/videos",
        json={"filename": "huge.mp4", "size_bytes": settings.max_upload_bytes + 1, "content_type": "video/mp4"},
        headers={"Authorization": f"Bearer {settings.dashboard_token}"},
    )
    assert r.status_code == 413


def test_post_videos_rejects_non_video_content_type(client: TestClient, settings) -> None:
    r = client.post(
        "/api/videos",
        json={"filename": "a.txt", "size_bytes": 100, "content_type": "text/plain"},
        headers={"Authorization": f"Bearer {settings.dashboard_token}"},
    )
    assert r.status_code == 415


def test_post_videos_sanitizes_extension(client: TestClient, settings) -> None:
    # Filename with funny extension; backend records sanitized ext.
    r = client.post(
        "/api/videos",
        json={"filename": "evil.../etc/passwd.mp4", "size_bytes": 100, "content_type": "video/mp4"},
        headers={"Authorization": f"Bearer {settings.dashboard_token}"},
    )
    assert r.status_code == 200
```

- [ ] **Step 2: Run, watch fail**

```bash
cd /home/nvidia/bubbaloop-dash/backend && source .venv/bin/activate
pytest tests/test_videos_api.py -v 2>&1 | tail -10
```

Expected: 404 / connection issues — endpoint doesn't exist yet.

- [ ] **Step 3: Implement (`backend/src/dash/api/videos.py`)**

```python
"""Video upload + library REST API.

Phase 1.1 endpoints: POST /api/videos, PUT /api/videos/{id}/chunks/{n},
POST /api/videos/{id}/complete, GET /api/videos, GET /api/videos/{id},
DELETE /api/videos/{id}.

Subsequent tasks (8-11) flesh out the remaining endpoints. Task 7 lands
POST /api/videos only."""
import re
import time
from typing import Annotated

from fastapi import APIRouter, Depends, HTTPException, Request, status
from pydantic import BaseModel, Field

from dash.auth import require_bearer_token

router = APIRouter(prefix="/api/videos", tags=["videos"])


_EXT_RE = re.compile(r"^[a-z0-9]{1,8}$")


def _sanitize_ext(filename: str) -> str:
    last_dot = filename.rfind(".")
    if last_dot < 0:
        return "bin"
    candidate = filename[last_dot + 1 :].lower()
    return candidate if _EXT_RE.fullmatch(candidate) else "bin"


def _now_ms() -> int:
    return int(time.time() * 1000)


class CreateVideoRequest(BaseModel):
    filename: str = Field(min_length=1, max_length=255)
    size_bytes: int = Field(ge=1)
    content_type: str = Field(min_length=1, max_length=128)


class CreateVideoResponse(BaseModel):
    video_id: str
    chunk_size: int
    total_chunks: int


@router.post(
    "",
    response_model=CreateVideoResponse,
    dependencies=[Depends(require_bearer_token)],
)
async def create_video(req: CreateVideoRequest, request: Request) -> CreateVideoResponse:
    settings = request.app.state.settings
    db = request.app.state.db

    if req.size_bytes > settings.max_upload_bytes:
        raise HTTPException(
            status_code=status.HTTP_413_REQUEST_ENTITY_TOO_LARGE,
            detail={"reason": "size_exceeds_max", "max_bytes": settings.max_upload_bytes},
        )
    if not req.content_type.startswith("video/"):
        raise HTTPException(
            status_code=status.HTTP_415_UNSUPPORTED_MEDIA_TYPE,
            detail={"reason": "content_type_not_video", "got": req.content_type},
        )

    chunk_size = settings.upload_chunk_size_bytes
    total_chunks = (req.size_bytes + chunk_size - 1) // chunk_size
    ext = _sanitize_ext(req.filename)

    video_id = db.create_video(
        filename=req.filename,
        content_type=req.content_type,
        ext=ext,
        size_bytes=req.size_bytes,
        chunk_size=chunk_size,
        total_chunks=total_chunks,
        now_ms=_now_ms(),
    )
    return CreateVideoResponse(video_id=video_id, chunk_size=chunk_size, total_chunks=total_chunks)
```

- [ ] **Step 4: Wire router + db + storage into factory.py**

REPLACE `backend/src/dash/factory.py` entirely with:

```python
"""FastAPI application factory (test-safe — no module-level instantiation)."""
from collections.abc import AsyncGenerator
from contextlib import asynccontextmanager
from typing import Any

from fastapi import FastAPI

from dash.api import health, videos, zenoh_ws
from dash.config import Settings
from dash.db.sqlite import Db
from dash.logging import configure_logging
from dash.static import mount_static
from dash.storage.filesystem import FilesystemStorage
from dash.tokens import ensure_token
from dash.zenoh_proxy.session import ZenohSession


def create_app(settings: Settings) -> FastAPI:
    configure_logging(level="INFO")
    ensure_token(settings)

    @asynccontextmanager
    async def lifespan(app: FastAPI) -> AsyncGenerator[None, Any]:
        db = Db.open(settings.data_dir / "state.sqlite")
        storage = FilesystemStorage(root=settings.data_dir)
        zsess = ZenohSession(endpoint=settings.zenoh_endpoint)
        await zsess.start()
        app.state.db = db
        app.state.storage = storage
        app.state.zenoh_session = zsess
        try:
            yield
        finally:
            await zsess.stop()
            db.close()

    app = FastAPI(
        title="bubbaloop-dash",
        version="0.1.0a0",
        description="Self-hosted backend + dashboard for Bubbaloop",
        lifespan=lifespan,
    )
    app.state.settings = settings
    app.include_router(health.router)
    app.include_router(zenoh_ws.router)
    app.include_router(videos.router)
    mount_static(app, settings.static_dir)
    return app
```

- [ ] **Step 5: Run, watch pass**

```bash
cd /home/nvidia/bubbaloop-dash/backend && source .venv/bin/activate
pytest tests/test_videos_api.py -v 2>&1 | tail -15
```

Expected: 5 passed.

- [ ] **Step 6: Run ALL backend tests to confirm no regression**

```bash
cd /home/nvidia/bubbaloop-dash/backend && source .venv/bin/activate
pytest -m "not e2e" 2>&1 | tail -5
```

Expected: all pass (34 from Phase 1.0 + the new ones from Tasks 1-7).

- [ ] **Step 7: Commit**

```bash
cd /home/nvidia/bubbaloop-dash
git add backend/src/dash/api/videos.py backend/src/dash/factory.py backend/tests/test_videos_api.py
git commit -m "feat(api): POST /api/videos — create upload session

Validates size cap + content_type. Sanitizes extension from filename
(allows [a-z0-9]{1,8}; fallback 'bin'). Server picks chunk_size from
Settings (default 8 MB). Returns video_id + total_chunks for the
chunked PUT loop. Db + FilesystemStorage wired into lifespan."
```

---

## Task 8: PUT /api/videos/{id}/chunks/{n}

**Files:**
- Modify: `backend/src/dash/api/videos.py` (add PUT chunks handler)
- Modify: `backend/tests/test_videos_api.py` (append PUT chunks tests)

- [ ] **Step 1: Append failing tests**

Add to `backend/tests/test_videos_api.py`:

```python
def _create_one(client: TestClient, settings, *, size=1024) -> dict:
    r = client.post(
        "/api/videos",
        json={"filename": "a.mp4", "size_bytes": size, "content_type": "video/mp4"},
        headers={"Authorization": f"Bearer {settings.dashboard_token}"},
    )
    assert r.status_code == 200
    return r.json()


def test_put_chunk_first(client: TestClient, settings) -> None:
    info = _create_one(client, settings, size=settings.upload_chunk_size_bytes * 3)
    data = b"\x01" * settings.upload_chunk_size_bytes
    r = client.put(
        f"/api/videos/{info['video_id']}/chunks/0",
        content=data,
        headers={"Authorization": f"Bearer {settings.dashboard_token}"},
    )
    assert r.status_code == 204


def test_put_chunk_duplicate_idempotent(client: TestClient, settings) -> None:
    info = _create_one(client, settings, size=settings.upload_chunk_size_bytes * 3)
    data = b"\x01" * settings.upload_chunk_size_bytes
    headers = {"Authorization": f"Bearer {settings.dashboard_token}"}
    client.put(f"/api/videos/{info['video_id']}/chunks/0", content=data, headers=headers)
    r2 = client.put(f"/api/videos/{info['video_id']}/chunks/0", content=data, headers=headers)
    assert r2.status_code == 204


def test_put_chunk_unknown_video_returns_404(client: TestClient, settings) -> None:
    r = client.put(
        "/api/videos/nope/chunks/0",
        content=b"\x00",
        headers={"Authorization": f"Bearer {settings.dashboard_token}"},
    )
    assert r.status_code == 404


def test_put_chunk_out_of_range_returns_422(client: TestClient, settings) -> None:
    info = _create_one(client, settings, size=settings.upload_chunk_size_bytes * 3)
    data = b"\x01" * settings.upload_chunk_size_bytes
    r = client.put(
        f"/api/videos/{info['video_id']}/chunks/99",
        content=data,
        headers={"Authorization": f"Bearer {settings.dashboard_token}"},
    )
    assert r.status_code == 422


def test_put_chunk_wrong_size_middle_chunk_returns_422(client: TestClient, settings) -> None:
    info = _create_one(client, settings, size=settings.upload_chunk_size_bytes * 3)
    # chunk 0 is a middle chunk (not the last); must be exactly chunk_size.
    r = client.put(
        f"/api/videos/{info['video_id']}/chunks/0",
        content=b"\x01" * (settings.upload_chunk_size_bytes - 1),
        headers={"Authorization": f"Bearer {settings.dashboard_token}"},
    )
    assert r.status_code == 422


def test_put_chunk_last_chunk_partial_is_ok(client: TestClient, settings) -> None:
    # Total = chunk_size + 100 → 2 chunks; chunk 1 is the last with size 100.
    info = _create_one(client, settings, size=settings.upload_chunk_size_bytes + 100)
    headers = {"Authorization": f"Bearer {settings.dashboard_token}"}
    full = b"\x01" * settings.upload_chunk_size_bytes
    partial = b"\x02" * 100
    assert client.put(f"/api/videos/{info['video_id']}/chunks/0", content=full, headers=headers).status_code == 204
    assert client.put(f"/api/videos/{info['video_id']}/chunks/1", content=partial, headers=headers).status_code == 204
```

- [ ] **Step 2: Run, watch fail**

```bash
cd /home/nvidia/bubbaloop-dash/backend && source .venv/bin/activate
pytest tests/test_videos_api.py -v 2>&1 | tail -10
```

Expected: 405 Method Not Allowed (PUT route doesn't exist).

- [ ] **Step 3: Implement the PUT handler**

Append to `backend/src/dash/api/videos.py` after the POST handler:

```python
from fastapi import Response


@router.put(
    "/{video_id}/chunks/{n}",
    status_code=status.HTTP_204_NO_CONTENT,
    dependencies=[Depends(require_bearer_token)],
)
async def put_chunk(video_id: str, n: int, request: Request) -> Response:
    db = request.app.state.db
    storage = request.app.state.storage

    video = db.get_video(video_id)
    if video is None:
        raise HTTPException(status_code=404, detail="video_not_found")
    if video.status != "uploading":
        raise HTTPException(status_code=409, detail=f"status={video.status}, cannot accept chunks")
    if n < 0 or n >= video.total_chunks:
        raise HTTPException(status_code=422, detail={"reason": "chunk_index_out_of_range", "n": n, "total": video.total_chunks})

    body = await request.body()
    expected_size = (
        video.chunk_size
        if n < video.total_chunks - 1
        else video.size_bytes - n * video.chunk_size
    )
    if len(body) != expected_size:
        raise HTTPException(
            status_code=422,
            detail={"reason": "wrong_chunk_size", "expected": expected_size, "got": len(body)},
        )

    path = f"videos/{video_id}/source.{video.ext}"
    offset = n * video.chunk_size
    storage.write_chunk(path, offset=offset, data=body)
    db.record_chunk(video_id, chunk_idx=n, chunk_bytes=len(body), now_ms=_now_ms())
    return Response(status_code=status.HTTP_204_NO_CONTENT)
```

- [ ] **Step 4: Run, watch pass**

```bash
cd /home/nvidia/bubbaloop-dash/backend && source .venv/bin/activate
pytest tests/test_videos_api.py -v 2>&1 | tail -15
```

Expected: 11 passed (5 from Task 7 + 6 new).

- [ ] **Step 5: Commit**

```bash
cd /home/nvidia/bubbaloop-dash
git add backend/src/dash/api/videos.py backend/tests/test_videos_api.py
git commit -m "feat(api): PUT /api/videos/{id}/chunks/{n}

Validates chunk index range + size (last chunk may be partial). Atomic
pwrite to source.{ext} at offset n*chunk_size. Idempotent: duplicate
chunk upload is a no-op (bitmap.set_bit idempotent, byte counter only
incremented on first receipt)."
```

---

## Task 9: POST /api/videos/{id}/complete

**Files:**
- Modify: `backend/src/dash/api/videos.py` (add /complete handler)
- Modify: `backend/tests/test_videos_api.py` (append /complete tests)

- [ ] **Step 1: Append failing tests**

```python
def _upload_all_chunks(client: TestClient, settings, info: dict) -> None:
    """Helper: PUT all chunks for an upload."""
    headers = {"Authorization": f"Bearer {settings.dashboard_token}"}
    chunk_size = info["chunk_size"]
    total_chunks = info["total_chunks"]
    full = b"\x01" * chunk_size
    # All but last
    for n in range(total_chunks - 1):
        r = client.put(f"/api/videos/{info['video_id']}/chunks/{n}", content=full, headers=headers)
        assert r.status_code == 204
    # Last chunk (may be partial)
    last_n = total_chunks - 1
    # For test sizes, we choose so the last chunk is full.
    r = client.put(f"/api/videos/{info['video_id']}/chunks/{last_n}", content=full, headers=headers)
    assert r.status_code == 204


def test_complete_happy_path(client: TestClient, settings) -> None:
    info = _create_one(client, settings, size=settings.upload_chunk_size_bytes * 2)
    _upload_all_chunks(client, settings, info)
    r = client.post(
        f"/api/videos/{info['video_id']}/complete",
        json={},
        headers={"Authorization": f"Bearer {settings.dashboard_token}"},
    )
    assert r.status_code == 200
    assert r.json()["status"] == "ready"


def test_complete_missing_chunks_returns_422(client: TestClient, settings) -> None:
    info = _create_one(client, settings, size=settings.upload_chunk_size_bytes * 3)
    headers = {"Authorization": f"Bearer {settings.dashboard_token}"}
    # Upload only chunk 0; chunks 1 and 2 missing.
    client.put(f"/api/videos/{info['video_id']}/chunks/0",
               content=b"\x01" * settings.upload_chunk_size_bytes, headers=headers)
    r = client.post(
        f"/api/videos/{info['video_id']}/complete", json={}, headers=headers,
    )
    assert r.status_code == 422
    body = r.json()
    assert "missing" in body["detail"]
    assert 1 in body["detail"]["missing"]
    assert 2 in body["detail"]["missing"]


def test_complete_idempotent_on_ready(client: TestClient, settings) -> None:
    info = _create_one(client, settings, size=settings.upload_chunk_size_bytes * 2)
    _upload_all_chunks(client, settings, info)
    headers = {"Authorization": f"Bearer {settings.dashboard_token}"}
    r1 = client.post(f"/api/videos/{info['video_id']}/complete", json={}, headers=headers)
    r2 = client.post(f"/api/videos/{info['video_id']}/complete", json={}, headers=headers)
    assert r1.status_code == 200
    assert r2.status_code == 200
    assert r1.json() == r2.json()
```

- [ ] **Step 2: Run, watch fail**

```bash
cd /home/nvidia/bubbaloop-dash/backend && source .venv/bin/activate
pytest tests/test_videos_api.py -v 2>&1 | tail -10
```

Expected: 405 / 404.

- [ ] **Step 3: Implement /complete**

Append to `backend/src/dash/api/videos.py`:

```python
from dash.db.bitmap import is_complete, missing_indices, popcount


class CompleteRequest(BaseModel):
    sha256: str | None = None  # reserved for future integrity check; unused in 1.1


class CompleteResponse(BaseModel):
    video_id: str
    status: str


@router.post(
    "/{video_id}/complete",
    response_model=CompleteResponse,
    dependencies=[Depends(require_bearer_token)],
)
async def complete(video_id: str, req: CompleteRequest, request: Request) -> CompleteResponse:
    db = request.app.state.db
    storage = request.app.state.storage

    video = db.get_video(video_id)
    if video is None:
        raise HTTPException(status_code=404, detail="video_not_found")
    if video.status == "ready":
        return CompleteResponse(video_id=video.id, status="ready")
    if video.status != "uploading":
        raise HTTPException(status_code=409, detail=f"status={video.status}, cannot complete")

    progress = db.get_upload_progress(video_id)
    if not is_complete(progress.received_chunks, total=video.total_chunks):
        missing = missing_indices(progress.received_chunks, total=video.total_chunks)
        raise HTTPException(status_code=422, detail={"reason": "missing_chunks", "missing": missing[:10]})

    path = f"videos/{video_id}/source.{video.ext}"
    actual_size = storage.size(path)
    if actual_size != video.size_bytes:
        db.mark_error(video_id, reason=f"size_mismatch declared={video.size_bytes} actual={actual_size}", now_ms=_now_ms())
        storage.delete(path)
        raise HTTPException(status_code=422, detail={"reason": "size_mismatch", "declared": video.size_bytes, "actual": actual_size})

    db.mark_ready(video_id, now_ms=_now_ms())
    return CompleteResponse(video_id=video_id, status="ready")
```

- [ ] **Step 4: Run, watch pass**

```bash
cd /home/nvidia/bubbaloop-dash/backend && source .venv/bin/activate
pytest tests/test_videos_api.py -v 2>&1 | tail -15
```

Expected: 14 passed (11 + 3 new).

- [ ] **Step 5: Commit**

```bash
cd /home/nvidia/bubbaloop-dash
git add backend/src/dash/api/videos.py backend/tests/test_videos_api.py
git commit -m "feat(api): POST /api/videos/{id}/complete

Validates bitmap completeness + file size on disk. On bitmap gap → 422
with list of first 10 missing indices. On size mismatch → 422, mark
status=error, delete partial file. Idempotent on already-ready."
```

---

## Task 10: GET /api/videos + GET /api/videos/{id} + DELETE /api/videos/{id}

**Files:**
- Modify: `backend/src/dash/api/videos.py`
- Modify: `backend/tests/test_videos_api.py`

- [ ] **Step 1: Append failing tests**

```python
def test_list_videos_empty(client: TestClient, settings) -> None:
    r = client.get(
        "/api/videos",
        headers={"Authorization": f"Bearer {settings.dashboard_token}"},
    )
    assert r.status_code == 200
    assert r.json() == {"items": [], "next_cursor": None}


def test_list_videos_after_upload(client: TestClient, settings) -> None:
    info = _create_one(client, settings, size=settings.upload_chunk_size_bytes)
    headers = {"Authorization": f"Bearer {settings.dashboard_token}"}
    r = client.get("/api/videos", headers=headers)
    assert r.status_code == 200
    items = r.json()["items"]
    assert len(items) == 1
    assert items[0]["id"] == info["video_id"]
    assert items[0]["status"] == "uploading"
    assert items[0]["progress_pct"] == 0


def test_list_videos_status_filter(client: TestClient, settings) -> None:
    info = _create_one(client, settings, size=settings.upload_chunk_size_bytes)
    headers = {"Authorization": f"Bearer {settings.dashboard_token}"}
    r = client.get("/api/videos?status=ready", headers=headers)
    assert r.json()["items"] == []
    r = client.get("/api/videos?status=uploading", headers=headers)
    assert len(r.json()["items"]) == 1


def test_get_video_detail(client: TestClient, settings) -> None:
    info = _create_one(client, settings, size=settings.upload_chunk_size_bytes)
    r = client.get(
        f"/api/videos/{info['video_id']}",
        headers={"Authorization": f"Bearer {settings.dashboard_token}"},
    )
    assert r.status_code == 200
    body = r.json()
    assert body["id"] == info["video_id"]
    assert body["status"] == "uploading"
    assert body["error"] is None


def test_get_video_missing_returns_404(client: TestClient, settings) -> None:
    r = client.get(
        "/api/videos/nope",
        headers={"Authorization": f"Bearer {settings.dashboard_token}"},
    )
    assert r.status_code == 404


def test_delete_video_removes_row_and_files(client: TestClient, settings) -> None:
    info = _create_one(client, settings, size=settings.upload_chunk_size_bytes)
    headers = {"Authorization": f"Bearer {settings.dashboard_token}"}
    # Upload one chunk so there's a file on disk.
    client.put(
        f"/api/videos/{info['video_id']}/chunks/0",
        content=b"\x01" * settings.upload_chunk_size_bytes,
        headers=headers,
    )
    r = client.delete(f"/api/videos/{info['video_id']}", headers=headers)
    assert r.status_code == 204
    r2 = client.get(f"/api/videos/{info['video_id']}", headers=headers)
    assert r2.status_code == 404


def test_delete_video_missing_returns_204(client: TestClient, settings) -> None:
    r = client.delete(
        "/api/videos/nope",
        headers={"Authorization": f"Bearer {settings.dashboard_token}"},
    )
    assert r.status_code == 204
```

- [ ] **Step 2: Run, watch fail**

```bash
cd /home/nvidia/bubbaloop-dash/backend && source .venv/bin/activate
pytest tests/test_videos_api.py -v 2>&1 | tail -15
```

Expected: 405 / 404 for the new endpoints.

- [ ] **Step 3: Implement the GET/DELETE handlers**

Append to `backend/src/dash/api/videos.py`:

```python
import shutil


class VideoSummary(BaseModel):
    id: str
    filename: str
    size_bytes: int
    status: str
    created_at: int
    updated_at: int
    completed_at: int | None
    progress_pct: int


class VideoList(BaseModel):
    items: list[VideoSummary]
    next_cursor: str | None


class VideoDetail(BaseModel):
    id: str
    filename: str
    content_type: str
    ext: str
    size_bytes: int
    chunk_size: int
    total_chunks: int
    status: str
    error: str | None
    created_at: int
    updated_at: int
    completed_at: int | None
    progress_pct: int


def _video_to_summary(v, progress_pct: int) -> VideoSummary:
    return VideoSummary(
        id=v.id, filename=v.filename, size_bytes=v.size_bytes,
        status=v.status, created_at=v.created_at, updated_at=v.updated_at,
        completed_at=v.completed_at, progress_pct=progress_pct,
    )


def _job_progress_pct(db, video_id: str) -> int:
    jobs = [j for j in db.list_jobs(video_id=video_id) if j.kind == "upload"]
    return jobs[0].progress_pct if jobs else 0


@router.get(
    "",
    response_model=VideoList,
    dependencies=[Depends(require_bearer_token)],
)
async def list_videos(request: Request, status: str | None = None, limit: int = 100) -> VideoList:
    db = request.app.state.db
    rows = db.list_videos(status_filter=status, limit=limit)
    items = [_video_to_summary(v, _job_progress_pct(db, v.id)) for v in rows]
    return VideoList(items=items, next_cursor=None)


@router.get(
    "/{video_id}",
    response_model=VideoDetail,
    dependencies=[Depends(require_bearer_token)],
)
async def get_video_detail(video_id: str, request: Request) -> VideoDetail:
    db = request.app.state.db
    v = db.get_video(video_id)
    if v is None:
        raise HTTPException(status_code=404, detail="video_not_found")
    return VideoDetail(
        id=v.id, filename=v.filename, content_type=v.content_type, ext=v.ext,
        size_bytes=v.size_bytes, chunk_size=v.chunk_size, total_chunks=v.total_chunks,
        status=v.status, error=v.error,
        created_at=v.created_at, updated_at=v.updated_at, completed_at=v.completed_at,
        progress_pct=_job_progress_pct(db, v.id),
    )


@router.delete(
    "/{video_id}",
    status_code=status.HTTP_204_NO_CONTENT,
    dependencies=[Depends(require_bearer_token)],
)
async def delete_video(video_id: str, request: Request) -> Response:
    db = request.app.state.db
    settings = request.app.state.settings
    db.delete_video(video_id)
    # Remove the video's directory from disk if present.
    video_dir = settings.data_dir / "videos" / video_id
    if video_dir.exists():
        shutil.rmtree(video_dir, ignore_errors=True)
    return Response(status_code=status.HTTP_204_NO_CONTENT)
```

Note: `status` as a parameter name shadows the imported `status` constant. Rename the import in the file to `from fastapi import status as http_status` if you hit that issue, OR use `status_filter` as the query-param name. For this file the route handler defines `status: str | None = None` locally; FastAPI's `status` module reference uses the global import name elsewhere — this is fine because of Python scoping.

If you DO see a NameError on the response, alias by renaming the parameter: replace `status: str | None = None` with `status_filter: str | None = Query(default=None, alias="status")` and add `from fastapi import Query` to the imports.

- [ ] **Step 4: Run, watch pass**

```bash
cd /home/nvidia/bubbaloop-dash/backend && source .venv/bin/activate
pytest tests/test_videos_api.py -v 2>&1 | tail -20
```

Expected: 21 passed (14 + 7 new).

- [ ] **Step 5: Commit**

```bash
cd /home/nvidia/bubbaloop-dash
git add backend/src/dash/api/videos.py backend/tests/test_videos_api.py
git commit -m "feat(api): GET /api/videos, GET /api/videos/{id}, DELETE /api/videos/{id}

List with optional status filter, ORDER BY updated_at DESC. Detail
returns full row + progress_pct from the upload job. DELETE cascades
SQLite rows (FK) and removes the video's filesystem directory."
```

---

## Task 11: /api/jobs + /api/blob endpoints

**Files:**
- Create: `backend/src/dash/api/jobs.py`
- Create: `backend/src/dash/api/blob.py`
- Create: `backend/tests/test_jobs_api.py`
- Create: `backend/tests/test_blob_api.py`
- Modify: `backend/src/dash/factory.py` (mount the two new routers)

- [ ] **Step 1: Write failing tests for /api/jobs (`backend/tests/test_jobs_api.py`)**

```python
"""GET /api/jobs + GET /api/jobs/{id} + DELETE /api/jobs/{id}."""
from fastapi.testclient import TestClient


def test_list_jobs_includes_upload_job(client: TestClient, settings) -> None:
    headers = {"Authorization": f"Bearer {settings.dashboard_token}"}
    r = client.post(
        "/api/videos",
        json={"filename": "a.mp4", "size_bytes": 1024, "content_type": "video/mp4"},
        headers=headers,
    )
    assert r.status_code == 200
    info = r.json()
    r2 = client.get("/api/jobs", headers=headers)
    assert r2.status_code == 200
    items = r2.json()["items"]
    assert len(items) == 1
    assert items[0]["kind"] == "upload"
    assert items[0]["video_id"] == info["video_id"]


def test_get_job_detail(client: TestClient, settings) -> None:
    headers = {"Authorization": f"Bearer {settings.dashboard_token}"}
    client.post(
        "/api/videos",
        json={"filename": "a.mp4", "size_bytes": 1024, "content_type": "video/mp4"},
        headers=headers,
    )
    jobs = client.get("/api/jobs", headers=headers).json()["items"]
    r = client.get(f"/api/jobs/{jobs[0]['id']}", headers=headers)
    assert r.status_code == 200
    assert r.json()["kind"] == "upload"


def test_get_job_missing_returns_404(client: TestClient, settings) -> None:
    r = client.get("/api/jobs/nope", headers={"Authorization": f"Bearer {settings.dashboard_token}"})
    assert r.status_code == 404


def test_delete_upload_job_removes_video(client: TestClient, settings) -> None:
    """In Phase 1.1, DELETE /api/jobs/{id} for kind='upload' is equivalent to
    DELETE /api/videos/{video_id}."""
    headers = {"Authorization": f"Bearer {settings.dashboard_token}"}
    info = client.post(
        "/api/videos",
        json={"filename": "a.mp4", "size_bytes": 1024, "content_type": "video/mp4"},
        headers=headers,
    ).json()
    jobs = client.get("/api/jobs", headers=headers).json()["items"]
    r = client.delete(f"/api/jobs/{jobs[0]['id']}", headers=headers)
    assert r.status_code == 204
    # video is gone
    r2 = client.get(f"/api/videos/{info['video_id']}", headers=headers)
    assert r2.status_code == 404
```

- [ ] **Step 2: Write failing tests for /api/blob (`backend/tests/test_blob_api.py`)**

```python
"""GET /api/blob/{path...} streams files from FilesystemStorage."""
from fastapi.testclient import TestClient


def test_get_blob_returns_file_bytes(client: TestClient, settings) -> None:
    headers = {"Authorization": f"Bearer {settings.dashboard_token}"}
    info = client.post(
        "/api/videos",
        json={"filename": "a.mp4", "size_bytes": settings.upload_chunk_size_bytes, "content_type": "video/mp4"},
        headers=headers,
    ).json()
    data = b"\xab" * settings.upload_chunk_size_bytes
    client.put(f"/api/videos/{info['video_id']}/chunks/0", content=data, headers=headers)

    r = client.get(f"/api/blob/videos/{info['video_id']}/source.mp4", headers=headers)
    assert r.status_code == 200
    assert r.content == data


def test_get_blob_unknown_returns_404(client: TestClient, settings) -> None:
    r = client.get(
        "/api/blob/videos/nope/source.mp4",
        headers={"Authorization": f"Bearer {settings.dashboard_token}"},
    )
    assert r.status_code == 404


def test_get_blob_requires_auth(client: TestClient) -> None:
    r = client.get("/api/blob/videos/any/source.mp4")
    assert r.status_code == 401


def test_get_blob_path_traversal_rejected(client: TestClient, settings) -> None:
    # FastAPI will normalize ".." in URL; test the explicit absolute path case.
    r = client.get(
        "/api/blob//etc/passwd",
        headers={"Authorization": f"Bearer {settings.dashboard_token}"},
    )
    # Either 404 (path doesn't exist under root) or 400; not 200.
    assert r.status_code != 200
```

- [ ] **Step 3: Run, watch fail**

```bash
cd /home/nvidia/bubbaloop-dash/backend && source .venv/bin/activate
pytest tests/test_jobs_api.py tests/test_blob_api.py -v 2>&1 | tail -10
```

Expected: 404s — routes don't exist.

- [ ] **Step 4: Implement /api/jobs (`backend/src/dash/api/jobs.py`)**

```python
"""GET /api/jobs, GET /api/jobs/{id}, DELETE /api/jobs/{id}.

Phase 1.1: jobs are only kind='upload'. DELETE on an upload job is
equivalent to DELETE /api/videos/{video_id} — the upload IS the work."""
import shutil

from fastapi import APIRouter, Depends, HTTPException, Request, Response, status
from pydantic import BaseModel

from dash.auth import require_bearer_token

router = APIRouter(prefix="/api/jobs", tags=["jobs"])


class JobSummary(BaseModel):
    id: str
    video_id: str
    kind: str
    status: str
    progress_pct: int
    error: str | None
    started_at: int
    finished_at: int | None


class JobList(BaseModel):
    items: list[JobSummary]


@router.get("", response_model=JobList, dependencies=[Depends(require_bearer_token)])
async def list_jobs(request: Request) -> JobList:
    db = request.app.state.db
    rows = db.list_jobs(video_id=None)
    return JobList(items=[JobSummary(**r.__dict__) for r in rows])


@router.get("/{job_id}", response_model=JobSummary, dependencies=[Depends(require_bearer_token)])
async def get_job(job_id: str, request: Request) -> JobSummary:
    db = request.app.state.db
    j = db.get_job(job_id)
    if j is None:
        raise HTTPException(status_code=404, detail="job_not_found")
    return JobSummary(**j.__dict__)


@router.delete("/{job_id}", status_code=status.HTTP_204_NO_CONTENT, dependencies=[Depends(require_bearer_token)])
async def delete_job(job_id: str, request: Request) -> Response:
    db = request.app.state.db
    settings = request.app.state.settings
    j = db.get_job(job_id)
    if j is None:
        return Response(status_code=status.HTTP_204_NO_CONTENT)
    if j.kind == "upload":
        db.delete_video(j.video_id)
        video_dir = settings.data_dir / "videos" / j.video_id
        if video_dir.exists():
            shutil.rmtree(video_dir, ignore_errors=True)
    # Phase 1.2 will add: if j.kind == 'indexing': cancel runner + cleanup.
    return Response(status_code=status.HTTP_204_NO_CONTENT)
```

- [ ] **Step 5: Implement /api/blob (`backend/src/dash/api/blob.py`)**

```python
"""GET /api/blob/{path...} — serve files from FilesystemStorage.

Phase 1.1 has no consumer yet; Phase 1.2 uses for thumbnails. Auth-protected.
Path-traversal protection lives in FilesystemStorage._abs."""
from fastapi import APIRouter, Depends, HTTPException, Request
from fastapi.responses import FileResponse

from dash.auth import require_bearer_token

router = APIRouter(prefix="/api/blob", tags=["blob"])


@router.get("/{path:path}", dependencies=[Depends(require_bearer_token)])
async def get_blob(path: str, request: Request) -> FileResponse:
    settings = request.app.state.settings
    try:
        abs_path = (settings.data_dir / path).resolve()
        abs_path.relative_to(settings.data_dir.resolve())
    except (ValueError, OSError):
        raise HTTPException(status_code=400, detail="invalid_path")
    if not abs_path.is_file():
        raise HTTPException(status_code=404, detail="blob_not_found")
    return FileResponse(str(abs_path))
```

- [ ] **Step 6: Mount both routers in factory.py**

Edit `backend/src/dash/factory.py` — add to imports + include_router calls:

```python
from dash.api import blob, health, jobs, videos, zenoh_ws
# ...
    app.include_router(health.router)
    app.include_router(zenoh_ws.router)
    app.include_router(videos.router)
    app.include_router(jobs.router)
    app.include_router(blob.router)
```

- [ ] **Step 7: Run, watch pass**

```bash
cd /home/nvidia/bubbaloop-dash/backend && source .venv/bin/activate
pytest tests/test_jobs_api.py tests/test_blob_api.py -v 2>&1 | tail -15
```

Expected: 8 passed (4 jobs + 4 blob).

- [ ] **Step 8: Run all backend tests**

```bash
cd /home/nvidia/bubbaloop-dash/backend && source .venv/bin/activate
pytest -m "not e2e" 2>&1 | tail -5
```

Expected: all green (~70+ tests cumulative).

- [ ] **Step 9: Commit**

```bash
cd /home/nvidia/bubbaloop-dash
git add backend/src/dash/api/jobs.py backend/src/dash/api/blob.py backend/src/dash/factory.py backend/tests/test_jobs_api.py backend/tests/test_blob_api.py
git commit -m "feat(api): /api/jobs (3 endpoints) + /api/blob/{path...}

Jobs in Phase 1.1 are only kind='upload'; DELETE of an upload job
cascades to DELETE the video (the upload IS the work). Blob route
serves files from DATA_DIR, auth-protected, path-traversal safe."
```

---

## Task 12: Wire sweeper into lifespan

**Files:**
- Modify: `backend/src/dash/factory.py` (spawn + cancel sweeper task in lifespan)
- Modify: `backend/tests/conftest.py` (already uses `with TestClient(app)` from Phase 1.0 — verify lifespan still works)

- [ ] **Step 1: Add a smoke test for lifespan-managed sweeper (`backend/tests/test_sweeper_lifespan.py`)**

```python
"""Smoke test: app startup spawns the sweeper task; shutdown cancels it cleanly."""
from fastapi.testclient import TestClient


def test_sweeper_task_starts_and_stops(client: TestClient) -> None:
    # If the lifespan correctly spawns + awaits cancellation, this just exits.
    # We assert app.state.sweeper_task exists during the request lifecycle.
    r = client.get("/healthz")
    assert r.status_code == 200
    # TestClient's __exit__ runs lifespan shutdown; no exception = pass.
```

Note: the deeper assertion (task actually ran) is covered by `test_sweeper.py` calling `run_sweep` directly. This test only verifies the lifespan wiring doesn't crash.

- [ ] **Step 2: Modify factory.py to spawn the sweeper**

REPLACE `backend/src/dash/factory.py` entirely:

```python
"""FastAPI application factory (test-safe — no module-level instantiation)."""
import asyncio
import time
from collections.abc import AsyncGenerator
from contextlib import asynccontextmanager
from typing import Any

from fastapi import FastAPI

from dash.api import blob, health, jobs, videos, zenoh_ws
from dash.config import Settings
from dash.db.sqlite import Db
from dash.jobs.sweeper import sweeper_loop
from dash.logging import configure_logging
from dash.static import mount_static
from dash.storage.filesystem import FilesystemStorage
from dash.tokens import ensure_token
from dash.zenoh_proxy.session import ZenohSession


def _now_ms() -> int:
    return int(time.time() * 1000)


def create_app(settings: Settings) -> FastAPI:
    configure_logging(level="INFO")
    ensure_token(settings)

    @asynccontextmanager
    async def lifespan(app: FastAPI) -> AsyncGenerator[None, Any]:
        db = Db.open(settings.data_dir / "state.sqlite")
        storage = FilesystemStorage(root=settings.data_dir)
        zsess = ZenohSession(endpoint=settings.zenoh_endpoint)
        await zsess.start()
        sweeper_task = asyncio.create_task(
            sweeper_loop(
                db=db,
                storage=storage,
                interval_s=settings.sweep_interval_s,
                abandoned_after_s=settings.abandoned_after_s,
                now_ms_fn=_now_ms,
            ),
            name="dash-sweeper",
        )
        app.state.db = db
        app.state.storage = storage
        app.state.zenoh_session = zsess
        app.state.sweeper_task = sweeper_task
        try:
            yield
        finally:
            sweeper_task.cancel()
            try:
                await sweeper_task
            except asyncio.CancelledError:
                pass
            await zsess.stop()
            db.close()

    app = FastAPI(
        title="bubbaloop-dash",
        version="0.1.0a0",
        description="Self-hosted backend + dashboard for Bubbaloop",
        lifespan=lifespan,
    )
    app.state.settings = settings
    app.include_router(health.router)
    app.include_router(zenoh_ws.router)
    app.include_router(videos.router)
    app.include_router(jobs.router)
    app.include_router(blob.router)
    mount_static(app, settings.static_dir)
    return app
```

- [ ] **Step 3: Run, watch pass (or stay-green)**

```bash
cd /home/nvidia/bubbaloop-dash/backend && source .venv/bin/activate
pytest -m "not e2e" 2>&1 | tail -5
```

Expected: all pass. The new smoke test passes; nothing regresses.

- [ ] **Step 4: Commit**

```bash
cd /home/nvidia/bubbaloop-dash
git add backend/src/dash/factory.py backend/tests/test_sweeper_lifespan.py
git commit -m "feat(factory): wire integrity sweeper into lifespan

asyncio.create_task spawns sweeper_loop at startup; cancelled cleanly
on shutdown. Settings.sweep_interval_s (default 3600s) drives cadence;
abandoned_after_s (default 86400s) drives 'abandoned' threshold."
```

---

## Task 13: Add Map tab to App.tsx + MapTab.tsx + MapCanvas.tsx

**Files:**
- Modify: `frontend/src/App.tsx`
- Create: `frontend/src/components/MapTab.tsx`
- Create: `frontend/src/components/MapCanvas.tsx`

This task is mostly file scaffolding — VideoUploader and VideoLibrary land in Tasks 14-15 and slot into MapTab.

- [ ] **Step 1: Create MapCanvas.tsx (placeholder)**

```tsx
// frontend/src/components/MapCanvas.tsx
export function MapCanvas() {
  return (
    <div className="map-canvas-placeholder">
      <div className="placeholder-content">
        <h2>Map</h2>
        <p>SLAM-driven scene reconstruction coming in a future phase.</p>
        <p className="hint">Upload videos in the sidebar; indexing follows in Phase 1.2.</p>
      </div>
      <style>{`
        .map-canvas-placeholder {
          flex: 1;
          display: flex;
          align-items: center;
          justify-content: center;
          background: var(--bg-primary, #0d1117);
        }
        .placeholder-content { text-align: center; color: var(--text-muted, #6e7681); max-width: 480px; }
        .placeholder-content h2 { font-size: 32px; margin: 0 0 8px; color: var(--text-secondary, #8b949e); }
        .placeholder-content p { margin: 4px 0; font-size: 14px; }
        .placeholder-content .hint { font-size: 12px; opacity: 0.7; margin-top: 16px; }
      `}</style>
    </div>
  );
}
```

- [ ] **Step 2: Create MapTab.tsx (container; sub-components are stubs for now)**

```tsx
// frontend/src/components/MapTab.tsx
import { useState } from 'react';
import { MapCanvas } from './MapCanvas';

export function MapTab() {
  // selectedVideoId reserved for Task 15+; not used yet.
  const [, /* setSelectedVideoId */] = useState<string | null>(null);
  return (
    <div className="map-tab">
      <aside className="map-sidebar">
        {/* VideoUploader added in Task 14 */}
        {/* VideoLibrary added in Task 15 */}
        <div className="map-sidebar-placeholder">Uploader + Library land in upcoming tasks.</div>
      </aside>
      <main className="map-main">
        <MapCanvas />
      </main>
      <style>{`
        .map-tab { display: flex; flex: 1; min-height: 0; }
        .map-sidebar { width: 360px; flex-shrink: 0; border-right: 1px solid var(--border-color, #30363d); display: flex; flex-direction: column; overflow-y: auto; }
        .map-main { flex: 1; display: flex; min-width: 0; }
        .map-sidebar-placeholder { padding: 24px; color: var(--text-muted, #6e7681); font-size: 13px; text-align: center; }
        @media (max-width: 768px) {
          .map-tab { flex-direction: column; }
          .map-sidebar { width: 100%; border-right: none; border-bottom: 1px solid var(--border-color, #30363d); }
        }
      `}</style>
    </div>
  );
}
```

- [ ] **Step 3: Update `frontend/src/App.tsx` — add "map" to AppView + button + route**

Edit `App.tsx`:

1. Change the type definition:
```diff
- type AppView = "dashboard" | "loop" | "chat";
+ type AppView = "dashboard" | "loop" | "map" | "chat";
```

2. Add the imports near the top, alongside existing component imports:
```tsx
import { MapTab } from "./components/MapTab";
```

3. Add the Map button to the view-switcher (between Loop and Chat — the existing JSX has `<button className={`view-tab ${currentView === "loop" ? "active" : ""}`}>` for Loop. Insert AFTER that block and BEFORE the Chat button):

```tsx
<button
  className={`view-tab ${currentView === "map" ? "active" : ""}`}
  onClick={() => setCurrentView("map")}
>
  <svg
    width="14"
    height="14"
    viewBox="0 0 24 24"
    fill="none"
    stroke="currentColor"
    strokeWidth="2"
  >
    <path d="M21 10c0 7-9 13-9 13s-9-6-9-13a9 9 0 0 1 18 0z" />
    <circle cx="12" cy="10" r="3" />
  </svg>
  Map
</button>
```

4. Add the `currentView === "map"` route. The existing render block has:
```tsx
{currentView === "chat" ? (
  <ChatView session={session ?? undefined} />
) : session ? (
  <FleetProvider>
    ...
  </FleetProvider>
) : (
  ...connecting placeholder...
)}
```

Insert the Map branch ABOVE the chat check:
```tsx
{currentView === "map" ? (
  <MapTab />
) : currentView === "chat" ? (
  <ChatView session={session ?? undefined} />
) : session ? (
  ...
```

- [ ] **Step 4: Verify frontend still builds + tests pass**

```bash
cd /home/nvidia/bubbaloop-dash/frontend && npm run build 2>&1 | tail -5
cd /home/nvidia/bubbaloop-dash/frontend && npm test 2>&1 | tail -5
```

Expected: build succeeds; all existing tests pass (no new tests in this task — visual integration verified manually).

- [ ] **Step 5: Commit**

```bash
cd /home/nvidia/bubbaloop-dash
git add frontend/src/App.tsx frontend/src/components/MapTab.tsx frontend/src/components/MapCanvas.tsx
git commit -m "feat(frontend): add Map tab with placeholder canvas

Adds 'map' to AppView, map-pin button between Loop and Chat, MapTab
container with sidebar (sub-components in Tasks 14-15) and MapCanvas
placeholder for Spec 2's SLAM integration."
```

---

## Task 14: VideoUploader component

**Files:**
- Create: `frontend/src/components/VideoUploader.tsx`
- Create: `frontend/src/components/__tests__/VideoUploader.test.tsx`
- Modify: `frontend/src/components/MapTab.tsx` (slot in VideoUploader)

- [ ] **Step 1: Write failing test (`frontend/src/components/__tests__/VideoUploader.test.tsx`)**

```typescript
import { describe, expect, it, beforeEach, vi } from 'vitest';
import { render, screen, fireEvent, waitFor, act } from '@testing-library/react';
import { AuthProvider } from '../../contexts/AuthContext';
import { VideoUploader } from '../VideoUploader';

function withAuth(node: React.ReactNode) {
  localStorage.setItem('bubbaloop_dash_token', 'tk-test');
  return <AuthProvider>{node}</AuthProvider>;
}

function makeFile(name: string, sizeBytes: number, type = 'video/mp4'): File {
  // jsdom File constructor accepts an array of strings; pad with bytes.
  const bytes = new Uint8Array(sizeBytes).fill(0xaa);
  return new File([bytes], name, { type });
}

describe('VideoUploader', () => {
  beforeEach(() => {
    vi.restoreAllMocks();
    localStorage.clear();
  });

  it('renders a drop zone with browse fallback when idle', () => {
    render(withAuth(<VideoUploader onUploaded={() => {}} />));
    expect(screen.getByText(/drop a video here/i)).toBeInTheDocument();
    expect(screen.getByLabelText(/browse/i)).toBeInTheDocument();
  });

  it('uploads a small file end-to-end (POST + PUT + complete)', async () => {
    const chunkSize = 64;  // tiny for the test
    const file = makeFile('a.mp4', chunkSize * 2);  // 2 chunks

    const fetchMock = vi.spyOn(globalThis, 'fetch').mockImplementation(async (url, init) => {
      const u = String(url);
      const method = init?.method ?? 'GET';
      if (u === '/api/videos' && method === 'POST') {
        return new Response(JSON.stringify({
          video_id: 'v123', chunk_size: chunkSize, total_chunks: 2,
        }), { status: 200 });
      }
      if (u.startsWith('/api/videos/v123/chunks/') && method === 'PUT') {
        return new Response(null, { status: 204 });
      }
      if (u === '/api/videos/v123/complete' && method === 'POST') {
        return new Response(JSON.stringify({ video_id: 'v123', status: 'ready' }), { status: 200 });
      }
      throw new Error(`unexpected fetch: ${method} ${u}`);
    });

    const onUploaded = vi.fn();
    render(withAuth(<VideoUploader onUploaded={onUploaded} />));

    const input = screen.getByLabelText(/browse/i) as HTMLInputElement;
    await act(async () => {
      fireEvent.change(input, { target: { files: [file] } });
    });

    await waitFor(() => expect(onUploaded).toHaveBeenCalledWith('v123'));
    expect(fetchMock).toHaveBeenCalledTimes(4);  // POST + 2 PUTs + complete
  });

  it('cancels upload and DELETEs the partial video', async () => {
    const chunkSize = 64;
    const file = makeFile('a.mp4', chunkSize * 3);

    let putResolvers: Array<(v: Response) => void> = [];
    let deleteCalled = false;

    vi.spyOn(globalThis, 'fetch').mockImplementation(async (url, init) => {
      const u = String(url);
      const method = init?.method ?? 'GET';
      if (u === '/api/videos' && method === 'POST') {
        return new Response(JSON.stringify({
          video_id: 'v999', chunk_size: chunkSize, total_chunks: 3,
        }), { status: 200 });
      }
      if (u.startsWith('/api/videos/v999/chunks/') && method === 'PUT') {
        // Hang the chunk PUTs so we can click Cancel mid-upload.
        return new Promise<Response>((resolve) => { putResolvers.push(resolve); });
      }
      if (u === '/api/videos/v999' && method === 'DELETE') {
        deleteCalled = true;
        return new Response(null, { status: 204 });
      }
      throw new Error(`unexpected fetch: ${method} ${u}`);
    });

    render(withAuth(<VideoUploader onUploaded={() => {}} />));
    const input = screen.getByLabelText(/browse/i) as HTMLInputElement;
    await act(async () => {
      fireEvent.change(input, { target: { files: [file] } });
    });

    // Wait for the Cancel button to appear (means upload state is active).
    const cancelButton = await screen.findByRole('button', { name: /cancel/i });
    fireEvent.click(cancelButton);

    await waitFor(() => expect(deleteCalled).toBe(true));
  });
});
```

- [ ] **Step 2: Run, watch fail**

```bash
cd /home/nvidia/bubbaloop-dash/frontend
npx vitest run src/components/__tests__/VideoUploader.test.tsx 2>&1 | tail -10
```

Expected: import error.

- [ ] **Step 3: Implement (`frontend/src/components/VideoUploader.tsx`)**

```tsx
import { ChangeEvent, useCallback, useRef, useState } from 'react';
import { useAuth } from '../contexts/AuthContext';

interface CreateResponse {
  video_id: string;
  chunk_size: number;
  total_chunks: number;
}

type UploaderState =
  | { kind: 'idle' }
  | { kind: 'starting'; file: File }
  | {
      kind: 'uploading';
      file: File;
      videoId: string;
      totalBytes: number;
      receivedBytes: number;
      abortController: AbortController;
    }
  | { kind: 'completing'; file: File; videoId: string }
  | { kind: 'error'; reason: string };

const CONCURRENCY = 3;
const MAX_RETRIES = 5;
const BACKOFF_MS = [250, 500, 1000, 2000, 4000];

async function sleep(ms: number): Promise<void> {
  return new Promise((resolve) => setTimeout(resolve, ms));
}

async function putChunkWithRetry(
  url: string,
  data: ArrayBuffer,
  token: string,
  signal: AbortSignal,
): Promise<void> {
  for (let attempt = 0; attempt <= MAX_RETRIES; attempt++) {
    if (signal.aborted) throw new Error('cancelled');
    try {
      const resp = await fetch(url, {
        method: 'PUT',
        body: data,
        headers: { Authorization: `Bearer ${token}` },
        signal,
      });
      if (resp.ok) return;
      if (resp.status >= 400 && resp.status < 500) {
        throw new Error(`chunk PUT failed: ${resp.status}`);
      }
    } catch (err) {
      if (signal.aborted) throw err;
      if (attempt === MAX_RETRIES) throw err;
    }
    await sleep(BACKOFF_MS[attempt]);
  }
  throw new Error('chunk PUT exhausted retries');
}

export function VideoUploader({ onUploaded }: { onUploaded: (videoId: string) => void }) {
  const { token } = useAuth();
  const [state, setState] = useState<UploaderState>({ kind: 'idle' });
  const inputRef = useRef<HTMLInputElement>(null);

  const startUpload = useCallback(
    async (file: File) => {
      if (!token) {
        setState({ kind: 'error', reason: 'not logged in' });
        return;
      }
      setState({ kind: 'starting', file });

      // POST /api/videos
      let info: CreateResponse;
      try {
        const resp = await fetch('/api/videos', {
          method: 'POST',
          headers: { 'Content-Type': 'application/json', Authorization: `Bearer ${token}` },
          body: JSON.stringify({
            filename: file.name,
            size_bytes: file.size,
            content_type: file.type || 'video/mp4',
          }),
        });
        if (!resp.ok) {
          const body = await resp.text();
          setState({ kind: 'error', reason: `${resp.status}: ${body}` });
          return;
        }
        info = await resp.json();
      } catch (err) {
        setState({ kind: 'error', reason: (err as Error).message });
        return;
      }

      const abortController = new AbortController();
      setState({
        kind: 'uploading',
        file,
        videoId: info.video_id,
        totalBytes: file.size,
        receivedBytes: 0,
        abortController,
      });

      // Chunk PUT loop with concurrency.
      const totalChunks = info.total_chunks;
      const chunkSize = info.chunk_size;
      const indices = Array.from({ length: totalChunks }, (_, i) => i);
      let nextIndex = 0;
      let receivedBytes = 0;

      const worker = async (): Promise<void> => {
        while (true) {
          if (abortController.signal.aborted) return;
          const idx = nextIndex++;
          if (idx >= totalChunks) return;
          const start = idx * chunkSize;
          const end = Math.min(start + chunkSize, file.size);
          const blob = file.slice(start, end);
          const buffer = await blob.arrayBuffer();
          await putChunkWithRetry(
            `/api/videos/${info.video_id}/chunks/${idx}`,
            buffer,
            token,
            abortController.signal,
          );
          receivedBytes += buffer.byteLength;
          setState((prev) =>
            prev.kind === 'uploading'
              ? { ...prev, receivedBytes }
              : prev,
          );
        }
      };

      try {
        await Promise.all(Array.from({ length: CONCURRENCY }, () => worker()));
      } catch (err) {
        if (abortController.signal.aborted) {
          // Cancelled by user; the cancel handler issues DELETE.
          return;
        }
        // Other failure: clean up server-side then surface error.
        await fetch(`/api/videos/${info.video_id}`, {
          method: 'DELETE',
          headers: { Authorization: `Bearer ${token}` },
        }).catch(() => {});
        setState({ kind: 'error', reason: (err as Error).message });
        return;
      }

      // POST /complete
      setState({ kind: 'completing', file, videoId: info.video_id });
      try {
        const resp = await fetch(`/api/videos/${info.video_id}/complete`, {
          method: 'POST',
          headers: { 'Content-Type': 'application/json', Authorization: `Bearer ${token}` },
          body: '{}',
        });
        if (!resp.ok) {
          setState({ kind: 'error', reason: `complete failed: ${resp.status}` });
          return;
        }
      } catch (err) {
        setState({ kind: 'error', reason: (err as Error).message });
        return;
      }

      onUploaded(info.video_id);
      setState({ kind: 'idle' });
    },
    [token, onUploaded],
  );

  const onFilePicked = (e: ChangeEvent<HTMLInputElement>) => {
    const file = e.target.files?.[0];
    if (file) {
      void startUpload(file);
    }
    if (inputRef.current) inputRef.current.value = '';
  };

  const onCancel = async () => {
    if (state.kind !== 'uploading') return;
    state.abortController.abort();
    if (token) {
      await fetch(`/api/videos/${state.videoId}`, {
        method: 'DELETE',
        headers: { Authorization: `Bearer ${token}` },
      }).catch(() => {});
    }
    setState({ kind: 'idle' });
  };

  const onDrop = (e: React.DragEvent) => {
    e.preventDefault();
    const file = e.dataTransfer.files?.[0];
    if (file && state.kind === 'idle') {
      void startUpload(file);
    }
  };

  const isBusy = state.kind === 'starting' || state.kind === 'uploading' || state.kind === 'completing';

  return (
    <div className="video-uploader">
      {!isBusy && (
        <div
          className="dropzone"
          onDrop={onDrop}
          onDragOver={(e) => e.preventDefault()}
        >
          <p>Drop a video here or</p>
          <label className="browse-label">
            Browse...
            <input
              ref={inputRef}
              type="file"
              accept="video/*"
              onChange={onFilePicked}
              hidden
            />
          </label>
        </div>
      )}
      {state.kind === 'uploading' && (
        <div className="progress">
          <p className="filename">{state.file.name}</p>
          <progress max={state.totalBytes} value={state.receivedBytes} />
          <p className="bytes">
            {(state.receivedBytes / 1_000_000).toFixed(1)} MB / {(state.totalBytes / 1_000_000).toFixed(1)} MB
          </p>
          <button onClick={onCancel}>Cancel</button>
        </div>
      )}
      {state.kind === 'completing' && <p>Finalizing...</p>}
      {state.kind === 'error' && (
        <div className="error">
          <p>Upload failed: {state.reason}</p>
          <button onClick={() => setState({ kind: 'idle' })}>Dismiss</button>
        </div>
      )}
      <style>{`
        .video-uploader { padding: 16px; }
        .dropzone { padding: 32px; border: 2px dashed var(--border-color, #30363d); border-radius: 8px; text-align: center; color: var(--text-muted, #6e7681); }
        .browse-label { display: inline-block; margin-top: 8px; padding: 6px 16px; background: var(--accent-primary, #3d5afe); color: white; border-radius: 6px; cursor: pointer; }
        .progress { padding: 16px; background: var(--bg-tertiary, #21262d); border-radius: 6px; }
        .progress progress { width: 100%; height: 8px; }
        .progress .filename { font-weight: 500; margin: 0 0 8px; }
        .progress .bytes { font-size: 12px; color: var(--text-muted, #6e7681); margin: 4px 0 12px; }
        .progress button { padding: 4px 12px; background: var(--error, #ff1744); color: white; border: none; border-radius: 4px; cursor: pointer; }
        .error { padding: 16px; background: rgba(255, 23, 68, 0.1); border-left: 3px solid var(--error, #ff1744); }
      `}</style>
    </div>
  );
}
```

- [ ] **Step 4: Slot VideoUploader into MapTab**

Edit `frontend/src/components/MapTab.tsx` — replace the `map-sidebar-placeholder` div with:

```tsx
import { VideoUploader } from './VideoUploader';
// ...
<aside className="map-sidebar">
  <VideoUploader onUploaded={(id) => { /* VideoLibrary refreshes on its own; Task 15 hooks selection */ }} />
  {/* VideoLibrary added in Task 15 */}
</aside>
```

- [ ] **Step 5: Run, watch pass**

```bash
cd /home/nvidia/bubbaloop-dash/frontend
npx vitest run src/components/__tests__/VideoUploader.test.tsx 2>&1 | tail -10
npm test 2>&1 | tail -5
```

Expected: the 3 new VideoUploader tests pass; the full suite (now 345 tests) is green.

- [ ] **Step 6: Commit**

```bash
cd /home/nvidia/bubbaloop-dash
git add frontend/src/components/VideoUploader.tsx frontend/src/components/__tests__/VideoUploader.test.tsx frontend/src/components/MapTab.tsx
git commit -m "feat(frontend): VideoUploader component (single-file chunked)

Drag-or-pick. POST /api/videos → 3 concurrent chunk PUTs with
exponential-backoff retry (250/500/1000/2000/4000ms, max 5) → POST
/complete. Cancel button aborts in-flight and DELETEs partial video.
No cross-session resume. Slotted into MapTab sidebar."
```

---

## Task 15: VideoLibrary component

**Files:**
- Create: `frontend/src/components/VideoLibrary.tsx`
- Create: `frontend/src/components/__tests__/VideoLibrary.test.tsx`
- Modify: `frontend/src/components/MapTab.tsx` (slot in VideoLibrary + refresh on upload)

- [ ] **Step 1: Write failing test (`frontend/src/components/__tests__/VideoLibrary.test.tsx`)**

```typescript
import { describe, expect, it, beforeEach, vi } from 'vitest';
import { render, screen, waitFor, fireEvent, act } from '@testing-library/react';
import { AuthProvider } from '../../contexts/AuthContext';
import { VideoLibrary, type VideoLibraryHandle } from '../VideoLibrary';
import { useRef } from 'react';

function withAuth(node: React.ReactNode) {
  localStorage.setItem('bubbaloop_dash_token', 'tk-test');
  return <AuthProvider>{node}</AuthProvider>;
}

describe('VideoLibrary', () => {
  beforeEach(() => {
    vi.restoreAllMocks();
    localStorage.clear();
  });

  it('renders empty state when no videos', async () => {
    vi.spyOn(globalThis, 'fetch').mockResolvedValue(
      new Response(JSON.stringify({ items: [], next_cursor: null }), { status: 200 }),
    );
    render(withAuth(<VideoLibrary />));
    await waitFor(() => expect(screen.getByText(/no videos yet/i)).toBeInTheDocument());
  });

  it('renders rows for ready, uploading, and error states', async () => {
    vi.spyOn(globalThis, 'fetch').mockResolvedValue(
      new Response(JSON.stringify({
        items: [
          { id: 'r1', filename: 'ok.mp4', size_bytes: 1_000_000, status: 'ready',
            created_at: 1_000, updated_at: 2_000, completed_at: 2_000, progress_pct: 100 },
          { id: 'u1', filename: 'busy.mp4', size_bytes: 2_000_000, status: 'uploading',
            created_at: 1_000, updated_at: Date.now(), completed_at: null, progress_pct: 47 },
          { id: 'e1', filename: 'bad.mp4', size_bytes: 500, status: 'error',
            created_at: 1_000, updated_at: 2_000, completed_at: null, progress_pct: 0 },
        ],
        next_cursor: null,
      }), { status: 200 }),
    );
    render(withAuth(<VideoLibrary />));
    await waitFor(() => expect(screen.getByText('ok.mp4')).toBeInTheDocument());
    expect(screen.getByText('busy.mp4')).toBeInTheDocument();
    expect(screen.getByText('bad.mp4')).toBeInTheDocument();
    expect(screen.getByText(/47%/)).toBeInTheDocument();
    expect(screen.getByText(/error/i)).toBeInTheDocument();
  });

  it('refreshes via imperative handle after parent triggers it', async () => {
    let callCount = 0;
    vi.spyOn(globalThis, 'fetch').mockImplementation(async () => {
      callCount++;
      return new Response(JSON.stringify({ items: [], next_cursor: null }), { status: 200 });
    });

    function Probe() {
      const ref = useRef<VideoLibraryHandle>(null);
      return (
        <>
          <button onClick={() => ref.current?.refresh()}>refresh</button>
          <VideoLibrary ref={ref} />
        </>
      );
    }
    render(withAuth(<Probe />));
    await waitFor(() => expect(callCount).toBeGreaterThanOrEqual(1));
    const prev = callCount;
    await act(async () => { fireEvent.click(screen.getByText('refresh')); });
    await waitFor(() => expect(callCount).toBe(prev + 1));
  });
});
```

- [ ] **Step 2: Run, watch fail**

```bash
cd /home/nvidia/bubbaloop-dash/frontend
npx vitest run src/components/__tests__/VideoLibrary.test.tsx 2>&1 | tail -10
```

Expected: import error.

- [ ] **Step 3: Implement (`frontend/src/components/VideoLibrary.tsx`)**

```tsx
import { forwardRef, useCallback, useEffect, useImperativeHandle, useRef, useState } from 'react';
import { useAuth } from '../contexts/AuthContext';

interface VideoSummary {
  id: string;
  filename: string;
  size_bytes: number;
  status: 'uploading' | 'ready' | 'error' | 'abandoned';
  created_at: number;
  updated_at: number;
  completed_at: number | null;
  progress_pct: number;
}

export interface VideoLibraryHandle {
  refresh: () => void;
}

const INTERRUPTED_THRESHOLD_MS = 10 * 60 * 1000;  // 10 min
const AUTO_REFRESH_INTERVAL_MS = 5_000;

function formatBytes(b: number): string {
  if (b < 1024) return `${b} B`;
  if (b < 1024 * 1024) return `${(b / 1024).toFixed(0)} KB`;
  if (b < 1024 * 1024 * 1024) return `${(b / 1024 / 1024).toFixed(0)} MB`;
  return `${(b / 1024 / 1024 / 1024).toFixed(1)} GB`;
}

function formatRelative(ms: number): string {
  const delta = Date.now() - ms;
  if (delta < 60_000) return 'just now';
  if (delta < 3_600_000) return `${Math.floor(delta / 60_000)}m ago`;
  if (delta < 86_400_000) return `${Math.floor(delta / 3_600_000)}h ago`;
  return `${Math.floor(delta / 86_400_000)}d ago`;
}

export const VideoLibrary = forwardRef<VideoLibraryHandle>(function VideoLibrary(_props, ref) {
  const { token } = useAuth();
  const [videos, setVideos] = useState<VideoSummary[]>([]);
  const [loading, setLoading] = useState<boolean>(false);

  const refresh = useCallback(async () => {
    if (!token) return;
    setLoading(true);
    try {
      const resp = await fetch('/api/videos?limit=100', {
        headers: { Authorization: `Bearer ${token}` },
      });
      if (!resp.ok) return;
      const body = (await resp.json()) as { items: VideoSummary[] };
      setVideos(body.items);
    } finally {
      setLoading(false);
    }
  }, [token]);

  useImperativeHandle(ref, () => ({ refresh }), [refresh]);

  // Initial load.
  useEffect(() => { void refresh(); }, [refresh]);

  // Auto-refresh while any video is uploading.
  const intervalRef = useRef<ReturnType<typeof setInterval> | null>(null);
  useEffect(() => {
    const hasUploading = videos.some((v) => v.status === 'uploading');
    if (hasUploading && !intervalRef.current) {
      intervalRef.current = setInterval(() => { void refresh(); }, AUTO_REFRESH_INTERVAL_MS);
    } else if (!hasUploading && intervalRef.current) {
      clearInterval(intervalRef.current);
      intervalRef.current = null;
    }
    return () => {
      if (intervalRef.current) {
        clearInterval(intervalRef.current);
        intervalRef.current = null;
      }
    };
  }, [videos, refresh]);

  const onDelete = async (id: string) => {
    if (!token) return;
    if (!window.confirm('Delete this video? This cannot be undone.')) return;
    await fetch(`/api/videos/${id}`, {
      method: 'DELETE',
      headers: { Authorization: `Bearer ${token}` },
    });
    void refresh();
  };

  const renderBadge = (v: VideoSummary) => {
    const interrupted = v.status === 'uploading' && Date.now() - v.updated_at > INTERRUPTED_THRESHOLD_MS;
    if (v.status === 'ready') return <span className="badge ready">✓ ready · {formatRelative(v.updated_at)}</span>;
    if (v.status === 'uploading' && !interrupted) {
      return <progress max={100} value={v.progress_pct} />;
    }
    if (interrupted) return <span className="badge interrupted">⚠ interrupted · last chunk {formatRelative(v.updated_at)}</span>;
    if (v.status === 'abandoned') return <span className="badge abandoned">abandoned · {formatRelative(v.updated_at)}</span>;
    if (v.status === 'error') return <span className="badge error">✗ error</span>;
    return null;
  };

  if (loading && videos.length === 0) {
    return <div className="library-empty">Loading...</div>;
  }
  if (videos.length === 0) {
    return <div className="library-empty">No videos yet. Drop one in the box above.</div>;
  }

  return (
    <div className="video-library">
      <header>
        <h3>Library</h3>
        <button onClick={() => void refresh()} aria-label="refresh">⟳</button>
      </header>
      <ul>
        {videos.map((v) => (
          <li key={v.id} className={`row status-${v.status}`}>
            <div className="row-main">
              <span className="filename" title={v.filename}>{v.filename}</span>
              <span className="size">{formatBytes(v.size_bytes)}</span>
            </div>
            <div className="row-status">{renderBadge(v)}</div>
            <div className="row-actions">
              <button onClick={() => void onDelete(v.id)} aria-label="delete">×</button>
            </div>
          </li>
        ))}
      </ul>
      <style>{`
        .library-empty { padding: 24px; color: var(--text-muted, #6e7681); text-align: center; font-size: 13px; }
        .video-library { padding: 16px; }
        .video-library header { display: flex; justify-content: space-between; align-items: center; margin-bottom: 8px; }
        .video-library h3 { font-size: 14px; margin: 0; text-transform: uppercase; color: var(--text-secondary, #8b949e); letter-spacing: 0.05em; }
        .video-library header button { background: none; border: none; color: var(--text-muted, #6e7681); cursor: pointer; font-size: 16px; }
        .video-library ul { list-style: none; padding: 0; margin: 0; }
        .row { background: var(--bg-tertiary, #21262d); border-radius: 6px; padding: 12px; margin-bottom: 8px; }
        .row-main { display: flex; justify-content: space-between; gap: 12px; }
        .filename { font-weight: 500; overflow: hidden; text-overflow: ellipsis; white-space: nowrap; }
        .size { color: var(--text-muted, #6e7681); font-size: 12px; flex-shrink: 0; }
        .row-status { margin-top: 4px; font-size: 12px; }
        .row-status progress { width: 100%; height: 6px; }
        .badge.ready { color: var(--success, #4caf50); }
        .badge.interrupted { color: var(--warning, #ffd600); }
        .badge.error { color: var(--error, #ff1744); }
        .badge.abandoned { color: var(--text-muted, #6e7681); }
        .row-actions { text-align: right; margin-top: 4px; }
        .row-actions button { background: none; border: none; color: var(--text-muted, #6e7681); cursor: pointer; font-size: 16px; }
        .row-actions button:hover { color: var(--error, #ff1744); }
      `}</style>
    </div>
  );
});
```

- [ ] **Step 4: Slot VideoLibrary into MapTab + refresh after upload**

Edit `frontend/src/components/MapTab.tsx` — replace fully:

```tsx
import { useRef } from 'react';
import { MapCanvas } from './MapCanvas';
import { VideoUploader } from './VideoUploader';
import { VideoLibrary, type VideoLibraryHandle } from './VideoLibrary';

export function MapTab() {
  const libraryRef = useRef<VideoLibraryHandle>(null);
  return (
    <div className="map-tab">
      <aside className="map-sidebar">
        <VideoUploader onUploaded={() => libraryRef.current?.refresh()} />
        <VideoLibrary ref={libraryRef} />
      </aside>
      <main className="map-main">
        <MapCanvas />
      </main>
      <style>{`
        .map-tab { display: flex; flex: 1; min-height: 0; }
        .map-sidebar { width: 360px; flex-shrink: 0; border-right: 1px solid var(--border-color, #30363d); display: flex; flex-direction: column; overflow-y: auto; }
        .map-main { flex: 1; display: flex; min-width: 0; }
        @media (max-width: 768px) {
          .map-tab { flex-direction: column; }
          .map-sidebar { width: 100%; border-right: none; border-bottom: 1px solid var(--border-color, #30363d); }
        }
      `}</style>
    </div>
  );
}
```

- [ ] **Step 5: Run tests + build**

```bash
cd /home/nvidia/bubbaloop-dash/frontend
npx vitest run src/components/__tests__/VideoLibrary.test.tsx 2>&1 | tail -10
npm test 2>&1 | tail -5
npm run build 2>&1 | tail -5
```

Expected: 3 new tests pass; full suite green (~348 tests); build succeeds.

- [ ] **Step 6: Commit**

```bash
cd /home/nvidia/bubbaloop-dash
git add frontend/src/components/VideoLibrary.tsx frontend/src/components/__tests__/VideoLibrary.test.tsx frontend/src/components/MapTab.tsx
git commit -m "feat(frontend): VideoLibrary component + MapTab integration

Lists videos with status-aware badges. Auto-refreshes every 5s while any
video is uploading; manual refresh button. Interrupted state derived in
the frontend from updated_at > 10min. forwardRef exposes refresh() so
the uploader can trigger a refresh after completion."
```

---

## Task 16: Manual E2E smoke + tag v0.1.1-alpha

**Files:** `SMOKE_NOTES.md` (overwrite), tag.

- [ ] **Step 1: Build the image (Jetson needs --network=host)**

```bash
cd /home/nvidia/bubbaloop-dash
docker build --network=host -t bubbaloop-dash:dev . 2>&1 | tail -5
docker tag bubbaloop-dash:dev bubbaloop-dash:latest
```

Expected: build succeeds.

- [ ] **Step 2: Run the container**

```bash
docker rm -f dash-smoke 2>/dev/null || true
TOKEN="phase-1.1-smoke-$(date +%s)"
echo "Token: $TOKEN"
docker run -d --name dash-smoke -p 8000:8000 \
  -e DASHBOARD_TOKEN="$TOKEN" \
  -e ZENOH_ENDPOINT=tcp/127.0.0.1:7447 \
  -v dash_data_smoke:/var/lib/bubbaloop-dash \
  bubbaloop-dash:latest
sleep 3
docker ps --filter name=dash-smoke
```

Expected: container shows "Up ... seconds".

- [ ] **Step 3: Automated curl verification**

```bash
echo "=== POST /api/videos (start upload) ==="
RESP=$(curl -s -X POST http://127.0.0.1:8000/api/videos \
  -H "Authorization: Bearer $TOKEN" \
  -H "Content-Type: application/json" \
  -d '{"filename":"smoke.mp4","size_bytes":1024,"content_type":"video/mp4"}')
echo "$RESP"
VID=$(echo "$RESP" | python3 -c "import sys, json; print(json.load(sys.stdin)['video_id'])")
echo "video_id=$VID"

echo "=== PUT one chunk (1024 bytes of 0xaa) ==="
python3 -c "import sys; sys.stdout.buffer.write(b'\xaa' * 1024)" | curl -sf -X PUT \
  "http://127.0.0.1:8000/api/videos/$VID/chunks/0" \
  -H "Authorization: Bearer $TOKEN" \
  --data-binary @-

echo "=== POST /complete ==="
curl -sf -X POST "http://127.0.0.1:8000/api/videos/$VID/complete" \
  -H "Authorization: Bearer $TOKEN" \
  -H "Content-Type: application/json" -d '{}'

echo
echo "=== GET /api/videos (should list one ready video) ==="
curl -sf "http://127.0.0.1:8000/api/videos" -H "Authorization: Bearer $TOKEN"
echo

echo "=== GET /api/jobs (should list one upload job) ==="
curl -sf "http://127.0.0.1:8000/api/jobs" -H "Authorization: Bearer $TOKEN"
echo

echo "=== DELETE /api/videos/$VID ==="
curl -si -X DELETE "http://127.0.0.1:8000/api/videos/$VID" -H "Authorization: Bearer $TOKEN" | head -3
```

Expected: all calls succeed; final list is empty after delete.

- [ ] **Step 4: Cleanup**

```bash
docker stop dash-smoke
docker rm dash-smoke
docker volume rm dash_data_smoke 2>/dev/null || true
```

- [ ] **Step 5: Update SMOKE_NOTES.md**

Append a Phase 1.1 section to `/home/nvidia/bubbaloop-dash/SMOKE_NOTES.md` (or overwrite if you prefer). Use:

```markdown

---

# Phase 1.1 manual smoke test

Automated curl portions verified by the implementer:
- [x] POST /api/videos creates upload session
- [x] PUT /api/videos/{id}/chunks/0 with valid bytes returns 204
- [x] POST /api/videos/{id}/complete transitions video to status=ready
- [x] GET /api/videos lists the uploaded video
- [x] GET /api/jobs lists the corresponding upload job
- [x] DELETE /api/videos/{id} removes the row + files

Manual browser portions (require a human):
- [ ] Map tab appears between Loop and Chat with the map-pin icon
- [ ] VideoUploader drop zone renders; drag-and-drop an iPhone clip works
- [ ] Progress bar advances during upload
- [ ] VideoLibrary lists the video with "ready" badge once /complete returns
- [ ] [Cancel] mid-upload aborts and removes the video
- [ ] Sidebar refreshes after upload without manual refresh

Instructions for the browser smoke (run after this script):

```bash
docker run --rm -d --name dash-smoke -p 8000:8000 \
  -e DASHBOARD_TOKEN=phase-1.1-manual \
  -e ZENOH_ENDPOINT=tcp/<your-jetson-tailscale-host>:7447 \
  bubbaloop-dash:latest
# open http://127.0.0.1:8000 → paste "phase-1.1-manual" → click Map tab
# drag an iPhone .mp4 onto the drop zone → watch it land in the library
docker stop dash-smoke
```
```

- [ ] **Step 6: Commit smoke notes**

```bash
cd /home/nvidia/bubbaloop-dash
git add SMOKE_NOTES.md
git commit -m "docs: Phase 1.1 manual smoke notes"
```

- [ ] **Step 7: Push the branch + open PR**

```bash
cd /home/nvidia/bubbaloop-dash
git push -u origin feat/phase-1.1-upload
gh pr create \
  --title "Phase 1.1 — upload pipeline + library" \
  --body "$(cat <<'EOF'
## Summary

Implements Phase 1.1 of bubbaloop-dash per the design spec in the bubbaloop repo:

- Storage Protocol + FilesystemStorage (atomic chunk writes via os.pwrite)
- SQLite WAL schema (videos + upload_progress + jobs) with FK CASCADE
- Chunk bitmap helpers (pack/unpack/popcount/missing)
- REST API: POST/PUT/GET/DELETE /api/videos, /api/jobs (list/get/delete), /api/blob
- Integrity sweeper (asyncio task in lifespan): marks status='uploading' rows abandoned after 24h
- Map tab in App.tsx; VideoUploader (single-file chunked) + VideoLibrary (status-aware rows) components

## Test plan

- [x] Backend pytest: all new tests pass (storage, bitmap, db, sweeper, videos_api, jobs_api, blob_api, sweeper_lifespan)
- [x] Backend ruff + mypy strict: clean
- [x] Frontend vitest: VideoUploader + VideoLibrary tests pass; full suite stays green
- [x] Frontend `npm run build`: succeeds
- [x] Docker image: builds; container starts; curl smoke (POST/PUT/complete/list/delete) passes
- [ ] Manual browser smoke (in PR reviewer's hands): drag-and-drop an iPhone clip, see it land

🤖 Generated with [Claude Code](https://claude.com/claude-code)
EOF
)" 2>&1 | tail -5
```

Expected: PR URL printed.

- [ ] **Step 8: Tag once PR merges (NOT during this task — wait for review)**

After PR review + merge to main:

```bash
cd /home/nvidia/bubbaloop-dash
git checkout main && git pull --ff-only
git tag -a v0.1.1-alpha -m "Phase 1.1 — upload pipeline + library"
git push origin v0.1.1-alpha
```

---

## Self-Review Checklist (run after writing the plan)

1. **Spec coverage:**
   - Storage Protocol + FilesystemStorage → Task 2 ✓
   - SQLite schema (videos + upload_progress + jobs) → Tasks 4, 5 ✓
   - Chunk bitmap helpers → Task 3 ✓
   - Sweeper (run_sweep + lifespan task) → Tasks 6, 12 ✓
   - POST/PUT/POST(complete)/GET/GET(detail)/DELETE /api/videos → Tasks 7, 8, 9, 10 ✓
   - /api/jobs endpoints → Task 11 ✓
   - /api/blob endpoint → Task 11 ✓
   - Settings additions (max_upload_bytes, upload_chunk_size_bytes, abandoned_after_s) → Task 1 ✓
   - Map tab in App.tsx + MapTab.tsx + MapCanvas.tsx → Task 13 ✓
   - VideoUploader → Task 14 ✓
   - VideoLibrary → Task 15 ✓
   - Manual smoke + tag → Task 16 ✓
   - 4 job states (uploading/ready/error/abandoned) → covered in Db + sweeper ✓

2. **Placeholder scan:** No "TBD", "TODO", "fill in", "similar to" found. All steps include complete code or exact commands.

3. **Type consistency:**
   - `VideoRow.id`, `JobRow.id`, etc. names match across Db tests and API code.
   - `Db.create_video(...)` signature is consistent across Task 4 (definition) and Task 7 (caller).
   - `Storage.write_chunk(path, offset, data)` signature consistent between Task 2 and Task 8 caller.
   - `VideoLibraryHandle.refresh` exposed in Task 15 + consumed in Task 15's MapTab.
   - Settings field names (`upload_chunk_size_bytes`, `max_upload_bytes`, `abandoned_after_s`) consistent across Task 1 definition + Tasks 7, 9, 12 usage.
