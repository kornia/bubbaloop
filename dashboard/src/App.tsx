import { useMemo, useState } from 'react';
import { useZenohSession, useZenohTopicDiscovery, ConnectionStatus } from './lib/zenoh';
import { ZenohSubscriptionProvider } from './contexts/ZenohSubscriptionContext';
import { FleetProvider } from './contexts/FleetContext';
import { Dashboard } from './components/Dashboard';
import { FleetBar } from './components/FleetBar';
import { MeshView } from './components/MeshView';
import { H264Decoder } from './lib/h264-decoder';

type AppView = 'dashboard' | 'loop';

// Zenoh endpoint - proxied through Vite on /zenoh path
// This allows single-port HTTPS access (WebSocket tunneled through same connection)
function getZenohEndpoint(): string {
  const protocol = window.location.protocol === 'https:' ? 'wss' : 'ws';
  return `${protocol}://${window.location.host}/zenoh`;
}

const ZENOH_ENDPOINT = getZenohEndpoint();

// Default camera configuration - modify these to match your setup
// ros-z key format: {domain_id}/{topic_with_%_encoding}/{type_info}
// Topic paths in ros-z use % encoding: /camera/entrance/compressed -> camera%entrance%compressed
// Use wildcard pattern to match the type_info suffix
// Note: CameraView automatically adds a glob prefix (*) to match machine-specific prefixes
// (e.g., bubbaloop%local%nvidia_orin00%camera%terrace%compressed)
const DEFAULT_CAMERAS = [
  { name: 'entrance', topic: '0/camera%entrance%compressed/**' },
  { name: 'terrace', topic: '0/camera%terrace%compressed/**' },
];

function StatusIndicator({ status, endpoint, onReconnect }: { status: ConnectionStatus; endpoint: string; onReconnect: () => void }) {
  const statusConfig = {
    disconnected: { color: 'var(--text-muted)', label: 'Disconnected' },
    connecting: { color: 'var(--warning)', label: 'Connecting...' },
    connected: { color: 'var(--success)', label: 'Connected' },
    error: { color: 'var(--error)', label: 'Error' },
  };

  const config = statusConfig[status];

  return (
    <div className="status-indicator">
      <span className="status-endpoint" title={endpoint}>{endpoint}</span>
      <span className="status-dot" style={{ backgroundColor: config.color }} />
      <span className="status-label">{config.label}</span>
      {(status === 'error' || status === 'disconnected') && (
        <button className="reconnect-btn" onClick={onReconnect} title="Reconnect">
          ↻
        </button>
      )}

      <style>{`
        .status-indicator {
          display: flex;
          align-items: center;
          gap: 10px;
        }

        .status-endpoint {
          font-size: 12px;
          font-family: 'JetBrains Mono', monospace;
          color: var(--text-muted);
          padding: 4px 8px;
          background: var(--bg-tertiary);
          border-radius: 4px;
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

        .reconnect-btn {
          background: none;
          border: 1px solid var(--border-color);
          border-radius: 4px;
          color: var(--text-secondary);
          cursor: pointer;
          font-size: 14px;
          padding: 2px 8px;
          transition: all 0.2s;
        }

        .reconnect-btn:hover {
          background: var(--bg-tertiary);
          color: var(--text-primary);
          border-color: var(--accent-primary);
        }

        @keyframes pulse {
          0%, 100% { opacity: 1; }
          50% { opacity: 0.4; }
        }

        /* Mobile responsive styles */
        @media (max-width: 768px) {
          .status-indicator {
            gap: 6px;
          }

          .status-endpoint {
            display: none;
          }

          .status-label {
            font-size: 11px;
          }

          .reconnect-btn {
            padding: 4px 10px;
          }
        }
      `}</style>
    </div>
  );
}


function BrowserCheck() {
  const isSupported = H264Decoder.isSupported();
  const isSecureContext = window.isSecureContext;

  // WebCodecs requires secure context (localhost or https)
  if (isSupported && isSecureContext) return null;

  // Determine the specific issue
  const needsSecureContext = !isSecureContext;

  return (
    <div className="browser-check">
      <div className="warning-content">
        <span className="warning-icon">⚠️</span>
        <div>
          <strong>WebCodecs not supported</strong>
          {needsSecureContext ? (
            <>
              <p>WebCodecs requires a <strong>secure context</strong> (localhost or HTTPS).</p>
              <p>Access via <code>https://{window.location.host}</code> or <code>http://localhost:5173</code></p>
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
  const zenohConfig = useMemo(() => ({ endpoint: ZENOH_ENDPOINT }), []);
  const { session, status, error, reconnect } = useZenohSession(zenohConfig);
  const { topics: availableTopics } = useZenohTopicDiscovery(session, '**');
  const [currentView, setCurrentView] = useState<AppView>('dashboard');

  return (
    <div className="app">
      <header className="app-header">
        <div className="header-left">
          <h1>Bubbaloop</h1>
          <div className="view-switcher">
            <button
              className={`view-tab ${currentView === 'dashboard' ? 'active' : ''}`}
              onClick={() => setCurrentView('dashboard')}
            >
              <svg width="14" height="14" viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="2">
                <rect x="3" y="3" width="7" height="7" rx="1" />
                <rect x="14" y="3" width="7" height="7" rx="1" />
                <rect x="3" y="14" width="7" height="7" rx="1" />
                <rect x="14" y="14" width="7" height="7" rx="1" />
              </svg>
              Dashboard
            </button>
            <button
              className={`view-tab ${currentView === 'loop' ? 'active' : ''}`}
              onClick={() => setCurrentView('loop')}
            >
              <svg width="14" height="14" viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="2">
                <circle cx="12" cy="12" r="3" />
                <circle cx="5" cy="6" r="2" />
                <circle cx="19" cy="6" r="2" />
                <circle cx="5" cy="18" r="2" />
                <circle cx="19" cy="18" r="2" />
                <line x1="9.5" y1="10.5" x2="6.5" y2="7.5" />
                <line x1="14.5" y1="10.5" x2="17.5" y2="7.5" />
                <line x1="9.5" y1="13.5" x2="6.5" y2="16.5" />
                <line x1="14.5" y1="13.5" x2="17.5" y2="16.5" />
              </svg>
              The Loop
            </button>
          </div>
        </div>
        <StatusIndicator status={status} endpoint={ZENOH_ENDPOINT} onReconnect={reconnect} />
      </header>

      <BrowserCheck />

      {error && (
        <div className="error-banner">
          <span>⚠️</span> {error.message}
        </div>
      )}

      {session ? (
        <FleetProvider>
          <ZenohSubscriptionProvider session={session}>
            <FleetBar />
            {currentView === 'dashboard' ? (
              <Dashboard cameras={DEFAULT_CAMERAS} availableTopics={availableTopics} />
            ) : (
              <MeshView availableTopics={availableTopics} zenohEndpoint={ZENOH_ENDPOINT} connectionStatus={status} />
            )}
          </ZenohSubscriptionProvider>
        </FleetProvider>
      ) : (
        <div className="connecting-placeholder">
          <div className="placeholder-content">
            {status === 'connecting' ? (
              <>
                <div className="spinner" />
                <p>Connecting to Zenoh...</p>
                <p className="hint">{ZENOH_ENDPOINT}</p>
              </>
            ) : status === 'error' ? (
              <>
                <span className="error-icon">❌</span>
                <p>Failed to connect to {ZENOH_ENDPOINT}</p>
                <p className="hint">Check that zenoh-bridge-remote-api is running (proxied via Vite)</p>
              </>
            ) : (
              <>
                <div className="spinner" />
                <p>Initializing...</p>
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
          align-items: center;
          gap: 16px;
        }

        .app-header h1 {
          font-size: 20px;
          font-weight: 700;
          background: var(--accent-gradient);
          -webkit-background-clip: text;
          -webkit-text-fill-color: transparent;
          background-clip: text;
        }

        .view-switcher {
          display: flex;
          gap: 2px;
          background: var(--bg-primary);
          padding: 3px;
          border-radius: 10px;
          border: 1px solid var(--border-color);
        }

        .view-tab {
          display: flex;
          align-items: center;
          gap: 6px;
          padding: 5px 14px;
          border: none;
          border-radius: 7px;
          background: transparent;
          color: var(--text-muted);
          font-size: 12px;
          font-weight: 500;
          font-family: inherit;
          cursor: pointer;
          transition: all 0.2s;
          white-space: nowrap;
        }

        .view-tab:hover {
          color: var(--text-secondary);
          background: rgba(255,255,255,0.03);
        }

        .view-tab.active {
          background: var(--bg-tertiary);
          color: var(--text-primary);
          box-shadow: 0 1px 4px rgba(0,0,0,0.2);
        }

        .view-tab.active svg {
          stroke: var(--accent-secondary);
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

        .error-banner {
          padding: 10px 24px;
          background: rgba(255, 23, 68, 0.1);
          border-bottom: 1px solid var(--error);
          color: var(--error);
          font-size: 13px;
          display: flex;
          align-items: center;
          gap: 8px;
        }

        @keyframes spin {
          to { transform: rotate(360deg); }
        }

        /* Mobile responsive styles */
        @media (max-width: 768px) {
          .app-header {
            padding: 12px 16px;
            flex-wrap: wrap;
            gap: 8px;
          }

          .header-left {
            gap: 8px;
          }

          .app-header h1 {
            font-size: 16px;
          }

          .view-tab {
            padding: 5px 10px;
            font-size: 11px;
            gap: 4px;
          }

          .view-tab svg {
            width: 12px;
            height: 12px;
          }

          .error-banner {
            padding: 8px 16px;
            font-size: 12px;
          }
        }

        @media (max-width: 480px) {
          .app-header {
            padding: 10px 12px;
          }

          .app-header h1 {
            font-size: 14px;
          }

          .view-switcher {
            padding: 2px;
          }

          .view-tab {
            padding: 4px 8px;
            font-size: 10px;
          }

          .view-tab svg {
            display: none;
          }
        }
      `}</style>
    </div>
  );
}
