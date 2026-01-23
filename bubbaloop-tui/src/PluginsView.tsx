import React, { useState, useEffect } from "react";
import { Box, Text, useInput } from "ink";
import TextInput from "ink-text-input";
import { execSync, spawn } from "child_process";
import {
  existsSync,
  mkdirSync,
  readdirSync,
  readFileSync,
  writeFileSync,
  unlinkSync,
} from "fs";
import { join, relative, basename } from "path";
import {
  listPlugins,
  PluginManifest,
  PLUGINS_FILE,
  findProjectRoot,
  getTemplatesDir,
  ensureDirectories,
  registerPlugin,
  unregisterPlugin,
  getServiceName,
  getServicePath,
  generateServiceUnit,
  SYSTEMD_USER_DIR,
} from "./config.js";

interface PluginsViewProps {
  onBack: () => void;
}

type ServiceStatus = "stopped" | "running" | "failed" | "not-installed";

interface PluginInfo {
  path: string;
  manifest: PluginManifest;
  valid: boolean;
  serviceStatus: ServiceStatus;
  enabled: boolean;
}

type DialogMode = "none" | "create" | "register";
type CreateStep = "type" | "name" | "description" | "creating";

interface CreateState {
  step: CreateStep;
  type: "rust" | "python";
  name: string;
  description: string;
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
  mkdirSync(outputDir, { recursive: true });

  const files = walkDir(templateDir);

  for (const srcPath of files) {
    const relPath = relative(templateDir, srcPath);
    const destName = relPath.replace(/\.template$/, "");
    const destPath = join(outputDir, destName);

    const destDir = join(destPath, "..");
    mkdirSync(destDir, { recursive: true });

    const content = readFileSync(srcPath, "utf-8");
    const processed = processTemplate(content, vars);
    writeFileSync(destPath, processed);
  }
}

// Get systemd service status
function getServiceStatus(serviceName: string): { status: ServiceStatus; enabled: boolean } {
  try {
    const result = execSync(`systemctl --user is-active ${serviceName} 2>/dev/null`, {
      encoding: "utf-8",
    }).trim();

    let enabled = false;
    try {
      const enabledResult = execSync(`systemctl --user is-enabled ${serviceName} 2>/dev/null`, {
        encoding: "utf-8",
      }).trim();
      enabled = enabledResult === "enabled";
    } catch {
      enabled = false;
    }

    if (result === "active") {
      return { status: "running", enabled };
    } else if (result === "failed") {
      return { status: "failed", enabled };
    } else {
      return { status: "stopped", enabled };
    }
  } catch {
    return { status: "not-installed", enabled: false };
  }
}

// systemd commands
function installService(pluginPath: string, manifest: PluginManifest): { success: boolean; error?: string } {
  try {
    ensureDirectories();
    const servicePath = getServicePath(manifest.name);
    const unitContent = generateServiceUnit(pluginPath, manifest);
    writeFileSync(servicePath, unitContent);
    execSync("systemctl --user daemon-reload", { encoding: "utf-8" });
    return { success: true };
  } catch (err) {
    return { success: false, error: err instanceof Error ? err.message : String(err) };
  }
}

function uninstallService(manifest: PluginManifest): { success: boolean; error?: string } {
  try {
    const serviceName = getServiceName(manifest.name);
    const servicePath = getServicePath(manifest.name);

    // Stop and disable first
    try {
      execSync(`systemctl --user stop ${serviceName}`, { encoding: "utf-8" });
    } catch { /* ignore */ }
    try {
      execSync(`systemctl --user disable ${serviceName}`, { encoding: "utf-8" });
    } catch { /* ignore */ }

    // Remove service file
    if (existsSync(servicePath)) {
      unlinkSync(servicePath);
    }

    execSync("systemctl --user daemon-reload", { encoding: "utf-8" });
    return { success: true };
  } catch (err) {
    return { success: false, error: err instanceof Error ? err.message : String(err) };
  }
}

function startService(manifest: PluginManifest): { success: boolean; error?: string } {
  try {
    const serviceName = getServiceName(manifest.name);
    execSync(`systemctl --user start ${serviceName}`, { encoding: "utf-8" });
    return { success: true };
  } catch (err) {
    return { success: false, error: err instanceof Error ? err.message : String(err) };
  }
}

function stopService(manifest: PluginManifest): { success: boolean; error?: string } {
  try {
    const serviceName = getServiceName(manifest.name);
    execSync(`systemctl --user stop ${serviceName}`, { encoding: "utf-8" });
    return { success: true };
  } catch (err) {
    return { success: false, error: err instanceof Error ? err.message : String(err) };
  }
}

function enableService(manifest: PluginManifest): { success: boolean; error?: string } {
  try {
    const serviceName = getServiceName(manifest.name);
    execSync(`systemctl --user enable ${serviceName}`, { encoding: "utf-8" });
    return { success: true };
  } catch (err) {
    return { success: false, error: err instanceof Error ? err.message : String(err) };
  }
}

function disableService(manifest: PluginManifest): { success: boolean; error?: string } {
  try {
    const serviceName = getServiceName(manifest.name);
    execSync(`systemctl --user disable ${serviceName}`, { encoding: "utf-8" });
    return { success: true };
  } catch (err) {
    return { success: false, error: err instanceof Error ? err.message : String(err) };
  }
}

const PluginsView: React.FC<PluginsViewProps> = ({ onBack }) => {
  const [plugins, setPlugins] = useState<PluginInfo[]>([]);
  const [selectedIndex, setSelectedIndex] = useState(0);
  const [message, setMessage] = useState<string | null>(null);
  const [dialogMode, setDialogMode] = useState<DialogMode>("none");
  const [registerPath, setRegisterPath] = useState("");
  const [createState, setCreateState] = useState<CreateState>({
    step: "type",
    type: "rust",
    name: "",
    description: "A Bubbaloop plugin",
  });

  // Load plugins with service status
  const loadPlugins = () => {
    const registered = listPlugins();
    const withStatus = registered.map((p) => {
      const { status, enabled } = getServiceStatus(getServiceName(p.manifest.name));
      return {
        ...p,
        serviceStatus: status,
        enabled,
      };
    });
    setPlugins(withStatus);
  };

  useEffect(() => {
    loadPlugins();
    // Refresh status periodically
    const interval = setInterval(loadPlugins, 5000);
    return () => clearInterval(interval);
  }, []);

  const resetCreate = () => {
    setCreateState({
      step: "type",
      type: "rust",
      name: "",
      description: "A Bubbaloop plugin",
    });
    setDialogMode("none");
  };

  const resetRegister = () => {
    setRegisterPath("");
    setDialogMode("none");
  };

  // Create a new plugin from template
  const doCreatePlugin = () => {
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

    // Create in current directory or home
    const outputDir = join(process.cwd(), createState.name);
    if (existsSync(outputDir)) {
      setMessage(`Error: Directory already exists: ${outputDir}`);
      setTimeout(() => setMessage(null), 3000);
      resetCreate();
      return;
    }

    const vars = {
      plugin_name: createState.name,
      plugin_name_pascal: toPascalCase(createState.name),
      author: process.env.USER || "Anonymous",
      description: createState.description,
    };

    try {
      copyTemplate(templateDir, outputDir, vars);
      // Auto-register the new plugin
      const result = registerPlugin(outputDir);
      if (result.success) {
        setMessage(`Created and registered: ${createState.name} at ${outputDir}`);
        loadPlugins();
      } else {
        setMessage(`Created at ${outputDir}, but failed to register: ${result.error}`);
      }
    } catch (err) {
      setMessage(`Error: ${err instanceof Error ? err.message : String(err)}`);
    }

    setTimeout(() => setMessage(null), 4000);
    resetCreate();
  };

  // Register an existing plugin
  const doRegisterPlugin = () => {
    const result = registerPlugin(registerPath);
    if (result.success) {
      setMessage(`Registered plugin from: ${registerPath}`);
      loadPlugins();
    } else {
      setMessage(`Error: ${result.error}`);
    }
    setTimeout(() => setMessage(null), 3000);
    resetRegister();
  };

  // Handle keyboard input
  useInput(
    (input, key) => {
      if (key.escape) {
        if (dialogMode !== "none") {
          if (dialogMode === "create") resetCreate();
          else resetRegister();
        } else {
          onBack();
        }
        return;
      }

      if (dialogMode === "create" && createState.step === "type") {
        if (input === "r") {
          setCreateState((prev) => ({ ...prev, type: "rust", step: "name" }));
        } else if (input === "p") {
          setCreateState((prev) => ({ ...prev, type: "python", step: "name" }));
        }
        return;
      }

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

        const plugin = plugins[selectedIndex];

        // Actions
        if (input === "n") {
          setDialogMode("create");
        } else if (input === "a") {
          setDialogMode("register");
        } else if (input === "R") {
          loadPlugins();
          setMessage("Refreshed plugin list");
          setTimeout(() => setMessage(null), 2000);
        } else if (input === "o" && plugin) {
          openInEditor(plugin.path);
        } else if (input === "x" && plugin) {
          // Unregister
          const result = unregisterPlugin(plugin.path);
          if (result.success) {
            // Also uninstall service if installed
            if (plugin.serviceStatus !== "not-installed") {
              uninstallService(plugin.manifest);
            }
            setMessage(`Unregistered: ${plugin.manifest.name}`);
            loadPlugins();
          } else {
            setMessage(`Error: ${result.error}`);
          }
          setTimeout(() => setMessage(null), 2000);
        } else if (input === "i" && plugin && plugin.serviceStatus === "not-installed") {
          // Install service
          const result = installService(plugin.path, plugin.manifest);
          if (result.success) {
            setMessage(`Installed service: ${plugin.manifest.name}`);
            loadPlugins();
          } else {
            setMessage(`Error: ${result.error}`);
          }
          setTimeout(() => setMessage(null), 2000);
        } else if (input === "u" && plugin && plugin.serviceStatus !== "not-installed") {
          // Uninstall service
          const result = uninstallService(plugin.manifest);
          if (result.success) {
            setMessage(`Uninstalled service: ${plugin.manifest.name}`);
            loadPlugins();
          } else {
            setMessage(`Error: ${result.error}`);
          }
          setTimeout(() => setMessage(null), 2000);
        } else if (input === "s" && plugin && plugin.serviceStatus === "stopped") {
          // Start service
          const result = startService(plugin.manifest);
          if (result.success) {
            setMessage(`Started: ${plugin.manifest.name}`);
            loadPlugins();
          } else {
            setMessage(`Error: ${result.error}`);
          }
          setTimeout(() => setMessage(null), 2000);
        } else if (input === "s" && plugin && plugin.serviceStatus === "running") {
          // Stop service
          const result = stopService(plugin.manifest);
          if (result.success) {
            setMessage(`Stopped: ${plugin.manifest.name}`);
            loadPlugins();
          } else {
            setMessage(`Error: ${result.error}`);
          }
          setTimeout(() => setMessage(null), 2000);
        } else if (input === "e" && plugin && plugin.serviceStatus !== "not-installed") {
          // Toggle enable/disable
          if (plugin.enabled) {
            const result = disableService(plugin.manifest);
            if (result.success) {
              setMessage(`Disabled autostart: ${plugin.manifest.name}`);
              loadPlugins();
            } else {
              setMessage(`Error: ${result.error}`);
            }
          } else {
            const result = enableService(plugin.manifest);
            if (result.success) {
              setMessage(`Enabled autostart: ${plugin.manifest.name}`);
              loadPlugins();
            } else {
              setMessage(`Error: ${result.error}`);
            }
          }
          setTimeout(() => setMessage(null), 2000);
        }
      }
    },
    { isActive: dialogMode === "none" || (dialogMode === "create" && createState.step === "type") }
  );

  const openInEditor = (path: string) => {
    const editor = process.env.EDITOR || "code";
    spawn(editor, [path], { detached: true, stdio: "ignore" }).unref();
    setMessage(`Opening ${path} in ${editor}`);
    setTimeout(() => setMessage(null), 2000);
  };

  const handleNameSubmit = (value: string) => {
    const name = value.trim();
    if (!name) {
      setMessage("Plugin name cannot be empty");
      setTimeout(() => setMessage(null), 2000);
      return;
    }
    if (!/^[a-zA-Z][a-zA-Z0-9_-]*$/.test(name)) {
      setMessage("Invalid name. Use letters, numbers, dashes, underscores");
      setTimeout(() => setMessage(null), 2000);
      return;
    }
    setCreateState((prev) => ({ ...prev, name, step: "description" }));
  };

  const handleDescriptionSubmit = (value: string) => {
    const description = value.trim() || "A Bubbaloop plugin";
    setCreateState((prev) => ({ ...prev, description, step: "creating" }));
    setTimeout(() => doCreatePlugin(), 0);
  };

  const handleRegisterSubmit = (value: string) => {
    setRegisterPath(value);
    setTimeout(() => doRegisterPlugin(), 0);
  };

  // Status indicator
  const StatusBadge: React.FC<{ status: ServiceStatus; enabled: boolean }> = ({ status, enabled }) => {
    const config: Record<ServiceStatus, { color: string; symbol: string }> = {
      running: { color: "#95E1D3", symbol: "●" },
      stopped: { color: "#888", symbol: "○" },
      failed: { color: "#FF6B6B", symbol: "✗" },
      "not-installed": { color: "#666", symbol: "-" },
    };
    const { color, symbol } = config[status];
    return (
      <Text>
        <Text color={color}>{symbol}</Text>
        {enabled && status !== "not-installed" && <Text color="#FFD93D"> ⚡</Text>}
      </Text>
    );
  };

  // Register plugin dialog
  if (dialogMode === "register") {
    return (
      <Box flexDirection="column" padding={1}>
        <Box borderStyle="round" borderColor="#4ECDC4" paddingX={1}>
          <Text color="#4ECDC4" bold>
            Register Plugin
          </Text>
        </Box>

        <Box flexDirection="column" marginTop={1} paddingX={1}>
          <Text>Enter path to plugin directory:</Text>
          <Text color="#888">(must contain plugin.yaml)</Text>
          <Box marginTop={1}>
            <Text color="#4ECDC4">{"❯ "}</Text>
            <TextInput
              value={registerPath}
              onChange={setRegisterPath}
              onSubmit={handleRegisterSubmit}
              placeholder="/path/to/plugin or ~/projects/my-plugin"
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
              onChange={(value) => setCreateState((prev) => ({ ...prev, name: value }))}
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
              onChange={(value) => setCreateState((prev) => ({ ...prev, description: value }))}
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

  const plugin = plugins[selectedIndex];

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
          {plugins.length} registered •{" "}
          <Text color="#666">esc/q back</Text>
        </Text>
      </Box>

      {/* Info */}
      <Box marginX={1} marginTop={1}>
        <Text color="#888">
          Registry: <Text color="#4ECDC4">{PLUGINS_FILE}</Text>
        </Text>
      </Box>
      <Box marginX={1}>
        <Text color="#888">
          Services: <Text color="#4ECDC4">{SYSTEMD_USER_DIR}</Text>
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
          <Box width={3}>
            <Text color="#4ECDC4" bold>St</Text>
          </Box>
          <Box width="20%">
            <Text color="#4ECDC4" bold>Name</Text>
          </Box>
          <Box width="10%">
            <Text color="#4ECDC4" bold>Version</Text>
          </Box>
          <Box width="10%">
            <Text color="#4ECDC4" bold>Type</Text>
          </Box>
          <Box width="55%">
            <Text color="#4ECDC4" bold>Path</Text>
          </Box>
        </Box>

        {/* Table rows */}
        {plugins.length === 0 ? (
          <Box paddingX={1} paddingY={1}>
            <Text color="#888">
              No plugins registered. Press <Text color="#4ECDC4">n</Text> to create or{" "}
              <Text color="#4ECDC4">a</Text> to add existing.
            </Text>
          </Box>
        ) : (
          plugins.map((p, index) => {
            const isSelected = index === selectedIndex;
            return (
              <Box key={p.path} paddingX={1}>
                <Box width={3}>
                  <StatusBadge status={p.serviceStatus} enabled={p.enabled} />
                </Box>
                <Box width="20%">
                  <Text color={isSelected ? "#4ECDC4" : p.valid ? "#CCC" : "#FF6B6B"}>
                    {isSelected ? "❯ " : "  "}
                    {p.manifest.name}
                  </Text>
                </Box>
                <Box width="10%">
                  <Text color="#95E1D3">{p.manifest.version}</Text>
                </Box>
                <Box width="10%">
                  <Text color={p.manifest.type === "rust" ? "#FFD93D" : "#4ECDC4"}>
                    {p.manifest.type}
                  </Text>
                </Box>
                <Box width="55%">
                  <Text color="#888">
                    {p.path.length > 45 ? "..." + p.path.slice(-42) : p.path}
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
      <Box marginX={1} marginTop={1} flexDirection="column">
        <Text color="#666">
          <Text color="#4ECDC4">[n]</Text>ew{" "}
          <Text color="#4ECDC4">[a]</Text>dd existing{" "}
          <Text color="#4ECDC4">[x]</Text> unregister{" "}
          <Text color="#4ECDC4">[o]</Text>pen{" "}
          <Text color="#4ECDC4">[R]</Text>efresh
        </Text>
        {plugin && (
          <Text color="#666">
            {plugin.serviceStatus === "not-installed" ? (
              <><Text color="#4ECDC4">[i]</Text>nstall service</>
            ) : (
              <>
                <Text color="#4ECDC4">[s]</Text>{plugin.serviceStatus === "running" ? "top" : "tart"}{" "}
                <Text color="#4ECDC4">[e]</Text>{plugin.enabled ? " disable" : " enable"} autostart{" "}
                <Text color="#4ECDC4">[u]</Text>ninstall service
              </>
            )}
          </Text>
        )}
      </Box>

      {/* Legend */}
      <Box marginX={1} marginTop={1}>
        <Text color="#666">
          <Text color="#95E1D3">●</Text> running{" "}
          <Text color="#888">○</Text> stopped{" "}
          <Text color="#FF6B6B">✗</Text> failed{" "}
          <Text color="#666">-</Text> no service{" "}
          <Text color="#FFD93D">⚡</Text> autostart
        </Text>
      </Box>
    </Box>
  );
};

export default PluginsView;
