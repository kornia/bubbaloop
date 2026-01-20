import { useState, useEffect, useRef } from 'react';
import { Session, Sample } from '@eclipse-zenoh/zenoh-ts';

interface DragHandleProps {
  [key: string]: unknown;
}

interface StatsViewProps {
  session: Session;
  isMaximized?: boolean;
  onMaximize?: () => void;
  onRemove?: () => void;
  dragHandleProps?: DragHandleProps;
}

interface TopicStats {
  topic: string;
  messageCount: number;
  fps: number;
  lastSeen: number;
  bytesPerSec: number;
  lastBytes: number;
}

export function StatsViewPanel({
  session,
  isMaximized = false,
  onMaximize,
  onRemove,
  dragHandleProps,
}: StatsViewProps) {
  const [stats, setStats] = useState<Map<string, TopicStats>>(new Map());
  const statsRef = useRef<Map<string, TopicStats>>(new Map());
  const subscriberRef = useRef<any>(null);

  // Subscribe to all topics
  useEffect(() => {
    if (!session) return;

    let mounted = true;

    const setupSubscriber = async () => {
      try {
        const subscriber = await session.declareSubscriber('**', {
          handler: (sample: Sample) => {
            if (!mounted) return;

            const topic = sample.keyexpr().toString();
            const payload = sample.payload();
            const bytes = payload && typeof payload.toBytes === 'function'
              ? payload.toBytes().length
              : 0;

            const now = Date.now();
            const existing = statsRef.current.get(topic);

            if (existing) {
              existing.messageCount++;
              existing.lastSeen = now;
              existing.lastBytes += bytes;
            } else {
              statsRef.current.set(topic, {
                topic,
                messageCount: 1,
                fps: 0,
                lastSeen: now,
                bytesPerSec: 0,
                lastBytes: bytes,
              });
            }
          },
        });

        subscriberRef.current = subscriber;
        console.log('[StatsView] Subscribed to all topics');
      } catch (e) {
        console.error('[StatsView] Failed to subscribe:', e);
      }
    };

    setupSubscriber();

    // Update stats every second
    const interval = setInterval(() => {
      const newStats = new Map<string, TopicStats>();

      statsRef.current.forEach((stat, topic) => {
        // Keep track of previous count for FPS calculation
        const prevCount = (stat as any)._prevCount || 0;
        const fps = stat.messageCount - prevCount;
        (stat as any)._prevCount = stat.messageCount;

        // Calculate bytes per second
        const bytesPerSec = stat.lastBytes;
        stat.lastBytes = 0;

        newStats.set(topic, {
          ...stat,
          fps,
          bytesPerSec,
        });
      });

      setStats(new Map(newStats));
    }, 1000);

    return () => {
      mounted = false;
      clearInterval(interval);
      if (subscriberRef.current) {
        subscriberRef.current.undeclare().catch(console.error);
        subscriberRef.current = null;
      }
    };
  }, [session]);

  // Sort topics by FPS descending
  const sortedStats = Array.from(stats.values()).sort((a, b) => b.fps - a.fps);
  const totalMessages = sortedStats.reduce((sum, s) => sum + s.messageCount, 0);
  const totalFps = sortedStats.reduce((sum, s) => sum + s.fps, 0);

  const formatBytes = (bytes: number) => {
    if (bytes < 1024) return `${bytes} B/s`;
    if (bytes < 1024 * 1024) return `${(bytes / 1024).toFixed(1)} KB/s`;
    return `${(bytes / (1024 * 1024)).toFixed(1)} MB/s`;
  };

  const shortenTopic = (topic: string) => {
    // Shorten ros-z style topics for display
    const parts = topic.split('/');
    if (parts.length >= 3 && parts[2].startsWith('bubbaloop.')) {
      // Format: 0/topic%name/bubbaloop.x.v1.Type/RIHS...
      const topicName = parts[1].replace(/%/g, '/');
      const typeParts = parts[2].split('.');
      const typeName = typeParts[typeParts.length - 1];
      return `${topicName} (${typeName})`;
    }
    return topic;
  };

  return (
    <div className={`stats-panel ${isMaximized ? 'maximized' : ''}`}>
      <div className="panel-header" {...dragHandleProps}>
        <div className="panel-title">
          <svg width="14" height="14" viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="2">
            <path d="M18 20V10M12 20V4M6 20v-6" />
          </svg>
          <span className="panel-type-badge">STATS</span>
        </div>
        <div className="panel-actions">
          <button
            className="panel-action-btn maximize-btn"
            onClick={onMaximize}
            title={isMaximized ? 'Restore' : 'Maximize'}
          >
            {isMaximized ? (
              <svg width="14" height="14" viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="2">
                <path d="M8 3v3a2 2 0 01-2 2H3m18 0h-3a2 2 0 01-2-2V3m0 18v-3a2 2 0 012-2h3M3 16h3a2 2 0 012 2v3" />
              </svg>
            ) : (
              <svg width="14" height="14" viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="2">
                <path d="M8 3H5a2 2 0 00-2 2v3m18 0V5a2 2 0 00-2-2h-3m0 18h3a2 2 0 002-2v-3M3 16v3a2 2 0 002 2h3" />
              </svg>
            )}
          </button>
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
        <div className="stats-summary">
          <div className="stat-item">
            <span className="stat-label">Topics</span>
            <span className="stat-value">{sortedStats.length}</span>
          </div>
          <div className="stat-item">
            <span className="stat-label">Total Messages</span>
            <span className="stat-value">{totalMessages.toLocaleString()}</span>
          </div>
          <div className="stat-item">
            <span className="stat-label">Total FPS</span>
            <span className="stat-value">{totalFps}</span>
          </div>
        </div>

        <div className="stats-table">
          <div className="stats-table-header">
            <span className="col-topic">Topic</span>
            <span className="col-fps">FPS</span>
            <span className="col-msgs">Msgs</span>
            <span className="col-bw">BW</span>
          </div>
          <div className="stats-table-body">
            {sortedStats.length === 0 ? (
              <div className="no-data">Waiting for data...</div>
            ) : (
              sortedStats.map((stat) => (
                <div key={stat.topic} className="stats-row">
                  <span className="col-topic" title={stat.topic}>
                    {shortenTopic(stat.topic)}
                  </span>
                  <span className={`col-fps ${stat.fps > 0 ? 'active' : 'inactive'}`}>
                    {stat.fps}
                  </span>
                  <span className="col-msgs">{stat.messageCount.toLocaleString()}</span>
                  <span className="col-bw">{formatBytes(stat.bytesPerSec)}</span>
                </div>
              ))
            )}
          </div>
        </div>
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
          flex: 1;
          display: flex;
          flex-direction: column;
          overflow: hidden;
        }

        .stats-summary {
          display: flex;
          gap: 16px;
          padding: 12px 16px;
          border-bottom: 1px solid var(--border-color);
          background: var(--bg-secondary);
        }

        .stat-item {
          display: flex;
          flex-direction: column;
          gap: 2px;
        }

        .stat-label {
          font-size: 11px;
          color: var(--text-muted);
          text-transform: uppercase;
        }

        .stat-value {
          font-size: 18px;
          font-weight: 600;
          color: var(--accent-secondary);
          font-family: 'JetBrains Mono', monospace;
        }

        .stats-table {
          flex: 1;
          display: flex;
          flex-direction: column;
          overflow: hidden;
        }

        .stats-table-header {
          display: grid;
          grid-template-columns: 1fr 50px 70px 80px;
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
          grid-template-columns: 1fr 50px 70px 80px;
          gap: 8px;
          padding: 6px 16px;
          font-size: 12px;
          font-family: 'JetBrains Mono', monospace;
        }

        .stats-row:hover {
          background: var(--bg-tertiary);
        }

        .col-topic {
          overflow: hidden;
          text-overflow: ellipsis;
          white-space: nowrap;
          color: var(--text-secondary);
        }

        .col-fps {
          text-align: right;
          font-weight: 600;
        }

        .col-fps.active {
          color: var(--success);
        }

        .col-fps.inactive {
          color: var(--text-muted);
        }

        .col-msgs {
          text-align: right;
          color: var(--text-secondary);
        }

        .col-bw {
          text-align: right;
          color: var(--accent-secondary);
        }

        .no-data {
          padding: 24px;
          text-align: center;
          color: var(--text-muted);
          font-size: 13px;
        }

        @media (max-width: 768px) {
          .panel-type-badge {
            padding: 2px 6px;
            font-size: 9px;
          }

          .maximize-btn {
            display: none;
          }

          .stats-summary {
            flex-wrap: wrap;
            gap: 12px;
          }

          .stat-item {
            min-width: 80px;
          }

          .stats-table-header,
          .stats-row {
            grid-template-columns: 1fr 40px 60px 70px;
            font-size: 11px;
          }
        }
      `}</style>
    </div>
  );
}
