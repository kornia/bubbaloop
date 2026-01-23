import { readFileSync, writeFileSync, mkdirSync, existsSync, readdirSync } from "fs";
import { homedir } from "os";
import { join } from "path";

export interface BubbaloopConfig {
  // WebSocket endpoint for TUI to connect to local zenohd (ws://...)
  endpoint?: string;
  // TCP endpoint of remote server for zenohd to connect to (tcp/ip:port)
  serverEndpoint?: string;
}

// Standard bubbaloop directories
export const BUBBALOOP_HOME = join(homedir(), ".bubbaloop");
export const CONFIG_DIR = BUBBALOOP_HOME;
export const CONFIG_FILE = join(CONFIG_DIR, "config.json");
export const ZENOH_CLI_CONFIG = join(CONFIG_DIR, "zenoh.cli.json5");
export const PLUGINS_FILE = join(BUBBALOOP_HOME, "plugins.json");
export const LAUNCH_DIR = join(BUBBALOOP_HOME, "launch");
export const SYSTEMD_USER_DIR = join(homedir(), ".config/systemd/user");

// Find project root (where Cargo.toml is)
export function findProjectRoot(): string | null {
  // Check BUBBALOOP_ROOT env var first
  const envRoot = process.env.BUBBALOOP_ROOT;
  if (envRoot && existsSync(join(envRoot, "Cargo.toml"))) {
    return envRoot;
  }

  // Walk up from cwd
  let dir = process.cwd();
  while (dir !== "/") {
    if (existsSync(join(dir, "Cargo.toml"))) {
      return dir;
    }
    dir = join(dir, "..");
  }

  return null;
}

// Get templates directory
export function getTemplatesDir(): string | null {
  const projectRoot = findProjectRoot();
  if (projectRoot) {
    const templatesDir = join(projectRoot, "templates");
    if (existsSync(templatesDir)) {
      return templatesDir;
    }
  }

  // Check standard locations
  const locations = [
    join(BUBBALOOP_HOME, "templates"),
    "/usr/share/bubbaloop/templates",
  ];

  for (const loc of locations) {
    if (existsSync(loc)) {
      return loc;
    }
  }

  return null;
}

// Get launch files directory
export function getLaunchDir(): string | null {
  const projectRoot = findProjectRoot();
  if (projectRoot) {
    const launchDir = join(projectRoot, "launch");
    if (existsSync(launchDir)) {
      return launchDir;
    }
  }

  if (existsSync(LAUNCH_DIR)) {
    return LAUNCH_DIR;
  }

  return null;
}

// List available launch files
export function listLaunchFiles(): string[] {
  const launchDir = getLaunchDir();
  if (!launchDir) return [];

  try {
    return readdirSync(launchDir)
      .filter((f) => f.endsWith(".launch.yaml") || f.endsWith(".yaml"))
      .map((f) => join(launchDir, f));
  } catch {
    return [];
  }
}

// Plugin manifest (from plugin.yaml)
export interface PluginManifest {
  name: string;
  version: string;
  type: "rust" | "python";
  description: string;
  author?: string;
  // Optional: command to run (defaults based on type)
  command?: string;
}

// Plugin registry entry (stored in plugins.json)
export interface PluginEntry {
  path: string;
  addedAt: string;
}

// Plugins registry
export interface PluginsRegistry {
  plugins: PluginEntry[];
}

// Load plugins registry
export function loadPluginsRegistry(): PluginsRegistry {
  try {
    if (existsSync(PLUGINS_FILE)) {
      const data = readFileSync(PLUGINS_FILE, "utf-8");
      return JSON.parse(data);
    }
  } catch {
    // Ignore errors
  }
  return { plugins: [] };
}

// Save plugins registry
export function savePluginsRegistry(registry: PluginsRegistry): void {
  try {
    if (!existsSync(CONFIG_DIR)) {
      mkdirSync(CONFIG_DIR, { recursive: true });
    }
    writeFileSync(PLUGINS_FILE, JSON.stringify(registry, null, 2));
  } catch {
    // Ignore save errors
  }
}

// Register a plugin (add to registry)
export function registerPlugin(pluginPath: string): { success: boolean; error?: string } {
  // Expand ~ to home directory
  const expandedPath = pluginPath.startsWith("~")
    ? join(homedir(), pluginPath.slice(1))
    : pluginPath;

  // Check if directory exists
  if (!existsSync(expandedPath)) {
    return { success: false, error: `Directory not found: ${expandedPath}` };
  }

  // Check for plugin.yaml
  const manifestPath = join(expandedPath, "plugin.yaml");
  if (!existsSync(manifestPath)) {
    return { success: false, error: `No plugin.yaml found in ${expandedPath}` };
  }

  // Load registry
  const registry = loadPluginsRegistry();

  // Check if already registered
  if (registry.plugins.some((p) => p.path === expandedPath)) {
    return { success: false, error: "Plugin already registered" };
  }

  // Add to registry
  registry.plugins.push({
    path: expandedPath,
    addedAt: new Date().toISOString(),
  });

  savePluginsRegistry(registry);
  return { success: true };
}

// Unregister a plugin (remove from registry)
export function unregisterPlugin(pluginPath: string): { success: boolean; error?: string } {
  const registry = loadPluginsRegistry();
  const index = registry.plugins.findIndex((p) => p.path === pluginPath);

  if (index === -1) {
    return { success: false, error: "Plugin not found in registry" };
  }

  registry.plugins.splice(index, 1);
  savePluginsRegistry(registry);
  return { success: true };
}

// Simple YAML parser for plugin manifests
function parseSimpleYaml(content: string): Record<string, string> {
  const result: Record<string, string> = {};
  const lines = content.split("\n");

  for (const line of lines) {
    const match = line.match(/^(\w+):\s*["']?([^"'\n]+)["']?$/);
    if (match) {
      result[match[1]] = match[2].trim();
    }
  }

  return result;
}

// Read plugin manifest from path
export function readPluginManifest(pluginPath: string): PluginManifest | null {
  const manifestPath = join(pluginPath, "plugin.yaml");
  if (!existsSync(manifestPath)) {
    return null;
  }

  try {
    const content = readFileSync(manifestPath, "utf-8");
    return parseSimpleYaml(content) as PluginManifest;
  } catch {
    return null;
  }
}

// List registered plugins with their manifests
export function listPlugins(): { path: string; manifest: PluginManifest; valid: boolean }[] {
  const registry = loadPluginsRegistry();
  const plugins: { path: string; manifest: PluginManifest; valid: boolean }[] = [];

  for (const entry of registry.plugins) {
    const manifest = readPluginManifest(entry.path);
    if (manifest) {
      plugins.push({ path: entry.path, manifest, valid: true });
    } else {
      // Plugin path no longer valid, but keep in list to show error
      plugins.push({
        path: entry.path,
        manifest: {
          name: "unknown",
          version: "0.0.0",
          type: "rust",
          description: "Plugin not found",
        },
        valid: false,
      });
    }
  }

  return plugins;
}

// Get systemd service name for a plugin
export function getServiceName(pluginName: string): string {
  return `bubbaloop-plugin-${pluginName}.service`;
}

// Get systemd service file path
export function getServicePath(pluginName: string): string {
  return join(SYSTEMD_USER_DIR, getServiceName(pluginName));
}

// Generate systemd service unit file content
export function generateServiceUnit(pluginPath: string, manifest: PluginManifest): string {
  let execStart: string;
  let environment = "RUST_LOG=info";

  if (manifest.command) {
    execStart = manifest.command;
  } else if (manifest.type === "rust") {
    // For Rust: use cargo run --release (builds if needed, then runs)
    execStart = `/usr/bin/cargo run --release`;
  } else {
    // For Python: use local venv if exists, otherwise system python
    const venvPython = join(pluginPath, "venv/bin/python");
    execStart = `${venvPython} main.py`;
    environment = "PYTHONUNBUFFERED=1";
  }

  return `[Unit]
Description=Bubbaloop Plugin: ${manifest.name}
After=network.target

[Service]
Type=simple
WorkingDirectory=${pluginPath}
ExecStart=${execStart}
Restart=on-failure
RestartSec=5
Environment=${environment}

[Install]
WantedBy=default.target
`;
}

export function loadConfig(): BubbaloopConfig {
  try {
    if (existsSync(CONFIG_FILE)) {
      const data = readFileSync(CONFIG_FILE, "utf-8");
      return JSON.parse(data);
    }
  } catch {
    // Ignore errors, return default config
  }
  return {};
}

export function saveConfig(config: BubbaloopConfig): void {
  try {
    if (!existsSync(CONFIG_DIR)) {
      mkdirSync(CONFIG_DIR, { recursive: true });
    }
    writeFileSync(CONFIG_FILE, JSON.stringify(config, null, 2));

    // Also update zenoh.cli.json5 if serverEndpoint is set
    if (config.serverEndpoint) {
      updateZenohCliConfig(config.serverEndpoint);
    }
  } catch {
    // Ignore save errors
  }
}

// Generate zenoh.cli.json5 for zenohd client mode
export function updateZenohCliConfig(serverEndpoint: string): void {
  const zenohConfig = `{
  // Auto-generated by bubbaloop TUI
  // Server endpoint: ${serverEndpoint}
  mode: "router",
  connect: {
    endpoints: ["${serverEndpoint}"],
  },
  plugins: {
    remote_api: {
      websocket_port: 10000,
    },
  },
}
`;
  try {
    if (!existsSync(CONFIG_DIR)) {
      mkdirSync(CONFIG_DIR, { recursive: true });
    }
    writeFileSync(ZENOH_CLI_CONFIG, zenohConfig);
  } catch {
    // Ignore errors
  }
}

export function getZenohCliConfigPath(): string {
  return ZENOH_CLI_CONFIG;
}

// Ensure all bubbaloop directories exist
export function ensureDirectories(): void {
  const dirs = [BUBBALOOP_HOME, LAUNCH_DIR, SYSTEMD_USER_DIR];
  for (const dir of dirs) {
    if (!existsSync(dir)) {
      try {
        mkdirSync(dir, { recursive: true });
      } catch {
        // Ignore
      }
    }
  }
}

export const DEFAULT_ENDPOINT = "ws://127.0.0.1:10000";
