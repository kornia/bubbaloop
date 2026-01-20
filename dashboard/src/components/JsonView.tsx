import { useCallback, useState, useRef } from 'react';
import { Session, Sample } from '@eclipse-zenoh/zenoh-ts';
import { useZenohSubscriber, getSamplePayload } from '../lib/zenoh';
import { decodeCompressedImage } from '../proto/camera';
import { decodeCurrentWeather, decodeHourlyForecast, decodeDailyForecast } from '../proto/weather';
import JsonView from 'react18-json-view';
import 'react18-json-view/src/style.css';

// Extract schema name from ros-z topic format: <domain_id>/<topic>/<schema>/<hash>
// e.g., "0/weather%current/bubbaloop.weather.v1.CurrentWeather/RIHS01_..."
function extractSchemaFromTopic(topic: string): string | null {
  const parts = topic.split('/');
  // Schema is typically the third-to-last or second-to-last part
  for (let i = parts.length - 2; i >= 1; i--) {
    const part = parts[i];
    // Schema looks like "bubbaloop.weather.v1.CurrentWeather"
    if (part.includes('.') && !part.startsWith('RIHS')) {
      return part;
    }
  }
  return null;
}

// Convert BigInt values to strings for JSON serialization
function bigIntToString(obj: unknown): unknown {
  if (typeof obj === 'bigint') {
    return obj.toString();
  }
  if (Array.isArray(obj)) {
    return obj.map(bigIntToString);
  }
  if (obj !== null && typeof obj === 'object') {
    const result: Record<string, unknown> = {};
    for (const [key, value] of Object.entries(obj)) {
      result[key] = bigIntToString(value);
    }
    return result;
  }
  return obj;
}

// Try to decode payload in various formats, using schema hint from topic
function decodePayload(payload: Uint8Array, topic: string): { data: unknown; schema: string; error?: string } {
  const schemaFromTopic = extractSchemaFromTopic(topic);
  const text = new TextDecoder().decode(payload);

  // 1. Try JSON first
  try {
    const parsed = JSON.parse(text);
    return { data: parsed, schema: 'JSON' };
  } catch {
    // Not JSON, continue
  }

  // 2. Try decoding based on schema hint from topic
  if (schemaFromTopic) {
    // Weather types
    if (schemaFromTopic.includes('CurrentWeather')) {
      const msg = decodeCurrentWeather(payload);
      if (msg) {
        return { data: bigIntToString(msg), schema: schemaFromTopic };
      }
    }
    if (schemaFromTopic.includes('HourlyForecast')) {
      const msg = decodeHourlyForecast(payload);
      if (msg) {
        return { data: bigIntToString(msg), schema: schemaFromTopic };
      }
    }
    if (schemaFromTopic.includes('DailyForecast')) {
      const msg = decodeDailyForecast(payload);
      if (msg) {
        return { data: bigIntToString(msg), schema: schemaFromTopic };
      }
    }
    // Camera types
    if (schemaFromTopic.includes('CompressedImage')) {
      const msg = decodeCompressedImage(payload);
      if (msg.format || msg.header) {
        const jsonData: Record<string, unknown> = {
          format: msg.format,
          dataSize: msg.data.length,
        };
        if (msg.header) {
          jsonData.header = {
            acqTime: msg.header.acqTime.toString(),
            pubTime: msg.header.pubTime.toString(),
            sequence: msg.header.sequence,
            frameId: msg.header.frameId,
          };
          if (msg.header.acqTime > 0n && msg.header.pubTime > 0n) {
            const latencyNs = msg.header.pubTime - msg.header.acqTime;
            const latencyMs = Number(latencyNs) / 1_000_000;
            if (latencyMs > 0 && latencyMs < 10000) {
              jsonData.latencyMs = latencyMs.toFixed(2);
            }
          }
        }
        return { data: jsonData, schema: schemaFromTopic };
      }
    }
  }

  // 3. Fallback: try all known protobuf decoders
  // Try CompressedImage
  try {
    const msg = decodeCompressedImage(payload);
    if (msg.format || msg.header) {
      const jsonData: Record<string, unknown> = {
        format: msg.format,
        dataSize: msg.data.length,
      };
      if (msg.header) {
        jsonData.header = {
          acqTime: msg.header.acqTime.toString(),
          pubTime: msg.header.pubTime.toString(),
          sequence: msg.header.sequence,
          frameId: msg.header.frameId,
        };
        if (msg.header.acqTime > 0n && msg.header.pubTime > 0n) {
          const latencyNs = msg.header.pubTime - msg.header.acqTime;
          const latencyMs = Number(latencyNs) / 1_000_000;
          if (latencyMs > 0 && latencyMs < 10000) {
            jsonData.latencyMs = latencyMs.toFixed(2);
          }
        }
      }
      return { data: jsonData, schema: 'bubbaloop.camera.v1.CompressedImage' };
    }
  } catch {
    // Not a valid CompressedImage
  }

  // Try CurrentWeather
  try {
    const msg = decodeCurrentWeather(payload);
    if (msg && msg.timezone) {
      return { data: bigIntToString(msg), schema: 'bubbaloop.weather.v1.CurrentWeather' };
    }
  } catch {
    // Not a valid CurrentWeather
  }

  // 4. Show raw data preview
  const preview = payload.slice(0, 100);
  const hex = Array.from(preview).map(b => b.toString(16).padStart(2, '0')).join(' ');
  return {
    data: {
      _format: 'binary',
      _size: payload.length,
      _hexPreview: hex + (payload.length > 100 ? '...' : ''),
    },
    schema: 'Binary',
    error: 'Unknown binary format - showing hex preview',
  };
}

interface DragHandleProps {
  [key: string]: unknown;
}

interface RawDataViewPanelProps {
  session: Session;
  topic: string;
  isMaximized?: boolean;
  onMaximize?: () => void;
  onTopicChange?: (topic: string) => void;
  onRemove?: () => void;
  availableTopics?: string[];
  dragHandleProps?: DragHandleProps;
}

export function RawDataViewPanel({
  session,
  topic,
  isMaximized = false,
  onMaximize,
  onTopicChange,
  onRemove,
  availableTopics = [],
  dragHandleProps,
}: RawDataViewPanelProps) {
  const [jsonData, setJsonData] = useState<unknown>(null);
  const [schemaName, setSchemaName] = useState<string | null>(null);
  const [parseError, setParseError] = useState<string | null>(null);
  // Track the actual topic the data came from (for debugging/display purposes)
  const [, setCurrentTopic] = useState<string>('');
  const lastUpdateRef = useRef<number>(0);

  // Handle incoming samples from Zenoh
  const handleSample = useCallback((sample: Sample) => {
    try {
      const payload = getSamplePayload(sample);
      const sampleTopic = sample.keyexpr().toString();
      const result = decodePayload(payload, sampleTopic);

      setJsonData(result.data);
      setSchemaName(result.schema);
      setParseError(result.error || null);
      setCurrentTopic(sampleTopic);

      lastUpdateRef.current = Date.now();
    } catch (e) {
      console.error('[RawDataView] Failed to process sample:', e);
      setParseError(e instanceof Error ? e.message : 'Failed to process sample');
    }
  }, []);

  // Subscribe to topic
  const { fps, messageCount } = useZenohSubscriber(session, topic, handleSample);

  // Handle topic change from dropdown
  const handleTopicSelect = (newTopic: string) => {
    if (newTopic && newTopic !== topic && onTopicChange) {
      onTopicChange(newTopic);
    }
  };

  return (
    <div className={`rawdata-view-panel ${isMaximized ? 'maximized' : ''}`}>
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
          <span className="panel-type-badge">{schemaName || 'RAW DATA'}</span>
        </div>
        <div className="panel-stats">
          <span className="stat">
            <span className="stat-value">{fps}</span>
            <span className="stat-label">msg/s</span>
          </span>
          <span className="stat">
            <span className="stat-value mono">{messageCount.toLocaleString()}</span>
            <span className="stat-label">total</span>
          </span>
          {onMaximize && (
            <button className="icon-btn maximize-btn" onClick={onMaximize} title={isMaximized ? 'Restore' : 'Maximize'}>
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
          )}
          {onRemove && (
            <button className="icon-btn danger" onClick={onRemove} title="Remove panel">
              <svg width="14" height="14" viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="2">
                <path d="M18 6L6 18M6 6l12 12" />
              </svg>
            </button>
          )}
        </div>
      </div>

      <div className="rawdata-content-container">
        {!topic ? (
          <div className="rawdata-placeholder">
            <span className="placeholder-icon">{ }</span>
            <p>Select a topic to start receiving data</p>
          </div>
        ) : jsonData === null ? (
          <div className="rawdata-waiting">
            <div className="spinner" />
            <span>Waiting for data...</span>
          </div>
        ) : (
          <div className="rawdata-content">
            <JsonView
              src={jsonData}
              collapseStringsAfterLength={80}
              enableClipboard
              theme="a11y"
              dark
              style={{
                backgroundColor: 'transparent',
                fontSize: '12px',
                fontFamily: "'JetBrains Mono', 'Fira Code', monospace",
              }}
            />
            {parseError && (
              <div className="rawdata-parse-error">
                <span>⚠</span> {parseError}
              </div>
            )}
          </div>
        )}
      </div>

      <div className="panel-footer">
        {availableTopics.length > 0 ? (
          <select
            className="topic-select"
            value={topic}
            onChange={(e) => handleTopicSelect(e.target.value)}
          >
            <option value="">-- Select topic --</option>
            {availableTopics.map((t) => (
              <option key={t} value={t}>{t}</option>
            ))}
          </select>
        ) : (
          <span className="topic mono">{topic || 'No topic selected'}</span>
        )}
      </div>

      <style>{`
        .rawdata-view-panel {
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

        .rawdata-view-panel:hover {
          border-color: var(--border-glow);
          box-shadow: var(--shadow-glow);
        }

        .rawdata-view-panel.maximized {
          border-color: var(--accent-primary);
        }

        .panel-header {
          display: flex;
          justify-content: space-between;
          align-items: center;
          padding: 10px 12px;
          background: var(--bg-tertiary);
          border-bottom: 1px solid var(--border-color);
          gap: 8px;
        }

        .panel-header-left {
          display: flex;
          align-items: center;
          gap: 8px;
          min-width: 0;
        }

        .drag-handle {
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

        .drag-handle:hover {
          background: var(--bg-primary);
          color: var(--text-secondary);
        }

        .drag-handle:active {
          cursor: grabbing;
        }

        .panel-name {
          font-weight: 600;
          font-size: 14px;
          color: var(--text-primary);
          white-space: nowrap;
          overflow: hidden;
          text-overflow: ellipsis;
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

        .panel-stats {
          display: flex;
          gap: 12px;
          align-items: center;
          flex-shrink: 0;
        }

        .stat {
          display: flex;
          align-items: baseline;
          gap: 4px;
        }

        .stat-value {
          font-weight: 600;
          font-size: 13px;
          color: var(--accent-secondary);
        }

        .stat-label {
          font-size: 10px;
          color: var(--text-muted);
          text-transform: uppercase;
          letter-spacing: 0.5px;
        }

        .icon-btn {
          display: flex;
          align-items: center;
          justify-content: center;
          width: 28px;
          height: 28px;
          background: transparent;
          border: 1px solid var(--border-color);
          border-radius: 6px;
          color: var(--text-secondary);
          cursor: pointer;
          transition: all 0.15s;
        }

        .icon-btn:hover {
          background: var(--bg-primary);
          border-color: var(--text-muted);
          color: var(--text-primary);
        }

        .icon-btn.danger:hover {
          background: rgba(255, 23, 68, 0.1);
          border-color: var(--error);
          color: var(--error);
        }

        .rawdata-content-container {
          flex: 1;
          min-height: 200px;
          max-height: 500px;
          overflow: auto;
          background: var(--bg-primary);
        }

        .rawdata-placeholder,
        .rawdata-waiting {
          display: flex;
          flex-direction: column;
          align-items: center;
          justify-content: center;
          height: 100%;
          min-height: 200px;
          color: var(--text-muted);
          gap: 12px;
        }

        .placeholder-icon {
          font-size: 32px;
          font-family: 'JetBrains Mono', monospace;
          color: var(--text-muted);
        }

        .rawdata-waiting .spinner {
          width: 24px;
          height: 24px;
          border: 2px solid var(--border-color);
          border-top-color: var(--accent-primary);
          border-radius: 50%;
          animation: spin 1s linear infinite;
        }

        @keyframes spin {
          to { transform: rotate(360deg); }
        }

        .rawdata-content {
          padding: 12px;
        }

        .rawdata-parse-error {
          display: flex;
          align-items: center;
          gap: 6px;
          padding: 8px 12px;
          margin-top: 8px;
          background: rgba(255, 214, 0, 0.1);
          border: 1px solid var(--warning);
          border-radius: 6px;
          color: var(--warning);
          font-size: 12px;
        }

        .panel-footer {
          padding: 8px 12px;
          background: var(--bg-tertiary);
          border-top: 1px solid var(--border-color);
          min-width: 0;
          overflow: hidden;
        }

        .topic-select {
          width: 100%;
          padding: 6px 8px;
          background: var(--bg-primary);
          border: 1px solid var(--border-color);
          border-radius: 4px;
          color: var(--text-primary);
          font-size: 11px;
          font-family: 'JetBrains Mono', monospace;
          cursor: pointer;
        }

        .topic-select:focus {
          outline: none;
          border-color: var(--accent-primary);
        }

        .topic {
          font-size: 11px;
          color: var(--text-muted);
          white-space: nowrap;
          overflow: hidden;
          text-overflow: ellipsis;
          display: block;
        }

        /* Mobile responsive styles */
        @media (max-width: 768px) {
          .panel-header {
            padding: 8px 10px;
            flex-wrap: wrap;
          }

          .panel-header-left {
            gap: 6px;
          }

          .panel-name {
            font-size: 13px;
          }

          .panel-type-badge {
            padding: 2px 6px;
            font-size: 9px;
          }

          .panel-stats {
            gap: 8px;
          }

          .stat {
            gap: 2px;
          }

          .stat-value {
            font-size: 12px;
          }

          .stat-label {
            font-size: 9px;
          }

          .icon-btn {
            width: 32px;
            height: 32px;
            min-width: 32px;
          }

          .maximize-btn {
            display: none;
          }

          .rawdata-content-container {
            min-height: 150px;
            max-height: none;
          }

          .rawdata-content {
            padding: 10px;
          }

          .panel-footer {
            padding: 6px 10px;
          }

          .topic-select {
            padding: 10px 8px;
            font-size: 14px;
          }

          .topic {
            font-size: 10px;
          }
        }

        @media (max-width: 480px) {
          .panel-header {
            padding: 6px 8px;
          }

          .panel-stats .stat:not(:last-child) {
            display: none;
          }

          .icon-btn {
            width: 36px;
            height: 36px;
          }
        }
      `}</style>
    </div>
  );
}

// Legacy alias for backward compatibility
export const JsonViewPanel = RawDataViewPanel;
