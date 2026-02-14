import { vi, describe, it, expect, beforeEach, afterEach } from 'vitest';
import React from 'react';

// ---- Mock modules BEFORE importing the component ----

const mockStartMonitoring = vi.fn(async () => {});
const mockIsMonitoringEnabled = vi.fn(() => false);
const mockGetAllMonitoredStats = vi.fn(() => new Map());

vi.mock('../../contexts/ZenohSubscriptionContext', () => ({
  useZenohSubscriptionContext: vi.fn(() => ({
    manager: {},
    getSession: vi.fn(() => null),
    subscribe: vi.fn(() => 'listener_1'),
    unsubscribe: vi.fn(),
    getTopicStats: vi.fn(() => null),
    getAllStats: vi.fn(() => new Map()),
    getAllMonitoredStats: mockGetAllMonitoredStats,
    getActiveSubscriptions: vi.fn(() => []),
    getDiscoveredTopics: vi.fn(() => []),
    addRemoteEndpoint: vi.fn(),
    removeEndpoint: vi.fn(),
    startMonitoring: mockStartMonitoring,
    stopMonitoring: vi.fn(async () => {}),
    isMonitoringEnabled: mockIsMonitoringEnabled,
  })),
}));

vi.mock('../../contexts/FleetContext', () => ({
  useFleetContext: vi.fn(() => ({
    machines: [],
    reportMachines: vi.fn(),
    nodes: [],
    reportNodes: vi.fn(),
    selectedMachineId: null,
    setSelectedMachineId: vi.fn(),
  })),
  FleetProvider: ({ children }: { children: React.ReactNode }) => children,
}));

vi.mock('../../lib/zenoh', () => ({
  getSamplePayload: vi.fn(() => new Uint8Array()),
  extractMachineId: vi.fn((topic: string) => {
    // Simple extraction: match bubbaloop/{machineId}/...
    const match = topic.match(/^bubbaloop\/([^/]+)\//);
    return match ? match[1] : null;
  }),
}));

vi.mock('../MachineBadge', () => ({
  MachineBadge: () => null,
}));

// ---- Now import the component and testing utilities ----

import { render, screen, fireEvent, act } from '@testing-library/react';
import { StatsViewPanel } from '../StatsView';

describe('StatsViewPanel', () => {
  beforeEach(() => {
    vi.clearAllMocks();
    vi.useFakeTimers({ shouldAdvanceTime: true });
    mockIsMonitoringEnabled.mockReturnValue(false);
    mockStartMonitoring.mockResolvedValue(undefined);
    mockGetAllMonitoredStats.mockReturnValue(new Map());
  });

  afterEach(() => {
    vi.useRealTimers();
  });

  it('renders STATS badge', () => {
    render(<StatsViewPanel />);

    expect(screen.getByText('STATS')).toBeInTheDocument();
  });

  it('shows "Starting topic monitor..." initially when monitoring not enabled', () => {
    mockIsMonitoringEnabled.mockReturnValue(false);

    render(<StatsViewPanel />);

    expect(screen.getByText('Starting topic monitor...')).toBeInTheDocument();
  });

  it('shows "Waiting for messages..." after monitoring has started', async () => {
    mockIsMonitoringEnabled.mockReturnValue(false);
    let resolveMonitoring: () => void;
    mockStartMonitoring.mockImplementation(() => new Promise<void>((resolve) => {
      resolveMonitoring = resolve;
    }));

    render(<StatsViewPanel />);

    // Initially shows "Starting topic monitor..."
    expect(screen.getByText('Starting topic monitor...')).toBeInTheDocument();

    // Resolve the startMonitoring promise
    await act(async () => {
      resolveMonitoring!();
    });

    expect(screen.getByText('Waiting for messages...')).toBeInTheDocument();
  });

  it('calls startMonitoring on mount when not already enabled', () => {
    mockIsMonitoringEnabled.mockReturnValue(false);

    render(<StatsViewPanel />);

    expect(mockStartMonitoring).toHaveBeenCalledTimes(1);
  });

  it('does not call startMonitoring when already enabled', () => {
    mockIsMonitoringEnabled.mockReturnValue(true);

    render(<StatsViewPanel />);

    expect(mockStartMonitoring).not.toHaveBeenCalled();
  });

  it('remove button calls onRemove', () => {
    const onRemove = vi.fn();

    render(<StatsViewPanel onRemove={onRemove} />);

    const removeBtn = screen.getByTitle('Remove');
    fireEvent.click(removeBtn);
    expect(onRemove).toHaveBeenCalledTimes(1);
  });

  it('renders table headers (Machine, Topic, Hz, Msgs)', async () => {
    mockIsMonitoringEnabled.mockReturnValue(true);

    // Provide some stats so the table renders
    const statsMap = new Map();
    statsMap.set('bubbaloop/orin00/camera/test', {
      messageCount: 10,
      fps: 5.0,
      hasActiveListeners: false,
      listenerCount: 0,
    });
    mockGetAllMonitoredStats.mockReturnValue(statsMap);

    render(<StatsViewPanel />);

    // Advance timers to trigger the 1s stats polling interval
    await act(async () => {
      vi.advanceTimersByTime(1100);
    });

    expect(screen.getByText('Machine')).toBeInTheDocument();
    expect(screen.getByText('Topic')).toBeInTheDocument();
    expect(screen.getByText('Hz')).toBeInTheDocument();
    expect(screen.getByText('Msgs')).toBeInTheDocument();
  });

  it('shows stats rows when data available', async () => {
    mockIsMonitoringEnabled.mockReturnValue(true);

    const statsMap = new Map();
    statsMap.set('bubbaloop/orin00/camera/entrance', {
      messageCount: 42,
      fps: 30.0,
      hasActiveListeners: true,
      listenerCount: 1,
    });
    statsMap.set('bubbaloop/orin00/weather/current', {
      messageCount: 5,
      fps: 0.5,
      hasActiveListeners: false,
      listenerCount: 0,
    });
    mockGetAllMonitoredStats.mockReturnValue(statsMap);

    render(<StatsViewPanel />);

    // Advance timers to trigger the 1s stats polling interval
    await act(async () => {
      vi.advanceTimersByTime(1100);
    });

    // Check message counts are displayed
    expect(screen.getByText('42')).toBeInTheDocument();
    expect(screen.getByText('5')).toBeInTheDocument();

    // Check Hz values are displayed (formatted to 2 decimal places)
    expect(screen.getByText('30.00')).toBeInTheDocument();
    expect(screen.getByText('0.50')).toBeInTheDocument();
  });

  it('shows active listener indicator for topics with listeners', async () => {
    mockIsMonitoringEnabled.mockReturnValue(true);

    const statsMap = new Map();
    statsMap.set('0/camera%entrance%compressed', {
      messageCount: 100,
      fps: 30.0,
      hasActiveListeners: true,
      listenerCount: 1,
    });
    mockGetAllMonitoredStats.mockReturnValue(statsMap);

    render(<StatsViewPanel />);

    // Advance timers to trigger polling
    await act(async () => {
      vi.advanceTimersByTime(1100);
    });

    // The has-listeners class should be on the row
    const row = document.querySelector('.stats-row.has-listeners');
    expect(row).toBeInTheDocument();

    // The listener indicator dot should be present
    const indicator = row?.querySelector('.listener-indicator');
    expect(indicator).toBeInTheDocument();
  });

  it('shows footer with topic count', async () => {
    mockIsMonitoringEnabled.mockReturnValue(true);

    const statsMap = new Map();
    statsMap.set('topic1', { messageCount: 1, fps: 1.0, hasActiveListeners: false, listenerCount: 0 });
    statsMap.set('topic2', { messageCount: 2, fps: 2.0, hasActiveListeners: false, listenerCount: 0 });
    mockGetAllMonitoredStats.mockReturnValue(statsMap);

    render(<StatsViewPanel />);

    await act(async () => {
      vi.advanceTimersByTime(1100);
    });

    expect(screen.getByText(/2 topics/)).toBeInTheDocument();
  });

  it('renders panel header with drag handle support', () => {
    render(<StatsViewPanel dragHandleProps={{ 'data-testid': 'drag' }} />);

    const header = document.querySelector('.panel-header');
    expect(header).toBeInTheDocument();
    expect(header?.getAttribute('data-testid')).toBe('drag');
  });

  it('sets monitoringStarted immediately when already enabled', () => {
    mockIsMonitoringEnabled.mockReturnValue(true);

    render(<StatsViewPanel />);

    // Should show "Waiting for messages..." immediately (not "Starting topic monitor...")
    // since monitoring is already enabled
    expect(screen.getByText('Waiting for messages...')).toBeInTheDocument();
    expect(screen.queryByText('Starting topic monitor...')).not.toBeInTheDocument();
  });
});
