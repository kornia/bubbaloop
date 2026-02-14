import { describe, it, expect, vi, beforeEach, afterEach } from 'vitest';
import { renderHook, act } from '@testing-library/react';
import { ReactNode } from 'react';
import { SchemaRegistryProvider, useSchemaRegistry } from '../SchemaRegistryContext';

// Mock FleetContext
vi.mock('../FleetContext', () => ({
  useFleetContext: vi.fn(() => ({ machines: [] })),
}));

// Mock SchemaRegistry class
const mockFetchCoreSchemas = vi.fn().mockResolvedValue(true);
const mockDiscoverAllNodeSchemas = vi.fn().mockResolvedValue(0);
const mockDiscoverSchemaForTopic = vi.fn().mockResolvedValue(false);
const mockDecode = vi.fn().mockReturnValue(null);
const mockClear = vi.fn();

vi.mock('../../lib/schema-registry', () => {
  return {
    SchemaRegistry: class MockSchemaRegistry {
      fetchCoreSchemas = mockFetchCoreSchemas;
      discoverAllNodeSchemas = mockDiscoverAllNodeSchemas;
      discoverSchemaForTopic = mockDiscoverSchemaForTopic;
      decode = mockDecode;
      clear = mockClear;
    },
  };
});

// Minimal mock session
function createMockSession(): any {
  return { isClosed: false };
}

describe('SchemaRegistryContext', () => {
  beforeEach(() => {
    vi.clearAllMocks();
    vi.useFakeTimers();
  });

  afterEach(() => {
    vi.useRealTimers();
  });

  function makeWrapper(session: any) {
    return function Wrapper({ children }: { children: ReactNode }) {
      return (
        <SchemaRegistryProvider session={session}>
          {children}
        </SchemaRegistryProvider>
      );
    };
  }

  it('renders children', () => {
    const wrapper = makeWrapper(null);
    const { result } = renderHook(() => useSchemaRegistry(), { wrapper });
    expect(result.current).toBeDefined();
  });

  it('useSchemaRegistry throws outside provider', () => {
    const spy = vi.spyOn(console, 'error').mockImplementation(() => {});
    expect(() => {
      renderHook(() => useSchemaRegistry());
    }).toThrow('useSchemaRegistry must be used within a SchemaRegistryProvider');
    spy.mockRestore();
  });

  it('schemaVersion starts at 0', () => {
    const wrapper = makeWrapper(null);
    const { result } = renderHook(() => useSchemaRegistry(), { wrapper });
    expect(result.current.schemaVersion).toBe(0);
  });

  it('loading starts as false', () => {
    const wrapper = makeWrapper(null);
    const { result } = renderHook(() => useSchemaRegistry(), { wrapper });
    expect(result.current.loading).toBe(false);
  });

  it('registry is a SchemaRegistry instance', () => {
    const wrapper = makeWrapper(null);
    const { result } = renderHook(() => useSchemaRegistry(), { wrapper });
    expect(result.current.registry).toBeDefined();
    expect(result.current.registry.fetchCoreSchemas).toBeDefined();
    expect(result.current.registry.decode).toBeDefined();
  });

  it('calls fetchCoreSchemas and discoverAllNodeSchemas when session is provided', async () => {
    const session = createMockSession();
    const wrapper = makeWrapper(session);

    renderHook(() => useSchemaRegistry(), { wrapper });

    // Flush the microtask queue for the Promise.all in fetchSchemas
    await act(async () => {
      await Promise.resolve();
    });

    expect(mockFetchCoreSchemas).toHaveBeenCalledWith(session, undefined);
    expect(mockDiscoverAllNodeSchemas).toHaveBeenCalledWith(session, undefined);
  });

  it('increments schemaVersion when fetchCoreSchemas succeeds', async () => {
    mockFetchCoreSchemas.mockResolvedValue(true);
    mockDiscoverAllNodeSchemas.mockResolvedValue(0);

    const session = createMockSession();
    const wrapper = makeWrapper(session);

    const { result } = renderHook(() => useSchemaRegistry(), { wrapper });

    await act(async () => {
      await Promise.resolve();
    });

    expect(result.current.schemaVersion).toBe(1);
  });

  it('clears registry and resets schemaVersion when session becomes null', async () => {
    mockFetchCoreSchemas.mockResolvedValue(true);
    mockDiscoverAllNodeSchemas.mockResolvedValue(0);

    // First render with session, verify version increments
    const session = createMockSession();
    const sessionWrapper = makeWrapper(session);

    const { result: result1 } = renderHook(() => useSchemaRegistry(), {
      wrapper: sessionWrapper,
    });

    await act(async () => {
      await Promise.resolve();
    });

    expect(result1.current.schemaVersion).toBe(1);

    // Then render a fresh hook with null session, verify clear is called and version is 0
    mockClear.mockClear();

    const nullWrapper = makeWrapper(null);
    const { result: result2 } = renderHook(() => useSchemaRegistry(), {
      wrapper: nullWrapper,
    });

    expect(result2.current.schemaVersion).toBe(0);
    expect(mockClear).toHaveBeenCalled();
  });

  it('decode delegates to registry.decode', () => {
    const decodeResult = { data: { temp: 25 }, typeName: 'Weather', source: 'core' };
    mockDecode.mockReturnValue(decodeResult);

    const wrapper = makeWrapper(null);
    const { result } = renderHook(() => useSchemaRegistry(), { wrapper });

    const data = new Uint8Array([1, 2, 3]);
    const decoded = result.current.decode('Weather', data);

    expect(mockDecode).toHaveBeenCalledWith('Weather', data);
    expect(decoded).toEqual(decodeResult);
  });

  it('discoverForTopic calls registry.discoverSchemaForTopic and increments schemaVersion on success', async () => {
    mockFetchCoreSchemas.mockResolvedValue(true);
    mockDiscoverAllNodeSchemas.mockResolvedValue(0);
    mockDiscoverSchemaForTopic.mockResolvedValue(true);

    const session = createMockSession();
    const wrapper = makeWrapper(session);

    const { result } = renderHook(() => useSchemaRegistry(), { wrapper });

    // Wait for initial fetch to complete
    await act(async () => {
      await Promise.resolve();
    });

    const versionBefore = result.current.schemaVersion;

    await act(async () => {
      result.current.discoverForTopic('0/weather%current');
      // Flush the promise from discoverSchemaForTopic
      await Promise.resolve();
      await Promise.resolve();
    });

    expect(mockDiscoverSchemaForTopic).toHaveBeenCalledWith(session, '0/weather%current');
    expect(result.current.schemaVersion).toBeGreaterThan(versionBefore);
  });

  it('discoverForTopic does not increment schemaVersion when discovery returns false', async () => {
    mockFetchCoreSchemas.mockResolvedValue(true);
    mockDiscoverAllNodeSchemas.mockResolvedValue(0);
    mockDiscoverSchemaForTopic.mockResolvedValue(false);

    const session = createMockSession();
    const wrapper = makeWrapper(session);

    const { result } = renderHook(() => useSchemaRegistry(), { wrapper });

    await act(async () => {
      await Promise.resolve();
    });

    const versionBefore = result.current.schemaVersion;

    await act(async () => {
      result.current.discoverForTopic('0/unknown%topic');
      await Promise.resolve();
      await Promise.resolve();
    });

    expect(result.current.schemaVersion).toBe(versionBefore);
  });

  it('error is set when fetchCoreSchemas returns false and no node schemas found', async () => {
    mockFetchCoreSchemas.mockResolvedValue(false);
    mockDiscoverAllNodeSchemas.mockResolvedValue(0);

    const session = createMockSession();
    const wrapper = makeWrapper(session);

    const { result } = renderHook(() => useSchemaRegistry(), { wrapper });

    await act(async () => {
      await Promise.resolve();
    });

    expect(result.current.error).toBe('No schemas returned from daemon');
  });

  it('error is null when session is null', () => {
    const wrapper = makeWrapper(null);
    const { result } = renderHook(() => useSchemaRegistry(), { wrapper });
    expect(result.current.error).toBeNull();
  });
});
