/**
 * localStorage utilities for persisting dashboard state
 */

// Bump this version to force-clear stale panel configs (e.g., when topic format changes)
const PANEL_FORMAT_VERSION = 3;

const STORAGE_KEYS = {
  PANELS: 'bubbaloop-panels',
  PANEL_ORDER: 'bubbaloop-panel-order',
  PANEL_VERSION: 'bubbaloop-panel-version',
  // Legacy keys for migration
  CAMERAS: 'bubbaloop-cameras',
  CAMERA_ORDER: 'bubbaloop-camera-order',
} as const;

export type PanelType = 'camera' | 'json' | 'rawdata' | 'weather' | 'stats' | 'nodes' | 'telemetry' | 'network';

export interface BasePanelConfig {
  id: string;
  name: string;
  topic: string;
  type: PanelType;
}

export interface CameraPanelConfig extends BasePanelConfig {
  type: 'camera';
}

export interface JsonPanelConfig extends BasePanelConfig {
  type: 'json';
}

export interface RawDataPanelConfig extends BasePanelConfig {
  type: 'rawdata';
}

export interface WeatherPanelConfig extends BasePanelConfig {
  type: 'weather';
}

export interface StatsPanelConfig extends BasePanelConfig {
  type: 'stats';
}

export interface NodesPanelConfig extends BasePanelConfig {
  type: 'nodes';
}

export interface TelemetryPanelConfig extends BasePanelConfig {
  type: 'telemetry';
}

export interface NetworkPanelConfig extends BasePanelConfig {
  type: 'network';
}

export type PanelConfig = CameraPanelConfig | JsonPanelConfig | RawDataPanelConfig | WeatherPanelConfig | StatsPanelConfig | NodesPanelConfig | TelemetryPanelConfig | NetworkPanelConfig;

// Legacy type for migration
export interface LegacyCameraConfig {
  id: string;
  name: string;
  topic: string;
}

/**
 * Migrate legacy camera configs to new panel format
 */
function migrateLegacyCameras(): PanelConfig[] | null {
  try {
    const stored = localStorage.getItem(STORAGE_KEYS.CAMERAS);
    if (stored) {
      const cameras: LegacyCameraConfig[] = JSON.parse(stored);
      // Convert to new format
      const panels: PanelConfig[] = cameras.map((c) => ({
        ...c,
        type: 'camera' as const,
      }));
      // Save in new format and clean up legacy
      localStorage.setItem(STORAGE_KEYS.PANELS, JSON.stringify(panels));
      localStorage.removeItem(STORAGE_KEYS.CAMERAS);
      // Migrate order too
      const order = localStorage.getItem(STORAGE_KEYS.CAMERA_ORDER);
      if (order) {
        localStorage.setItem(STORAGE_KEYS.PANEL_ORDER, order);
        localStorage.removeItem(STORAGE_KEYS.CAMERA_ORDER);
      }
      return panels;
    }
  } catch (e) {
    console.warn('[Storage] Failed to migrate legacy cameras:', e);
  }
  return null;
}

/**
 * Load panel configurations from localStorage
 */
export function loadPanels(): PanelConfig[] | null {
  try {
    // Check panel format version - if outdated, clear and return null to use fresh defaults
    const storedVersion = localStorage.getItem(STORAGE_KEYS.PANEL_VERSION);
    const currentVersion = String(PANEL_FORMAT_VERSION);
    if (storedVersion !== currentVersion) {
      console.log(`[Storage] Panel format version changed (${storedVersion} -> ${currentVersion}), clearing stale panels`);
      // Clear new-format data only; preserve legacy keys so migration can still run
      localStorage.removeItem(STORAGE_KEYS.PANELS);
      localStorage.removeItem(STORAGE_KEYS.PANEL_ORDER);
      localStorage.setItem(STORAGE_KEYS.PANEL_VERSION, currentVersion);
    }

    const stored = localStorage.getItem(STORAGE_KEYS.PANELS);
    if (stored) {
      return JSON.parse(stored);
    }
    // Try migrating legacy format
    return migrateLegacyCameras();
  } catch (e) {
    console.warn('[Storage] Failed to load panels:', e);
  }
  return null;
}

/**
 * Save panel configurations to localStorage
 */
export function savePanels(panels: PanelConfig[]): void {
  try {
    localStorage.setItem(STORAGE_KEYS.PANELS, JSON.stringify(panels));
    localStorage.setItem(STORAGE_KEYS.PANEL_VERSION, String(PANEL_FORMAT_VERSION));
  } catch (e) {
    console.warn('[Storage] Failed to save panels:', e);
  }
}

/**
 * Load panel order from localStorage
 */
export function loadPanelOrder(): string[] | null {
  try {
    const stored = localStorage.getItem(STORAGE_KEYS.PANEL_ORDER);
    if (stored) {
      return JSON.parse(stored);
    }
    // Try legacy key
    const legacy = localStorage.getItem(STORAGE_KEYS.CAMERA_ORDER);
    if (legacy) {
      const order = JSON.parse(legacy);
      localStorage.setItem(STORAGE_KEYS.PANEL_ORDER, legacy);
      localStorage.removeItem(STORAGE_KEYS.CAMERA_ORDER);
      return order;
    }
  } catch (e) {
    console.warn('[Storage] Failed to load panel order:', e);
  }
  return null;
}

/**
 * Save panel order to localStorage
 */
export function savePanelOrder(order: string[]): void {
  try {
    localStorage.setItem(STORAGE_KEYS.PANEL_ORDER, JSON.stringify(order));
  } catch (e) {
    console.warn('[Storage] Failed to save panel order:', e);
  }
}

/**
 * Generate a unique ID for a new panel
 */
export function generatePanelId(type: PanelType): string {
  const prefixes: Record<PanelType, string> = {
    camera: 'cam',
    json: 'json',
    rawdata: 'rawdata',
    weather: 'weather',
    stats: 'stats',
    nodes: 'nodes',
    telemetry: 'telemetry',
    network: 'network',
  };
  const prefix = prefixes[type];
  return `${prefix}-${Date.now()}-${Math.random().toString(36).slice(2, 7)}`;
}
