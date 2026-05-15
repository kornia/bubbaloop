/**
 * LibraryView — spatialstore data-lake browser
 *
 * Fetches sessions from the spatialstore REST API (default: http://localhost:8080),
 * groups them by date, and shows a thumbnail grid for the selected session.
 * Frames are sorted by frame_index so temporal order is preserved —
 * this matters for feature matching strategy later (temporal window).
 */
import { useState, useEffect, useCallback } from "react";

const SPATIALSTORE_URL: string =
  (import.meta.env as Record<string, string | undefined>)["VITE_SPATIALSTORE_URL"] ??
  "http://localhost:8080";

interface Session {
  session_id: string;
  name: string;
  site_id: string;
  device: string | null;
  mcap_source: string | null;
  camera_id: string | null;
  frame_count: number;
  created_at_ns: number;
}

interface Frame {
  frame_id: string;
  session_id: string;
  timestamp_ns: number;
  camera_id: string;
  frame_index: number;
  width: number;
  height: number;
  episode_id: string | null;
}

function formatDate(ns: number): string {
  const ms = ns / 1_000_000;
  return new Date(ms).toLocaleDateString(undefined, {
    year: "numeric",
    month: "short",
    day: "numeric",
  });
}

function formatTime(ns: number): string {
  const ms = ns / 1_000_000;
  return new Date(ms).toLocaleTimeString(undefined, {
    hour: "2-digit",
    minute: "2-digit",
    second: "2-digit",
  });
}

function groupByDate(sessions: Session[]): Map<string, Session[]> {
  const map = new Map<string, Session[]>();
  for (const s of sessions) {
    const day = formatDate(s.created_at_ns);
    if (!map.has(day)) map.set(day, []);
    map.get(day)!.push(s);
  }
  return map;
}

// ── Thumbnail grid ────────────────────────────────────────────────────────────

function ThumbnailGrid({
  session,
  onFrameClick,
}: {
  session: Session;
  onFrameClick: (frame: Frame) => void;
}) {
  const [frames, setFrames] = useState<Frame[]>([]);
  const [loading, setLoading] = useState(true);
  const [page, setPage] = useState(0);
  const PAGE_SIZE = 48;

  useEffect(() => {
    setLoading(true);
    setFrames([]);
    setPage(0);
  }, [session.session_id]);

  useEffect(() => {
    let cancelled = false;
    setLoading(true);
    fetch(
      `${SPATIALSTORE_URL}/v1/frames?session_id=${session.session_id}&limit=${PAGE_SIZE}&offset=${page * PAGE_SIZE}`
    )
      .then((r) => r.json())
      .then((data: Frame[]) => {
        if (!cancelled) {
          // sort by frame_index to preserve temporal order
          const sorted = [...data].sort((a, b) => a.frame_index - b.frame_index);
          setFrames((prev) => (page === 0 ? sorted : [...prev, ...sorted]));
          setLoading(false);
        }
      })
      .catch(() => {
        if (!cancelled) setLoading(false);
      });
    return () => { cancelled = true; };
  }, [session.session_id, page]);

  const thumbnailUrl = (frame: Frame) =>
    `${SPATIALSTORE_URL}/v1/frames/${frame.frame_id}/thumbnail?site_id=${session.site_id}`;

  return (
    <div className="thumbnail-grid-container">
      <div className="grid-header">
        <span className="grid-title">{session.name}</span>
        <span className="grid-meta">
          {session.camera_id ?? "unknown camera"} ·{" "}
          {session.frame_count} frames ·{" "}
          {formatTime(frames[0]?.timestamp_ns ?? session.created_at_ns)}
        </span>
      </div>

      <div className="thumbnail-grid">
        {frames.map((f) => (
          <button
            key={f.frame_id}
            className="thumb-cell"
            onClick={() => onFrameClick(f)}
            title={`frame ${f.frame_index} · ${formatTime(f.timestamp_ns)}`}
          >
            <img
              src={thumbnailUrl(f)}
              alt={`frame ${f.frame_index}`}
              loading="lazy"
            />
            <span className="thumb-index">{f.frame_index}</span>
          </button>
        ))}
      </div>

      {loading && <div className="grid-loading">Loading frames…</div>}

      {!loading && frames.length < session.frame_count && (
        <button className="load-more-btn" onClick={() => setPage((p) => p + 1)}>
          Load more ({frames.length} / {session.frame_count})
        </button>
      )}
    </div>
  );
}

// ── Frame lightbox ────────────────────────────────────────────────────────────

function Lightbox({
  frame,
  siteId,
  onClose,
}: {
  frame: Frame;
  siteId: string;
  onClose: () => void;
}) {
  const imageUrl = `${SPATIALSTORE_URL}/v1/frames/${frame.frame_id}/image?site_id=${siteId}`;
  return (
    <div className="lightbox-overlay" onClick={onClose}>
      <div className="lightbox-content" onClick={(e) => e.stopPropagation()}>
        <div className="lightbox-header">
          <span>
            frame {frame.frame_index} · {frame.width}×{frame.height} ·{" "}
            {formatTime(frame.timestamp_ns)}
            {frame.episode_id && <span className="episode-tag"> ep:{frame.episode_id.slice(0, 8)}</span>}
          </span>
          <button className="lightbox-close" onClick={onClose}>✕</button>
        </div>
        <img src={imageUrl} alt={`frame ${frame.frame_index}`} className="lightbox-image" />
        <div className="lightbox-meta">
          <code>frame_id: {frame.frame_id}</code>
          <code>session: {frame.session_id.slice(0, 8)}…</code>
          <code>ts_ns: {frame.timestamp_ns}</code>
        </div>
      </div>
    </div>
  );
}

// ── Main view ─────────────────────────────────────────────────────────────────

export function LibraryView() {
  const [sessions, setSessions] = useState<Session[]>([]);
  const [loading, setLoading] = useState(true);
  const [error, setError] = useState<string | null>(null);
  const [selectedSession, setSelectedSession] = useState<Session | null>(null);
  const [lightboxFrame, setLightboxFrame] = useState<Frame | null>(null);

  const fetchSessions = useCallback(() => {
    setLoading(true);
    setError(null);
    fetch(`${SPATIALSTORE_URL}/v1/sessions`)
      .then((r) => {
        if (!r.ok) throw new Error(`HTTP ${r.status}`);
        return r.json();
      })
      .then((data: Session[]) => {
        setSessions(data);
        setLoading(false);
      })
      .catch((e) => {
        setError(`Cannot reach spatialstore at ${SPATIALSTORE_URL}: ${e.message}`);
        setLoading(false);
      });
  }, []);

  useEffect(() => { fetchSessions(); }, [fetchSessions]);

  const grouped = groupByDate(sessions);

  return (
    <div className="library-view">
      {/* ── Sidebar ── */}
      <aside className="library-sidebar">
        <div className="sidebar-header">
          <span className="sidebar-title">Recordings</span>
          <button className="refresh-btn" onClick={fetchSessions} title="Refresh">↻</button>
        </div>

        {loading && <div className="sidebar-state">Loading…</div>}
        {error && <div className="sidebar-state error">{error}</div>}
        {!loading && !error && sessions.length === 0 && (
          <div className="sidebar-state muted">
            No recordings yet.
            <br /><br />
            Run:
            <pre>python scripts/import_mcap.py *.mcap</pre>
          </div>
        )}

        {Array.from(grouped.entries()).map(([date, daySessions]) => (
          <div key={date} className="date-group">
            <div className="date-label">{date}</div>
            {daySessions.map((s) => (
              <button
                key={s.session_id}
                className={`session-row ${selectedSession?.session_id === s.session_id ? "active" : ""}`}
                onClick={() => setSelectedSession(s)}
              >
                <span className="session-name">{s.name}</span>
                <span className="session-count">{s.frame_count} fr</span>
              </button>
            ))}
          </div>
        ))}
      </aside>

      {/* ── Main panel ── */}
      <main className="library-main">
        {selectedSession ? (
          <ThumbnailGrid
            session={selectedSession}
            onFrameClick={setLightboxFrame}
          />
        ) : (
          <div className="empty-state">
            <p>Select a recording to browse frames</p>
          </div>
        )}
      </main>

      {/* ── Lightbox ── */}
      {lightboxFrame && selectedSession && (
        <Lightbox
          frame={lightboxFrame}
          siteId={selectedSession.site_id}
          onClose={() => setLightboxFrame(null)}
        />
      )}

      <style>{`
        .library-view {
          display: flex;
          flex: 1;
          overflow: hidden;
          height: calc(100vh - 61px);
        }

        /* sidebar */
        .library-sidebar {
          width: 260px;
          min-width: 200px;
          background: var(--bg-secondary);
          border-right: 1px solid var(--border-color);
          display: flex;
          flex-direction: column;
          overflow-y: auto;
        }
        .sidebar-header {
          display: flex;
          justify-content: space-between;
          align-items: center;
          padding: 12px 16px;
          border-bottom: 1px solid var(--border-color);
          position: sticky;
          top: 0;
          background: var(--bg-secondary);
          z-index: 1;
        }
        .sidebar-title { font-weight: 600; font-size: 13px; color: var(--text-primary); }
        .refresh-btn {
          background: none; border: none; color: var(--text-muted);
          cursor: pointer; font-size: 16px; padding: 2px 6px;
          border-radius: 4px;
        }
        .refresh-btn:hover { color: var(--text-primary); background: var(--bg-tertiary); }
        .sidebar-state {
          padding: 16px; font-size: 13px; color: var(--text-muted);
        }
        .sidebar-state.error { color: var(--error); }
        .sidebar-state.muted { color: var(--text-muted); }
        .sidebar-state pre {
          background: var(--bg-tertiary); padding: 6px 8px; border-radius: 4px;
          font-size: 11px; margin-top: 8px; white-space: pre-wrap; word-break: break-all;
        }
        .date-group { padding: 8px 0; }
        .date-label {
          padding: 4px 16px; font-size: 11px; font-weight: 600;
          color: var(--text-muted); text-transform: uppercase; letter-spacing: 0.05em;
        }
        .session-row {
          width: 100%; display: flex; justify-content: space-between; align-items: center;
          padding: 8px 16px; background: none; border: none;
          text-align: left; cursor: pointer; color: var(--text-secondary);
          font-size: 12px; gap: 8px;
        }
        .session-row:hover { background: var(--bg-tertiary); color: var(--text-primary); }
        .session-row.active { background: var(--bg-tertiary); color: var(--accent-primary); }
        .session-name { flex: 1; overflow: hidden; text-overflow: ellipsis; white-space: nowrap; }
        .session-count { flex-shrink: 0; color: var(--text-muted); font-size: 11px; }

        /* main panel */
        .library-main {
          flex: 1; overflow-y: auto; background: var(--bg-primary);
        }
        .empty-state {
          display: flex; align-items: center; justify-content: center;
          height: 100%; color: var(--text-muted); font-size: 14px;
        }

        /* thumbnail grid */
        .thumbnail-grid-container { padding: 16px; }
        .grid-header {
          display: flex; align-items: baseline; gap: 12px; margin-bottom: 12px;
        }
        .grid-title { font-size: 14px; font-weight: 600; color: var(--text-primary); }
        .grid-meta { font-size: 12px; color: var(--text-muted); }
        .thumbnail-grid {
          display: grid;
          grid-template-columns: repeat(auto-fill, minmax(128px, 1fr));
          gap: 4px;
        }
        .thumb-cell {
          position: relative; background: var(--bg-secondary); border: none;
          cursor: pointer; padding: 0; border-radius: 4px; overflow: hidden;
          aspect-ratio: 16/9;
        }
        .thumb-cell:hover { outline: 2px solid var(--accent-primary); }
        .thumb-cell img { width: 100%; height: 100%; object-fit: cover; display: block; }
        .thumb-index {
          position: absolute; bottom: 2px; right: 4px;
          font-size: 10px; color: rgba(255,255,255,0.7);
          font-family: 'JetBrains Mono', monospace;
          text-shadow: 0 1px 2px rgba(0,0,0,0.8);
        }
        .grid-loading { padding: 16px; text-align: center; color: var(--text-muted); font-size: 13px; }
        .load-more-btn {
          margin-top: 12px; display: block; width: 100%; padding: 10px;
          background: var(--bg-secondary); border: 1px solid var(--border-color);
          border-radius: 6px; color: var(--text-secondary); cursor: pointer; font-size: 13px;
        }
        .load-more-btn:hover { border-color: var(--accent-primary); color: var(--accent-primary); }

        /* lightbox */
        .lightbox-overlay {
          position: fixed; inset: 0; background: rgba(0,0,0,0.85);
          display: flex; align-items: center; justify-content: center; z-index: 1000;
        }
        .lightbox-content {
          background: var(--bg-secondary); border-radius: 8px;
          border: 1px solid var(--border-color); max-width: 90vw; max-height: 90vh;
          display: flex; flex-direction: column; overflow: hidden;
        }
        .lightbox-header {
          display: flex; justify-content: space-between; align-items: center;
          padding: 10px 14px; border-bottom: 1px solid var(--border-color);
          font-size: 12px; color: var(--text-muted); gap: 12px;
        }
        .lightbox-close {
          background: none; border: none; color: var(--text-muted);
          cursor: pointer; font-size: 16px; padding: 2px 6px;
        }
        .lightbox-close:hover { color: var(--text-primary); }
        .lightbox-image {
          max-width: 85vw; max-height: 75vh; object-fit: contain; display: block;
        }
        .lightbox-meta {
          display: flex; gap: 16px; padding: 8px 14px; flex-wrap: wrap;
          border-top: 1px solid var(--border-color);
        }
        .lightbox-meta code {
          font-size: 11px; color: var(--text-muted);
          font-family: 'JetBrains Mono', monospace;
        }
        .episode-tag { color: var(--accent-primary); margin-left: 6px; }
      `}</style>
    </div>
  );
}
