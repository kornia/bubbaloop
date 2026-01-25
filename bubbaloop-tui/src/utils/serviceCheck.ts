import { exec } from 'child_process';
import { promisify } from 'util';
import * as net from 'net';

const execAsync = promisify(exec);

export interface ServiceStatus {
  zenohd: boolean;
  bridge: boolean;
  daemon: boolean;
}

/**
 * Check if systemd user services are running
 */
export async function checkServices(): Promise<ServiceStatus> {
  const check = async (service: string): Promise<boolean> => {
    try {
      await execAsync(`systemctl --user is-active --quiet bubbaloop-${service}`);
      return true;
    } catch {
      return false;
    }
  };

  const [zenohd, bridge, daemon] = await Promise.all([
    check('zenohd'),
    check('bridge'),
    check('daemon'),
  ]);

  return { zenohd, bridge, daemon };
}

/**
 * Start all bubbaloop services
 */
export async function startServices(): Promise<void> {
  try {
    // Start zenohd first
    await execAsync('systemctl --user start bubbaloop-zenohd');
    // Wait for zenohd to be ready
    await new Promise(r => setTimeout(r, 2000));
    // Start bridge and daemon
    await execAsync('systemctl --user start bubbaloop-bridge bubbaloop-daemon');
    // Wait for services to initialize
    await new Promise(r => setTimeout(r, 2000));
  } catch (error) {
    const msg = error instanceof Error ? error.message : String(error);
    throw new Error(`Failed to start services: ${msg}`);
  }
}

/**
 * Check if the WebSocket port (10001) is ready
 */
export function checkWebSocketReady(port: number = 10001, host: string = '127.0.0.1'): Promise<boolean> {
  return new Promise((resolve) => {
    const socket = new net.Socket();
    socket.setTimeout(1000);

    socket.on('connect', () => {
      socket.destroy();
      resolve(true);
    });

    socket.on('timeout', () => {
      socket.destroy();
      resolve(false);
    });

    socket.on('error', () => {
      socket.destroy();
      resolve(false);
    });

    socket.connect(port, host);
  });
}

/**
 * Wait for WebSocket to become ready with retries
 */
export async function waitForWebSocket(
  port: number = 10001,
  host: string = '127.0.0.1',
  maxRetries: number = 10,
  retryDelay: number = 1000
): Promise<boolean> {
  for (let i = 0; i < maxRetries; i++) {
    if (await checkWebSocketReady(port, host)) {
      return true;
    }
    await new Promise(r => setTimeout(r, retryDelay));
  }
  return false;
}

/**
 * Ensure services are running, starting them if needed
 * Returns true if services are ready, false otherwise
 */
export async function ensureServicesRunning(): Promise<{ ready: boolean; started: boolean; error?: string }> {
  try {
    const status = await checkServices();

    // Only consider ready when all services are running
    if (status.zenohd && status.bridge && status.daemon) {
      if (await checkWebSocketReady()) {
        return { ready: true, started: false };
      }
    }

    // Need to start services
    await startServices();

    // Wait for WebSocket to be ready
    const ready = await waitForWebSocket();

    if (ready) {
      return { ready: true, started: true };
    } else {
      return { ready: false, started: true, error: 'Services started but WebSocket not ready' };
    }
  } catch (error) {
    const msg = error instanceof Error ? error.message : String(error);
    return { ready: false, started: false, error: msg };
  }
}
