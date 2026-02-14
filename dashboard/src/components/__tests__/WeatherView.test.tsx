import { vi, describe, it, expect, beforeEach } from 'vitest';
import React from 'react';
import { render, screen, fireEvent } from '@testing-library/react';

// Mock hooks BEFORE importing the component
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

// Mock react18-json-view and its CSS import
vi.mock('react18-json-view', () => ({
  default: () => React.createElement('div', { 'data-testid': 'json-view' }),
}));
vi.mock('react18-json-view/src/style.css', () => ({}));

import { WeatherViewPanel } from '../WeatherView';

describe('WeatherViewPanel', () => {
  beforeEach(() => {
    vi.clearAllMocks();
  });

  it('renders WEATHER badge in header', () => {
    render(<WeatherViewPanel />);
    expect(screen.getByText('WEATHER')).toBeInTheDocument();
  });

  it('shows "Waiting for weather data..." initially', () => {
    render(<WeatherViewPanel />);
    expect(screen.getByText('Waiting for weather data...')).toBeInTheDocument();
  });

  it('remove button calls onRemove', () => {
    const onRemove = vi.fn();
    render(<WeatherViewPanel onRemove={onRemove} />);
    const removeBtn = screen.getByTitle('Remove panel');
    fireEvent.click(removeBtn);
    expect(onRemove).toHaveBeenCalledTimes(1);
  });

  it('shows edit location section when edit button is clicked', () => {
    render(<WeatherViewPanel />);
    const editBtn = screen.getByTitle('Edit location');
    fireEvent.click(editBtn);
    expect(screen.getByLabelText('Latitude:')).toBeInTheDocument();
    expect(screen.getByLabelText('Longitude:')).toBeInTheDocument();
    // The button text "Update Location" and the section label "Update Location" both exist
    expect(screen.getByRole('button', { name: 'Update Location' })).toBeInTheDocument();
  });

  it('shows footer with topic names', () => {
    render(<WeatherViewPanel />);
    expect(screen.getByText('weather/current, weather/hourly, weather/daily')).toBeInTheDocument();
  });
});
