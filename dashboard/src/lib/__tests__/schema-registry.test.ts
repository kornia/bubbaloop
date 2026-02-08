import { describe, it, expect } from 'vitest';
import { snakeToCamel, extractTopicPrefix } from '../schema-registry';
import { extractMachineId } from '../zenoh';

describe('snakeToCamel', () => {
  it('converts snake_case keys to camelCase', () => {
    const input = { field_name: 'value', another_field: 123 };
    const expected = { fieldName: 'value', anotherField: 123 };
    expect(snakeToCamel(input)).toEqual(expected);
  });

  it('handles nested objects', () => {
    const input = {
      field_name: 1,
      nested_obj: {
        inner_field: 2,
        deep_nested: {
          very_deep_field: 3,
        },
      },
    };
    const expected = {
      fieldName: 1,
      nestedObj: {
        innerField: 2,
        deepNested: {
          veryDeepField: 3,
        },
      },
    };
    expect(snakeToCamel(input)).toEqual(expected);
  });

  it('handles arrays', () => {
    const input = {
      items: [
        { field_name: 'a' },
        { field_name: 'b' },
      ],
    };
    const expected = {
      items: [
        { fieldName: 'a' },
        { fieldName: 'b' },
      ],
    };
    expect(snakeToCamel(input)).toEqual(expected);
  });

  it('returns primitives unchanged', () => {
    expect(snakeToCamel('string')).toBe('string');
    expect(snakeToCamel(123)).toBe(123);
    expect(snakeToCamel(true)).toBe(true);
    expect(snakeToCamel(false)).toBe(false);
  });

  it('returns null unchanged', () => {
    expect(snakeToCamel(null)).toBe(null);
  });

  it('returns undefined unchanged', () => {
    expect(snakeToCamel(undefined)).toBe(undefined);
  });

  it('filters __proto__ keys', () => {
    const input = { __proto__: 'dangerous', safe_field: 'ok' };
    const result = snakeToCamel(input) as Record<string, unknown>;
    expect(result).not.toHaveProperty('__proto__');
    expect(result).toHaveProperty('safeField', 'ok');
  });

  it('filters constructor keys', () => {
    const input = { constructor: 'dangerous', safe_field: 'ok' };
    const result = snakeToCamel(input) as Record<string, unknown>;
    expect(result).not.toHaveProperty('constructor');
    expect(result).toHaveProperty('safeField', 'ok');
  });

  it('filters prototype keys', () => {
    const input = { prototype: 'dangerous', safe_field: 'ok' };
    const result = snakeToCamel(input) as Record<string, unknown>;
    expect(result).not.toHaveProperty('prototype');
    expect(result).toHaveProperty('safeField', 'ok');
  });

  it('handles keys with multiple underscores', () => {
    const input = { field_name_with_many_parts: 'value' };
    const expected = { fieldNameWithManyParts: 'value' };
    expect(snakeToCamel(input)).toEqual(expected);
  });

  it('handles keys with leading underscores', () => {
    const input = { _private_field: 'value' };
    // The regex converts _p to P, so _private_field becomes PrivateField
    const expected = { PrivateField: 'value' };
    expect(snakeToCamel(input)).toEqual(expected);
  });

  it('handles empty objects', () => {
    expect(snakeToCamel({})).toEqual({});
  });

  it('handles empty arrays', () => {
    expect(snakeToCamel([])).toEqual([]);
  });
});

describe('extractTopicPrefix', () => {
  it('extracts prefix from vanilla zenoh topic', () => {
    const topic = 'bubbaloop/local/m1/system-telemetry/metrics';
    expect(extractTopicPrefix(topic)).toBe('bubbaloop/local/m1/system-telemetry');
  });

  it('extracts prefix from ros-z topic', () => {
    const topic = '0/bubbaloop%local%m1%node%res/Type/Hash';
    expect(extractTopicPrefix(topic)).toBe('bubbaloop/local/m1/node');
  });

  it('returns null for short topics (fewer than 4 segments)', () => {
    expect(extractTopicPrefix('bubbaloop/foo')).toBe(null);
    expect(extractTopicPrefix('bubbaloop/foo/bar')).toBe(null);
  });

  it('returns null for non-bubbaloop topics', () => {
    expect(extractTopicPrefix('other/thing/foo/bar')).toBe(null);
  });

  it('handles daemon topics', () => {
    const topic = 'bubbaloop/local/m1/daemon/api/health';
    expect(extractTopicPrefix(topic)).toBe('bubbaloop/local/m1/daemon');
  });

  it('handles camera topics', () => {
    const topic = 'bubbaloop/local/m1/camera/entrance/compressed';
    expect(extractTopicPrefix(topic)).toBe('bubbaloop/local/m1/camera');
  });

  it('handles ros-z with multiple resource segments', () => {
    const topic = '0/bubbaloop%local%nvidia_orin00%camera%entrance%compressed/Type/RIHS123';
    expect(extractTopicPrefix(topic)).toBe('bubbaloop/local/nvidia_orin00/camera');
  });

  it('handles machine-scoped daemon topics', () => {
    const topic = 'bubbaloop/nvidia-orin00/daemon/nodes';
    // This has 4 segments: bubbaloop/nvidia-orin00/daemon/nodes
    expect(extractTopicPrefix(topic)).toBe('bubbaloop/nvidia-orin00/daemon/nodes');
  });
});

describe('extractMachineId', () => {
  it('extracts from scoped vanilla topic', () => {
    const topic = 'bubbaloop/local/nvidia_orin00/health/system-telemetry';
    expect(extractMachineId(topic)).toBe('nvidia_orin00');
  });

  it('extracts from machine-scoped daemon', () => {
    const topic = 'bubbaloop/nvidia-orin00/daemon/nodes';
    expect(extractMachineId(topic)).toBe('nvidia-orin00');
  });

  it('returns null for legacy daemon', () => {
    const topic = 'bubbaloop/daemon/nodes';
    expect(extractMachineId(topic)).toBe(null);
  });

  it('extracts from ros-z topic', () => {
    const topic = '0/bubbaloop%local%nvidia_orin00%camera%entrance/Type/Hash';
    expect(extractMachineId(topic)).toBe('nvidia_orin00');
  });

  it('returns null for legacy fleet topics', () => {
    const topic = 'bubbaloop/fleet/status';
    expect(extractMachineId(topic)).toBe(null);
  });

  it('handles system telemetry topic', () => {
    const topic = 'bubbaloop/local/m1/system-telemetry/metrics';
    expect(extractMachineId(topic)).toBe('m1');
  });

  it('handles weather topics', () => {
    const topic = 'bubbaloop/local/nvidia_orin00/weather/current';
    expect(extractMachineId(topic)).toBe('nvidia_orin00');
  });

  it('handles health topics', () => {
    const topic = 'bubbaloop/local/nvidia_orin00/health/system-telemetry';
    expect(extractMachineId(topic)).toBe('nvidia_orin00');
  });

  it('handles ros-z with domain ID 1', () => {
    const topic = '1/bubbaloop%local%test_machine%node%resource/Type/Hash';
    expect(extractMachineId(topic)).toBe('test_machine');
  });

  it('returns null for topics with insufficient segments', () => {
    expect(extractMachineId('bubbaloop/local')).toBe(null);
  });
});
