/**
 * localStorage utilities for persisting dashboard state
 */

const STORAGE_KEYS = {
  CAMERAS: 'bubbaloop-cameras',
  CAMERA_ORDER: 'bubbaloop-camera-order',
} as const;

export interface CameraConfig {
  id: string;
  name: string;
  topic: string;
}

/**
 * Load camera configurations from localStorage
 */
export function loadCameras(): CameraConfig[] | null {
  try {
    const stored = localStorage.getItem(STORAGE_KEYS.CAMERAS);
    if (stored) {
      return JSON.parse(stored);
    }
  } catch (e) {
    console.warn('[Storage] Failed to load cameras:', e);
  }
  return null;
}

/**
 * Save camera configurations to localStorage
 */
export function saveCameras(cameras: CameraConfig[]): void {
  try {
    localStorage.setItem(STORAGE_KEYS.CAMERAS, JSON.stringify(cameras));
  } catch (e) {
    console.warn('[Storage] Failed to save cameras:', e);
  }
}

/**
 * Load camera order from localStorage
 */
export function loadCameraOrder(): string[] | null {
  try {
    const stored = localStorage.getItem(STORAGE_KEYS.CAMERA_ORDER);
    if (stored) {
      return JSON.parse(stored);
    }
  } catch (e) {
    console.warn('[Storage] Failed to load camera order:', e);
  }
  return null;
}

/**
 * Save camera order to localStorage
 */
export function saveCameraOrder(order: string[]): void {
  try {
    localStorage.setItem(STORAGE_KEYS.CAMERA_ORDER, JSON.stringify(order));
  } catch (e) {
    console.warn('[Storage] Failed to save camera order:', e);
  }
}

/**
 * Generate a unique ID for a new camera
 */
export function generateCameraId(): string {
  return `cam-${Date.now()}-${Math.random().toString(36).slice(2, 7)}`;
}
