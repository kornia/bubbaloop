import { useRef, useEffect, useCallback, useState } from 'react';
import { Sample } from '@eclipse-zenoh/zenoh-ts';
import { getSamplePayload, getEncodingInfo, hasExplicitEncoding, EncodingPredefined } from '../lib/zenoh';
import { decode as cborDecode } from 'cbor-x';
import { useZenohSubscription } from '../hooks/useZenohSubscription';

const COLORS = [
  '#3d5afe', '#00e5ff', '#00c853',
  '#ffd600', '#ff6d00', '#e040fb',
  '#ff1744', '#00bfa5', '#76ff03',
  '#f50057', '#40c4ff', '#ffab40',
];

// Fields excluded from channel discovery — they are timestamps / sequence numbers,
// not sensor signals, and their large magnitudes would corrupt the Y scale.
const EXCLUDE_PATHS = new Set(['timestamp_us', 'timestamp_ns', 'timestamp', 'sequence', 'seq']);
const EXCLUDE_SUFFIXES = ['_us', '_ns', '_ms', 'time', 'Time'];

function isExcluded(path: string): boolean {
  const leaf = path.split('.').pop()!;
  if (EXCLUDE_PATHS.has(leaf)) return true;
  return EXCLUDE_SUFFIXES.some(s => leaf.endsWith(s) || leaf.startsWith('timestamp'));
}

interface Point { t: number; v: number }
interface Channel { path: string; points: Point[]; color: string }

function flattenNumeric(obj: unknown, prefix = ''): { path: string; value: number }[] {
  if (typeof obj === 'number') return prefix && !isExcluded(prefix) ? [{ path: prefix, value: obj }] : [];
  if (typeof obj === 'bigint') return prefix && !isExcluded(prefix) ? [{ path: prefix, value: Number(obj) }] : [];
  if (obj && typeof obj === 'object' && !ArrayBuffer.isView(obj) && !Array.isArray(obj)) {
    return Object.entries(obj as Record<string, unknown>).flatMap(([k, v]) =>
      flattenNumeric(v, prefix ? `${prefix}.${k}` : k)
    );
  }
  return [];
}

interface DragHandleProps { [key: string]: unknown }

interface TimeseriesViewProps {
  panelName: string;
  topic: string;
  windowSecs?: number;
  isHidden?: boolean;
  onTopicChange?: (topic: string) => void;
  onRemove?: () => void;
  availableTopics?: Array<{ display: string; raw: string }>;
  dragHandleProps?: DragHandleProps;
}

export function TimeseriesView({
  panelName,
  topic,
  windowSecs = 10,
  isHidden = false,
  onTopicChange,
  onRemove,
  availableTopics = [],
  dragHandleProps,
}: TimeseriesViewProps) {
  const canvasRef = useRef<HTMLCanvasElement>(null);
  const channelsRef = useRef<Map<string, Channel>>(new Map());
  const rafRef = useRef<number | null>(null);
  const [channelNames, setChannelNames] = useState<string[]>([]);
  const [disabled, setDisabled] = useState<Set<string>>(new Set());
  const disabledRef = useRef<Set<string>>(new Set());
  const [showTopicEditor, setShowTopicEditor] = useState(!topic);
  const lastValuesRef = useRef<Map<string, number>>(new Map());

  const windowMs = windowSecs * 1000;

  // Keep disabledRef in sync with state so the draw loop sees current value
  useEffect(() => { disabledRef.current = disabled; }, [disabled]);

  const toggleChannel = useCallback((path: string) => {
    setDisabled(prev => {
      const next = new Set(prev);
      next.has(path) ? next.delete(path) : next.add(path);
      return next;
    });
  }, []);

  const draw = useCallback(() => {
    const canvas = canvasRef.current;
    if (!canvas || canvas.width === 0 || canvas.height === 0) {
      rafRef.current = requestAnimationFrame(draw);
      return;
    }
    const ctx = canvas.getContext('2d');
    if (!ctx) { rafRef.current = requestAnimationFrame(draw); return; }

    const W = canvas.width;
    const H = canvas.height;
    const now = Date.now();
    const tMin = now - windowMs;
    const dis = disabledRef.current;

    ctx.clearRect(0, 0, W, H);
    ctx.fillStyle = '#15151f';
    ctx.fillRect(0, 0, W, H);

    const channels = Array.from(channelsRef.current.values());
    if (channels.length === 0) { rafRef.current = requestAnimationFrame(draw); return; }

    // Y-axis label width — reserve left margin for tick labels
    ctx.font = '9px "JetBrains Mono", monospace';
    const Y_MARGIN = 42;

    const toX = (t: number) => Y_MARGIN + ((t - tMin) / windowMs) * (W - Y_MARGIN);

    // Collect visible points per channel, compute global [lo, hi] across all enabled channels.
    type VisibleData = { visible: Point[] };
    const visMap = new Map<string, VisibleData>();
    let globalLo = Infinity, globalHi = -Infinity;

    for (const ch of channels) {
      const cutoff = tMin - 200;
      while (ch.points.length > 1 && ch.points[0].t < cutoff) ch.points.shift();
      const visible = ch.points.filter(p => p.t >= tMin);
      visMap.set(ch.path, { visible });
      if (dis.has(ch.path) || visible.length < 2) continue;
      for (const p of visible) {
        if (p.v < globalLo) globalLo = p.v;
        if (p.v > globalHi) globalHi = p.v;
      }
    }

    // Shared absolute Y axis with 10% padding
    const span = (globalHi - globalLo) || 1;
    const pad = span * 0.1;
    const yLo = globalLo - pad;
    const yHi = globalHi + pad;
    const MARGIN_V = 10;
    const toY = (v: number) => MARGIN_V + (H - 2 * MARGIN_V) - ((v - yLo) / (yHi - yLo)) * (H - 2 * MARGIN_V);

    // Y-axis grid lines + labels (5 ticks)
    const TICK_COUNT = 5;
    for (let i = 0; i <= TICK_COUNT; i++) {
      const v = yLo + (yHi - yLo) * (i / TICK_COUNT);
      const y = Math.round(toY(v)) + 0.5;
      // Grid line
      ctx.strokeStyle = '#1e1e2e';
      ctx.lineWidth = 1;
      ctx.beginPath(); ctx.moveTo(Y_MARGIN, y); ctx.lineTo(W, y); ctx.stroke();
      // Tick label — right-aligned in the margin
      const label = Math.abs(v) >= 100 ? v.toFixed(0) : Math.abs(v) >= 1 ? v.toFixed(1) : v.toFixed(3);
      ctx.fillStyle = '#606070';
      ctx.textAlign = 'right';
      ctx.fillText(label, Y_MARGIN - 4, y + 3);
    }
    ctx.textAlign = 'left';

    // Zero line (distinct from grid)
    if (yLo < 0 && yHi > 0) {
      const zy = toY(0);
      ctx.strokeStyle = '#2a2a3a';
      ctx.lineWidth = 1;
      ctx.beginPath(); ctx.moveTo(Y_MARGIN, zy + 0.5); ctx.lineTo(W, zy + 0.5); ctx.stroke();
    }

    // Y-axis border line
    ctx.strokeStyle = '#2a2a3a';
    ctx.lineWidth = 1;
    ctx.beginPath(); ctx.moveTo(Y_MARGIN + 0.5, 0); ctx.lineTo(Y_MARGIN + 0.5, H); ctx.stroke();

    const newLastValues = new Map<string, number>();
    for (const ch of channels) {
      const { visible } = visMap.get(ch.path)!;
      const last = visible[visible.length - 1];
      if (last) newLastValues.set(ch.path, last.v);
      if (dis.has(ch.path) || visible.length < 2) continue;

      ctx.strokeStyle = ch.color;
      ctx.lineWidth = 1.5;
      ctx.lineJoin = 'round';
      ctx.beginPath();
      ctx.moveTo(toX(visible[0].t), toY(visible[0].v));
      for (let j = 1; j < visible.length; j++) {
        ctx.lineTo(toX(visible[j].t), toY(visible[j].v));
      }
      ctx.stroke();
    }

    lastValuesRef.current = newLastValues;

    rafRef.current = requestAnimationFrame(draw);
  }, [windowMs]);

  useEffect(() => {
    if (isHidden) { if (rafRef.current) cancelAnimationFrame(rafRef.current); return; }
    rafRef.current = requestAnimationFrame(draw);
    return () => { if (rafRef.current) cancelAnimationFrame(rafRef.current); };
  }, [draw, isHidden]);

  useEffect(() => {
    const canvas = canvasRef.current;
    if (!canvas) return;
    const ro = new ResizeObserver(() => {
      canvas.width = canvas.offsetWidth;
      canvas.height = canvas.offsetHeight;
    });
    ro.observe(canvas);
    return () => ro.disconnect();
  }, [topic]);

  const handleSample = useCallback((sample: Sample) => {
    try {
      const payload = getSamplePayload(sample);
      const encodingInfo = getEncodingInfo(sample);
      if (hasExplicitEncoding(encodingInfo)) {
        const id = encodingInfo.id;
        if (id !== EncodingPredefined.APPLICATION_CBOR &&
            id !== EncodingPredefined.APPLICATION_OCTET_STREAM &&
            id !== EncodingPredefined.APPLICATION_JSON &&
            id !== EncodingPredefined.TEXT_PLAIN) return;
      }

      // eslint-disable-next-line @typescript-eslint/no-explicit-any
      let msg: any;
      try { msg = cborDecode(payload); }
      catch { try { msg = JSON.parse(new TextDecoder().decode(payload)); } catch { return; } }

      if (msg && typeof msg === 'object' && 'body' in msg) msg = msg.body;

      const leaves = flattenNumeric(msg);
      if (leaves.length === 0) return;

      const now = Date.now();
      const channels = channelsRef.current;
      let added = false;
      for (const { path, value } of leaves) {
        if (!channels.has(path)) {
          channels.set(path, { path, points: [], color: COLORS[channels.size % COLORS.length] });
          added = true;
        }
        channels.get(path)!.points.push({ t: now, v: value });
      }
      if (added) setChannelNames(Array.from(channels.keys()));
    } catch { /* ignore */ }
  }, []);

  useZenohSubscription(topic, handleSample);

  const applyTopic = useCallback((raw: string) => {
    if (!raw) return;
    // Pass exactly as-is — subscription manager handles wildcard internally
    onTopicChange?.(raw);
    channelsRef.current.clear();
    setChannelNames([]);
    setShowTopicEditor(false);
  }, [onTopicChange]);

  const nonMeta = availableTopics.filter(t =>
    !t.display.endsWith('/health') &&
    !t.display.endsWith('/schema') &&
    !t.display.endsWith('/manifest') &&
    !t.display.endsWith('/compressed')
  );

  return (
    <div style={{
      display: 'flex', flexDirection: 'column', height: '100%',
      background: 'var(--bg-card)', borderRadius: 8, overflow: 'hidden',
      border: '1px solid var(--border-color)',
    }}>
      {/* Header — same structure as CameraView */}
      <div style={{
        display: 'flex', alignItems: 'center', gap: 6,
        padding: '6px 10px', background: 'var(--bg-tertiary)',
        borderBottom: '1px solid var(--border-color)',
        cursor: 'grab', userSelect: 'none', minHeight: 32,
      }} {...dragHandleProps}>
        <svg width="12" height="12" viewBox="0 0 24 24" fill="none" stroke="var(--text-muted)" strokeWidth="2">
          <polyline points="22 12 18 12 15 21 9 3 6 12 2 12" />
        </svg>
        <span style={{ flex: 1, color: 'var(--text-secondary)', fontSize: 12, fontWeight: 600, fontFamily: 'inherit' }}>
          {panelName}
        </span>
        {channelNames.length > 0 && (
          <span style={{ color: 'var(--text-muted)', fontSize: 10, fontFamily: 'monospace' }}>
            {channelNames.length}ch · {windowSecs}s
          </span>
        )}
        <button onClick={() => setShowTopicEditor(v => !v)} title="Change topic"
          style={{ background: 'none', border: 'none', cursor: 'pointer', color: 'var(--accent-secondary)', fontSize: 10, fontFamily: 'monospace', padding: '2px 4px' }}>
          {topic ? topic.split('/').slice(-2).join('/') : 'select topic'}
        </button>
        {onRemove && (
          <button onClick={onRemove} title="Remove"
            style={{ background: 'none', border: 'none', cursor: 'pointer', color: 'var(--text-muted)', padding: '2px 4px', fontSize: 14, lineHeight: 1 }}>✕</button>
        )}
      </div>

      {/* Topic picker */}
      {showTopicEditor && (
        <div style={{
          padding: '8px 10px', background: 'var(--bg-secondary)',
          borderBottom: '1px solid var(--border-color)', display: 'flex', flexDirection: 'column', gap: 6,
        }}>
          <select
            defaultValue=""
            onChange={e => e.target.value && applyTopic(e.target.value)}
            style={{
              background: 'var(--bg-primary)', color: 'var(--text-primary)',
              border: '1px solid var(--border-color)', borderRadius: 4,
              padding: '4px 8px', fontSize: 11, fontFamily: 'monospace', width: '100%',
            }}>
            <option value="" disabled>— select a topic —</option>
            {nonMeta.map(t => <option key={t.raw} value={t.raw}>{t.display}</option>)}
          </select>
          {nonMeta.length === 0 && (
            <span style={{ color: 'var(--text-muted)', fontSize: 11 }}>
              No topics discovered yet. Topics appear after nodes start publishing.
            </span>
          )}
        </div>
      )}

      {/* Empty state */}
      {!topic && !showTopicEditor && (
        <div style={{ flex: 1, display: 'flex', alignItems: 'center', justifyContent: 'center', color: 'var(--text-muted)', fontSize: 12 }}>
          Select a topic to begin plotting
        </div>
      )}

      {/* Canvas */}
      {topic && (
        <canvas ref={canvasRef} style={{ flex: 1, width: '100%', display: 'block', minHeight: 0 }} />
      )}

      {/* Legend — HTML row so buttons are clickable */}
      {channelNames.length > 0 && (
        <div style={{
          display: 'flex', flexWrap: 'wrap', gap: '4px 10px',
          padding: '4px 8px', borderTop: '1px solid var(--border-color)',
          background: 'var(--bg-secondary)', minHeight: 26,
        }}>
          {Array.from(channelsRef.current.values()).map(ch => {
            const off = disabled.has(ch.path);
            return (
              <button key={ch.path} onClick={() => toggleChannel(ch.path)}
                title={off ? 'Enable' : 'Disable'}
                style={{
                  display: 'flex', alignItems: 'center', gap: 5,
                  background: 'none', border: 'none', cursor: 'pointer',
                  padding: '1px 6px', borderRadius: 3,
                  opacity: off ? 0.35 : 1,
                }}>
                <span style={{ width: 8, height: 8, borderRadius: 2, background: ch.color, display: 'inline-block', flexShrink: 0 }} />
                <span style={{ fontFamily: 'monospace', fontSize: 10, color: 'var(--text-secondary)', whiteSpace: 'nowrap' }}>
                  {ch.path}
                </span>
              </button>
            );
          })}
        </div>
      )}
    </div>
  );
}
