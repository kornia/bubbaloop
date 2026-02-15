/**
 * End-to-end integration tests for the schema-ready gating pattern.
 *
 * Verifies that ALL protobuf-dependent view components gate their subscription
 * callbacks on schema readiness, while views with fallback decode chains (JsonView)
 * do NOT gate.
 *
 * This prevents the race condition where Zenoh subscriptions deliver messages
 * before fetchSchemas() completes, causing silent data loss.
 */
import { vi, describe, it, expect, beforeEach } from 'vitest';
import React from 'react';
import { render } from '@testing-library/react';

// --- Shared mock state ---

const _schemaReadyState = vi.hoisted(() => ({ ready: false }));

const _h264State = vi.hoisted(() => ({
  isSupported: true,
  initResolves: true,
}));

// --- Mock all shared dependencies ---

vi.mock('../../hooks/useZenohSubscription', () => ({
  useZenohSubscription: vi.fn(() => ({ messageCount: 0, fps: 0, instantFps: 0 })),
  useAllTopicStats: vi.fn(() => new Map()),
}));

vi.mock('../../hooks/useSchemaReady', () => ({
  useSchemaReady: vi.fn(() => _schemaReadyState.ready),
}));

vi.mock('../../lib/h264-decoder', () => {
  class MockH264Decoder {
    static isSupported() { return _h264State.isSupported; }
    init() { return _h264State.initResolves ? Promise.resolve() : new Promise<void>(() => {}); }
    close() {}
    decode() { return Promise.resolve(); }
  }
  return { H264Decoder: MockH264Decoder };
});

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

vi.mock('../../lib/subscription-manager', () => ({
  normalizeTopicPattern: vi.fn((key: string) => key),
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

vi.mock('react18-json-view', () => ({
  default: () => React.createElement('div', { 'data-testid': 'json-view' }),
}));
vi.mock('react18-json-view/src/style.css', () => ({}));

// --- Import components and mock ---

import { useZenohSubscription } from '../../hooks/useZenohSubscription';
import { CameraView } from '../CameraView';
import { WeatherViewPanel } from '../WeatherView';
import { SystemTelemetryViewPanel } from '../SystemTelemetryView';
import { NetworkMonitorViewPanel } from '../NetworkMonitorView';
import { RawDataViewPanel } from '../JsonView';

// Helper: get all subscription callbacks from mock calls
function getSubscriptionCallbacks(): Array<unknown> {
  const mockSub = vi.mocked(useZenohSubscription);
  return mockSub.mock.calls.map(call => call[1]);
}

describe('Schema-Ready Gating (end-to-end)', () => {
  beforeEach(() => {
    vi.clearAllMocks();
    _schemaReadyState.ready = false;
    _h264State.isSupported = true;
    _h264State.initResolves = true;
  });

  describe('when schemas are NOT ready (schemaVersion === 0)', () => {
    beforeEach(() => {
      _schemaReadyState.ready = false;
    });

    it('CameraView does not pass callback to subscription', () => {
      render(<CameraView cameraName="test" topic="camera/test/**" />);
      const callbacks = getSubscriptionCallbacks();
      expect(callbacks).toHaveLength(1);
      expect(callbacks[0]).toBeUndefined();
    });

    it('WeatherView does not pass any callbacks to subscriptions', () => {
      render(<WeatherViewPanel />);
      const callbacks = getSubscriptionCallbacks();
      // WeatherView has 3 subscriptions: current, hourly, daily
      expect(callbacks).toHaveLength(3);
      expect(callbacks.every(cb => cb === undefined)).toBe(true);
    });

    it('SystemTelemetryView does not pass callback to subscription', () => {
      render(<SystemTelemetryViewPanel />);
      const callbacks = getSubscriptionCallbacks();
      expect(callbacks).toHaveLength(1);
      expect(callbacks[0]).toBeUndefined();
    });

    it('NetworkMonitorView does not pass callback to subscription', () => {
      render(<NetworkMonitorViewPanel />);
      const callbacks = getSubscriptionCallbacks();
      expect(callbacks).toHaveLength(1);
      expect(callbacks[0]).toBeUndefined();
    });

    it('JsonView ALWAYS passes callback (has own fallback decode chain)', () => {
      render(<RawDataViewPanel topic="some/topic/**" />);
      const callbacks = getSubscriptionCallbacks();
      expect(callbacks).toHaveLength(1);
      expect(typeof callbacks[0]).toBe('function');
    });
  });

  describe('when schemas ARE ready (schemaVersion > 0)', () => {
    beforeEach(() => {
      _schemaReadyState.ready = true;
    });

    it('CameraView passes callback to subscription', () => {
      render(<CameraView cameraName="test" topic="camera/test/**" />);
      const callbacks = getSubscriptionCallbacks();
      expect(callbacks).toHaveLength(1);
      expect(typeof callbacks[0]).toBe('function');
    });

    it('WeatherView passes all 3 callbacks to subscriptions', () => {
      render(<WeatherViewPanel />);
      const callbacks = getSubscriptionCallbacks();
      expect(callbacks).toHaveLength(3);
      expect(callbacks.every(cb => typeof cb === 'function')).toBe(true);
    });

    it('SystemTelemetryView passes callback to subscription', () => {
      render(<SystemTelemetryViewPanel />);
      const callbacks = getSubscriptionCallbacks();
      expect(callbacks).toHaveLength(1);
      expect(typeof callbacks[0]).toBe('function');
    });

    it('NetworkMonitorView passes callback to subscription', () => {
      render(<NetworkMonitorViewPanel />);
      const callbacks = getSubscriptionCallbacks();
      expect(callbacks).toHaveLength(1);
      expect(typeof callbacks[0]).toBe('function');
    });

    it('JsonView still passes callback (unchanged behavior)', () => {
      render(<RawDataViewPanel topic="some/topic/**" />);
      const callbacks = getSubscriptionCallbacks();
      expect(callbacks).toHaveLength(1);
      expect(typeof callbacks[0]).toBe('function');
    });
  });

  describe('subscription topics are always registered (even when gated)', () => {
    it('all views register their topics regardless of schema readiness', () => {
      _schemaReadyState.ready = false;
      const mockSub = vi.mocked(useZenohSubscription);

      // Render all protobuf-dependent views
      const { unmount: u1 } = render(<CameraView cameraName="test" topic="camera/test/**" />);
      const { unmount: u2 } = render(<WeatherViewPanel />);
      const { unmount: u3 } = render(<SystemTelemetryViewPanel />);
      const { unmount: u4 } = render(<NetworkMonitorViewPanel />);

      // All should have called useZenohSubscription with a topic (first arg)
      // even though callbacks are undefined
      const topics = mockSub.mock.calls.map(call => call[0]);
      expect(topics).toHaveLength(6); // 1 + 3 + 1 + 1
      for (const topic of topics) {
        expect(typeof topic).toBe('string');
        expect(topic.length).toBeGreaterThan(0);
      }

      u1(); u2(); u3(); u4();
    });
  });
});
