import { vi, describe, it, expect, beforeEach } from 'vitest';
import React from 'react';
import { render, screen, fireEvent } from '@testing-library/react';

vi.mock('../../hooks/useZenohSubscription', () => ({
  useZenohSubscription: vi.fn(() => ({ messageCount: 0, fps: 0, instantFps: 0 })),
  useAllTopicStats: vi.fn(() => new Map()),
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

import { SystemTelemetryViewPanel } from '../SystemTelemetryView';

describe('SystemTelemetryViewPanel', () => {
  beforeEach(() => {
    vi.clearAllMocks();
  });

  it('renders TELEMETRY badge in header', () => {
    render(<SystemTelemetryViewPanel />);
    expect(screen.getByText('TELEMETRY')).toBeInTheDocument();
  });

  it('shows "Waiting for system telemetry..." when no data', () => {
    render(<SystemTelemetryViewPanel />);
    expect(screen.getByText('Waiting for system telemetry...')).toBeInTheDocument();
  });

  it('remove button calls onRemove', () => {
    const onRemove = vi.fn();
    render(<SystemTelemetryViewPanel onRemove={onRemove} />);
    const removeBtn = screen.getByTitle('Remove panel');
    fireEvent.click(removeBtn);
    expect(onRemove).toHaveBeenCalledTimes(1);
  });

  it('shows footer with topic info', () => {
    render(<SystemTelemetryViewPanel />);
    expect(screen.getByText('system-telemetry/metrics')).toBeInTheDocument();
  });

  it('does not render remove button when onRemove is not provided', () => {
    render(<SystemTelemetryViewPanel />);
    expect(screen.queryByTitle('Remove panel')).not.toBeInTheDocument();
  });
});
