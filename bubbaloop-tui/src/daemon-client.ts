import { Session, Config, ReplyError, GetOptions } from "@eclipse-zenoh/zenoh-ts";
import { Duration } from "typed-duration";

export interface NodeState {
  name: string;
  path: string;
  status: "unknown" | "stopped" | "running" | "failed" | "installing" | "building" | "not-installed";
  installed: boolean;
  autostart_enabled: boolean;
  version: string;
  description: string;
  node_type: string;
  is_built: boolean;
  build_output: string[];
}

export interface NodeListResponse {
  nodes: NodeState[];
  timestamp_ms: number;
}

export interface CommandResponse {
  success: boolean;
  message: string;
  output: string;
  node_state?: NodeState;
}

export interface LogsResponse {
  node_name: string;
  lines: string[];
  success: boolean;
  error?: string;
}

const API_PREFIX = "bubbaloop/daemon/api";
const API_HEALTH = `${API_PREFIX}/health`;
const API_NODES = `${API_PREFIX}/nodes`;
const API_NODES_ADD = `${API_PREFIX}/nodes/add`;
const API_REFRESH = `${API_PREFIX}/refresh`;

const DEFAULT_WS_ENDPOINT = "ws://127.0.0.1:10001";

async function withSuppressedConsole<T>(fn: () => Promise<T>): Promise<T> {
  const originalLog = console.log;
  console.log = () => {};
  try {
    return await fn();
  } finally {
    console.log = originalLog;
  }
}

export class DaemonClient {
  private session: Session | null = null;
  private connecting: boolean = false;
  private connectionPromise: Promise<void> | null = null;
  private endpoint: string;

  constructor(endpoint: string = DEFAULT_WS_ENDPOINT) {
    this.endpoint = endpoint;
  }

  setEndpoint(endpoint: string): void {
    this.endpoint = endpoint;
  }

  async connect(): Promise<void> {
    if (this.session) return;
    if (this.connecting && this.connectionPromise) return this.connectionPromise;

    this.connecting = true;
    this.connectionPromise = (async () => {
      try {
        // Suppress console.log to prevent Zenoh "Connected to..." message from corrupting TUI
        const config = new Config(this.endpoint, 2000);
        this.session = await withSuppressedConsole(() => Session.open(config));
      } catch (err) {
        // Clear promise on failure to allow retries
        this.connectionPromise = null;
        throw err;
      } finally {
        this.connecting = false;
      }
    })();

    return this.connectionPromise;
  }

  async disconnect(): Promise<void> {
    if (!this.session) return;
    await this.session.close();
    this.session = null;
  }

  private async getSession(): Promise<Session> {
    if (!this.session) await this.connect();
    if (!this.session) throw new Error("Failed to connect to Zenoh");
    return this.session;
  }

  private async query<T>(keyExpr: string, payload?: object): Promise<T> {
    const session = await this.getSession();

    const options: GetOptions = {
      timeout: Duration.milliseconds.of(5000),
    };

    if (payload) {
      const jsonStr = JSON.stringify(payload);
      options.payload = jsonStr;
    }

    const receiver = await session.get(keyExpr, options);
    if (!receiver) {
      throw new Error(`No receiver for query to ${keyExpr}`);
    }

    for await (const reply of receiver) {
      const result = reply.result();

      if (result instanceof ReplyError) {
        const errorPayload = result.payload().toString();
        throw new Error(`Query error: ${errorPayload}`);
      }

      // It's a Sample
      const sample = result;
      const text = sample.payload().toString();
      return JSON.parse(text) as T;
    }

    throw new Error(`No reply received for ${keyExpr}`);
  }

  async isAvailable(): Promise<boolean> {
    try {
      const response = await this.query<{ status: string }>(API_HEALTH);
      return response.status === "ok";
    } catch {
      return false;
    }
  }

  async listNodes(): Promise<NodeListResponse> {
    return this.query<NodeListResponse>(API_NODES);
  }

  async getNode(name: string): Promise<NodeState | null> {
    try {
      return await this.query<NodeState>(`${API_NODES}/${encodeURIComponent(name)}`);
    } catch (e) {
      const error = e as Error;
      if (error.message?.includes("404") || error.message?.includes("not found")) {
        return null;
      }
      throw e;
    }
  }

  async getLogs(name: string): Promise<LogsResponse> {
    return this.query<LogsResponse>(`${API_NODES}/${encodeURIComponent(name)}/logs`);
  }

  async executeCommand(nodeName: string, command: string, nodePath?: string): Promise<CommandResponse> {
    return this.query<CommandResponse>(
      `${API_NODES}/${encodeURIComponent(nodeName)}/command`,
      { command, node_path: nodePath ?? "" }
    );
  }

  async addNode(path: string): Promise<CommandResponse> {
    return this.query<CommandResponse>(API_NODES_ADD, { command: "add", node_path: path });
  }

  async refresh(): Promise<CommandResponse> {
    return this.query<CommandResponse>(API_REFRESH);
  }

  startNode(name: string): Promise<CommandResponse> {
    return this.executeCommand(name, "start");
  }

  stopNode(name: string): Promise<CommandResponse> {
    return this.executeCommand(name, "stop");
  }

  restartNode(name: string): Promise<CommandResponse> {
    return this.executeCommand(name, "restart");
  }

  installNode(name: string): Promise<CommandResponse> {
    return this.executeCommand(name, "install");
  }

  uninstallNode(name: string): Promise<CommandResponse> {
    return this.executeCommand(name, "uninstall");
  }

  buildNode(name: string): Promise<CommandResponse> {
    return this.executeCommand(name, "build");
  }

  cleanNode(name: string): Promise<CommandResponse> {
    return this.executeCommand(name, "clean");
  }

  enableAutostart(name: string): Promise<CommandResponse> {
    return this.executeCommand(name, "enable_autostart");
  }

  disableAutostart(name: string): Promise<CommandResponse> {
    return this.executeCommand(name, "disable_autostart");
  }

  removeNode(name: string): Promise<CommandResponse> {
    return this.executeCommand(name, "remove");
  }
}

// Default daemon client instance
export const daemonClient = new DaemonClient();
