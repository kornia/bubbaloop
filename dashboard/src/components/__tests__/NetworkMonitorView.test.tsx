import { vi, describe, it, expect, beforeEach } from 'vitest';
import React from 'react';
import { render, screen, fireEvent } from '@testing-library/react';

vi.mock('../../hooks/useZenohSubscription', () => ({
  useZenohSubscription: vi.fn(() => ({ messageCount: 0, fps: 0, instantFps: 0 })),
  useAllTopicStats: vi.fn(() => new Map()),
}));

const _schemaReadyState = vi.hoisted(() => ({ ready: false }));

vi.mock('../../hooks/useSchemaReady', () => ({
  useSchemaReady: vi.fn(() => _schemaReadyState.ready),
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

vi.mock('../../contexts/SchemaRegistryContext', () => ({
  useSchemaRegistry: vi.fn(() => ({
    registry: { lookupType: vi.fn(() => null), tryDecodeForTopic: vi.fn(() => null), decode: vi.fn(() => null) },
    loading: false,
    error: null,
    refresh: vi.fn(),
    decode: vi.fn(() => null),
    discoverForTopic: vi.fn(),
    schemaVersion: 0,
  })),
}));

vi.mock('../../contexts/ZenohSubscriptionContext', () => ({
  useZenohSubscriptionContext: vi.fn(() => ({
    manager: {},
    getSession: vi.fn(() => null),
    subscribe: vi.fn(() => 'listener_1'),
    unsubscribe: vi.fn(),
    getTopicStats: vi.fn(() => null),
    getAllStats: vi.fn(() => new Map()),
    getAllMonitoredStats: vi.fn(() => new Map()),
    getActiveSubscriptions: vi.fn(() => []),
    getDiscoveredTopics: vi.fn(() => []),
    addRemoteEndpoint: vi.fn(),
    removeEndpoint: vi.fn(),
    startMonitoring: vi.fn(async () => {}),
    stopMonitoring: vi.fn(async () => {}),
    isMonitoringEnabled: vi.fn(() => false),
  })),
}));

vi.mock('./MachineBadge', () => ({
  MachineBadge: () => React.createElement('span', { 'data-testid': 'machine-badge' }),
}));

import { NetworkMonitorViewPanel } from '../NetworkMonitorView';
import { useZenohSubscription } from '../../hooks/useZenohSubscription';

describe('NetworkMonitorViewPanel', () => {
  beforeEach(() => {
    vi.clearAllMocks();
    _schemaReadyState.ready = false;
  });

  it('renders NETWORK badge in header', () => {
    render(<NetworkMonitorViewPanel />);
    expect(screen.getByText('NETWORK')).toBeInTheDocument();
  });

  it('shows "Waiting for network status..." when no data', () => {
    render(<NetworkMonitorViewPanel />);
    expect(screen.getByText('Waiting for network status...')).toBeInTheDocument();
  });

  it('remove button calls onRemove', () => {
    const onRemove = vi.fn();
    render(<NetworkMonitorViewPanel onRemove={onRemove} />);
    const removeBtn = screen.getByTitle('Remove panel');
    fireEvent.click(removeBtn);
    expect(onRemove).toHaveBeenCalledTimes(1);
  });

  it('shows footer with topic info', () => {
    render(<NetworkMonitorViewPanel />);
    expect(screen.getByText('network-monitor/status')).toBeInTheDocument();
  });

  it('does not render remove button when onRemove is not provided', () => {
    render(<NetworkMonitorViewPanel />);
    expect(screen.queryByTitle('Remove panel')).not.toBeInTheDocument();
  });

  describe('schema-ready gating', () => {
    it('does not pass callback when schemas are not ready', () => {
      _schemaReadyState.ready = false;
      render(<NetworkMonitorViewPanel />);

      const mockSub = vi.mocked(useZenohSubscription);
      expect(mockSub).toHaveBeenCalledTimes(1);
      expect(mockSub.mock.calls[0][1]).toBeUndefined();
    });

    it('passes callback when schemas are ready', () => {
      _schemaReadyState.ready = true;
      render(<NetworkMonitorViewPanel />);

      const mockSub = vi.mocked(useZenohSubscription);
      expect(mockSub).toHaveBeenCalledTimes(1);
      expect(typeof mockSub.mock.calls[0][1]).toBe('function');
    });
  });
});
