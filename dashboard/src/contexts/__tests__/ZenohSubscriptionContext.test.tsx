import { describe, it, expect, vi, beforeEach } from 'vitest';
import { renderHook } from '@testing-library/react';
import { ReactNode } from 'react';
import {
  ZenohSubscriptionProvider,
  useZenohSubscriptionContext,
} from '../ZenohSubscriptionContext';

// Mock subscription manager
const mockSubscribe = vi.fn().mockReturnValue('listener_1');
const mockUnsubscribe = vi.fn();
const mockSetSession = vi.fn();
const mockDestroy = vi.fn();
const mockGetTopicStats = vi.fn().mockReturnValue(null);
const mockGetAllStats = vi.fn().mockReturnValue(new Map());
const mockGetAllMonitoredStats = vi.fn().mockReturnValue(new Map());
const mockGetActiveSubscriptions = vi.fn().mockReturnValue([]);
const mockGetDiscoveredTopics = vi.fn().mockReturnValue([]);
const mockAddRemoteEndpoint = vi.fn();
const mockRemoveEndpoint = vi.fn();
const mockStartMonitoring = vi.fn().mockResolvedValue(undefined);
const mockStopMonitoring = vi.fn().mockResolvedValue(undefined);
const mockIsMonitoringEnabled = vi.fn().mockReturnValue(false);

vi.mock('../../lib/subscription-manager', () => {
  return {
    ZenohSubscriptionManager: class MockZenohSubscriptionManager {
      subscribe = mockSubscribe;
      unsubscribe = mockUnsubscribe;
      setSession = mockSetSession;
      destroy = mockDestroy;
      getTopicStats = mockGetTopicStats;
      getAllStats = mockGetAllStats;
      getAllMonitoredStats = mockGetAllMonitoredStats;
      getActiveSubscriptions = mockGetActiveSubscriptions;
      getDiscoveredTopics = mockGetDiscoveredTopics;
      addRemoteEndpoint = mockAddRemoteEndpoint;
      removeEndpoint = mockRemoveEndpoint;
      startMonitoring = mockStartMonitoring;
      stopMonitoring = mockStopMonitoring;
      isMonitoringEnabled = mockIsMonitoringEnabled;
    },
  };
});

function createMockSession(): any {
  return { isClosed: false };
}

function makeWrapper(session: any) {
  return function Wrapper({ children }: { children: ReactNode }) {
    return (
      <ZenohSubscriptionProvider session={session}>
        {children}
      </ZenohSubscriptionProvider>
    );
  };
}

describe('ZenohSubscriptionContext', () => {
  beforeEach(() => {
    vi.clearAllMocks();
  });

  it('renders children', () => {
    const wrapper = makeWrapper(null);
    const { result } = renderHook(() => useZenohSubscriptionContext(), { wrapper });
    expect(result.current).toBeDefined();
  });

  it('useZenohSubscriptionContext throws outside provider', () => {
    const spy = vi.spyOn(console, 'error').mockImplementation(() => {});
    expect(() => {
      renderHook(() => useZenohSubscriptionContext());
    }).toThrow('useZenohSubscriptionContext must be used within a ZenohSubscriptionProvider');
    spy.mockRestore();
  });

  it('manager is created once via ref and not recreated on re-render', () => {
    const session = createMockSession();

    const { result, rerender } = renderHook(() => useZenohSubscriptionContext(), {
      wrapper: makeWrapper(session),
    });

    const managerFirst = result.current.manager;

    rerender();

    const managerSecond = result.current.manager;
    expect(managerFirst).toBe(managerSecond);
  });

  it('subscribe forwards to manager.subscribe', () => {
    const session = createMockSession();
    const wrapper = makeWrapper(session);

    const { result } = renderHook(() => useZenohSubscriptionContext(), { wrapper });

    const callback = vi.fn();
    const listenerId = result.current.subscribe('test/topic', callback);

    expect(mockSubscribe).toHaveBeenCalledWith('test/topic', callback, undefined, undefined);
    expect(listenerId).toBe('listener_1');
  });

  it('unsubscribe forwards to manager.unsubscribe', () => {
    const session = createMockSession();
    const wrapper = makeWrapper(session);

    const { result } = renderHook(() => useZenohSubscriptionContext(), { wrapper });

    result.current.unsubscribe('test/topic', 'listener_1');

    expect(mockUnsubscribe).toHaveBeenCalledWith('test/topic', 'listener_1', undefined, undefined);
  });

  it('getSession returns current session', () => {
    const session = createMockSession();
    const wrapper = makeWrapper(session);

    const { result } = renderHook(() => useZenohSubscriptionContext(), { wrapper });

    expect(result.current.getSession()).toBe(session);
  });

  it('getSession returns null when no session', () => {
    const wrapper = makeWrapper(null);
    const { result } = renderHook(() => useZenohSubscriptionContext(), { wrapper });

    expect(result.current.getSession()).toBeNull();
  });

  it('setSession is called when session prop changes', () => {
    const session1 = createMockSession();
    const wrapper = makeWrapper(session1);

    renderHook(() => useZenohSubscriptionContext(), { wrapper });

    expect(mockSetSession).toHaveBeenCalledWith(session1);
  });

  it('setSession is called with null session', () => {
    makeWrapper(null);
    renderHook(() => useZenohSubscriptionContext(), { wrapper: makeWrapper(null) });

    expect(mockSetSession).toHaveBeenCalledWith(null);
  });

  it('cleanup on unmount calls manager.destroy', () => {
    const wrapper = makeWrapper(null);
    const { unmount } = renderHook(() => useZenohSubscriptionContext(), { wrapper });

    expect(mockDestroy).not.toHaveBeenCalled();

    unmount();

    expect(mockDestroy).toHaveBeenCalledTimes(1);
  });

  it('getTopicStats forwards to manager', () => {
    const stats = { messageCount: 10, fps: 5, instantFps: 6, lastSeen: Date.now() };
    mockGetTopicStats.mockReturnValue(stats);

    const wrapper = makeWrapper(null);
    const { result } = renderHook(() => useZenohSubscriptionContext(), { wrapper });

    const returned = result.current.getTopicStats('test/topic');
    expect(mockGetTopicStats).toHaveBeenCalledWith('test/topic', undefined);
    expect(returned).toEqual(stats);
  });

  it('getAllStats forwards to manager', () => {
    const statsMap = new Map([
      ['topic1', { messageCount: 5, fps: 2, instantFps: 3, lastSeen: 0 }],
    ]);
    mockGetAllStats.mockReturnValue(statsMap);

    const wrapper = makeWrapper(null);
    const { result } = renderHook(() => useZenohSubscriptionContext(), { wrapper });

    const returned = result.current.getAllStats();
    expect(mockGetAllStats).toHaveBeenCalled();
    expect(returned).toBe(statsMap);
  });
});
