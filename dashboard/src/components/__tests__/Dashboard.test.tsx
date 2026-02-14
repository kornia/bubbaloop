import { vi, describe, it, expect, beforeEach } from 'vitest';
import React from 'react';

// ---- Mock modules BEFORE importing the component ----

// Track calls to savePanels and savePanelOrder
const mockSavePanels = vi.fn();
const mockSavePanelOrder = vi.fn();
let panelIdCounter = 0;

vi.mock('../../lib/storage', () => ({
  loadPanels: vi.fn(() => null),
  savePanels: (...args: unknown[]) => mockSavePanels(...args),
  loadPanelOrder: vi.fn(() => null),
  savePanelOrder: (...args: unknown[]) => mockSavePanelOrder(...args),
  generatePanelId: vi.fn((type: string) => `${type}-test-${++panelIdCounter}`),
  PanelType: {},
}));

// Mock all Sortable card components to simplify testing
vi.mock('../SortableCameraCard', () => ({
  SortableCameraCard: React.memo(({ id, cameraName, onRemove }: { id: string; cameraName: string; onRemove: () => void }) => (
    <div data-testid={`camera-card-${id}`}>
      <span>Camera: {cameraName}</span>
      <button onClick={onRemove}>Remove</button>
    </div>
  )),
}));

vi.mock('../SortableJsonCard', () => ({
  SortableJsonCard: React.memo(({ id, onRemove }: { id: string; onRemove: () => void }) => (
    <div data-testid={`json-card-${id}`}>
      <span>Raw Data</span>
      <button onClick={onRemove}>Remove</button>
    </div>
  )),
}));

vi.mock('../SortableWeatherCard', () => ({
  SortableWeatherCard: React.memo(({ id, onRemove }: { id: string; onRemove: () => void }) => (
    <div data-testid={`weather-card-${id}`}>
      <span>Weather</span>
      <button onClick={onRemove}>Remove</button>
    </div>
  )),
}));

vi.mock('../SortableStatsCard', () => ({
  SortableStatsCard: React.memo(({ id, onRemove }: { id: string; onRemove: () => void }) => (
    <div data-testid={`stats-card-${id}`}>
      <span>Stats</span>
      <button onClick={onRemove}>Remove</button>
    </div>
  )),
}));

vi.mock('../SortableNodesCard', () => ({
  SortableNodesCard: React.memo(({ id, onRemove }: { id: string; onRemove: () => void }) => (
    <div data-testid={`nodes-card-${id}`}>
      <span>Nodes</span>
      <button onClick={onRemove}>Remove</button>
    </div>
  )),
}));

vi.mock('../SortableSystemTelemetryCard', () => ({
  SortableSystemTelemetryCard: React.memo(({ id, onRemove }: { id: string; onRemove: () => void }) => (
    <div data-testid={`telemetry-card-${id}`}>
      <span>Telemetry</span>
      <button onClick={onRemove}>Remove</button>
    </div>
  )),
}));

vi.mock('../SortableNetworkMonitorCard', () => ({
  SortableNetworkMonitorCard: React.memo(({ id, onRemove }: { id: string; onRemove: () => void }) => (
    <div data-testid={`network-card-${id}`}>
      <span>Network</span>
      <button onClick={onRemove}>Remove</button>
    </div>
  )),
}));

// Mock dnd-kit to avoid drag-and-drop complexity in unit tests
vi.mock('@dnd-kit/core', () => ({
  DndContext: ({ children }: { children: React.ReactNode }) => <div>{children}</div>,
  closestCenter: vi.fn(),
  KeyboardSensor: vi.fn(),
  PointerSensor: vi.fn(),
  useSensor: vi.fn(() => ({})),
  useSensors: vi.fn(() => []),
}));

vi.mock('@dnd-kit/sortable', () => ({
  SortableContext: ({ children }: { children: React.ReactNode }) => <div>{children}</div>,
  sortableKeyboardCoordinates: vi.fn(),
  rectSortingStrategy: vi.fn(),
  arrayMove: vi.fn((arr: unknown[], oldIndex: number, newIndex: number) => {
    const result = [...arr];
    const [removed] = result.splice(oldIndex, 1);
    result.splice(newIndex, 0, removed);
    return result;
  }),
}));

// ---- Now import the component and testing utilities ----

import { render, screen, fireEvent, waitFor } from '@testing-library/react';
import { Dashboard } from '../Dashboard';
import { loadPanels } from '../../lib/storage';

describe('Dashboard', () => {
  beforeEach(() => {
    vi.clearAllMocks();
    panelIdCounter = 0;
    localStorage.clear();
    // Default: no stored panels
    (loadPanels as ReturnType<typeof vi.fn>).mockReturnValue(null);
  });

  it('renders "No panels configured" when no stored panels and empty cameras', () => {
    render(<Dashboard cameras={[]} />);

    expect(screen.getByText('No panels configured')).toBeInTheDocument();
  });

  it('renders default camera panels from cameras prop', () => {
    const cameras = [
      { name: 'entrance', topic: 'topic-1' },
      { name: 'parking', topic: 'topic-2' },
    ];

    render(<Dashboard cameras={cameras} />);

    expect(screen.getByText('Camera: entrance')).toBeInTheDocument();
    expect(screen.getByText('Camera: parking')).toBeInTheDocument();
  });

  it('shows panel count in header', () => {
    const cameras = [
      { name: 'cam1', topic: 't1' },
      { name: 'cam2', topic: 't2' },
    ];

    render(<Dashboard cameras={cameras} />);

    expect(screen.getByText('Panels (2)')).toBeInTheDocument();
  });

  it('Add Panel button opens menu with all panel types', () => {
    render(<Dashboard cameras={[]} />);

    const addBtn = screen.getByRole('button', { name: /Add Panel/i });
    fireEvent.click(addBtn);

    expect(screen.getByText('Camera Panel')).toBeInTheDocument();
    expect(screen.getByText('Raw Data Panel')).toBeInTheDocument();
    expect(screen.getByText('Weather Panel')).toBeInTheDocument();
    expect(screen.getByText('Stats Panel')).toBeInTheDocument();
    expect(screen.getByText('Nodes Panel')).toBeInTheDocument();
    expect(screen.getByText('Telemetry Panel')).toBeInTheDocument();
    expect(screen.getByText('Network Panel')).toBeInTheDocument();
  });

  it('adding a camera panel creates a new camera panel', async () => {
    render(<Dashboard cameras={[]} />);

    // Open menu
    const addBtn = screen.getByRole('button', { name: /Add Panel/i });
    fireEvent.click(addBtn);

    // Click camera
    fireEvent.click(screen.getByText('Camera Panel'));

    // Panel should now be visible
    await waitFor(() => {
      expect(screen.getByText('Panels (1)')).toBeInTheDocument();
    });

    // savePanels should have been called with the new camera panel
    expect(mockSavePanels).toHaveBeenCalled();
    const lastCallArgs = mockSavePanels.mock.calls[mockSavePanels.mock.calls.length - 1][0];
    expect(lastCallArgs).toEqual(
      expect.arrayContaining([
        expect.objectContaining({ type: 'camera', name: 'Camera 1' }),
      ])
    );
  });

  it('adding each panel type creates correct config', { timeout: 15000 }, async () => {
    const panelTypes = [
      { menuLabel: 'Camera Panel', expectedType: 'camera' },
      { menuLabel: 'Raw Data Panel', expectedType: 'rawdata' },
      { menuLabel: 'Weather Panel', expectedType: 'weather' },
      { menuLabel: 'Stats Panel', expectedType: 'stats' },
      { menuLabel: 'Nodes Panel', expectedType: 'nodes' },
      { menuLabel: 'Telemetry Panel', expectedType: 'telemetry' },
      { menuLabel: 'Network Panel', expectedType: 'network' },
    ];

    for (const { menuLabel, expectedType } of panelTypes) {
      vi.clearAllMocks();
      panelIdCounter = 0;
      (loadPanels as ReturnType<typeof vi.fn>).mockReturnValue(null);

      const { unmount } = render(<Dashboard cameras={[]} />);

      // Open menu and add panel
      fireEvent.click(screen.getByRole('button', { name: /Add Panel/i }));
      fireEvent.click(screen.getByText(menuLabel));

      await waitFor(() => {
        expect(mockSavePanels).toHaveBeenCalled();
      });

      const lastCallArgs = mockSavePanels.mock.calls[mockSavePanels.mock.calls.length - 1][0];
      expect(lastCallArgs).toEqual(
        expect.arrayContaining([
          expect.objectContaining({ type: expectedType }),
        ])
      );

      unmount();
    }
  });

  it('remove panel removes from both panels and order', async () => {
    const cameras = [
      { name: 'entrance', topic: 'topic-1' },
      { name: 'parking', topic: 'topic-2' },
    ];

    render(<Dashboard cameras={cameras} />);

    // Click the first Remove button
    const removeButtons = screen.getAllByText('Remove');
    fireEvent.click(removeButtons[0]);

    await waitFor(() => {
      expect(screen.getByText('Panels (1)')).toBeInTheDocument();
    });

    // savePanels should have been called with one fewer panel
    const lastPanelsCall = mockSavePanels.mock.calls[mockSavePanels.mock.calls.length - 1][0];
    expect(lastPanelsCall).toHaveLength(1);

    // savePanelOrder should also have been called with one fewer id
    const lastOrderCall = mockSavePanelOrder.mock.calls[mockSavePanelOrder.mock.calls.length - 1][0];
    expect(lastOrderCall).toHaveLength(1);
  });

  it('shows "No panels configured" with quick-add buttons when panels array is empty', () => {
    render(<Dashboard cameras={[]} />);

    expect(screen.getByText('No panels configured')).toBeInTheDocument();

    // Quick-add buttons should exist
    expect(screen.getByText('Add Camera')).toBeInTheDocument();
    expect(screen.getByText('Add Raw Data')).toBeInTheDocument();
    expect(screen.getByText('Add Weather')).toBeInTheDocument();
    expect(screen.getByText('Add Stats')).toBeInTheDocument();
    expect(screen.getByText('Add Nodes')).toBeInTheDocument();
    expect(screen.getByText('Add Telemetry')).toBeInTheDocument();
    expect(screen.getByText('Add Network')).toBeInTheDocument();
  });

  it('saves panels to localStorage when panels change', async () => {
    const cameras = [{ name: 'cam1', topic: 't1' }];

    render(<Dashboard cameras={cameras} />);

    // Initial save on mount due to useEffect
    await waitFor(() => {
      expect(mockSavePanels).toHaveBeenCalled();
    });

    const savedPanels = mockSavePanels.mock.calls[0][0];
    expect(savedPanels).toEqual(
      expect.arrayContaining([
        expect.objectContaining({ name: 'cam1', type: 'camera' }),
      ])
    );
  });

  it('saves panel order to localStorage when order changes', async () => {
    const cameras = [{ name: 'cam1', topic: 't1' }];

    render(<Dashboard cameras={cameras} />);

    // Initial save on mount due to useEffect
    await waitFor(() => {
      expect(mockSavePanelOrder).toHaveBeenCalled();
    });

    const savedOrder = mockSavePanelOrder.mock.calls[0][0];
    expect(savedOrder).toEqual(expect.arrayContaining([expect.any(String)]));
  });

  it('quick-add camera button adds a camera panel', async () => {
    render(<Dashboard cameras={[]} />);

    fireEvent.click(screen.getByText('Add Camera'));

    await waitFor(() => {
      expect(screen.getByText('Panels (1)')).toBeInTheDocument();
    });
  });
});
