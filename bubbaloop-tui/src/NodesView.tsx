import React, { useState, useEffect, useRef, useLayoutEffect } from "react";
import { Box, Text, useInput } from "ink";
import TextInput from "ink-text-input";
import { existsSync, readdirSync, readFileSync } from "fs";
import { join, resolve, dirname, basename } from "path";
import { homedir } from "os";
import YAML from "yaml";
import {
  listNodes,
  NodeManifest,
  registerNode,
  unregisterNode,
  getEnabledSources,
  getAllSources,
  addSource,
  removeSource,
  toggleSource,
  updateSource,
  NodeSource,
} from "./config.js";
import { DaemonClient, NodeState } from "./daemon-client.js";
import NodeDetailView, { BuildState } from "./NodeDetailView.js";

interface NodesViewProps {
  onBack: () => void;
  onExit?: () => void;
  exitWarning?: boolean;
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

interface DiscoverableNode {
  path: string;
  manifest: NodeManifest;
  source: string; // e.g., "bubbaloop-nodes", "~/.bubbaloop/nodes"
}

type ViewMode = "list" | "detail" | "add" | "marketplace-form";
type TabMode = "installed" | "discover" | "marketplace";

function getDiscoverPaths(): string[] {
  const paths: string[] = [];

  // Add paths from enabled sources
  const sources = getEnabledSources();
  for (const source of sources) {
    if (source.type === "local") {
      paths.push(source.path);
    }
  }

  // Add local development path if in a bubbaloop project
  const localDev = join(process.cwd(), "crates/bubbaloop-nodes");
  if (existsSync(localDev) && !paths.includes(localDev)) {
    paths.push(localDev);
  }

  return paths;
}

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

function scanForNodes(basePath: string): DiscoverableNode[] {
  if (!existsSync(basePath)) return [];

  const nodes: DiscoverableNode[] = [];

  try {
    const entries = readdirSync(basePath, { withFileTypes: true });
    for (const entry of entries) {
      if (!entry.isDirectory()) continue;

      const nodePath = join(basePath, entry.name);
      const manifestPath = join(nodePath, "node.yaml");

      if (!existsSync(manifestPath)) continue;

      try {
        const content = readFileSync(manifestPath, "utf-8");
        const manifest = YAML.parse(content) as NodeManifest;
        nodes.push({ path: nodePath, manifest, source: basename(basePath) });
      } catch {
        // Skip invalid manifests
      }
    }
  } catch {
    // Ignore scan errors
  }

  return nodes;
}

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

const STATUS_CONFIG: Record<ServiceStatus, { color: string; symbol: string }> = {
  running: { color: "#95E1D3", symbol: "\u25cf" },
  stopped: { color: "#888", symbol: "\u25cb" },
  failed: { color: "#FF6B6B", symbol: "x" },
  "not-installed": { color: "#666", symbol: "-" },
  building: { color: "#FFD93D", symbol: "✿" },
  unknown: { color: "#666", symbol: "?" },
};

const StatusBadge: React.FC<{ status: ServiceStatus }> = ({ status }) => {
  const { color, symbol } = STATUS_CONFIG[status];
  return <Text color={color}>{symbol}</Text>;
};

const NodesView: React.FC<NodesViewProps> = ({ onBack, onExit, exitWarning }) => {
  const [nodes, setNodes] = useState<NodeInfo[]>([]);
  const [discoverableNodes, setDiscoverableNodes] = useState<DiscoverableNode[]>([]);
  const [sources, setSources] = useState<NodeSource[]>([]);
  const [selectedIndex, setSelectedIndex] = useState(0);
  const [discoverIndex, setDiscoverIndex] = useState(0);
  const [sourceIndex, setSourceIndex] = useState(0);
  const [message, setMessage] = useState<string | null>(null);
  const [viewMode, setViewMode] = useState<ViewMode>("list");
  const [tabMode, setTabMode] = useState<TabMode>("installed");
  const [selectedNode, setSelectedNode] = useState<NodeInfo | null>(null);
  const [addPath, setAddPath] = useState("");
  // Marketplace form state
  const [marketplaceName, setMarketplaceName] = useState("");
  const [marketplacePath, setMarketplacePath] = useState("");
  const [marketplaceEditPath, setMarketplaceEditPath] = useState<string | null>(null); // null = add mode, string = edit mode (original path)
  const [marketplaceActiveField, setMarketplaceActiveField] = useState<"name" | "path">("name");
  const [pathSuggestions, setPathSuggestions] = useState<string[]>([]);
  const [suggestionIndex, setSuggestionIndex] = useState(0);
  const [confirmRemove, setConfirmRemove] = useState(false);
  const [daemonAvailable, setDaemonAvailable] = useState(false);
  const buildStates = useRef<Map<string, BuildState>>(new Map());
  const daemonClient = useRef(new DaemonClient());
  const [, forceUpdate] = useState({});
  const initialLoadDone = useRef(false);

  // Clear screen on first load to prevent TUI artifacts
  useLayoutEffect(() => {
    if (nodes.length > 0 && !initialLoadDone.current) {
      initialLoadDone.current = true;
      process.stdout.write('\x1b[2J\x1b[H');
    }
  }, [nodes.length]);

  const getBuildState = (nodeName: string): BuildState => {
    return buildStates.current.get(nodeName) || { status: "idle", output: [], process: null };
  };

  const updateBuildState = (nodeName: string, update: Partial<BuildState>) => {
    const current = getBuildState(nodeName);
    buildStates.current.set(nodeName, { ...current, ...update });
    forceUpdate({});
  };

  const convertDaemonNode = (state: NodeState): NodeInfo => ({
    path: state.path,
    manifest: {
      name: state.name,
      version: state.version,
      type: state.node_type as "rust" | "python",
      description: state.description,
    },
    valid: true,
    serviceStatus: (state.status as ServiceStatus) ?? "stopped",
    isBuilt: state.is_built,
    autostartEnabled: state.autostart_enabled,
    buildOutput: state.build_output,
  });

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

  const loadSources = () => setSources(getAllSources());

  // Scan for discoverable nodes
  const loadDiscoverableNodes = () => {
    const discovered: DiscoverableNode[] = [];
    const registeredPaths = new Set(nodes.map(n => resolve(n.path)));
    const discoverPaths = getDiscoverPaths();

    for (const basePath of discoverPaths) {
      const found = scanForNodes(basePath);
      for (const node of found) {
        // Only show nodes not already registered
        if (!registeredPaths.has(resolve(node.path))) {
          discovered.push(node);
        }
      }
    }

    setDiscoverableNodes(discovered);
  };

  useEffect(() => {
    loadNodes();
    const interval = setInterval(loadNodes, 3000);
    return () => clearInterval(interval);
  }, []);

  // Update discoverable nodes when nodes or tab changes
  useEffect(() => {
    if (tabMode === "discover") {
      loadDiscoverableNodes();
    }
    if (tabMode === "marketplace") {
      loadSources();
    }
  }, [tabMode, nodes]);

  // Handle add mode input separately
  useInput((input, key) => {
    if (viewMode !== "add" && viewMode !== "marketplace-form") return;

    // Global exit: Ctrl+C or Ctrl+X
    if (key.ctrl && (input === 'c' || input === 'x')) {
      onExit?.();
      return;
    }

    if (key.escape) {
      setViewMode("list");
      setAddPath("");
      setMarketplaceName("");
      setMarketplacePath("");
      setMarketplaceEditPath(null);
      setMarketplaceActiveField("name");
      setPathSuggestions([]);
      return;
    }

    if (viewMode === "add" && key.tab && pathSuggestions.length > 0) {
      // Cycle through suggestions
      if (key.shift) {
        setSuggestionIndex((i) => (i - 1 + pathSuggestions.length) % pathSuggestions.length);
      } else {
        setSuggestionIndex((i) => (i + 1) % pathSuggestions.length);
      }
      setAddPath(pathSuggestions[suggestionIndex] + "/");
    }

    if (viewMode === "marketplace-form") {
      // Tab to switch between name and path fields, or cycle suggestions
      if (key.tab) {
        if (marketplaceActiveField === "path" && pathSuggestions.length > 0) {
          // Cycle through path suggestions
          if (key.shift) {
            setSuggestionIndex((i) => (i - 1 + pathSuggestions.length) % pathSuggestions.length);
          } else {
            setSuggestionIndex((i) => (i + 1) % pathSuggestions.length);
          }
          setMarketplacePath(pathSuggestions[suggestionIndex] + "/");
        } else {
          // Switch between fields
          setMarketplaceActiveField(marketplaceActiveField === "name" ? "path" : "name");
        }
      }
    }
  }, { isActive: viewMode === "add" || viewMode === "marketplace-form" });

  useInput((input, key) => {
    if (viewMode === "detail" || viewMode === "add" || viewMode === "marketplace-form") return;

    // Global exit: Ctrl+C or Ctrl+X
    if (key.ctrl && (input === 'c' || input === 'x')) {
      onExit?.();
      return;
    }

    if (key.escape || input === "q") {
      onBack();
      return;
    }

    // Tab switching with Tab key or 1/2/3 keys
    // TAB = forward, Shift+TAB = backward
    if (key.tab || input === "1" || input === "2" || input === "3") {
      if (key.tab) {
        const tabs: TabMode[] = ["installed", "discover", "marketplace"];
        const currentIdx = tabs.indexOf(tabMode);
        if (key.shift) {
          // Backward
          setTabMode(tabs[(currentIdx - 1 + tabs.length) % tabs.length]);
        } else {
          // Forward
          setTabMode(tabs[(currentIdx + 1) % tabs.length]);
        }
      } else if (input === "1") {
        setTabMode("installed");
      } else if (input === "2") {
        setTabMode("discover");
      } else if (input === "3") {
        setTabMode("marketplace");
      }
      setSelectedIndex(0);
      setDiscoverIndex(0);
      setSourceIndex(0);
      setConfirmRemove(false);
      return;
    }

    if (tabMode === "installed") {
      // Installed tab navigation
      if (key.upArrow || input === "k") {
        setSelectedIndex((p) => Math.max(0, p - 1));
      } else if (key.downArrow || input === "j") {
        setSelectedIndex((p) => Math.min(nodes.length - 1, p + 1));
      }

      const node = nodes[selectedIndex];

      if (key.return && node) {
        setSelectedNode(node);
        setViewMode("detail");
      } else if ((input === "s" || input === " ") && node) {
        // Start/stop only if installed in systemd (not "not-installed" status)
        if (node.serviceStatus === "not-installed") {
          setMessage("Enable service first (press enter for details)");
          setTimeout(() => setMessage(null), 2000);
        } else if (node.serviceStatus === "running") {
          handleStopNode(node);
        } else if (node.isBuilt) {
          handleStartNode(node);
        } else {
          setMessage("Build first (press enter for details)");
          setTimeout(() => setMessage(null), 2000);
        }
      }
    } else if (tabMode === "discover") {
      // Discover tab navigation
      if (key.upArrow || input === "k") {
        setDiscoverIndex((p) => Math.max(0, p - 1));
      } else if (key.downArrow || input === "j") {
        setDiscoverIndex((p) => Math.min(discoverableNodes.length - 1, p + 1));
      }

      const discoverNode = discoverableNodes[discoverIndex];

      if ((key.return || input === "a") && discoverNode) {
        // Add the discovered node
        handleAddDiscoveredNode(discoverNode);
      }
    } else if (tabMode === "marketplace") {
      // Marketplace tab navigation
      if (key.upArrow || input === "k") {
        setSourceIndex((p) => Math.max(0, p - 1));
        setConfirmRemove(false);
      } else if (key.downArrow || input === "j") {
        setSourceIndex((p) => Math.min(sources.length - 1, p + 1));
        setConfirmRemove(false);
      }

      const source = sources[sourceIndex];

      if (input === "a") {
        // Add new marketplace entry
        setViewMode("marketplace-form");
        setMarketplaceName("");
        setMarketplacePath("");
        setMarketplaceEditPath(null);
        setMarketplaceActiveField("name");
      } else if (key.return && source) {
        // Edit selected marketplace entry
        setViewMode("marketplace-form");
        setMarketplaceName(source.name);
        setMarketplacePath(source.path);
        setMarketplaceEditPath(source.path);
        setMarketplaceActiveField("name");
      } else if (input === "e" && source && !source.enabled) {
        // Enable
        const result = toggleSource(source.path);
        if (result.success) {
          setMessage(`Enabled: ${source.name}`);
        } else {
          setMessage("Error: " + result.error);
        }
        loadSources();
        setTimeout(() => setMessage(null), 2000);
      } else if (input === "d" && source && source.enabled) {
        // Disable
        const result = toggleSource(source.path);
        if (result.success) {
          setMessage(`Disabled: ${source.name}`);
        } else {
          setMessage("Error: " + result.error);
        }
        loadSources();
        setTimeout(() => setMessage(null), 2000);
      } else if (input === "r" && source) {
        // Remove source
        if (confirmRemove) {
          const result = removeSource(source.path);
          setMessage(result.success ? "Removed: " + source.name : "Error: " + result.error);
          loadSources();
          setConfirmRemove(false);
        } else {
          setConfirmRemove(true);
          setMessage("Press [r] again to confirm removal");
        }
        setTimeout(() => {
          setConfirmRemove(false);
          setMessage(null);
        }, 3000);
      }
    }
  }, { isActive: viewMode === "list" });

  const showMessageThenClear = (msg: string, duration = 2000) => {
    setMessage(msg);
    setTimeout(() => setMessage(null), duration);
  };

  const formatError = (err: unknown): string =>
    err instanceof Error ? err.message : String(err);

  const executeDaemonCommand = async (
    action: () => Promise<{ success: boolean; message: string }>,
    successPrefix: string,
    nodeName: string
  ): Promise<void> => {
    if (!daemonAvailable) {
      showMessageThenClear("Daemon not available");
      return;
    }
    try {
      const result = await action();
      showMessageThenClear(result.success ? `${successPrefix}: ${nodeName}` : `Error: ${result.message}`);
    } catch (err) {
      showMessageThenClear(`Error: ${formatError(err)}`);
    }
    loadNodes();
  };

  const handleStartNode = (node: NodeInfo): Promise<void> =>
    executeDaemonCommand(
      () => daemonClient.current.startNode(node.manifest.name),
      "Started",
      node.manifest.name
    );

  const handleStopNode = (node: NodeInfo): Promise<void> =>
    executeDaemonCommand(
      () => daemonClient.current.stopNode(node.manifest.name),
      "Stopped",
      node.manifest.name
    );

  const handleInstallNode = (node: NodeInfo): Promise<void> =>
    executeDaemonCommand(
      () => daemonClient.current.installNode(node.manifest.name),
      "Installed",
      node.manifest.name
    );

  const handleRemoveNode = async (node: NodeInfo): Promise<void> => {
    if (daemonAvailable) {
      await executeDaemonCommand(
        () => daemonClient.current.removeNode(node.manifest.name),
        "Removed",
        node.manifest.name
      );
    } else {
      const result = unregisterNode(node.path);
      showMessageThenClear(result.success ? `Removed: ${node.manifest.name}` : `Error: ${result.error}`);
      loadNodes();
    }
  };

  const handleAddDiscoveredNode = async (node: DiscoverableNode) => {
    if (daemonAvailable) {
      try {
        const result = await daemonClient.current.addNode(node.path);
        setMessage(result.success ? "Added: " + node.manifest.name : "Error: " + result.message);
        if (result.success) {
          setTabMode("installed");
        }
      } catch (err) {
        setMessage("Error: " + (err instanceof Error ? err.message : String(err)));
      }
    } else {
      // Fallback to local registry
      const result = registerNode(node.path);
      if (result.success) {
        setMessage("Added: " + node.manifest.name);
        setTabMode("installed");
      } else {
        setMessage("Error: " + result.error);
      }
    }
    loadNodes();
    setTimeout(() => setMessage(null), 2000);
  };

  // Handle marketplace path input change (for suggestions)
  const handleMarketplacePathChange = (value: string) => {
    setMarketplacePath(value);
    // Only show path suggestions for local paths
    if (value.startsWith("/") || value.startsWith("~") || value.startsWith(".")) {
      setPathSuggestions(getPathSuggestions(value));
    } else {
      setPathSuggestions([]);
    }
    setSuggestionIndex(0);
  };

  // Handle marketplace form submit
  const handleMarketplaceSubmit = () => {
    const name = marketplaceName.trim();
    const path = marketplacePath.trim();

    if (!name) {
      setMessage("Error: Name cannot be empty");
      setTimeout(() => setMessage(null), 2000);
      return;
    }

    if (!path) {
      setMessage("Error: Path cannot be empty");
      setTimeout(() => setMessage(null), 2000);
      return;
    }

    // Check for git paths (not supported yet)
    if (path.includes("github.com") || path.startsWith("git@") || path.endsWith(".git")) {
      setMessage("Git sources coming soon! Use local paths for now.");
      setTimeout(() => setMessage(null), 3000);
      return;
    }

    // Expand path
    let expandedPath = path;
    if (expandedPath.startsWith("~")) {
      expandedPath = join(homedir(), expandedPath.slice(1));
    } else if (expandedPath.startsWith("./")) {
      expandedPath = resolve(process.cwd(), expandedPath);
    }

    let result;
    if (marketplaceEditPath) {
      // Edit mode - update existing
      result = updateSource(marketplaceEditPath, name, expandedPath);
      if (result.success) {
        setMessage("Updated: " + name);
      }
    } else {
      // Add mode - create new
      result = addSource(name, expandedPath, "local");
      if (result.success) {
        setMessage("Added: " + name);
      }
    }

    if (!result.success) {
      setMessage("Error: " + result.error);
    }

    setViewMode("list");
    setMarketplaceName("");
    setMarketplacePath("");
    setMarketplaceEditPath(null);
    setMarketplaceActiveField("name");
    setPathSuggestions([]);
    loadSources();
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


  const handleNextNode = () => {
    const nextIndex = (selectedIndex + 1) % nodes.length;
    const node = nodes[nextIndex];
    if (node) {
      setSelectedIndex(nextIndex);
      setSelectedNode(node);
    }
  };

  const handlePrevNode = () => {
    const prevIndex = (selectedIndex - 1 + nodes.length) % nodes.length;
    const node = nodes[prevIndex];
    if (node) {
      setSelectedIndex(prevIndex);
      setSelectedNode(node);
    }
  };

  // Get current list item count for display
  const getListInfo = () => {
    if (tabMode === "installed") return `${nodes.length} registered`;
    if (tabMode === "discover") return `${discoverableNodes.length} available`;
    return `${sources.length} entries`;
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
        onUninstall={() => {
          // Refresh nodes list after uninstall
          loadNodes();
          // Reset selection if needed
          if (selectedIndex >= nodes.length - 1) {
            setSelectedIndex(Math.max(0, nodes.length - 2));
          }
        }}
        onExit={onExit}
        exitWarning={exitWarning}
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

  // Marketplace add/edit form view
  if (viewMode === "marketplace-form") {
    const isEditing = marketplaceEditPath !== null;
    return (
      <Box flexDirection="column" padding={0}>
        <Box borderStyle="round" borderColor="#4ECDC4" paddingX={1} justifyContent="space-between">
          <Text color="#4ECDC4" bold>{isEditing ? "Edit Marketplace Entry" : "Add Marketplace Entry"}</Text>
          <Text color="#888">
            <Text color="#4ECDC4">[tab]</Text> switch field
            <Text color="#4ECDC4">[enter]</Text> save
            <Text color="#4ECDC4">[esc]</Text> cancel
          </Text>
        </Box>

        <Box marginX={1} marginTop={1} flexDirection="column">
          {/* Name field */}
          <Box>
            <Text color={marketplaceActiveField === "name" ? "#4ECDC4" : "#666"}>
              {marketplaceActiveField === "name" ? "❯ " : "  "}Name:
            </Text>
            {marketplaceActiveField === "name" ? (
              <TextInput
                value={marketplaceName}
                onChange={setMarketplaceName}
                onSubmit={() => setMarketplaceActiveField("path")}
                placeholder="My Nodes"
              />
            ) : (
              <Text color="#CCC">{marketplaceName || "(empty)"}</Text>
            )}
          </Box>

          {/* Path field */}
          <Box marginTop={1}>
            <Text color={marketplaceActiveField === "path" ? "#4ECDC4" : "#666"}>
              {marketplaceActiveField === "path" ? "❯ " : "  "}Path:
            </Text>
            {marketplaceActiveField === "path" ? (
              <TextInput
                value={marketplacePath}
                onChange={handleMarketplacePathChange}
                onSubmit={handleMarketplaceSubmit}
                placeholder="~/path/to/nodes or /absolute/path"
              />
            ) : (
              <Text color="#CCC">{marketplacePath || "(empty)"}</Text>
            )}
          </Box>

          {/* Path suggestions */}
          {marketplaceActiveField === "path" && pathSuggestions.length > 0 && (
            <Box flexDirection="column" marginTop={1} marginLeft={2}>
              <Text color="#888">Suggestions (tab to cycle):</Text>
              {pathSuggestions.map((s, i) => (
                <Text key={s} color={i === suggestionIndex ? "#4ECDC4" : "#666"}>
                  {i === suggestionIndex ? "❯ " : "  "}{s}
                </Text>
              ))}
            </Box>
          )}

          {/* Help text */}
          <Box marginTop={2} flexDirection="column">
            <Text color="#666">Marketplace entries are directories containing nodes.</Text>
            <Text color="#666">They will be scanned in the Discover tab.</Text>
          </Box>
        </Box>
      </Box>
    );
  }

  // Get derived values for rendering
  const currentNode = nodes[selectedIndex];
  const currentSource = sources[sourceIndex];
  const discoverPaths = getDiscoverPaths();


  // Main list view - all JSX inline to avoid component recreation
  return (
    <Box flexDirection="column" padding={0}>
      {/* Header - static like Services to prevent render artifacts */}
      <Box borderStyle="round" borderColor="#4ECDC4" paddingX={1} justifyContent="space-between">
        <Text color="#4ECDC4" bold>Nodes</Text>
        <Text color="#888">esc/q back</Text>
      </Box>

      {/* Installed Tab Content */}
      {tabMode === "installed" && (
        <>
          <Box flexDirection="column" borderStyle="single" borderColor="#444" marginTop={0}>
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
                        {isSelected ? "❯ " : "  "}{n.manifest.name}
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

          {currentNode && (
            <Box marginX={1} marginTop={1}>
              <Text color="#555">{currentNode.path}</Text>
            </Box>
          )}

          <Box marginX={1} marginTop={1}>
            <Text color="#666">
              <Text color="#4ECDC4">s</Text> start/stop •{" "}
              <Text color="#4ECDC4">enter</Text> details •{" "}
              <Text color="#4ECDC4">↑↓</Text> select •{" "}
              <Text color="#4ECDC4">esc/q</Text> back
            </Text>
          </Box>
        </>
      )}

      {/* Discover Tab Content */}
      {tabMode === "discover" && (
        <>
          <Box flexDirection="column" borderStyle="single" borderColor="#444" marginTop={1}>
            <Box paddingX={1} borderBottom borderColor="#444">
              <Box width="25%"><Text color="#4ECDC4" bold>Name</Text></Box>
              <Box width="10%"><Text color="#4ECDC4" bold>Version</Text></Box>
              <Box width="10%"><Text color="#4ECDC4" bold>Type</Text></Box>
              <Box width="15%"><Text color="#4ECDC4" bold>Source</Text></Box>
              <Box width="40%"><Text color="#4ECDC4" bold>Path</Text></Box>
            </Box>

            {discoverableNodes.length === 0 ? (
              <Box paddingX={1} paddingY={1} flexDirection="column">
                <Text color="#888">No discoverable nodes found.</Text>
                <Text color="#666">Add entries in [3] Marketplace tab to discover more nodes.</Text>
              </Box>
            ) : (
              discoverableNodes.map((n, index) => {
                const isSelected = index === discoverIndex;
                return (
                  <Box key={n.path} paddingX={1}>
                    <Box width="25%">
                      <Text color={isSelected ? "#4ECDC4" : "#CCC"}>
                        {isSelected ? "\u276f " : "  "}{n.manifest.name}
                      </Text>
                    </Box>
                    <Box width="10%"><Text color="#95E1D3">{n.manifest.version}</Text></Box>
                    <Box width="10%">
                      <Text color={n.manifest.type === "rust" ? "#FFD93D" : "#4ECDC4"}>{n.manifest.type}</Text>
                    </Box>
                    <Box width="15%"><Text color="#888">{n.source}</Text></Box>
                    <Box width="40%">
                      <Text color="#666">{n.path.length > 35 ? "..." + n.path.slice(-32) : n.path}</Text>
                    </Box>
                  </Box>
                );
              })
            )}
          </Box>

          <Box marginX={1} marginTop={1}>
            <Text color="#666">
              <Text color="#4ECDC4">[enter]</Text> or <Text color="#4ECDC4">[a]</Text> to add selected node
            </Text>
          </Box>

          <Box marginX={1} marginTop={1}>
            <Text color="#666">
              Scanning: {discoverPaths.filter(p => existsSync(p)).join(", ") || "no sources configured"}
            </Text>
          </Box>
        </>
      )}

      {/* Marketplace Tab Content */}
      {tabMode === "marketplace" && (
        <>
          <Box flexDirection="column" borderStyle="single" borderColor="#444" marginTop={1}>
            <Box paddingX={1} borderBottom borderColor="#444">
              <Box width={3}><Text color="#4ECDC4" bold>On</Text></Box>
              <Box width="20%"><Text color="#4ECDC4" bold>Name</Text></Box>
              <Box width="10%"><Text color="#4ECDC4" bold>Type</Text></Box>
              <Box width="67%"><Text color="#4ECDC4" bold>Path</Text></Box>
            </Box>

            {sources.length === 0 ? (
              <Box paddingX={1} paddingY={1}>
                <Text color="#888">No marketplace entries. Press [a] to add one.</Text>
              </Box>
            ) : (
              sources.map((s, index) => {
                const isSelected = index === sourceIndex;
                const pathExists = existsSync(s.path);
                return (
                  <Box key={s.path} paddingX={1}>
                    <Box width={3}>
                      <Text color={s.enabled ? "#95E1D3" : "#666"}>{s.enabled ? "●" : "○"}</Text>
                    </Box>
                    <Box width="20%">
                      <Text color={isSelected ? "#4ECDC4" : "#CCC"}>
                        {isSelected ? "\u276f " : "  "}{s.name}
                      </Text>
                    </Box>
                    <Box width="10%">
                      <Text color={s.type === "git" ? "#FF6B6B" : "#95E1D3"}>{s.type}</Text>
                    </Box>
                    <Box width="67%">
                      <Text color={pathExists ? "#666" : "#FF6B6B"}>
                        {s.path.length > 50 ? "..." + s.path.slice(-47) : s.path}
                        {!pathExists && " (not found)"}
                      </Text>
                    </Box>
                  </Box>
                );
              })
            )}
          </Box>

          <Box marginX={1} marginTop={1}>
            <Text color="#666">
              <Text color="#4ECDC4">[a]</Text>dd
              {currentSource && (
                <>
                  {"  "}<Text color="#4ECDC4">[enter]</Text> edit
                  {"  "}{currentSource.enabled ? (
                    <><Text color="#4ECDC4">[d]</Text>isable</>
                  ) : (
                    <><Text color="#4ECDC4">[e]</Text>nable</>
                  )}
                  {"  "}{confirmRemove ? <Text color="#FF6B6B">[r] CONFIRM?</Text> : <><Text color="#4ECDC4">[r]</Text>emove</>}
                </>
              )}
            </Text>
          </Box>

          <Box marginX={1} marginTop={1}>
            <Text color="#666">
              Marketplace entries are directories containing nodes. Enabled entries are scanned in Discover.
            </Text>
          </Box>
        </>
      )}

      {message && <Box marginX={1} marginTop={1}><Text color="#FFD93D">{message}</Text></Box>}
      {exitWarning && <Box marginX={1} marginTop={1}><Text color="#FF6B6B">Press Ctrl+C again to exit</Text></Box>}
    </Box>
  );
};

export default NodesView;
