/**
 * Client for communicating with bubbaloop-daemon HTTP API
 */

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

export class DaemonClient {
  private baseUrl: string;

  constructor(port: number = 8088) {
    this.baseUrl = `http://localhost:${port}`;
  }

  /**
   * Check if daemon is running
   */
  async isAvailable(): Promise<boolean> {
    try {
      const response = await fetch(`${this.baseUrl}/health`, {
        signal: AbortSignal.timeout(1000),
      });
      return response.ok;
    } catch {
      return false;
    }
  }

  /**
   * Get all nodes
   */
  async listNodes(): Promise<NodeListResponse> {
    const response = await fetch(`${this.baseUrl}/nodes`);
    if (!response.ok) {
      throw new Error(`Failed to list nodes: ${response.statusText}`);
    }
    return response.json();
  }

  /**
   * Get a single node's state
   */
  async getNode(name: string): Promise<NodeState | null> {
    const response = await fetch(`${this.baseUrl}/nodes/${encodeURIComponent(name)}`);
    if (response.status === 404) {
      return null;
    }
    if (!response.ok) {
      throw new Error(`Failed to get node: ${response.statusText}`);
    }
    return response.json();
  }

  /**
   * Execute a command on a node
   */
  async executeCommand(
    nodeName: string,
    command: string,
    nodePath?: string
  ): Promise<CommandResponse> {
    const response = await fetch(
      `${this.baseUrl}/nodes/${encodeURIComponent(nodeName)}/command`,
      {
        method: "POST",
        headers: { "Content-Type": "application/json" },
        body: JSON.stringify({ command, node_path: nodePath || "" }),
      }
    );
    if (!response.ok) {
      throw new Error(`Failed to execute command: ${response.statusText}`);
    }
    return response.json();
  }

  /**
   * Add a new node
   */
  async addNode(path: string): Promise<CommandResponse> {
    const response = await fetch(`${this.baseUrl}/nodes/add`, {
      method: "POST",
      headers: { "Content-Type": "application/json" },
      body: JSON.stringify({ command: "add", node_path: path }),
    });
    if (!response.ok) {
      throw new Error(`Failed to add node: ${response.statusText}`);
    }
    return response.json();
  }

  /**
   * Refresh all node states
   */
  async refresh(): Promise<CommandResponse> {
    const response = await fetch(`${this.baseUrl}/refresh`, {
      method: "POST",
    });
    if (!response.ok) {
      throw new Error(`Failed to refresh: ${response.statusText}`);
    }
    return response.json();
  }

  // Convenience methods for common commands
  async startNode(name: string): Promise<CommandResponse> {
    return this.executeCommand(name, "start");
  }

  async stopNode(name: string): Promise<CommandResponse> {
    return this.executeCommand(name, "stop");
  }

  async restartNode(name: string): Promise<CommandResponse> {
    return this.executeCommand(name, "restart");
  }

  async installNode(name: string): Promise<CommandResponse> {
    return this.executeCommand(name, "install");
  }

  async uninstallNode(name: string): Promise<CommandResponse> {
    return this.executeCommand(name, "uninstall");
  }

  async buildNode(name: string): Promise<CommandResponse> {
    return this.executeCommand(name, "build");
  }

  async cleanNode(name: string): Promise<CommandResponse> {
    return this.executeCommand(name, "clean");
  }

  async enableAutostart(name: string): Promise<CommandResponse> {
    return this.executeCommand(name, "enable_autostart");
  }

  async disableAutostart(name: string): Promise<CommandResponse> {
    return this.executeCommand(name, "disable_autostart");
  }

  async removeNode(name: string): Promise<CommandResponse> {
    return this.executeCommand(name, "remove");
  }
}

// Default daemon client instance
export const daemonClient = new DaemonClient();
