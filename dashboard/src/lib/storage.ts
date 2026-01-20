/**
 * localStorage utilities for persisting dashboard state
 */

const STORAGE_KEYS = {
  PANELS: 'bubbaloop-panels',
  PANEL_ORDER: 'bubbaloop-panel-order',
  // Legacy keys for migration
  CAMERAS: 'bubbaloop-cameras',
  CAMERA_ORDER: 'bubbaloop-camera-order',
} as const;

export type PanelType = 'camera' | 'json' | 'weather' | 'stats';

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

export interface WeatherPanelConfig extends BasePanelConfig {
  type: 'weather';
}

export interface StatsPanelConfig extends BasePanelConfig {
  type: 'stats';
}

export type PanelConfig = CameraPanelConfig | JsonPanelConfig | WeatherPanelConfig | StatsPanelConfig;

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
    weather: 'weather',
    stats: 'stats',
  };
  const prefix = prefixes[type];
  return `${prefix}-${Date.now()}-${Math.random().toString(36).slice(2, 7)}`;
}
