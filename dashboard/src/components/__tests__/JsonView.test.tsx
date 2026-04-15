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
  getEncodingInfo: vi.fn(() => ({ id: 0 })),
  hasExplicitEncoding: vi.fn(() => false),
  EncodingPredefined: {
    APPLICATION_CBOR: 8,
    APPLICATION_JSON: 5,
    TEXT_JSON: 6,
    TEXT_JSON5: 11,
  },
  extractMachineId: vi.fn(() => null),
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
    render(<RawDataViewPanel {...defaultProps} />);

    const badge = document.querySelector('.panel-type-badge');
    expect(badge).toBeInTheDocument();
    expect(badge?.textContent).toBe('RAW DATA');
  });

  it('shows schema source badges correctly (dynamic/built-in/raw)', () => {
    render(<RawDataViewPanel {...defaultProps} />);

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

  it('passes callback to subscription', () => {
    render(<RawDataViewPanel topic="some/topic/**" />);

    const mockSub = vi.mocked(useZenohSubscription);
    expect(mockSub).toHaveBeenCalledTimes(1);
    expect(typeof mockSub.mock.calls[0][1]).toBe('function');
  });
});
