import { describe, it, expect, beforeEach } from 'vitest';
import {
  generatePanelId,
  savePanels,
  loadPanels,
  savePanelOrder,
  loadPanelOrder,
  type PanelConfig,
} from '../storage';

describe('generatePanelId', () => {
  it('generates id with correct prefix for camera', () => {
    const id = generatePanelId('camera');
    expect(id).toMatch(/^cam-\d+-[a-z0-9]+$/);
  });

  it('generates id with correct prefix for each type', () => {
    expect(generatePanelId('json')).toMatch(/^json-/);
    expect(generatePanelId('weather')).toMatch(/^weather-/);
    expect(generatePanelId('stats')).toMatch(/^stats-/);
    expect(generatePanelId('nodes')).toMatch(/^nodes-/);
    expect(generatePanelId('telemetry')).toMatch(/^telemetry-/);
    expect(generatePanelId('network')).toMatch(/^network-/);
    expect(generatePanelId('rawdata')).toMatch(/^rawdata-/);
  });

  it('generates unique ids', () => {
    const ids = new Set(Array.from({ length: 10 }, () => generatePanelId('camera')));
    expect(ids.size).toBe(10);
  });
});

describe('savePanels / loadPanels', () => {
  beforeEach(() => {
    localStorage.clear();
  });

  it('roundtrips panel configs', () => {
    const panels: PanelConfig[] = [
      { id: 'cam-1', name: 'Camera 1', topic: 'topic1', type: 'camera' },
      { id: 'weather-1', name: 'Weather', topic: 'weather/**', type: 'weather' },
    ];
    savePanels(panels);
    const loaded = loadPanels();
    expect(loaded).toEqual(panels);
  });

  it('returns null when no panels stored and no legacy', () => {
    // Set the version so it doesn't clear
    localStorage.setItem('bubbaloop-panel-version', '3');
    const loaded = loadPanels();
    expect(loaded).toBeNull();
  });

  it('returns null when version mismatch', () => {
    localStorage.setItem('bubbaloop-panel-version', '1');
    localStorage.setItem('bubbaloop-panels', JSON.stringify([{ id: '1' }]));
    const loaded = loadPanels();
    expect(loaded).toBeNull();
    // Should have cleared the stale data
    expect(localStorage.getItem('bubbaloop-panels')).toBeNull();
  });

  it('handles corrupted JSON gracefully', () => {
    localStorage.setItem('bubbaloop-panel-version', '3');
    localStorage.setItem('bubbaloop-panels', '{invalid json');
    const loaded = loadPanels();
    expect(loaded).toBeNull();
  });

  it('saves version when saving panels', () => {
    const panels: PanelConfig[] = [
      { id: 'cam-1', name: 'Test', topic: 'test', type: 'camera' },
    ];
    savePanels(panels);
    expect(localStorage.getItem('bubbaloop-panel-version')).toBe('3');
  });

  it('migrates legacy cameras to new format', () => {
    // Set up legacy storage format
    const legacyCameras = [
      { id: 'cam-1', name: 'Legacy Camera', topic: 'camera/legacy' },
    ];
    localStorage.setItem('bubbaloop-cameras', JSON.stringify(legacyCameras));
    localStorage.setItem('bubbaloop-camera-order', JSON.stringify(['cam-1']));

    const loaded = loadPanels();
    expect(loaded).toEqual([
      { id: 'cam-1', name: 'Legacy Camera', topic: 'camera/legacy', type: 'camera' },
    ]);

    // Should have cleaned up legacy keys
    expect(localStorage.getItem('bubbaloop-cameras')).toBeNull();
  });

  it('regression: version mismatch with legacy data still migrates successfully', () => {
    // This is the exact bug that was fixed: when the version changed,
    // the old code cleared ALL keys (including legacy) before migration could run.
    // The fix preserves legacy keys so migration still works.
    const legacyCameras = [
      { id: 'cam-1', name: 'Legacy Cam', topic: 'camera/legacy' },
      { id: 'cam-2', name: 'Legacy Cam 2', topic: 'camera/legacy2' },
    ];
    // Simulate: old version stored, plus legacy camera data
    localStorage.setItem('bubbaloop-panel-version', '1');
    localStorage.setItem('bubbaloop-cameras', JSON.stringify(legacyCameras));
    localStorage.setItem('bubbaloop-camera-order', JSON.stringify(['cam-1', 'cam-2']));

    const loaded = loadPanels();

    // Migration should succeed despite version mismatch
    expect(loaded).toEqual([
      { id: 'cam-1', name: 'Legacy Cam', topic: 'camera/legacy', type: 'camera' },
      { id: 'cam-2', name: 'Legacy Cam 2', topic: 'camera/legacy2', type: 'camera' },
    ]);

    // Version should be updated
    expect(localStorage.getItem('bubbaloop-panel-version')).toBe('3');
    // Legacy keys should be cleaned up after migration
    expect(localStorage.getItem('bubbaloop-cameras')).toBeNull();
  });

  it('version mismatch clears stale new-format data but preserves legacy keys', () => {
    // When version changes, stale new-format panels should be cleared,
    // but legacy keys must be preserved for migration
    localStorage.setItem('bubbaloop-panel-version', '2');
    localStorage.setItem('bubbaloop-panels', JSON.stringify([{ id: 'stale-panel' }]));
    localStorage.setItem('bubbaloop-panel-order', JSON.stringify(['stale-panel']));
    localStorage.setItem('bubbaloop-cameras', JSON.stringify([
      { id: 'cam-1', name: 'Legacy', topic: 'camera/test' },
    ]));

    const loaded = loadPanels();

    // Stale new-format data should be cleared, legacy data should migrate
    expect(loaded).toEqual([
      { id: 'cam-1', name: 'Legacy', topic: 'camera/test', type: 'camera' },
    ]);
    // Legacy key cleaned up after migration
    expect(localStorage.getItem('bubbaloop-cameras')).toBeNull();
  });
});

describe('savePanelOrder / loadPanelOrder', () => {
  beforeEach(() => {
    localStorage.clear();
  });

  it('roundtrips panel order', () => {
    const order = ['panel-1', 'panel-2', 'panel-3'];
    savePanelOrder(order);
    const loaded = loadPanelOrder();
    expect(loaded).toEqual(order);
  });

  it('returns null when no order stored', () => {
    const loaded = loadPanelOrder();
    expect(loaded).toBeNull();
  });

  it('migrates legacy camera order', () => {
    const legacyOrder = ['cam-1', 'cam-2'];
    localStorage.setItem('bubbaloop-camera-order', JSON.stringify(legacyOrder));

    const loaded = loadPanelOrder();
    expect(loaded).toEqual(legacyOrder);

    // Should have migrated to new key
    expect(localStorage.getItem('bubbaloop-panel-order')).toBe(JSON.stringify(legacyOrder));
    expect(localStorage.getItem('bubbaloop-camera-order')).toBeNull();
  });

  it('handles corrupted JSON gracefully', () => {
    localStorage.setItem('bubbaloop-panel-order', '{invalid json');
    const loaded = loadPanelOrder();
    expect(loaded).toBeNull();
  });
});
