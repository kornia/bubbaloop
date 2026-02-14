import { useState, useEffect, useCallback, useMemo } from "react";
import { useZenohSubscriptionContext } from "../contexts/ZenohSubscriptionContext";
import { useFleetContext } from "../contexts/FleetContext";
import {
  useNodeDiscovery,
  type DiscoveredNode,
} from "../contexts/NodeDiscoveryContext";
import {
  NodeCommandProto,
  CommandResultProto,
  CommandType,
} from "../proto/daemon";
import { Duration } from "typed-duration";
import { Reply, ReplyError } from "@eclipse-zenoh/zenoh-ts";

// Drag handle props type
interface DragHandleProps {
  [key: string]: unknown;
}

interface NodesViewPanelProps {
  onRemove?: () => void;
  dragHandleProps?: DragHandleProps;
}

const STATUS_CONFIG: Record<
  string,
  { color: string; icon: string; label: string }
> = {
  running: { color: "#00c853", icon: "\u25CF", label: "Running" },
  stopped: { color: "#9090a0", icon: "\u25CB", label: "Stopped" },
  failed: { color: "#ff1744", icon: "\u2715", label: "Failed" },
  building: { color: "#ffd600", icon: "\u25D0", label: "Building" },
  installing: { color: "#ffd600", icon: "\u25D0", label: "Installing" },
  "not-installed": { color: "#606070", icon: "\u2212", label: "Not Installed" },
  unknown: { color: "#606070", icon: "?", label: "Unknown" },
};

// Map protobuf status number to string
export const STATUS_MAP: Record<number, DiscoveredNode["status"]> = {
  1: "stopped",
  2: "running",
  3: "failed",
  4: "installing",
  5: "building",
  6: "not-installed",
};

export function statusNumberToString(status: number): DiscoveredNode["status"] {
  return STATUS_MAP[status] ?? "unknown";
}

const DISCOVERY_VIA_CONFIG: Record<
  string,
  { color: string; label: string; title: string }
> = {
  daemon: { color: "#3d5afe", label: "D", title: "Discovered via daemon" },
  manifest: {
    color: "#ff9800",
    label: "M",
    title: "Discovered via manifest only (daemon offline)",
  },
  both: {
    color: "#00c853",
    label: "B",
    title: "Discovered via daemon + manifest",
  },
};

export function NodesViewPanel({
  onRemove,
  dragHandleProps,
}: NodesViewPanelProps) {
  const { getSession } = useZenohSubscriptionContext();
  const { nodes, loading, error, daemonConnected, manifestDiscoveryActive } =
    useNodeDiscovery();
  const [selectedNode, setSelectedNode] = useState<string | null>(null);
  const [actionLoading, setActionLoading] = useState<string | null>(null);
  const [message, setMessage] = useState<{
    text: string;
    type: "success" | "error";
  } | null>(null);

  // Execute command via Zenoh query
  const executeCommand = useCallback(
    async (nodeName: string, command: string) => {
      const session = getSession();
      if (!session) {
        setMessage({ text: "Not connected to Zenoh", type: "error" });
        return;
      }

      setActionLoading(`${nodeName}-${command}`);
      setMessage(null);

      try {
        // Map command string to enum
        const commandMap: Record<string, number> = {
          start: CommandType.COMMAND_TYPE_START,
          stop: CommandType.COMMAND_TYPE_STOP,
          restart: CommandType.COMMAND_TYPE_RESTART,
          install: CommandType.COMMAND_TYPE_INSTALL,
          uninstall: CommandType.COMMAND_TYPE_UNINSTALL,
          build: CommandType.COMMAND_TYPE_BUILD,
          clean: CommandType.COMMAND_TYPE_CLEAN,
          enable_autostart: CommandType.COMMAND_TYPE_ENABLE_AUTOSTART,
          disable_autostart: CommandType.COMMAND_TYPE_DISABLE_AUTOSTART,
          get_logs: CommandType.COMMAND_TYPE_GET_LOGS,
        };

        // Look up target node to route command to correct machine
        const targetNode = nodes.find((n) => n.name === nodeName);
        const commandKey = targetNode?.machine_id
          ? `bubbaloop/${targetNode.machine_id}/daemon/command`
          : "bubbaloop/daemon/command";

        const cmd = NodeCommandProto.create({
          command: commandMap[command] ?? CommandType.COMMAND_TYPE_START,
          nodeName: nodeName,
          nodePath: "",
          requestId: crypto.randomUUID(),
          targetMachine: targetNode?.machine_id || "",
        });

        const payload = NodeCommandProto.encode(cmd).finish();

        // Send query and wait for reply
        const receiver = await session.get(commandKey, {
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
                  setMessage({
                    text: "Reply error from daemon",
                    type: "error",
                  });
                  break;
                }
                sample = replyResult;
              } else {
                sample = replyItem;
              }

              // Extract payload from Sample
              const replyPayload = (
                sample as { payload: () => { toBytes: () => Uint8Array } }
              )
                ?.payload?.()
                ?.toBytes?.();
              if (replyPayload) {
                const result = CommandResultProto.decode(replyPayload);
                if (result.success) {
                  setMessage({
                    text: result.message || "Command executed",
                    type: "success",
                  });
                } else {
                  setMessage({
                    text: result.message || "Command failed",
                    type: "error",
                  });
                }
              }
            } catch (e) {
              console.error("[NodesView] Failed to decode reply:", e);
            }
            break; // Only process first reply
          }
        }

        if (!gotReply) {
          setMessage({ text: "No response from daemon", type: "error" });
        }
      } catch (err) {
        console.error("[NodesView] Command failed:", err);
        setMessage({
          text: `Failed: ${err instanceof Error ? err.message : "Unknown error"}`,
          type: "error",
        });
      } finally {
        setActionLoading(null);
        setTimeout(() => setMessage(null), 4000);
      }
    },
    [getSession, nodes],
  );

  // Fetch logs for a node via Zenoh query
  const fetchLogs = useCallback(
    async (nodeName: string): Promise<string> => {
      const session = getSession();
      if (!session) {
        throw new Error("Not connected to Zenoh");
      }

      // Look up target node to route logs request to correct machine
      const targetNode = nodes.find((n) => n.name === nodeName);
      const commandKey = targetNode?.machine_id
        ? `bubbaloop/${targetNode.machine_id}/daemon/command`
        : "bubbaloop/daemon/command";

      const cmd = NodeCommandProto.create({
        command: CommandType.COMMAND_TYPE_GET_LOGS,
        nodeName: nodeName,
        nodePath: "",
        requestId: crypto.randomUUID(),
        targetMachine: targetNode?.machine_id || "",
      });

      const payload = NodeCommandProto.encode(cmd).finish();

      const receiver = await session.get(commandKey, {
        payload: payload,
        timeout: Duration.milliseconds.of(10000),
      });

      if (receiver) {
        for await (const replyItem of receiver) {
          let sample: unknown;
          if (replyItem instanceof Reply) {
            const replyResult = replyItem.result();
            if (replyResult instanceof ReplyError) {
              throw new Error("Reply error from daemon");
            }
            sample = replyResult;
          } else {
            sample = replyItem;
          }

          const replyPayload = (
            sample as { payload: () => { toBytes: () => Uint8Array } }
          )
            ?.payload?.()
            ?.toBytes?.();
          if (replyPayload) {
            const result = CommandResultProto.decode(replyPayload);
            if (result.success) {
              return result.output || "No logs available";
            } else {
              throw new Error(result.message || "Failed to fetch logs");
            }
          }
          break;
        }
      }

      throw new Error("No response from daemon");
    },
    [getSession, nodes],
  );

  // Group nodes by machine for multi-machine rendering
  const machineGroups = useMemo(() => {
    const groups = new Map<
      string,
      {
        hostname: string;
        machineId: string;
        nodes: DiscoveredNode[];
        isOnline: boolean;
      }
    >();
    for (const node of nodes) {
      const mid = node.machine_id || "local";
      if (!groups.has(mid)) {
        groups.set(mid, {
          hostname: node.machine_hostname || "local",
          machineId: mid,
          nodes: [],
          isOnline: !node.stale,
        });
      }
      groups.get(mid)!.nodes.push(node);
    }
    return Array.from(groups.values());
  }, [nodes]);

  // Filter by selected machine from FleetBar
  const { selectedMachineId } = useFleetContext();

  const filteredMachineGroups = useMemo(() => {
    if (!selectedMachineId) return machineGroups;
    return machineGroups.filter((g) => g.machineId === selectedMachineId);
  }, [machineGroups, selectedMachineId]);

  const filteredNodes = useMemo(() => {
    if (!selectedMachineId) return nodes;
    return nodes.filter((n) => (n.machine_id || "local") === selectedMachineId);
  }, [nodes, selectedMachineId]);

  const [collapsedMachines, setCollapsedMachines] = useState<Set<string>>(
    new Set(),
  );

  const toggleMachineCollapse = useCallback((machineId: string) => {
    setCollapsedMachines((prev) => {
      const next = new Set(prev);
      if (next.has(machineId)) {
        next.delete(machineId);
      } else {
        next.add(machineId);
      }
      return next;
    });
  }, []);

  const selectedNodeData = selectedNode
    ? nodes.find((n) => n.name === selectedNode)
    : undefined;

  // Render a discovery source badge
  const renderDiscoveryBadge = (via: DiscoveredNode["discoveredVia"]) => {
    const cfg = DISCOVERY_VIA_CONFIG[via] || DISCOVERY_VIA_CONFIG.daemon;
    return (
      <span
        className="discovery-badge"
        title={cfg.title}
        style={{ color: cfg.color, borderColor: cfg.color }}
      >
        {cfg.label}
      </span>
    );
  };

  // Render a node row (shared between single-machine and multi-machine views)
  const renderNodeRow = (node: DiscoveredNode) => {
    const statusCfg = STATUS_CONFIG[node.status] || STATUS_CONFIG.unknown;
    const isSelected = selectedNode === node.name;
    const isBuilding =
      node.status === "building" || node.status === "installing";

    return (
      <div
        key={`${node.machine_id}-${node.name}`}
        className={`node-row ${isSelected ? "selected" : ""} ${node.stale ? "stale" : ""}`}
        onClick={() => setSelectedNode(isSelected ? null : node.name)}
      >
        <span className="col-status" style={{ color: statusCfg.color }}>
          {isBuilding ? (
            <span className="pulse">{statusCfg.icon}</span>
          ) : (
            statusCfg.icon
          )}
        </span>
        <span className="col-name">{node.name}</span>
        <span className="col-source">
          {renderDiscoveryBadge(node.discoveredVia)}
        </span>
        <span className="col-machine">{node.machine_hostname || "local"}</span>
        <span className="col-ip mono">{node.machine_ips?.[0] || ""}</span>
        <span className="col-version">{node.version}</span>
        <span className={`col-type type-${node.node_type}`}>
          {node.node_type}
        </span>
      </div>
    );
  };

  return (
    <div className="nodes-panel">
      {/* Header */}
      <div className="panel-header" {...dragHandleProps}>
        <div className="panel-title-section">
          <svg
            width="18"
            height="18"
            viewBox="0 0 24 24"
            fill="none"
            stroke="currentColor"
            strokeWidth="2"
          >
            <rect x="4" y="4" width="16" height="16" rx="2" />
            <path d="M9 9h6M9 13h6M9 17h4" />
          </svg>
          <span className="panel-title">Nodes</span>
          <span
            className={`daemon-status ${daemonConnected ? "connected" : "disconnected"}`}
          >
            {daemonConnected ? "\u25CF zenoh" : "\u25CB offline"}
          </span>
          {manifestDiscoveryActive && (
            <span
              className="manifest-discovery-status"
              title="Manifest discovery in progress"
            >
              scanning...
            </span>
          )}
        </div>
        <div className="panel-actions">
          {onRemove && (
            <button
              className="remove-btn"
              onClick={onRemove}
              title="Remove panel"
            >
              <svg
                width="14"
                height="14"
                viewBox="0 0 24 24"
                fill="none"
                stroke="currentColor"
                strokeWidth="2"
              >
                <path d="M18 6L6 18M6 6l12 12" />
              </svg>
            </button>
          )}
        </div>
      </div>

      {/* Message banner */}
      {message && (
        <div className={`message-banner ${message.type}`}>{message.text}</div>
      )}

      {/* Content */}
      <div className="panel-content">
        {loading && nodes.length === 0 ? (
          <div className="loading-state">
            <div className="spinner" />
            <span>Discovering nodes via Zenoh...</span>
          </div>
        ) : error && nodes.length === 0 ? (
          <div className="error-state">
            <svg
              width="32"
              height="32"
              viewBox="0 0 24 24"
              fill="none"
              stroke="currentColor"
              strokeWidth="1.5"
            >
              <circle cx="12" cy="12" r="10" />
              <path d="M12 8v4M12 16h.01" />
            </svg>
            <span>{error}</span>
            <p className="error-hint">
              Make sure bubbaloop-daemon is running and publishing to Zenoh
            </p>
          </div>
        ) : (
          <div className="nodes-layout">
            {/* Node list */}
            <div className="nodes-list">
              <div className="list-header">
                <span className="col-status">St</span>
                <span className="col-name">Name</span>
                <span className="col-source">Src</span>
                <span className="col-machine">Machine</span>
                <span className="col-ip">IP</span>
                <span className="col-version">Version</span>
                <span className="col-type">Type</span>
              </div>
              {filteredNodes.length === 0 ? (
                <div className="no-nodes">No nodes registered</div>
              ) : filteredMachineGroups.length <= 1 ? (
                /* Single machine -- flat rendering */
                filteredNodes.map(renderNodeRow)
              ) : (
                /* Multiple machines -- grouped rendering with collapsible headers */
                filteredMachineGroups.map((group) => {
                  const isCollapsed = collapsedMachines.has(group.machineId);
                  const runningCount = group.nodes.filter(
                    (n) => n.status === "running",
                  ).length;

                  return (
                    <div key={`machine-${group.machineId}`}>
                      <div
                        className="machine-group-header"
                        onClick={() => toggleMachineCollapse(group.machineId)}
                      >
                        <span
                          className={`collapse-arrow ${isCollapsed ? "collapsed" : ""}`}
                        >
                          &#9660;
                        </span>
                        <span
                          className={`machine-status-dot ${group.isOnline ? "online" : "offline"}`}
                        />
                        <span>{group.hostname}</span>
                        <span className="machine-ip">
                          {group.nodes[0]?.machine_ips?.[0] || ""}
                        </span>
                        <span className="machine-node-count">
                          {runningCount}/{group.nodes.length} running
                        </span>
                      </div>
                      {!isCollapsed && group.nodes.map(renderNodeRow)}
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

        .manifest-discovery-status {
          font-size: 10px;
          color: var(--text-muted);
          font-style: italic;
          animation: pulse 1.5s ease-in-out infinite;
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

        .list-header .col-ip {
          font-family: inherit;
          font-size: inherit;
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

        .node-row.stale {
          opacity: 0.6;
        }

        .col-status { width: 30px; text-align: center; }
        .col-name { flex: 1; color: var(--text-primary); font-weight: 500; }
        .col-source { width: 36px; text-align: center; }
        .col-machine { width: 100px; color: var(--text-muted); font-size: 11px; }
        .col-ip {
          flex: 1.2;
          font-family: 'JetBrains Mono', monospace;
          font-size: 11px;
          color: var(--text-muted);
          overflow: hidden;
          text-overflow: ellipsis;
          white-space: nowrap;
        }
        .col-version { width: 70px; color: var(--accent-secondary); font-family: 'JetBrains Mono', monospace; }
        .col-type { width: 60px; text-transform: uppercase; font-size: 10px; font-weight: 600; }

        .col-type.type-rust { color: #ffd600; }
        .col-type.type-python { color: #00e5ff; }

        .discovery-badge {
          display: inline-block;
          font-size: 9px;
          font-weight: 700;
          width: 16px;
          height: 16px;
          line-height: 16px;
          text-align: center;
          border-radius: 3px;
          border: 1px solid;
        }

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

        .machine-group-header {
          display: flex;
          align-items: center;
          padding: 8px 16px;
          background: var(--bg-tertiary);
          cursor: pointer;
          user-select: none;
          gap: 10px;
          font-size: 12px;
          font-weight: 600;
          color: var(--text-secondary);
          border-bottom: 1px solid var(--border-color);
        }

        .machine-group-header:hover {
          background: rgba(61, 90, 254, 0.05);
        }

        .machine-status-dot {
          width: 6px;
          height: 6px;
          border-radius: 50%;
        }

        .machine-status-dot.online {
          background: var(--success);
        }

        .machine-status-dot.offline {
          background: var(--error);
        }

        .machine-ip {
          font-size: 11px;
          font-family: 'JetBrains Mono', monospace;
          color: var(--text-muted);
        }

        .machine-node-count {
          color: var(--text-muted);
          font-weight: 400;
          margin-left: auto;
        }

        .collapse-arrow {
          transition: transform 0.15s;
          color: var(--text-muted);
          font-size: 10px;
        }

        .collapse-arrow.collapsed {
          transform: rotate(-90deg);
        }

        @media (max-width: 768px) {
          .nodes-panel {
            min-height: 300px;
          }

          .col-source, .col-machine, .col-version, .col-type {
            display: none;
          }
        }
      `}</style>
    </div>
  );
}

// Node detail sub-component
interface NodeDetailProps {
  node: DiscoveredNode;
  onCommand: (nodeName: string, command: string) => void;
  onFetchLogs: (nodeName: string) => Promise<string>;
  actionLoading: string | null;
  onClose: () => void;
}

function NodeDetail({
  node,
  onCommand,
  onFetchLogs,
  actionLoading,
  onClose,
}: NodeDetailProps) {
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
      setLogs(
        `Failed to fetch logs: ${e instanceof Error ? e.message : "Unknown error"}`,
      );
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

  const discoveryVia =
    DISCOVERY_VIA_CONFIG[node.discoveredVia] || DISCOVERY_VIA_CONFIG.daemon;

  return (
    <div className="node-detail">
      <div className="detail-header">
        <div className="detail-title">
          <button className="back-btn" onClick={onClose} title="Back to list">
            <svg
              width="16"
              height="16"
              viewBox="0 0 24 24"
              fill="none"
              stroke="currentColor"
              strokeWidth="2"
            >
              <path d="M19 12H5M12 19l-7-7 7-7" />
            </svg>
          </button>
          <h3>{node.name}</h3>
          <span
            className="discovery-via-badge"
            style={{
              color: discoveryVia.color,
              borderColor: discoveryVia.color,
            }}
            title={discoveryVia.title}
          >
            {discoveryVia.label === "D"
              ? "daemon"
              : discoveryVia.label === "M"
                ? "manifest"
                : "daemon+manifest"}
          </span>
        </div>
        <span
          className="status-badge"
          style={{ color: statusCfg.color, borderColor: statusCfg.color }}
        >
          {statusCfg.icon} {statusCfg.label}
        </span>
      </div>

      <div className="detail-info">
        <div className="info-row">
          <span className="label">Type</span>
          <span className={`value type-${node.node_type}`}>
            {node.node_type}
          </span>
        </div>
        <div className="info-row">
          <span className="label">Version</span>
          <span className="value mono">{node.version}</span>
        </div>
        <div className="info-row">
          <span className="label">Machine</span>
          <span className="value">{node.machine_hostname || "local"}</span>
        </div>
        {node.machine_ips && node.machine_ips.length > 0 && (
          <div className="info-row full">
            <span className="label">IPs</span>
            <span className="value mono">{node.machine_ips.join(", ")}</span>
          </div>
        )}
        {node.description && (
          <div className="info-row full">
            <span className="label">Description</span>
            <span className="value">{node.description}</span>
          </div>
        )}
        {node.path && (
          <div className="info-row full">
            <span className="label">Path</span>
            <span className="value mono path">{node.path}</span>
          </div>
        )}
      </div>

      {/* Manifest details (when available) */}
      {node.manifest && (
        <div className="manifest-section">
          <div className="manifest-header">Manifest Details</div>
          <div className="manifest-grid">
            {node.manifest.capabilities.length > 0 && (
              <div className="info-row full">
                <span className="label">Capabilities</span>
                <div className="tag-list">
                  {node.manifest.capabilities.map((cap) => (
                    <span key={cap} className="tag capability">
                      {cap}
                    </span>
                  ))}
                </div>
              </div>
            )}
            {node.manifest.requires_hardware.length > 0 && (
              <div className="info-row full">
                <span className="label">Hardware Requirements</span>
                <div className="tag-list">
                  {node.manifest.requires_hardware.map((hw) => (
                    <span key={hw} className="tag hardware">
                      {hw}
                    </span>
                  ))}
                </div>
              </div>
            )}
            {node.manifest.publishes.length > 0 && (
              <div className="info-row full">
                <span className="label">Publishes</span>
                <div className="publish-list">
                  {node.manifest.publishes.map((pub) => (
                    <div key={pub.topic_suffix} className="publish-item">
                      <span className="mono">{pub.topic_suffix}</span>
                      {pub.rate_hz > 0 && (
                        <span className="rate">{pub.rate_hz} Hz</span>
                      )}
                    </div>
                  ))}
                </div>
              </div>
            )}
            {node.manifest.subscribes.length > 0 && (
              <div className="info-row full">
                <span className="label">Subscribes</span>
                <div className="tag-list">
                  {node.manifest.subscribes.map((sub) => (
                    <span key={sub} className="tag subscribe mono">
                      {sub}
                    </span>
                  ))}
                </div>
              </div>
            )}
            {node.manifest.scope && (
              <div className="info-row">
                <span className="label">Scope</span>
                <span className="value">{node.manifest.scope}</span>
              </div>
            )}
            {node.manifest.security.data_classification && (
              <div className="info-row">
                <span className="label">Data Classification</span>
                <span className="value">
                  {node.manifest.security.data_classification}
                </span>
              </div>
            )}
            {node.manifest.time.clock_source && (
              <div className="info-row">
                <span className="label">Clock Source</span>
                <span className="value">{node.manifest.time.clock_source}</span>
              </div>
            )}
          </div>
        </div>
      )}

      {/* Controls row: start/stop, logs toggle, auto-refresh */}
      <div className="controls-row">
        <div className="control-group">
          {node.discoveredVia === "manifest" ? (
            <span className="hint-text">
              Discovered via manifest only (daemon offline)
            </span>
          ) : node.status === "not-installed" ? (
            <span className="hint-text">
              Service not installed (use TUI to install)
            </span>
          ) : node.status === "running" ? (
            <button
              className="action-btn stop"
              onClick={() => onCommand(node.name, "stop")}
              disabled={isLoading("stop")}
            >
              {isLoading("stop") ? "Stopping..." : "Stop"}
            </button>
          ) : node.is_built ? (
            <button
              className="action-btn primary"
              onClick={() => onCommand(node.name, "start")}
              disabled={isLoading("start") || node.status === "building"}
            >
              {isLoading("start") ? "Starting..." : "Start"}
            </button>
          ) : (
            <span className="hint-text">Not built (use TUI to build)</span>
          )}
        </div>

        {node.discoveredVia !== "manifest" && (
          <div className="control-group">
            <button
              className={`action-btn ${showLogs ? "active" : "secondary"}`}
              onClick={() => setShowLogs(!showLogs)}
            >
              {showLogs ? "Hide Logs" : "Show Logs"}
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
        )}
      </div>

      {node.discoveredVia !== "manifest" && (
        <div className="logs-section">
          {/* Service logs */}
          {showLogs && (
            <div className="service-logs">
              <div className="output-content">
                {logs === null ? (
                  <div className="logs-loading">Loading logs...</div>
                ) : logs ? (
                  logs.split("\n").map((line, i) => (
                    <div key={i} className="output-line">
                      {line}
                    </div>
                  ))
                ) : (
                  <div className="no-logs">No logs available</div>
                )}
              </div>
            </div>
          )}
        </div>
      )}

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

        .discovery-via-badge {
          font-size: 10px;
          font-weight: 500;
          padding: 2px 6px;
          border-radius: 4px;
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

        .manifest-section {
          margin-bottom: 16px;
          border: 1px solid var(--border-color);
          border-radius: 8px;
          overflow: hidden;
        }

        .manifest-header {
          padding: 8px 12px;
          background: var(--bg-tertiary);
          font-size: 11px;
          font-weight: 600;
          color: var(--text-muted);
          text-transform: uppercase;
          letter-spacing: 0.5px;
        }

        .manifest-grid {
          display: grid;
          grid-template-columns: repeat(auto-fit, minmax(150px, 1fr));
          gap: 8px;
          padding: 12px;
        }

        .tag-list {
          display: flex;
          flex-wrap: wrap;
          gap: 4px;
        }

        .tag {
          font-size: 11px;
          padding: 2px 8px;
          border-radius: 4px;
          font-weight: 500;
        }

        .tag.capability {
          background: rgba(61, 90, 254, 0.1);
          color: #3d5afe;
        }

        .tag.hardware {
          background: rgba(255, 152, 0, 0.1);
          color: #ff9800;
        }

        .tag.subscribe {
          background: rgba(0, 229, 255, 0.1);
          color: #00e5ff;
          font-size: 10px;
        }

        .publish-list {
          display: flex;
          flex-direction: column;
          gap: 4px;
        }

        .publish-item {
          display: flex;
          align-items: center;
          gap: 8px;
          font-size: 12px;
          color: var(--text-secondary);
        }

        .publish-item .mono {
          font-family: 'JetBrains Mono', monospace;
          font-size: 11px;
        }

        .publish-item .rate {
          font-size: 10px;
          color: var(--text-muted);
          background: var(--bg-tertiary);
          padding: 1px 6px;
          border-radius: 4px;
        }

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
