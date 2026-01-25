import React, { useState, useEffect, useRef } from "react";
import { Box, Text, useInput } from "ink";
import { spawn, ChildProcess } from "child_process";
import { NodeManifest, getServiceName } from "./config.js";
import { DaemonClient, NodeState } from "./daemon-client.js";

type ServiceStatus = "stopped" | "running" | "failed" | "not-installed" | "building" | "unknown";
type BuildStatus = "idle" | "building" | "cleaning";

export interface BuildState {
  status: BuildStatus;
  output: string[];
  process: ChildProcess | null;
}

interface NodeDetailViewProps {
  nodePath: string;
  manifest: NodeManifest;
  buildState: BuildState;
  updateBuildState: (update: Partial<BuildState>) => void;
  onBack: () => void;
  onNext: () => void;
  onPrev: () => void;
  currentIndex: number;
  totalNodes: number;
  daemonAvailable: boolean;
  onUninstall?: () => void; // Called after full uninstall (systemd + registry)
  onExit?: () => void; // Global exit callback
  exitWarning?: boolean;
}

// Flower-like spinner with whimsical verbs
const FLOWER_FRAMES = ["✻", "✼", "✽", "✾", "✿", "❀", "❁", "✿", "✾", "✽", "✼"];
const VERBS_BY_TYPE: Record<string, string[]> = {
  build: ["Compiling", "Assembling", "Forging", "Crafting", "Conjuring", "Materializing", "Synthesizing", "Weaving"],
  clean: ["Tidying", "Sweeping", "Purging", "Clearing", "Decluttering", "Vanishing", "Dissolving"],
  logs: ["Observing", "Watching", "Monitoring", "Scrutinizing", "Surveying", "Peering"],
};

function getRandomVerb(type: string): string {
  const verbs = VERBS_BY_TYPE[type] ?? VERBS_BY_TYPE.build;
  return verbs[Math.floor(Math.random() * verbs.length)];
}

const FlowerSpinner: React.FC<{ type?: "build" | "clean" | "logs" }> = ({ type = "build" }) => {
  const [frame, setFrame] = useState(0);
  const [verb, setVerb] = useState(() => getRandomVerb(type));

  useEffect(() => {
    const interval = setInterval(() => {
      setFrame((f) => (f + 1) % FLOWER_FRAMES.length);
    }, 150);
    return () => clearInterval(interval);
  }, []);

  useEffect(() => {
    const interval = setInterval(() => {
      setVerb(getRandomVerb(type));
    }, 4000);
    return () => clearInterval(interval);
  }, [type]);

  return <Text color="#FFD93D">{FLOWER_FRAMES[frame]} {verb}...</Text>;
};

const NodeDetailView: React.FC<NodeDetailViewProps> = ({
  nodePath,
  manifest,
  buildState,
  updateBuildState,
  onBack,
  onNext,
  onPrev,
  currentIndex,
  totalNodes,
  daemonAvailable,
  onUninstall,
  onExit,
  exitWarning,
}) => {
  const [nodeState, setNodeState] = useState<NodeState | null>(null);
  const [message, setMessage] = useState<string | null>(null);
  const [logs, setLogs] = useState<string[]>([]);
  const [showLogs, setShowLogs] = useState(false);
  const [systemdInfo, setSystemdInfo] = useState<string[]>([]);
  const [confirmUninstall, setConfirmUninstall] = useState(false);
  const [confirmClean, setConfirmClean] = useState(false);
  const logsRef = useRef<ReturnType<typeof spawn> | null>(null);
  const daemonClient = useRef(new DaemonClient());

  const serviceName = getServiceName(manifest.name);
  const { status: buildStatus, output: buildOutput } = buildState;

  // Derive status from nodeState or buildState
  const serviceStatus: ServiceStatus = nodeState
    ? (nodeState.status as ServiceStatus)
    : "unknown";
  const isBuilt = nodeState?.is_built ?? false;

  // Fetch node state from daemon
  const refreshStatus = async () => {
    if (daemonAvailable) {
      try {
        const state = await daemonClient.current.getNode(manifest.name);
        setNodeState(state);
      } catch {
        setNodeState(null);
      }
    }
  };

  // Fetch systemd service info
  const refreshSystemdInfo = () => {
    const child = spawn("systemctl", ["--user", "status", serviceName, "--no-pager"], {
      stdio: ["ignore", "pipe", "pipe"],
    });

    let output = "";
    child.stdout?.on("data", (data) => {
      output += data.toString();
    });
    child.stderr?.on("data", (data) => {
      output += data.toString();
    });
    child.on("error", (err: Error) => {
      setSystemdInfo([`[error] ${err.message}`]);
    });
    child.on("close", () => {
      const lines = output.split("\n").filter((l: string) => l.trim());
      setSystemdInfo(lines);
    });
  };

  // Track the current node for the log stream
  const currentLogNode = useRef<string | null>(null);

  // Refresh status and systemd info periodically
  useEffect(() => {
    refreshStatus();
    refreshSystemdInfo();
    const interval = setInterval(() => {
      refreshStatus();
      if (!showLogs) {
        refreshSystemdInfo();
      }
    }, 2000);
    return () => {
      clearInterval(interval);
    };
  }, [manifest.name, daemonAvailable, showLogs]);

  // Cleanup log stream on unmount
  useEffect(() => {
    return () => {
      if (logsRef.current) {
        logsRef.current.kill();
        logsRef.current = null;
      }
    };
  }, []);

  // When node changes while logs are showing, restart the log stream
  useEffect(() => {
    if (showLogs && currentLogNode.current !== manifest.name) {
      // Restart log stream for the new node
      startLogStream();
    }
  }, [manifest.name, showLogs]);

  const startLogStream = () => {
    // Kill any existing stream
    if (logsRef.current) {
      logsRef.current.kill();
      logsRef.current = null;
    }

    currentLogNode.current = manifest.name;
    setLogs([`=== Logs for ${manifest.name} ===`]);
    setShowLogs(true);

    // Use _SYSTEMD_USER_UNIT filter for user services (logs are in system journal)
    const currentServiceName = getServiceName(manifest.name);
    const child = spawn("journalctl", [`_SYSTEMD_USER_UNIT=${currentServiceName}`, "-f", "-n", "30", "--no-pager", "-o", "cat"], {
      stdio: ["ignore", "pipe", "pipe"],
    });

    logsRef.current = child;

    child.stdout?.on("data", (data) => {
      const lines = data.toString().split("\n").filter((l: string) => l.trim());
      if (lines.length > 0) {
        setLogs((prev) => [...prev.slice(-100), ...lines]);
      }
    });

    child.stderr?.on("data", (data) => {
      const lines = data.toString().split("\n").filter((l: string) => l.trim());
      if (lines.length > 0) {
        setLogs((prev) => [...prev.slice(-100), ...lines.map((l: string) => `[err] ${l}`)]);
      }
    });

    child.on("error", (err) => {
      setLogs((prev) => [...prev, `[error] ${err.message}`]);
    });

    child.on("close", (code) => {
      if (code !== 0 && code !== null) {
        setLogs((prev) => [...prev, `[stream ended: code ${code}]`]);
      }
    });
  };

  const stopLogStream = () => {
    if (logsRef.current) {
      logsRef.current.kill();
      logsRef.current = null;
    }
    currentLogNode.current = null;
    setShowLogs(false);
  };

  // Command handlers using daemon
  const handleCommand = async (command: string, successMsg: string) => {
    if (!daemonAvailable) {
      setMessage("Daemon not available");
      setTimeout(() => setMessage(null), 2000);
      return;
    }

    try {
      const result = await daemonClient.current.executeCommand(manifest.name, command);
      setMessage(result.success ? successMsg : `Error: ${result.message}`);
      refreshStatus();
    } catch (err) {
      setMessage("Error: " + (err instanceof Error ? err.message : String(err)));
    }
    setTimeout(() => setMessage(null), 3000);
  };

  // Full uninstall: disable systemd service + remove from registry
  const handleFullUninstall = async () => {
    if (!daemonAvailable) {
      setMessage("Daemon not available");
      setTimeout(() => setMessage(null), 2000);
      return;
    }

    try {
      setMessage("Uninstalling...");

      // Step 1: Uninstall from systemd (if installed)
      if (serviceStatus !== "not-installed") {
        const uninstallResult = await daemonClient.current.uninstallNode(manifest.name);
        if (!uninstallResult.success) {
          setMessage(`Error uninstalling service: ${uninstallResult.message}`);
          setTimeout(() => setMessage(null), 3000);
          return;
        }
      }

      // Step 2: Remove from registry
      const removeResult = await daemonClient.current.removeNode(manifest.name);
      if (!removeResult.success) {
        setMessage(`Error removing from registry: ${removeResult.message}`);
        setTimeout(() => setMessage(null), 3000);
        return;
      }

      setMessage("Uninstalled successfully");
      setTimeout(() => {
        setMessage(null);
        onUninstall?.();
        onBack();
      }, 1500);
    } catch (err) {
      setMessage("Error: " + (err instanceof Error ? err.message : String(err)));
      setTimeout(() => setMessage(null), 3000);
    }
  };

  // Shared logic for running local build/clean commands
  const runLocalCommand = (cmd: string, actionName: string, status: BuildStatus): void => {
    const child = spawn("sh", ["-c", cmd], {
      cwd: nodePath,
      stdio: "pipe",
      detached: true,
    });

    updateBuildState({ status, output: [], process: child });
    setMessage(null);

    const handleOutput = (data: Buffer): void => {
      const lines = data.toString().split("\n").filter((l: string) => l.trim());
      updateBuildState({ output: [...buildOutput.slice(-20), ...lines] });
    };

    child.stdout?.on("data", handleOutput);
    child.stderr?.on("data", handleOutput);

    child.on("close", (code) => {
      refreshStatus();
      const success = code === 0;
      const suffix = success ? "completed" : `failed (exit ${code})`;
      updateBuildState({
        status: "idle",
        process: null,
        output: [...buildOutput, `--- ${actionName} ${suffix} ---`],
      });
      setMessage(`${actionName} ${suffix}`);
      setTimeout(() => setMessage(null), 3000);
    });

    child.on("error", (err) => {
      updateBuildState({
        status: "idle",
        process: null,
        output: [...buildOutput, `--- ${actionName} error: ${err.message} ---`],
      });
      setMessage(`${actionName} error: ${err.message}`);
      setTimeout(() => setMessage(null), 5000);
    });
  };

  const runBuild = async (): Promise<void> => {
    if (buildStatus !== "idle") return;

    if (daemonAvailable) {
      try {
        updateBuildState({ status: "building", output: [] });
        const result = await daemonClient.current.buildNode(manifest.name);
        setMessage(result.success ? "Build started" : `Error: ${result.message}`);
      } catch (err) {
        updateBuildState({ status: "idle" });
        setMessage("Error: " + (err instanceof Error ? err.message : String(err)));
      }
      setTimeout(() => setMessage(null), 3000);
    } else if (manifest.build) {
      runLocalCommand(manifest.build, "Build", "building");
    }
  };

  const runClean = async (): Promise<void> => {
    if (buildStatus !== "idle") return;

    if (daemonAvailable) {
      try {
        updateBuildState({ status: "cleaning", output: [] });
        const result = await daemonClient.current.cleanNode(manifest.name);
        setMessage(result.success ? "Clean started" : `Error: ${result.message}`);
      } catch (err) {
        updateBuildState({ status: "idle" });
        setMessage("Error: " + (err instanceof Error ? err.message : String(err)));
      }
      setTimeout(() => setMessage(null), 3000);
    } else {
      runLocalCommand("pixi run clean", "Clean", "cleaning");
    }
  };

  useInput((input, key) => {
    // Global exit: Ctrl+C or Ctrl+X
    if (key.ctrl && (input === 'c' || input === 'x')) {
      onExit?.();
      return;
    }

    if (key.escape) {
      if (showLogs) {
        // Exit logs mode first
        stopLogStream();
        return;
      }
      onBack();
      return;
    }

    if (input === "e" && serviceStatus === "not-installed") {
      // Enable = install to systemd
      handleCommand("install", "Service enabled");
    } else if (input === "d" && serviceStatus !== "not-installed" && serviceStatus !== "unknown") {
      // Disable = uninstall from systemd
      handleCommand("uninstall", "Service disabled");
    } else if (input === "u") {
      // Uninstall = disable systemd + remove from registry
      if (confirmUninstall) {
        handleFullUninstall();
        setConfirmUninstall(false);
      } else {
        setConfirmUninstall(true);
        setMessage("⚠ This will remove the node from registry. Press [u] to confirm.");
        setTimeout(() => {
          setConfirmUninstall(false);
          setMessage(null);
        }, 3000);
      }
    } else if (input === "s" && serviceStatus === "running") {
      handleCommand("stop", "Service stopped");
      stopLogStream();
    } else if (input === "s" && (serviceStatus === "stopped" || serviceStatus === "failed") && isBuilt) {
      handleCommand("start", "Service started");
    } else if (input === "s" && (serviceStatus === "stopped" || serviceStatus === "failed") && !isBuilt) {
      setMessage("Cannot start: node not built");
      setTimeout(() => setMessage(null), 2000);
    } else if (input === "b" && manifest.build && buildStatus === "idle") {
      runBuild();
    } else if (input === "c" && buildStatus === "idle") {
      if (confirmClean) {
        setConfirmClean(false);
        runClean();
      } else {
        setConfirmClean(true);
        setMessage("⚠ This will remove build artifacts. Press [c] to confirm.");
        setTimeout(() => {
          setConfirmClean(false);
          setMessage(null);
        }, 3000);
      }
    } else if (input === "x" && buildStatus !== "idle") {
      // Cancel build/clean - kill process group (local only)
      if (buildState.process && buildState.process.pid) {
        try {
          process.kill(-buildState.process.pid, "SIGKILL");
        } catch {
          buildState.process.kill("SIGKILL");
        }
      }
      updateBuildState({ status: "idle", process: null, output: [...buildOutput, "--- Cancelled by user ---"] });
      setMessage("Cancelled");
      setTimeout(() => setMessage(null), 2000);
    } else if (input === "l" && serviceStatus !== "not-installed" && serviceStatus !== "unknown") {
      if (showLogs) {
        stopLogStream();
      } else {
        startLogStream();
      }
    }

    // TAB navigation between nodes (works in logs mode too - logs auto-restart)
    if (key.tab) {
      if (key.shift) {
        onPrev();
      } else {
        onNext();
      }
    }
  });

  const StatusBadge: React.FC<{ status: ServiceStatus }> = ({ status }) => {
    const cfg: Record<ServiceStatus, { color: string; label: string }> = {
      running: { color: "#95E1D3", label: "RUNNING" },
      stopped: { color: "#888", label: "STOPPED" },
      failed: { color: "#FF6B6B", label: "FAILED" },
      "not-installed": { color: "#666", label: "NOT INSTALLED" },
      building: { color: "#FFD93D", label: "BUILDING" },
      unknown: { color: "#666", label: "UNKNOWN" },
    };
    const { color, label } = cfg[status];
    return <Text color={color} bold>{label}</Text>;
  };

  // Use build output from daemon if available
  const displayOutput = nodeState?.build_output?.length
    ? nodeState.build_output
    : buildOutput;

  // Full-screen logs mode
  if (showLogs) {
    return (
      <Box flexDirection="column" padding={1}>
        <Box borderStyle="round" borderColor="#95E1D3" paddingX={1} justifyContent="space-between">
          <Box>
            <FlowerSpinner type="logs" />
            <Text color="#95E1D3" bold> {manifest.name}</Text>
            <Text color="#666"> ({currentIndex + 1}/{totalNodes})</Text>
          </Box>
          <Text color="#888">
            <Text color="#4ECDC4">[tab]</Text> next node
            {"  "}<Text color="#4ECDC4">[esc]</Text> exit
          </Text>
        </Box>

        <Box flexDirection="column" marginTop={1} borderStyle="single" borderColor="#444" paddingX={1} paddingY={1} height={20}>
          {logs.length > 0 ? (
            logs.slice(-18).map((line, i) => (
              <Text key={i} color="#CCC" wrap="truncate">{line}</Text>
            ))
          ) : (
            <Text color="#666">Waiting for logs...</Text>
          )}
        </Box>

        {message && (
          <Box marginX={1} marginTop={1}>
            <Text color="#FFD93D">{message}</Text>
          </Box>
        )}
        {exitWarning && (
          <Box marginX={1} marginTop={1}>
            <Text color="#FF6B6B">Press Ctrl+C again to exit</Text>
          </Box>
        )}
      </Box>
    );
  }

  // Normal detail view
  return (
    <Box flexDirection="column" padding={1}>
      <Box borderStyle="round" borderColor="#4ECDC4" paddingX={1} justifyContent="space-between">
        <Text color="#4ECDC4" bold>{manifest.name}</Text>
        <Text color="#888">
          <Text color="#666">{currentIndex + 1}/{totalNodes}</Text>
          {" "}<Text color="#4ECDC4">[tab]</Text> next
          {" "}<Text color="#4ECDC4">[shift+tab]</Text> prev
          {" "}<Text color="#4ECDC4">[esc]</Text> back
        </Text>
      </Box>

      {message && (
        <Box marginX={1} marginTop={1}>
          <Text color={message.includes("failed") || message.includes("error") || message.includes("Error") ? "#FF6B6B" : "#FFD93D"}>{message}</Text>
        </Box>
      )}

      <Box flexDirection="row" marginTop={1}>
        {/* Left column - Info and Actions */}
        <Box flexDirection="column" width="25%" paddingRight={1}>
          <Box flexDirection="column" paddingX={1}>
            <Box><Text color="#888">Version:     </Text><Text color="#95E1D3">{manifest.version}</Text></Box>
            <Box><Text color="#888">Type:        </Text><Text color={manifest.type === "rust" ? "#FFD93D" : "#4ECDC4"}>{manifest.type}</Text></Box>
            <Box><Text color="#888">Description: </Text><Text color="#CCC">{manifest.description || "-"}</Text></Box>
            <Box flexDirection="column">
              <Text color="#888">Path:</Text>
              <Text color="#CCC">  {nodePath}</Text>
            </Box>
            <Box><Text color="#888">Service:     </Text><Text color="#CCC">{serviceName}</Text></Box>
            <Box marginTop={1}><Text color="#888">Status:      </Text><StatusBadge status={serviceStatus} /></Box>
            <Box>
              <Text color="#888">Built:       </Text>
              {buildStatus !== "idle" || serviceStatus === "building" ? (
                <FlowerSpinner type={buildStatus === "building" ? "build" : "clean"} />
              ) : (
                <Text color={isBuilt ? "#95E1D3" : "#FF6B6B"} bold>{isBuilt ? "YES" : "NO"}</Text>
              )}
            </Box>
            {!daemonAvailable && (
              <Box marginTop={1}>
                <Text color="#FF6B6B">⚠ Daemon not available - limited functionality</Text>
              </Box>
            )}
          </Box>

          <Box flexDirection="column" marginTop={1} paddingX={1} borderStyle="single" borderColor="#444" paddingY={1}>
            <Text color="#4ECDC4" bold>Actions</Text>
            <Box marginTop={1} flexDirection="column">
              {daemonAvailable && (
                serviceStatus === "not-installed" ? (
                  <Text color="#666"><Text color="#4ECDC4">[e]</Text>nable service</Text>
                ) : serviceStatus !== "unknown" && (
                  <Text color="#666"><Text color="#4ECDC4">[d]</Text>isable service</Text>
                )
              )}
              {serviceStatus !== "not-installed" && serviceStatus !== "unknown" && (
                <>
                  {serviceStatus === "running" ? (
                    <Text color="#666"><Text color="#4ECDC4">[s]</Text>top</Text>
                  ) : isBuilt ? (
                    <Text color="#666"><Text color="#4ECDC4">[s]</Text>tart</Text>
                  ) : (
                    <Text color="#555">[s]tart <Text color="#FF6B6B">(build first)</Text></Text>
                  )}
                  <Text color="#666"><Text color="#4ECDC4">[l]</Text>ogs</Text>
                </>
              )}
              {manifest.build && buildStatus === "idle" && (
                <Text color="#666"><Text color="#4ECDC4">[b]</Text>uild</Text>
              )}
              {buildStatus === "idle" && (
                confirmClean ? (
                  <Text color="#FF6B6B" bold>⚠ Press [c] again to CLEAN</Text>
                ) : (
                  <Text color="#666"><Text color="#4ECDC4">[c]</Text>lean</Text>
                )
              )}
              {buildStatus !== "idle" && buildState.process && (
                <Text color="#666"><Text color="#FF6B6B">[x]</Text> cancel {buildStatus}</Text>
              )}
              {confirmUninstall ? (
                <Text color="#FF6B6B" bold>⚠ Press [u] again to UNINSTALL</Text>
              ) : (
                <Text color="#666"><Text color="#4ECDC4">[u]</Text>ninstall node</Text>
              )}
            </Box>
          </Box>
        </Box>

        {/* Right column - Systemd Info */}
        <Box flexDirection="column" width="75%" borderStyle="single" borderColor="#444" paddingX={1}>
          <Text color="#4ECDC4" bold>
            {buildStatus === "building" || serviceStatus === "building" ? (
              <FlowerSpinner type="build" />
            ) : buildStatus === "cleaning" ? (
              <FlowerSpinner type="clean" />
            ) : (
              "Systemd Status"
            )}
          </Text>
          <Box flexDirection="column" marginTop={1} height={16}>
            {buildStatus !== "idle" ? (
              displayOutput.length > 0 ? (
                displayOutput.slice(-14).map((line, i) => (
                  <Text key={i} color="#888" wrap="truncate">{line}</Text>
                ))
              ) : (
                <Text color="#666">Building...</Text>
              )
            ) : systemdInfo.length > 0 ? (
              systemdInfo.slice(0, 16).map((line, i) => (
                <Text key={i} color="#888" wrap="truncate">{line}</Text>
              ))
            ) : (
              <Text color="#666">No systemd info available</Text>
            )}
          </Box>
        </Box>
      </Box>
      {exitWarning && (
        <Box marginX={1} marginTop={1}>
          <Text color="#FF6B6B">Press Ctrl+C again to exit</Text>
        </Box>
      )}
    </Box>
  );
};

export default NodeDetailView;
