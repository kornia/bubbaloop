import React, { useState, useEffect } from "react";
import { Box, Text, useInput } from "ink";
import { spawn } from "child_process";
import {
  listPlugins,
  PluginManifest,
  PLUGINS_DIR,
  findProjectRoot,
  getTemplatesDir,
} from "./config.js";

interface PluginsViewProps {
  onBack: () => void;
}

interface PluginInfo {
  path: string;
  manifest: PluginManifest;
  status: "installed" | "running" | "error";
}

const PluginsView: React.FC<PluginsViewProps> = ({ onBack }) => {
  const [plugins, setPlugins] = useState<PluginInfo[]>([]);
  const [selectedIndex, setSelectedIndex] = useState(0);
  const [message, setMessage] = useState<string | null>(null);
  const [showCreate, setShowCreate] = useState(false);

  // Load plugins
  const loadPlugins = () => {
    const installed = listPlugins();
    setPlugins(
      installed.map((p) => ({
        ...p,
        status: "installed" as const,
      }))
    );
  };

  useEffect(() => {
    loadPlugins();
  }, []);

  // Handle keyboard input
  useInput((input, key) => {
    if (key.escape || input === "q") {
      if (showCreate) {
        setShowCreate(false);
      } else {
        onBack();
      }
      return;
    }

    if (showCreate) {
      if (input === "r") {
        createPlugin("rust");
        setShowCreate(false);
      } else if (input === "p") {
        createPlugin("python");
        setShowCreate(false);
      }
      return;
    }

    // Navigation
    if (key.upArrow || input === "k") {
      setSelectedIndex((prev) => Math.max(0, prev - 1));
    } else if (key.downArrow || input === "j") {
      setSelectedIndex((prev) => Math.min(plugins.length - 1, prev + 1));
    }

    // Actions
    if (input === "n") {
      setShowCreate(true);
    } else if (input === "r" && plugins[selectedIndex]) {
      loadPlugins();
      setMessage("Refreshed plugin list");
      setTimeout(() => setMessage(null), 2000);
    } else if (input === "o" && plugins[selectedIndex]) {
      openInEditor(plugins[selectedIndex].path);
    }
  });

  // Create a new plugin
  const createPlugin = (type: "rust" | "python") => {
    const name = `my-${type}-plugin-${Date.now()}`;
    const projectRoot = findProjectRoot();

    if (!projectRoot) {
      setMessage("Error: Could not find project root");
      setTimeout(() => setMessage(null), 3000);
      return;
    }

    const bubbaloop = `${projectRoot}/target/debug/bubbaloop`;
    const child = spawn(bubbaloop, ["plugin", "init", name, "-t", type], {
      cwd: projectRoot,
    });

    child.on("close", (code) => {
      if (code === 0) {
        setMessage(`Created ${type} plugin: ${name}`);
        loadPlugins();
      } else {
        setMessage(`Failed to create plugin (exit code ${code})`);
      }
      setTimeout(() => setMessage(null), 3000);
    });

    child.on("error", (err) => {
      setMessage(`Error: ${err.message}`);
      setTimeout(() => setMessage(null), 3000);
    });
  };

  // Open plugin directory in editor
  const openInEditor = (path: string) => {
    const editor = process.env.EDITOR || "code";
    spawn(editor, [path], { detached: true, stdio: "ignore" }).unref();
    setMessage(`Opening ${path} in ${editor}`);
    setTimeout(() => setMessage(null), 2000);
  };

  // Create plugin dialog
  if (showCreate) {
    return (
      <Box flexDirection="column" padding={1}>
        <Box borderStyle="round" borderColor="#4ECDC4" paddingX={1}>
          <Text color="#4ECDC4" bold>
            Create New Plugin
          </Text>
        </Box>

        <Box flexDirection="column" marginTop={1} paddingX={1}>
          <Text>Select plugin type:</Text>
          <Text> </Text>
          <Text>
            <Text color="#4ECDC4">[r]</Text> Rust plugin
          </Text>
          <Text>
            <Text color="#4ECDC4">[p]</Text> Python plugin
          </Text>
          <Text> </Text>
          <Text color="#888">
            <Text color="#4ECDC4">esc</Text> cancel
          </Text>
        </Box>
      </Box>
    );
  }

  return (
    <Box flexDirection="column" padding={0}>
      {/* Header */}
      <Box
        borderStyle="round"
        borderColor="#4ECDC4"
        paddingX={1}
        justifyContent="space-between"
      >
        <Text color="#4ECDC4" bold>
          Plugins
        </Text>
        <Text color="#888">
          {plugins.length} installed •{" "}
          <Text color="#666">esc/q back</Text>
        </Text>
      </Box>

      {/* Info */}
      <Box marginX={1} marginTop={1}>
        <Text color="#888">
          Plugin directory: <Text color="#4ECDC4">{PLUGINS_DIR}</Text>
        </Text>
      </Box>
      <Box marginX={1}>
        <Text color="#888">
          Templates: <Text color="#4ECDC4">{getTemplatesDir() || "not found"}</Text>
        </Text>
      </Box>

      {/* Plugins table */}
      <Box
        flexDirection="column"
        borderStyle="single"
        borderColor="#444"
        marginTop={1}
      >
        {/* Table header */}
        <Box paddingX={1} borderBottom borderColor="#444">
          <Box width="25%">
            <Text color="#4ECDC4" bold>
              Name
            </Text>
          </Box>
          <Box width="15%">
            <Text color="#4ECDC4" bold>
              Version
            </Text>
          </Box>
          <Box width="15%">
            <Text color="#4ECDC4" bold>
              Type
            </Text>
          </Box>
          <Box width="45%">
            <Text color="#4ECDC4" bold>
              Description
            </Text>
          </Box>
        </Box>

        {/* Table rows */}
        {plugins.length === 0 ? (
          <Box paddingX={1} paddingY={1}>
            <Text color="#888">
              No plugins installed. Press <Text color="#4ECDC4">n</Text> to create one.
            </Text>
          </Box>
        ) : (
          plugins.map((plugin, index) => {
            const isSelected = index === selectedIndex;
            return (
              <Box key={plugin.path} paddingX={1}>
                <Box width="25%">
                  <Text color={isSelected ? "#4ECDC4" : "#CCC"}>
                    {isSelected ? "❯ " : "  "}
                    {plugin.manifest.name || "unknown"}
                  </Text>
                </Box>
                <Box width="15%">
                  <Text color="#95E1D3">
                    {plugin.manifest.version || "0.0.0"}
                  </Text>
                </Box>
                <Box width="15%">
                  <Text color={plugin.manifest.type === "rust" ? "#FFD93D" : "#4ECDC4"}>
                    {plugin.manifest.type || "unknown"}
                  </Text>
                </Box>
                <Box width="45%">
                  <Text color="#888">
                    {(plugin.manifest.description || "").slice(0, 40)}
                    {(plugin.manifest.description || "").length > 40 ? "..." : ""}
                  </Text>
                </Box>
              </Box>
            );
          })
        )}
      </Box>

      {/* Message */}
      {message && (
        <Box marginX={1} marginTop={1}>
          <Text color="#FFD93D">{message}</Text>
        </Box>
      )}

      {/* Footer with keybindings */}
      <Box marginX={1} marginTop={1}>
        <Text color="#666">
          <Text color="#4ECDC4">[n]</Text>ew{" "}
          <Text color="#4ECDC4">[o]</Text>pen in editor{" "}
          <Text color="#4ECDC4">[r]</Text>efresh{" "}
          <Text color="#4ECDC4">[q]</Text>back
        </Text>
      </Box>
    </Box>
  );
};

export default PluginsView;
