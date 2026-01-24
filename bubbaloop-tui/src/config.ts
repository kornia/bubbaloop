import { readFileSync, writeFileSync, mkdirSync, existsSync, readdirSync } from "fs";
import { homedir } from "os";
import { join, resolve } from "path";

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
export const NODES_DIR = join(BUBBALOOP_HOME, "nodes");
export const NODES_FILE = join(BUBBALOOP_HOME, "nodes.json");
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

// Node manifest (from node.yaml)
export interface NodeManifest {
  name: string;
  version: string;
  type: "rust" | "python";
  description: string;
  author?: string;
  // Optional: build command (runs from project root)
  build?: string;
  // Optional: command to run (defaults based on type)
  command?: string;
}

// Node registry entry (stored in nodes.json)
export interface NodeEntry {
  path: string;
  addedAt: string;
}

// Nodes registry
export interface NodesRegistry {
  nodes: NodeEntry[];
}

// Load nodes registry
export function loadNodesRegistry(): NodesRegistry {
  try {
    if (existsSync(NODES_FILE)) {
      const data = readFileSync(NODES_FILE, "utf-8");
      return JSON.parse(data);
    }
  } catch {
    // Ignore errors
  }
  return { nodes: [] };
}

// Save nodes registry
export function saveNodesRegistry(registry: NodesRegistry): void {
  try {
    if (!existsSync(CONFIG_DIR)) {
      mkdirSync(CONFIG_DIR, { recursive: true });
    }
    writeFileSync(NODES_FILE, JSON.stringify(registry, null, 2));
  } catch {
    // Ignore save errors
  }
}

// Register a node (add to registry)
export function registerNode(nodePath: string): { success: boolean; error?: string } {
  // Expand ~ to home directory
  const expandedPath = nodePath.startsWith("~")
    ? join(homedir(), nodePath.slice(1))
    : nodePath;

  // Check if directory exists
  if (!existsSync(expandedPath)) {
    return { success: false, error: `Directory not found: ${expandedPath}` };
  }

  // Check for node.yaml
  const manifestPath = join(expandedPath, "node.yaml");
  if (!existsSync(manifestPath)) {
    return { success: false, error: `No node.yaml found in ${expandedPath}` };
  }

  // Load registry
  const registry = loadNodesRegistry();

  // Check if already registered
  if (registry.nodes.some((n) => n.path === expandedPath)) {
    return { success: false, error: "Node already registered" };
  }

  // Add to registry
  registry.nodes.push({
    path: expandedPath,
    addedAt: new Date().toISOString(),
  });

  saveNodesRegistry(registry);
  return { success: true };
}

// Unregister a node (remove from registry)
export function unregisterNode(nodePath: string): { success: boolean; error?: string } {
  const registry = loadNodesRegistry();
  const index = registry.nodes.findIndex((n) => n.path === nodePath);

  if (index === -1) {
    return { success: false, error: "Node not found in registry" };
  }

  registry.nodes.splice(index, 1);
  saveNodesRegistry(registry);
  return { success: true };
}

// Simple YAML parser for node manifests
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

// Read node manifest from path
export function readNodeManifest(nodePath: string): NodeManifest | null {
  const manifestPath = join(nodePath, "node.yaml");
  if (!existsSync(manifestPath)) {
    return null;
  }

  try {
    const content = readFileSync(manifestPath, "utf-8");
    return parseSimpleYaml(content) as unknown as NodeManifest;
  } catch {
    return null;
  }
}

// List registered nodes with their manifests
export function listNodes(): { path: string; manifest: NodeManifest; valid: boolean }[] {
  const registry = loadNodesRegistry();
  const nodes: { path: string; manifest: NodeManifest; valid: boolean }[] = [];

  for (const entry of registry.nodes) {
    const manifest = readNodeManifest(entry.path);
    if (manifest) {
      nodes.push({ path: entry.path, manifest, valid: true });
    } else {
      // Node path no longer valid, but keep in list to show error
      nodes.push({
        path: entry.path,
        manifest: {
          name: "unknown",
          version: "0.0.0",
          type: "rust",
          description: "Node not found",
        },
        valid: false,
      });
    }
  }

  return nodes;
}

// Get systemd service name for a node
export function getServiceName(nodeName: string): string {
  return `bubbaloop-${nodeName}.service`;
}

// Get systemd service file path
export function getServicePath(nodeName: string): string {
  return join(SYSTEMD_USER_DIR, getServiceName(nodeName));
}

// Generate systemd service unit file content
export function generateServiceUnit(nodePath: string, manifest: NodeManifest): string {
  let execStart: string;
  let environment = "RUST_LOG=info";
  const cargoPath = join(homedir(), ".cargo/bin/cargo");
  const pixiBin = join(homedir(), ".pixi/bin");
  const pathEnv = `PATH=${join(homedir(), ".cargo/bin")}:${pixiBin}:/usr/local/bin:/usr/bin:/bin`;

  if (manifest.command) {
    // If command starts with cargo, replace with full path
    if (manifest.command.startsWith("cargo ")) {
      execStart = manifest.command.replace(/^cargo /, `${cargoPath} `);
    } else {
      // Resolve relative paths to absolute paths (required by systemd)
      execStart = resolve(nodePath, manifest.command);
    }
  } else if (manifest.type === "rust") {
    // For Rust: use cargo run --release (builds if needed, then runs)
    execStart = `${cargoPath} run --release`;
  } else {
    // For Python: use local venv if exists, otherwise system python
    const venvPython = join(nodePath, "venv/bin/python");
    execStart = `${venvPython} main.py`;
    environment = "PYTHONUNBUFFERED=1";
  }

  return `[Unit]
Description=Bubbaloop Node: ${manifest.name}
After=network.target

[Service]
Type=simple
WorkingDirectory=${nodePath}
ExecStart=${execStart}
Restart=on-failure
RestartSec=5
Environment=${environment}
Environment=${pathEnv}

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
  const dirs = [BUBBALOOP_HOME, NODES_DIR, LAUNCH_DIR, SYSTEMD_USER_DIR];
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
