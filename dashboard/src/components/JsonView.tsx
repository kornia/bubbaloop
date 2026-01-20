import { useCallback, useState, useRef, useEffect } from 'react';
import { Session, Sample } from '@eclipse-zenoh/zenoh-ts';
import { useZenohSubscriber, getSamplePayload } from '../lib/zenoh';
import { decodeCompressedImage } from '../proto/camera';
import JsonView from 'react18-json-view';
import 'react18-json-view/src/style.css';

// Try to decode payload in various formats
function decodePayload(payload: Uint8Array): { data: unknown; format: string; error?: string } {
  const text = new TextDecoder().decode(payload);

  // 1. Try JSON first
  try {
    const parsed = JSON.parse(text);
    return { data: parsed, format: 'json' };
  } catch {
    // Not JSON, continue
  }

  // 2. Try CompressedImage protobuf
  try {
    const msg = decodeCompressedImage(payload);
    // Check if it looks like a valid CompressedImage (has format or header)
    if (msg.format || msg.header) {
      // Convert to JSON-serializable format (BigInt -> string)
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
        // Calculate latency if both times are valid
        if (msg.header.acqTime > 0n && msg.header.pubTime > 0n) {
          const latencyNs = msg.header.pubTime - msg.header.acqTime;
          const latencyMs = Number(latencyNs) / 1_000_000;
          if (latencyMs > 0 && latencyMs < 10000) {
            jsonData.latencyMs = latencyMs.toFixed(2);
          }
        }
      }
      return { data: jsonData, format: 'protobuf (CompressedImage)' };
    }
  } catch {
    // Not a valid CompressedImage, continue
  }

  // 3. Show raw data preview
  const preview = payload.slice(0, 100);
  const hex = Array.from(preview).map(b => b.toString(16).padStart(2, '0')).join(' ');
  return {
    data: {
      _format: 'binary',
      _size: payload.length,
      _hexPreview: hex + (payload.length > 100 ? '...' : ''),
    },
    format: 'binary',
    error: 'Unknown binary format - showing hex preview',
  };
}

interface DragHandleProps {
  [key: string]: unknown;
}

interface JsonViewPanelProps {
  session: Session;
  panelName: string;
  topic: string;
  isMaximized?: boolean;
  onMaximize?: () => void;
  onTopicChange?: (topic: string) => void;
  onNameChange?: (name: string) => void;
  onRemove?: () => void;
  availableTopics?: string[];
  dragHandleProps?: DragHandleProps;
}

export function JsonViewPanel({
  session,
  panelName,
  topic,
  isMaximized = false,
  onMaximize,
  onTopicChange,
  onNameChange,
  onRemove,
  availableTopics = [],
  dragHandleProps,
}: JsonViewPanelProps) {
  const [jsonData, setJsonData] = useState<unknown>(null);
  const [dataFormat, setDataFormat] = useState<string | null>(null);
  const [parseError, setParseError] = useState<string | null>(null);
  const [isEditing, setIsEditing] = useState(false);
  const [editName, setEditName] = useState(panelName);
  const [editTopic, setEditTopic] = useState(topic);
  const lastUpdateRef = useRef<number>(0);
  const editFooterRef = useRef<HTMLDivElement>(null);

  // Handle incoming samples from Zenoh
  const handleSample = useCallback((sample: Sample) => {
    try {
      const payload = getSamplePayload(sample);
      const result = decodePayload(payload);

      setJsonData(result.data);
      setDataFormat(result.format);
      setParseError(result.error || null);

      lastUpdateRef.current = Date.now();
    } catch (e) {
      console.error('[JsonView] Failed to process sample:', e);
      setParseError(e instanceof Error ? e.message : 'Failed to process sample');
    }
  }, []);

  // Subscribe to topic
  const { fps, messageCount } = useZenohSubscriber(session, topic, handleSample);

  // Scroll edit footer into view when editing starts
  useEffect(() => {
    if (isEditing && editFooterRef.current) {
      setTimeout(() => {
        editFooterRef.current?.scrollIntoView({ behavior: 'smooth', block: 'nearest' });
      }, 100);
    }
  }, [isEditing]);

  const handleSaveEdit = () => {
    if (editName !== panelName && onNameChange) {
      onNameChange(editName);
    }
    if (editTopic !== topic && onTopicChange) {
      onTopicChange(editTopic);
    }
    setIsEditing(false);
  };

  const handleCancelEdit = () => {
    setEditName(panelName);
    setEditTopic(topic);
    setIsEditing(false);
  };

  // Update edit state when props change
  useEffect(() => {
    setEditName(panelName);
    setEditTopic(topic);
  }, [panelName, topic]);

  return (
    <div className={`json-view-panel ${isMaximized ? 'maximized' : ''}`}>
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
          {isEditing ? (
            <input
              type="text"
              value={editName}
              onChange={(e) => setEditName(e.target.value)}
              className="panel-name-input"
              autoFocus
            />
          ) : (
            <span className="panel-name">{panelName}</span>
          )}
          <span className="panel-type-badge">{dataFormat || 'DATA'}</span>
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
            <button className="icon-btn" onClick={onMaximize} title={isMaximized ? 'Restore' : 'Maximize'}>
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
          <button className="icon-btn" onClick={() => setIsEditing(!isEditing)} title="Edit panel">
            <svg width="14" height="14" viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="2">
              <path d="M11 4H4a2 2 0 00-2 2v14a2 2 0 002 2h14a2 2 0 002-2v-7" />
              <path d="M18.5 2.5a2.121 2.121 0 013 3L12 15l-4 1 1-4 9.5-9.5z" />
            </svg>
          </button>
          {onRemove && (
            <button className="icon-btn danger" onClick={onRemove} title="Remove panel">
              <svg width="14" height="14" viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="2">
                <path d="M18 6L6 18M6 6l12 12" />
              </svg>
            </button>
          )}
        </div>
      </div>

      <div className="json-content-container">
        {!topic ? (
          <div className="json-placeholder">
            <span className="placeholder-icon">{ }</span>
            <p>Select a topic to start receiving JSON data</p>
          </div>
        ) : jsonData === null ? (
          <div className="json-waiting">
            <div className="spinner" />
            <span>Waiting for data...</span>
          </div>
        ) : (
          <div className="json-content">
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
              <div className="json-parse-error">
                <span>⚠</span> {parseError}
              </div>
            )}
          </div>
        )}
      </div>

      {isEditing ? (
        <div ref={editFooterRef} className="panel-edit-footer">
          <div className="topic-selector">
            <label>Topic:</label>
            {availableTopics.length > 0 ? (
              <select
                value={editTopic}
                onChange={(e) => setEditTopic(e.target.value)}
                className="topic-select"
              >
                <option value="">-- Select topic --</option>
                {availableTopics.map((t) => (
                  <option key={t} value={t}>{t}</option>
                ))}
              </select>
            ) : null}
            <input
              type="text"
              value={editTopic}
              onChange={(e) => setEditTopic(e.target.value)}
              placeholder="Enter topic pattern..."
              className="topic-input mono"
            />
          </div>
          <div className="edit-actions">
            <button className="btn-secondary" onClick={handleCancelEdit}>Cancel</button>
            <button className="btn-primary" onClick={handleSaveEdit}>Save</button>
          </div>
        </div>
      ) : (
        <div className="panel-footer">
          <span className="topic mono">{topic || 'No topic selected'}</span>
        </div>
      )}

      <style>{`
        .json-view-panel {
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

        .json-view-panel:hover {
          border-color: var(--border-glow);
          box-shadow: var(--shadow-glow);
        }

        .json-view-panel.maximized {
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

        .panel-name-input {
          font-weight: 600;
          font-size: 14px;
          color: var(--text-primary);
          background: var(--bg-primary);
          border: 1px solid var(--accent-primary);
          border-radius: 4px;
          padding: 2px 6px;
          width: 120px;
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

        .json-content-container {
          flex: 1;
          min-height: 200px;
          max-height: 500px;
          overflow: auto;
          background: var(--bg-primary);
        }

        .json-placeholder,
        .json-waiting {
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

        .json-waiting .spinner {
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

        .json-content {
          padding: 12px;
        }

        .json-parse-error {
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

        .panel-edit-footer {
          padding: 12px;
          background: var(--bg-tertiary);
          border-top: 1px solid var(--border-color);
          display: flex;
          flex-direction: column;
          gap: 12px;
          min-width: 0;
          overflow: hidden;
        }

        .topic-selector {
          display: flex;
          flex-direction: column;
          gap: 6px;
          min-width: 0;
          width: 100%;
        }

        .topic-selector label {
          font-size: 11px;
          text-transform: uppercase;
          letter-spacing: 0.5px;
          color: var(--text-muted);
        }

        .topic-select {
          padding: 6px 10px;
          background: var(--bg-primary);
          border: 1px solid var(--border-color);
          border-radius: 6px;
          color: var(--text-primary);
          font-size: 12px;
          width: 100%;
          box-sizing: border-box;
          min-width: 0;
        }

        .topic-input {
          padding: 6px 10px;
          background: var(--bg-primary);
          border: 1px solid var(--border-color);
          border-radius: 6px;
          color: var(--text-primary);
          font-size: 12px;
          width: 100%;
          box-sizing: border-box;
          min-width: 0;
        }

        .topic-input:focus,
        .topic-select:focus {
          border-color: var(--accent-primary);
          outline: none;
        }

        .edit-actions {
          display: flex;
          gap: 8px;
          justify-content: flex-end;
        }

        .btn-primary,
        .btn-secondary {
          padding: 6px 16px;
          border-radius: 6px;
          font-size: 12px;
          font-weight: 500;
          cursor: pointer;
          transition: all 0.15s;
        }

        .btn-primary {
          background: var(--accent-primary);
          border: none;
          color: white;
        }

        .btn-primary:hover {
          background: #5c7cfa;
        }

        .btn-secondary {
          background: transparent;
          border: 1px solid var(--border-color);
          color: var(--text-secondary);
        }

        .btn-secondary:hover {
          background: var(--bg-primary);
          border-color: var(--text-muted);
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

          .json-content-container {
            min-height: 150px;
            max-height: none;
          }

          .json-content {
            padding: 10px;
          }

          .panel-footer {
            padding: 6px 10px;
          }

          .panel-edit-footer {
            padding: 16px;
            gap: 16px;
          }

          .topic-input,
          .topic-select {
            padding: 14px 12px;
            font-size: 16px;
          }

          .edit-actions {
            flex-direction: column;
            gap: 10px;
          }

          .btn-primary,
          .btn-secondary {
            width: 100%;
            padding: 14px 24px;
            font-size: 15px;
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
