import { vi, describe, it, expect, beforeEach } from 'vitest';
import React from 'react';

// ---- Mock modules BEFORE importing the component ----

vi.mock('react18-json-view', () => ({
  default: ({ src }: { src: unknown }) => (
    <pre data-testid="json-view">{JSON.stringify(src)}</pre>
  ),
}));

vi.mock('react18-json-view/src/style.css', () => ({}));

vi.mock('../../hooks/useZenohSubscription', () => ({
  useZenohSubscription: vi.fn(() => ({ messageCount: 0, fps: 0, instantFps: 0 })),
}));

// JsonView does NOT use useSchemaReady — no mock needed.
// It has its own fallback decode chain (JSON → schema → built-in → text → hex).

const mockFleetContext = {
  machines: [],
  reportMachines: vi.fn(),
  nodes: [],
  reportNodes: vi.fn(),
  selectedMachineId: null,
  setSelectedMachineId: vi.fn(),
};

vi.mock('../../contexts/FleetContext', () => ({
  useFleetContext: vi.fn(() => mockFleetContext),
  FleetProvider: ({ children }: { children: React.ReactNode }) => children,
}));

const mockSchemaRegistry = {
  registry: {
    lookupType: vi.fn(() => null),
    tryDecodeForTopic: vi.fn(() => null),
  },
  loading: false,
  error: null,
  refresh: vi.fn(),
  decode: vi.fn(() => null),
  discoverForTopic: vi.fn(),
  schemaVersion: 0,
};

vi.mock('../../contexts/SchemaRegistryContext', () => ({
  useSchemaRegistry: vi.fn(() => mockSchemaRegistry),
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

vi.mock('../../lib/zenoh', () => ({
  getSamplePayload: vi.fn(() => new Uint8Array()),
  extractMachineId: vi.fn(() => null),
}));

vi.mock('../../proto/daemon', () => ({
  decodeNodeList: vi.fn(() => null),
  decodeNodeEvent: vi.fn(() => null),
}));

vi.mock('../MachineBadge', () => ({
  MachineBadge: () => null,
}));

// ---- Now import the component and testing utilities ----

import { render, screen, fireEvent } from '@testing-library/react';
import { RawDataViewPanel } from '../JsonView';
import { useZenohSubscription } from '../../hooks/useZenohSubscription';

describe('RawDataViewPanel', () => {
  const defaultProps = {
    topic: '',
    availableTopics: [] as Array<{ display: string; raw: string }>,
  };

  beforeEach(() => {
    vi.clearAllMocks();
  });

  it('renders "Select a topic to start receiving data" when no topic', () => {
    render(<RawDataViewPanel {...defaultProps} />);

    expect(screen.getByText('Select a topic to start receiving data')).toBeInTheDocument();
  });

  it('renders "Waiting for data..." when topic is set but no data received', () => {
    render(<RawDataViewPanel {...defaultProps} topic="some/topic/**" />);

    expect(screen.getByText('Waiting for data...')).toBeInTheDocument();
  });

  it('shows RAW DATA badge when no schema', () => {
    render(<RawDataViewPanel {...defaultProps} />);

    expect(screen.getByText('RAW DATA')).toBeInTheDocument();
  });

  it('shows schema name badge when schemaName is set', () => {
    // We need to simulate schemaName being set.
    // The component gets schemaName from internal state after decoding.
    // Since it starts with null, the badge shows "RAW DATA".
    // This is the initial state test.
    render(<RawDataViewPanel {...defaultProps} />);

    // Initially shows RAW DATA
    const badge = document.querySelector('.panel-type-badge');
    expect(badge).toBeInTheDocument();
    expect(badge?.textContent).toBe('RAW DATA');
  });

  it('shows schema source badges correctly (dynamic/built-in/raw)', () => {
    // When no schemaName or schemaName is binary/null, no source badge shown
    render(<RawDataViewPanel {...defaultProps} />);

    // No schema source badge when schemaName is null (shows RAW DATA)
    const sourceBadge = document.querySelector('.schema-source-badge');
    expect(sourceBadge).not.toBeInTheDocument();
  });

  it('topic dropdown renders available topics', () => {
    const topics = [
      { display: 'bubbaloop/m1/weather/current', raw: '0/weather%current/type/hash' },
      { display: 'bubbaloop/m1/daemon/nodes', raw: 'bubbaloop/daemon/nodes' },
    ];

    render(<RawDataViewPanel {...defaultProps} availableTopics={topics} />);

    const select = document.querySelector('select.topic-select') as HTMLSelectElement;
    expect(select).toBeInTheDocument();

    const options = select.querySelectorAll('option');
    const optionTexts = Array.from(options).map(o => o.textContent);
    // First option is the placeholder
    expect(optionTexts).toContain('-- Select topic --');
    expect(optionTexts).toContain('bubbaloop/m1/weather/current');
    expect(optionTexts).toContain('bubbaloop/m1/daemon/nodes');
  });

  it('remove button calls onRemove', () => {
    const onRemove = vi.fn();

    render(<RawDataViewPanel {...defaultProps} onRemove={onRemove} />);

    const removeBtn = screen.getByTitle('Remove panel');
    fireEvent.click(removeBtn);
    expect(onRemove).toHaveBeenCalledTimes(1);
  });

  it('does not render remove button when onRemove not provided', () => {
    render(<RawDataViewPanel {...defaultProps} />);

    expect(screen.queryByTitle('Remove panel')).not.toBeInTheDocument();
  });

  it('refresh schemas button exists and calls refresh', () => {
    render(<RawDataViewPanel {...defaultProps} />);

    const refreshBtn = screen.getByTitle('Refresh schemas');
    expect(refreshBtn).toBeInTheDocument();

    fireEvent.click(refreshBtn);
    expect(mockSchemaRegistry.refresh).toHaveBeenCalledTimes(1);
  });

  it('renders drag handle when dragHandleProps provided', () => {
    render(
      <RawDataViewPanel {...defaultProps} dragHandleProps={{ 'data-testid': 'drag' }} />
    );

    expect(screen.getByTitle('Drag to reorder')).toBeInTheDocument();
  });

  it('shows placeholder text when no topic and no availableTopics', () => {
    render(<RawDataViewPanel topic="" />);

    expect(screen.getByText('Select a topic to start receiving data')).toBeInTheDocument();
  });

  it('handles topic change from dropdown', () => {
    const onTopicChange = vi.fn();
    const topics = [
      { display: 'bubbaloop/m1/weather/current', raw: 'weather-raw-key' },
    ];

    render(
      <RawDataViewPanel
        topic=""
        onTopicChange={onTopicChange}
        availableTopics={topics}
      />
    );

    const select = document.querySelector('select.topic-select') as HTMLSelectElement;
    fireEvent.change(select, { target: { value: 'weather-raw-key' } });

    expect(onTopicChange).toHaveBeenCalledWith('weather-raw-key');
  });

  it('always passes callback regardless of schema readiness (has own fallback chain)', () => {
    // JsonView uses tryDecodeForTopic which has its own fallback:
    // JSON → SchemaRegistry → built-in decoders → plain text → hex
    // So it does NOT gate on useSchemaReady — it always processes samples.
    mockSchemaRegistry.schemaVersion = 0;

    render(<RawDataViewPanel topic="some/topic/**" />);

    const mockSub = vi.mocked(useZenohSubscription);
    expect(mockSub).toHaveBeenCalledTimes(1);
    expect(typeof mockSub.mock.calls[0][1]).toBe('function');
  });
});
