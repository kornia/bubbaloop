import { useCallback, useState, useRef, useEffect } from 'react';
import { Sample } from '@eclipse-zenoh/zenoh-ts';
import { getSamplePayload, extractMachineId } from '../lib/zenoh';
import { useZenohSubscription } from '../hooks/useZenohSubscription';
import { useSchemaReady } from '../hooks/useSchemaReady';
import { useFleetContext } from '../contexts/FleetContext';
import { useSchemaRegistry } from '../contexts/SchemaRegistryContext';
import { MachineBadge } from './MachineBadge';

// Local interfaces for decoded system metrics (SchemaRegistry returns longs as strings)
interface CpuMetrics {
  usagePercent: number;
  count: number;
  perCore: number[];
}

interface MemoryMetrics {
  totalBytes: string;
  usedBytes: string;
  availableBytes: string;
  usagePercent: number;
}

interface DiskMetrics {
  totalBytes: string;
  usedBytes: string;
  availableBytes: string;
  usagePercent: number;
}

interface NetworkMetrics {
  bytesSent: string;
  bytesRecv: string;
}

interface LoadMetrics {
  oneMin: number;
  fiveMin: number;
  fifteenMin: number;
}

interface SystemMetrics {
  cpu?: CpuMetrics;
  memory?: MemoryMetrics;
  disk?: DiskMetrics;
  network?: NetworkMetrics;
  load?: LoadMetrics;
  uptimeSecs: string;
}

interface DragHandleProps {
  [key: string]: unknown;
}

interface SystemTelemetryViewPanelProps {
  onRemove?: () => void;
  dragHandleProps?: DragHandleProps;
}

interface MachineMetrics {
  metrics: SystemMetrics;
  lastUpdate: number;
}

function formatBytes(bytes: string | number): string {
  const num = typeof bytes === 'string' ? Number(bytes) : bytes;
  if (num >= 1e12) return `${(num / 1e12).toFixed(1)} TB`;
  if (num >= 1e9) return `${(num / 1e9).toFixed(1)} GB`;
  if (num >= 1e6) return `${(num / 1e6).toFixed(1)} MB`;
  if (num >= 1e3) return `${(num / 1e3).toFixed(1)} KB`;
  return `${num} B`;
}

function formatUptime(secs: string | number): string {
  const s = typeof secs === 'string' ? Number(secs) : secs;
  const days = Math.floor(s / 86400);
  const hours = Math.floor((s % 86400) / 3600);
  const mins = Math.floor((s % 3600) / 60);
  if (days > 0) return `${days}d ${hours}h ${mins}m`;
  if (hours > 0) return `${hours}h ${mins}m`;
  return `${mins}m`;
}

function UsageBar({ percent, color }: { percent: number; color: string }) {
  return (
    <div className="usage-bar">
      <div
        className="usage-bar-fill"
        style={{ width: `${Math.min(percent, 100)}%`, background: color }}
      />
    </div>
  );
}

export function SystemTelemetryViewPanel({
  onRemove,
  dragHandleProps,
}: SystemTelemetryViewPanelProps) {
  const { machines, selectedMachineId } = useFleetContext();
  const { registry, discoverForTopic } = useSchemaRegistry();
  const schemaReady = useSchemaReady();
  const [metricsMap, setMetricsMap] = useState<Map<string, MachineMetrics>>(new Map());
  const metricsMapRef = useRef(metricsMap);
  metricsMapRef.current = metricsMap;

  // Throttle: store latest data in ref, flush to state at 250ms intervals
  const pendingRef = useRef<Map<string, MachineMetrics> | null>(null);

  useEffect(() => {
    const timer = setInterval(() => {
      if (pendingRef.current) {
        setMetricsMap(pendingRef.current);
        pendingRef.current = null;
      }
    }, 250);
    return () => clearInterval(timer);
  }, []);

  // Match any machine/scope via ** wildcard (vanilla Zenoh format)
  const telemetryTopic = '**/system-telemetry/metrics';

  const handleSample = useCallback((sample: Sample) => {
    try {
      const payload = getSamplePayload(sample);
      const topic = sample.keyexpr().toString();
      const machineId = extractMachineId(topic) ?? 'unknown';

      const result = registry.decode('bubbaloop.system_telemetry.v1.SystemMetrics', payload);
      if (result) {
        const data = result.data as unknown as SystemMetrics;
        const base = pendingRef.current ?? new Map(metricsMapRef.current);
        base.set(machineId, { metrics: data, lastUpdate: Date.now() });
        pendingRef.current = base;
      } else {
        discoverForTopic(topic);
      }
    } catch (e) {
      console.error('[SystemTelemetry] Failed to decode:', e);
    }
  }, [registry, discoverForTopic]);

  // Gate callback on schema readiness — samples are ignored until schemas load
  const { messageCount } = useZenohSubscription(telemetryTopic, schemaReady ? handleSample : undefined);

  // Filter metrics by selectedMachineId (null = show all)
  const filteredEntries = Array.from(metricsMap.entries()).filter(([machineId]) =>
    selectedMachineId === null || machineId === selectedMachineId
  );

  const now = Date.now();
  const STALE_THRESHOLD = 15000; // 15 seconds

  const renderMetricsCard = (machineId: string, machineMetrics: MachineMetrics) => {
    const { metrics, lastUpdate } = machineMetrics;
    const isStale = now - lastUpdate > STALE_THRESHOLD;

    const cpuColor = (metrics?.cpu?.usagePercent ?? 0) > 80 ? 'var(--error)' : 'var(--accent-primary)';
    const memColor = (metrics?.memory?.usagePercent ?? 0) > 80 ? 'var(--error)' : 'var(--success)';
    const diskColor = (metrics?.disk?.usagePercent ?? 0) > 80 ? 'var(--error)' : 'var(--accent-secondary)';

    return (
      <div key={machineId} className="machine-metrics-card" style={{ opacity: isStale ? 0.5 : 1 }}>
        <div className="machine-header">
          <span className="machine-name-badge">{machineId}</span>
          {isStale && <span className="stale-indicator">(stale)</span>}
        </div>
        <div className="telemetry-content">
          {/* Uptime */}
          <div className="metric-row uptime-row">
            <span className="uptime-label">Uptime</span>
            <span className="uptime-value">{formatUptime(metrics.uptimeSecs)}</span>
          </div>

          {/* CPU */}
          {metrics.cpu && (
            <div className="metric-section">
              <div className="metric-header">
                <span className="metric-label">CPU</span>
                <span className="metric-value" style={{ color: cpuColor }}>
                  {metrics.cpu.usagePercent.toFixed(1)}%
                </span>
              </div>
              <UsageBar percent={metrics.cpu.usagePercent} color={cpuColor} />
              {metrics.cpu.perCore.length > 0 && (
                <div className="core-grid">
                  {metrics.cpu.perCore.map((usage, i) => (
                    <div key={i} className="core-item" title={`Core ${i}: ${usage.toFixed(1)}%`}>
                      <div className="core-bar">
                        <div
                          className="core-bar-fill"
                          style={{
                            height: `${Math.min(usage, 100)}%`,
                            background: usage > 80 ? 'var(--error)' : 'var(--accent-primary)',
                          }}
                        />
                      </div>
                    </div>
                  ))}
                </div>
              )}
            </div>
          )}

          {/* Memory */}
          {metrics.memory && (
            <div className="metric-section">
              <div className="metric-header">
                <span className="metric-label">Memory</span>
                <span className="metric-value" style={{ color: memColor }}>
                  {metrics.memory.usagePercent.toFixed(1)}%
                </span>
              </div>
              <UsageBar percent={metrics.memory.usagePercent} color={memColor} />
              <div className="metric-detail">
                {formatBytes(metrics.memory.usedBytes)} / {formatBytes(metrics.memory.totalBytes)}
              </div>
            </div>
          )}

          {/* Disk */}
          {metrics.disk && (
            <div className="metric-section">
              <div className="metric-header">
                <span className="metric-label">Disk</span>
                <span className="metric-value" style={{ color: diskColor }}>
                  {metrics.disk.usagePercent.toFixed(1)}%
                </span>
              </div>
              <UsageBar percent={metrics.disk.usagePercent} color={diskColor} />
              <div className="metric-detail">
                {formatBytes(metrics.disk.usedBytes)} / {formatBytes(metrics.disk.totalBytes)}
              </div>
            </div>
          )}

          {/* Load */}
          {metrics.load && (
            <div className="metric-section">
              <div className="metric-header">
                <span className="metric-label">Load Average</span>
              </div>
              <div className="load-values">
                <div className="load-item">
                  <span className="load-period">1m</span>
                  <span className="load-num">{metrics.load.oneMin.toFixed(2)}</span>
                </div>
                <div className="load-item">
                  <span className="load-period">5m</span>
                  <span className="load-num">{metrics.load.fiveMin.toFixed(2)}</span>
                </div>
                <div className="load-item">
                  <span className="load-period">15m</span>
                  <span className="load-num">{metrics.load.fifteenMin.toFixed(2)}</span>
                </div>
              </div>
            </div>
          )}

          {/* Network I/O */}
          {metrics.network && (
            <div className="metric-section">
              <div className="metric-header">
                <span className="metric-label">Network I/O</span>
              </div>
              <div className="net-values">
                <div className="net-item">
                  <span className="net-direction">TX</span>
                  <span className="net-bytes">{formatBytes(metrics.network.bytesSent)}</span>
                </div>
                <div className="net-item">
                  <span className="net-direction">RX</span>
                  <span className="net-bytes">{formatBytes(metrics.network.bytesRecv)}</span>
                </div>
              </div>
            </div>
          )}
        </div>
      </div>
    );
  };

  return (
    <div className="telemetry-panel">
      <div className="panel-header">
        <div className="panel-header-left">
          {dragHandleProps && (
            <button className="drag-handle" title="Drag to reorder" {...dragHandleProps}>
              <svg width="16" height="16" viewBox="0 0 16 16" fill="currentColor">
                <circle cx="5" cy="4" r="1.5" />
                <circle cx="11" cy="4" r="1.5" />
                <circle cx="5" cy="8" r="1.5" />
                <circle cx="11" cy="8" r="1.5" />
                <circle cx="5" cy="12" r="1.5" />
                <circle cx="11" cy="12" r="1.5" />
              </svg>
            </button>
          )}
          <span className="panel-type-badge telemetry">TELEMETRY</span>
          <MachineBadge machines={machines} />
        </div>
        <div className="panel-stats">
          <span className="stat">
            <span className="stat-value mono">{messageCount.toLocaleString()}</span>
            <span className="stat-label">msgs</span>
          </span>
          {onRemove && (
            <button className="icon-btn danger" onClick={onRemove} title="Remove panel">
              <svg width="14" height="14" viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="2">
                <path d="M18 6L6 18M6 6l12 12" />
              </svg>
            </button>
          )}
        </div>
      </div>

      <div className="telemetry-content-container">
        {filteredEntries.length === 0 ? (
          <div className="telemetry-waiting">
            <div className="spinner" />
            <span>Waiting for system telemetry...</span>
          </div>
        ) : (
          <div className="machines-grid">
            {filteredEntries.map(([machineId, machineMetrics]) =>
              renderMetricsCard(machineId, machineMetrics)
            )}
          </div>
        )}
      </div>

      <div className="panel-footer">
        <span className="footer-info">system-telemetry/metrics</span>
      </div>

      <style>{`
        .telemetry-panel {
          background: var(--bg-card);
          border: 1px solid var(--border-color);
          border-radius: 12px;
          overflow: hidden;
          display: flex;
          flex-direction: column;
          transition: border-color 0.2s, box-shadow 0.2s;
          min-width: 0;
          max-width: 100%;
        }

        .telemetry-panel:hover {
          border-color: var(--border-glow);
          box-shadow: var(--shadow-glow);
        }

        .telemetry-panel .panel-header {
          display: flex;
          justify-content: space-between;
          align-items: center;
          padding: 10px 12px;
          background: var(--bg-tertiary);
          border-bottom: 1px solid var(--border-color);
          gap: 8px;
        }

        .telemetry-panel .panel-header-left {
          display: flex;
          align-items: center;
          gap: 8px;
          min-width: 0;
          flex: 1;
        }

        .telemetry-panel .drag-handle {
          display: flex;
          align-items: center;
          justify-content: center;
          width: 24px;
          height: 24px;
          background: transparent;
          border: none;
          color: var(--text-muted);
          cursor: grab;
          border-radius: 4px;
          flex-shrink: 0;
        }

        .telemetry-panel .drag-handle:hover {
          background: var(--bg-primary);
          color: var(--text-secondary);
        }

        .telemetry-panel .drag-handle:active {
          cursor: grabbing;
        }

        .panel-type-badge.telemetry {
          padding: 2px 8px;
          border-radius: 4px;
          font-size: 10px;
          font-weight: 600;
          letter-spacing: 0.5px;
          background: rgba(99, 102, 241, 0.15);
          color: #818cf8;
          text-transform: uppercase;
          white-space: nowrap;
          flex-shrink: 0;
        }

        .telemetry-panel .panel-stats {
          display: flex;
          gap: 8px;
          align-items: center;
          flex-shrink: 0;
        }

        .telemetry-panel .stat {
          display: flex;
          align-items: baseline;
          gap: 4px;
        }

        .telemetry-panel .stat-value {
          font-weight: 600;
          font-size: 13px;
          color: var(--accent-secondary);
        }

        .telemetry-panel .stat-label {
          font-size: 10px;
          color: var(--text-muted);
          text-transform: uppercase;
          letter-spacing: 0.5px;
        }

        .telemetry-panel .icon-btn {
          display: flex;
          align-items: center;
          justify-content: center;
          width: 32px;
          height: 32px;
          background: transparent;
          border: 1px solid var(--border-color);
          border-radius: 6px;
          color: var(--text-secondary);
          cursor: pointer;
          transition: all 0.15s;
          flex-shrink: 0;
        }

        .telemetry-panel .icon-btn:hover {
          background: var(--bg-primary);
          border-color: var(--text-muted);
          color: var(--text-primary);
        }

        .telemetry-panel .icon-btn.danger:hover {
          background: rgba(255, 23, 68, 0.1);
          border-color: var(--error);
          color: var(--error);
        }

        .telemetry-content-container {
          position: relative;
          min-height: 240px;
          overflow-y: auto;
          overflow-x: hidden;
          background: var(--bg-primary);
        }

        .telemetry-waiting {
          position: absolute;
          inset: 0;
          display: flex;
          flex-direction: column;
          align-items: center;
          justify-content: center;
          color: var(--text-muted);
          gap: 12px;
        }

        .telemetry-waiting .spinner {
          width: 24px;
          height: 24px;
          border: 2px solid var(--border-color);
          border-top-color: var(--accent-primary);
          border-radius: 50%;
          animation: telemetry-spin 1s linear infinite;
        }

        @keyframes telemetry-spin {
          to { transform: rotate(360deg); }
        }

        .machines-grid {
          padding: 16px;
          display: flex;
          flex-wrap: wrap;
          gap: 12px;
        }

        .machine-metrics-card {
          flex: 1;
          min-width: 320px;
          background: var(--bg-card);
          border: 1px solid var(--border-color);
          border-radius: 8px;
          padding: 12px;
          transition: opacity 0.3s;
        }

        .machine-header {
          display: flex;
          align-items: center;
          gap: 8px;
          margin-bottom: 12px;
          padding-bottom: 8px;
          border-bottom: 1px solid var(--border-color);
        }

        .machine-name-badge {
          padding: 2px 8px;
          border-radius: 4px;
          font-size: 11px;
          font-weight: 600;
          letter-spacing: 0.3px;
          background: rgba(34, 197, 94, 0.15);
          color: #4ade80;
          text-transform: uppercase;
          white-space: nowrap;
        }

        .stale-indicator {
          font-size: 10px;
          color: var(--text-muted);
          font-style: italic;
        }

        .telemetry-content {
          display: flex;
          flex-direction: column;
          gap: 16px;
        }

        .uptime-row {
          display: flex;
          justify-content: space-between;
          align-items: center;
          padding: 8px 12px;
          background: var(--bg-tertiary);
          border-radius: 6px;
        }

        .uptime-label {
          font-size: 11px;
          color: var(--text-muted);
          text-transform: uppercase;
          letter-spacing: 0.5px;
        }

        .uptime-value {
          font-size: 13px;
          font-weight: 600;
          color: var(--text-primary);
          font-family: 'JetBrains Mono', monospace;
        }

        .metric-section {
          display: flex;
          flex-direction: column;
          gap: 6px;
        }

        .metric-header {
          display: flex;
          justify-content: space-between;
          align-items: center;
        }

        .metric-label {
          font-size: 12px;
          font-weight: 600;
          color: var(--text-secondary);
          text-transform: uppercase;
          letter-spacing: 0.5px;
        }

        .metric-value {
          font-size: 14px;
          font-weight: 700;
          font-family: 'JetBrains Mono', monospace;
        }

        .metric-detail {
          font-size: 11px;
          color: var(--text-muted);
          font-family: 'JetBrains Mono', monospace;
        }

        .usage-bar {
          height: 6px;
          background: var(--bg-tertiary);
          border-radius: 3px;
          overflow: hidden;
        }

        .usage-bar-fill {
          height: 100%;
          border-radius: 3px;
          transition: width 0.3s ease;
        }

        .core-grid {
          display: flex;
          gap: 3px;
          margin-top: 4px;
        }

        .core-item {
          flex: 1;
          min-width: 0;
        }

        .core-bar {
          height: 24px;
          background: var(--bg-tertiary);
          border-radius: 2px;
          display: flex;
          flex-direction: column-reverse;
          overflow: hidden;
        }

        .core-bar-fill {
          width: 100%;
          border-radius: 2px;
          transition: height 0.3s ease;
        }

        .load-values {
          display: flex;
          gap: 12px;
        }

        .load-item {
          flex: 1;
          display: flex;
          flex-direction: column;
          align-items: center;
          gap: 2px;
          padding: 8px;
          background: var(--bg-tertiary);
          border-radius: 6px;
        }

        .load-period {
          font-size: 10px;
          color: var(--text-muted);
          text-transform: uppercase;
        }

        .load-num {
          font-size: 14px;
          font-weight: 600;
          color: var(--text-primary);
          font-family: 'JetBrains Mono', monospace;
        }

        .net-values {
          display: flex;
          gap: 12px;
        }

        .net-item {
          flex: 1;
          display: flex;
          align-items: center;
          gap: 8px;
          padding: 8px 12px;
          background: var(--bg-tertiary);
          border-radius: 6px;
        }

        .net-direction {
          font-size: 10px;
          font-weight: 600;
          color: var(--text-muted);
          text-transform: uppercase;
          letter-spacing: 0.5px;
        }

        .net-bytes {
          font-size: 13px;
          font-weight: 600;
          color: var(--text-primary);
          font-family: 'JetBrains Mono', monospace;
        }

        .telemetry-panel .panel-footer {
          padding: 8px 12px;
          background: var(--bg-tertiary);
          border-top: 1px solid var(--border-color);
        }

        .telemetry-panel .footer-info {
          font-size: 11px;
          color: var(--text-muted);
          font-family: 'JetBrains Mono', monospace;
        }

        .telemetry-panel .mono {
          font-family: 'JetBrains Mono', 'Fira Code', monospace;
        }

        @media (max-width: 768px) {
          .telemetry-content-container {
            min-height: 180px;
          }

          .telemetry-content {
            padding: 12px;
            gap: 12px;
          }

          .core-grid {
            gap: 2px;
          }

          .core-bar {
            height: 18px;
          }

          .load-values, .net-values {
            gap: 8px;
          }
        }
      `}</style>
    </div>
  );
}
