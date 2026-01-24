import React, { useState, useEffect, useRef } from "react";
import { Box, Text, useInput } from "ink";
import TextInput from "ink-text-input";
import { existsSync, readdirSync } from "fs";
import { join, resolve, dirname, basename } from "path";
import { homedir } from "os";
import {
  listNodes,
  NodeManifest,
  getServiceName,
  registerNode,
  unregisterNode,
} from "./config.js";
import { DaemonClient, NodeState } from "./daemon-client.js";
import NodeDetailView from "./NodeDetailView.js";
import { ChildProcess } from "child_process";

interface NodesViewProps {
  onBack: () => void;
}

type ServiceStatus = "stopped" | "running" | "failed" | "not-installed" | "building" | "unknown";

interface NodeInfo {
  path: string;
  manifest: NodeManifest;
  valid: boolean;
  serviceStatus: ServiceStatus;
  isBuilt: boolean;
  autostartEnabled: boolean;
  buildOutput: string[];
}

type ViewMode = "list" | "detail" | "add";
type BuildStatus = "idle" | "building" | "cleaning";

// Get directory suggestions for path completion
function getPathSuggestions(inputPath: string): string[] {
  try {
    // Expand ~ to home directory
    const expanded = inputPath.startsWith("~")
      ? join(homedir(), inputPath.slice(1))
      : inputPath;

    const dir = expanded.endsWith("/") ? expanded : dirname(expanded);
    const prefix = expanded.endsWith("/") ? "" : basename(expanded);

    if (!existsSync(dir)) return [];

    const entries = readdirSync(dir, { withFileTypes: true });
    return entries
      .filter(e => e.isDirectory() && e.name.startsWith(prefix) && !e.name.startsWith("."))
      .map(e => join(dir, e.name))
      .slice(0, 5);
  } catch {
    return [];
  }
}

export interface BuildState {
  status: BuildStatus;
  output: string[];
  process: ChildProcess | null;
}

// Flower-like spinner
const FLOWER_FRAMES = ["✻", "✼", "✽", "✾", "✿", "❀", "❁", "✿", "✾", "✽", "✼"];

const FlowerSpinner: React.FC = () => {
  const [frame, setFrame] = useState(0);

  useEffect(() => {
    const interval = setInterval(() => {
      setFrame((f) => (f + 1) % FLOWER_FRAMES.length);
    }, 150);
    return () => clearInterval(interval);
  }, []);

  return <Text color="#FFD93D">{FLOWER_FRAMES[frame]}</Text>;
};

const NodesView: React.FC<NodesViewProps> = ({ onBack }) => {
  const [nodes, setNodes] = useState<NodeInfo[]>([]);
  const [selectedIndex, setSelectedIndex] = useState(0);
  const [message, setMessage] = useState<string | null>(null);
  const [viewMode, setViewMode] = useState<ViewMode>("list");
  const [selectedNode, setSelectedNode] = useState<NodeInfo | null>(null);
  const [addPath, setAddPath] = useState("");
  const [pathSuggestions, setPathSuggestions] = useState<string[]>([]);
  const [suggestionIndex, setSuggestionIndex] = useState(0);
  const [confirmRemove, setConfirmRemove] = useState(false);
  const [daemonAvailable, setDaemonAvailable] = useState(false);
  const buildStates = useRef<Map<string, BuildState>>(new Map());
  const daemonClient = useRef(new DaemonClient());
  const [, forceUpdate] = useState({});

  const getBuildState = (nodeName: string): BuildState => {
    return buildStates.current.get(nodeName) || { status: "idle", output: [], process: null };
  };

  const updateBuildState = (nodeName: string, update: Partial<BuildState>) => {
    const current = getBuildState(nodeName);
    buildStates.current.set(nodeName, { ...current, ...update });
    forceUpdate({});
  };

  // Convert daemon NodeState to local NodeInfo
  const convertDaemonNode = (state: NodeState): NodeInfo => {
    return {
      path: state.path,
      manifest: {
        name: state.name,
        version: state.version,
        type: state.node_type as "rust" | "python",
        description: state.description,
      },
      valid: true,
      serviceStatus: state.status === "not-installed" ? "not-installed" :
                     state.status === "running" ? "running" :
                     state.status === "stopped" ? "stopped" :
                     state.status === "failed" ? "failed" :
                     state.status === "building" ? "building" : "stopped",
      isBuilt: state.is_built,
      autostartEnabled: state.autostart_enabled,
      buildOutput: state.build_output,
    };
  };

  // Load nodes from daemon if available, otherwise fallback to local registry
  const loadNodes = async () => {
    try {
      // Check daemon availability
      const available = await daemonClient.current.isAvailable();
      setDaemonAvailable(available);

      if (available) {
        // Use daemon for node state
        const response = await daemonClient.current.listNodes();
        const daemonNodes = response.nodes.map(convertDaemonNode);
        setNodes(daemonNodes);
      } else {
        // Fallback to local registry (without systemd status)
        const registered = listNodes();
        const localNodes = registered.map((n) => ({
          ...n,
          serviceStatus: "unknown" as ServiceStatus,
          isBuilt: false,
          autostartEnabled: false,
          buildOutput: [] as string[],
        }));
        setNodes(localNodes);
      }
    } catch (err) {
      // On error, use local registry
      setDaemonAvailable(false);
      const registered = listNodes();
      const localNodes = registered.map((n) => ({
        ...n,
        serviceStatus: "unknown" as ServiceStatus,
        isBuilt: false,
        autostartEnabled: false,
        buildOutput: [] as string[],
      }));
      setNodes(localNodes);
    }
  };

  useEffect(() => {
    loadNodes();
    const interval = setInterval(loadNodes, 3000);
    return () => clearInterval(interval);
  }, []);

  // Handle add mode input separately
  useInput((input, key) => {
    if (viewMode !== "add") return;

    if (key.escape) {
      setViewMode("list");
      setAddPath("");
      setPathSuggestions([]);
      return;
    }

    if (key.tab && pathSuggestions.length > 0) {
      // Cycle through suggestions
      if (key.shift) {
        setSuggestionIndex((i) => (i - 1 + pathSuggestions.length) % pathSuggestions.length);
      } else {
        setSuggestionIndex((i) => (i + 1) % pathSuggestions.length);
      }
      setAddPath(pathSuggestions[suggestionIndex] + "/");
    }
  }, { isActive: viewMode === "add" });

  useInput((input, key) => {
    if (viewMode === "detail" || viewMode === "add") return;

    if (key.escape || input === "q") {
      onBack();
      return;
    }

    if (key.upArrow || input === "k") {
      setSelectedIndex((p) => Math.max(0, p - 1));
      setConfirmRemove(false);
    } else if (key.downArrow || input === "j") {
      setSelectedIndex((p) => Math.min(nodes.length - 1, p + 1));
      setConfirmRemove(false);
    }

    const node = nodes[selectedIndex];

    if (key.return && node) {
      setSelectedNode(node);
      setViewMode("detail");
    } else if (input === "a") {
      // Add node
      setViewMode("add");
      setAddPath("");
      setPathSuggestions([]);
    } else if (input === "r" && node) {
      // Remove node (unregister)
      if (confirmRemove) {
        handleRemoveNode(node);
        setConfirmRemove(false);
      } else {
        setConfirmRemove(true);
        setMessage("Press [r] again to confirm removal");
        setTimeout(() => {
          setConfirmRemove(false);
          setMessage(null);
        }, 3000);
      }
    } else if (input === "i" && node && node.serviceStatus === "not-installed") {
      handleInstallNode(node);
    } else if (input === "s" && node && (node.serviceStatus === "stopped" || node.serviceStatus === "failed") && node.isBuilt) {
      handleStartNode(node);
    } else if (input === "s" && node && (node.serviceStatus === "stopped" || node.serviceStatus === "failed") && !node.isBuilt) {
      setMessage("Cannot start: " + node.manifest.name + " not built");
      setTimeout(() => setMessage(null), 2000);
    } else if (input === "s" && node && node.serviceStatus === "running") {
      handleStopNode(node);
    }
  }, { isActive: viewMode === "list" });

  // Command handlers using daemon when available
  const handleStartNode = async (node: NodeInfo) => {
    if (daemonAvailable) {
      try {
        const result = await daemonClient.current.startNode(node.manifest.name);
        setMessage(result.success ? "Started: " + node.manifest.name : "Error: " + result.message);
      } catch (err) {
        setMessage("Error: " + (err instanceof Error ? err.message : String(err)));
      }
    } else {
      setMessage("Daemon not available");
    }
    loadNodes();
    setTimeout(() => setMessage(null), 2000);
  };

  const handleStopNode = async (node: NodeInfo) => {
    if (daemonAvailable) {
      try {
        const result = await daemonClient.current.stopNode(node.manifest.name);
        setMessage(result.success ? "Stopped: " + node.manifest.name : "Error: " + result.message);
      } catch (err) {
        setMessage("Error: " + (err instanceof Error ? err.message : String(err)));
      }
    } else {
      setMessage("Daemon not available");
    }
    loadNodes();
    setTimeout(() => setMessage(null), 2000);
  };

  const handleInstallNode = async (node: NodeInfo) => {
    if (daemonAvailable) {
      try {
        const result = await daemonClient.current.installNode(node.manifest.name);
        setMessage(result.success ? "Installed: " + node.manifest.name : "Error: " + result.message);
      } catch (err) {
        setMessage("Error: " + (err instanceof Error ? err.message : String(err)));
      }
    } else {
      setMessage("Daemon not available");
    }
    loadNodes();
    setTimeout(() => setMessage(null), 2000);
  };

  const handleRemoveNode = async (node: NodeInfo) => {
    if (daemonAvailable) {
      try {
        const result = await daemonClient.current.removeNode(node.manifest.name);
        setMessage(result.success ? "Removed: " + node.manifest.name : "Error: " + result.message);
      } catch (err) {
        setMessage("Error: " + (err instanceof Error ? err.message : String(err)));
      }
    } else {
      // Fallback to local registry
      const result = unregisterNode(node.path);
      setMessage(result.success ? "Removed: " + node.manifest.name : "Error: " + result.error);
    }
    loadNodes();
    setTimeout(() => setMessage(null), 2000);
  };

  // Handle path input changes
  const handleAddPathChange = (value: string) => {
    setAddPath(value);
    setPathSuggestions(getPathSuggestions(value));
    setSuggestionIndex(0);
  };

  const handleAddSubmit = async (value: string) => {
    // Normalize path: expand ~ and resolve to absolute
    let expanded = value.trim();
    if (expanded.startsWith("~")) {
      expanded = join(homedir(), expanded.slice(1));
    }
    expanded = resolve(expanded);
    // Remove trailing slash for consistent comparison
    if (expanded.endsWith("/") && expanded.length > 1) {
      expanded = expanded.slice(0, -1);
    }

    // Check if already registered
    const alreadyExists = nodes.some(n => {
      let nodePath = n.path;
      if (nodePath.endsWith("/") && nodePath.length > 1) {
        nodePath = nodePath.slice(0, -1);
      }
      return nodePath === expanded;
    });

    if (alreadyExists) {
      setMessage("Error: Node already registered");
      setViewMode("list");
      setAddPath("");
      setPathSuggestions([]);
      setTimeout(() => setMessage(null), 3000);
      return;
    }

    if (daemonAvailable) {
      try {
        const result = await daemonClient.current.addNode(expanded);
        setMessage(result.success ? "Added: " + expanded : "Error: " + result.message);
      } catch (err) {
        setMessage("Error: " + (err instanceof Error ? err.message : String(err)));
      }
    } else {
      // Fallback to local registry
      const result = registerNode(expanded);
      if (result.success) {
        setMessage("Added: " + expanded);
      } else {
        setMessage("Error: " + result.error);
      }
    }

    setViewMode("list");
    setAddPath("");
    setPathSuggestions([]);
    loadNodes();
    setTimeout(() => setMessage(null), 3000);
  };

  const StatusBadge: React.FC<{ status: ServiceStatus }> = ({ status }) => {
    const cfg: Record<ServiceStatus, { color: string; symbol: string }> = {
      running: { color: "#95E1D3", symbol: "\u25cf" },
      stopped: { color: "#888", symbol: "\u25cb" },
      failed: { color: "#FF6B6B", symbol: "x" },
      "not-installed": { color: "#666", symbol: "-" },
      building: { color: "#FFD93D", symbol: "✿" },
      unknown: { color: "#666", symbol: "?" },
    };
    const { color, symbol } = cfg[status];
    return <Text color={color}>{symbol}</Text>;
  };

  const navigateToNode = (index: number) => {
    const node = nodes[index];
    if (node) {
      setSelectedIndex(index);
      setSelectedNode(node);
    }
  };

  const handleNextNode = () => {
    const nextIndex = (selectedIndex + 1) % nodes.length;
    navigateToNode(nextIndex);
  };

  const handlePrevNode = () => {
    const prevIndex = (selectedIndex - 1 + nodes.length) % nodes.length;
    navigateToNode(prevIndex);
  };

  // Detail view
  if (viewMode === "detail" && selectedNode) {
    return (
      <NodeDetailView
        nodePath={selectedNode.path}
        manifest={selectedNode.manifest}
        buildState={getBuildState(selectedNode.manifest.name)}
        updateBuildState={(update) => updateBuildState(selectedNode.manifest.name, update)}
        onBack={() => {
          setViewMode("list");
          setSelectedNode(null);
          loadNodes();
        }}
        onNext={handleNextNode}
        onPrev={handlePrevNode}
        currentIndex={selectedIndex}
        totalNodes={nodes.length}
        daemonAvailable={daemonAvailable}
      />
    );
  }

  // Add node view
  if (viewMode === "add") {
    return (
      <Box flexDirection="column" padding={0}>
        <Box borderStyle="round" borderColor="#4ECDC4" paddingX={1} justifyContent="space-between">
          <Text color="#4ECDC4" bold>Add Node</Text>
          <Text color="#888"><Text color="#4ECDC4">[esc]</Text> cancel  <Text color="#4ECDC4">[tab]</Text> complete</Text>
        </Box>

        <Box marginX={1} marginTop={1} flexDirection="column">
          <Text color="#888">Enter path to node directory (must contain node.yaml):</Text>
          <Box marginTop={1}>
            <Text color="#4ECDC4">Path: </Text>
            <TextInput
              value={addPath}
              onChange={handleAddPathChange}
              onSubmit={handleAddSubmit}
              placeholder="~/path/to/node or /absolute/path"
            />
          </Box>

          {pathSuggestions.length > 0 && (
            <Box flexDirection="column" marginTop={1}>
              <Text color="#888">Suggestions:</Text>
              {pathSuggestions.map((s, i) => (
                <Text key={s} color={i === suggestionIndex ? "#4ECDC4" : "#666"}>
                  {i === suggestionIndex ? "❯ " : "  "}{s}
                </Text>
              ))}
            </Box>
          )}
        </Box>
      </Box>
    );
  }

  // Main list view
  const node = nodes[selectedIndex];

  return (
    <Box flexDirection="column" padding={0}>
      <Box borderStyle="round" borderColor="#4ECDC4" paddingX={1} justifyContent="space-between">
        <Text color="#4ECDC4" bold>Nodes</Text>
        <Text color="#888">
          {nodes.length} registered
          {daemonAvailable ? <Text color="#95E1D3"> ● daemon</Text> : <Text color="#FF6B6B"> ○ no daemon</Text>}
          {" "}{"\u2022"} <Text color="#666">esc/q back</Text>
        </Text>
      </Box>

      <Box flexDirection="column" borderStyle="single" borderColor="#444" marginTop={1}>
        <Box paddingX={1} borderBottom borderColor="#444">
          <Box width={3}><Text color="#4ECDC4" bold>St</Text></Box>
          <Box width="20%"><Text color="#4ECDC4" bold>Name</Text></Box>
          <Box width="8%"><Text color="#4ECDC4" bold>Version</Text></Box>
          <Box width="8%"><Text color="#4ECDC4" bold>Type</Text></Box>
          <Box width="8%"><Text color="#4ECDC4" bold>Built</Text></Box>
          <Box width="51%"><Text color="#4ECDC4" bold>Description</Text></Box>
        </Box>

        {nodes.length === 0 ? (
          <Box paddingX={1} paddingY={1}>
            <Text color="#888">No nodes registered.</Text>
          </Box>
        ) : (
          nodes.map((n, index) => {
            const isSelected = index === selectedIndex;
            const nodeBuildState = getBuildState(n.manifest.name);
            const isBuilding = nodeBuildState.status !== "idle" || n.serviceStatus === "building";
            return (
              <Box key={n.path} paddingX={1}>
                <Box width={3}>
                  {isBuilding ? <FlowerSpinner /> : <StatusBadge status={n.serviceStatus} />}
                </Box>
                <Box width="20%">
                  <Text color={isSelected ? "#4ECDC4" : n.valid ? "#CCC" : "#FF6B6B"}>
                    {isSelected ? "\u276f " : "  "}{n.manifest.name}
                  </Text>
                </Box>
                <Box width="8%"><Text color="#95E1D3">{n.manifest.version}</Text></Box>
                <Box width="8%">
                  <Text color={n.manifest.type === "rust" ? "#FFD93D" : "#4ECDC4"}>{n.manifest.type}</Text>
                </Box>
                <Box width="8%">
                  <Text color={isBuilding ? "#FFD93D" : n.isBuilt ? "#95E1D3" : "#FF6B6B"}>
                    {isBuilding ? "..." : n.isBuilt ? "yes" : "no"}
                  </Text>
                </Box>
                <Box width="51%">
                  <Text color="#888">{(n.manifest.description || "").slice(0, 40)}</Text>
                </Box>
              </Box>
            );
          })
        )}
      </Box>

      {message && <Box marginX={1} marginTop={1}><Text color="#FFD93D">{message}</Text></Box>}

      <Box marginX={1} marginTop={1} flexDirection="column">
        <Text color="#666">
          <Text color="#4ECDC4">[enter]</Text> details  <Text color="#4ECDC4">[a]</Text>dd node  {node && (confirmRemove ? <Text color="#FF6B6B">[r] CONFIRM?</Text> : <><Text color="#4ECDC4">[r]</Text>emove</>)}
        </Text>
        {node && (
          <Text color="#666">
            {node.serviceStatus === "not-installed" && <><Text color="#4ECDC4">[i]</Text>nstall  </>}
            {node.serviceStatus !== "not-installed" && node.serviceStatus !== "unknown" && (
              <>
                {node.serviceStatus === "running" ? (
                  <><Text color="#4ECDC4">[s]</Text>top  </>
                ) : node.isBuilt ? (
                  <><Text color="#4ECDC4">[s]</Text>tart  </>
                ) : (
                  <Text color="#555">[s]tart <Text color="#FF6B6B">(build first)</Text>  </Text>
                )}
              </>
            )}
          </Text>
        )}
      </Box>

      <Box marginX={1} marginTop={1}>
        <Text color="#666">
          <Text color="#95E1D3">{"\u25cf"}</Text> running  <Text color="#888">{"\u25cb"}</Text> stopped  <Text color="#FF6B6B">x</Text> failed  <Text color="#666">-</Text> not installed  <Text color="#FFD93D">✿</Text> building
        </Text>
      </Box>
    </Box>
  );
};

export default NodesView;
