import { describe, it, expect } from 'vitest';
import { extractMachineId, getSamplePayload, normalizeKeyExpr } from '../zenoh';
import type { Sample } from '@eclipse-zenoh/zenoh-ts';

describe('extractMachineId', () => {
  describe('ros-z format', () => {
    it('extracts machine id from ros-z encoded topic', () => {
      const key = '0/bubbaloop%local%nvidia_orin00%camera%entrance/bubbaloop.camera.v1.Image/RIHS01_abc';
      expect(extractMachineId(key)).toBe('nvidia_orin00');
    });

    it('extracts machine id from ros-z with different scope', () => {
      const key = '0/bubbaloop%production%jetson_nano%health%system/type/hash';
      expect(extractMachineId(key)).toBe('jetson_nano');
    });

    it('returns null for ros-z without bubbaloop prefix', () => {
      const key = '0/other%topic/type/hash';
      expect(extractMachineId(key)).toBeNull();
    });

    it('returns null for ros-z with too few segments', () => {
      const key = '0/bubbaloop%short/type/hash';
      // decoded: "bubbaloop/short" → segments length < 3
      expect(extractMachineId(key)).toBeNull();
    });

    it('handles ros-z with multiple domain IDs', () => {
      const key = '42/bubbaloop%dev%orin_dev01%weather%current/type/hash';
      expect(extractMachineId(key)).toBe('orin_dev01');
    });
  });

  describe('vanilla zenoh format', () => {
    it('extracts machine id from machine-scoped daemon path', () => {
      expect(extractMachineId('bubbaloop/nvidia-orin00/daemon/nodes')).toBe('nvidia-orin00');
    });

    it('extracts machine id from machine-scoped daemon API path', () => {
      expect(extractMachineId('bubbaloop/jetson-nano/daemon/api/health')).toBe('jetson-nano');
    });

    it('extracts machine id from full-scoped path', () => {
      expect(extractMachineId('bubbaloop/local/nvidia_orin00/health/system-telemetry')).toBe('nvidia_orin00');
    });

    it('extracts machine id from full-scoped camera path', () => {
      expect(extractMachineId('bubbaloop/production/orin_02/camera/entrance/compressed')).toBe('orin_02');
    });

    it('handles machine id with special characters', () => {
      expect(extractMachineId('bubbaloop/jetson-nano_01/daemon/api')).toBe('jetson-nano_01');
    });

    it('handles machine id with underscores', () => {
      expect(extractMachineId('bubbaloop/local/nvidia_orin_00/weather/current')).toBe('nvidia_orin_00');
    });
  });

  describe('legacy format (returns null)', () => {
    it('returns null for legacy daemon path', () => {
      expect(extractMachineId('bubbaloop/daemon/nodes')).toBeNull();
    });

    it('returns null for legacy daemon API path', () => {
      expect(extractMachineId('bubbaloop/daemon/api/health')).toBeNull();
    });

    it('returns null for fleet path', () => {
      expect(extractMachineId('bubbaloop/fleet/something')).toBeNull();
    });

    it('returns null for fleet with deeper path', () => {
      expect(extractMachineId('bubbaloop/fleet/nodes/list')).toBeNull();
    });
  });

  describe('edge cases', () => {
    it('returns null for non-bubbaloop prefix', () => {
      expect(extractMachineId('other/path/here')).toBeNull();
    });

    it('returns null for short bubbaloop path', () => {
      expect(extractMachineId('bubbaloop')).toBeNull();
    });

    it('returns null for bubbaloop with single segment', () => {
      expect(extractMachineId('bubbaloop/single')).toBeNull();
    });

    it('returns null for empty string', () => {
      expect(extractMachineId('')).toBeNull();
    });

    it('returns null for path with only two bubbaloop segments', () => {
      expect(extractMachineId('bubbaloop/local/nvidia')).toBeNull();
    });
  });
});

describe('normalizeKeyExpr', () => {
  describe('ros-z topics from different machines', () => {
    it('normalizes ros-z topic from nvidia_orin00', () => {
      const key = '0/bubbaloop%local%nvidia_orin00%camera%entrance%compressed/bubbaloop.camera.v1.Image/RIHS01_abc';
      const result = normalizeKeyExpr(key);
      expect(result.display).toBe('local/nvidia_orin00/camera/entrance/compressed');
      expect(result.raw).toBe(key);
    });

    it('normalizes ros-z topic from jetson_nano', () => {
      const key = '0/bubbaloop%production%jetson_nano%health%system/type/hash';
      const result = normalizeKeyExpr(key);
      expect(result.display).toBe('production/jetson_nano/health/system');
      expect(result.raw).toBe(key);
    });

    it('normalizes ros-z topic from orin_dev01 with different domain ID', () => {
      const key = '42/bubbaloop%dev%orin_dev01%weather%current/type/hash';
      const result = normalizeKeyExpr(key);
      expect(result.display).toBe('dev/orin_dev01/weather/current');
      expect(result.raw).toBe(key);
    });

    it('handles ros-z topic without bubbaloop prefix', () => {
      const key = '0/custom%topic%data/type/hash';
      const result = normalizeKeyExpr(key);
      expect(result.display).toBe('custom/topic/data');
      expect(result.raw).toBe(key);
    });
  });

  describe('vanilla zenoh topics from different scopes', () => {
    it('normalizes topic with local scope', () => {
      const key = 'bubbaloop/local/nvidia_orin00/health/system-telemetry';
      const result = normalizeKeyExpr(key);
      expect(result.display).toBe('local/nvidia_orin00/health/system-telemetry');
      expect(result.raw).toBe(key);
    });

    it('normalizes topic with production scope', () => {
      const key = 'bubbaloop/production/orin_02/camera/entrance/compressed';
      const result = normalizeKeyExpr(key);
      expect(result.display).toBe('production/orin_02/camera/entrance/compressed');
      expect(result.raw).toBe(key);
    });
  });

  describe('legacy topics without machine ID', () => {
    it('normalizes legacy daemon topic', () => {
      const result = normalizeKeyExpr('bubbaloop/daemon/nodes');
      expect(result.display).toBe('daemon/nodes');
    });

    it('normalizes legacy daemon API topic', () => {
      const result = normalizeKeyExpr('bubbaloop/daemon/api/health');
      expect(result.display).toBe('daemon/api/health');
    });
  });

  describe('machine-scoped daemon paths', () => {
    it('normalizes machine-scoped daemon topic', () => {
      const result = normalizeKeyExpr('bubbaloop/nvidia-orin00/daemon/nodes');
      expect(result.display).toBe('nvidia-orin00/daemon/nodes');
    });

    it('normalizes machine-scoped daemon API topic', () => {
      const result = normalizeKeyExpr('bubbaloop/jetson-nano/daemon/api/schemas');
      expect(result.display).toBe('jetson-nano/daemon/api/schemas');
    });
  });

  describe('edge cases', () => {
    it('returns raw key unchanged for non-bubbaloop topics', () => {
      const key = 'other/random/topic';
      const result = normalizeKeyExpr(key);
      expect(result.display).toBe(key);
      expect(result.raw).toBe(key);
    });
  });
});

describe('getSamplePayload', () => {
  it('extracts Uint8Array from sample with toBytes method', () => {
    const expectedBytes = new Uint8Array([1, 2, 3, 4, 5]);
    const mockSample = {
      payload: () => ({
        toBytes: () => expectedBytes,
      }),
    } as unknown as Sample;

    const result = getSamplePayload(mockSample);
    expect(result).toBe(expectedBytes);
  });

  it('handles payload that is already Uint8Array', () => {
    const expectedBytes = new Uint8Array([10, 20, 30]);
    const mockSample = {
      payload: () => expectedBytes,
    } as unknown as Sample;

    const result = getSamplePayload(mockSample);
    expect(result).toBe(expectedBytes);
  });

  it('returns empty Uint8Array when payload is null', () => {
    const mockSample = {
      payload: () => null,
    } as unknown as Sample;

    const result = getSamplePayload(mockSample);
    expect(result).toBeInstanceOf(Uint8Array);
    expect(result.length).toBe(0);
  });

  it('returns empty Uint8Array when toBytes is not a function', () => {
    const mockSample = {
      payload: () => ({ notToBytes: 'invalid' }),
    } as unknown as Sample;

    const result = getSamplePayload(mockSample);
    expect(result).toBeInstanceOf(Uint8Array);
    expect(result.length).toBe(0);
  });

  it('handles protobuf binary data', () => {
    // Simulate a real protobuf message payload
    const protobufBytes = new Uint8Array([0x08, 0x96, 0x01, 0x12, 0x04, 0x74, 0x65, 0x73, 0x74]);
    const mockSample = {
      payload: () => ({
        toBytes: () => protobufBytes,
      }),
    } as unknown as Sample;

    const result = getSamplePayload(mockSample);
    expect(result).toEqual(protobufBytes);
    expect(result.length).toBe(9);
  });
});
