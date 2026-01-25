import { useState, useEffect, useCallback, useRef } from 'react';
import { useZenohSubscriptionContext } from '../contexts/ZenohSubscriptionContext';
import { decodeNodeList } from '../proto/daemon';
import { getSamplePayload } from '../lib/zenoh';
import { Duration } from 'typed-duration';
import { Reply, ReplyError } from '@eclipse-zenoh/zenoh-ts';

// Node state from protobuf
interface NodeState {
  name: string;
  path: string;
  status: 'unknown' | 'stopped' | 'running' | 'failed' | 'installing' | 'building' | 'not-installed';
  installed: boolean;
  autostart_enabled: boolean;
  version: string;
  description: string;
  node_type: string;
  is_built: boolean;
  build_output: string[];
}

// Drag handle props type
interface DragHandleProps {
  [key: string]: unknown;
}

interface NodesViewPanelProps {
  onRemove?: () => void;
  dragHandleProps?: DragHandleProps;
}

const STATUS_CONFIG: Record<string, { color: string; icon: string; label: string }> = {
  running: { color: '#00c853', icon: '●', label: 'Running' },
  stopped: { color: '#9090a0', icon: '○', label: 'Stopped' },
  failed: { color: '#ff1744', icon: '✕', label: 'Failed' },
  building: { color: '#ffd600', icon: '◐', label: 'Building' },
  installing: { color: '#ffd600', icon: '◐', label: 'Installing' },
  'not-installed': { color: '#606070', icon: '−', label: 'Not Installed' },
  unknown: { color: '#606070', icon: '?', label: 'Unknown' },
};

// Map protobuf status number to string
const STATUS_MAP: Record<number, NodeState['status']> = {
  1: 'stopped',
  2: 'running',
  3: 'failed',
  4: 'installing',
  5: 'building',
  6: 'not-installed',
};

function statusNumberToString(status: number): NodeState['status'] {
  return STATUS_MAP[status] ?? 'unknown';
}

export function NodesViewPanel({
  onRemove,
  dragHandleProps,
}: NodesViewPanelProps) {
  const { subscribe, unsubscribe, getSession } = useZenohSubscriptionContext();
  const [nodes, setNodes] = useState<NodeState[]>([]);
  const [loading, setLoading] = useState(true);
  const [error, setError] = useState<string | null>(null);
  const [selectedNode, setSelectedNode] = useState<string | null>(null);
  const [actionLoading, setActionLoading] = useState<string | null>(null);
  const [message, setMessage] = useState<{ text: string; type: 'success' | 'error' } | null>(null);
  const [daemonConnected, setDaemonConnected] = useState(false);
  const listenerIdRef = useRef<string | null>(null);

  // Subscribe to daemon nodes topic
  useEffect(() => {
    const topic = 'bubbaloop/daemon/nodes';

    const handleSample = (sample: { payload: () => { toBytes: () => Uint8Array } }) => {
      try {
        const payload = getSamplePayload(sample as Parameters<typeof getSamplePayload>[0]);
        const nodeList = decodeNodeList(payload);

        if (nodeList && nodeList.nodes.length > 0) {
          const mappedNodes: NodeState[] = nodeList.nodes.map(n => ({
            name: n.name,
            path: n.path,
            status: statusNumberToString(n.status),
            installed: n.installed,
            autostart_enabled: n.autostartEnabled,
            version: n.version,
            description: n.description,
            node_type: n.nodeType,
            is_built: n.isBuilt,
            build_output: n.buildOutput,
          }));
          setNodes(mappedNodes);
          setDaemonConnected(true);
          setError(null);
          setLoading(false);
        }
      } catch (err) {
        console.error('[NodesView] Failed to decode NodeList:', err);
      }
    };

    listenerIdRef.current = subscribe(topic, handleSample);

    // Set timeout for connection
    const timeout = setTimeout(() => {
      if (nodes.length === 0) {
        setLoading(false);
        setError('No data from daemon - check if bubbaloop-daemon is running');
      }
    }, 5000);

    return () => {
      clearTimeout(timeout);
      if (listenerIdRef.current) {
        unsubscribe(topic, listenerIdRef.current);
      }
    };
  }, [subscribe, unsubscribe, nodes.length]);

  // Execute command via Zenoh query
  const executeCommand = useCallback(async (nodeName: string, command: string) => {
    const session = getSession();
    if (!session) {
      setMessage({ text: 'Not connected to Zenoh', type: 'error' });
      return;
    }

    setActionLoading(`${nodeName}-${command}`);
    setMessage(null);

    try {
      // Import proto encoder dynamically
      const { bubbaloop } = await import('../proto/messages.pb.js');
      const NodeCommand = bubbaloop.daemon.v1.NodeCommand;
      const CommandType = bubbaloop.daemon.v1.CommandType;

      // Map command string to enum
      const commandMap: Record<string, number> = {
        'start': CommandType.COMMAND_TYPE_START,
        'stop': CommandType.COMMAND_TYPE_STOP,
        'restart': CommandType.COMMAND_TYPE_RESTART,
        'install': CommandType.COMMAND_TYPE_INSTALL,
        'uninstall': CommandType.COMMAND_TYPE_UNINSTALL,
        'build': CommandType.COMMAND_TYPE_BUILD,
        'clean': CommandType.COMMAND_TYPE_CLEAN,
        'enable_autostart': CommandType.COMMAND_TYPE_ENABLE_AUTOSTART,
        'disable_autostart': CommandType.COMMAND_TYPE_DISABLE_AUTOSTART,
        'get_logs': CommandType.COMMAND_TYPE_GET_LOGS,
      };

      const cmd = NodeCommand.create({
        command: commandMap[command] ?? CommandType.COMMAND_TYPE_START,
        nodeName: nodeName,
        nodePath: '',
        requestId: crypto.randomUUID(),
      });

      const payload = NodeCommand.encode(cmd).finish();

      // Send query and wait for reply
      const receiver = await session.get('bubbaloop/daemon/command', {
        payload: payload,
        timeout: Duration.milliseconds.of(10000),
      });

      // Process first reply
      let gotReply = false;
      if (receiver) {
        for await (const replyItem of receiver) {
          gotReply = true;
          try {
            // Reply can be a Reply object with result() method
            let sample: unknown;
            if (replyItem instanceof Reply) {
              const replyResult = replyItem.result();
              if (replyResult instanceof ReplyError) {
                setMessage({ text: 'Reply error from daemon', type: 'error' });
                break;
              }
              sample = replyResult;
            } else {
              sample = replyItem;
            }

            // Extract payload from Sample
            const replyPayload = (sample as { payload: () => { toBytes: () => Uint8Array } })?.payload?.()?.toBytes?.();
            if (replyPayload) {
              const CommandResult = bubbaloop.daemon.v1.CommandResult;
              const result = CommandResult.decode(replyPayload);
              if (result.success) {
                setMessage({ text: result.message || 'Command executed', type: 'success' });
              } else {
                setMessage({ text: result.message || 'Command failed', type: 'error' });
              }
            }
          } catch (e) {
            console.error('[NodesView] Failed to decode reply:', e);
          }
          break; // Only process first reply
        }
      }

      if (!gotReply) {
        setMessage({ text: 'No response from daemon', type: 'error' });
      }
    } catch (err) {
      console.error('[NodesView] Command failed:', err);
      setMessage({ text: `Failed: ${err instanceof Error ? err.message : 'Unknown error'}`, type: 'error' });
    } finally {
      setActionLoading(null);
      setTimeout(() => setMessage(null), 4000);
    }
  }, [getSession]);

  // Fetch logs for a node via Zenoh query
  const fetchLogs = useCallback(async (nodeName: string): Promise<string> => {
    const session = getSession();
    if (!session) {
      throw new Error('Not connected to Zenoh');
    }

    const { bubbaloop } = await import('../proto/messages.pb.js');
    const NodeCommand = bubbaloop.daemon.v1.NodeCommand;
    const CommandType = bubbaloop.daemon.v1.CommandType;

    const cmd = NodeCommand.create({
      command: CommandType.COMMAND_TYPE_GET_LOGS,
      nodeName: nodeName,
      nodePath: '',
      requestId: crypto.randomUUID(),
    });

    const payload = NodeCommand.encode(cmd).finish();

    const receiver = await session.get('bubbaloop/daemon/command', {
      payload: payload,
      timeout: Duration.milliseconds.of(10000),
    });

    if (receiver) {
      for await (const replyItem of receiver) {
        let sample: unknown;
        if (replyItem instanceof Reply) {
          const replyResult = replyItem.result();
          if (replyResult instanceof ReplyError) {
            throw new Error('Reply error from daemon');
          }
          sample = replyResult;
        } else {
          sample = replyItem;
        }

        const replyPayload = (sample as { payload: () => { toBytes: () => Uint8Array } })?.payload?.()?.toBytes?.();
        if (replyPayload) {
          const CommandResult = bubbaloop.daemon.v1.CommandResult;
          const result = CommandResult.decode(replyPayload);
          if (result.success) {
            return result.output || 'No logs available';
          } else {
            throw new Error(result.message || 'Failed to fetch logs');
          }
        }
        break;
      }
    }

    throw new Error('No response from daemon');
  }, [getSession]);

  const selectedNodeData = selectedNode ? nodes.find(n => n.name === selectedNode) : undefined;

  return (
    <div className="nodes-panel">
      {/* Header */}
      <div className="panel-header" {...dragHandleProps}>
        <div className="panel-title-section">
          <svg width="18" height="18" viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="2">
            <rect x="4" y="4" width="16" height="16" rx="2" />
            <path d="M9 9h6M9 13h6M9 17h4" />
          </svg>
          <span className="panel-title">Nodes</span>
          <span className={`daemon-status ${daemonConnected ? 'connected' : 'disconnected'}`}>
            {daemonConnected ? '● zenoh' : '○ offline'}
          </span>
        </div>
        <div className="panel-actions">
          {onRemove && (
            <button className="remove-btn" onClick={onRemove} title="Remove panel">
              <svg width="14" height="14" viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="2">
                <path d="M18 6L6 18M6 6l12 12" />
              </svg>
            </button>
          )}
        </div>
      </div>

      {/* Message banner */}
      {message && (
        <div className={`message-banner ${message.type}`}>
          {message.text}
        </div>
      )}

      {/* Content */}
      <div className="panel-content">
        {loading && nodes.length === 0 ? (
          <div className="loading-state">
            <div className="spinner" />
            <span>Connecting to daemon via Zenoh...</span>
          </div>
        ) : error && nodes.length === 0 ? (
          <div className="error-state">
            <svg width="32" height="32" viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="1.5">
              <circle cx="12" cy="12" r="10" />
              <path d="M12 8v4M12 16h.01" />
            </svg>
            <span>{error}</span>
            <p className="error-hint">Make sure bubbaloop-daemon is running and publishing to Zenoh</p>
          </div>
        ) : (
          <div className="nodes-layout">
            {/* Node list */}
            <div className="nodes-list">
              <div className="list-header">
                <span className="col-status">St</span>
                <span className="col-name">Name</span>
                <span className="col-version">Version</span>
                <span className="col-type">Type</span>
              </div>
              {nodes.length === 0 ? (
                <div className="no-nodes">No nodes registered</div>
              ) : (
                nodes.map(node => {
                  const statusCfg = STATUS_CONFIG[node.status] || STATUS_CONFIG.unknown;
                  const isSelected = selectedNode === node.name;
                  const isBuilding = node.status === 'building' || node.status === 'installing';

                  return (
                    <div
                      key={node.name}
                      className={`node-row ${isSelected ? 'selected' : ''}`}
                      onClick={() => setSelectedNode(isSelected ? null : node.name)}
                    >
                      <span className="col-status" style={{ color: statusCfg.color }}>
                        {isBuilding ? <span className="pulse">{statusCfg.icon}</span> : statusCfg.icon}
                      </span>
                      <span className="col-name">{node.name}</span>
                      <span className="col-version">{node.version}</span>
                      <span className={`col-type type-${node.node_type}`}>{node.node_type}</span>
                    </div>
                  );
                })
              )}
            </div>

            {/* Node detail */}
            {selectedNodeData && (
              <NodeDetail
                node={selectedNodeData}
                onCommand={executeCommand}
                onFetchLogs={fetchLogs}
                actionLoading={actionLoading}
                onClose={() => setSelectedNode(null)}
              />
            )}
          </div>
        )}
      </div>

      <style>{`
        .nodes-panel {
          background: var(--bg-card);
          border: 1px solid var(--border-color);
          border-radius: 12px;
          display: flex;
          flex-direction: column;
          min-height: 400px;
          max-height: 600px;
        }

        .panel-header {
          display: flex;
          align-items: center;
          justify-content: space-between;
          padding: 12px 16px;
          border-bottom: 1px solid var(--border-color);
          cursor: grab;
          user-select: none;
        }

        .panel-header:active {
          cursor: grabbing;
        }

        .panel-title-section {
          display: flex;
          align-items: center;
          gap: 10px;
          color: var(--text-secondary);
        }

        .panel-title {
          font-size: 14px;
          font-weight: 600;
          color: var(--text-primary);
        }

        .daemon-status {
          font-size: 11px;
          padding: 2px 8px;
          border-radius: 10px;
          font-weight: 500;
        }

        .daemon-status.connected {
          color: #00c853;
          background: rgba(0, 200, 83, 0.1);
        }

        .daemon-status.disconnected {
          color: #ff1744;
          background: rgba(255, 23, 68, 0.1);
        }

        .panel-actions {
          display: flex;
          gap: 4px;
        }

        .remove-btn {
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

        .remove-btn:hover {
          background: rgba(255, 23, 68, 0.1);
          color: #ff1744;
        }

        .message-banner {
          padding: 8px 16px;
          font-size: 12px;
          font-weight: 500;
        }

        .message-banner.success {
          background: rgba(0, 200, 83, 0.1);
          color: #00c853;
        }

        .message-banner.error {
          background: rgba(255, 23, 68, 0.1);
          color: #ff1744;
        }

        .panel-content {
          flex: 1;
          overflow: hidden;
          display: flex;
          flex-direction: column;
        }

        .loading-state, .error-state {
          flex: 1;
          display: flex;
          flex-direction: column;
          align-items: center;
          justify-content: center;
          gap: 12px;
          color: var(--text-muted);
        }

        .error-state svg {
          opacity: 0.5;
        }

        .error-hint {
          font-size: 12px;
          opacity: 0.7;
        }

        .spinner {
          width: 24px;
          height: 24px;
          border: 2px solid var(--border-color);
          border-top-color: var(--accent-primary);
          border-radius: 50%;
          animation: spin 0.8s linear infinite;
        }

        @keyframes spin {
          to { transform: rotate(360deg); }
        }

        .nodes-layout {
          flex: 1;
          display: flex;
          flex-direction: column;
          overflow: hidden;
        }

        .nodes-list {
          flex: 1;
          overflow-y: auto;
          font-size: 12px;
        }

        .list-header {
          display: flex;
          padding: 8px 16px;
          border-bottom: 1px solid var(--border-color);
          color: var(--text-muted);
          font-weight: 600;
          font-size: 11px;
          text-transform: uppercase;
          letter-spacing: 0.5px;
          position: sticky;
          top: 0;
          background: var(--bg-card);
          z-index: 1;
        }

        .node-row {
          display: flex;
          padding: 10px 16px;
          cursor: pointer;
          transition: background 0.15s;
          border-bottom: 1px solid var(--border-color);
        }

        .node-row:hover {
          background: var(--bg-tertiary);
        }

        .node-row.selected {
          background: rgba(61, 90, 254, 0.1);
          border-left: 2px solid var(--accent-primary);
          padding-left: 14px;
        }

        .col-status { width: 30px; text-align: center; }
        .col-name { flex: 1; color: var(--text-primary); font-weight: 500; }
        .col-version { width: 70px; color: var(--accent-secondary); font-family: 'JetBrains Mono', monospace; }
        .col-type { width: 60px; text-transform: uppercase; font-size: 10px; font-weight: 600; }

        .col-type.type-rust { color: #ffd600; }
        .col-type.type-python { color: #00e5ff; }

        .no-nodes {
          padding: 32px 16px;
          text-align: center;
          color: var(--text-muted);
        }

        .pulse {
          animation: pulse 1s ease-in-out infinite;
        }

        @keyframes pulse {
          0%, 100% { opacity: 1; }
          50% { opacity: 0.5; }
        }

        @media (max-width: 768px) {
          .nodes-panel {
            min-height: 300px;
          }

          .col-version, .col-type {
            display: none;
          }
        }
      `}</style>
    </div>
  );
}

// Node detail sub-component
interface NodeDetailProps {
  node: NodeState;
  onCommand: (nodeName: string, command: string) => void;
  onFetchLogs: (nodeName: string) => Promise<string>;
  actionLoading: string | null;
  onClose: () => void;
}

function NodeDetail({ node, onCommand, onFetchLogs, actionLoading, onClose }: NodeDetailProps) {
  const statusCfg = STATUS_CONFIG[node.status] || STATUS_CONFIG.unknown;
  const isLoading = (cmd: string) => actionLoading === `${node.name}-${cmd}`;
  const [logs, setLogs] = useState<string | null>(null);
  const [showLogs, setShowLogs] = useState(true); // Auto-show logs by default
  const [autoRefresh, setAutoRefresh] = useState(true); // Auto-refresh enabled by default

  const fetchLogsInternal = useCallback(async () => {
    try {
      const logsText = await onFetchLogs(node.name);
      setLogs(logsText);
    } catch (e) {
      setLogs(`Failed to fetch logs: ${e instanceof Error ? e.message : 'Unknown error'}`);
    }
  }, [onFetchLogs, node.name]);

  // Fetch logs immediately on mount
  useEffect(() => {
    fetchLogsInternal();
  }, [fetchLogsInternal]);

  // Auto-refresh logs every 3 seconds when showing
  useEffect(() => {
    if (!showLogs || !autoRefresh) return;
    const interval = setInterval(fetchLogsInternal, 3000);
    return () => clearInterval(interval);
  }, [showLogs, autoRefresh, fetchLogsInternal]);

  return (
    <div className="node-detail">
      <div className="detail-header">
        <div className="detail-title">
          <button className="back-btn" onClick={onClose} title="Back to list">
            <svg width="16" height="16" viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="2">
              <path d="M19 12H5M12 19l-7-7 7-7" />
            </svg>
          </button>
          <h3>{node.name}</h3>
        </div>
        <span className="status-badge" style={{ color: statusCfg.color, borderColor: statusCfg.color }}>
          {statusCfg.icon} {statusCfg.label}
        </span>
      </div>

      <div className="detail-info">
        <div className="info-row">
          <span className="label">Type</span>
          <span className={`value type-${node.node_type}`}>{node.node_type}</span>
        </div>
        <div className="info-row">
          <span className="label">Version</span>
          <span className="value mono">{node.version}</span>
        </div>
        {node.description && (
          <div className="info-row full">
            <span className="label">Description</span>
            <span className="value">{node.description}</span>
          </div>
        )}
        <div className="info-row full">
          <span className="label">Path</span>
          <span className="value mono path">{node.path}</span>
        </div>
      </div>

      {/* Controls row: start/stop, logs toggle, auto-refresh */}
      <div className="controls-row">
        <div className="control-group">
          {node.status === 'not-installed' ? (
            <span className="hint-text">Service not installed (use TUI to install)</span>
          ) : node.status === 'running' ? (
            <button
              className="action-btn stop"
              onClick={() => onCommand(node.name, 'stop')}
              disabled={isLoading('stop')}
            >
              {isLoading('stop') ? 'Stopping...' : 'Stop'}
            </button>
          ) : node.is_built ? (
            <button
              className="action-btn primary"
              onClick={() => onCommand(node.name, 'start')}
              disabled={isLoading('start') || node.status === 'building'}
            >
              {isLoading('start') ? 'Starting...' : 'Start'}
            </button>
          ) : (
            <span className="hint-text">Not built (use TUI to build)</span>
          )}
        </div>

        <div className="control-group">
          <button
            className={`action-btn ${showLogs ? 'active' : 'secondary'}`}
            onClick={() => setShowLogs(!showLogs)}
          >
            {showLogs ? 'Hide Logs' : 'Show Logs'}
          </button>
          {showLogs && (
            <label className="auto-refresh-switch">
              <input
                type="checkbox"
                checked={autoRefresh}
                onChange={() => setAutoRefresh(!autoRefresh)}
              />
              <span className="switch-slider"></span>
              <span className="switch-label">Auto</span>
            </label>
          )}
        </div>
      </div>

      <div className="logs-section">

        {/* Service logs */}
        {showLogs && (
          <div className="service-logs">
            <div className="output-content">
              {logs === null ? (
                <div className="logs-loading">Loading logs...</div>
              ) : logs ? (
                logs.split('\n').map((line, i) => (
                  <div key={i} className="output-line">{line}</div>
                ))
              ) : (
                <div className="no-logs">No logs available</div>
              )}
            </div>
          </div>
        )}
      </div>

      <style>{`
        .node-detail {
          border-top: 1px solid var(--border-color);
          padding: 16px;
          background: var(--bg-secondary);
        }

        .detail-header {
          display: flex;
          align-items: center;
          justify-content: space-between;
          margin-bottom: 16px;
        }

        .detail-title {
          display: flex;
          align-items: center;
          gap: 10px;
        }

        .detail-header h3 {
          margin: 0;
          font-size: 16px;
          font-weight: 600;
          color: var(--text-primary);
        }

        .back-btn {
          display: flex;
          align-items: center;
          justify-content: center;
          width: 32px;
          height: 32px;
          background: var(--bg-tertiary);
          border: 1px solid var(--border-color);
          border-radius: 6px;
          color: var(--text-secondary);
          cursor: pointer;
          transition: all 0.15s;
        }

        .back-btn:hover {
          background: var(--bg-card);
          color: var(--accent-primary);
          border-color: var(--accent-primary);
        }

        .status-badge {
          font-size: 11px;
          font-weight: 600;
          padding: 4px 10px;
          border-radius: 12px;
          border: 1px solid;
          background: transparent;
        }

        .detail-info {
          display: grid;
          grid-template-columns: repeat(auto-fit, minmax(150px, 1fr));
          gap: 8px;
          margin-bottom: 16px;
        }

        .info-row {
          display: flex;
          flex-direction: column;
          gap: 2px;
        }

        .info-row.full {
          grid-column: 1 / -1;
        }

        .info-row .label {
          font-size: 10px;
          font-weight: 600;
          color: var(--text-muted);
          text-transform: uppercase;
          letter-spacing: 0.5px;
        }

        .info-row .value {
          font-size: 13px;
          color: var(--text-secondary);
        }

        .info-row .value.mono {
          font-family: 'JetBrains Mono', monospace;
          font-size: 12px;
        }

        .info-row .value.path {
          word-break: break-all;
          font-size: 11px;
        }

        .info-row .value.type-rust { color: #ffd600; }
        .info-row .value.type-python { color: #00e5ff; }

        .controls-row {
          display: flex;
          align-items: center;
          justify-content: space-between;
          gap: 12px;
          flex-wrap: wrap;
        }

        .control-group {
          display: flex;
          align-items: center;
          gap: 10px;
        }

        .hint-text {
          font-size: 12px;
          color: var(--text-muted);
          font-style: italic;
        }

        .action-btn {
          padding: 6px 12px;
          border-radius: 6px;
          font-size: 12px;
          font-weight: 500;
          cursor: pointer;
          transition: all 0.15s;
          border: 1px solid transparent;
        }

        .action-btn:disabled {
          opacity: 0.5;
          cursor: not-allowed;
        }

        .action-btn.primary {
          background: var(--accent-primary);
          color: white;
          border-color: var(--accent-primary);
        }

        .action-btn.primary:hover:not(:disabled) {
          background: #5c7cff;
        }

        .action-btn.secondary {
          background: var(--bg-tertiary);
          color: var(--text-secondary);
          border-color: var(--border-color);
        }

        .action-btn.secondary:hover:not(:disabled) {
          background: var(--bg-card);
          color: var(--text-primary);
        }

        .action-btn.stop {
          background: var(--bg-tertiary);
          color: #ff9800;
          border-color: #ff9800;
        }

        .action-btn.stop:hover:not(:disabled) {
          background: rgba(255, 152, 0, 0.15);
        }

        .action-btn.active {
          background: var(--accent-primary);
          color: white;
          border-color: var(--accent-primary);
        }

        .action-btn.active:hover:not(:disabled) {
          background: #5c7cff;
        }

        .output-header {
          padding: 8px 12px;
          background: var(--bg-tertiary);
          font-size: 11px;
          font-weight: 600;
          color: var(--text-muted);
          text-transform: uppercase;
          letter-spacing: 0.5px;
        }

        .output-content {
          padding: 12px;
          background: var(--bg-primary);
          font-family: 'JetBrains Mono', monospace;
          font-size: 11px;
          color: var(--text-muted);
          max-height: 150px;
          overflow-y: auto;
        }

        .output-line {
          white-space: pre-wrap;
          word-break: break-all;
          line-height: 1.5;
        }

        .logs-section {
          margin-top: 12px;
        }

        .auto-refresh-switch {
          display: flex;
          align-items: center;
          gap: 8px;
          cursor: pointer;
          user-select: none;
        }

        .auto-refresh-switch input {
          display: none;
        }

        .switch-slider {
          width: 36px;
          height: 20px;
          background: var(--bg-tertiary);
          border-radius: 10px;
          position: relative;
          transition: background 0.2s;
          border: 1px solid var(--border-color);
        }

        .switch-slider::after {
          content: '';
          position: absolute;
          width: 14px;
          height: 14px;
          background: var(--text-muted);
          border-radius: 50%;
          top: 2px;
          left: 2px;
          transition: all 0.2s;
        }

        .auto-refresh-switch input:checked + .switch-slider {
          background: #00c853;
          border-color: #00c853;
        }

        .auto-refresh-switch input:checked + .switch-slider::after {
          left: 18px;
          background: white;
        }

        .switch-label {
          font-size: 12px;
          color: var(--text-muted);
        }

        .service-logs {
          border: 1px solid var(--border-color);
          border-radius: 8px;
          overflow: hidden;
        }

        .service-logs .output-content {
          max-height: 250px;
        }

        .logs-loading, .no-logs {
          color: var(--text-muted);
          font-style: italic;
        }

        @media (max-width: 768px) {
          .action-btn {
            padding: 10px 16px;
          }
        }
      `}</style>
    </div>
  );
}
