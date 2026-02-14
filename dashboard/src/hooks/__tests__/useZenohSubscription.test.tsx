import { describe, it, expect, vi, beforeEach, afterEach } from 'vitest';
import { renderHook, act } from '@testing-library/react';

// Mock the context module
vi.mock('../../contexts/ZenohSubscriptionContext', () => ({
  useZenohSubscriptionContext: vi.fn(),
}));

import { useZenohSubscriptionContext } from '../../contexts/ZenohSubscriptionContext';
import { useZenohSubscription, useAllTopicStats } from '../useZenohSubscription';

function createMockContext() {
  return {
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
    startMonitoring: vi.fn().mockResolvedValue(undefined),
    stopMonitoring: vi.fn().mockResolvedValue(undefined),
    isMonitoringEnabled: vi.fn(() => false),
  };
}

describe('useZenohSubscription', () => {
  let mockCtx: ReturnType<typeof createMockContext>;

  beforeEach(() => {
    vi.useFakeTimers();
    mockCtx = createMockContext();
    vi.mocked(useZenohSubscriptionContext).mockReturnValue(mockCtx as any);
  });

  afterEach(() => {
    vi.useRealTimers();
    vi.clearAllMocks();
  });

  it('returns initial zeroes for stats', () => {
    const { result } = renderHook(() => useZenohSubscription('test/topic'));

    expect(result.current.messageCount).toBe(0);
    expect(result.current.fps).toBe(0);
    expect(result.current.instantFps).toBe(0);
  });

  it('subscribes to topic on mount', () => {
    renderHook(() => useZenohSubscription('camera/raw'));

    expect(mockCtx.subscribe).toHaveBeenCalledTimes(1);
    expect(mockCtx.subscribe).toHaveBeenCalledWith(
      'camera/raw',
      expect.any(Function),
      undefined
    );
  });

  it('unsubscribes on unmount', () => {
    const { unmount } = renderHook(() => useZenohSubscription('camera/raw'));

    expect(mockCtx.unsubscribe).not.toHaveBeenCalled();

    unmount();

    expect(mockCtx.unsubscribe).toHaveBeenCalledTimes(1);
    expect(mockCtx.unsubscribe).toHaveBeenCalledWith('camera/raw', 'listener_1', undefined);
  });

  it('re-subscribes when topic changes', () => {
    const { rerender } = renderHook(
      ({ topic }: { topic: string }) => useZenohSubscription(topic),
      { initialProps: { topic: 'topic-a' } }
    );

    expect(mockCtx.subscribe).toHaveBeenCalledTimes(1);
    expect(mockCtx.subscribe).toHaveBeenCalledWith('topic-a', expect.any(Function), undefined);

    // Change topic
    mockCtx.subscribe.mockReturnValue('listener_2');
    rerender({ topic: 'topic-b' });

    // Should have unsubscribed from old and subscribed to new
    expect(mockCtx.unsubscribe).toHaveBeenCalledWith('topic-a', 'listener_1', undefined);
    expect(mockCtx.subscribe).toHaveBeenCalledTimes(2);
    expect(mockCtx.subscribe).toHaveBeenLastCalledWith('topic-b', expect.any(Function), undefined);
  });

  it('does not subscribe when topic is empty', () => {
    renderHook(() => useZenohSubscription(''));

    expect(mockCtx.subscribe).not.toHaveBeenCalled();
  });

  it('polls getTopicStats every 2s', () => {
    renderHook(() => useZenohSubscription('test/topic'));

    // Initial call from subscribe, but no poll yet
    expect(mockCtx.getTopicStats).not.toHaveBeenCalled();

    // Advance 2 seconds
    act(() => {
      vi.advanceTimersByTime(2000);
    });

    expect(mockCtx.getTopicStats).toHaveBeenCalledWith('test/topic', undefined);

    // Advance another 2 seconds
    act(() => {
      vi.advanceTimersByTime(2000);
    });

    expect(mockCtx.getTopicStats).toHaveBeenCalledTimes(2);
  });

  it('updates stats from polled data', () => {
    const statsData = { messageCount: 42, fps: 10, instantFps: 12, lastSeen: Date.now() };
    mockCtx.getTopicStats.mockReturnValue(statsData as any);

    const { result } = renderHook(() => useZenohSubscription('test/topic'));

    // Advance timer to trigger poll
    act(() => {
      vi.advanceTimersByTime(2000);
    });

    expect(result.current.messageCount).toBe(42);
    expect(result.current.fps).toBe(10);
    expect(result.current.instantFps).toBe(12);
  });

  it('does not update stats when getTopicStats returns null', () => {
    mockCtx.getTopicStats.mockReturnValue(null);

    const { result } = renderHook(() => useZenohSubscription('test/topic'));

    act(() => {
      vi.advanceTimersByTime(2000);
    });

    expect(result.current.messageCount).toBe(0);
    expect(result.current.fps).toBe(0);
    expect(result.current.instantFps).toBe(0);
  });

  it('passes onSample callback through wrapper', () => {
    const onSample = vi.fn();
    renderHook(() => useZenohSubscription('test/topic', onSample));

    // Extract the wrapper callback passed to subscribe
    expect(mockCtx.subscribe).toHaveBeenCalled();
    const subscribeCalls = mockCtx.subscribe.mock.calls as any[];
    const wrapperCallback = subscribeCalls[0]?.[1] as ((sample: any) => void) | undefined;
    expect(wrapperCallback).toBeDefined();

    // Simulate a sample arriving
    const fakeSample = { payload: new Uint8Array([1, 2, 3]) };
    wrapperCallback!(fakeSample);

    expect(onSample).toHaveBeenCalledWith(fakeSample);
  });

  it('clears stats poll interval on unmount', () => {
    const { unmount } = renderHook(() => useZenohSubscription('test/topic'));

    unmount();

    // Advance time after unmount -- should not call getTopicStats
    mockCtx.getTopicStats.mockClear();
    act(() => {
      vi.advanceTimersByTime(4000);
    });

    expect(mockCtx.getTopicStats).not.toHaveBeenCalled();
  });
});

describe('useAllTopicStats', () => {
  let mockCtx: ReturnType<typeof createMockContext>;

  beforeEach(() => {
    vi.useFakeTimers();
    mockCtx = createMockContext();
    vi.mocked(useZenohSubscriptionContext).mockReturnValue(mockCtx as any);
  });

  afterEach(() => {
    vi.useRealTimers();
    vi.clearAllMocks();
  });

  it('returns empty map initially', () => {
    const { result } = renderHook(() => useAllTopicStats());

    expect(result.current).toBeInstanceOf(Map);
    expect(result.current.size).toBe(0);
  });

  it('polls at default 1s interval', () => {
    renderHook(() => useAllTopicStats());

    // getAllStats called once initially in the effect
    expect(mockCtx.getAllStats).toHaveBeenCalledTimes(1);

    act(() => {
      vi.advanceTimersByTime(1000);
    });

    expect(mockCtx.getAllStats).toHaveBeenCalledTimes(2);

    act(() => {
      vi.advanceTimersByTime(1000);
    });

    expect(mockCtx.getAllStats).toHaveBeenCalledTimes(3);
  });

  it('polls at specified interval', () => {
    renderHook(() => useAllTopicStats(500));

    // Initial call
    expect(mockCtx.getAllStats).toHaveBeenCalledTimes(1);

    act(() => {
      vi.advanceTimersByTime(500);
    });

    expect(mockCtx.getAllStats).toHaveBeenCalledTimes(2);

    act(() => {
      vi.advanceTimersByTime(500);
    });

    expect(mockCtx.getAllStats).toHaveBeenCalledTimes(3);
  });

  it('updates with data from getAllStats', () => {
    const statsMap = new Map([
      ['topic1', { messageCount: 10, fps: 5, instantFps: 6, lastSeen: 1000 }],
      ['topic2', { messageCount: 20, fps: 8, instantFps: 9, lastSeen: 2000 }],
    ]);
    mockCtx.getAllStats.mockReturnValue(statsMap);

    const { result } = renderHook(() => useAllTopicStats());

    // The initial fetch in useEffect should populate the stats
    // But since setStats happens asynchronously, we advance timers
    act(() => {
      vi.advanceTimersByTime(0);
    });

    expect(result.current.size).toBe(2);
    expect(result.current.get('topic1')?.messageCount).toBe(10);
    expect(result.current.get('topic2')?.messageCount).toBe(20);
  });

  it('cleans up interval on unmount', () => {
    const { unmount } = renderHook(() => useAllTopicStats());

    unmount();

    mockCtx.getAllStats.mockClear();
    act(() => {
      vi.advanceTimersByTime(5000);
    });

    expect(mockCtx.getAllStats).not.toHaveBeenCalled();
  });
});
