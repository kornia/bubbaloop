import { useRef, useEffect, useCallback, useState } from 'react';
import { Sample } from '@eclipse-zenoh/zenoh-ts';
import { getSamplePayload } from '../lib/zenoh';
import { useZenohSubscription } from '../hooks/useZenohSubscription';
import { H264Decoder } from '../lib/h264-decoder';
import { decodeCompressedImage, Header } from '../proto/camera';

interface DragHandleProps {
  [key: string]: unknown;
}

interface CameraViewProps {
  cameraName: string;
  topic: string;
  isMaximized?: boolean;
  onMaximize?: () => void;
  onTopicChange?: (topic: string) => void;
  onRemove?: () => void;
  availableTopics?: string[];
  dragHandleProps?: DragHandleProps;
}

export function CameraView({
  cameraName,
  topic,
  isMaximized = false,
  onMaximize,
  onTopicChange,
  onRemove,
  availableTopics = [],
  dragHandleProps,
}: CameraViewProps) {
  const canvasRef = useRef<HTMLCanvasElement>(null);
  const panelRef = useRef<HTMLDivElement>(null);
  const decoderRef = useRef<H264Decoder | null>(null);
  const [decoderError, setDecoderError] = useState<string | null>(null);
  const [isReady, setIsReady] = useState(false);
  const [dimensions, setDimensions] = useState<{ width: number; height: number } | null>(null);
  const frameCountRef = useRef(0);
  const [showInfo, setShowInfo] = useState(false);
  const lastMetaRef = useRef<{
    header?: Header;
    format: string;
    dataSize: number;
  } | null>(null);
  const [lastMeta, setLastMeta] = useState(lastMetaRef.current);
  const metaUpdateIntervalRef = useRef<number | null>(null);

  // Handle decoded frame - render to canvas
  const handleFrame = useCallback((frame: VideoFrame) => {
    const canvas = canvasRef.current;
    if (!canvas) {
      frame.close();
      return;
    }

    // Update canvas dimensions if needed
    if (canvas.width !== frame.displayWidth || canvas.height !== frame.displayHeight) {
      canvas.width = frame.displayWidth;
      canvas.height = frame.displayHeight;
      setDimensions({ width: frame.displayWidth, height: frame.displayHeight });
    }

    // Draw frame to canvas
    const ctx = canvas.getContext('2d');
    if (ctx) {
      ctx.drawImage(frame, 0, 0);
      frameCountRef.current++;
    }

    // Important: close the frame to release resources
    frame.close();
  }, []);

  // Initialize decoder
  useEffect(() => {
    if (!H264Decoder.isSupported()) {
      setDecoderError('WebCodecs not supported. Use Chrome 94+, Edge 94+, or Safari 16.4+');
      return;
    }

    const decoder = new H264Decoder({
      onFrame: handleFrame,
      onError: (error) => setDecoderError(error.message),
    });

    decoder.init()
      .then(() => {
        decoderRef.current = decoder;
        setIsReady(true);
        console.log(`[CameraView] Decoder ready for ${cameraName}`);
      })
      .catch((e) => {
        setDecoderError(e.message);
      });

    return () => {
      decoder.close();
      decoderRef.current = null;
    };
  }, [cameraName, handleFrame]);

  // Handle incoming samples from Zenoh
  const handleSample = useCallback((sample: Sample) => {
    const decoder = decoderRef.current;
    if (!decoder) return;

    try {
      const payload = getSamplePayload(sample);
      const msg = decodeCompressedImage(payload);

      // Store latest metadata in ref (no re-render)
      lastMetaRef.current = {
        header: msg.header,
        format: msg.format,
        dataSize: msg.data.length,
      };

      // Skip non-h264 formats (but allow empty format in case field is missing)
      if (msg.format && msg.format !== 'h264') {
        console.warn(`[CameraView] Unexpected format: ${msg.format}`);
        return;
      }

      // Use pub_time as timestamp (convert from nanoseconds to microseconds)
      const timestamp = msg.header
        ? Number(msg.header.pubTime / 1000n)
        : Date.now() * 1000;

      // Fire-and-forget async decode call
      decoder.decode(msg.data, timestamp).catch((e) => {
        console.error('[CameraView] Decode error:', e);
      });
    } catch (e) {
      console.error('[CameraView] Failed to process sample:', e);
    }
  }, []);

  // Subscribe to camera topic
  useZenohSubscription(topic, handleSample);

  // Periodically update metadata state when info panel is visible
  useEffect(() => {
    if (showInfo) {
      // Update immediately
      setLastMeta(lastMetaRef.current);
      // Then update every 500ms
      metaUpdateIntervalRef.current = window.setInterval(() => {
        setLastMeta(lastMetaRef.current);
      }, 500);
    }
    return () => {
      if (metaUpdateIntervalRef.current) {
        clearInterval(metaUpdateIntervalRef.current);
        metaUpdateIntervalRef.current = null;
      }
    };
  }, [showInfo]);

  // Handle topic change from dropdown
  const handleTopicSelect = (newTopic: string) => {
    if (newTopic && newTopic !== topic && onTopicChange) {
      onTopicChange(newTopic);
    }
  };

  return (
    <div ref={panelRef} className={`camera-view ${isMaximized ? 'maximized' : ''}`}>
      <div className="camera-header">
        <div className="camera-header-left">
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
          <span className="panel-type-badge">CAMERA</span>
        </div>
        <div className="camera-stats">
          {dimensions && (
            <span className="stat">
              <span className="stat-value mono">{dimensions.width}x{dimensions.height}</span>
              <span className="stat-label">res</span>
            </span>
          )}
          <span className={`status-badge ${isReady ? 'ready' : 'waiting'}`}>
            {isReady ? 'LIVE' : 'INIT'}
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
          <button className={`icon-btn ${showInfo ? 'active' : ''}`} onClick={() => setShowInfo(!showInfo)} title="Show metadata">
            <svg width="14" height="14" viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="2">
              <circle cx="12" cy="12" r="10" />
              <path d="M12 16v-4M12 8h.01" />
            </svg>
          </button>
          {onRemove && (
            <button className="icon-btn danger" onClick={onRemove} title="Remove camera">
              <svg width="14" height="14" viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="2">
                <path d="M18 6L6 18M6 6l12 12" />
              </svg>
            </button>
          )}
        </div>
      </div>

      <div className="camera-canvas-container">
        {decoderError ? (
          <div className="camera-error">
            <span className="error-icon">⚠</span>
            <span>{decoderError}</span>
          </div>
        ) : (
          <>
            <canvas ref={canvasRef} className="camera-canvas" />
            {!dimensions && isReady && (
              <div className="camera-waiting">
                <span>Waiting for keyframe...</span>
              </div>
            )}
          </>
        )}
        {!isReady && !decoderError && (
          <div className="camera-loading">
            <div className="spinner" />
            <span>Initializing decoder...</span>
          </div>
        )}
      </div>

      {showInfo && lastMeta && (
        <div className="camera-info-panel">
          <div className="info-grid">
            <div className="info-item">
              <span className="info-label">Format</span>
              <span className="info-value mono">{lastMeta.format || 'N/A'}</span>
            </div>
            <div className="info-item">
              <span className="info-label">Data Size</span>
              <span className="info-value mono">{lastMeta.dataSize.toLocaleString()} bytes</span>
            </div>
            {lastMeta.header && (
              <>
                <div className="info-item">
                  <span className="info-label">Sequence</span>
                  <span className="info-value mono">{lastMeta.header.sequence.toLocaleString()}</span>
                </div>
                <div className="info-item">
                  <span className="info-label">Frame ID</span>
                  <span className="info-value mono">{lastMeta.header.frameId || 'N/A'}</span>
                </div>
                <div className="info-item full-width">
                  <span className="info-label">Acq Time (ns)</span>
                  <span className="info-value mono">
                    {lastMeta.header.acqTime > 0n ? lastMeta.header.acqTime.toString() : 'N/A'}
                  </span>
                </div>
                <div className="info-item full-width">
                  <span className="info-label">Pub Time (ns)</span>
                  <span className="info-value mono">{lastMeta.header.pubTime.toString()}</span>
                </div>
                {(() => {
                  // Only show latency if both times are valid and the difference is reasonable (< 10s)
                  const latencyNs = lastMeta.header.pubTime - lastMeta.header.acqTime;
                  const latencyMs = Number(latencyNs) / 1_000_000;
                  if (lastMeta.header.acqTime > 0n && latencyNs > 0n && latencyMs < 10000) {
                    return (
                      <div className="info-item full-width">
                        <span className="info-label">Latency</span>
                        <span className="info-value mono">{latencyMs.toFixed(2)} ms</span>
                      </div>
                    );
                  }
                  return null;
                })()}
              </>
            )}
          </div>
        </div>
      )}

      <div className="camera-footer">
        {(() => {
          // Filter to only show CompressedImage topics
          const cameraTopics = availableTopics.filter(t => t.includes('CompressedImage'));
          return cameraTopics.length > 0 ? (
            <select
              className="topic-select"
              value={topic}
              onChange={(e) => handleTopicSelect(e.target.value)}
            >
              {!topic && <option value="">-- Select camera --</option>}
              {cameraTopics.map((t) => (
                <option key={t} value={t}>{t}</option>
              ))}
            </select>
          ) : (
            <span className="topic mono">{topic || 'No camera topics available'}</span>
          );
        })()}
      </div>

      <style>{`
        .camera-view {
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

        .camera-view:hover {
          border-color: var(--border-glow);
          box-shadow: var(--shadow-glow);
        }

        .camera-view.maximized {
          border-color: var(--accent-primary);
          height: calc(100vh - 140px);
        }

        .camera-header {
          display: flex;
          justify-content: space-between;
          align-items: center;
          padding: 10px 12px;
          background: var(--bg-tertiary);
          border-bottom: 1px solid var(--border-color);
          gap: 8px;
        }

        .camera-header-left {
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

        .camera-name {
          font-weight: 600;
          font-size: 14px;
          color: var(--text-primary);
          text-transform: capitalize;
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

        .camera-stats {
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

        .status-badge {
          padding: 2px 8px;
          border-radius: 4px;
          font-size: 10px;
          font-weight: 600;
          letter-spacing: 0.5px;
        }

        .status-badge.ready {
          background: rgba(0, 200, 83, 0.2);
          color: var(--success);
        }

        .status-badge.waiting {
          background: rgba(255, 214, 0, 0.2);
          color: var(--warning);
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

        .icon-btn.active {
          background: var(--accent-primary);
          border-color: var(--accent-primary);
          color: white;
        }

        .camera-canvas-container {
          display: flex;
          align-items: center;
          justify-content: center;
          background: #000;
          min-height: 240px;
          aspect-ratio: 16 / 9;
          position: relative;
        }

        .camera-view.maximized .camera-canvas-container {
          aspect-ratio: unset;
          flex: 1;
          min-height: 60vh;
        }

        .camera-canvas {
          max-width: 100%;
          max-height: 100%;
          object-fit: contain;
        }

        .camera-view.maximized .camera-canvas {
          width: 100%;
          height: 100%;
        }

        .camera-error {
          display: flex;
          flex-direction: column;
          align-items: center;
          gap: 8px;
          color: var(--error);
          padding: 24px;
          text-align: center;
        }

        .error-icon {
          font-size: 24px;
        }

        .camera-loading,
        .camera-waiting {
          position: absolute;
          display: flex;
          flex-direction: column;
          align-items: center;
          gap: 12px;
          color: var(--text-muted);
        }

        .camera-loading .spinner {
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

        .camera-footer {
          padding: 8px 12px;
          background: var(--bg-tertiary);
          border-top: 1px solid var(--border-color);
          min-width: 0;
          overflow: hidden;
        }

        .camera-info-panel {
          padding: 12px;
          background: var(--bg-tertiary);
          border-top: 1px solid var(--border-color);
          min-width: 0;
          overflow: hidden;
        }

        .info-grid {
          display: grid;
          grid-template-columns: 1fr 1fr;
          gap: 8px;
        }

        .info-item {
          display: flex;
          flex-direction: column;
          gap: 2px;
        }

        .info-item.full-width {
          grid-column: 1 / -1;
        }

        .info-label {
          font-size: 10px;
          text-transform: uppercase;
          letter-spacing: 0.5px;
          color: var(--text-muted);
        }

        .info-value {
          font-size: 12px;
          color: var(--text-primary);
          word-break: break-all;
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
          .camera-header {
            padding: 8px 10px;
            flex-wrap: wrap;
          }

          .camera-header-left {
            gap: 6px;
          }

          .camera-name {
            font-size: 13px;
          }

          .panel-type-badge {
            padding: 2px 6px;
            font-size: 9px;
          }

          .camera-stats {
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

          .status-badge {
            padding: 2px 6px;
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

          .camera-canvas-container {
            min-height: 180px;
          }

          .camera-footer {
            padding: 6px 10px;
          }

          .camera-info-panel {
            padding: 10px;
          }

          .info-grid {
            gap: 6px;
          }

          .info-label {
            font-size: 9px;
          }

          .info-value {
            font-size: 11px;
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
          .camera-header {
            padding: 6px 8px;
          }

          .camera-stats .stat:not(:last-child) {
            display: none;
          }

          .camera-canvas-container {
            min-height: 150px;
          }

          .info-grid {
            grid-template-columns: 1fr;
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
