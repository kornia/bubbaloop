import { useState, useMemo } from 'react';
import { useZenohSession, useZenohTopicDiscovery, ConnectionStatus } from './lib/zenoh';
import { Dashboard } from './components/Dashboard';
import { H264Decoder } from './lib/h264-decoder';

// Default Zenoh endpoint (zenoh-plugin-remote-api WebSocket)
const DEFAULT_ZENOH_ENDPOINT = 'ws://127.0.0.1:10000';

// Default camera configuration - modify these to match your setup
// ros-z key format: {domain_id}/{topic_with_%_encoding}/{type_info}
// Topic paths in ros-z use % encoding: /camera/entrance/compressed -> camera%entrance%compressed
// Use wildcard pattern to match the type_info suffix
const DEFAULT_CAMERAS = [
  { name: 'entrance', topic: '0/camera%entrance%compressed/**' },
  { name: 'terrace', topic: '0/camera%terrace%compressed/**' },
];

function StatusIndicator({ status }: { status: ConnectionStatus }) {
  const statusConfig = {
    disconnected: { color: 'var(--text-muted)', label: 'Disconnected' },
    connecting: { color: 'var(--warning)', label: 'Connecting...' },
    connected: { color: 'var(--success)', label: 'Connected' },
    error: { color: 'var(--error)', label: 'Error' },
  };

  const config = statusConfig[status];

  return (
    <div className="status-indicator">
      <span className="status-dot" style={{ backgroundColor: config.color }} />
      <span className="status-label">{config.label}</span>

      <style>{`
        .status-indicator {
          display: flex;
          align-items: center;
          gap: 8px;
        }

        .status-dot {
          width: 8px;
          height: 8px;
          border-radius: 50%;
          animation: ${status === 'connecting' ? 'pulse 1s infinite' : 'none'};
        }

        .status-label {
          font-size: 13px;
          color: var(--text-secondary);
        }

        @keyframes pulse {
          0%, 100% { opacity: 1; }
          50% { opacity: 0.4; }
        }
      `}</style>
    </div>
  );
}

function ConnectionPanel({
  endpoint,
  onEndpointChange,
  status,
  error,
  onReconnect
}: {
  endpoint: string;
  onEndpointChange: (endpoint: string) => void;
  status: ConnectionStatus;
  error: Error | null;
  onReconnect: () => void;
}) {
  const [inputValue, setInputValue] = useState(endpoint);

  const handleSubmit = (e: React.FormEvent) => {
    e.preventDefault();
    if (inputValue !== endpoint) {
      onEndpointChange(inputValue);
    } else {
      onReconnect();
    }
  };

  return (
    <div className="connection-panel">
      <form onSubmit={handleSubmit}>
        <label>
          <span className="label-text">Zenoh Endpoint</span>
          <input
            type="text"
            value={inputValue}
            onChange={(e) => setInputValue(e.target.value)}
            placeholder="ws://127.0.0.1:10000"
            className="mono"
          />
        </label>
        <button type="submit" disabled={status === 'connecting'}>
          {status === 'connecting' ? 'Connecting...' : 'Connect'}
        </button>
      </form>
      {error && (
        <div className="error-message">
          {error.message}
        </div>
      )}

      <style>{`
        .connection-panel {
          padding: 12px 16px;
          background: var(--bg-tertiary);
          border-bottom: 1px solid var(--border-color);
        }

        .connection-panel form {
          display: flex;
          align-items: flex-end;
          gap: 12px;
        }

        .connection-panel label {
          flex: 1;
          display: flex;
          flex-direction: column;
          gap: 4px;
        }

        .label-text {
          font-size: 11px;
          text-transform: uppercase;
          letter-spacing: 0.5px;
          color: var(--text-muted);
        }

        .connection-panel input {
          padding: 8px 12px;
          background: var(--bg-primary);
          border: 1px solid var(--border-color);
          border-radius: 6px;
          color: var(--text-primary);
          font-size: 13px;
          outline: none;
          transition: border-color 0.2s;
        }

        .connection-panel input:focus {
          border-color: var(--accent-primary);
        }

        .connection-panel button {
          padding: 8px 20px;
          background: var(--accent-primary);
          border: none;
          border-radius: 6px;
          color: white;
          font-size: 13px;
          font-weight: 500;
          cursor: pointer;
          transition: background 0.2s, opacity 0.2s;
        }

        .connection-panel button:hover:not(:disabled) {
          background: #5c7cfa;
        }

        .connection-panel button:disabled {
          opacity: 0.6;
          cursor: not-allowed;
        }

        .error-message {
          margin-top: 8px;
          padding: 8px 12px;
          background: rgba(255, 23, 68, 0.1);
          border: 1px solid var(--error);
          border-radius: 6px;
          color: var(--error);
          font-size: 13px;
        }
      `}</style>
    </div>
  );
}

function BrowserCheck() {
  const isSupported = H264Decoder.isSupported();
  const isSecureContext = window.isSecureContext;
  const isLocalhost = window.location.hostname === 'localhost' || window.location.hostname === '127.0.0.1';

  // WebCodecs requires secure context (localhost or https)
  if (isSupported && isSecureContext) return null;

  return (
    <div className="browser-check">
      <div className="warning-content">
        <span className="warning-icon">⚠️</span>
        <div>
          <strong>WebCodecs not supported</strong>
          {!isSecureContext && !isLocalhost ? (
            <>
              <p>WebCodecs requires a <strong>secure context</strong>.</p>
              <p>Access via <code>http://localhost:5173</code> instead of <code>{window.location.host}</code></p>
            </>
          ) : (
            <>
              <p>Your browser doesn't support WebCodecs API for H264 decoding.</p>
              <p>Please use Chrome 94+, Edge 94+, or Safari 16.4+.</p>
            </>
          )}
        </div>
      </div>

      <style>{`
        .browser-check {
          padding: 16px 24px;
          background: rgba(255, 214, 0, 0.1);
          border-bottom: 1px solid var(--warning);
        }

        .warning-content {
          display: flex;
          gap: 12px;
          align-items: flex-start;
          max-width: 800px;
          margin: 0 auto;
        }

        .warning-icon {
          font-size: 24px;
        }

        .warning-content strong {
          color: var(--warning);
          display: block;
          margin-bottom: 4px;
        }

        .warning-content p {
          margin: 0;
          color: var(--text-secondary);
          font-size: 13px;
        }

        .warning-content p + p {
          margin-top: 4px;
        }
      `}</style>
    </div>
  );
}

export default function App() {
  const [endpoint, setEndpoint] = useState(DEFAULT_ZENOH_ENDPOINT);

  const zenohConfig = useMemo(() => ({ endpoint }), [endpoint]);
  const { session, status, error, reconnect } = useZenohSession(zenohConfig);
  const { topics: availableTopics } = useZenohTopicDiscovery(session, '**');

  return (
    <div className="app">
      <header className="app-header">
        <div className="header-left">
          <h1>Bubbaloop</h1>
          <span className="header-subtitle">Camera Dashboard</span>
        </div>
        <StatusIndicator status={status} />
      </header>

      <BrowserCheck />

      <ConnectionPanel
        endpoint={endpoint}
        onEndpointChange={setEndpoint}
        status={status}
        error={error}
        onReconnect={reconnect}
      />

      {session ? (
        <Dashboard session={session} cameras={DEFAULT_CAMERAS} availableTopics={availableTopics} />
      ) : (
        <div className="connecting-placeholder">
          <div className="placeholder-content">
            {status === 'connecting' ? (
              <>
                <div className="spinner" />
                <p>Connecting to Zenoh...</p>
              </>
            ) : status === 'error' ? (
              <>
                <span className="error-icon">❌</span>
                <p>Failed to connect</p>
                <p className="hint">Check that zenohd is running with the remote-api plugin</p>
              </>
            ) : (
              <>
                <span className="info-icon">ℹ️</span>
                <p>Enter Zenoh endpoint to connect</p>
              </>
            )}
          </div>
        </div>
      )}

      <style>{`
        .app {
          min-height: 100vh;
          display: flex;
          flex-direction: column;
          background:
            radial-gradient(ellipse at top left, rgba(61, 90, 254, 0.05) 0%, transparent 50%),
            radial-gradient(ellipse at bottom right, rgba(0, 229, 255, 0.03) 0%, transparent 50%),
            var(--bg-primary);
        }

        .app-header {
          display: flex;
          justify-content: space-between;
          align-items: center;
          padding: 16px 24px;
          background: var(--bg-secondary);
          border-bottom: 1px solid var(--border-color);
        }

        .header-left {
          display: flex;
          align-items: baseline;
          gap: 12px;
        }

        .app-header h1 {
          font-size: 20px;
          font-weight: 700;
          background: var(--accent-gradient);
          -webkit-background-clip: text;
          -webkit-text-fill-color: transparent;
          background-clip: text;
        }

        .header-subtitle {
          font-size: 13px;
          color: var(--text-muted);
          font-weight: 400;
        }

        .connecting-placeholder {
          flex: 1;
          display: flex;
          align-items: center;
          justify-content: center;
        }

        .placeholder-content {
          text-align: center;
          color: var(--text-secondary);
        }

        .placeholder-content p {
          margin: 8px 0;
        }

        .placeholder-content .hint {
          font-size: 13px;
          color: var(--text-muted);
        }

        .spinner {
          width: 32px;
          height: 32px;
          border: 3px solid var(--border-color);
          border-top-color: var(--accent-primary);
          border-radius: 50%;
          margin: 0 auto 16px;
          animation: spin 1s linear infinite;
        }

        .error-icon, .info-icon {
          font-size: 32px;
          display: block;
          margin-bottom: 8px;
        }

        @keyframes spin {
          to { transform: rotate(360deg); }
        }
      `}</style>
    </div>
  );
}
