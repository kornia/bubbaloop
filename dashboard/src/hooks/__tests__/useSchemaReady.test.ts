import { vi, describe, it, expect, beforeEach } from 'vitest';
import { renderHook } from '@testing-library/react';

const _mockState = vi.hoisted(() => ({ schemaVersion: 0 }));

vi.mock('../../contexts/SchemaRegistryContext', () => ({
  useSchemaRegistry: () => ({
    registry: {},
    loading: false,
    error: null,
    refresh: vi.fn(),
    decode: vi.fn(),
    discoverForTopic: vi.fn(),
    schemaVersion: _mockState.schemaVersion,
  }),
}));

import { useSchemaReady } from '../useSchemaReady';

describe('useSchemaReady', () => {
  beforeEach(() => {
    _mockState.schemaVersion = 0;
  });

  it('returns false when schemaVersion is 0', () => {
    _mockState.schemaVersion = 0;
    const { result } = renderHook(() => useSchemaReady());
    expect(result.current).toBe(false);
  });

  it('returns true when schemaVersion > 0', () => {
    _mockState.schemaVersion = 1;
    const { result } = renderHook(() => useSchemaReady());
    expect(result.current).toBe(true);
  });

  it('returns true when schemaVersion is greater than 1', () => {
    _mockState.schemaVersion = 5;
    const { result } = renderHook(() => useSchemaReady());
    expect(result.current).toBe(true);
  });
});
