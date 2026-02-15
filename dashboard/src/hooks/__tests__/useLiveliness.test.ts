import { describe, it, expect, vi, beforeEach, afterEach } from 'vitest';
import { renderHook, act, waitFor } from '@testing-library/react';
import { useLiveliness } from '../useLiveliness';

function createMockSession(): any {
  return {
    declareSubscriber: vi.fn().mockResolvedValue({ undeclare: vi.fn() }),
    get: vi.fn().mockResolvedValue((async function* () {})()),
    put: vi.fn().mockResolvedValue(undefined),
    close: vi.fn().mockResolvedValue(undefined),
    liveliness: vi.fn().mockReturnValue({
      declare_subscriber: vi.fn().mockResolvedValue({ undeclare: vi.fn() }),
      declare_token: vi.fn().mockResolvedValue({ undeclare: vi.fn() }),
      get: vi.fn().mockResolvedValue((async function* () {})()),
    }),
  };
}

describe('useLiveliness', () => {
  beforeEach(() => {
    vi.clearAllMocks();
  });

  afterEach(() => {
    vi.restoreAllMocks();
  });

  it('initializes with empty state', () => {
    const mockSession = createMockSession();
    const { result } = renderHook(() => useLiveliness(mockSession));

    expect(result.current.aliveNodes).toBeInstanceOf(Set);
    expect(result.current.aliveNodes.size).toBe(0);
    expect(result.current.events).toEqual([]);
  });

  it('handles null session gracefully', () => {
    const { result } = renderHook(() => useLiveliness(null));

    expect(result.current.aliveNodes).toBeInstanceOf(Set);
    expect(result.current.aliveNodes.size).toBe(0);
    expect(result.current.events).toEqual([]);
  });

  it('gracefully handles missing liveliness API', async () => {
    const mockSession = createMockSession();
    // Remove liveliness API to simulate older zenoh-ts version
    (mockSession as any).liveliness = undefined;

    const consoleLogSpy = vi.spyOn(console, 'log').mockImplementation(() => {});

    const { result } = renderHook(() => useLiveliness(mockSession));

    // Wait for async setup to complete
    await waitFor(() => {
      expect(consoleLogSpy).toHaveBeenCalledWith(
        '[Liveliness] API not available in this zenoh-ts version'
      );
    });

    expect(result.current.aliveNodes.size).toBe(0);
    expect(result.current.events).toEqual([]);
  });

  it('subscribes to bubbaloop/** when session is available', async () => {
    const mockSession = createMockSession();
    const mockSubscriber = { undeclare: vi.fn() };
    const declareSubscriberSpy = vi.fn().mockResolvedValue(mockSubscriber);

    (mockSession as any).liveliness = () => ({
      declare_subscriber: declareSubscriberSpy,
    });

    renderHook(() => useLiveliness(mockSession));

    await waitFor(() => {
      expect(declareSubscriberSpy).toHaveBeenCalledWith('bubbaloop/**', {
        callback: expect.any(Function),
      });
    });
  });

  it('tracks join events and adds to aliveNodes', async () => {
    const mockSession = createMockSession();
    let capturedCallback: ((sample: any) => void) | undefined;

    (mockSession as any).liveliness = () => ({
      declare_subscriber: vi.fn().mockImplementation((_topic: string, options: any) => {
        capturedCallback = options.callback;
        return Promise.resolve({ undeclare: vi.fn() });
      }),
    });

    const { result } = renderHook(() => useLiveliness(mockSession));

    // Wait for subscription to be set up
    await waitFor(() => {
      expect(capturedCallback).toBeDefined();
    });

    // Simulate a join event
    act(() => {
      capturedCallback!({
        keyexpr: { toString: () => 'bubbaloop/node1' },
        kind: 'PUT',
      });
    });

    expect(result.current.aliveNodes.has('bubbaloop/node1')).toBe(true);
    expect(result.current.events).toHaveLength(1);
    expect(result.current.events[0]).toMatchObject({
      keyExpr: 'bubbaloop/node1',
      type: 'join',
    });
  });

  it('tracks leave events and removes from aliveNodes', async () => {
    const mockSession = createMockSession();
    let capturedCallback: ((sample: any) => void) | undefined;

    (mockSession as any).liveliness = () => ({
      declare_subscriber: vi.fn().mockImplementation((_topic: string, options: any) => {
        capturedCallback = options.callback;
        return Promise.resolve({ undeclare: vi.fn() });
      }),
    });

    const { result } = renderHook(() => useLiveliness(mockSession));

    await waitFor(() => {
      expect(capturedCallback).toBeDefined();
    });

    // Join first
    act(() => {
      capturedCallback!({
        keyexpr: { toString: () => 'bubbaloop/node1' },
        kind: 'PUT',
      });
    });

    expect(result.current.aliveNodes.has('bubbaloop/node1')).toBe(true);

    // Then leave
    act(() => {
      capturedCallback!({
        keyexpr: { toString: () => 'bubbaloop/node1' },
        kind: 'DELETE',
      });
    });

    expect(result.current.aliveNodes.has('bubbaloop/node1')).toBe(false);
    expect(result.current.events).toHaveLength(2);
    expect(result.current.events[1]).toMatchObject({
      keyExpr: 'bubbaloop/node1',
      type: 'leave',
    });
  });

  it('calls onNodeJoin callback when node joins', async () => {
    const mockSession = createMockSession();
    let capturedCallback: ((sample: any) => void) | undefined;
    const onNodeJoin = vi.fn();

    (mockSession as any).liveliness = () => ({
      declare_subscriber: vi.fn().mockImplementation((_topic: string, options: any) => {
        capturedCallback = options.callback;
        return Promise.resolve({ undeclare: vi.fn() });
      }),
    });

    renderHook(() => useLiveliness(mockSession, onNodeJoin));

    await waitFor(() => {
      expect(capturedCallback).toBeDefined();
    });

    act(() => {
      capturedCallback!({
        keyexpr: { toString: () => 'bubbaloop/camera' },
        kind: 'PUT',
      });
    });

    expect(onNodeJoin).toHaveBeenCalledWith('bubbaloop/camera');
  });

  it('calls onNodeLeave callback when node leaves', async () => {
    const mockSession = createMockSession();
    let capturedCallback: ((sample: any) => void) | undefined;
    const onNodeLeave = vi.fn();

    (mockSession as any).liveliness = () => ({
      declare_subscriber: vi.fn().mockImplementation((_topic: string, options: any) => {
        capturedCallback = options.callback;
        return Promise.resolve({ undeclare: vi.fn() });
      }),
    });

    renderHook(() => useLiveliness(mockSession, undefined, onNodeLeave));

    await waitFor(() => {
      expect(capturedCallback).toBeDefined();
    });

    act(() => {
      capturedCallback!({
        keyexpr: { toString: () => 'bubbaloop/weather' },
        kind: 'DELETE',
      });
    });

    expect(onNodeLeave).toHaveBeenCalledWith('bubbaloop/weather');
  });

  it('keeps last 100 events only', async () => {
    const mockSession = createMockSession();
    let capturedCallback: ((sample: any) => void) | undefined;

    (mockSession as any).liveliness = () => ({
      declare_subscriber: vi.fn().mockImplementation((_topic: string, options: any) => {
        capturedCallback = options.callback;
        return Promise.resolve({ undeclare: vi.fn() });
      }),
    });

    const { result } = renderHook(() => useLiveliness(mockSession));

    await waitFor(() => {
      expect(capturedCallback).toBeDefined();
    });

    // Simulate 150 events
    act(() => {
      for (let i = 0; i < 150; i++) {
        capturedCallback!({
          keyexpr: { toString: () => `bubbaloop/node${i}` },
          kind: 'PUT',
        });
      }
    });

    expect(result.current.events).toHaveLength(100);
    // First event should be node50 (events 0-49 were dropped)
    expect(result.current.events[0].keyExpr).toBe('bubbaloop/node50');
  });

  it('handles alternative keyExpr accessor (key_expr)', async () => {
    const mockSession = createMockSession();
    let capturedCallback: ((sample: any) => void) | undefined;

    (mockSession as any).liveliness = () => ({
      declare_subscriber: vi.fn().mockImplementation((_topic: string, options: any) => {
        capturedCallback = options.callback;
        return Promise.resolve({ undeclare: vi.fn() });
      }),
    });

    const { result } = renderHook(() => useLiveliness(mockSession));

    await waitFor(() => {
      expect(capturedCallback).toBeDefined();
    });

    // Use key_expr instead of keyexpr
    act(() => {
      capturedCallback!({
        key_expr: { toString: () => 'bubbaloop/alternative' },
        kind: 'PUT',
      });
    });

    expect(result.current.aliveNodes.has('bubbaloop/alternative')).toBe(true);
  });

  it('undeclares subscriber on unmount', async () => {
    const mockSession = createMockSession();
    const undeclareSpy = vi.fn();

    (mockSession as any).liveliness = () => ({
      declare_subscriber: vi.fn().mockResolvedValue({
        undeclare: undeclareSpy,
      }),
    });

    const { unmount } = renderHook(() => useLiveliness(mockSession));

    await waitFor(() => {
      expect(undeclareSpy).not.toHaveBeenCalled();
    });

    unmount();

    expect(undeclareSpy).toHaveBeenCalled();
  });

  it('handles errors during setup gracefully', async () => {
    const mockSession = createMockSession();
    const consoleLogSpy = vi.spyOn(console, 'log').mockImplementation(() => {});

    (mockSession as any).liveliness = () => ({
      declare_subscriber: vi.fn().mockRejectedValue(new Error('Connection failed')),
    });

    const { result } = renderHook(() => useLiveliness(mockSession));

    await waitFor(() => {
      expect(consoleLogSpy).toHaveBeenCalledWith(
        '[Liveliness] Not supported:',
        expect.any(Error)
      );
    });

    expect(result.current.aliveNodes.size).toBe(0);
    expect(result.current.events).toEqual([]);
  });

  it('does not re-subscribe when callbacks change', async () => {
    const mockSession = createMockSession();
    const declareSubscriberSpy = vi.fn().mockResolvedValue({ undeclare: vi.fn() });

    (mockSession as any).liveliness = () => ({
      declare_subscriber: declareSubscriberSpy,
    });

    const onJoin1 = vi.fn();
    const onJoin2 = vi.fn();

    const { rerender } = renderHook(
      ({ onJoin }) => useLiveliness(mockSession, onJoin),
      { initialProps: { onJoin: onJoin1 } }
    );

    await waitFor(() => {
      expect(declareSubscriberSpy).toHaveBeenCalledTimes(1);
    });

    // Change callback
    rerender({ onJoin: onJoin2 });

    // Should not re-subscribe
    expect(declareSubscriberSpy).toHaveBeenCalledTimes(1);
  });
});
