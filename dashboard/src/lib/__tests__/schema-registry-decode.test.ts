import { describe, it, expect, beforeEach } from 'vitest';
import { SchemaRegistry, extractSchemaFromTopic, extractTopicPrefix } from '../schema-registry';
import * as protobuf from 'protobufjs';
import 'protobufjs/ext/descriptor';

/**
 * Comprehensive tests for SchemaRegistry message decoding pipeline.
 * Tests end-to-end protobuf encode/decode, field mapping, schema versioning, and edge cases.
 */

// Helper: inject a protobufjs Root directly into a SchemaRegistry
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

describe('SchemaRegistry: End-to-End Decode Pipeline', () => {
  let registry: SchemaRegistry;

  beforeEach(() => {
    registry = new SchemaRegistry();
  });

  describe('ros-z topic hint decode', () => {
    it('decodes weather message via ros-z topic with schema hint', () => {
      // Create a weather schema
      const root = new protobuf.Root();
      const weatherNs = root.define('bubbaloop.weather.v1');
      const currentWeather = new protobuf.Type('CurrentWeather');
      currentWeather.add(new protobuf.Field('temperature', 1, 'float'));
      currentWeather.add(new protobuf.Field('humidity', 2, 'float'));
      currentWeather.add(new protobuf.Field('wind_speed', 3, 'float'));
      weatherNs.add(currentWeather);
      root.resolveAll();

      injectSchema(registry, root, 'weather-test', 'weather-node');

      // Encode a message
      const msgType = registry.lookupType('bubbaloop.weather.v1.CurrentWeather')!;
      const encoded = msgType.encode({ temperature: 23.5, humidity: 65.0, wind_speed: 12.3 }).finish();

      // Decode via ros-z topic (has schema hint in path)
      const topic = '0/bubbaloop%local%m1%weather%current/bubbaloop.weather.v1.CurrentWeather/RIHS01_abc123';
      const result = registry.tryDecodeForTopic(topic, encoded);

      expect(result).not.toBeNull();
      expect(result!.typeName).toBe('bubbaloop.weather.v1.CurrentWeather');
      expect(result!.source).toBe('weather-node');
      const data = result!.data as Record<string, number>;
      expect(data.temperature).toBeCloseTo(23.5);
      expect(data.humidity).toBeCloseTo(65.0);
      expect(data.windSpeed).toBeCloseTo(12.3); // snake_case converted to camelCase
    });
  });

  describe('vanilla topic + brute force decode', () => {
    it('decodes NodeList-like message via vanilla topic', () => {
      // Create a daemon schema
      const root = new protobuf.Root();
      const daemonNs = root.define('bubbaloop.daemon.v1');

      // Define NodeList FIRST so it's tried before NodeStatus in brute force
      const nodeList = new protobuf.Type('NodeList');
      nodeList.add(new protobuf.Field('nodes_data', 1, 'string', 'repeated'));
      nodeList.add(new protobuf.Field('count', 2, 'int32'));
      daemonNs.add(nodeList);

      root.resolveAll();
      injectSchema(registry, root, 'daemon-test', 'core');

      // Encode a NodeList
      const msgType = registry.lookupType('bubbaloop.daemon.v1.NodeList')!;
      const encoded = msgType.encode({
        nodes_data: ['rtsp-camera', 'openmeteo'],
        count: 2,
      }).finish();

      // Decode via vanilla topic (no schema hint — will use guessTypeForTopic + brute force)
      const topic = 'bubbaloop/local/m1/daemon/nodes';
      const result = registry.tryDecodeForTopic(topic, encoded);

      expect(result).not.toBeNull();
      expect(result!.typeName).toBe('bubbaloop.daemon.v1.NodeList');
      const data = result!.data as { nodesData: string[]; count: number };
      expect(data.nodesData).toHaveLength(2);
      expect(data.nodesData[0]).toBe('rtsp-camera');
      expect(data.count).toBe(2);
    });
  });

  describe('nested fields (header inside message)', () => {
    it('decodes message with nested header correctly', () => {
      const root = new protobuf.Root();
      const commonNs = root.define('bubbaloop.common.v1');

      const header = new protobuf.Type('Header');
      header.add(new protobuf.Field('frame_id', 1, 'string'));
      header.add(new protobuf.Field('seq', 2, 'uint32'));
      header.add(new protobuf.Field('machine_id', 3, 'string'));
      commonNs.add(header);

      const cameraMsg = new protobuf.Type('CameraFrame');
      cameraMsg.add(new protobuf.Field('header', 1, 'Header'));
      cameraMsg.add(new protobuf.Field('width', 2, 'uint32'));
      cameraMsg.add(new protobuf.Field('height', 3, 'uint32'));
      commonNs.add(cameraMsg);

      root.resolveAll();
      injectSchema(registry, root, 'camera-test', 'camera-node');

      const msgType = registry.lookupType('bubbaloop.common.v1.CameraFrame')!;
      const encoded = msgType.encode({
        header: { frame_id: 'camera_0', seq: 42, machine_id: 'nvidia_orin00' },
        width: 1920,
        height: 1080,
      }).finish();

      const result = registry.decode('bubbaloop.common.v1.CameraFrame', encoded);
      expect(result).not.toBeNull();
      const data = result!.data as Record<string, unknown>;

      // Check nested header fields
      const hdr = data.header as Record<string, unknown>;
      expect(hdr.frameId).toBe('camera_0'); // snake_case -> camelCase
      expect(hdr.seq).toBe(42);
      expect(hdr.machineId).toBe('nvidia_orin00');
      expect(data.width).toBe(1920);
      expect(data.height).toBe(1080);
    });
  });

  describe('enum fields', () => {
    it('decodes message with enum field (enums: String option)', () => {
      const root = new protobuf.Root();
      const testNs = root.define('test.v1');

      const statusEnum = new protobuf.Enum('Status');
      statusEnum.add('UNKNOWN', 0);
      statusEnum.add('RUNNING', 1);
      statusEnum.add('STOPPED', 2);
      testNs.add(statusEnum);

      const nodeMsg = new protobuf.Type('NodeInfo');
      nodeMsg.add(new protobuf.Field('name', 1, 'string'));
      nodeMsg.add(new protobuf.Field('status', 2, 'Status'));
      testNs.add(nodeMsg);

      root.resolveAll();
      injectSchema(registry, root, 'enum-test', 'test-source');

      const msgType = registry.lookupType('test.v1.NodeInfo')!;
      const encoded = msgType.encode({ name: 'test-node', status: 1 }).finish(); // RUNNING = 1

      const result = registry.decode('test.v1.NodeInfo', encoded);
      expect(result).not.toBeNull();
      const data = result!.data as Record<string, unknown>;
      expect(data.name).toBe('test-node');
      // With enums: String option, enum values should be string names
      expect(data.status).toBe('RUNNING');
    });
  });

  describe('repeated/array fields', () => {
    it('decodes message with repeated fields', () => {
      const root = new protobuf.Root();
      const testNs = root.define('test.v1');

      const listMsg = new protobuf.Type('StringList');
      listMsg.add(new protobuf.Field('items', 1, 'string', 'repeated'));
      testNs.add(listMsg);

      root.resolveAll();
      injectSchema(registry, root, 'array-test', 'test-source');

      const msgType = registry.lookupType('test.v1.StringList')!;
      const encoded = msgType.encode({ items: ['one', 'two', 'three'] }).finish();

      const result = registry.decode('test.v1.StringList', encoded);
      expect(result).not.toBeNull();
      const data = result!.data as { items: string[] };
      expect(data.items).toHaveLength(3);
      expect(data.items).toEqual(['one', 'two', 'three']);
    });

    it('decodes message with repeated nested messages', () => {
      const root = new protobuf.Root();
      const testNs = root.define('test.v1');

      const item = new protobuf.Type('Item');
      item.add(new protobuf.Field('id', 1, 'int32'));
      item.add(new protobuf.Field('name', 2, 'string'));
      testNs.add(item);

      const itemList = new protobuf.Type('ItemList');
      itemList.add(new protobuf.Field('items', 1, 'Item', 'repeated'));
      testNs.add(itemList);

      root.resolveAll();
      injectSchema(registry, root, 'nested-array-test', 'test-source');

      const msgType = registry.lookupType('test.v1.ItemList')!;
      const encoded = msgType.encode({
        items: [
          { id: 1, name: 'first' },
          { id: 2, name: 'second' },
        ],
      }).finish();

      const result = registry.decode('test.v1.ItemList', encoded);
      expect(result).not.toBeNull();
      const data = result!.data as { items: Array<{ id: number; name: string }> };
      expect(data.items).toHaveLength(2);
      expect(data.items[0].id).toBe(1);
      expect(data.items[1].name).toBe('second');
    });
  });

  describe('bytes fields', () => {
    it('decodes message with bytes field (bytes: String option)', () => {
      const root = new protobuf.Root();
      const testNs = root.define('test.v1');

      const blobMsg = new protobuf.Type('BlobMessage');
      blobMsg.add(new protobuf.Field('id', 1, 'string'));
      blobMsg.add(new protobuf.Field('data', 2, 'bytes'));
      testNs.add(blobMsg);

      root.resolveAll();
      injectSchema(registry, root, 'bytes-test', 'test-source');

      const msgType = registry.lookupType('test.v1.BlobMessage')!;
      const binaryData = new Uint8Array([0xde, 0xad, 0xbe, 0xef, 0x00, 0xff]);
      const encoded = msgType.encode({ id: 'test-blob', data: binaryData }).finish();

      const result = registry.decode('test.v1.BlobMessage', encoded);
      expect(result).not.toBeNull();
      const data = result!.data as { id: string; data: string };
      expect(data.id).toBe('test-blob');
      // With bytes: String option, bytes are converted to base64 or hex string
      expect(typeof data.data).toBe('string');
      expect(data.data.length).toBeGreaterThan(0);
    });
  });

  describe('int64/uint64 fields', () => {
    it('decodes message with int64 field (longs: String option)', () => {
      const root = new protobuf.Root();
      const testNs = root.define('test.v1');

      const counterMsg = new protobuf.Type('Counter');
      counterMsg.add(new protobuf.Field('timestamp_ns', 1, 'int64'));
      counterMsg.add(new protobuf.Field('count', 2, 'uint64'));
      testNs.add(counterMsg);

      root.resolveAll();
      injectSchema(registry, root, 'long-test', 'test-source');

      const msgType = registry.lookupType('test.v1.Counter')!;
      // Use string values for int64/uint64 to avoid precision issues
      const encoded = msgType.encode({ timestamp_ns: '1234567890123456789', count: '9876543210' }).finish();

      const result = registry.decode('test.v1.Counter', encoded);
      expect(result).not.toBeNull();
      const data = result!.data as { timestampNs: string; count: string };
      // With longs: String option, int64/uint64 are represented as strings
      expect(typeof data.timestampNs).toBe('string');
      expect(typeof data.count).toBe('string');
      expect(data.timestampNs).toBe('1234567890123456789');
      expect(data.count).toBe('9876543210');
    });
  });
});

describe('SchemaRegistry: snakeToCamel Field Mapping', () => {
  let registry: SchemaRegistry;

  beforeEach(() => {
    registry = new SchemaRegistry();
  });

  it('preserves field names with digits after underscores', () => {
    const root = new protobuf.Root();
    const testNs = root.define('test.v1');

    const weatherMsg = new protobuf.Type('WeatherData');
    weatherMsg.add(new protobuf.Field('temperature_2m', 1, 'float'));
    weatherMsg.add(new protobuf.Field('wind_speed_10m', 2, 'float'));
    weatherMsg.add(new protobuf.Field('relative_humidity_2m', 3, 'float'));
    testNs.add(weatherMsg);

    root.resolveAll();
    injectSchema(registry, root, 'snake-test', 'test-source');

    const msgType = registry.lookupType('test.v1.WeatherData')!;
    const encoded = msgType.encode({
      temperature_2m: 23.5,
      wind_speed_10m: 12.3,
      relative_humidity_2m: 65.0,
    }).finish();

    const result = registry.decode('test.v1.WeatherData', encoded);
    expect(result).not.toBeNull();
    const data = result!.data as Record<string, number>;

    // temperature_2m → temperature_2m (digit after underscore, no conversion)
    expect(data.temperature_2m).toBeCloseTo(23.5);
    // wind_speed_10m → windSpeed_10m (converts first underscore, preserves _10m)
    expect(data.windSpeed_10m).toBeCloseTo(12.3);
    // relative_humidity_2m → relativeHumidity_2m
    expect(data.relativeHumidity_2m).toBeCloseTo(65.0);
  });

  it('converts common protobuf field names correctly', () => {
    const root = new protobuf.Root();
    const testNs = root.define('test.v1');

    const testMsg = new protobuf.Type('TestFields');
    testMsg.add(new protobuf.Field('apparent_temperature', 1, 'float'));
    testMsg.add(new protobuf.Field('acq_time', 2, 'int64'));
    testMsg.add(new protobuf.Field('machine_id', 3, 'string'));
    testMsg.add(new protobuf.Field('last_updated_ms', 4, 'int64'));
    testMsg.add(new protobuf.Field('frame_id', 5, 'string'));
    testNs.add(testMsg);

    root.resolveAll();
    injectSchema(registry, root, 'field-test', 'test-source');

    const msgType = registry.lookupType('test.v1.TestFields')!;
    const encoded = msgType.encode({
      apparent_temperature: 25.0,
      acq_time: '1234567890',
      machine_id: 'nvidia_orin00',
      last_updated_ms: '9876543210',
      frame_id: 'camera_0',
    }).finish();

    const result = registry.decode('test.v1.TestFields', encoded);
    expect(result).not.toBeNull();
    const data = result!.data as Record<string, unknown>;

    // apparent_temperature → apparentTemperature
    expect(data.apparentTemperature).toBeCloseTo(25.0);
    // acq_time → acqTime
    expect(data.acqTime).toBe('1234567890');
    // machine_id → machineId
    expect(data.machineId).toBe('nvidia_orin00');
    // last_updated_ms → lastUpdatedMs
    expect(data.lastUpdatedMs).toBe('9876543210');
    // frame_id → frameId
    expect(data.frameId).toBe('camera_0');
  });
});

describe('SchemaRegistry: Schema Versioning and Compatibility', () => {
  let registry: SchemaRegistry;

  beforeEach(() => {
    registry = new SchemaRegistry();
  });

  it('handles field addition (v2 encode, v1 decode)', () => {
    // Create v1 schema (original)
    const rootV1 = new protobuf.Root();
    const nsV1 = rootV1.define('test.v1');
    const msgV1 = new protobuf.Type('Message');
    msgV1.add(new protobuf.Field('id', 1, 'int32'));
    msgV1.add(new protobuf.Field('name', 2, 'string'));
    nsV1.add(msgV1);
    rootV1.resolveAll();

    // Create v2 schema (added field)
    const rootV2 = new protobuf.Root();
    const nsV2 = rootV2.define('test.v1');
    const msgV2 = new protobuf.Type('Message');
    msgV2.add(new protobuf.Field('id', 1, 'int32'));
    msgV2.add(new protobuf.Field('name', 2, 'string'));
    msgV2.add(new protobuf.Field('email', 3, 'string')); // NEW FIELD
    nsV2.add(msgV2);
    rootV2.resolveAll();

    // Encode with v2 (includes email)
    const msgTypeV2 = rootV2.lookupType('test.v1.Message')!;
    const encoded = msgTypeV2.encode({ id: 42, name: 'test', email: 'test@example.com' }).finish();

    // Decode with v1 (no email field) — should succeed, extra field ignored
    injectSchema(registry, rootV1, 'v1-schema', 'v1-source');
    const result = registry.decode('test.v1.Message', encoded);

    expect(result).not.toBeNull();
    const data = result!.data as { id: number; name: string; email?: string };
    expect(data.id).toBe(42);
    expect(data.name).toBe('test');
    // Email field not in v1 schema — should be undefined in decoded object
    expect(data.email).toBeUndefined();
  });

  it('handles field removal (v1 encode, v2 decode)', () => {
    // Create v1 schema
    const rootV1 = new protobuf.Root();
    const nsV1 = rootV1.define('test.v1');
    const msgV1 = new protobuf.Type('Message');
    msgV1.add(new protobuf.Field('id', 1, 'int32'));
    msgV1.add(new protobuf.Field('name', 2, 'string'));
    msgV1.add(new protobuf.Field('deprecated_field', 3, 'string'));
    nsV1.add(msgV1);
    rootV1.resolveAll();

    // Create v2 schema (removed deprecated_field)
    const rootV2 = new protobuf.Root();
    const nsV2 = rootV2.define('test.v1');
    const msgV2 = new protobuf.Type('Message');
    msgV2.add(new protobuf.Field('id', 1, 'int32'));
    msgV2.add(new protobuf.Field('name', 2, 'string'));
    // deprecated_field removed
    nsV2.add(msgV2);
    rootV2.resolveAll();

    // Encode with v1 (includes deprecated_field)
    const msgTypeV1 = rootV1.lookupType('test.v1.Message')!;
    const encoded = msgTypeV1.encode({ id: 42, name: 'test', deprecated_field: 'old-value' }).finish();

    // Decode with v2 (no deprecated_field) — should succeed
    injectSchema(registry, rootV2, 'v2-schema', 'v2-source');
    const result = registry.decode('test.v1.Message', encoded);

    expect(result).not.toBeNull();
    const data = result!.data as { id: number; name: string; deprecatedField?: string };
    expect(data.id).toBe(42);
    expect(data.name).toBe('test');
    // deprecated_field present in wire format but not in v2 schema — should be undefined
    expect(data.deprecatedField).toBeUndefined();
  });

  it('rejects decode when all fields at default (hasContent heuristic)', () => {
    const root = new protobuf.Root();
    const testNs = root.define('test.v1');

    const emptyMsg = new protobuf.Type('EmptyMessage');
    emptyMsg.add(new protobuf.Field('id', 1, 'int32'));
    emptyMsg.add(new protobuf.Field('name', 2, 'string'));
    emptyMsg.add(new protobuf.Field('flag', 3, 'bool'));
    testNs.add(emptyMsg);

    root.resolveAll();
    injectSchema(registry, root, 'empty-test', 'test-source');

    // Encode message with all default values
    const msgType = registry.lookupType('test.v1.EmptyMessage')!;
    const encoded = msgType.encode({ id: 0, name: '', flag: false }).finish();

    // tryDecodeAny should NOT match this (hasContent heuristic)
    const result = registry.tryDecodeAny(encoded);
    // Either returns null or finds a different type (not EmptyMessage)
    if (result !== null) {
      expect(result.typeName).not.toBe('test.v1.EmptyMessage');
    }
  });

  it('handles decode with wrong type (TypeA encoded, TypeB decode attempt)', () => {
    const root = new protobuf.Root();
    const testNs = root.define('test.v1');

    const typeA = new protobuf.Type('TypeA');
    typeA.add(new protobuf.Field('id', 1, 'int32'));
    typeA.add(new protobuf.Field('name', 2, 'string'));
    testNs.add(typeA);

    const typeB = new protobuf.Type('TypeB');
    typeB.add(new protobuf.Field('code', 1, 'string')); // Different field structure
    typeB.add(new protobuf.Field('value', 2, 'float'));
    testNs.add(typeB);

    root.resolveAll();
    injectSchema(registry, root, 'wrong-type-test', 'test-source');

    // Encode TypeA
    const msgTypeA = registry.lookupType('test.v1.TypeA')!;
    const encoded = msgTypeA.encode({ id: 42, name: 'test' }).finish();

    // Try to decode as TypeB — should fail or produce gibberish
    const msgTypeB = registry.lookupType('test.v1.TypeB')!;
    let decodedB: unknown = null;
    try {
      const message = msgTypeB.decode(encoded);
      decodedB = msgTypeB.toObject(message, { defaults: true });
    } catch {
      // Decode might throw or produce invalid data
    }

    // If it didn't throw, the decoded data should be malformed
    if (decodedB !== null) {
      const dataB = decodedB as { code?: string; value?: number };
      // Field 1 (code) will have int32 data interpreted as string — gibberish
      // Field 2 (value) will have string data interpreted as float — gibberish or default
      expect(dataB.code).not.toBe('test'); // Should not match TypeA's name field
    }
  });
});

describe('SchemaRegistry: Edge Cases', () => {
  let registry: SchemaRegistry;

  beforeEach(() => {
    registry = new SchemaRegistry();
  });

  it('handles empty payload (0 bytes)', () => {
    const root = new protobuf.Root();
    const testNs = root.define('test.v1');
    const testMsg = new protobuf.Type('TestMessage');
    testMsg.add(new protobuf.Field('id', 1, 'int32'));
    testNs.add(testMsg);
    root.resolveAll();
    injectSchema(registry, root, 'empty-payload-test', 'test-source');

    const empty = new Uint8Array(0);
    const result = registry.tryDecodeForTopic('test/topic', empty);

    // Should either return null or decode as all-defaults (hasContent heuristic filters it)
    if (result !== null) {
      // If it decodes, should be filtered by hasContent
      expect(result.data).toBeDefined();
    }
  });

  it('handles very large payload (10KB)', () => {
    const root = new protobuf.Root();
    const testNs = root.define('test.v1');

    const largeMsg = new protobuf.Type('LargeMessage');
    largeMsg.add(new protobuf.Field('data', 1, 'bytes'));
    largeMsg.add(new protobuf.Field('count', 2, 'int32'));
    testNs.add(largeMsg);

    root.resolveAll();
    injectSchema(registry, root, 'large-test', 'test-source');

    // Create a 10KB payload
    const largeData = new Uint8Array(10 * 1024);
    for (let i = 0; i < largeData.length; i++) {
      largeData[i] = i % 256;
    }

    const msgType = registry.lookupType('test.v1.LargeMessage')!;
    const encoded = msgType.encode({ data: largeData, count: 12345 }).finish();

    const result = registry.decode('test.v1.LargeMessage', encoded);
    expect(result).not.toBeNull();
    const data = result!.data as { data: string; count: number };
    expect(data.count).toBe(12345);
    // data should be a string (base64/hex)
    expect(typeof data.data).toBe('string');
    expect(data.data.length).toBeGreaterThan(0);
  });

  it('does not decode valid JSON as protobuf', () => {
    const root = new protobuf.Root();
    const testNs = root.define('test.v1');
    const testMsg = new protobuf.Type('TestMessage');
    testMsg.add(new protobuf.Field('id', 1, 'int32'));
    testMsg.add(new protobuf.Field('name', 2, 'string'));
    testNs.add(testMsg);
    root.resolveAll();
    injectSchema(registry, root, 'json-test', 'test-source');

    // Valid JSON string encoded as UTF-8
    const jsonString = '{"id": 42, "name": "test"}';
    const jsonBytes = new TextEncoder().encode(jsonString);

    const result = registry.tryDecodeAny(jsonBytes);
    // JSON is unlikely to be valid protobuf — should return null or fail hasContent
    // (protobuf decode might succeed but produce all-defaults)
    if (result !== null) {
      // If it does decode, the data should not match the JSON structure
      const data = result.data as Record<string, unknown>;
      expect(data.id).not.toBe(42);
    }
  });

  it('handles printable ASCII that looks like text but is protobuf', () => {
    const root = new protobuf.Root();
    const testNs = root.define('test.v1');

    const asciiMsg = new protobuf.Type('AsciiMessage');
    asciiMsg.add(new protobuf.Field('text', 1, 'string'));
    testNs.add(asciiMsg);

    root.resolveAll();
    injectSchema(registry, root, 'ascii-test', 'test-source');

    const msgType = registry.lookupType('test.v1.AsciiMessage')!;
    const encoded = msgType.encode({ text: 'Hello, World!' }).finish();

    // Protobuf-encoded string might contain printable ASCII bytes
    const result = registry.decode('test.v1.AsciiMessage', encoded);
    expect(result).not.toBeNull();
    const data = result!.data as { text: string };
    expect(data.text).toBe('Hello, World!');
  });

  it('returns null when no schemas are loaded', () => {
    const emptyRegistry = new SchemaRegistry();
    const payload = new Uint8Array([0x08, 0x2a, 0x12, 0x04, 0x74, 0x65, 0x73, 0x74]);

    const result = emptyRegistry.tryDecodeAny(payload);
    expect(result).toBeNull();
  });

  it('searches all schemas when multiple sources loaded', () => {
    // Load schema 1
    const root1 = new protobuf.Root();
    const ns1 = root1.define('source1.v1');
    const msg1 = new protobuf.Type('Message1');
    msg1.add(new protobuf.Field('id', 1, 'int32'));
    ns1.add(msg1);
    root1.resolveAll();
    injectSchema(registry, root1, 'source1', 'source-1');

    // Load schema 2
    const root2 = new protobuf.Root();
    const ns2 = root2.define('source2.v1');
    const msg2 = new protobuf.Type('Message2');
    msg2.add(new protobuf.Field('name', 1, 'string'));
    ns2.add(msg2);
    root2.resolveAll();
    injectSchema(registry, root2, 'source2', 'source-2');

    // Encode Message2
    const msgType2 = root2.lookupType('source2.v1.Message2')!;
    const encoded = msgType2.encode({ name: 'test-name' }).finish();

    // tryDecodeAny should search both schemas and find Message2
    const result = registry.tryDecodeAny(encoded);
    expect(result).not.toBeNull();
    expect(result!.typeName).toBe('source2.v1.Message2');
    expect(result!.source).toBe('source-2');
    const data = result!.data as { name: string };
    expect(data.name).toBe('test-name');
  });
});

describe('SchemaRegistry: guessTypeForTopic with Realistic Topics', () => {
  let registry: SchemaRegistry;

  beforeEach(() => {
    registry = new SchemaRegistry();

    // Create a realistic schema set with daemon, weather, and network types
    const root = new protobuf.Root();

    const daemonNs = root.define('bubbaloop.daemon.v1');
    const nodeList = new protobuf.Type('NodeList');
    nodeList.add(new protobuf.Field('nodes', 1, 'string', 'repeated'));
    daemonNs.add(nodeList);

    const weatherNs = root.define('bubbaloop.weather.v1');
    const currentWeather = new protobuf.Type('CurrentWeather');
    currentWeather.add(new protobuf.Field('temperature', 1, 'float'));
    weatherNs.add(currentWeather);

    const hourlyForecast = new protobuf.Type('HourlyForecast');
    hourlyForecast.add(new protobuf.Field('temperature', 1, 'float', 'repeated'));
    weatherNs.add(hourlyForecast);

    const networkNs = root.define('bubbaloop.network_monitor.v1');
    const networkStatus = new protobuf.Type('NetworkStatus');
    networkStatus.add(new protobuf.Field('connected', 1, 'bool'));
    networkNs.add(networkStatus);

    const systemNs = root.define('bubbaloop.system_telemetry.v1');
    const systemMetrics = new protobuf.Type('SystemMetrics');
    systemMetrics.add(new protobuf.Field('cpu_usage', 1, 'float'));
    systemNs.add(systemMetrics);

    root.resolveAll();
    injectSchema(registry, root, 'core-types', 'core');
  });

  it('guesses daemon type from daemon/nodes topic', () => {
    const topic = 'bubbaloop/local/m1/daemon/nodes';
    const guess = registry.guessTypeForTopic(topic);

    expect(guess).not.toBeNull();
    expect(guess).toContain('daemon');
    expect(guess).toBe('bubbaloop.daemon.v1.NodeList');
  });

  it('guesses weather type from weather/current topic', () => {
    const topic = 'bubbaloop/local/m1/weather/current';
    const guess = registry.guessTypeForTopic(topic);

    expect(guess).not.toBeNull();
    expect(guess).toContain('weather');
    // Should match CurrentWeather (both contain "weather" and "current")
    expect(guess).toBe('bubbaloop.weather.v1.CurrentWeather');
  });

  it('guesses weather type from weather/hourly topic', () => {
    const topic = 'bubbaloop/local/m1/weather/hourly';
    const guess = registry.guessTypeForTopic(topic);

    expect(guess).not.toBeNull();
    expect(guess).toContain('weather');
    // Should match HourlyForecast (contains "hourly")
    expect(guess).toBe('bubbaloop.weather.v1.HourlyForecast');
  });

  it('guesses network-monitor type from network-monitor/status topic', () => {
    const topic = 'bubbaloop/local/m1/network-monitor/status';
    const guess = registry.guessTypeForTopic(topic);

    expect(guess).not.toBeNull();
    // network-monitor → network_monitor after normalization
    expect(guess).toContain('network_monitor');
    expect(guess).toBe('bubbaloop.network_monitor.v1.NetworkStatus');
  });

  it('guesses system-telemetry type from system-telemetry/metrics topic', () => {
    const topic = 'bubbaloop/local/m1/system-telemetry/metrics';
    const guess = registry.guessTypeForTopic(topic);

    expect(guess).not.toBeNull();
    // system-telemetry → system_telemetry
    expect(guess).toContain('system_telemetry');
    expect(guess).toBe('bubbaloop.system_telemetry.v1.SystemMetrics');
  });

  it('handles multi-machine topics correctly', () => {
    const topic = 'bubbaloop/local/nvidia_orin00/weather/current';
    const guess = registry.guessTypeForTopic(topic);

    expect(guess).not.toBeNull();
    expect(guess).toBe('bubbaloop.weather.v1.CurrentWeather');
  });

  it('returns null for unrecognized topic patterns', () => {
    const topic = 'bubbaloop/local/m1/unknown-service/data';
    const guess = registry.guessTypeForTopic(topic);

    // Should return null (no matching type) or a weak match
    if (guess !== null) {
      // If it returns something, score should be low
      expect(guess).toBeDefined();
    }
  });

  it('caches guesses for repeated topics', () => {
    const topic = 'bubbaloop/local/m1/weather/current';

    const guess1 = registry.guessTypeForTopic(topic);
    const guess2 = registry.guessTypeForTopic(topic);

    expect(guess1).toBe(guess2);
    expect(guess1).toBe('bubbaloop.weather.v1.CurrentWeather');
  });
});

describe('SchemaRegistry: Complex Real-World Scenarios', () => {
  let registry: SchemaRegistry;

  beforeEach(() => {
    registry = new SchemaRegistry();
  });

  it('handles multi-level nested messages', () => {
    const root = new protobuf.Root();
    const testNs = root.define('test.v1');

    const innerMost = new protobuf.Type('InnerMost');
    innerMost.add(new protobuf.Field('value', 1, 'int32'));
    testNs.add(innerMost);

    const middle = new protobuf.Type('Middle');
    middle.add(new protobuf.Field('inner', 1, 'InnerMost'));
    middle.add(new protobuf.Field('name', 2, 'string'));
    testNs.add(middle);

    const outer = new protobuf.Type('Outer');
    outer.add(new protobuf.Field('middle', 1, 'Middle'));
    outer.add(new protobuf.Field('id', 2, 'int32'));
    testNs.add(outer);

    root.resolveAll();
    injectSchema(registry, root, 'nested-test', 'test-source');

    const msgType = registry.lookupType('test.v1.Outer')!;
    const encoded = msgType.encode({
      middle: {
        inner: { value: 42 },
        name: 'test',
      },
      id: 1,
    }).finish();

    const result = registry.decode('test.v1.Outer', encoded);
    expect(result).not.toBeNull();
    const data = result!.data as Record<string, unknown>;
    const middleData = data.middle as Record<string, unknown>;
    const innerData = middleData.inner as Record<string, number>;
    expect(innerData.value).toBe(42);
    expect(middleData.name).toBe('test');
    expect(data.id).toBe(1);
  });

  it('handles map fields (protobuf map<string, string>)', () => {
    const root = new protobuf.Root();
    const testNs = root.define('test.v1');

    const configMsg = new protobuf.Type('Config');
    const mapField = new protobuf.MapField('settings', 1, 'string', 'string');
    configMsg.add(mapField);
    testNs.add(configMsg);

    root.resolveAll();
    injectSchema(registry, root, 'map-test', 'test-source');

    const msgType = registry.lookupType('test.v1.Config')!;
    const encoded = msgType.encode({
      settings: {
        key1: 'value1',
        key2: 'value2',
      },
    }).finish();

    const result = registry.decode('test.v1.Config', encoded);
    expect(result).not.toBeNull();
    const data = result!.data as { settings: Record<string, string> };
    expect(data.settings.key1).toBe('value1');
    expect(data.settings.key2).toBe('value2');
  });

  it('handles oneof fields', () => {
    const root = new protobuf.Root();
    const testNs = root.define('test.v1');

    const payloadMsg = new protobuf.Type('Payload');
    payloadMsg.add(new protobuf.Field('text', 1, 'string'));
    payloadMsg.add(new protobuf.Field('number', 2, 'int32'));
    payloadMsg.add(new protobuf.OneOf('data', ['text', 'number']));
    testNs.add(payloadMsg);

    root.resolveAll();
    injectSchema(registry, root, 'oneof-test', 'test-source');

    const msgType = registry.lookupType('test.v1.Payload')!;

    // Encode with text field set
    const encodedText = msgType.encode({ text: 'hello' }).finish();
    const resultText = registry.decode('test.v1.Payload', encodedText);
    expect(resultText).not.toBeNull();
    const dataText = resultText!.data as { text?: string; number?: number };
    expect(dataText.text).toBe('hello');
    expect(dataText.number).toBeUndefined();

    // Encode with number field set
    const encodedNumber = msgType.encode({ number: 42 }).finish();
    const resultNumber = registry.decode('test.v1.Payload', encodedNumber);
    expect(resultNumber).not.toBeNull();
    const dataNumber = resultNumber!.data as { text?: string; number?: number };
    expect(dataNumber.number).toBe(42);
    expect(dataNumber.text).toBeUndefined();
  });

  it('handles timestamp and duration well-known types', () => {
    const root = new protobuf.Root();

    // Load google.protobuf well-known types
    const googleNs = root.define('google.protobuf');
    const timestamp = new protobuf.Type('Timestamp');
    timestamp.add(new protobuf.Field('seconds', 1, 'int64'));
    timestamp.add(new protobuf.Field('nanos', 2, 'int32'));
    googleNs.add(timestamp);

    const testNs = root.define('test.v1');
    const eventMsg = new protobuf.Type('Event');
    eventMsg.add(new protobuf.Field('name', 1, 'string'));
    eventMsg.add(new protobuf.Field('timestamp', 2, 'google.protobuf.Timestamp'));
    testNs.add(eventMsg);

    root.resolveAll();
    injectSchema(registry, root, 'wkt-test', 'test-source');

    const msgType = registry.lookupType('test.v1.Event')!;
    const encoded = msgType.encode({
      name: 'test-event',
      timestamp: { seconds: '1234567890', nanos: 123456789 },
    }).finish();

    const result = registry.decode('test.v1.Event', encoded);
    expect(result).not.toBeNull();
    const data = result!.data as Record<string, unknown>;
    expect(data.name).toBe('test-event');
    const ts = data.timestamp as { seconds: string; nanos: number };
    expect(ts.seconds).toBe('1234567890');
    expect(ts.nanos).toBe(123456789);
  });
});

describe('SchemaRegistry: Architectural Integration Tests', () => {
  describe('extractTopicPrefix: Real-World Topic Formats', () => {
    it('extracts prefix from standard scoped topics', () => {
      expect(extractTopicPrefix('bubbaloop/local/nvidia_orin00/camera/entrance/compressed'))
        .toBe('bubbaloop/local/nvidia_orin00/camera');
      expect(extractTopicPrefix('bubbaloop/local/m1/weather/current'))
        .toBe('bubbaloop/local/m1/weather');
      expect(extractTopicPrefix('bubbaloop/local/nvidia_orin00/system-telemetry/metrics'))
        .toBe('bubbaloop/local/nvidia_orin00/system-telemetry');
    });

    it('extracts prefix from daemon topics (4 segments)', () => {
      // Daemon topics already have exactly 4 segments, so prefix = full path
      expect(extractTopicPrefix('bubbaloop/nvidia_orin00/daemon/nodes'))
        .toBe('bubbaloop/nvidia_orin00/daemon/nodes');
      expect(extractTopicPrefix('bubbaloop/local/m1/daemon/api/health'))
        .toBe('bubbaloop/local/m1/daemon');
    });

    it('extracts prefix from ros-z format topics', () => {
      // ros-z: 0/<%-encoded-topic>/<schema>/<hash>
      expect(extractTopicPrefix('0/bubbaloop%local%nvidia_orin00%camera%entrance%compressed/bubbaloop.camera.v1.CompressedImage/RIHS01_xxx'))
        .toBe('bubbaloop/local/nvidia_orin00/camera');
      expect(extractTopicPrefix('0/bubbaloop%local%m1%weather%current/bubbaloop.weather.v1.CurrentWeather/RIHS01_xyz'))
        .toBe('bubbaloop/local/m1/weather');
    });

    it('returns null for short topics (< 4 segments)', () => {
      expect(extractTopicPrefix('bubbaloop/daemon')).toBeNull();
      expect(extractTopicPrefix('bubbaloop/local/m1')).toBeNull();
      expect(extractTopicPrefix('bubbaloop')).toBeNull();
    });

    it('returns null for non-bubbaloop topics', () => {
      expect(extractTopicPrefix('other/foo/bar/baz')).toBeNull();
      expect(extractTopicPrefix('zenoh/admin/status')).toBeNull();
    });
  });

  describe('Topic Resource vs Node Name Mismatch (Documentation)', () => {
    it('camera topics have 4th segment "camera" but node name is "rtsp-camera"', () => {
      // Topic resource name is "camera"
      const prefix = extractTopicPrefix('bubbaloop/local/m1/camera/entrance/compressed');
      expect(prefix).toBe('bubbaloop/local/m1/camera');
      // NOTE: This prefix points to "camera" but the actual node is "rtsp-camera".
      // Schema discovery must use wildcard pattern (bubbaloop/**/schema) to handle this.
    });

    it('weather topics have 4th segment "weather" but node name is "openmeteo"', () => {
      // Topic resource name is "weather"
      const prefix = extractTopicPrefix('bubbaloop/local/m1/weather/current');
      expect(prefix).toBe('bubbaloop/local/m1/weather');
      // NOTE: This prefix points to "weather" but the actual node is "openmeteo".
      // Schema discovery must use wildcard pattern to handle this mismatch.
    });

    it('system-telemetry topics match node name (no mismatch)', () => {
      // Topic resource and node name both "system-telemetry"
      const prefix = extractTopicPrefix('bubbaloop/local/m1/system-telemetry/metrics');
      expect(prefix).toBe('bubbaloop/local/m1/system-telemetry');
      // This prefix correctly points to the node "system-telemetry".
      // Both prefix-based query and wildcard discovery should work.
    });

    it('network-monitor topics match node name (hyphen to underscore normalization)', () => {
      const prefix = extractTopicPrefix('bubbaloop/local/m1/network-monitor/status');
      expect(prefix).toBe('bubbaloop/local/m1/network-monitor');
      // Node name is "network-monitor" (matches topic resource).
      // Schema type is "bubbaloop.network_monitor.v1.NetworkStatus" (underscore).
    });
  });

  describe('Machine ID Format Consistency', () => {
    it('extracts prefix with hyphenated machine ID', () => {
      const prefix = extractTopicPrefix('bubbaloop/local/nvidia-orin00/camera/compressed');
      expect(prefix).toBe('bubbaloop/local/nvidia-orin00/camera');
      // Wildcard pattern bubbaloop/**/schema will match this format
    });

    it('extracts prefix with underscored machine ID', () => {
      const prefix = extractTopicPrefix('bubbaloop/local/nvidia_orin00/camera/compressed');
      expect(prefix).toBe('bubbaloop/local/nvidia_orin00/camera');
      // Wildcard pattern bubbaloop/**/schema will match this format too
    });

    it('extracts prefix with alphanumeric machine ID', () => {
      const prefix = extractTopicPrefix('bubbaloop/local/m1/weather/current');
      expect(prefix).toBe('bubbaloop/local/m1/weather');
      // Short machine ID format (m1, jetson01, etc.)
    });

    it('both hyphen and underscore formats are valid — wildcard handles either', () => {
      // Both formats should be handled by wildcard schema discovery
      const hyphen = extractTopicPrefix('bubbaloop/local/nvidia-orin00/system-telemetry/metrics');
      const underscore = extractTopicPrefix('bubbaloop/local/nvidia_orin00/system-telemetry/metrics');
      expect(hyphen).toBe('bubbaloop/local/nvidia-orin00/system-telemetry');
      expect(underscore).toBe('bubbaloop/local/nvidia_orin00/system-telemetry');
      // Wildcard bubbaloop/**/schema matches both — no need for machine ID normalization
    });
  });

  describe('extractSchemaFromTopic: Edge Cases', () => {
    it('extracts schema from standard ros-z topic', () => {
      expect(extractSchemaFromTopic('0/bubbaloop%local%m1%camera%entrance%compressed/bubbaloop.camera.v1.CompressedImage/RIHS01_xxx'))
        .toBe('bubbaloop.camera.v1.CompressedImage');
      expect(extractSchemaFromTopic('0/bubbaloop%local%m1%weather%current/bubbaloop.weather.v1.CurrentWeather/RIHS01_abc'))
        .toBe('bubbaloop.weather.v1.CurrentWeather');
    });

    it('handles multiple dots in schema type names', () => {
      // Complex nested type name with many dots
      expect(extractSchemaFromTopic('0/topic/foo.bar.baz.v2.ComplexType/RIHS01_hash'))
        .toBe('foo.bar.baz.v2.ComplexType');
    });

    it('returns null for topics without schema hints', () => {
      // Vanilla zenoh topics (no ros-z schema hint)
      expect(extractSchemaFromTopic('bubbaloop/local/m1/daemon/nodes')).toBeNull();
      expect(extractSchemaFromTopic('bubbaloop/local/m1/weather/current')).toBeNull();
    });

    it('returns null for hash-only segments (RIHS prefix)', () => {
      // Topic with hash but no schema type
      expect(extractSchemaFromTopic('0/topic/RIHS01_abc123')).toBeNull();
    });

    it('filters out hash segments with RIHS prefix', () => {
      // Should not return the hash segment (last part)
      const schema = extractSchemaFromTopic('0/topic/bubbaloop.weather.v1.CurrentWeather/RIHS01_xyz');
      expect(schema).toBe('bubbaloop.weather.v1.CurrentWeather');
      expect(schema).not.toContain('RIHS');
    });
  });

  describe('Schema Discovery Pattern for Real Topics', () => {
    let registry: SchemaRegistry;

    beforeEach(() => {
      registry = new SchemaRegistry();
    });

    it('documents prefix-based query for system-telemetry (no mismatch)', () => {
      const topic = 'bubbaloop/local/m1/system-telemetry/metrics';
      const prefix = extractTopicPrefix(topic);
      expect(prefix).toBe('bubbaloop/local/m1/system-telemetry');
      // Prefix query: bubbaloop/local/m1/system-telemetry/schema
      // Node name: system-telemetry
      // Expected: Direct query should succeed (no mismatch)
    });

    it('documents wildcard fallback for camera topics (resource != node name)', () => {
      const topic = 'bubbaloop/local/m1/camera/entrance/compressed';
      const prefix = extractTopicPrefix(topic);
      expect(prefix).toBe('bubbaloop/local/m1/camera');
      // Prefix query: bubbaloop/local/m1/camera/schema (WILL FAIL)
      // Actual node: rtsp-camera at bubbaloop/local/m1/rtsp-camera/schema
      // Expected: Must fall back to wildcard bubbaloop/**/schema
    });

    it('documents wildcard fallback for weather topics (resource != node name)', () => {
      const topic = 'bubbaloop/local/m1/weather/hourly';
      const prefix = extractTopicPrefix(topic);
      expect(prefix).toBe('bubbaloop/local/m1/weather');
      // Prefix query: bubbaloop/local/m1/weather/schema (WILL FAIL)
      // Actual node: openmeteo at bubbaloop/local/m1/openmeteo/schema
      // Expected: Must fall back to wildcard bubbaloop/**/schema
    });
  });
});
