import React, { useState, useEffect, useCallback, useRef } from "react";
import { Box, Text, useApp, useInput } from "ink";
import TextInput from "ink-text-input";
import { userInfo } from "os";
import { exec } from "child_process";
import { promisify } from "util";

import NodesView from "./NodesView.js";

const execAsync = promisify(exec);
const USERNAME = userInfo().username;
const VERSION = "0.1.0";

type ViewMode = "home" | "nodes" | "services";

// Available slash commands
const COMMANDS = [
  { cmd: "/nodes", description: "Manage local nodes" },
  { cmd: "/services", description: "Show service status" },
  { cmd: "/quit", description: "Exit Bubbaloop" },
];

const SPINNER_FRAMES = ["⠋", "⠙", "⠹", "⠸", "⠼", "⠴", "⠦", "⠧", "⠇", "⠏"];

const Spinner: React.FC = () => {
  const [frame, setFrame] = useState(0);

  useEffect(() => {
    const timer = setInterval(() => {
      setFrame((i) => (i + 1) % SPINNER_FRAMES.length);
    }, 80);
    return () => clearInterval(timer);
  }, []);

  return <Text color="#FFD93D">{SPINNER_FRAMES[frame]}</Text>;
};

// Bubbaloop mascot with blinking eyes
const RobotLogo: React.FC = () => {
  const [eyesOn, setEyesOn] = useState(true);

  useEffect(() => {
    const interval = setInterval(() => {
      setEyesOn((prev) => !prev);
    }, 800);
    return () => clearInterval(interval);
  }, []);

  return (
    <Box flexDirection="column" alignItems="center" marginY={1}>
      <Text>
        <Text color="#4ECDC4"> ▄▀▀▀▄</Text>
      </Text>
      <Text>
        <Text color="#4ECDC4">█</Text>
        <Text color={eyesOn ? "#6BB5FF" : "#2A5A8A"}> ▓ ▓ </Text>
        <Text color="#4ECDC4">█</Text>
      </Text>
      <Text color="#4ECDC4"> ▀▄█▄▀</Text>
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

interface Message {
  text: string;
  color: string;
  isUser?: boolean;
}

const MessageLine: React.FC<{ msg: Message }> = ({ msg }) => {
  return <Text color={msg.color}>{msg.text}</Text>;
};

interface ServicesViewProps {
  onBack: () => void;
  onExit: () => void;
  exitWarning?: boolean;
}

const ServicesView: React.FC<ServicesViewProps> = ({ onBack, onExit, exitWarning }) => {
  const [services, setServices] = useState<{ name: string; status: string }[]>([]);
  const [loading, setLoading] = useState(true);
  const [selectedIndex, setSelectedIndex] = useState(0);
  const [actionMessage, setActionMessage] = useState<string | null>(null);

  const serviceNames = ["bubbaloop-zenohd", "bubbaloop-bridge", "bubbaloop-daemon"];

  const fetchStatus = useCallback(async () => {
    const results = await Promise.all(
      serviceNames.map(async (name) => {
        try {
          const { stdout } = await execAsync(
            `systemctl --user is-active ${name} 2>/dev/null || echo "inactive"`
          );
          return { name, status: stdout.trim() };
        } catch {
          return { name, status: "unknown" };
        }
      })
    );
    setServices(results);
    setLoading(false);
  }, []);

  useEffect(() => {
    fetchStatus();
    const interval = setInterval(fetchStatus, 2000);
    return () => clearInterval(interval);
  }, [fetchStatus]);

  const handleAction = async (action: "start" | "stop" | "restart") => {
    const service = services[selectedIndex];
    if (!service) return;

    setActionMessage(`${action}ing ${service.name}...`);
    try {
      await execAsync(`systemctl --user ${action} ${service.name}`);
      setActionMessage(`${service.name} ${action}ed`);
      await fetchStatus();
    } catch (e) {
      setActionMessage(`Failed to ${action} ${service.name}`);
    }
    setTimeout(() => setActionMessage(null), 2000);
  };

  useInput((input, key) => {
    // Global exit: Ctrl+C or Ctrl+X
    if (key.ctrl && (input === 'c' || input === 'x')) {
      onExit();
      return;
    }

    if (key.escape || input === "q") {
      onBack();
    } else if (key.upArrow) {
      setSelectedIndex((i) => (i > 0 ? i - 1 : services.length - 1));
    } else if (key.downArrow) {
      setSelectedIndex((i) => (i < services.length - 1 ? i + 1 : 0));
    } else if (input === "s") {
      handleAction("start");
    } else if (input === "x") {
      handleAction("stop");
    } else if (input === "r") {
      handleAction("restart");
    }
  });

  const getStatusColor = (status: string): string => {
    const colors: Record<string, string> = {
      active: "#95E1D3",
      inactive: "#888",
      failed: "#FF6B6B",
    };
    return colors[status] ?? "#FFD93D";
  };

  return (
    <Box flexDirection="column" padding={0}>
      <Box borderStyle="round" borderColor="#4ECDC4" paddingX={1} justifyContent="space-between">
        <Text color="#4ECDC4" bold>Services</Text>
        <Text color="#888">esc/q back</Text>
      </Box>

      <Box flexDirection="column" borderStyle="single" borderColor="#444" marginTop={0}>
        {loading ? (
          <Box paddingX={1} paddingY={1}>
            <Spinner />
            <Text color="#888"> Loading...</Text>
          </Box>
        ) : (
          services.map((svc, index) => (
            <Box key={svc.name} paddingX={1}>
              <Text color={index === selectedIndex ? "#4ECDC4" : "#888"}>
                {index === selectedIndex ? "❯ " : "  "}
              </Text>
              <Text color={getStatusColor(svc.status)}>●</Text>
              <Text color={index === selectedIndex ? "#FFF" : "#AAA"}>
                {" "}{svc.name.replace("bubbaloop-", "")}
              </Text>
              <Text color="#888"> - {svc.status}</Text>
            </Box>
          ))
        )}
      </Box>

      {actionMessage && (
        <Box marginX={1} marginTop={1}>
          <Text color="#FFD93D">{actionMessage}</Text>
        </Box>
      )}

      {exitWarning && (
        <Box marginX={1} marginTop={1}>
          <Text color="#FF6B6B">Press Ctrl+C again to exit</Text>
        </Box>
      )}

      <Box marginX={1} marginTop={1}>
        <Text color="#666">
          <Text color="#4ECDC4">s</Text> start •{" "}
          <Text color="#4ECDC4">x</Text> stop •{" "}
          <Text color="#4ECDC4">r</Text> restart •{" "}
          <Text color="#4ECDC4">↑↓</Text> select •{" "}
          <Text color="#4ECDC4">esc/q</Text> back
        </Text>
      </Box>
    </Box>
  );
};


const App: React.FC = () => {
  const { exit } = useApp();
  const [input, setInput] = useState("");
  const [messages, setMessages] = useState<Message[]>([]);
  const [commandIndex, setCommandIndex] = useState(0);

  // Loading state for async operations
  const [isLoading, setIsLoading] = useState(false);
  const [loadingMessage, setLoadingMessage] = useState("");

  // Command history
  const [commandHistory, setCommandHistory] = useState<string[]>([]);
  const [historyIndex, setHistoryIndex] = useState(-1);
  const [savedInput, setSavedInput] = useState("");

  // Current view
  const [viewMode, setViewMode] = useState<ViewMode>("home");

  // Switch views - clear screen synchronously before state change
  const switchView = useCallback((newView: ViewMode) => {
    if (newView === viewMode) return;
    // Clear screen before changing view to prevent artifacts
    process.stdout.write('\x1b[2J\x1b[H');
    setViewMode(newView);
  }, [viewMode]);

  // Exit confirmation (double Ctrl+C)
  const [exitWarning, setExitWarning] = useState(false);
  const exitTimeoutRef = useRef<NodeJS.Timeout | null>(null);

  const handleExitRequest = useCallback(() => {
    if (exitWarning) {
      // Second press - actually exit
      if (exitTimeoutRef.current) clearTimeout(exitTimeoutRef.current);
      exit();
    } else {
      // First press - show warning
      setExitWarning(true);
      exitTimeoutRef.current = setTimeout(() => {
        setExitWarning(false);
      }, 2000);
    }
  }, [exitWarning, exit]);

  const showCommands = input.startsWith("/");
  const filteredCommands = COMMANDS.filter((c) =>
    c.cmd.toLowerCase().startsWith(input.toLowerCase())
  );

  useInput((char, key) => {
    // Global exit: Ctrl+C or Ctrl+X (double press required)
    if (key.ctrl && (char === 'c' || char === 'x')) {
      handleExitRequest();
      return;
    }

    // Command history navigation
    const browsingHistory = historyIndex >= 0;

    if (key.upArrow && commandHistory.length > 0 && (browsingHistory || !input)) {
      if (historyIndex === -1) {
        setSavedInput(input);
        setHistoryIndex(commandHistory.length - 1);
        setInput(commandHistory[commandHistory.length - 1]);
      } else if (historyIndex > 0) {
        setHistoryIndex(historyIndex - 1);
        setInput(commandHistory[historyIndex - 1]);
      }
    } else if (key.downArrow && browsingHistory) {
      if (historyIndex < commandHistory.length - 1) {
        setHistoryIndex(historyIndex + 1);
        setInput(commandHistory[historyIndex + 1]);
      } else {
        setHistoryIndex(-1);
        setInput(savedInput);
      }
    } else if (showCommands && filteredCommands.length > 0) {
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
    }

    if (key.escape) {
      if (input) {
        setInput("");
      }
    }
  });

  useEffect(() => {
    setCommandIndex(0);
  }, [input]);

  const addMessage = (text: string, color: string, isUser = false) => {
    setMessages((prev) => [...prev.slice(-10), { text, color, isUser }]);
  };

  const handleExitSubview = useCallback(() => {
    switchView("home");
  }, [switchView]);

  const handleSubmit = (value: string) => {
    let trimmed = value.trim();
    if (!trimmed) return;

    // If showing suggestions and input is partial, use the selected command
    const isExactCommand = COMMANDS.some(c => c.cmd === trimmed);
    if (showCommands && filteredCommands.length > 0 && !isExactCommand) {
      trimmed = filteredCommands[commandIndex].cmd;
    }

    // Add to command history (avoid duplicates)
    const lastCommand = commandHistory[commandHistory.length - 1];
    if (lastCommand !== trimmed) {
      setCommandHistory((prev) => [...prev.slice(-50), trimmed]);
    }
    setHistoryIndex(-1);
    setSavedInput("");

    if (!trimmed.startsWith("/")) {
      addMessage(`❯ ${trimmed}`, "#888", true);
      setInput("");
      return;
    }

    const cmd = trimmed.toLowerCase();

    // Exit commands
    if (cmd === "/quit" || cmd === "/exit" || cmd === "/q") {
      exit();
      return;
    }

    // View navigation commands
    if (cmd === "/nodes" || cmd === "/services") {
      setInput("");
      switchView(cmd.slice(1) as ViewMode);
      return;
    }

    // Unknown command
    addMessage(`❯ ${trimmed}`, "#888", true);
    addMessage(`└ Unknown command: ${trimmed}`, "#FF6B6B");
    setInput("");
  };

  // Build title line like Claude Code: ╭─── Bubbaloop v0.1.0 ───...───╮
  const termWidth = process.stdout.columns || 80;
  const contentWidth = termWidth - 2; // minus the ╭ and ╮
  const titlePart = "─── Bubbaloop ";
  const versionPart = `v${VERSION} `;
  const remainingDashes = contentWidth - titlePart.length - versionPart.length;

  // Render based on viewMode - completely separate returns to avoid tree diffing issues
  if (viewMode === "services") {
    return (
      <Box flexDirection="column" padding={0}>
        <ServicesView onBack={handleExitSubview} onExit={handleExitRequest} exitWarning={exitWarning} />
      </Box>
    );
  }

  if (viewMode === "nodes") {
    return (
      <Box flexDirection="column" padding={0}>
        <NodesView onBack={handleExitSubview} onExit={handleExitRequest} exitWarning={exitWarning} />
      </Box>
    );
  }

  // Home view
  return (
    <Box flexDirection="column" padding={0}>
      {/* Header bar - Claude Code style */}
      <Text>
        <Text color="#4ECDC4">╭{titlePart}</Text>
        <Text color="#888">{versionPart}</Text>
        <Text color="#4ECDC4">{"─".repeat(Math.max(0, remainingDashes))}╮</Text>
      </Text>

      {/* Main content - two columns with sides only */}
      <Box
        flexDirection="row"
        borderStyle="single"
        borderColor="#4ECDC4"
        borderTop={false}
        borderBottom={false}
      >
        {/* Left column - Welcome + Robot */}
        <Box
          flexDirection="column"
          alignItems="center"
          paddingX={4}
          paddingTop={1}
          width="50%"
        >
          <Text bold color="#FFF">
            Welcome {USERNAME}!
          </Text>
          <RobotLogo />
          <Text color="#AAA">Multi-agent orchestration</Text>
          <Text color="#AAA">for Physical AI</Text>
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
        <Box flexDirection="column" width="50%" paddingX={2} paddingTop={1}>
          <Text bold color="#FFD93D">
            Node Management
          </Text>
          <Text color="#CCC">
            <Text color="#4ECDC4">/nodes</Text> manage local nodes
          </Text>
          <Text color="#CCC">
            <Text color="#4ECDC4">/services</Text> service status
          </Text>
          <Text color="#CCC">
            <Text color="#4ECDC4">/quit</Text> exit
          </Text>
        </Box>
      </Box>
      {/* Bottom border */}
      <Text color="#4ECDC4">{`╰${"─".repeat(Math.max(0, (process.stdout.columns || 80) - 2))}╯`}</Text>

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
          ❯{" "}
        </Text>
        {isLoading ? (
          <Text color="#666">waiting...</Text>
        ) : (
          <TextInput
            value={input}
            onChange={setInput}
            onSubmit={handleSubmit}
            placeholder='Type "/" for commands'
          />
        )}
      </Box>

      {/* Exit warning */}
      {exitWarning && (
        <Box marginX={1}>
          <Text color="#FF6B6B">Press Ctrl+C again to exit</Text>
        </Box>
      )}

      {/* Footer */}
      <Box marginX={1}>
        <Text color="#666">esc clear • ↑↓ history • /quit exit</Text>
      </Box>
    </Box>
  );
};

export default App;
