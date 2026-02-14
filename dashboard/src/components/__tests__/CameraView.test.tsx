import { vi, describe, it, expect, beforeEach } from 'vitest';
import React from 'react';

// ---- Mock modules BEFORE importing the component ----

// vi.hoisted() runs before vi.mock hoisting, making these available in mock factories
const _h264State = vi.hoisted(() => ({
  isSupported: true,
  initResolves: true,
}));

vi.mock('../../lib/h264-decoder', () => {
  class MockH264Decoder {
    static isSupported() {
      return _h264State.isSupported;
    }

    init() {
      if (_h264State.initResolves) {
        return Promise.resolve();
      }
      return new Promise<void>(() => {});
    }

    close() {}

    decode() {
      return Promise.resolve();
    }
  }

  return { H264Decoder: MockH264Decoder };
});

vi.mock('../../hooks/useZenohSubscription', () => ({
  useZenohSubscription: vi.fn(() => ({ messageCount: 0, fps: 0, instantFps: 0 })),
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
    registry: { lookupType: vi.fn(() => null), tryDecodeForTopic: vi.fn(() => null) },
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

vi.mock('../../lib/subscription-manager', () => ({
  normalizeTopicPattern: vi.fn((key: string) => key),
}));

vi.mock('../../lib/zenoh', () => ({
  getSamplePayload: vi.fn(() => new Uint8Array()),
  extractMachineId: vi.fn(() => null),
}));

vi.mock('../MachineBadge', () => ({
  MachineBadge: () => null,
}));

// ---- Now import the component and testing utilities ----

import { render, screen, fireEvent } from '@testing-library/react';
import { CameraView } from '../CameraView';
import { useZenohSubscription } from '../../hooks/useZenohSubscription';

describe('CameraView', () => {
  const defaultProps = {
    cameraName: 'entrance',
    topic: '0/camera%entrance%compressed/**',
  };

  beforeEach(() => {
    vi.clearAllMocks();
    _h264State.isSupported = true;
    _h264State.initResolves = true;
    _schemaReadyState.ready = false;
  });

  it('renders canvas element and CAMERA badge in header', () => {
    render(<CameraView {...defaultProps} />);

    expect(screen.getByText('CAMERA')).toBeInTheDocument();
    const canvas = document.querySelector('canvas.camera-canvas');
    expect(canvas).toBeInTheDocument();
  });

  it('shows "Initializing decoder..." while decoder is not yet ready', () => {
    // The decoder init is async, so initially isReady = false
    // We make init hang to keep the loading state visible
    _h264State.initResolves = false;

    render(<CameraView {...defaultProps} />);

    expect(screen.getByText('Initializing decoder...')).toBeInTheDocument();
  });

  it('shows WebCodecs error message when H264Decoder.isSupported() returns false', () => {
    _h264State.isSupported = false;

    render(<CameraView {...defaultProps} />);

    expect(
      screen.getByText(/WebCodecs not supported/)
    ).toBeInTheDocument();
  });

  it('shows INIT status when not ready, LIVE when ready', async () => {
    // Init hangs so it stays in INIT
    _h264State.initResolves = false;

    render(<CameraView {...defaultProps} />);

    expect(screen.getByText('INIT')).toBeInTheDocument();
    expect(screen.queryByText('LIVE')).not.toBeInTheDocument();
  });

  it('shows LIVE status when decoder init resolves', async () => {
    // Init resolves immediately
    _h264State.initResolves = true;

    render(<CameraView {...defaultProps} />);

    const liveBadge = await screen.findByText('LIVE');
    expect(liveBadge).toBeInTheDocument();
  });

  it('renders topic dropdown with camera topics filtered by camera and compressed', () => {
    const cameraTopics = [
      { display: 'bubbaloop/m1/camera/entrance/compressed', raw: '0/camera%entrance%compressed/type/hash' },
      { display: 'bubbaloop/m1/camera/parking/compressed', raw: '0/camera%parking%compressed/type/hash' },
      { display: 'bubbaloop/m1/weather/current', raw: '0/weather%current/type/hash' },
    ];

    render(
      <CameraView
        {...defaultProps}
        availableTopics={cameraTopics}
      />
    );

    const select = document.querySelector('select.topic-select') as HTMLSelectElement;
    expect(select).toBeInTheDocument();
    // Only camera topics with 'camera' and 'compressed' in display should appear
    const options = select.querySelectorAll('option');
    const optionTexts = Array.from(options).map(o => o.textContent);
    expect(optionTexts).toContain('bubbaloop/m1/camera/entrance/compressed');
    expect(optionTexts).toContain('bubbaloop/m1/camera/parking/compressed');
    expect(optionTexts).not.toContain('bubbaloop/m1/weather/current');
  });

  it('shows "No camera topics available" when no camera topics match', () => {
    const nonCameraTopics = [
      { display: 'bubbaloop/m1/weather/current', raw: '0/weather%current/type/hash' },
    ];

    render(
      <CameraView
        {...defaultProps}
        topic=""
        availableTopics={nonCameraTopics}
      />
    );

    expect(screen.getByText('No camera topics available')).toBeInTheDocument();
  });

  it('remove button calls onRemove', () => {
    const onRemove = vi.fn();

    render(<CameraView {...defaultProps} onRemove={onRemove} />);

    const removeBtn = screen.getByTitle('Remove camera');
    fireEvent.click(removeBtn);
    expect(onRemove).toHaveBeenCalledTimes(1);
  });

  it('maximize button calls onMaximize', () => {
    const onMaximize = vi.fn();

    render(<CameraView {...defaultProps} onMaximize={onMaximize} />);

    const maximizeBtn = screen.getByTitle('Maximize');
    fireEvent.click(maximizeBtn);
    expect(onMaximize).toHaveBeenCalledTimes(1);
  });

  it('info button toggles metadata panel', () => {
    render(<CameraView {...defaultProps} />);

    const infoBtn = screen.getByTitle('Show metadata');
    // Initially no info panel visible (no metadata yet, so panel won't show even if toggled)
    fireEvent.click(infoBtn);
    // The info button should have the 'active' class after click
    expect(infoBtn.classList.contains('active')).toBe(true);

    // Click again to toggle off
    fireEvent.click(infoBtn);
    expect(infoBtn.classList.contains('active')).toBe(false);
  });

  it('auto-detects camera topic from availableTopics matching cameraName', () => {
    const onTopicChange = vi.fn();
    const topics = [
      { display: 'bubbaloop/m1/camera/entrance/compressed', raw: '0/camera%entrance%compressed/type/hash' },
    ];

    render(
      <CameraView
        cameraName="entrance"
        topic=""
        onTopicChange={onTopicChange}
        availableTopics={topics}
      />
    );

    // Should auto-detect and call onTopicChange
    expect(onTopicChange).toHaveBeenCalledTimes(1);
    expect(onTopicChange).toHaveBeenCalledWith(
      expect.stringContaining('/**')
    );
  });

  it('does not render maximize button if onMaximize not provided', () => {
    render(<CameraView {...defaultProps} />);

    expect(screen.queryByTitle('Maximize')).not.toBeInTheDocument();
  });

  it('does not render remove button if onRemove not provided', () => {
    render(<CameraView {...defaultProps} />);

    expect(screen.queryByTitle('Remove camera')).not.toBeInTheDocument();
  });

  it('renders drag handle when dragHandleProps provided', () => {
    render(
      <CameraView {...defaultProps} dragHandleProps={{ 'data-testid': 'drag' }} />
    );

    expect(screen.getByTitle('Drag to reorder')).toBeInTheDocument();
  });

  it('does not pass handleSample when schemas are not ready', () => {
    _schemaReadyState.ready = false;

    render(<CameraView {...defaultProps} />);

    // useZenohSubscription should be called with undefined callback
    const mockSub = vi.mocked(useZenohSubscription);
    const lastCall = mockSub.mock.calls[mockSub.mock.calls.length - 1];
    expect(lastCall[0]).toBe(defaultProps.topic);
    expect(lastCall[1]).toBeUndefined();
  });

  it('passes handleSample when schemas are ready', () => {
    _schemaReadyState.ready = true;

    render(<CameraView {...defaultProps} />);

    // useZenohSubscription should be called with a function callback
    const mockSub = vi.mocked(useZenohSubscription);
    const lastCall = mockSub.mock.calls[mockSub.mock.calls.length - 1];
    expect(lastCall[0]).toBe(defaultProps.topic);
    expect(typeof lastCall[1]).toBe('function');
  });
});
