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
}

// Flower-like spinner with whimsical verbs
const FLOWER_FRAMES = ["✻", "✼", "✽", "✾", "✿", "❀", "❁", "✿", "✾", "✽", "✼"];
const BUILD_VERBS = ["Compiling", "Assembling", "Forging", "Crafting", "Conjuring", "Materializing", "Synthesizing", "Weaving"];
const CLEAN_VERBS = ["Tidying", "Sweeping", "Purging", "Clearing", "Decluttering", "Vanishing", "Dissolving"];
const LOG_VERBS = ["Observing", "Watching", "Monitoring", "Scrutinizing", "Surveying", "Peering"];

const FlowerSpinner: React.FC<{ type?: "build" | "clean" | "logs" }> = ({ type = "build" }) => {
  const [frame, setFrame] = useState(0);
  const [verb, setVerb] = useState(() => {
    const verbs = type === "build" ? BUILD_VERBS : type === "clean" ? CLEAN_VERBS : LOG_VERBS;
    return verbs[Math.floor(Math.random() * verbs.length)];
  });

  useEffect(() => {
    const interval = setInterval(() => {
      setFrame((f) => (f + 1) % FLOWER_FRAMES.length);
    }, 150);
    return () => clearInterval(interval);
  }, []);

  // Change verb occasionally
  useEffect(() => {
    const interval = setInterval(() => {
      const verbs = type === "build" ? BUILD_VERBS : type === "clean" ? CLEAN_VERBS : LOG_VERBS;
      setVerb(verbs[Math.floor(Math.random() * verbs.length)]);
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
}) => {
  const [nodeState, setNodeState] = useState<NodeState | null>(null);
  const [message, setMessage] = useState<string | null>(null);
  const [logs, setLogs] = useState<string[]>([]);
  const [showLogs, setShowLogs] = useState(false);
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
  const isEnabled = nodeState?.autostart_enabled ?? false;

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

  useEffect(() => {
    refreshStatus();
    const interval = setInterval(refreshStatus, 2000);
    return () => {
      clearInterval(interval);
      if (logsRef.current) {
        logsRef.current.kill();
      }
    };
  }, [manifest.name, daemonAvailable]);

  const startLogStream = () => {
    if (logsRef.current) {
      logsRef.current.kill();
    }
    setLogs([]);
    setShowLogs(true);

    const child = spawn("journalctl", ["--user", "-u", serviceName, "-f", "-n", "20", "--no-pager"], {
      stdio: ["ignore", "pipe", "pipe"],
    });

    logsRef.current = child;

    child.stdout?.on("data", (data) => {
      const lines = data.toString().split("\n").filter((l: string) => l.trim());
      setLogs((prev) => [...prev.slice(-50), ...lines]);
    });

    child.stderr?.on("data", (data) => {
      const lines = data.toString().split("\n").filter((l: string) => l.trim());
      setLogs((prev) => [...prev.slice(-50), ...lines]);
    });
  };

  const stopLogStream = () => {
    if (logsRef.current) {
      logsRef.current.kill();
      logsRef.current = null;
    }
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

  const runBuild = async () => {
    if (buildStatus !== "idle") return;

    if (daemonAvailable) {
      // Use daemon for build
      try {
        updateBuildState({ status: "building", output: [] });
        const result = await daemonClient.current.buildNode(manifest.name);
        setMessage(result.success ? "Build started" : `Error: ${result.message}`);
        // The daemon runs build in background, poll for updates
      } catch (err) {
        updateBuildState({ status: "idle" });
        setMessage("Error: " + (err instanceof Error ? err.message : String(err)));
      }
      setTimeout(() => setMessage(null), 3000);
    } else if (manifest.build) {
      // Fallback to local build
      const child = spawn("sh", ["-c", manifest.build], {
        cwd: nodePath,
        stdio: "pipe",
        detached: true,
      });

      updateBuildState({ status: "building", output: [], process: child });
      setMessage(null);

      const handleOutput = (data: Buffer) => {
        const lines = data.toString().split("\n").filter((l: string) => l.trim());
        updateBuildState({ output: [...buildOutput.slice(-20), ...lines] });
      };

      child.stdout?.on("data", handleOutput);
      child.stderr?.on("data", handleOutput);

      child.on("close", (code) => {
        refreshStatus();
        if (code === 0) {
          updateBuildState({ status: "idle", process: null, output: [...buildOutput, "--- Build completed successfully ---"] });
          setMessage("Build completed successfully");
        } else {
          updateBuildState({ status: "idle", process: null, output: [...buildOutput, `--- Build failed (exit code ${code}) ---`] });
          setMessage(`Build failed (exit code ${code})`);
        }
        setTimeout(() => setMessage(null), 3000);
      });

      child.on("error", (err) => {
        updateBuildState({ status: "idle", process: null, output: [...buildOutput, `--- Build error: ${err.message} ---`] });
        setMessage(`Build error: ${err.message}`);
        setTimeout(() => setMessage(null), 5000);
      });
    }
  };

  const runClean = async () => {
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
      // Fallback to local clean
      const cleanCmd = "pixi run clean";

      const child = spawn("sh", ["-c", cleanCmd], {
        cwd: nodePath,
        stdio: "pipe",
        detached: true,
      });

      updateBuildState({ status: "cleaning", output: [], process: child });
      setMessage(null);

      const handleOutput = (data: Buffer) => {
        const lines = data.toString().split("\n").filter((l: string) => l.trim());
        updateBuildState({ output: [...buildOutput.slice(-20), ...lines] });
      };

      child.stdout?.on("data", handleOutput);
      child.stderr?.on("data", handleOutput);

      child.on("close", (code) => {
        refreshStatus();
        if (code === 0) {
          updateBuildState({ status: "idle", process: null, output: [...buildOutput, "--- Clean completed ---"] });
          setMessage("Clean completed");
        } else {
          updateBuildState({ status: "idle", process: null, output: [...buildOutput, `--- Clean failed (exit ${code}) ---`] });
          setMessage(`Clean failed (exit ${code})`);
        }
        setTimeout(() => setMessage(null), 3000);
      });

      child.on("error", (err) => {
        updateBuildState({ status: "idle", process: null, output: [...buildOutput, `--- Clean error: ${err.message} ---`] });
        setMessage(`Clean error: ${err.message}`);
        setTimeout(() => setMessage(null), 5000);
      });
    }
  };

  useInput((input, key) => {
    if (key.escape) {
      if (showLogs) {
        stopLogStream();
      }
      onBack();
      return;
    }

    if (input === "i" && serviceStatus === "not-installed") {
      handleCommand("install", "Service installed");
    } else if (input === "u" && serviceStatus !== "not-installed" && serviceStatus !== "unknown") {
      if (confirmUninstall) {
        handleCommand("uninstall", "Service uninstalled");
        setConfirmUninstall(false);
      } else {
        setConfirmUninstall(true);
        setMessage("Press [u] again to confirm uninstall");
        setTimeout(() => {
          setConfirmUninstall(false);
          setMessage(null);
        }, 3000);
      }
    } else if (input === "s" && serviceStatus === "running") {
      handleCommand("stop", "Service stopped");
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
        setMessage("Press [c] again to confirm clean");
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
    } else if (input === "e" && serviceStatus !== "not-installed" && serviceStatus !== "unknown") {
      if (isEnabled) {
        handleCommand("disable_autostart", "Autostart disabled");
      } else {
        handleCommand("enable_autostart", "Autostart enabled");
      }
    }

    // TAB navigation between nodes
    if (key.tab && !showLogs) {
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
        <Box flexDirection="column" width="50%" paddingRight={1}>
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
            {serviceStatus !== "not-installed" && serviceStatus !== "unknown" && (
              <Box><Text color="#888">Autostart:   </Text><Text color={isEnabled ? "#95E1D3" : "#888"} bold>{isEnabled ? "ENABLED" : "DISABLED"}</Text></Box>
            )}
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
              {serviceStatus === "not-installed" && daemonAvailable && (
                <Text color="#666"><Text color="#4ECDC4">[i]</Text>nstall service</Text>
              )}
              {serviceStatus !== "not-installed" && serviceStatus !== "unknown" && (
                <>
                  {serviceStatus === "running" ? (
                    <Text color="#666"><Text color="#4ECDC4">[s]</Text>top service</Text>
                  ) : isBuilt ? (
                    <Text color="#666"><Text color="#4ECDC4">[s]</Text>tart service</Text>
                  ) : (
                    <Text color="#555">[s]tart service <Text color="#FF6B6B">(build first)</Text></Text>
                  )}
                  {confirmUninstall ? (
                    <Text color="#FF6B6B"><Text color="#FF6B6B" bold>[u]</Text> CONFIRM UNINSTALL?</Text>
                  ) : (
                    <Text color="#666"><Text color="#4ECDC4">[u]</Text>ninstall service</Text>
                  )}
                  <Text color="#666">
                    <Text color="#4ECDC4">[e]</Text>{isEnabled ? " disable" : " enable"} autostart
                  </Text>
                  {showLogs ? (
                    <Text color="#666"><Text color="#4ECDC4">[l]</Text> stop logs</Text>
                  ) : (
                    <Text color="#666"><Text color="#4ECDC4">[l]</Text> stream logs</Text>
                  )}
                </>
              )}
              {manifest.build && buildStatus === "idle" && (
                <Text color="#666"><Text color="#4ECDC4">[b]</Text>uild</Text>
              )}
              {buildStatus === "idle" && (
                confirmClean ? (
                  <Text color="#FF6B6B"><Text color="#FF6B6B" bold>[c]</Text> CONFIRM CLEAN?</Text>
                ) : (
                  <Text color="#666"><Text color="#4ECDC4">[c]</Text>lean</Text>
                )
              )}
              {buildStatus !== "idle" && buildState.process && (
                <Text color="#666"><Text color="#FF6B6B">[x]</Text> cancel {buildStatus}</Text>
              )}
            </Box>
          </Box>
        </Box>

        {/* Right column - Output/Logs */}
        <Box flexDirection="column" width="50%" borderStyle="single" borderColor="#444" paddingX={1}>
          <Text color="#4ECDC4" bold>
            {showLogs ? (
              <FlowerSpinner type="logs" />
            ) : buildStatus === "building" || serviceStatus === "building" ? (
              <FlowerSpinner type="build" />
            ) : buildStatus === "cleaning" ? (
              <FlowerSpinner type="clean" />
            ) : (
              "Output"
            )}
          </Text>
          <Box flexDirection="column" marginTop={1} height={12}>
            {showLogs ? (
              logs.length > 0 ? (
                logs.slice(-10).map((line, i) => (
                  <Text key={i} color="#CCC" wrap="truncate">{line.slice(0, 60)}</Text>
                ))
              ) : (
                <Text color="#666">Waiting for logs...</Text>
              )
            ) : displayOutput.length > 0 ? (
              displayOutput.slice(-10).map((line, i) => (
                <Text key={i} color="#888" wrap="truncate">{line.slice(0, 60)}</Text>
              ))
            ) : (
              <Text color="#666">No output</Text>
            )}
          </Box>
        </Box>
      </Box>
    </Box>
  );
};

export default NodeDetailView;
