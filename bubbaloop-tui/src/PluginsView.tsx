import React, { useState, useEffect } from "react";
import { Box, Text, useInput } from "ink";
import TextInput from "ink-text-input";
import { spawn } from "child_process";
import {
  existsSync,
  mkdirSync,
  readdirSync,
  readFileSync,
  writeFileSync,
  symlinkSync,
  lstatSync,
  unlinkSync,
} from "fs";
import { join, relative } from "path";
import {
  listPlugins,
  PluginManifest,
  PLUGINS_DIR,
  findProjectRoot,
  getTemplatesDir,
  ensureDirectories,
} from "./config.js";

interface PluginsViewProps {
  onBack: () => void;
}

interface PluginInfo {
  path: string;
  manifest: PluginManifest;
  status: "installed" | "running" | "error";
  isLinked: boolean;
}

type CreateStep = "type" | "name" | "description" | "creating";
type DialogMode = "none" | "create" | "link";

interface CreateState {
  step: CreateStep;
  type: "rust" | "python";
  name: string;
  description: string;
}

interface LinkState {
  path: string;
}

// Convert kebab-case or snake_case to PascalCase
function toPascalCase(s: string): string {
  return s
    .split(/[-_]/)
    .map((word) => word.charAt(0).toUpperCase() + word.slice(1))
    .join("");
}

// Process template content with variable substitution
function processTemplate(content: string, vars: Record<string, string>): string {
  let result = content;
  for (const [key, value] of Object.entries(vars)) {
    result = result.replace(new RegExp(`\\{\\{${key}\\}\\}`, "g"), value);
  }
  return result;
}

// Recursively walk directory
function walkDir(dir: string): string[] {
  const files: string[] = [];
  const entries = readdirSync(dir, { withFileTypes: true });

  for (const entry of entries) {
    const fullPath = join(dir, entry.name);
    if (entry.isDirectory()) {
      files.push(...walkDir(fullPath));
    } else {
      files.push(fullPath);
    }
  }

  return files;
}

// Copy and process template directory
function copyTemplate(
  templateDir: string,
  outputDir: string,
  vars: Record<string, string>
): void {
  // Create output directory
  mkdirSync(outputDir, { recursive: true });

  // Get all files in template
  const files = walkDir(templateDir);

  for (const srcPath of files) {
    // Get relative path
    const relPath = relative(templateDir, srcPath);

    // Process filename (remove .template suffix)
    const destName = relPath.replace(/\.template$/, "");
    const destPath = join(outputDir, destName);

    // Create parent directories
    const destDir = join(destPath, "..");
    mkdirSync(destDir, { recursive: true });

    // Read, process, and write
    const content = readFileSync(srcPath, "utf-8");
    const processed = processTemplate(content, vars);
    writeFileSync(destPath, processed);
  }
}

// Check if a path is a symlink
function isSymlink(path: string): boolean {
  try {
    return lstatSync(path).isSymbolicLink();
  } catch {
    return false;
  }
}

const PluginsView: React.FC<PluginsViewProps> = ({ onBack }) => {
  const [plugins, setPlugins] = useState<PluginInfo[]>([]);
  const [selectedIndex, setSelectedIndex] = useState(0);
  const [message, setMessage] = useState<string | null>(null);
  const [dialogMode, setDialogMode] = useState<DialogMode>("none");
  const [createState, setCreateState] = useState<CreateState>({
    step: "type",
    type: "rust",
    name: "",
    description: "A Bubbaloop plugin",
  });
  const [linkState, setLinkState] = useState<LinkState>({ path: "" });

  // Load plugins
  const loadPlugins = () => {
    const installed = listPlugins();
    setPlugins(
      installed.map((p) => ({
        ...p,
        status: "installed" as const,
        isLinked: isSymlink(p.path),
      }))
    );
  };

  useEffect(() => {
    loadPlugins();
  }, []);

  // Reset create state
  const resetCreate = () => {
    setCreateState({
      step: "type",
      type: "rust",
      name: "",
      description: "A Bubbaloop plugin",
    });
    setDialogMode("none");
  };

  // Reset link state
  const resetLink = () => {
    setLinkState({ path: "" });
    setDialogMode("none");
  };

  // Create the plugin
  const doCreatePlugin = () => {
    // Ensure directories exist
    ensureDirectories();

    const templatesDir = getTemplatesDir();
    if (!templatesDir) {
      setMessage("Error: Templates directory not found");
      setTimeout(() => setMessage(null), 3000);
      resetCreate();
      return;
    }

    const templateDir = join(templatesDir, `${createState.type}-plugin`);
    if (!existsSync(templateDir)) {
      setMessage(`Error: Template not found: ${templateDir}`);
      setTimeout(() => setMessage(null), 3000);
      resetCreate();
      return;
    }

    const outputDir = join(PLUGINS_DIR, createState.name);
    if (existsSync(outputDir)) {
      setMessage(`Error: Plugin already exists: ${createState.name}`);
      setTimeout(() => setMessage(null), 3000);
      resetCreate();
      return;
    }

    // Template variables
    const vars = {
      plugin_name: createState.name,
      plugin_name_pascal: toPascalCase(createState.name),
      author: process.env.USER || "Anonymous",
      description: createState.description,
    };

    try {
      copyTemplate(templateDir, outputDir, vars);
      setMessage(`Created ${createState.type} plugin: ${createState.name}`);
      loadPlugins();
    } catch (err) {
      setMessage(`Error: ${err instanceof Error ? err.message : String(err)}`);
    }

    setTimeout(() => setMessage(null), 3000);
    resetCreate();
  };

  // Link an external plugin
  const doLinkPlugin = () => {
    ensureDirectories();

    const externalPath = linkState.path.trim();
    if (!externalPath) {
      setMessage("Path cannot be empty");
      setTimeout(() => setMessage(null), 2000);
      return;
    }

    // Expand ~ to home directory
    const expandedPath = externalPath.startsWith("~")
      ? join(process.env.HOME || "", externalPath.slice(1))
      : externalPath;

    // Check if directory exists
    if (!existsSync(expandedPath)) {
      setMessage(`Error: Directory not found: ${expandedPath}`);
      setTimeout(() => setMessage(null), 3000);
      resetLink();
      return;
    }

    // Check for plugin.yaml
    const manifestPath = join(expandedPath, "plugin.yaml");
    if (!existsSync(manifestPath)) {
      setMessage(`Error: No plugin.yaml found in ${expandedPath}`);
      setTimeout(() => setMessage(null), 3000);
      resetLink();
      return;
    }

    // Get plugin name from manifest
    let pluginName: string;
    try {
      const content = readFileSync(manifestPath, "utf-8");
      const match = content.match(/^name:\s*["']?([^"'\n]+)["']?$/m);
      pluginName = match ? match[1].trim() : "";
      if (!pluginName) {
        throw new Error("No name found in manifest");
      }
    } catch (err) {
      setMessage(`Error reading manifest: ${err instanceof Error ? err.message : String(err)}`);
      setTimeout(() => setMessage(null), 3000);
      resetLink();
      return;
    }

    // Create symlink
    const linkPath = join(PLUGINS_DIR, pluginName);
    if (existsSync(linkPath)) {
      setMessage(`Error: Plugin "${pluginName}" already exists`);
      setTimeout(() => setMessage(null), 3000);
      resetLink();
      return;
    }

    try {
      symlinkSync(expandedPath, linkPath);
      setMessage(`Linked plugin: ${pluginName} -> ${expandedPath}`);
      loadPlugins();
    } catch (err) {
      setMessage(`Error creating symlink: ${err instanceof Error ? err.message : String(err)}`);
    }

    setTimeout(() => setMessage(null), 3000);
    resetLink();
  };

  // Unlink a plugin (remove symlink only)
  const unlinkPlugin = (plugin: PluginInfo) => {
    if (!plugin.isLinked) {
      setMessage("Cannot unlink: not a linked plugin");
      setTimeout(() => setMessage(null), 2000);
      return;
    }

    try {
      unlinkSync(plugin.path);
      setMessage(`Unlinked plugin: ${plugin.manifest.name}`);
      loadPlugins();
    } catch (err) {
      setMessage(`Error unlinking: ${err instanceof Error ? err.message : String(err)}`);
    }
    setTimeout(() => setMessage(null), 2000);
  };

  // Handle keyboard input
  useInput(
    (input, key) => {
      // Handle escape
      if (key.escape) {
        if (dialogMode === "create") {
          resetCreate();
        } else if (dialogMode === "link") {
          resetLink();
        } else {
          onBack();
        }
        return;
      }

      // In create wizard - type selection
      if (dialogMode === "create" && createState.step === "type") {
        if (input === "r") {
          setCreateState((prev) => ({ ...prev, type: "rust", step: "name" }));
        } else if (input === "p") {
          setCreateState((prev) => ({ ...prev, type: "python", step: "name" }));
        }
        return;
      }

      // Main view input handling
      if (dialogMode === "none") {
        if (input === "q") {
          onBack();
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
          setDialogMode("create");
        } else if (input === "l") {
          setDialogMode("link");
        } else if (input === "r" && plugins.length > 0) {
          loadPlugins();
          setMessage("Refreshed plugin list");
          setTimeout(() => setMessage(null), 2000);
        } else if (input === "o" && plugins[selectedIndex]) {
          openInEditor(plugins[selectedIndex].path);
        } else if (input === "u" && plugins[selectedIndex]?.isLinked) {
          unlinkPlugin(plugins[selectedIndex]);
        } else if (input === "d" && plugins[selectedIndex]) {
          deletePlugin(plugins[selectedIndex]);
        }
      }
    },
    { isActive: dialogMode === "none" || (dialogMode === "create" && createState.step === "type") }
  );

  // Open plugin directory in editor
  const openInEditor = (path: string) => {
    const editor = process.env.EDITOR || "code";
    spawn(editor, [path], { detached: true, stdio: "ignore" }).unref();
    setMessage(`Opening ${path} in ${editor}`);
    setTimeout(() => setMessage(null), 2000);
  };

  // Delete plugin
  const deletePlugin = (plugin: PluginInfo) => {
    // Just show message - actual deletion would need confirmation
    setMessage(`Delete not implemented. Remove: ${plugin.path}`);
    setTimeout(() => setMessage(null), 3000);
  };

  // Handle name input submit
  const handleNameSubmit = (value: string) => {
    const name = value.trim();
    if (!name) {
      setMessage("Plugin name cannot be empty");
      setTimeout(() => setMessage(null), 2000);
      return;
    }
    // Validate name (alphanumeric, dashes, underscores)
    if (!/^[a-zA-Z][a-zA-Z0-9_-]*$/.test(name)) {
      setMessage("Invalid name. Use letters, numbers, dashes, underscores");
      setTimeout(() => setMessage(null), 2000);
      return;
    }
    setCreateState((prev) => ({ ...prev, name, step: "description" }));
  };

  // Handle description input submit
  const handleDescriptionSubmit = (value: string) => {
    const description = value.trim() || "A Bubbaloop plugin";
    setCreateState((prev) => ({ ...prev, description, step: "creating" }));
    // Trigger creation after state update
    setTimeout(() => doCreatePlugin(), 0);
  };

  // Handle link path input submit
  const handleLinkSubmit = (value: string) => {
    setLinkState({ path: value });
    setTimeout(() => doLinkPlugin(), 0);
  };

  // Link plugin dialog
  if (dialogMode === "link") {
    return (
      <Box flexDirection="column" padding={1}>
        <Box borderStyle="round" borderColor="#4ECDC4" paddingX={1}>
          <Text color="#4ECDC4" bold>
            Link External Plugin
          </Text>
        </Box>

        <Box flexDirection="column" marginTop={1} paddingX={1}>
          <Text>Enter path to plugin directory:</Text>
          <Text color="#888">(must contain plugin.yaml)</Text>
          <Box marginTop={1}>
            <Text color="#4ECDC4">{"❯ "}</Text>
            <TextInput
              value={linkState.path}
              onChange={(value) => setLinkState({ path: value })}
              onSubmit={handleLinkSubmit}
              placeholder="/path/to/my-plugin or ~/repos/my-plugin"
            />
          </Box>
          <Text> </Text>
          <Text color="#888">
            <Text color="#4ECDC4">enter</Text> confirm •{" "}
            <Text color="#4ECDC4">esc</Text> cancel
          </Text>
        </Box>

        {message && (
          <Box marginX={1} marginTop={1}>
            <Text color="#FF6B6B">{message}</Text>
          </Box>
        )}
      </Box>
    );
  }

  // Create plugin wizard - type selection
  if (dialogMode === "create" && createState.step === "type") {
    return (
      <Box flexDirection="column" padding={1}>
        <Box borderStyle="round" borderColor="#4ECDC4" paddingX={1}>
          <Text color="#4ECDC4" bold>
            Create New Plugin (1/3)
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

  // Create plugin wizard - name input
  if (dialogMode === "create" && createState.step === "name") {
    return (
      <Box flexDirection="column" padding={1}>
        <Box borderStyle="round" borderColor="#4ECDC4" paddingX={1}>
          <Text color="#4ECDC4" bold>
            Create New Plugin (2/3) - {createState.type}
          </Text>
        </Box>

        <Box flexDirection="column" marginTop={1} paddingX={1}>
          <Text>Enter plugin name (e.g., my-sensor):</Text>
          <Box marginTop={1}>
            <Text color="#4ECDC4">{"❯ "}</Text>
            <TextInput
              value={createState.name}
              onChange={(value) =>
                setCreateState((prev) => ({ ...prev, name: value }))
              }
              onSubmit={handleNameSubmit}
              placeholder="my-plugin"
            />
          </Box>
          <Text> </Text>
          <Text color="#888">
            <Text color="#4ECDC4">enter</Text> confirm •{" "}
            <Text color="#4ECDC4">esc</Text> cancel
          </Text>
        </Box>

        {message && (
          <Box marginX={1} marginTop={1}>
            <Text color="#FF6B6B">{message}</Text>
          </Box>
        )}
      </Box>
    );
  }

  // Create plugin wizard - description input
  if (dialogMode === "create" && createState.step === "description") {
    return (
      <Box flexDirection="column" padding={1}>
        <Box borderStyle="round" borderColor="#4ECDC4" paddingX={1}>
          <Text color="#4ECDC4" bold>
            Create New Plugin (3/3) - {createState.name}
          </Text>
        </Box>

        <Box flexDirection="column" marginTop={1} paddingX={1}>
          <Text>Enter plugin description:</Text>
          <Box marginTop={1}>
            <Text color="#4ECDC4">{"❯ "}</Text>
            <TextInput
              value={createState.description}
              onChange={(value) =>
                setCreateState((prev) => ({ ...prev, description: value }))
              }
              onSubmit={handleDescriptionSubmit}
              placeholder="A Bubbaloop plugin"
            />
          </Box>
          <Text> </Text>
          <Text color="#888">
            <Text color="#4ECDC4">enter</Text> confirm •{" "}
            <Text color="#4ECDC4">esc</Text> cancel
          </Text>
        </Box>
      </Box>
    );
  }

  // Creating state
  if (dialogMode === "create" && createState.step === "creating") {
    return (
      <Box flexDirection="column" padding={1}>
        <Box borderStyle="round" borderColor="#4ECDC4" paddingX={1}>
          <Text color="#4ECDC4" bold>
            Creating Plugin...
          </Text>
        </Box>
        <Box marginTop={1} paddingX={1}>
          <Text color="#FFD93D">Creating {createState.name}...</Text>
        </Box>
      </Box>
    );
  }

  // Main plugins list view
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
                    {plugin.isLinked ? "🔗 " : ""}
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
          <Text color="#4ECDC4">[l]</Text>ink{" "}
          <Text color="#4ECDC4">[o]</Text>pen{" "}
          {plugins[selectedIndex]?.isLinked && (
            <>
              <Text color="#4ECDC4">[u]</Text>nlink{" "}
            </>
          )}
          <Text color="#4ECDC4">[r]</Text>efresh{" "}
          <Text color="#4ECDC4">[q]</Text>back
        </Text>
      </Box>
    </Box>
  );
};

export default PluginsView;
