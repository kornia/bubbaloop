import { useState, useCallback, useEffect, useRef } from 'react';
import { Sample, Reply, ReplyError, Duration } from '@eclipse-zenoh/zenoh-ts';
import L from 'leaflet';
import 'leaflet/dist/leaflet.css';
import 'leaflet.heat';
import { getEncodingInfo, getSamplePayload, EncodingPredefined, snakeToCamel } from '../lib/zenoh';
import { useZenohSubscription } from '../hooks/useZenohSubscription';
import { useZenohSubscriptionContext } from '../contexts/ZenohSubscriptionContext';

// leaflet.heat doesn't ship type declarations; declare the bit we use.
declare module 'leaflet' {
  function heatLayer(
    latlngs: Array<[number, number, number?]>,
    options?: {
      radius?: number;
      blur?: number;
      maxZoom?: number;
      minOpacity?: number;
      max?: number;
      gradient?: Record<number, string>;
    },
  ): L.Layer & {
    setLatLngs(latlngs: Array<[number, number, number?]>): void;
    addLatLng(latlng: [number, number, number?]): void;
    redraw(): void;
  };
}

// ---------------------------------------------------------------------------
// Types
// ---------------------------------------------------------------------------

interface WorldDetection {
  track_id: string;
  class_name: string;
  confidence: number;
  world_x: number;
  world_y: number;
  pose_quality: string;
}

interface WorldDetectionsMsg {
  timestamp_ns: number;
  camera: string;
  detections: WorldDetection[];
}

interface PoseMsg {
  timestamp_ns: number;
  camera?: string;
  R: number[];
  t_world?: number[];
  tilt_deg?: number;
  height_m?: number;
  tracking_state?: string;
  source?: string;
}

interface LiveDot {
  x: number;
  y: number;
  class_name: string;
  last_seen: number; // Date.now() ms
}

interface TrailPoint {
  x: number;
  y: number;
  ts: number; // Date.now() ms
}

interface Zone {
  id: string;
  name: string;
  polygon: [number, number][];
  color: string;
}

interface ViewBox {
  x: number;
  y: number;
  w: number;
  h: number;
}

interface SiteMapViewProps {
  onRemove: () => void;
  dragHandleProps?: React.HTMLAttributes<HTMLElement>;
  topic?: string;
  /** When true, render as a full-page tab layout (map + sidebar) instead of a panel. */
  fullScreen?: boolean;
}

type TimeRange = '1h' | '6h' | '24h' | '7d' | 'all';

function rangeToWindow(range: TimeRange): { start_ns: number | null; end_ns: number | null } {
  const now = Date.now() * 1_000_000;
  const HOUR_NS = 3_600_000_000_000;
  const DAY_NS = 24 * HOUR_NS;
  switch (range) {
    case '1h':  return { start_ns: now - HOUR_NS,    end_ns: null };
    case '6h':  return { start_ns: now - 6 * HOUR_NS, end_ns: null };
    case '24h': return { start_ns: now - DAY_NS,     end_ns: null };
    case '7d':  return { start_ns: now - 7 * DAY_NS, end_ns: null };
    case 'all': return { start_ns: null, end_ns: null };
  }
}

// ---------------------------------------------------------------------------
// Constants
// ---------------------------------------------------------------------------

const SCALE = 20; // px per meter
const TRAIL_WINDOW_MS = 30_000;
const STALE_MS = 5_000;
const GRID_SPACING = 1; // 1 meter
// Initial viewBox centered on (0,0) world origin (= camera position).
// Y is flipped (svg-down = world-back). Showing roughly 30m × 20m of world space.
// World X ∈ [-15, +15] m, Y ∈ [-2, +18] m  →  svg [-300, +300] × [-360, +40]
const INITIAL_VIEW: ViewBox = { x: -300, y: -360, w: 600, h: 400 };

// Heatmap binning resolution + temporal decay.
// HEATMAP_CELL_M=0.5 means each cell is 50cm × 50cm in world coords.
const HEATMAP_CELL_M = 0.5;
// Decay applied every HEATMAP_DECAY_INTERVAL_MS — multiplies count by HEATMAP_DECAY_FACTOR.
// At 0.95 every 5s → cell halves in ~70s, fades to ~0 in ~5min.
const HEATMAP_DECAY_INTERVAL_MS = 5_000;
const HEATMAP_DECAY_FACTOR = 0.95;
// Cells with count below this threshold are pruned.
const HEATMAP_MIN_COUNT = 0.05;

const CLASS_COLORS: Record<string, string> = {
  person: '#3B82F6',
  car: '#EF4444',
  dog: '#10B981',
  bicycle: '#F59E0B',
  default: '#8B5CF6',
};

function classColor(name: string): string {
  return CLASS_COLORS[name] ?? CLASS_COLORS.default;
}

// World → SVG: svgX = world_x * SCALE, svgY = -world_y * SCALE
function worldToSvg(wx: number, wy: number): [number, number] {
  return [wx * SCALE, -wy * SCALE];
}

// Styles for the full-screen map page — glassmorphism over the map canvas,
// floating overlays in corners, mobile-responsive collapse.
const mapPageStyles = `
.map-page {
  position: relative;
  width: 100%;
  height: 100%;
  flex: 1;
  min-height: 600px;
  background:
    radial-gradient(ellipse at 30% 20%, rgba(59,130,246,0.08), transparent 60%),
    radial-gradient(ellipse at 70% 80%, rgba(34,211,238,0.06), transparent 60%),
    #0a0e1a;
  border-radius: 12px;
  overflow: hidden;
  isolation: isolate;
}
.map-canvas {
  position: absolute;
  inset: 0;
}
.map-canvas .site-map-svg {
  width: 100%;
  height: 100%;
  display: block;
  cursor: grab;
}
.map-canvas .site-map-svg:active { cursor: grabbing; }

/* Leaflet container — fills the page, dark theme. */
.leaflet-canvas {
  position: absolute;
  inset: 0;
  background: #0a0e1a;
  z-index: 1;
}
.leaflet-canvas .leaflet-control-attribution { display: none; }
.leaflet-canvas .leaflet-control-zoom {
  border: 1px solid rgba(255,255,255,0.08) !important;
  background: rgba(15, 20, 35, 0.72);
  backdrop-filter: blur(16px) saturate(160%);
  -webkit-backdrop-filter: blur(16px) saturate(160%);
  border-radius: 10px !important;
  overflow: hidden;
  margin: 16px !important;
  box-shadow: 0 4px 16px rgba(0,0,0,0.3);
}
.leaflet-canvas .leaflet-control-zoom a {
  background: transparent !important;
  color: rgba(255,255,255,0.85) !important;
  border: none !important;
  border-bottom: 1px solid rgba(255,255,255,0.06) !important;
  width: 32px !important;
  height: 32px !important;
  line-height: 32px !important;
  font-size: 16px !important;
  font-weight: 400 !important;
}
.leaflet-canvas .leaflet-control-zoom a:last-child { border-bottom: none !important; }
.leaflet-canvas .leaflet-control-zoom a:hover {
  background: rgba(255,255,255,0.08) !important;
  color: white !important;
}
.leaflet-canvas .leaflet-tooltip {
  background: rgba(15, 20, 35, 0.92);
  border: 1px solid rgba(255,255,255,0.1);
  color: rgba(255,255,255,0.9);
  font-size: 11px;
  padding: 4px 8px;
  border-radius: 6px;
  box-shadow: 0 2px 8px rgba(0,0,0,0.4);
}
.leaflet-canvas .leaflet-tooltip::before { border: none !important; }
.leaflet-canvas .camera-tooltip {
  background: rgba(34, 211, 238, 0.18) !important;
  border: 1px solid rgba(34, 211, 238, 0.5) !important;
  color: #22d3ee !important;
  font-weight: 600;
  letter-spacing: 0.3px;
}

/* Pose popup — dark glassmorphic, matches floating overlay cards */
.leaflet-canvas .pose-popup .leaflet-popup-content-wrapper {
  background: rgba(15, 20, 35, 0.92) !important;
  border: 1px solid rgba(255,255,255,0.08);
  border-radius: 12px !important;
  backdrop-filter: blur(16px) saturate(160%);
  -webkit-backdrop-filter: blur(16px) saturate(160%);
  color: rgba(255,255,255,0.92);
  box-shadow: 0 6px 24px rgba(0,0,0,0.45) !important;
}
.leaflet-canvas .pose-popup .leaflet-popup-content {
  margin: 0 !important;
  width: auto !important;
  min-width: 220px;
}
.leaflet-canvas .pose-popup .leaflet-popup-tip {
  background: rgba(15, 20, 35, 0.92);
  border: 1px solid rgba(255,255,255,0.08);
}
.leaflet-canvas .pose-popup .leaflet-popup-close-button {
  color: rgba(255,255,255,0.6) !important;
  padding: 6px 8px !important;
  font-size: 16px !important;
}
.leaflet-canvas .pose-popup .leaflet-popup-close-button:hover {
  color: white !important;
}
.pose-popup-body {
  padding: 14px 16px;
  display: flex;
  flex-direction: column;
  gap: 10px;
}
.pose-popup-title {
  font-size: 14px;
  font-weight: 600;
  color: #22d3ee;
  letter-spacing: -0.2px;
}
.pose-popup-status {
  align-self: flex-start;
  font-size: 10px;
  font-weight: 600;
  text-transform: uppercase;
  letter-spacing: 0.6px;
  padding: 3px 8px;
  border-radius: 999px;
  background: rgba(34, 197, 94, 0.18);
  color: #4ade80;
  border: 1px solid rgba(34, 197, 94, 0.3);
}
.pose-popup-status.status-degraded {
  background: rgba(250, 204, 21, 0.15);
  color: #facc15;
  border-color: rgba(250, 204, 21, 0.3);
}
.pose-popup-status.status-lost,
.pose-popup-status.status-recently_lost,
.pose-popup-status.status-lost_reloc {
  background: rgba(239, 68, 68, 0.18);
  color: #f87171;
  border-color: rgba(239, 68, 68, 0.3);
}
.pose-popup-empty {
  font-size: 12px;
  color: rgba(255,255,255,0.5);
  font-style: italic;
}
.pose-popup-grid {
  display: grid;
  grid-template-columns: auto 1fr;
  gap: 6px 14px;
  font-size: 12px;
  font-variant-numeric: tabular-nums;
}
.pose-popup-label {
  color: rgba(255,255,255,0.55);
  font-size: 11px;
  text-transform: uppercase;
  letter-spacing: 0.5px;
  align-self: center;
}
.pose-popup-value {
  color: rgba(255,255,255,0.95);
  font-weight: 500;
  text-align: right;
}
.leaflet-canvas .leaflet-pane > .leaflet-overlay-pane {
  filter: drop-shadow(0 0 4px rgba(59, 130, 246, 0.25));
}

/* Glassmorphism overlay cards */
.map-overlay {
  position: absolute;
  z-index: 10;
  padding: 12px 14px;
  border-radius: 14px;
  display: flex;
  flex-direction: column;
  gap: 10px;
  pointer-events: auto;
  font-size: 13px;
  color: rgba(255,255,255,0.92);
}
.glass {
  background: rgba(15, 20, 35, 0.72);
  border: 1px solid rgba(255,255,255,0.08);
  backdrop-filter: blur(16px) saturate(160%);
  -webkit-backdrop-filter: blur(16px) saturate(160%);
  box-shadow:
    0 4px 24px rgba(0,0,0,0.35),
    0 1px 0 rgba(255,255,255,0.03) inset;
}
.map-overlay-tl { top: 16px; left: 16px; min-width: 220px; }
.map-overlay-tr { top: 16px; right: 16px; min-width: 220px; }
.map-overlay-b  { bottom: 16px; left: 50%; transform: translateX(-50%); min-width: 280px; max-width: 90%; }

/* Top-left title + chips */
.overlay-title {
  display: flex;
  align-items: center;
  gap: 8px;
  font-size: 14px;
  font-weight: 600;
  letter-spacing: -0.2px;
}
.overlay-sub {
  margin-left: auto;
  font-size: 11px;
  font-weight: 400;
  color: rgba(255,255,255,0.55);
  text-transform: lowercase;
  letter-spacing: 0;
}
.dot-pulse {
  width: 8px; height: 8px;
  border-radius: 50%;
  background: #22d3ee;
  box-shadow: 0 0 0 0 rgba(34,211,238,0.7);
  animation: dotPulse 1.8s ease-out infinite;
}
@keyframes dotPulse {
  0%   { box-shadow: 0 0 0 0 rgba(34,211,238,0.7); }
  70%  { box-shadow: 0 0 0 10px rgba(34,211,238,0); }
  100% { box-shadow: 0 0 0 0 rgba(34,211,238,0); }
}

.time-chips {
  display: flex;
  gap: 4px;
  background: rgba(0,0,0,0.25);
  padding: 3px;
  border-radius: 10px;
}
.chip {
  flex: 1;
  padding: 6px 10px;
  font-size: 11px;
  font-weight: 500;
  background: transparent;
  color: rgba(255,255,255,0.62);
  border: none;
  border-radius: 7px;
  cursor: pointer;
  transition: all 0.15s ease;
  font-variant-numeric: tabular-nums;
}
.chip:hover { color: rgba(255,255,255,0.9); }
.chip-active {
  background: rgba(59,130,246,0.9);
  color: white;
  box-shadow: 0 2px 8px rgba(59,130,246,0.35);
}

/* Top-right stats grid */
.stats-grid {
  display: grid;
  grid-template-columns: 1fr 1fr;
  gap: 8px;
}
.stat-cell {
  background: rgba(255,255,255,0.04);
  border: 1px solid rgba(255,255,255,0.06);
  padding: 8px 10px;
  border-radius: 8px;
}
.stat-cell-wide { grid-column: 1 / -1; }
.stat-cell-num {
  font-size: 22px;
  font-weight: 600;
  letter-spacing: -0.6px;
  font-variant-numeric: tabular-nums;
  line-height: 1;
}
.stat-cell-lbl {
  font-size: 10px;
  margin-top: 4px;
  color: rgba(255,255,255,0.5);
  text-transform: uppercase;
  letter-spacing: 0.6px;
}

.overlay-controls {
  display: flex;
  gap: 6px;
}
.pill {
  display: inline-flex;
  align-items: center;
  gap: 6px;
  padding: 6px 12px;
  background: rgba(255,255,255,0.05);
  border: 1px solid rgba(255,255,255,0.08);
  color: rgba(255,255,255,0.78);
  border-radius: 999px;
  font-size: 11px;
  font-weight: 500;
  cursor: pointer;
  transition: all 0.15s ease;
}
.pill:hover { background: rgba(255,255,255,0.08); color: white; }
.pill-on {
  background: rgba(59,130,246,0.85);
  border-color: rgba(59,130,246,1);
  color: white;
}

/* Bottom histogram */
.overlay-section-label {
  font-size: 10px;
  text-transform: uppercase;
  letter-spacing: 0.6px;
  color: rgba(255,255,255,0.5);
  margin-bottom: 4px;
}
.hour-histogram { display: block; }

/* ── Mobile (≤ 768px): collapse overlays to a stacked bottom panel ── */
@media (max-width: 768px) {
  .map-overlay-tl,
  .map-overlay-tr,
  .map-overlay-b {
    position: absolute;
    left: 8px;
    right: 8px;
    width: auto;
    transform: none;
    min-width: 0;
    max-width: none;
  }
  .map-overlay-tl { top: 8px; }
  .map-overlay-tr {
    top: auto;
    bottom: 188px;
  }
  .map-overlay-b {
    bottom: 8px;
  }
  .stats-grid { grid-template-columns: 1fr 1fr 1fr; }
  .stat-cell-wide { grid-column: auto; }
  .stat-cell-num { font-size: 18px; }
  .overlay-title { font-size: 13px; }
  .chip { padding: 5px 8px; font-size: 10px; }
}

/* ── Tiny screens (≤ 420px): even more compact ── */
@media (max-width: 420px) {
  .map-overlay { padding: 10px 12px; gap: 8px; }
  .map-overlay-tr { bottom: 168px; }
  .stats-grid { gap: 6px; }
  .stat-cell { padding: 6px 8px; }
  .pill { padding: 5px 10px; font-size: 10px; }
}
`;

// ---------------------------------------------------------------------------
// Component
// ---------------------------------------------------------------------------

export function SiteMapView({ onRemove, dragHandleProps, topic, fullScreen = false }: SiteMapViewProps) {
  const subscribeTopic = topic ?? 'bubbaloop/global/**/world_detections';
  const { getSession } = useZenohSubscriptionContext();

  // Live state
  const liveRef = useRef<Map<string, LiveDot>>(new Map());
  const trailsRef = useRef<Map<string, TrailPoint[]>>(new Map());
  const [renderTick, setRenderTick] = useState(0);

  // Zones (static demo — could be fetched from config later)
  const [zones] = useState<Zone[]>([]);

  // Heatmap — accumulated client-side from incoming detections + backfilled rows.
  // Each cell key = `${gridX},${gridY}` at HEATMAP_CELL_M resolution.
  const heatmapRef = useRef<Map<string, { x: number; y: number; count: number }>>(new Map());
  const [showHeatmap, setShowHeatmap] = useState(true);

  // Time range + Lance backfill (full-page mode only — panel mode stays live-only).
  const [timeRange, setTimeRange] = useState<TimeRange>('1h');
  const [hourlyBuckets, setHourlyBuckets] = useState<number[]>(Array(24).fill(0));
  const [totalRows, setTotalRows] = useState<number>(0);
  const [backfillState, setBackfillState] = useState<'idle' | 'loading' | 'ok' | 'err'>('idle');

  // Pan/zoom viewBox
  const [viewBox, setViewBox] = useState<ViewBox>(INITIAL_VIEW);
  const viewBoxRef = useRef<ViewBox>(INITIAL_VIEW);
  viewBoxRef.current = viewBox;

  const svgRef = useRef<SVGSVGElement>(null);
  const isPanningRef = useRef(false);
  const panStartRef = useRef({ x: 0, y: 0, vbx: 0, vby: 0 });

  // ---- Incoming sample handler ----
  const handleSample = useCallback((sample: Sample) => {
    const encoding = getEncodingInfo(sample);
    let parsed: unknown = null;

    if (encoding.id === EncodingPredefined.APPLICATION_JSON ||
        encoding.id === EncodingPredefined.TEXT_JSON) {
      try {
        const bytes = getSamplePayload(sample);
        const text = new TextDecoder().decode(bytes);
        parsed = JSON.parse(text);
      } catch {
        return;
      }
    } else {
      try {
        const bytes = getSamplePayload(sample);
        const text = new TextDecoder().decode(bytes);
        parsed = JSON.parse(text);
      } catch {
        return;
      }
    }

    // Unwrap SDK provenance envelope: {"header": {...}, "body": {...}}
    // The body holds the actual detection payload.
    const obj = parsed as Record<string, unknown> | null;
    const body = obj && typeof obj === 'object' && 'body' in obj && 'header' in obj
      ? (obj.body as Record<string, unknown>)
      : obj;
    const msg = snakeToCamel(body) as WorldDetectionsMsg | null;

    if (!msg || !Array.isArray(msg.detections)) return;

    const now = Date.now();
    const live = liveRef.current;
    const trails = trailsRef.current;

    const heat = heatmapRef.current;

    for (const d of msg.detections) {
      live.set(d.track_id, {
        x: d.world_x,
        y: d.world_y,
        class_name: d.class_name,
        last_seen: now,
      });

      const trail = trails.get(d.track_id) ?? [];
      trail.push({ x: d.world_x, y: d.world_y, ts: now });
      // Prune old trail points
      const cutoff = now - TRAIL_WINDOW_MS;
      const trimmed = trail.filter(p => p.ts >= cutoff);
      trails.set(d.track_id, trimmed);

      // Heatmap binning
      const gx = Math.round(d.world_x / HEATMAP_CELL_M) * HEATMAP_CELL_M;
      const gy = Math.round(d.world_y / HEATMAP_CELL_M) * HEATMAP_CELL_M;
      const key = `${gx.toFixed(2)},${gy.toFixed(2)}`;
      const cell = heat.get(key);
      if (cell) {
        cell.count += 1;
      } else {
        heat.set(key, { x: gx, y: gy, count: 1 });
      }
    }

    setRenderTick(t => t + 1);
  }, []);

  useZenohSubscription(subscribeTopic, handleSample);

  // Pose subscription — latest pose populates the camera marker popup.
  const [latestPose, setLatestPose] = useState<PoseMsg | null>(null);
  const handlePoseSample = useCallback((sample: Sample) => {
    try {
      const bytes = getSamplePayload(sample);
      const text = new TextDecoder().decode(bytes);
      const parsed = JSON.parse(text);
      const body = (parsed && typeof parsed === 'object'
        && 'body' in parsed && 'header' in parsed)
        ? (parsed as { body: unknown }).body
        : parsed;
      if (body && typeof body === 'object' && 'tilt_deg' in body) {
        setLatestPose(body as PoseMsg);
      }
    } catch {
      // ignore decode errors
    }
  }, []);
  useZenohSubscription('bubbaloop/global/**/pose', handlePoseSample);

  // Prune stale detections every 2s
  useEffect(() => {
    const id = setInterval(() => {
      const now = Date.now();
      let changed = false;
      for (const [id, dot] of liveRef.current) {
        if (now - dot.last_seen > STALE_MS) {
          liveRef.current.delete(id);
          trailsRef.current.delete(id);
          changed = true;
        }
      }
      if (changed) setRenderTick(t => t + 1);
    }, 2000);
    return () => clearInterval(id);
  }, []);

  // Heatmap decay — periodically multiply all cells by HEATMAP_DECAY_FACTOR
  // so old activity fades. Prune cells that fall below threshold.
  // Decay is disabled in full-screen mode where the heatmap shows historical
  // data — we want it stable, not fading.
  useEffect(() => {
    if (fullScreen) return;
    const id = setInterval(() => {
      const heat = heatmapRef.current;
      let changed = false;
      for (const [key, cell] of heat) {
        cell.count *= HEATMAP_DECAY_FACTOR;
        if (cell.count < HEATMAP_MIN_COUNT) {
          heat.delete(key);
          changed = true;
        }
      }
      if (changed || heat.size > 0) setRenderTick(t => t + 1);
    }, HEATMAP_DECAY_INTERVAL_MS);
    return () => clearInterval(id);
  }, [fullScreen]);

  // Backfill from Lance via spatial-map's queryable (full-screen mode only).
  // Runs on mount and whenever the time-range filter changes.
  useEffect(() => {
    if (!fullScreen) return;
    let cancelled = false;
    const session = getSession();
    if (!session) return;
    setBackfillState('loading');

    (async () => {
      const { start_ns, end_ns } = rangeToWindow(timeRange);
      const params: string[] = [];
      if (start_ns !== null) params.push(`start_ns=${start_ns}`);
      if (end_ns !== null) params.push(`end_ns=${end_ns}`);
      const paramStr = params.length ? `&${params.join('&')}` : '';

      const decodeReply = (replyItem: unknown): string | null => {
        let sample: unknown;
        if (replyItem instanceof Reply) {
          const result = replyItem.result();
          if (result instanceof ReplyError) return null;
          sample = result;
        } else {
          sample = replyItem;
        }
        const bytes = (sample as { payload: () => { toBytes: () => Uint8Array } })
          ?.payload?.()?.toBytes?.();
        if (!bytes) return null;
        return new TextDecoder().decode(bytes);
      };

      try {
        const opts = { timeout: Duration.milliseconds.of(8000) };

        // 1) Hourly histogram.
        const histReplies = await session.get(
          `bubbaloop/global/**/spatial-map/query?op=hourly${paramStr}`,
          opts,
        );
        if (histReplies && !cancelled) {
          for await (const reply of histReplies) {
            const text = decodeReply(reply);
            if (!text) continue;
            try {
              const parsed = JSON.parse(text) as { buckets?: number[]; total?: number; error?: string };
              if (parsed.buckets) setHourlyBuckets(parsed.buckets);
              if (typeof parsed.total === 'number') setTotalRows(parsed.total);
            } catch (e) {
              console.error('[SiteMapView] hist parse failed', e, text.slice(0, 200));
            }
          }
        }

        // 2) Rows for the heatmap backfill.
        const rowReplies = await session.get(
          `bubbaloop/global/**/spatial-map/query?op=rows&limit=10000${paramStr}`,
          opts,
        );
        if (rowReplies && !cancelled) {
          heatmapRef.current.clear();
          let totalRowsBackfilled = 0;
          for await (const reply of rowReplies) {
            const text = decodeReply(reply);
            if (!text) continue;
            try {
              const parsed = JSON.parse(text) as { rows?: Array<{ world_x: number; world_y: number }> };
              if (!parsed.rows) continue;
              const heat = heatmapRef.current;
              for (const r of parsed.rows) {
                const gx = Math.round(r.world_x / HEATMAP_CELL_M) * HEATMAP_CELL_M;
                const gy = Math.round(r.world_y / HEATMAP_CELL_M) * HEATMAP_CELL_M;
                const key = `${gx.toFixed(2)},${gy.toFixed(2)}`;
                const cell = heat.get(key);
                if (cell) cell.count += 1;
                else heat.set(key, { x: gx, y: gy, count: 1 });
              }
              totalRowsBackfilled += parsed.rows.length;
            } catch (e) {
              console.error('[SiteMapView] rows parse failed', e, text.slice(0, 200));
            }
          }
          if (!cancelled) {
            console.log(`[SiteMapView] backfilled ${totalRowsBackfilled} rows → ${heatmapRef.current.size} cells`);
            setBackfillState('ok');
            setRenderTick(t => t + 1);
          }
        }
      } catch (err) {
        console.error('[SiteMapView] backfill failed', err);
        if (!cancelled) setBackfillState('err');
      }
    })();

    return () => { cancelled = true; };
  }, [fullScreen, timeRange, getSession]);

  // ---- Pan/zoom ----
  const handleWheel = useCallback((e: WheelEvent) => {
    e.preventDefault();
    const svg = svgRef.current;
    if (!svg) return;
    const rect = svg.getBoundingClientRect();
    const vb = viewBoxRef.current;
    const mx = vb.x + ((e.clientX - rect.left) / rect.width) * vb.w;
    const my = vb.y + ((e.clientY - rect.top) / rect.height) * vb.h;
    const factor = e.deltaY > 0 ? 1.08 : 0.93;
    const newW = Math.max(100, Math.min(8000, vb.w * factor));
    const newH = Math.max(80, Math.min(6000, vb.h * factor));
    const newX = mx - ((mx - vb.x) / vb.w) * newW;
    const newY = my - ((my - vb.y) / vb.h) * newH;
    setViewBox({ x: newX, y: newY, w: newW, h: newH });
  }, []);

  useEffect(() => {
    const svg = svgRef.current;
    if (!svg) return;
    svg.addEventListener('wheel', handleWheel, { passive: false });
    return () => svg.removeEventListener('wheel', handleWheel);
  }, [handleWheel]);

  const handlePointerDown = useCallback((e: React.PointerEvent<SVGSVGElement>) => {
    isPanningRef.current = true;
    panStartRef.current = {
      x: e.clientX,
      y: e.clientY,
      vbx: viewBoxRef.current.x,
      vby: viewBoxRef.current.y,
    };
    (e.currentTarget as SVGSVGElement).setPointerCapture(e.pointerId);
  }, []);

  const handlePointerMove = useCallback((e: React.PointerEvent<SVGSVGElement>) => {
    if (!isPanningRef.current) return;
    const svg = svgRef.current;
    if (!svg) return;
    const rect = svg.getBoundingClientRect();
    const vb = viewBoxRef.current;
    const scaleX = vb.w / rect.width;
    const scaleY = vb.h / rect.height;
    const dx = (e.clientX - panStartRef.current.x) * scaleX;
    const dy = (e.clientY - panStartRef.current.y) * scaleY;
    setViewBox({
      ...vb,
      x: panStartRef.current.vbx - dx,
      y: panStartRef.current.vby - dy,
    });
  }, []);

  const handlePointerUp = useCallback((e: React.PointerEvent<SVGSVGElement>) => {
    isPanningRef.current = false;
    (e.currentTarget as SVGSVGElement).releasePointerCapture(e.pointerId);
  }, []);

  const resetView = useCallback(() => setViewBox(INITIAL_VIEW), []);

  // ---- Render helpers ----
  const now = Date.now();

  // Compute grid bounds from viewBox (in world coords, meters)
  const worldMinX = viewBox.x / SCALE;
  const worldMaxX = (viewBox.x + viewBox.w) / SCALE;
  const worldMinY = -(viewBox.y + viewBox.h) / SCALE;
  const worldMaxY = -viewBox.y / SCALE;

  const gridLines: React.ReactNode[] = [];
  const startX = Math.floor(worldMinX / GRID_SPACING) * GRID_SPACING;
  const endX = Math.ceil(worldMaxX / GRID_SPACING) * GRID_SPACING;
  const startY = Math.floor(worldMinY / GRID_SPACING) * GRID_SPACING;
  const endY = Math.ceil(worldMaxY / GRID_SPACING) * GRID_SPACING;

  for (let wx = startX; wx <= endX; wx += GRID_SPACING) {
    const [sx] = worldToSvg(wx, 0);
    const [, sy1] = worldToSvg(0, worldMinY);
    const [, sy2] = worldToSvg(0, worldMaxY);
    const isMajor = wx % 5 === 0;
    gridLines.push(
      <line
        key={`vx${wx}`}
        x1={sx} y1={sy1} x2={sx} y2={sy2}
        stroke={isMajor ? '#334155' : '#1e293b'}
        strokeWidth={isMajor ? 0.5 : 0.25}
      />
    );
    if (isMajor) {
      gridLines.push(
        <text
          key={`vxt${wx}`}
          x={sx + 1}
          y={sy2 + 10}
          fontSize={8}
          fill="#475569"
          textAnchor="middle"
        >{wx}m</text>
      );
    }
  }
  for (let wy = startY; wy <= endY; wy += GRID_SPACING) {
    const [, sy] = worldToSvg(0, wy);
    const [sx1] = worldToSvg(worldMinX, 0);
    const [sx2] = worldToSvg(worldMaxX, 0);
    const isMajor = wy % 5 === 0;
    gridLines.push(
      <line
        key={`hy${wy}`}
        x1={sx1} y1={sy} x2={sx2} y2={sy}
        stroke={isMajor ? '#334155' : '#1e293b'}
        strokeWidth={isMajor ? 0.5 : 0.25}
      />
    );
    if (isMajor) {
      gridLines.push(
        <text
          key={`hyt${wy}`}
          x={sx1 - 2}
          y={sy + 3}
          fontSize={8}
          fill="#475569"
          textAnchor="end"
        >{wy}m</text>
      );
    }
  }

  // Zone polygons
  const zoneElements = zones.map(zone => {
    const points = zone.polygon.map(([wx, wy]) => {
      const [sx, sy] = worldToSvg(wx, wy);
      return `${sx},${sy}`;
    }).join(' ');
    // Centroid for label
    const cx = zone.polygon.reduce((s, [x]) => s + x, 0) / zone.polygon.length;
    const cy = zone.polygon.reduce((s, [, y]) => s + y, 0) / zone.polygon.length;
    const [lx, ly] = worldToSvg(cx, cy);
    return (
      <g key={zone.id}>
        <polygon
          points={points}
          fill={zone.color}
          fillOpacity={0.3}
          stroke={zone.color}
          strokeWidth={1}
          strokeOpacity={0.6}
        />
        <text x={lx} y={ly} textAnchor="middle" dominantBaseline="middle" fontSize={10} fill={zone.color} fontWeight="bold">
          {zone.name}
        </text>
      </g>
    );
  });

  // Heatmap cells (rendered from accumulated detections in heatmapRef)
  const heatCells = showHeatmap ? Array.from(heatmapRef.current.values()) : [];
  const maxCount = heatCells.length ? Math.max(...heatCells.map(c => c.count), 1) : 1;
  const heatElements = heatCells.length ? heatCells.map((cell, i) => {
    const t = cell.count / maxCount; // 0..1
    // Blue (cold) → red (hot)
    const r = Math.round(t * 239 + (1 - t) * 59);
    const g = Math.round(t * 68 + (1 - t) * 130);
    const b = Math.round(t * 68 + (1 - t) * 246);
    const [sx, sy] = worldToSvg(cell.x, cell.y);
    const cellPx = SCALE * HEATMAP_CELL_M * 1.1;
    return (
      <rect
        key={i}
        x={sx - cellPx / 2}
        y={sy - cellPx / 2}
        width={cellPx}
        height={cellPx}
        fill={`rgb(${r},${g},${b})`}
        fillOpacity={0.35 + t * 0.45}
        filter="url(#heatBlur)"
      />
    );
  }) : null;

  // Trails
  const trailElements: React.ReactNode[] = [];
  for (const [trackId, points] of trailsRef.current) {
    if (points.length < 2) continue;
    const dot = liveRef.current.get(trackId);
    const color = dot ? classColor(dot.class_name) : CLASS_COLORS.default;
    // Draw segments with fading opacity
    for (let i = 1; i < points.length; i++) {
      const prev = points[i - 1];
      const curr = points[i];
      const ageS = (now - curr.ts) / 1000;
      const opacity = Math.max(0, 1 - ageS / 30);
      const [x1, y1] = worldToSvg(prev.x, prev.y);
      const [x2, y2] = worldToSvg(curr.x, curr.y);
      trailElements.push(
        <line
          key={`${trackId}-${i}`}
          x1={x1} y1={y1} x2={x2} y2={y2}
          stroke={color}
          strokeWidth={1.5}
          strokeOpacity={opacity}
          strokeLinecap="round"
        />
      );
    }
  }

  // Live dots
  const dotElements: React.ReactNode[] = [];
  for (const [trackId, dot] of liveRef.current) {
    const [sx, sy] = worldToSvg(dot.x, dot.y);
    const color = classColor(dot.class_name);
    dotElements.push(
      <g key={trackId}>
        <circle cx={sx} cy={sy} r={6} fill={color} fillOpacity={0.9} />
        <circle cx={sx} cy={sy} r={9} fill="none" stroke={color} strokeWidth={1} strokeOpacity={0.4} />
        <text x={sx} y={sy - 12} textAnchor="middle" fontSize={7} fill={color} fontWeight="500">
          {dot.class_name}
        </text>
      </g>
    );
  }

  const dotCount = liveRef.current.size;

  // Suppress unused renderTick lint
  void renderTick;

  // ─── FULL-SCREEN PAGE LAYOUT (Leaflet-based) ─────────────────────────
  if (fullScreen) {
    return (
      <div className="map-page">
        <LeafletMap
          heatmapRef={heatmapRef}
          liveRef={liveRef}
          trailsRef={trailsRef}
          renderTick={renderTick}
          showHeatmap={showHeatmap}
          zones={zones}
          latestPose={latestPose}
        />

        {/* Floating top-left: title + time range chips */}
        <div className="map-overlay map-overlay-tl glass">
          <div className="overlay-title">
            <span className="dot-pulse" />
            <span>Site Map</span>
            <span className="overlay-sub">
              {backfillState === 'loading' ? 'syncing…'
                : backfillState === 'err' ? 'error'
                : `${totalRows.toLocaleString()} obs`}
            </span>
          </div>
          <div className="time-chips">
            {(['1h', '6h', '24h', '7d', 'all'] as TimeRange[]).map(r => (
              <button
                key={r}
                className={`chip ${timeRange === r ? 'chip-active' : ''}`}
                onClick={() => setTimeRange(r)}
              >
                {r}
              </button>
            ))}
          </div>
        </div>

        {/* Floating top-right: stats grid + heatmap toggle (collapses on mobile) */}
        <div className="map-overlay map-overlay-tr glass">
          <div className="stats-grid">
            <div className="stat-cell">
              <div className="stat-cell-num">{liveRef.current.size}</div>
              <div className="stat-cell-lbl">live</div>
            </div>
            <div className="stat-cell">
              <div className="stat-cell-num">{heatmapRef.current.size}</div>
              <div className="stat-cell-lbl">cells</div>
            </div>
            <div className="stat-cell stat-cell-wide">
              <div className="stat-cell-num">{totalRows.toLocaleString()}</div>
              <div className="stat-cell-lbl">total</div>
            </div>
          </div>
          <div className="overlay-controls">
            <button
              className={`pill ${showHeatmap ? 'pill-on' : ''}`}
              onClick={() => setShowHeatmap(s => !s)}
            >
              <svg width="12" height="12" viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="2.2">
                <rect x="3" y="3" width="7" height="7" rx="1" />
                <rect x="14" y="3" width="7" height="7" rx="1" />
                <rect x="3" y="14" width="7" height="7" rx="1" />
                <rect x="14" y="14" width="7" height="7" rx="1" />
              </svg>
              Heatmap
            </button>
            <button className="pill" onClick={resetView} title="Reset view">
              <svg width="12" height="12" viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="2.2">
                <path d="M3 12a9 9 0 1 0 9-9 9.75 9.75 0 0 0-6.74 2.74L3 8" />
                <path d="M3 3v5h5" />
              </svg>
            </button>
          </div>
        </div>

        {/* Floating bottom: hourly histogram */}
        <div className="map-overlay map-overlay-b glass">
          <div className="overlay-section-label">Activity by hour</div>
          <HourHistogram buckets={hourlyBuckets} />
        </div>

        {/* Page-level styles for the new design */}
        <style>{mapPageStyles}</style>
      </div>
    );
  }

  // ─── ORIGINAL PANEL LAYOUT (unchanged for non-fullScreen use) ───────
  return (
    <div className="site-map-card">
      <div className="site-map-header">
        <div className="site-map-drag" {...dragHandleProps}>
          <svg width="14" height="14" viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="2">
            <path d="M9 3H5a2 2 0 00-2 2v4m6-6h10a2 2 0 012 2v4M9 3v18m0 0h10a2 2 0 002-2v-4M9 21H5a2 2 0 01-2-2v-4m0 0h18" />
          </svg>
          <span>Site Map</span>
        </div>
        <div className="site-map-meta">
          {dotCount > 0 && (
            <span className="site-map-badge">{dotCount} live</span>
          )}
          <div className="site-map-controls">
            <button
              className="site-map-ctrl-btn"
              onClick={() => setShowHeatmap(s => !s)}
              title={showHeatmap ? 'Hide heatmap' : 'Show heatmap'}
              style={{ opacity: showHeatmap ? 1 : 0.4 }}
            >
              <svg width="12" height="12" viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="2">
                <rect x="3" y="3" width="7" height="7" />
                <rect x="14" y="3" width="7" height="7" />
                <rect x="3" y="14" width="7" height="7" />
                <rect x="14" y="14" width="7" height="7" />
              </svg>
            </button>
            <button className="site-map-ctrl-btn" onClick={resetView} title="Reset view">
              <svg width="12" height="12" viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="2">
                <path d="M3 12a9 9 0 1 0 9-9 9.75 9.75 0 0 0-6.74 2.74L3 8" />
                <path d="M3 3v5h5" />
              </svg>
            </button>
            {!fullScreen && (
              <button className="site-map-ctrl-btn site-map-remove" onClick={onRemove} title="Remove panel">
                <svg width="12" height="12" viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="2">
                  <path d="M18 6L6 18M6 6l12 12" />
                </svg>
              </button>
            )}
          </div>
        </div>
      </div>

      <div className="site-map-body">
        <svg
          ref={svgRef}
          className="site-map-svg"
          viewBox={`${viewBox.x} ${viewBox.y} ${viewBox.w} ${viewBox.h}`}
          onPointerDown={handlePointerDown}
          onPointerMove={handlePointerMove}
          onPointerUp={handlePointerUp}
          style={{ cursor: isPanningRef.current ? 'grabbing' : 'grab' }}
        >
          <defs>
            <filter id="heatBlur" x="-50%" y="-50%" width="200%" height="200%">
              <feGaussianBlur stdDeviation={SCALE * 0.6} />
            </filter>
          </defs>

          {/* Grid */}
          <g className="site-map-grid">{gridLines}</g>

          {/* Origin crosshair */}
          <line x1={-10} y1={0} x2={10} y2={0} stroke="#64748b" strokeWidth={0.8} />
          <line x1={0} y1={-10} x2={0} y2={10} stroke="#64748b" strokeWidth={0.8} />

          {/* Heatmap */}
          {heatElements && <g className="site-map-heatmap">{heatElements}</g>}

          {/* Zones */}
          <g className="site-map-zones">{zoneElements}</g>

          {/* Trails */}
          <g className="site-map-trails">{trailElements}</g>

          {/* Live dots */}
          <g className="site-map-dots">{dotElements}</g>
        </svg>

        {dotCount === 0 && (
          <div className="site-map-empty">
            <svg width="24" height="24" viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="1.5">
              <circle cx="12" cy="12" r="3" />
              <path d="M12 2v2M12 20v2M4.22 4.22l1.42 1.42M18.36 18.36l1.42 1.42M2 12h2M20 12h2M4.22 19.78l1.42-1.42M18.36 5.64l1.42-1.42" />
            </svg>
            <span>No detections on <code>world_detections</code></span>
          </div>
        )}
      </div>

      {/* Legend */}
      <div className="site-map-legend">
        {Object.entries(CLASS_COLORS).filter(([k]) => k !== 'default').map(([cls, color]) => (
          <span key={cls} className="site-map-legend-item">
            <span className="site-map-legend-dot" style={{ background: color }} />
            {cls}
          </span>
        ))}
      </div>

      <style>{`
        .site-map-card {
          background: var(--bg-card);
          border: 1px solid var(--border-color);
          border-radius: 12px;
          overflow: hidden;
          display: flex;
          flex-direction: column;
          min-height: 380px;
        }

        .site-map-header {
          display: flex;
          align-items: center;
          justify-content: space-between;
          padding: 10px 14px;
          border-bottom: 1px solid var(--border-color);
          background: var(--bg-secondary);
          flex-shrink: 0;
        }

        .site-map-drag {
          display: flex;
          align-items: center;
          gap: 8px;
          font-size: 13px;
          font-weight: 500;
          color: var(--text-secondary);
          cursor: grab;
          user-select: none;
        }

        .site-map-drag:active {
          cursor: grabbing;
        }

        .site-map-meta {
          display: flex;
          align-items: center;
          gap: 8px;
        }

        .site-map-badge {
          font-size: 11px;
          padding: 2px 8px;
          background: #3B82F620;
          color: #3B82F6;
          border: 1px solid #3B82F640;
          border-radius: 20px;
        }

        .site-map-controls {
          display: flex;
          align-items: center;
          gap: 4px;
        }

        .site-map-ctrl-btn {
          display: flex;
          align-items: center;
          justify-content: center;
          width: 26px;
          height: 26px;
          background: transparent;
          border: 1px solid var(--border-color);
          border-radius: 6px;
          color: var(--text-muted);
          cursor: pointer;
          transition: all 0.15s;
        }

        .site-map-ctrl-btn:hover {
          background: var(--bg-tertiary);
          color: var(--text-secondary);
        }

        .site-map-remove:hover {
          border-color: #ef444460;
          color: #ef4444;
          background: #ef444415;
        }

        .site-map-body {
          flex: 1;
          position: relative;
          background: #0a0f1a;
          min-height: 300px;
        }

        .site-map-svg {
          width: 100%;
          height: 100%;
          display: block;
          min-height: 300px;
          touch-action: none;
        }

        .site-map-empty {
          position: absolute;
          inset: 0;
          display: flex;
          flex-direction: column;
          align-items: center;
          justify-content: center;
          gap: 10px;
          color: #475569;
          font-size: 12px;
          pointer-events: none;
        }

        .site-map-empty code {
          font-size: 11px;
          color: #334155;
        }

        .site-map-legend {
          display: flex;
          align-items: center;
          gap: 12px;
          padding: 6px 14px;
          border-top: 1px solid var(--border-color);
          background: var(--bg-secondary);
          flex-shrink: 0;
          flex-wrap: wrap;
        }

        .site-map-legend-item {
          display: flex;
          align-items: center;
          gap: 5px;
          font-size: 11px;
          color: var(--text-muted);
        }

        .site-map-legend-dot {
          width: 8px;
          height: 8px;
          border-radius: 50%;
          flex-shrink: 0;
        }

        /* ── Full-screen page layout ──────────────────────────────────── */
        .site-map-page {
          display: grid;
          grid-template-columns: 280px 1fr;
          gap: 0;
          width: 100%;
          height: 100%;
          flex: 1;
          background: var(--bg-primary);
          border: 1px solid var(--border-color);
          border-radius: 12px;
          overflow: hidden;
        }
        .site-map-page .site-map-header {
          grid-column: 2;
        }
        .site-map-page .site-map-body {
          grid-column: 2;
          flex: 1;
        }
        .site-map-sidebar {
          grid-column: 1;
          grid-row: 1 / -1;
          background: var(--bg-secondary);
          border-right: 1px solid var(--border-color);
          padding: 18px 16px;
          display: flex;
          flex-direction: column;
          gap: 18px;
          overflow-y: auto;
        }
        .sidebar-block { display: flex; flex-direction: column; gap: 6px; }
        .sidebar-block h3 {
          margin: 0;
          font-size: 16px;
          font-weight: 600;
          color: var(--text-primary);
        }
        .sidebar-sub {
          margin: 0;
          font-size: 12px;
          color: var(--text-tertiary);
        }
        .sidebar-label {
          font-size: 11px;
          font-weight: 500;
          text-transform: uppercase;
          letter-spacing: 0.5px;
          color: var(--text-tertiary);
        }
        .sidebar-segs {
          display: flex;
          gap: 4px;
          background: var(--bg-tertiary);
          padding: 3px;
          border-radius: 6px;
        }
        .sidebar-seg {
          flex: 1;
          padding: 4px 0;
          font-size: 11px;
          background: transparent;
          color: var(--text-secondary);
          border: none;
          border-radius: 4px;
          cursor: pointer;
          transition: all 0.12s;
        }
        .sidebar-seg.active {
          background: var(--color-primary, #3b82f6);
          color: white;
          font-weight: 500;
        }
        .sidebar-seg:hover:not(.active) { background: var(--bg-hover, rgba(255,255,255,0.04)); }
        .sidebar-toggle {
          padding: 6px 12px;
          font-size: 12px;
          background: var(--bg-tertiary);
          color: var(--text-secondary);
          border: 1px solid var(--border-color);
          border-radius: 6px;
          cursor: pointer;
        }
        .sidebar-toggle.on {
          background: var(--color-primary, #3b82f6);
          color: white;
          border-color: var(--color-primary, #3b82f6);
        }
        .sidebar-stats {
          flex-direction: row;
          gap: 12px;
          margin-top: auto;
          padding-top: 16px;
          border-top: 1px solid var(--border-color);
        }
        .stat { flex: 1; }
        .stat-num {
          font-size: 22px;
          font-weight: 600;
          color: var(--text-primary);
          line-height: 1;
        }
        .stat-label {
          font-size: 10px;
          text-transform: uppercase;
          letter-spacing: 0.5px;
          color: var(--text-tertiary);
          margin-top: 4px;
        }
        .hour-histogram { display: block; }
      `}</style>
    </div>
  );
}

// Build a popup body for the camera marker showing the latest pose.
// Uses safe DOM construction (createElement + textContent) — no innerHTML.
function buildPosePopup(pose: PoseMsg | null): HTMLElement {
  const root = document.createElement('div');
  root.className = 'pose-popup-body';

  const title = document.createElement('div');
  title.className = 'pose-popup-title';
  title.textContent = pose?.camera ? `📷 ${pose.camera}` : '📷 Camera';
  root.appendChild(title);

  if (!pose) {
    const sub = document.createElement('div');
    sub.className = 'pose-popup-empty';
    sub.textContent = 'Awaiting pose…';
    root.appendChild(sub);
    return root;
  }

  // Status pill
  const status = pose.tracking_state || 'OK';
  const statusEl = document.createElement('div');
  statusEl.className = `pose-popup-status status-${status.toLowerCase()}`;
  statusEl.textContent = status;
  root.appendChild(statusEl);

  // Stats grid
  const grid = document.createElement('div');
  grid.className = 'pose-popup-grid';

  const addStat = (label: string, value: string) => {
    const lbl = document.createElement('div');
    lbl.className = 'pose-popup-label';
    lbl.textContent = label;
    const val = document.createElement('div');
    val.className = 'pose-popup-value';
    val.textContent = value;
    grid.appendChild(lbl);
    grid.appendChild(val);
  };

  if (typeof pose.tilt_deg === 'number') {
    addStat('Tilt', `${pose.tilt_deg.toFixed(1)}°`);
  }
  if (typeof pose.height_m === 'number') {
    addStat('Height', `${pose.height_m.toFixed(2)} m`);
  }
  if (Array.isArray(pose.t_world) && pose.t_world.length === 3) {
    const [x, y, z] = pose.t_world;
    addStat('Position', `(${x.toFixed(2)}, ${y.toFixed(2)}, ${z.toFixed(2)}) m`);
  }
  if (pose.source) {
    addStat('Source', pose.source);
  }
  if (typeof pose.timestamp_ns === 'number') {
    const dt = new Date(pose.timestamp_ns / 1_000_000);
    const ageS = (Date.now() - dt.getTime()) / 1000;
    const ageStr = ageS < 60 ? `${ageS.toFixed(1)}s ago`
      : ageS < 3600 ? `${(ageS / 60).toFixed(1)}m ago`
      : `${(ageS / 3600).toFixed(1)}h ago`;
    addStat('Last update', ageStr);
  }

  root.appendChild(grid);
  return root;
}

// ─── LeafletMap ──────────────────────────────────────────────────────────────
// Wraps a Leaflet instance with L.CRS.Simple. World coords (x meters east,
// y meters forward) translate to Leaflet [lat, lng] = [-y, x].
function LeafletMap({
  heatmapRef,
  liveRef,
  trailsRef,
  renderTick,
  showHeatmap,
  zones,
  latestPose,
}: {
  heatmapRef: React.MutableRefObject<Map<string, { x: number; y: number; count: number }>>;
  liveRef: React.MutableRefObject<Map<string, LiveDot>>;
  trailsRef: React.MutableRefObject<Map<string, TrailPoint[]>>;
  renderTick: number;
  showHeatmap: boolean;
  zones: Zone[];
  latestPose: PoseMsg | null;
}) {
  const containerRef = useRef<HTMLDivElement>(null);
  const mapRef = useRef<L.Map | null>(null);
  const heatLayerRef = useRef<ReturnType<typeof L.heatLayer> | null>(null);
  const liveLayerRef = useRef<L.LayerGroup | null>(null);
  const trailLayerRef = useRef<L.LayerGroup | null>(null);
  const zoneLayerRef = useRef<L.LayerGroup | null>(null);
  const cameraCoreRef = useRef<L.CircleMarker | null>(null);

  const wToLL = (wx: number, wy: number): [number, number] => [-wy, wx];

  useEffect(() => {
    if (!containerRef.current || mapRef.current) return;

    const map = L.map(containerRef.current, {
      crs: L.CRS.Simple,
      minZoom: -3,
      maxZoom: 5,
      zoomSnap: 0.25,
      zoomDelta: 0.5,
      attributionControl: false,
      preferCanvas: true,
      zoomControl: true,
    });
    mapRef.current = map;

    // Initial view ~12m × 12m centred on origin, with the camera at the
    // bottom of frame and Y > 0 (world-forward) extending upward.
    // Camera-local floor frame: origin = directly below camera at floor level.
    map.fitBounds([[-2, -8], [12, 8]]);

    // Camera marker at world origin (directly below mounted camera).
    // Decorative layers (non-interactive, just visual):
    //   - faint halo (radius 24, low opacity)
    //   - mid-ring (radius 12)
    //   - solid core (radius 6, bright)
    // Plus a single large INVISIBLE click target (radius 28) bound to popup +
    // permanent label, so any tap within ~56px of the origin opens the popup.
    const cameraGroup = L.layerGroup().addTo(map);
    L.circleMarker([0, 0], {
      radius: 24,
      color: 'transparent',
      fillColor: '#22d3ee',
      fillOpacity: 0.10,
      weight: 0,
      interactive: false,
    }).addTo(cameraGroup);
    L.circleMarker([0, 0], {
      radius: 12,
      color: '#22d3ee',
      fillColor: '#22d3ee',
      fillOpacity: 0.20,
      weight: 1.5,
      interactive: false,
    }).addTo(cameraGroup);
    L.circleMarker([0, 0], {
      radius: 6,
      color: '#0a0e1a',
      fillColor: '#22d3ee',
      fillOpacity: 1,
      weight: 2,
      interactive: false,
    }).addTo(cameraGroup);

    // Large invisible click target — this is what gets the popup + tooltip.
    const clickTarget = L.circleMarker([0, 0], {
      radius: 28,
      color: 'transparent',
      fillColor: 'transparent',
      fillOpacity: 0,
      weight: 0,
      interactive: true,
      bubblingMouseEvents: false,
    })
      .bindTooltip('📷 Camera', {
        permanent: true,
        direction: 'right',
        offset: [10, 0],
        className: 'camera-tooltip',
      })
      .addTo(cameraGroup);

    // Explicit click handler ensures popup opens reliably even on mobile.
    clickTarget.on('click', () => {
      clickTarget.openPopup();
    });

    cameraCoreRef.current = clickTarget;

    heatLayerRef.current = L.heatLayer([], {
      radius: 28,
      blur: 22,
      maxZoom: 4,
      minOpacity: 0.25,
      gradient: {
        0.2: '#1e3a8a',
        0.4: '#2563eb',
        0.6: '#06b6d4',
        0.8: '#facc15',
        1.0: '#ef4444',
      },
    }).addTo(map);

    trailLayerRef.current = L.layerGroup().addTo(map);
    liveLayerRef.current = L.layerGroup().addTo(map);
    zoneLayerRef.current = L.layerGroup().addTo(map);

    return () => {
      map.remove();
      mapRef.current = null;
      heatLayerRef.current = null;
      liveLayerRef.current = null;
      trailLayerRef.current = null;
      zoneLayerRef.current = null;
    };
  }, []);

  useEffect(() => {
    const layer = zoneLayerRef.current;
    if (!layer) return;
    layer.clearLayers();
    for (const z of zones) {
      const latlngs = z.polygon.map(([wx, wy]) => wToLL(wx, wy));
      L.polygon(latlngs, {
        color: z.color,
        weight: 1.5,
        opacity: 0.7,
        fillOpacity: 0.15,
      })
        .bindTooltip(z.name)
        .addTo(layer);
    }
  }, [zones]);

  // Bind / refresh the camera popup whenever latestPose updates.
  useEffect(() => {
    const marker = cameraCoreRef.current;
    if (!marker) return;
    const popupContent = buildPosePopup(latestPose);
    marker.bindPopup(popupContent, {
      className: 'pose-popup',
      maxWidth: 280,
      closeButton: true,
      autoPan: true,
    });
  }, [latestPose]);

  useEffect(() => {
    const map = mapRef.current;
    const heat = heatLayerRef.current;
    if (!map || !heat) return;
    if (showHeatmap && !map.hasLayer(heat)) heat.addTo(map);
    if (!showHeatmap && map.hasLayer(heat)) map.removeLayer(heat);
  }, [showHeatmap]);

  // On every renderTick, re-derive layer contents from parent refs.
  useEffect(() => {
    const heat = heatLayerRef.current;
    const liveLayer = liveLayerRef.current;
    const trailLayer = trailLayerRef.current;
    if (!heat || !liveLayer || !trailLayer) return;

    const cells = Array.from(heatmapRef.current.values());
    const max = cells.reduce((m, c) => Math.max(m, c.count), 1);
    heat.setLatLngs(
      cells.map(c => [-c.y, c.x, c.count / max] as [number, number, number]),
    );

    liveLayer.clearLayers();
    for (const dot of liveRef.current.values()) {
      L.circleMarker(wToLL(dot.x, dot.y), {
        radius: 7,
        color: '#3b82f6',
        fillColor: '#60a5fa',
        fillOpacity: 0.9,
        weight: 2,
      })
        .bindTooltip(dot.class_name, { direction: 'top', offset: [0, -8] })
        .addTo(liveLayer);
    }

    trailLayer.clearLayers();
    for (const [, points] of trailsRef.current) {
      if (points.length < 2) continue;
      const latlngs = points.map(p => wToLL(p.x, p.y));
      L.polyline(latlngs, {
        color: '#60a5fa',
        weight: 2,
        opacity: 0.55,
      }).addTo(trailLayer);
    }
  }, [renderTick, heatmapRef, liveRef, trailsRef]);

  return <div ref={containerRef} className="leaflet-canvas" />;
}

// ─── HourHistogram ───────────────────────────────────────────────────────────
function HourHistogram({ buckets }: { buckets: number[] }) {
  const max = Math.max(1, ...buckets);
  const W = 240;
  const H = 80;
  const barW = W / 24;
  return (
    <svg width={W} height={H + 18} className="hour-histogram">
      {buckets.map((n, h) => {
        const barH = (n / max) * H;
        const x = h * barW;
        const y = H - barH;
        return (
          <g key={h}>
            <rect
              x={x + 1}
              y={y}
              width={barW - 2}
              height={barH}
              fill={n > 0 ? 'var(--color-primary, #3b82f6)' : 'transparent'}
              opacity={0.85}
            />
            <text
              x={x + barW / 2}
              y={H + 12}
              fontSize={8}
              textAnchor="middle"
              fill="var(--text-tertiary, #888)"
            >
              {h % 6 === 0 ? h : ''}
            </text>
          </g>
        );
      })}
      <line x1={0} y1={H} x2={W} y2={H} stroke="var(--border-color)" strokeWidth={0.5} />
    </svg>
  );
}
