/**
 * NodeDiscoveryContext — Hybrid node discovery via daemon API + direct manifest queries.
 *
 * Two independent discovery loops:
 * 1. Manifest discovery: queries `bubbaloop/** /manifest` (every 10s, backs off to 30s)
 *    Each running node responds with its JSON manifest.
 * 2. Daemon polling: queries `bubbaloop/daemon/nodes` (every 3s) for protobuf NodeList.
 *
 * Results are merged: manifest provides rich metadata, daemon provides runtime state.
 * When daemon is down, nodes discovered via manifest still appear (status = 'unknown').
 */

import {
  createContext,
  useContext,
  useState,
  useEffect,
  useCallback,
  useRef,
  useMemo,
  type ReactNode,
} from "react";
import { Session, Reply, ReplyError, Sample } from "@eclipse-zenoh/zenoh-ts";
import { useFleetContext } from "./FleetContext";
import { decodeNodeList } from "../proto/daemon";
import { getSamplePayload } from "../lib/zenoh";
import { Duration } from "typed-duration";

// ---------------------------------------------------------------------------
// Types
// ---------------------------------------------------------------------------

export interface NodeManifest {
  name: string;
  version: string;
  language: string;
  description: string;
  machine_id: string;
  scope: string;
  capabilities: string[];
  requires_hardware: string[];
  publishes: Array<{
    topic_suffix: string;
    full_topic: string;
    rate_hz: number;
  }>;
  subscribes: string[];
  schema_key: string;
  health_key: string;
  config_key: string;
  security: {
    acl_prefix: string;
    data_classification: string;
  };
  time: {
    clock_source: string;
    timestamp_field: string;
    timestamp_unit: string;
  };
}

export interface DiscoveredNode {
  // Core identity
  name: string;
  machine_id: string;

  // From manifest (available when discovered via manifest)
  manifest?: NodeManifest;

  // From daemon (available when daemon is online)
  path: string;
  status:
    | "unknown"
    | "stopped"
    | "running"
    | "failed"
    | "installing"
    | "building"
    | "not-installed";
  installed: boolean;
  autostart_enabled: boolean;
  version: string;
  description: string;
  node_type: string;
  is_built: boolean;
  build_output: string[];
  machine_hostname: string;
  machine_ips: string[];
  base_node: string;

  // Discovery metadata
  discoveredVia: "manifest" | "daemon" | "both";
  stale: boolean;
  lastSeen: number;
}

export interface NodeDiscoveryContextValue {
  nodes: DiscoveredNode[];
  loading: boolean;
  error: string | null;
  daemonConnected: boolean;
  manifestDiscoveryActive: boolean;
  refresh: () => void;
}

const NodeDiscoveryContext = createContext<NodeDiscoveryContextValue>({
  nodes: [],
  loading: true,
  error: null,
  daemonConnected: false,
  manifestDiscoveryActive: false,
  refresh: () => {},
});

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

/** Map protobuf status number to string. */
function statusNumberToString(status: number): DiscoveredNode["status"] {
  const map: Record<number, DiscoveredNode["status"]> = {
    1: "stopped",
    2: "running",
    3: "failed",
    4: "installing",
    5: "building",
    6: "not-installed",
  };
  return map[status] ?? "unknown";
}

/** Unique key for a node across machines. */
function nodeKey(machineId: string, name: string): string {
  return `${machineId || "local"}::${name}`;
}

/** Create a default DiscoveredNode (manifest-only, no daemon info). */
function manifestOnlyNode(manifest: NodeManifest, now: number): DiscoveredNode {
  return {
    name: manifest.name,
    machine_id: manifest.machine_id || "local",
    manifest,
    path: "",
    status: "unknown",
    installed: false,
    autostart_enabled: false,
    version: manifest.version || "",
    description: manifest.description || "",
    node_type: manifest.language || "",
    is_built: false,
    build_output: [],
    machine_hostname: "",
    machine_ips: [],
    base_node: "",
    discoveredVia: "manifest",
    stale: false,
    lastSeen: now,
  };
}

/** Parse a JSON payload into a NodeManifest, returning null on failure. */
function parseManifest(data: Uint8Array): NodeManifest | null {
  try {
    const text = new TextDecoder().decode(data);
    const obj = JSON.parse(text);
    if (!obj || typeof obj !== "object" || !obj.name) return null;
    return {
      name: obj.name ?? "",
      version: obj.version ?? "",
      language: obj.language ?? "",
      description: obj.description ?? "",
      machine_id: obj.machine_id ?? "",
      scope: obj.scope ?? "",
      capabilities: Array.isArray(obj.capabilities) ? obj.capabilities : [],
      requires_hardware: Array.isArray(obj.requires_hardware)
        ? obj.requires_hardware
        : [],
      publishes: Array.isArray(obj.publishes) ? obj.publishes : [],
      subscribes: Array.isArray(obj.subscribes) ? obj.subscribes : [],
      schema_key: obj.schema_key ?? "",
      health_key: obj.health_key ?? "",
      config_key: obj.config_key ?? "",
      security: {
        acl_prefix: obj.security?.acl_prefix ?? "",
        data_classification: obj.security?.data_classification ?? "",
      },
      time: {
        clock_source: obj.time?.clock_source ?? "",
        timestamp_field: obj.time?.timestamp_field ?? "",
        timestamp_unit: obj.time?.timestamp_unit ?? "",
      },
    };
  } catch {
    return null;
  }
}

// ---------------------------------------------------------------------------
// Provider
// ---------------------------------------------------------------------------

interface NodeDiscoveryProviderProps {
  session: Session | null;
  children: ReactNode;
}

export function NodeDiscoveryProvider({
  session,
  children,
}: NodeDiscoveryProviderProps) {
  const sessionRef = useRef<Session | null>(null);
  sessionRef.current = session;

  // State exposed via context
  const [nodes, setNodes] = useState<DiscoveredNode[]>([]);
  const [loading, setLoading] = useState(true);
  const [error, setError] = useState<string | null>(null);
  const [daemonConnected, setDaemonConnected] = useState(false);
  const [manifestDiscoveryActive, setManifestDiscoveryActive] = useState(false);

  // Internal tracking
  const hasReceivedDaemonDataRef = useRef(false);

  // Daemon: track per-machine last-seen and last-known nodes for stale detection
  const machineLastSeenRef = useRef(new Map<string, number>());
  const prevDaemonNodesRef = useRef(new Map<string, DiscoveredNode[]>());

  // Manifest: keyed by nodeKey
  const manifestMapRef = useRef(new Map<string, NodeManifest>());
  const manifestLastSeenRef = useRef(new Map<string, number>());

  // Version counter to track refreshes
  const refreshCounterRef = useRef(0);

  const STALE_THRESHOLD_MS = 15_000;

  // -----------------------------------------------------------------------
  // Daemon polling (every 3s)
  // -----------------------------------------------------------------------
  const pollDaemon = useCallback(async (): Promise<DiscoveredNode[]> => {
    const currentSession = sessionRef.current;
    if (!currentSession) return [];

    try {
      const receiver = await currentSession.get("bubbaloop/daemon/nodes", {
        timeout: Duration.milliseconds.of(5000),
      });

      const allNodes = new Map<string, DiscoveredNode[]>();

      if (receiver) {
        for await (const replyItem of receiver) {
          if (replyItem instanceof Reply) {
            let sample: Sample;
            try {
              const result = replyItem.result();
              if (result instanceof ReplyError) continue;
              sample = result as Sample;
            } catch {
              continue;
            }

            const payload = getSamplePayload(sample);
            const nodeList = decodeNodeList(payload);
            if (!nodeList || nodeList.nodes.length === 0) continue;

            const listMachineId = nodeList.machineId || "";
            const mapped: DiscoveredNode[] = nodeList.nodes.map((n) => ({
              name: n.name,
              machine_id: n.machineId || listMachineId,
              path: n.path,
              status: statusNumberToString(n.status),
              installed: n.installed,
              autostart_enabled: n.autostartEnabled,
              version: n.version,
              description: n.description,
              node_type: n.nodeType,
              is_built: n.isBuilt,
              build_output: n.buildOutput,
              machine_hostname: n.machineHostname,
              machine_ips: n.machineIps || [],
              base_node: n.baseNode || "",
              discoveredVia: "daemon" as const,
              stale: false,
              lastSeen: Date.now(),
            }));

            allNodes.set(listMachineId, mapped);
            machineLastSeenRef.current.set(listMachineId, Date.now());

            if (!hasReceivedDaemonDataRef.current) {
              hasReceivedDaemonDataRef.current = true;
              setDaemonConnected(true);
              setError(null);
              setLoading(false);
            }
          }
        }
      }

      // Build merged daemon nodes (fresh + stale from offline machines)
      const now = Date.now();
      const mergedDaemonNodes: DiscoveredNode[] = [];

      // Update prevDaemonNodes with fresh replies
      for (const [mid, machineNodes] of allNodes.entries()) {
        prevDaemonNodesRef.current.set(mid, machineNodes);
      }

      // Fresh nodes
      for (const machineNodes of allNodes.values()) {
        mergedDaemonNodes.push(...machineNodes);
      }

      // Stale nodes from machines that did not reply
      for (const [mid, lastSeen] of machineLastSeenRef.current.entries()) {
        if (!allNodes.has(mid)) {
          if (now - lastSeen <= STALE_THRESHOLD_MS) {
            const lastKnown = prevDaemonNodesRef.current.get(mid);
            if (lastKnown) {
              mergedDaemonNodes.push(
                ...lastKnown.map((n) => ({ ...n, stale: true })),
              );
            }
          } else {
            prevDaemonNodesRef.current.delete(mid);
            machineLastSeenRef.current.delete(mid);
          }
        }
      }

      if (mergedDaemonNodes.length > 0) {
        setDaemonConnected(true);
      }

      return mergedDaemonNodes;
    } catch (err) {
      console.warn("[NodeDiscovery] Daemon poll failed:", err);
      return [];
    }
  }, []);

  // -----------------------------------------------------------------------
  // Manifest discovery (every 10s, backs off to 30s)
  // -----------------------------------------------------------------------
  const discoverManifests = useCallback(async (): Promise<
    Map<string, NodeManifest>
  > => {
    const currentSession = sessionRef.current;
    if (!currentSession) return manifestMapRef.current;

    try {
      setManifestDiscoveryActive(true);

      const receiver = await currentSession.get("bubbaloop/**/manifest", {
        timeout: Duration.milliseconds.of(5000),
      });

      if (receiver) {
        const now = Date.now();
        for await (const replyItem of receiver) {
          if (replyItem instanceof Reply) {
            let sample: Sample;
            try {
              const result = replyItem.result();
              if (result instanceof ReplyError) continue;
              sample = result as Sample;
            } catch {
              continue;
            }

            const payload = getSamplePayload(sample);
            const manifest = parseManifest(payload);
            if (manifest) {
              const key = nodeKey(manifest.machine_id, manifest.name);
              manifestMapRef.current.set(key, manifest);
              manifestLastSeenRef.current.set(key, now);
            }
          }
        }
      }
    } catch (err) {
      console.warn("[NodeDiscovery] Manifest discovery failed:", err);
    } finally {
      setManifestDiscoveryActive(false);
    }

    return manifestMapRef.current;
  }, []);

  // -----------------------------------------------------------------------
  // Merge daemon nodes + manifest data
  // -----------------------------------------------------------------------
  const mergeNodes = useCallback(
    (
      daemonNodes: DiscoveredNode[],
      manifests: Map<string, NodeManifest>,
    ): DiscoveredNode[] => {
      const now = Date.now();
      const merged = new Map<string, DiscoveredNode>();

      // Add all daemon nodes first
      for (const dn of daemonNodes) {
        const key = nodeKey(dn.machine_id, dn.name);
        const manifest = manifests.get(key);
        merged.set(key, {
          ...dn,
          manifest: manifest ?? undefined,
          discoveredVia: manifest ? "both" : "daemon",
        });
      }

      // Add manifest-only nodes (not in daemon)
      for (const [key, manifest] of manifests.entries()) {
        if (!merged.has(key)) {
          const lastSeen = manifestLastSeenRef.current.get(key) ?? now;
          // Only include if not too stale (60s for manifests since they poll less often)
          if (now - lastSeen <= 60_000) {
            merged.set(key, manifestOnlyNode(manifest, lastSeen));
          } else {
            // Clean up old entries
            manifests.delete(key);
            manifestLastSeenRef.current.delete(key);
          }
        }
      }

      return Array.from(merged.values());
    },
    [],
  );

  // -----------------------------------------------------------------------
  // Combined polling loop
  // -----------------------------------------------------------------------
  useEffect(() => {
    if (!session) {
      setNodes([]);
      setLoading(true);
      setError(null);
      setDaemonConnected(false);
      hasReceivedDaemonDataRef.current = false;
      manifestMapRef.current.clear();
      manifestLastSeenRef.current.clear();
      machineLastSeenRef.current.clear();
      prevDaemonNodesRef.current.clear();
      return;
    }

    let mounted = true;
    let daemonTimer: ReturnType<typeof setTimeout> | null = null;
    let manifestTimer: ReturnType<typeof setTimeout> | null = null;
    let manifestEmptyConsecutive = 0;
    const capturedRefreshCounter = refreshCounterRef.current;

    // Daemon loop
    const runDaemonPoll = async () => {
      if (!mounted) return;

      const daemonNodes = await pollDaemon();

      if (!mounted) return;

      const allNodes = mergeNodes(daemonNodes, manifestMapRef.current);
      if (allNodes.length > 0) {
        setNodes(allNodes);
      }

      if (mounted && capturedRefreshCounter === refreshCounterRef.current) {
        daemonTimer = setTimeout(runDaemonPoll, 3000);
      }
    };

    // Manifest loop
    const runManifestDiscovery = async () => {
      if (!mounted) return;

      const prevSize = manifestMapRef.current.size;
      await discoverManifests();

      if (!mounted) return;

      const newSize = manifestMapRef.current.size;
      if (newSize > prevSize) {
        manifestEmptyConsecutive = 0;
        // Re-merge with latest daemon state
        const daemonNodes: DiscoveredNode[] = [];
        for (const machineNodes of prevDaemonNodesRef.current.values()) {
          daemonNodes.push(...machineNodes);
        }
        const allNodes = mergeNodes(daemonNodes, manifestMapRef.current);
        if (allNodes.length > 0) {
          setNodes(allNodes);
        }
      } else {
        manifestEmptyConsecutive++;
      }

      if (mounted && capturedRefreshCounter === refreshCounterRef.current) {
        const interval = manifestEmptyConsecutive >= 3 ? 30_000 : 10_000;
        manifestTimer = setTimeout(runManifestDiscovery, interval);
      }
    };

    // Start both loops
    runDaemonPoll();
    // Delay manifest discovery slightly so daemon data arrives first
    manifestTimer = setTimeout(runManifestDiscovery, 2000);

    // Timeout for initial connection
    const timeout = setTimeout(() => {
      if (
        !hasReceivedDaemonDataRef.current &&
        manifestMapRef.current.size === 0
      ) {
        setLoading(false);
        setError(
          "No data from daemon or nodes - check if bubbaloop-daemon is running",
        );
      }
    }, 15_000);

    return () => {
      mounted = false;
      clearTimeout(timeout);
      if (daemonTimer) clearTimeout(daemonTimer);
      if (manifestTimer) clearTimeout(manifestTimer);
    };
  }, [session, pollDaemon, discoverManifests, mergeNodes]);

  // -----------------------------------------------------------------------
  // Fleet reporting
  // -----------------------------------------------------------------------
  const { reportMachines, reportNodes } = useFleetContext();

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

  useEffect(() => {
    reportMachines(
      machineGroups.map((g) => ({
        machineId: g.machineId,
        hostname: g.hostname,
        nodeCount: g.nodes.length,
        runningCount: g.nodes.filter((n) => n.status === "running").length,
        isOnline: g.isOnline,
        ips: g.nodes[0]?.machine_ips || [],
      })),
    );
  }, [machineGroups, reportMachines]);

  useEffect(() => {
    reportNodes(
      nodes.map((n) => ({
        name: n.name,
        status: n.status,
        machineId: n.machine_id || "local",
        hostname: n.machine_hostname || "local",
        ips: n.machine_ips || [],
        nodeType: n.node_type,
        version: n.version,
        baseNode: n.base_node || "",
      })),
    );
  }, [nodes, reportNodes]);

  // -----------------------------------------------------------------------
  // Refresh (restarts both loops)
  // -----------------------------------------------------------------------
  const refresh = useCallback(() => {
    refreshCounterRef.current++;
    // Trigger re-mount of effect by bumping a dependency
    // The effect depends on session, so we force it by clearing state
    setLoading(true);
    setError(null);
    hasReceivedDaemonDataRef.current = false;
    // The effect won't re-run just from state changes; we need the session
    // to be the same. Instead, manually trigger a poll.
    (async () => {
      const daemonNodes = await pollDaemon();
      await discoverManifests();
      const allNodes = mergeNodes(daemonNodes, manifestMapRef.current);
      setNodes(allNodes);
      setLoading(false);
    })();
  }, [pollDaemon, discoverManifests, mergeNodes]);

  // -----------------------------------------------------------------------
  // Context value
  // -----------------------------------------------------------------------
  const value = useMemo<NodeDiscoveryContextValue>(
    () => ({
      nodes,
      loading,
      error,
      daemonConnected,
      manifestDiscoveryActive,
      refresh,
    }),
    [nodes, loading, error, daemonConnected, manifestDiscoveryActive, refresh],
  );

  return (
    <NodeDiscoveryContext.Provider value={value}>
      {children}
    </NodeDiscoveryContext.Provider>
  );
}

// ---------------------------------------------------------------------------
// Hook
// ---------------------------------------------------------------------------

export function useNodeDiscovery(): NodeDiscoveryContextValue {
  return useContext(NodeDiscoveryContext);
}
