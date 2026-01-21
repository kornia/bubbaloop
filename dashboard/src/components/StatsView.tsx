import { useState, useEffect, useRef } from 'react';
import { useZenohSubscriptionContext } from '../contexts/ZenohSubscriptionContext';

interface DragHandleProps {
  [key: string]: unknown;
}

interface StatsViewProps {
  onRemove?: () => void;
  dragHandleProps?: DragHandleProps;
}

interface DisplayStats {
  topic: string;
  messageCount: number;
  hz: number;
  hasActiveListeners: boolean;
  listenerCount: number;
}

export function StatsViewPanel({
  onRemove,
  dragHandleProps,
}: StatsViewProps) {
  const { getAllMonitoredStats, startMonitoring, isMonitoringEnabled } = useZenohSubscriptionContext();
  const [stats, setStats] = useState<DisplayStats[]>([]);
  const [monitoringStarted, setMonitoringStarted] = useState(false);
  const startedRef = useRef(false);

  // Start monitoring when component mounts
  useEffect(() => {
    // Prevent double-start in strict mode
    if (startedRef.current) return;
    startedRef.current = true;

    if (!isMonitoringEnabled()) {
      startMonitoring().then(() => {
        setMonitoringStarted(true);
      });
    } else {
      setMonitoringStarted(true);
    }

    // Don't stop monitoring on unmount - other components may need it
    // and we want to persist topic discovery across panel toggles
  }, [startMonitoring, isMonitoringEnabled]);

  // Poll stats from centralized manager
  useEffect(() => {
    const interval = setInterval(() => {
      const allStats = getAllMonitoredStats();
      const displayStats: DisplayStats[] = [];

      allStats.forEach((stat, topic) => {
        displayStats.push({
          topic,
          messageCount: stat.messageCount,
          hz: stat.fps,
          hasActiveListeners: stat.hasActiveListeners,
          listenerCount: stat.listenerCount,
        });
      });

      // Sort by topic name alphabetically
      displayStats.sort((a, b) => a.topic.localeCompare(b.topic));
      setStats(displayStats);
    }, 1000);

    return () => clearInterval(interval);
  }, [getAllMonitoredStats]);

  const shortenTopic = (topic: string) => {
    // Handle two formats:
    // 1. ros-z encoded: "0/camera%terrace%raw_shm" -> "camera/terrace/raw_shm"
    // 2. Raw zenoh key: "camera/terrace/raw_shm" (already normalized, type/hash stripped)
    const parts = topic.split('/');

    // Check if it's ros-z format (starts with domain ID like "0/")
    if (parts.length >= 2 && /^\d+$/.test(parts[0])) {
      // ros-z format: domain/encoded_topic
      // Skip domain ID (parts[0]), decode topic name (parts[1])
      return parts[1].replace(/%/g, '/');
    }

    // Raw format: already normalized, just return as-is
    // The normalizeTopicPattern in subscription-manager strips type/hash
    return topic;
  };

  return (
    <div className="stats-panel">
      <div className="panel-header" {...dragHandleProps}>
        <div className="panel-title">
          <svg width="14" height="14" viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="2">
            <path d="M18 20V10M12 20V4M6 20v-6" />
          </svg>
          <span className="panel-type-badge">STATS</span>
        </div>
        <div className="panel-actions">
          <button
            className="panel-action-btn danger"
            onClick={onRemove}
            title="Remove"
          >
            <svg width="14" height="14" viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="2">
              <path d="M18 6L6 18M6 6l12 12" />
            </svg>
          </button>
        </div>
      </div>

      <div className="stats-content">
        <div className="stats-table">
          <div className="stats-table-header">
            <span className="col-topic">Topic</span>
            <span className="col-hz">Hz</span>
            <span className="col-msgs">Msgs</span>
          </div>
          <div className="stats-table-body">
            {stats.length === 0 ? (
              <div className="no-data">
                {monitoringStarted ? 'Waiting for messages...' : 'Starting topic monitor...'}
              </div>
            ) : (
              stats.map((stat) => (
                <div key={stat.topic} className={`stats-row ${stat.hasActiveListeners ? 'has-listeners' : ''}`}>
                  <span className="col-topic" title={stat.topic}>
                    {stat.hasActiveListeners && <span className="listener-indicator">●</span>}
                    {shortenTopic(stat.topic)}
                  </span>
                  <span className={`col-hz ${stat.hasActiveListeners && stat.hz > 0 ? 'active' : 'inactive'}`}>
                    {stat.hz.toFixed(2)}
                  </span>
                  <span className="col-msgs">{stat.messageCount.toLocaleString()}</span>
                </div>
              ))
            )}
          </div>
        </div>
      </div>

      <div className="panel-footer">
        <span className="footer-info">
          <span className="listener-indicator">●</span> = active listener | {stats.length} topics
        </span>
      </div>

      <style>{`
        .stats-panel {
          background: var(--bg-card);
          border: 1px solid var(--border-color);
          border-radius: 12px;
          overflow: hidden;
          display: flex;
          flex-direction: column;
        }

        .stats-panel.maximized {
          min-height: 80vh;
        }

        .panel-header {
          display: flex;
          justify-content: space-between;
          align-items: center;
          padding: 12px 16px;
          background: var(--bg-tertiary);
          border-bottom: 1px solid var(--border-color);
          cursor: grab;
        }

        .panel-header:active {
          cursor: grabbing;
        }

        .panel-title {
          display: flex;
          align-items: center;
          gap: 8px;
          font-size: 14px;
          font-weight: 500;
          color: var(--text-primary);
        }

        .panel-type-badge {
          padding: 2px 8px;
          border-radius: 4px;
          font-size: 10px;
          font-weight: 600;
          letter-spacing: 0.5px;
          background: rgba(0, 229, 255, 0.15);
          color: var(--accent-secondary);
          text-transform: uppercase;
          white-space: nowrap;
        }

        .panel-actions {
          display: flex;
          gap: 4px;
        }

        .panel-action-btn {
          display: flex;
          align-items: center;
          justify-content: center;
          width: 28px;
          height: 28px;
          background: transparent;
          border: none;
          border-radius: 6px;
          color: var(--text-muted);
          cursor: pointer;
          transition: all 0.15s;
        }

        .panel-action-btn:hover {
          background: var(--bg-secondary);
          color: var(--text-primary);
        }

        .panel-action-btn.danger:hover {
          background: rgba(255, 23, 68, 0.1);
          color: var(--error);
        }

        .stats-content {
          position: relative;
          aspect-ratio: 16 / 9;
          min-height: 240px;
          overflow-y: auto;
          overflow-x: hidden;
          background: var(--bg-primary);
        }

        .stats-panel.maximized .stats-content {
          aspect-ratio: unset;
          flex: 1;
          min-height: 400px;
        }

        .stats-table {
          display: flex;
          flex-direction: column;
        }

        .stats-table-header {
          display: grid;
          grid-template-columns: 1fr 60px 80px;
          gap: 8px;
          padding: 8px 16px;
          background: var(--bg-tertiary);
          border-bottom: 1px solid var(--border-color);
          font-size: 11px;
          font-weight: 600;
          color: var(--text-muted);
          text-transform: uppercase;
        }

        .stats-table-body {
          flex: 1;
          overflow-y: auto;
          padding: 8px 0;
        }

        .stats-row {
          display: grid;
          grid-template-columns: 1fr 60px 80px;
          gap: 8px;
          padding: 6px 16px;
          font-size: 12px;
          font-family: 'JetBrains Mono', monospace;
        }

        .stats-row:hover {
          background: var(--bg-tertiary);
        }

        .stats-row.has-listeners {
          background: rgba(0, 229, 255, 0.05);
        }

        .stats-row.has-listeners:hover {
          background: rgba(0, 229, 255, 0.1);
        }

        .listener-indicator {
          color: var(--accent-primary);
          margin-right: 6px;
          font-size: 10px;
        }

        .col-topic {
          overflow: hidden;
          text-overflow: ellipsis;
          white-space: nowrap;
          color: var(--text-secondary);
        }

        .col-hz {
          text-align: right;
          font-weight: 600;
        }

        .col-hz.active {
          color: var(--success);
        }

        .col-hz.inactive {
          color: var(--text-muted);
        }

        .col-msgs {
          text-align: right;
          color: var(--text-secondary);
        }

        .no-data {
          padding: 24px;
          text-align: center;
          color: var(--text-muted);
          font-size: 13px;
        }

        .panel-footer {
          padding: 8px 12px;
          background: var(--bg-tertiary);
          border-top: 1px solid var(--border-color);
        }

        .footer-info {
          font-size: 11px;
          color: var(--text-muted);
          display: flex;
          align-items: center;
          gap: 2px;
        }

        .footer-info .listener-indicator {
          margin-right: 2px;
        }

        @media (max-width: 768px) {
          .panel-type-badge {
            padding: 2px 6px;
            font-size: 9px;
          }

          .maximize-btn {
            display: none;
          }

          .stats-content {
            min-height: 180px;
          }

          .stats-table-header,
          .stats-row {
            grid-template-columns: 1fr 50px 70px;
            font-size: 11px;
          }
        }
      `}</style>
    </div>
  );
}
