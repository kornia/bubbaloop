import { describe, it, expect, beforeEach, vi } from 'vitest';
import { snakeToCamel, extractTopicPrefix, extractSchemaFromTopic, SchemaRegistry } from '../schema-registry';
import { extractMachineId } from '../zenoh';
import * as protobuf from 'protobufjs';
import 'protobufjs/ext/descriptor';

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

  describe('multi-machine topics', () => {
    it('extracts prefix from nvidia_orin00 camera topic', () => {
      const topic = 'bubbaloop/local/nvidia_orin00/camera/entrance/compressed';
      expect(extractTopicPrefix(topic)).toBe('bubbaloop/local/nvidia_orin00/camera');
    });

    it('extracts prefix from jetson_nano health topic', () => {
      const topic = 'bubbaloop/production/jetson_nano/health/system-telemetry';
      expect(extractTopicPrefix(topic)).toBe('bubbaloop/production/jetson_nano/health');
    });

    it('extracts prefix from orin_dev01 weather topic', () => {
      const topic = 'bubbaloop/dev/orin_dev01/weather/current';
      expect(extractTopicPrefix(topic)).toBe('bubbaloop/dev/orin_dev01/weather');
    });

    it('extracts prefix from ros-z topic on jetson_nano', () => {
      const topic = '0/bubbaloop%production%jetson_nano%camera%rear%compressed/Type/RIHS123';
      expect(extractTopicPrefix(topic)).toBe('bubbaloop/production/jetson_nano/camera');
    });

    it('extracts prefix from ros-z topic on orin_dev01', () => {
      const topic = '42/bubbaloop%dev%orin_dev01%system-telemetry%metrics/Type/Hash';
      expect(extractTopicPrefix(topic)).toBe('bubbaloop/dev/orin_dev01/system-telemetry');
    });
  });

  describe('topics with fewer than 4 segments', () => {
    it('returns null for single segment', () => {
      expect(extractTopicPrefix('bubbaloop')).toBe(null);
    });

    it('returns null for two segments', () => {
      expect(extractTopicPrefix('bubbaloop/local')).toBe(null);
    });

    it('returns null for three segments', () => {
      expect(extractTopicPrefix('bubbaloop/local/m1')).toBe(null);
    });

    it('returns null for empty string', () => {
      expect(extractTopicPrefix('')).toBe(null);
    });
  });

  describe('ros-z new format (slash-preserved)', () => {
    it('extracts prefix from new format camera topic', () => {
      const topic = '0/bubbaloop/local/nvidia_orin00/camera/entrance/compressed/bubbaloop.camera.v1.Image/RIHS01_abc';
      expect(extractTopicPrefix(topic)).toBe('bubbaloop/local/nvidia_orin00/camera');
    });

    it('extracts prefix from new format telemetry topic', () => {
      const topic = '0/bubbaloop/local/m1/system_telemetry/metrics/bubbaloop.system_telemetry.v1.SystemMetrics/RIHS01_xyz';
      expect(extractTopicPrefix(topic)).toBe('bubbaloop/local/m1/system_telemetry');
    });

    it('extracts prefix from new format weather topic', () => {
      const topic = '42/bubbaloop/dev/orin_dev01/weather/current/type/hash';
      expect(extractTopicPrefix(topic)).toBe('bubbaloop/dev/orin_dev01/weather');
    });

    it('returns null for new format without bubbaloop prefix', () => {
      const topic = '0/other/local/m1/node/output';
      expect(extractTopicPrefix(topic)).toBe(null);
    });

    it('returns null for new format with too few segments', () => {
      const topic = '0/bubbaloop/local/m1';
      expect(extractTopicPrefix(topic)).toBe(null);
    });
  });

  describe('format equivalence', () => {
    it('old and new format extract same prefix for camera topic', () => {
      const oldTopic = '0/bubbaloop%local%nvidia_orin00%camera%entrance%compressed/Type/RIHS123';
      const newTopic = '0/bubbaloop/local/nvidia_orin00/camera/entrance/compressed/Type/RIHS123';
      expect(extractTopicPrefix(oldTopic)).toBe(extractTopicPrefix(newTopic));
    });

    it('old and new format extract same prefix for telemetry topic', () => {
      const oldTopic = '0/bubbaloop%production%jetson_nano%system_telemetry%metrics/Type/Hash';
      const newTopic = '0/bubbaloop/production/jetson_nano/system_telemetry/metrics/Type/Hash';
      expect(extractTopicPrefix(oldTopic)).toBe(extractTopicPrefix(newTopic));
    });
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

describe('extractSchemaFromTopic', () => {
  it('extracts schema from ros-z topic format', () => {
    const topic = '0/weather%current/bubbaloop.weather.v1.CurrentWeather/RIHS01_abc123';
    expect(extractSchemaFromTopic(topic)).toBe('bubbaloop.weather.v1.CurrentWeather');
  });

  it('extracts schema from system telemetry ros-z topic', () => {
    const topic = '0/bubbaloop%local%m1%system-telemetry%metrics/bubbaloop.system_telemetry.v1.SystemMetrics/RIHS01_xyz';
    expect(extractSchemaFromTopic(topic)).toBe('bubbaloop.system_telemetry.v1.SystemMetrics');
  });

  it('returns null for vanilla zenoh topics without schema', () => {
    const topic = 'bubbaloop/local/m1/network-monitor/status';
    expect(extractSchemaFromTopic(topic)).toBe(null);
  });

  it('returns null for topics with no dotted segments', () => {
    const topic = 'bubbaloop/daemon/nodes';
    expect(extractSchemaFromTopic(topic)).toBe(null);
  });

  it('skips RIHS hash segments', () => {
    const topic = '0/topic/RIHS01_abc123';
    expect(extractSchemaFromTopic(topic)).toBe(null);
  });

  it('returns null for empty topic', () => {
    expect(extractSchemaFromTopic('')).toBe(null);
  });
});

// Create a test protobuf.Root with known types (bypasses FileDescriptorSet serialization)
function buildTestRoot(): protobuf.Root {
  const root = new protobuf.Root();

  const testNs = root.define('test.v1');
  const testMsg = new protobuf.Type('TestMessage');
  testMsg.add(new protobuf.Field('name', 1, 'string'));
  testMsg.add(new protobuf.Field('value', 2, 'int32'));
  testNs.add(testMsg);

  const weatherNs = root.define('bubbaloop.weather.v1');
  const weatherMsg = new protobuf.Type('CurrentWeather');
  weatherMsg.add(new protobuf.Field('temperature', 1, 'float'));
  weatherMsg.add(new protobuf.Field('humidity', 2, 'float'));
  weatherNs.add(weatherMsg);

  root.resolveAll();
  return root;
}

// Inject a Root directly into a SchemaRegistry (bypasses loadDescriptorSet)
function injectSchema(registry: SchemaRegistry, root: protobuf.Root, key: string, label: string) {
  // eslint-disable-next-line @typescript-eslint/no-explicit-any
  const schemas = (registry as any).schemas as Map<string, { source: string; root: protobuf.Root }>;
  schemas.set(key, { source: label, root });
  // Invalidate caches
  // eslint-disable-next-line @typescript-eslint/no-explicit-any
  (registry as any).typeNamesCache = null;
  // eslint-disable-next-line @typescript-eslint/no-explicit-any
  (registry as any).topicGuessCache?.clear?.();
}

describe('SchemaRegistry', () => {
  let registry: SchemaRegistry;
  let testRoot: protobuf.Root;

  beforeEach(() => {
    registry = new SchemaRegistry();
    testRoot = buildTestRoot();
    injectSchema(registry, testRoot, 'test', 'test-source');
  });

  describe('decode', () => {
    it('decodes a known type', () => {
      const msgType = registry.lookupType('test.v1.TestMessage');
      expect(msgType).not.toBeNull();
      const encoded = msgType!.encode({ name: 'hello', value: 42 }).finish();
      const result = registry.decode('test.v1.TestMessage', encoded);
      expect(result).not.toBeNull();
      expect(result!.data).toEqual({ name: 'hello', value: 42 });
      expect(result!.typeName).toBe('test.v1.TestMessage');
      expect(result!.source).toBe('test-source');
    });

    it('returns null for unknown type', () => {
      const result = registry.decode('nonexistent.Type', new Uint8Array([0x0a, 0x03, 0x66, 0x6f, 0x6f]));
      expect(result).toBeNull();
    });

    it('decodes weather message', () => {
      const msgType = registry.lookupType('bubbaloop.weather.v1.CurrentWeather')!;
      const encoded = msgType.encode({ temperature: 23.5, humidity: 65.0 }).finish();
      const result = registry.decode('bubbaloop.weather.v1.CurrentWeather', encoded);
      expect(result).not.toBeNull();
      expect((result!.data as Record<string, number>).temperature).toBeCloseTo(23.5);
      expect((result!.data as Record<string, number>).humidity).toBeCloseTo(65.0);
    });
  });

  describe('tryDecodeForTopic', () => {
    it('decodes using ros-z schema hint in topic', () => {
      const msgType = registry.lookupType('bubbaloop.weather.v1.CurrentWeather')!;
      const encoded = msgType.encode({ temperature: 23.5, humidity: 65.0 }).finish();
      const topic = '0/bubbaloop%local%m1%weather%current/bubbaloop.weather.v1.CurrentWeather/RIHS01_abc';
      const result = registry.tryDecodeForTopic(topic, encoded);
      expect(result).not.toBeNull();
      expect(result!.typeName).toBe('bubbaloop.weather.v1.CurrentWeather');
      expect((result!.data as Record<string, number>).temperature).toBeCloseTo(23.5);
    });

    it('falls back to brute force when no schema hint', () => {
      const msgType = registry.lookupType('test.v1.TestMessage')!;
      const encoded = msgType.encode({ name: 'test', value: 99 }).finish();
      // No schema hint in topic — will try guessTypeForTopic then brute force
      const result = registry.tryDecodeForTopic('some/random/topic', encoded);
      expect(result).not.toBeNull();
      expect(result!.data).toEqual({ name: 'test', value: 99 });
    });

    it('does not crash on garbage payload', () => {
      const garbage = new Uint8Array(Array.from({ length: 50 }, () => Math.floor(Math.random() * 256)));
      // Should not throw — returns null or a (possibly wrong) decode
      expect(() => registry.tryDecodeForTopic('some/topic', garbage)).not.toThrow();
    });
  });

  describe('guessTypeForTopic', () => {
    it('guesses weather type from weather topic', () => {
      const guess = registry.guessTypeForTopic('bubbaloop/local/m1/weather/current');
      expect(guess).toBe('bubbaloop.weather.v1.CurrentWeather');
    });

    it('returns null for unrecognized topic', () => {
      const guess = registry.guessTypeForTopic('bubbaloop/local/m1/unknown-thing/data');
      expect(guess).toBeNull();
    });

    it('caches results for same topic', () => {
      const g1 = registry.guessTypeForTopic('bubbaloop/local/m1/weather/current');
      const g2 = registry.guessTypeForTopic('bubbaloop/local/m1/weather/current');
      expect(g1).toBe(g2);
    });
  });

  describe('lookupType', () => {
    it('finds known types', () => {
      expect(registry.lookupType('test.v1.TestMessage')).not.toBeNull();
      expect(registry.lookupType('bubbaloop.weather.v1.CurrentWeather')).not.toBeNull();
    });

    it('returns null for unknown types', () => {
      expect(registry.lookupType('no.such.Type')).toBeNull();
    });
  });

  describe('getTypeNames', () => {
    it('lists all loaded types', () => {
      const types = registry.getTypeNames();
      expect(types).toContain('test.v1.TestMessage');
      expect(types).toContain('bubbaloop.weather.v1.CurrentWeather');
    });
  });

  describe('retry logic', () => {
    it('skips re-query for succeeded prefixes', async () => {
      // eslint-disable-next-line @typescript-eslint/no-explicit-any
      (registry as any).succeededPrefixes.add('bubbaloop/local/m1/mynode');

      const mockSession = { get: vi.fn() };
      // eslint-disable-next-line @typescript-eslint/no-explicit-any
      const result = await registry.fetchNodeSchema(mockSession as any, 'bubbaloop/local/m1/mynode');
      expect(result).toBe(false);
      expect(mockSession.get).not.toHaveBeenCalled();
    });

    it('retries failed prefixes after cooldown', async () => {
      // Set a failed prefix with old timestamp (well past cooldown)
      // eslint-disable-next-line @typescript-eslint/no-explicit-any
      (registry as any).failedPrefixes.set('bubbaloop/local/m1/mynode', Date.now() - 60_000);

      // Mock session.get to return empty (no responses)
      const mockReceiver = { [Symbol.asyncIterator]: () => ({ next: async () => ({ done: true, value: undefined }) }) };
      const mockSession = { get: vi.fn().mockResolvedValue(mockReceiver) };
      // eslint-disable-next-line @typescript-eslint/no-explicit-any
      await registry.fetchNodeSchema(mockSession as any, 'bubbaloop/local/m1/mynode');
      // Should have attempted the query since cooldown elapsed
      expect(mockSession.get).toHaveBeenCalled();
    });

    it('skips failed prefixes within cooldown', async () => {
      // Set a recent failure (within cooldown)
      // eslint-disable-next-line @typescript-eslint/no-explicit-any
      (registry as any).failedPrefixes.set('bubbaloop/local/m1/mynode', Date.now());

      const mockSession = { get: vi.fn() };
      // eslint-disable-next-line @typescript-eslint/no-explicit-any
      const result = await registry.fetchNodeSchema(mockSession as any, 'bubbaloop/local/m1/mynode');
      expect(result).toBe(false);
      expect(mockSession.get).not.toHaveBeenCalled();
    });
  });

  describe('clear', () => {
    it('resets all state', () => {
      expect(registry.getTypeNames().length).toBeGreaterThan(0);
      registry.clear();
      expect(registry.getTypeNames()).toEqual([]);
    });
  });

  describe('fetchCoreSchemas endpoint construction', () => {
    it('queries machine-scoped endpoints when machineIds provided', async () => {
      const queriedEndpoints: string[] = [];
      const mockReceiver = { [Symbol.asyncIterator]: () => ({ next: async () => ({ done: true, value: undefined }) }) };
      const mockSession = {
        get: vi.fn().mockImplementation((endpoint: string) => {
          queriedEndpoints.push(endpoint);
          return Promise.resolve(mockReceiver);
        }),
      };

      // eslint-disable-next-line @typescript-eslint/no-explicit-any
      await registry.fetchCoreSchemas(mockSession as any, ['m1', 'm2']);

      expect(queriedEndpoints).toContain('bubbaloop/m1/daemon/api/schemas');
      expect(queriedEndpoints).toContain('bubbaloop/m2/daemon/api/schemas');
      expect(queriedEndpoints).toHaveLength(2);
    });

    it('queries legacy endpoint when no machineIds', async () => {
      const queriedEndpoints: string[] = [];
      const mockReceiver = { [Symbol.asyncIterator]: () => ({ next: async () => ({ done: true, value: undefined }) }) };
      const mockSession = {
        get: vi.fn().mockImplementation((endpoint: string) => {
          queriedEndpoints.push(endpoint);
          return Promise.resolve(mockReceiver);
        }),
      };

      // eslint-disable-next-line @typescript-eslint/no-explicit-any
      await registry.fetchCoreSchemas(mockSession as any);

      expect(queriedEndpoints).toEqual(['bubbaloop/daemon/api/schemas']);
    });

    it('queries legacy endpoint when machineIds is empty array', async () => {
      const queriedEndpoints: string[] = [];
      const mockReceiver = { [Symbol.asyncIterator]: () => ({ next: async () => ({ done: true, value: undefined }) }) };
      const mockSession = {
        get: vi.fn().mockImplementation((endpoint: string) => {
          queriedEndpoints.push(endpoint);
          return Promise.resolve(mockReceiver);
        }),
      };

      // eslint-disable-next-line @typescript-eslint/no-explicit-any
      await registry.fetchCoreSchemas(mockSession as any, []);

      expect(queriedEndpoints).toEqual(['bubbaloop/daemon/api/schemas']);
    });
  });
});
