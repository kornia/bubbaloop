import React, { useState, useEffect, useRef, useCallback } from "react";
import { Box, Text, useApp, useInput } from "ink";
import TextInput from "ink-text-input";
import open from "open";
import { Session, Config, Sample, Subscriber } from "@eclipse-zenoh/zenoh-ts";
import {
  loadConfig,
  saveConfig,
  getZenohCliConfigPath,
  DEFAULT_ENDPOINT,
} from "./config.js";

const VERSION = "0.1.0";

type ConnectionStatus = "disconnected" | "connecting" | "connected";
type InputMode = "command" | "endpoint" | "server";
type ViewMode = "home" | "topics";

// Available slash commands
const COMMANDS = [
  { cmd: "/server", description: "Configure remote server endpoint" },
  { cmd: "/connect", description: "Connect to local zenohd" },
  { cmd: "/disconnect", description: "Disconnect from zenoh" },
  { cmd: "/topics", description: "List available topics" },
  { cmd: "/dashboard", description: "Open web dashboard" },
  { cmd: "/record", description: "Record topics to MCAP file" },
  { cmd: "/quit", description: "Exit Bubbaloop" },
];

// Spinner component for loading states
const Spinner: React.FC = () => {
  const frames = ["⠋", "⠙", "⠹", "⠸", "⠼", "⠴", "⠦", "⠧", "⠇", "⠏"];
  const [frame, setFrame] = useState(0);

  useEffect(() => {
    const timer = setInterval(() => {
      setFrame((i) => (i + 1) % frames.length);
    }, 80);
    return () => clearInterval(timer);
  }, []);

  return <Text color="#FFD93D">{frames[frame]}</Text>;
};

// Robot emoji-style component with blinking antenna
const RobotLogo: React.FC = () => {
  const [antennaOn, setAntennaOn] = useState(true);

  useEffect(() => {
    const interval = setInterval(() => {
      setAntennaOn((prev) => !prev);
    }, 600);
    return () => clearInterval(interval);
  }, []);

  return (
    <Box flexDirection="column" alignItems="center">
      <Text>
        {"   "}
        <Text color={antennaOn ? "#FFF" : "#555"}>
          {antennaOn ? "◎" : "○"}
        </Text>
      </Text>
      <Text>
        {"   "}
        <Text color="#888">│</Text>
      </Text>
      <Text color="#888">┌╌╌╌╌╌╌╌┐</Text>
      <Text>
        <Text color="#888">┆</Text>
        <Text color="#FFF"> ●   ○ </Text>
        <Text color="#888">┆</Text>
      </Text>
      <Text>
        <Text color="#888">┆</Text>
        <Text color="#FFF">  ▭▭▭  </Text>
        <Text color="#888">┆</Text>
      </Text>
      <Text color="#888">└╌╌╌╌╌╌╌┘</Text>
    </Box>
  );
};

// Command suggestions component
interface CommandSuggestionsProps {
  filter: string;
  selectedIndex: number;
}

const CommandSuggestions: React.FC<CommandSuggestionsProps> = ({
  filter,
  selectedIndex,
}) => {
  const filtered = COMMANDS.filter((c) =>
    c.cmd.toLowerCase().startsWith(filter.toLowerCase())
  );

  if (filtered.length === 0) return null;

  return (
    <Box flexDirection="column" paddingX={1}>
      {filtered.map((cmd, index) => (
        <Box key={cmd.cmd}>
          <Text color={index === selectedIndex ? "#4ECDC4" : "#888"}>
            {index === selectedIndex ? "❯ " : "  "}
          </Text>
          <Text
            bold={index === selectedIndex}
            color={index === selectedIndex ? "#FFF" : "#AAA"}
          >
            {cmd.cmd}
          </Text>
          <Text color="#888"> - {cmd.description}</Text>
        </Box>
      ))}
    </Box>
  );
};

// Connection status indicator component
interface StatusIndicatorProps {
  status: ConnectionStatus;
  endpoint: string;
}

const StatusIndicator: React.FC<StatusIndicatorProps> = ({ status, endpoint }) => {
  const statusConfig = {
    disconnected: { color: "#FF6B6B", symbol: "●", text: "disconnected" },
    connecting: { color: "#FFD93D", symbol: "●", text: "connecting..." },
    connected: { color: "#95E1D3", symbol: "●", text: endpoint },
  };

  const { color, symbol, text } = statusConfig[status];

  return (
    <Text>
      <Text color={color}>{symbol}</Text>
      <Text color="#888"> {text}</Text>
    </Text>
  );
};

// Message type
interface Message {
  text: string;
  color: string;
  isUser?: boolean;
  details?: string; // Expandable error details
}

// Message component - details are always shown when present
const MessageLine: React.FC<{ msg: Message }> = ({ msg }) => {
  const hasDetails = Boolean(msg.details && msg.details.length > 0);
  const MAX_DETAIL_LINES = 5;

  return (
    <Box flexDirection="column">
      <Text color={msg.color}>{msg.text}</Text>
      {hasDetails && (
        <Box flexDirection="column" marginLeft={2}>
          {msg.details!.split("\n").slice(0, MAX_DETAIL_LINES).map((line, i) => (
            <Text key={i} color="#666" dimColor>
              {line}
            </Text>
          ))}
          {msg.details!.split("\n").length > MAX_DETAIL_LINES && (
            <Text color="#666" dimColor>... ({msg.details!.split("\n").length - MAX_DETAIL_LINES} more lines)</Text>
          )}
        </Box>
      )}
    </Box>
  );
};

// Topics view component - live table of topics with Hz stats
interface TopicStats {
  count: number;
  timestamps: number[]; // Sliding window of timestamps for Hz calculation
  minHz: number | null;
  maxHz: number | null;
  avgHz: number | null;
}

interface TopicsViewProps {
  session: Session;
  onExit: () => void;
}

const TopicsView: React.FC<TopicsViewProps> = ({ session, onExit }) => {
  const [topicStats, setTopicStats] = useState<Map<string, TopicStats>>(new Map());
  const [totalSamples, setTotalSamples] = useState(0);
  const [windowSize, setWindowSize] = useState(10);
  const subscriberRef = useRef<Subscriber | null>(null);

  // Compute Hz stats from timestamps
  const computeHzStats = (timestamps: number[]): { min: number | null; max: number | null; avg: number | null } => {
    if (timestamps.length < 2) {
      return { min: null, max: null, avg: null };
    }

    const intervals: number[] = [];
    for (let i = 1; i < timestamps.length; i++) {
      const interval = timestamps[i] - timestamps[i - 1];
      if (interval > 0) {
        intervals.push(1000 / interval); // Convert ms interval to Hz
      }
    }

    if (intervals.length === 0) {
      return { min: null, max: null, avg: null };
    }

    const min = Math.min(...intervals);
    const max = Math.max(...intervals);
    const avg = intervals.reduce((a, b) => a + b, 0) / intervals.length;

    return { min, max, avg };
  };

  useEffect(() => {
    let mounted = true;

    const startSubscription = async () => {
      try {
        const subscriber = await session.declareSubscriber("**", {
          handler: (sample: Sample) => {
            if (!mounted) return;
            const keyExpr = sample.keyexpr().toString();
            const now = Date.now();

            setTopicStats((prev) => {
              const newMap = new Map(prev);
              const existing = newMap.get(keyExpr) || {
                count: 0,
                timestamps: [],
                minHz: null,
                maxHz: null,
                avgHz: null,
              };

              // Add timestamp to sliding window
              const newTimestamps = [...existing.timestamps, now].slice(-windowSize);
              const hzStats = computeHzStats(newTimestamps);

              newMap.set(keyExpr, {
                count: existing.count + 1,
                timestamps: newTimestamps,
                minHz: hzStats.min,
                maxHz: hzStats.max,
                avgHz: hzStats.avg,
              });

              return newMap;
            });
            setTotalSamples((prev) => prev + 1);
          },
        });
        subscriberRef.current = subscriber;
      } catch (e) {
        // Subscription failed, exit view
        onExit();
      }
    };

    startSubscription();

    return () => {
      mounted = false;
      if (subscriberRef.current) {
        subscriberRef.current.undeclare().catch(() => {});
      }
    };
  }, [session, onExit, windowSize]);

  // Handle input for exiting and window size adjustment
  useInput((input, key) => {
    if (key.escape || input === "q") {
      onExit();
    } else if (input === "+" || input === "=") {
      setWindowSize((prev) => Math.min(prev + 5, 100));
    } else if (input === "-" || input === "_") {
      setWindowSize((prev) => Math.max(prev - 5, 5));
    }
  });

  // Sort topics alphabetically
  const sortedTopics = Array.from(topicStats.entries()).sort((a, b) =>
    a[0].localeCompare(b[0])
  );

  // Format Hz value
  const formatHz = (hz: number | null): string => {
    if (hz === null) return "-";
    if (hz >= 1000) return `${(hz / 1000).toFixed(1)}k`;
    if (hz >= 100) return hz.toFixed(0);
    if (hz >= 10) return hz.toFixed(1);
    return hz.toFixed(2);
  };

  return (
    <Box flexDirection="column" padding={0}>
      {/* Header */}
      <Box borderStyle="round" borderColor="#4ECDC4" paddingX={1} justifyContent="space-between">
        <Text color="#4ECDC4" bold>
          Topics Monitor
        </Text>
        <Text color="#888">
          {sortedTopics.length} topics • {totalSamples} samples • window: {windowSize} • <Text color="#666">esc/q exit</Text>
        </Text>
      </Box>

      {/* Topics table */}
      <Box flexDirection="column" borderStyle="single" borderColor="#444" marginTop={0}>
        {/* Table header */}
        <Box paddingX={1} borderBottom borderColor="#444">
          <Box width="40%">
            <Text color="#4ECDC4" bold>Topic</Text>
          </Box>
          <Box width="12%" justifyContent="flex-end">
            <Text color="#4ECDC4" bold>Count</Text>
          </Box>
          <Box width="16%" justifyContent="flex-end">
            <Text color="#4ECDC4" bold>Min Hz</Text>
          </Box>
          <Box width="16%" justifyContent="flex-end">
            <Text color="#4ECDC4" bold>Avg Hz</Text>
          </Box>
          <Box width="16%" justifyContent="flex-end">
            <Text color="#4ECDC4" bold>Max Hz</Text>
          </Box>
        </Box>

        {/* Table rows */}
        {sortedTopics.length === 0 ? (
          <Box paddingX={1} paddingY={1}>
            <Text color="#888">Waiting for messages...</Text>
          </Box>
        ) : (
          sortedTopics.slice(0, 20).map(([topic, stats]) => (
            <Box key={topic} paddingX={1}>
              <Box width="40%">
                <Text color="#CCC">{topic.length > 35 ? topic.slice(0, 32) + "..." : topic}</Text>
              </Box>
              <Box width="12%" justifyContent="flex-end">
                <Text color="#95E1D3">{stats.count}</Text>
              </Box>
              <Box width="16%" justifyContent="flex-end">
                <Text color="#FFD93D">{formatHz(stats.minHz)}</Text>
              </Box>
              <Box width="16%" justifyContent="flex-end">
                <Text color="#4ECDC4">{formatHz(stats.avgHz)}</Text>
              </Box>
              <Box width="16%" justifyContent="flex-end">
                <Text color="#FF6B6B">{formatHz(stats.maxHz)}</Text>
              </Box>
            </Box>
          ))
        )}

        {sortedTopics.length > 20 && (
          <Box paddingX={1}>
            <Text color="#888">... and {sortedTopics.length - 20} more topics</Text>
          </Box>
        )}
      </Box>

      {/* Footer */}
      <Box marginX={1} marginTop={1}>
        <Text color="#666">
          <Text color="#4ECDC4">+/-</Text> window size • <Text color="#4ECDC4">esc/q</Text> exit
        </Text>
      </Box>
    </Box>
  );
};

// Store original console functions at module level
const originalConsole = {
  log: console.log,
  warn: console.warn,
  error: console.error,
};

// Suppress console output (for zenoh background activity)
const suppressConsole = () => {
  const noop = () => {};
  console.log = noop;
  console.warn = noop;
  console.error = noop;
};

const restoreConsole = () => {
  console.log = originalConsole.log;
  console.warn = originalConsole.warn;
  console.error = originalConsole.error;
};

// Main App component
const App: React.FC = () => {
  const { exit } = useApp();
  const [input, setInput] = useState("");
  const [messages, setMessages] = useState<Message[]>([]);
  const [commandIndex, setCommandIndex] = useState(0);

  // Connection state
  const [connectionStatus, setConnectionStatus] = useState<ConnectionStatus>("disconnected");
  const [endpoint, setEndpoint] = useState(() => loadConfig().endpoint || DEFAULT_ENDPOINT);
  const sessionRef = useRef<Session | null>(null);
  const consoleSuppressedRef = useRef(false);

  // Input mode: command or endpoint entry
  const [inputMode, setInputMode] = useState<InputMode>("command");

  // Loading state for async operations
  const [isLoading, setIsLoading] = useState(false);
  const [loadingMessage, setLoadingMessage] = useState("");

  // Command history
  const [commandHistory, setCommandHistory] = useState<string[]>([]);
  const [historyIndex, setHistoryIndex] = useState(-1);
  const [savedInput, setSavedInput] = useState(""); // Save current input when navigating history

  // Current view
  const [viewMode, setViewMode] = useState<ViewMode>("home");

  const showCommands = inputMode === "command" && input.startsWith("/");
  const filteredCommands = COMMANDS.filter((c) =>
    c.cmd.toLowerCase().startsWith(input.toLowerCase())
  );

  useInput((char, key) => {
    if (inputMode === "endpoint" || inputMode === "server") {
      if (key.escape) {
        setInputMode("command");
        setInput("");
        addMessage("└ Cancelled", "#888");
      }
      return;
    }

    if (showCommands && filteredCommands.length > 0) {
      if (key.upArrow) {
        setCommandIndex((prev) =>
          prev > 0 ? prev - 1 : filteredCommands.length - 1
        );
      } else if (key.downArrow) {
        setCommandIndex((prev) =>
          prev < filteredCommands.length - 1 ? prev + 1 : 0
        );
      } else if (key.tab) {
        setInput(filteredCommands[commandIndex].cmd);
      }
    } else if (!showCommands && commandHistory.length > 0) {
      // Navigate command history when not showing suggestions
      if (key.upArrow) {
        if (historyIndex === -1) {
          // Save current input before navigating history
          setSavedInput(input);
          setHistoryIndex(commandHistory.length - 1);
          setInput(commandHistory[commandHistory.length - 1]);
        } else if (historyIndex > 0) {
          setHistoryIndex(historyIndex - 1);
          setInput(commandHistory[historyIndex - 1]);
        }
      } else if (key.downArrow) {
        if (historyIndex >= 0) {
          if (historyIndex < commandHistory.length - 1) {
            setHistoryIndex(historyIndex + 1);
            setInput(commandHistory[historyIndex + 1]);
          } else {
            // Return to saved input
            setHistoryIndex(-1);
            setInput(savedInput);
          }
        }
      }
    }

    if (key.escape) {
      // Escape clears input, doesn't exit - use /quit to exit
      if (input) {
        setInput("");
      }
    }
  });

  useEffect(() => {
    setCommandIndex(0);
  }, [input]);

  // Cleanup session on unmount
  useEffect(() => {
    return () => {
      if (sessionRef.current) {
        sessionRef.current.close().catch(() => {});
      }
      // Always restore console on unmount
      restoreConsole();
    };
  }, []);

  // Connection health monitoring
  useEffect(() => {
    if (connectionStatus === "connected" && sessionRef.current) {
      let isChecking = false;
      let disconnected = false;

      // Keep console suppressed during connected state to catch any zenoh background output
      suppressConsole();

      const healthCheck = setInterval(async () => {
        // Prevent concurrent checks and multiple disconnection events
        if (!sessionRef.current || isChecking || disconnected) return;

        isChecking = true;

        try {
          // Check if session is explicitly closed
          if (sessionRef.current.isClosed()) {
            disconnected = true;
            sessionRef.current = null;
            setConnectionStatus("disconnected");
            // Force complete re-render to clear any terminal corruption
            setTimeout(() => {
              (global as any).__bubbaloop_forceRerender?.();
            }, 50);
            return;
          }

          // Try to get session info as a ping - if it fails, connection is lost
          const timeoutPromise = new Promise<never>((_, reject) => {
            setTimeout(() => reject(new Error("Health check timeout")), 2000);
          });
          await Promise.race([
            sessionRef.current.info(),
            timeoutPromise,
          ]);
        } catch {
          // Connection lost - just update status
          if (!disconnected) {
            disconnected = true;
            sessionRef.current = null;
            setConnectionStatus("disconnected");
            // Force complete re-render to clear any terminal corruption
            setTimeout(() => {
              (global as any).__bubbaloop_forceRerender?.();
            }, 50);
          }
        } finally {
          isChecking = false;
        }
      }, 1000);
      return () => {
        clearInterval(healthCheck);
      };
    }
  }, [connectionStatus, endpoint]);

  const addMessage = (text: string, color: string, isUser = false, details?: string) => {
    setMessages((prev) => [...prev.slice(-10), { text, color, isUser, details }]);
  };

  const handleConnect = async (ep: string) => {
    // Close existing session if any
    if (sessionRef.current) {
      try {
        await sessionRef.current.close();
      } catch {
        // Ignore close errors
      }
      sessionRef.current = null;
    }

    setConnectionStatus("connecting");
    setIsLoading(true);
    setLoadingMessage(`Connecting to ${ep}...`);

    // Capture console output from zenoh-ts library
    const capturedLogs: string[] = [];

    const captureLog = (...args: unknown[]) => {
      capturedLogs.push(args.map(a => typeof a === 'object' ? JSON.stringify(a) : String(a)).join(' '));
    };

    // Suppress console during connection
    console.log = captureLog;
    console.warn = captureLog;
    console.error = captureLog;
    consoleSuppressedRef.current = true;

    try {
      const CONNECTION_TIMEOUT_MS = 3000;
      const config = new Config(ep, 1000);

      // Race between connection and timeout
      const timeoutPromise = new Promise<never>((_, reject) => {
        setTimeout(() => reject(new Error("Connection timeout")), CONNECTION_TIMEOUT_MS);
      });

      const session = await Promise.race([
        Session.open(config),
        timeoutPromise,
      ]);

      sessionRef.current = session;
      setEndpoint(ep);
      setConnectionStatus("connected");
      addMessage(`└ Connected to ${ep}`, "#95E1D3");

      // Save successful endpoint to config
      saveConfig({ endpoint: ep });

      // Restore console on success
      restoreConsole();
      consoleSuppressedRef.current = false;
    } catch (e) {
      setConnectionStatus("disconnected");
      let errMsg = "Unknown error";

      if (e instanceof Error) {
        errMsg = e.message || e.name;
      } else if (typeof e === "object" && e !== null) {
        errMsg = (e as Record<string, unknown>).message as string || "Connection error";
      } else {
        errMsg = String(e);
      }

      // Combine captured console output as details (deduplicated)
      const uniqueLogs = [...new Set(capturedLogs)];
      const errDetails = uniqueLogs.length > 0 ? uniqueLogs.join("\n") : undefined;

      addMessage(`└ Connection failed: ${errMsg}`, "#FF6B6B", false, errDetails);

      // Keep console suppressed - zenoh keeps retrying in background
      // Will be restored on next successful connection or app exit
      suppressConsole();
    } finally {
      setIsLoading(false);
      setLoadingMessage("");
    }
  };

  const handleDisconnect = async () => {
    if (sessionRef.current) {
      const currentEndpoint = endpoint;
      try {
        await sessionRef.current.close();
        addMessage(`└ Disconnected from ${currentEndpoint}`, "#95E1D3");
      } catch (e) {
        const err = e instanceof Error ? e.message : String(e);
        addMessage(`└ Error disconnecting: ${err}`, "#FF6B6B");
      }
      sessionRef.current = null;
    } else {
      addMessage("└ Not connected", "#888");
    }
    setConnectionStatus("disconnected");
  };

  const handleSetServer = (serverEndpoint: string) => {
    // Normalize the endpoint format
    let normalized = serverEndpoint.trim();
    if (!normalized.startsWith("tcp/")) {
      // Assume it's just an IP:port, add tcp/ prefix
      normalized = `tcp/${normalized}`;
    }
    // Ensure it has a port
    if (!normalized.includes(":")) {
      normalized = `${normalized}:7447`;
    }

    // Save to config (this also generates zenoh.cli.json5)
    const currentConfig = loadConfig();
    saveConfig({ ...currentConfig, serverEndpoint: normalized });

    const configPath = getZenohCliConfigPath();
    addMessage(`└ Server endpoint set to: ${normalized}`, "#95E1D3");
    addMessage(`  Config saved to: ${configPath}`, "#888");
    addMessage(`  Run: zenohd -c ${configPath}`, "#4ECDC4");
  };

  const handleTopics = () => {
    if (!sessionRef.current) {
      addMessage("└ Not connected", "#FF6B6B");
      return;
    }
    // Switch to topics view
    setViewMode("topics");
  };

  const handleExitTopicsView = useCallback(() => {
    setViewMode("home");
  }, []);

  const handleSubmit = (value: string) => {
    const trimmedInput = value.trim();

    // Handle endpoint input mode
    if (inputMode === "endpoint") {
      if (!trimmedInput) {
        addMessage("└ Cancelled", "#888");
      } else {
        handleConnect(trimmedInput);
      }
      setInputMode("command");
      setInput("");
      return;
    }

    // Handle server input mode
    if (inputMode === "server") {
      if (!trimmedInput) {
        addMessage("└ Cancelled", "#888");
      } else {
        handleSetServer(trimmedInput);
      }
      setInputMode("command");
      setInput("");
      return;
    }

    let trimmed = trimmedInput;
    if (!trimmed) return;

    // If showing suggestions and input is partial, use the selected command
    if (showCommands && filteredCommands.length > 0 && !COMMANDS.some(c => c.cmd === trimmed)) {
      trimmed = filteredCommands[commandIndex].cmd;
    }

    // Add to command history (avoid duplicates of last command)
    if (commandHistory.length === 0 || commandHistory[commandHistory.length - 1] !== trimmed) {
      setCommandHistory((prev) => [...prev.slice(-50), trimmed]); // Keep last 50 commands
    }
    setHistoryIndex(-1);
    setSavedInput("");

    // Show user input
    addMessage(`❯ ${trimmed}`, "#888", true);

    if (trimmed.startsWith("/")) {
      const cmd = trimmed.toLowerCase();

      if (cmd === "/quit" || cmd === "/exit" || cmd === "/q") {
        exit();
        return;
      }

      if (cmd === "/server") {
        const currentConfig = loadConfig();
        const currentServer = currentConfig.serverEndpoint || "tcp/192.168.1.100:7447";
        addMessage(`└ Enter server endpoint (e.g. tcp/192.168.1.100:7447):`, "#4ECDC4");
        setInputMode("server");
        setInput(currentServer); // Pre-fill with last used server
        return;
      } else if (cmd === "/connect") {
        addMessage(`└ Enter endpoint (e.g. ${DEFAULT_ENDPOINT}):`, "#4ECDC4");
        setInputMode("endpoint");
        setInput(endpoint); // Pre-fill with last used endpoint
        return;
      } else if (cmd === "/disconnect") {
        handleDisconnect();
      } else if (cmd === "/topics") {
        if (connectionStatus !== "connected") {
          addMessage("└ Not connected. Use /connect first", "#FF6B6B");
        } else {
          handleTopics();
        }
      } else if (cmd === "/dashboard") {
        addMessage("└ Opening dashboard at http://localhost:5173", "#95E1D3");
        open("http://localhost:5173");
      } else if (cmd === "/record") {
        if (connectionStatus !== "connected") {
          addMessage("└ Not connected. Use /connect first", "#FF6B6B");
        } else {
          addMessage("└ Not implemented", "#FFD93D");
        }
      } else {
        addMessage(`└ Unknown command: ${trimmed}`, "#FF6B6B");
      }
    }

    setInput("");
  };

  const getInputPlaceholder = () => {
    if (inputMode === "endpoint") {
      return "ws://ip:port (esc to cancel)";
    }
    if (inputMode === "server") {
      return "tcp/ip:port or just ip:port (esc to cancel)";
    }
    return 'Type "/" for commands';
  };

  // If in topics view, render TopicsView component
  if (viewMode === "topics" && sessionRef.current) {
    return <TopicsView session={sessionRef.current} onExit={handleExitTopicsView} />;
  }

  return (
    <Box flexDirection="column" padding={0}>
      {/* Header bar with status */}
      <Box borderStyle="round" borderColor="#4ECDC4" paddingX={1} justifyContent="space-between">
        <Text color="#4ECDC4" bold>
          Bubbaloop v{VERSION}
        </Text>
        <StatusIndicator status={connectionStatus} endpoint={endpoint} />
      </Box>

      {/* Main content - two columns with border */}
      <Box
        flexDirection="row"
        borderStyle="single"
        borderColor="#4ECDC4"
        borderTop={false}
      >
        {/* Left column - Welcome + Robot */}
        <Box
          flexDirection="column"
          alignItems="center"
          paddingX={4}
          width="50%"
        >
          <Text bold color="#FFF">
            Welcome to Bubbaloop!
          </Text>
          <RobotLogo />
          <Text color="#AAA">Multi-camera streaming with ROS-Z</Text>
        </Box>

        {/* Vertical divider */}
        <Box
          borderStyle="single"
          borderColor="#4ECDC4"
          borderLeft={false}
          borderTop={false}
          borderBottom={false}
        />

        {/* Right column - Tips */}
        <Box flexDirection="column" width="50%" paddingX={2}>
          <Text bold color="#FFD93D">
            Tips for getting started
          </Text>
          <Text color="#CCC">
            <Text color="#4ECDC4">/connect</Text> connect to robot
          </Text>
          <Text color="#CCC">
            <Text color="#4ECDC4">/topics</Text> list available topics
          </Text>
          <Text color="#CCC">
            <Text color="#4ECDC4">/dashboard</Text> open the web UI
          </Text>
          <Text color="#CCC">
            <Text color="#4ECDC4">/record</Text> record to MCAP
          </Text>
        </Box>
      </Box>

      {/* Output messages area */}
      {(messages.length > 0 || isLoading) && (
        <Box flexDirection="column" marginX={1}>
          {messages.map((msg, i) => (
            <MessageLine key={i} msg={msg} />
          ))}
          {isLoading && (
            <Text>
              <Text color="#888">└ </Text>
              <Spinner />
              <Text color="#FFD93D"> {loadingMessage}</Text>
            </Text>
          )}
        </Box>
      )}

      {/* Command suggestions - above input */}
      {showCommands && !isLoading && (
        <Box marginX={1}>
          <CommandSuggestions filter={input} selectedIndex={commandIndex} />
        </Box>
      )}

      {/* Input area - always at bottom with border */}
      <Box marginX={1} borderStyle="round" borderColor={isLoading ? "#666" : "#4ECDC4"} paddingX={1}>
        <Text color={isLoading ? "#666" : "#4ECDC4"} bold>
          {inputMode === "endpoint" ? "endpoint: " : "❯ "}
        </Text>
        {isLoading ? (
          <Text color="#666">waiting...</Text>
        ) : (
          <TextInput
            value={input}
            onChange={setInput}
            onSubmit={handleSubmit}
            placeholder={getInputPlaceholder()}
          />
        )}
      </Box>

      {/* Footer */}
      <Box marginX={1}>
        <Text color="#666">esc clear • ↑↓ history • /quit exit</Text>
      </Box>
    </Box>
  );
};

export default App;
