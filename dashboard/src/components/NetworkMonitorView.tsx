import { useCallback, useState, useRef } from 'react';
import { Sample } from '@eclipse-zenoh/zenoh-ts';
import { getSamplePayload } from '../lib/zenoh';
import { useZenohSubscription } from '../hooks/useZenohSubscription';
import { useFleetContext } from '../contexts/FleetContext';
import { MachineBadge } from './MachineBadge';
import {
  decodeNetworkStatus,
  NetworkStatus,
  HealthCheck,
} from '../proto/network_monitor';

interface DragHandleProps {
  [key: string]: unknown;
}

interface NetworkMonitorViewPanelProps {
  onRemove?: () => void;
  dragHandleProps?: DragHandleProps;
}

function StatusDot({ status }: { status: string }) {
  const color =
    status === 'OK' ? 'var(--success)' :
    status === 'TIMEOUT' ? 'var(--warning, #ffa726)' :
    'var(--error)';
  return (
    <span className="status-dot" style={{ background: color }} title={status} />
  );
}

function CheckRow({ check }: { check: HealthCheck }) {
  return (
    <div className={`check-row ${check.statusName === 'OK' ? 'ok' : 'failed'}`}>
      <div className="check-main">
        <StatusDot status={check.statusName} />
        <span className="check-name">{check.name}</span>
        <span className="check-type-tag">{check.typeName}</span>
      </div>
      <div className="check-details">
        <span className="check-target" title={check.target}>{check.target}</span>
        {check.latencyMs > 0 && (
          <span className="check-latency">{check.latencyMs.toFixed(0)}ms</span>
        )}
        {check.statusCode > 0 && (
          <span className="check-status-code">{check.statusCode}</span>
        )}
        {check.error && (
          <span className="check-error" title={check.error}>{check.error}</span>
        )}
      </div>
    </div>
  );
}

export function NetworkMonitorViewPanel({
  onRemove,
  dragHandleProps,
}: NetworkMonitorViewPanelProps) {
  const { machines } = useFleetContext();
  const [status, setStatus] = useState<NetworkStatus | null>(null);
  const lastUpdateRef = useRef<number>(0);

  const networkTopic = '0/network-monitor%status/**';

  const handleSample = useCallback((sample: Sample) => {
    try {
      const payload = getSamplePayload(sample);
      const data = decodeNetworkStatus(payload);
      if (data) {
        setStatus(data);
        lastUpdateRef.current = Date.now();
      }
    } catch (e) {
      console.error('[NetworkMonitor] Failed to decode:', e);
    }
  }, []);

  const { messageCount } = useZenohSubscription(networkTopic, handleSample);

  const healthyCount = status?.summary?.healthy ?? 0;
  const unhealthyCount = status?.summary?.unhealthy ?? 0;
  const totalCount = status?.summary?.total ?? 0;
  const allHealthy = totalCount > 0 && unhealthyCount === 0;

  return (
    <div className="network-panel">
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
          <span className="panel-type-badge network">NETWORK</span>
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

      <div className="network-content-container">
        {!status ? (
          <div className="network-waiting">
            <div className="spinner" />
            <span>Waiting for network status...</span>
          </div>
        ) : (
          <div className="network-content">
            {/* Summary */}
            <div className={`summary-bar ${allHealthy ? 'all-ok' : 'has-issues'}`}>
              <div className="summary-icon">
                {allHealthy ? (
                  <svg width="18" height="18" viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="2.5">
                    <path d="M20 6L9 17l-5-5" />
                  </svg>
                ) : (
                  <svg width="18" height="18" viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="2.5">
                    <path d="M10.29 3.86L1.82 18a2 2 0 001.71 3h16.94a2 2 0 001.71-3L13.71 3.86a2 2 0 00-3.42 0z" />
                    <line x1="12" y1="9" x2="12" y2="13" />
                    <line x1="12" y1="17" x2="12.01" y2="17" />
                  </svg>
                )}
              </div>
              <div className="summary-text">
                <span className="summary-status">
                  {allHealthy ? 'All Systems Operational' : `${unhealthyCount} Check${unhealthyCount !== 1 ? 's' : ''} Failing`}
                </span>
                <span className="summary-counts">
                  {healthyCount}/{totalCount} healthy
                </span>
              </div>
            </div>

            {/* Checks List */}
            <div className="checks-list">
              {status.checks.map((check, i) => (
                <CheckRow key={`${check.name}-${i}`} check={check} />
              ))}
            </div>
          </div>
        )}
      </div>

      <div className="panel-footer">
        <span className="footer-info">network-monitor/status</span>
      </div>

      <style>{`
        .network-panel {
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

        .network-panel:hover {
          border-color: var(--border-glow);
          box-shadow: var(--shadow-glow);
        }

        .network-panel .panel-header {
          display: flex;
          justify-content: space-between;
          align-items: center;
          padding: 10px 12px;
          background: var(--bg-tertiary);
          border-bottom: 1px solid var(--border-color);
          gap: 8px;
        }

        .network-panel .panel-header-left {
          display: flex;
          align-items: center;
          gap: 8px;
          min-width: 0;
          flex: 1;
        }

        .network-panel .drag-handle {
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

        .network-panel .drag-handle:hover {
          background: var(--bg-primary);
          color: var(--text-secondary);
        }

        .network-panel .drag-handle:active {
          cursor: grabbing;
        }

        .panel-type-badge.network {
          padding: 2px 8px;
          border-radius: 4px;
          font-size: 10px;
          font-weight: 600;
          letter-spacing: 0.5px;
          background: rgba(52, 211, 153, 0.15);
          color: #34d399;
          text-transform: uppercase;
          white-space: nowrap;
          flex-shrink: 0;
        }

        .network-panel .panel-stats {
          display: flex;
          gap: 8px;
          align-items: center;
          flex-shrink: 0;
        }

        .network-panel .stat {
          display: flex;
          align-items: baseline;
          gap: 4px;
        }

        .network-panel .stat-value {
          font-weight: 600;
          font-size: 13px;
          color: var(--accent-secondary);
        }

        .network-panel .stat-label {
          font-size: 10px;
          color: var(--text-muted);
          text-transform: uppercase;
          letter-spacing: 0.5px;
        }

        .network-panel .icon-btn {
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

        .network-panel .icon-btn:hover {
          background: var(--bg-primary);
          border-color: var(--text-muted);
          color: var(--text-primary);
        }

        .network-panel .icon-btn.danger:hover {
          background: rgba(255, 23, 68, 0.1);
          border-color: var(--error);
          color: var(--error);
        }

        .network-content-container {
          position: relative;
          min-height: 240px;
          overflow-y: auto;
          overflow-x: hidden;
          background: var(--bg-primary);
        }

        .network-waiting {
          position: absolute;
          inset: 0;
          display: flex;
          flex-direction: column;
          align-items: center;
          justify-content: center;
          color: var(--text-muted);
          gap: 12px;
        }

        .network-waiting .spinner {
          width: 24px;
          height: 24px;
          border: 2px solid var(--border-color);
          border-top-color: var(--accent-primary);
          border-radius: 50%;
          animation: network-spin 1s linear infinite;
        }

        @keyframes network-spin {
          to { transform: rotate(360deg); }
        }

        .network-content {
          padding: 16px;
          display: flex;
          flex-direction: column;
          gap: 12px;
        }

        .summary-bar {
          display: flex;
          align-items: center;
          gap: 12px;
          padding: 12px 16px;
          border-radius: 8px;
        }

        .summary-bar.all-ok {
          background: rgba(52, 211, 153, 0.1);
          border: 1px solid rgba(52, 211, 153, 0.3);
        }

        .summary-bar.all-ok .summary-icon {
          color: var(--success);
        }

        .summary-bar.has-issues {
          background: rgba(255, 23, 68, 0.1);
          border: 1px solid rgba(255, 23, 68, 0.3);
        }

        .summary-bar.has-issues .summary-icon {
          color: var(--error);
        }

        .summary-icon {
          flex-shrink: 0;
        }

        .summary-text {
          display: flex;
          flex-direction: column;
          gap: 2px;
        }

        .summary-status {
          font-size: 13px;
          font-weight: 600;
          color: var(--text-primary);
        }

        .summary-counts {
          font-size: 11px;
          color: var(--text-muted);
        }

        .checks-list {
          display: flex;
          flex-direction: column;
          gap: 6px;
        }

        .check-row {
          padding: 10px 12px;
          background: var(--bg-tertiary);
          border-radius: 8px;
          border-left: 3px solid transparent;
          display: flex;
          flex-direction: column;
          gap: 4px;
        }

        .check-row.ok {
          border-left-color: var(--success);
        }

        .check-row.failed {
          border-left-color: var(--error);
        }

        .check-main {
          display: flex;
          align-items: center;
          gap: 8px;
        }

        .status-dot {
          width: 8px;
          height: 8px;
          border-radius: 50%;
          flex-shrink: 0;
        }

        .check-name {
          font-size: 12px;
          font-weight: 600;
          color: var(--text-primary);
          flex: 1;
          min-width: 0;
          overflow: hidden;
          text-overflow: ellipsis;
          white-space: nowrap;
        }

        .check-type-tag {
          font-size: 9px;
          font-weight: 600;
          padding: 1px 6px;
          border-radius: 3px;
          background: var(--bg-primary);
          color: var(--text-muted);
          text-transform: uppercase;
          letter-spacing: 0.5px;
          flex-shrink: 0;
        }

        .check-details {
          display: flex;
          align-items: center;
          gap: 10px;
          padding-left: 16px;
          flex-wrap: wrap;
        }

        .check-target {
          font-size: 11px;
          color: var(--text-muted);
          font-family: 'JetBrains Mono', monospace;
          overflow: hidden;
          text-overflow: ellipsis;
          white-space: nowrap;
          max-width: 200px;
        }

        .check-latency {
          font-size: 11px;
          font-weight: 600;
          color: var(--accent-secondary);
          font-family: 'JetBrains Mono', monospace;
        }

        .check-status-code {
          font-size: 10px;
          font-weight: 600;
          padding: 1px 5px;
          border-radius: 3px;
          background: rgba(99, 102, 241, 0.15);
          color: #818cf8;
          font-family: 'JetBrains Mono', monospace;
        }

        .check-error {
          font-size: 11px;
          color: var(--error);
          overflow: hidden;
          text-overflow: ellipsis;
          white-space: nowrap;
          max-width: 200px;
        }

        .network-panel .panel-footer {
          padding: 8px 12px;
          background: var(--bg-tertiary);
          border-top: 1px solid var(--border-color);
        }

        .network-panel .footer-info {
          font-size: 11px;
          color: var(--text-muted);
          font-family: 'JetBrains Mono', monospace;
        }

        .network-panel .mono {
          font-family: 'JetBrains Mono', 'Fira Code', monospace;
        }

        @media (max-width: 768px) {
          .network-content-container {
            min-height: 180px;
          }

          .network-content {
            padding: 12px;
            gap: 10px;
          }

          .summary-bar {
            padding: 10px 12px;
          }

          .check-details {
            padding-left: 16px;
          }

          .check-target {
            max-width: 150px;
          }

          .check-error {
            max-width: 150px;
          }
        }
      `}</style>
    </div>
  );
}
