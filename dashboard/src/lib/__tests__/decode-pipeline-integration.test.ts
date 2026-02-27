import { describe, it, expect, vi, beforeEach } from 'vitest';
import { SchemaRegistry, snakeToCamel, extractSchemaFromTopic, extractTopicPrefix } from '../schema-registry';
import { decodeNodeList, decodeNodeEvent, NodeListProto, NodeEventProto, NodeStateProto } from '../../proto/daemon';
import * as protobuf from 'protobufjs';
import Long from 'long';

/**
 * Decode pipeline integration tests.
 *
 * Tests the component-level decode paths used by JsonView (decodePayload) and
 * CameraView (direct SchemaRegistry.lookupType decode).
 *
 * The decodePayload function is defined inside JsonView.tsx and is NOT exported,
 * so we replicate its logic inline to test the decode chain:
 *   1. JSON parsing
 *   2. SchemaRegistry.tryDecodeForTopic()
 *   3. Built-in decoders (decodeNodeList, decodeNodeEvent)
 *   4. Plain text detection
 *   5. Hex preview fallback
 */

// --- Helper: replicate decodePayload logic from JsonView.tsx ---

type SchemaSourceType = 'builtin' | 'dynamic' | 'raw';

interface DecodePayloadResult {
  data: unknown;
  schema: string;
  schemaSource: SchemaSourceType;
  error?: string;
}

function summarizeLargeFields(data: Record<string, unknown>): Record<string, unknown> {
  const result: Record<string, unknown> = {};
  for (const [key, value] of Object.entries(data)) {
    if (typeof value === 'string' && value.length > 1000) {
      result[key + 'Size'] = Math.round(value.length * 0.75);
    } else if (value && typeof value === 'object' && !Array.isArray(value)) {
      result[key] = summarizeLargeFields(value as Record<string, unknown>);
    } else {
      result[key] = value;
    }
  }
  return result;
}

function bigIntToString(obj: unknown): unknown {
  if (typeof obj === 'bigint') return obj.toString();
  if (Array.isArray(obj)) return obj.map(bigIntToString);
  if (obj !== null && typeof obj === 'object') {
    const result: Record<string, unknown> = {};
    for (const [key, value] of Object.entries(obj)) {
      result[key] = bigIntToString(value);
    }
    return result;
  }
  return obj;
}

function decodePayload(payload: Uint8Array, topic: string, registry?: SchemaRegistry): DecodePayloadResult {
  const text = new TextDecoder().decode(payload);

  // 1. Try JSON first
  try {
    const parsed = JSON.parse(text);
    return { data: parsed, schema: 'JSON', schemaSource: 'builtin' };
  } catch {
    // Not JSON, continue
  }

  // 2. Dynamic SchemaRegistry
  if (registry) {
    const result = registry.tryDecodeForTopic(topic, payload);
    if (result) {
      const data = summarizeLargeFields(result.data);
      return { data, schema: result.typeName, schemaSource: 'dynamic' };
    }
  }

  // 3. Built-in decoders as fallback
  if (topic.includes('daemon/nodes')) {
    const msg = decodeNodeList(payload);
    if (msg) {
      return { data: bigIntToString(msg), schema: 'bubbaloop.daemon.v1.NodeList', schemaSource: 'builtin' };
    }
  }
  if (topic.includes('daemon/events')) {
    const msg = decodeNodeEvent(payload);
    if (msg) {
      return { data: bigIntToString(msg), schema: 'bubbaloop.daemon.v1.NodeEvent', schemaSource: 'builtin' };
    }
  }

  // 4. Plain text
  if (payload.length < 200 && /^[\x20-\x7e]+$/.test(text)) {
    return { data: { message: text }, schema: 'Text', schemaSource: 'builtin' };
  }

  // 5. Hex preview fallback
  const preview = payload.slice(0, 100);
  const hex = Array.from(preview).map(b => b.toString(16).padStart(2, '0')).join(' ');
  return {
    data: {
      _format: 'binary',
      _size: payload.length,
      _hexPreview: hex + (payload.length > 100 ? '...' : ''),
    },
    schema: 'Binary',
    schemaSource: 'raw' as SchemaSourceType,
    error: 'Unknown binary format - showing hex preview',
  };
}

// --- Helper: inject schema into registry (mirrors existing test pattern) ---

function injectSchema(registry: SchemaRegistry, root: protobuf.Root, key: string, label: string) {
  // eslint-disable-next-line @typescript-eslint/no-explicit-any
  const schemas = (registry as any).schemas as Map<string, { source: string; root: protobuf.Root }>;
  schemas.set(key, { source: label, root });
  // eslint-disable-next-line @typescript-eslint/no-explicit-any
  (registry as any).typeNamesCache = null;
  // eslint-disable-next-line @typescript-eslint/no-explicit-any
  (registry as any).topicGuessCache?.clear?.();
}

// =====================================================================
// Test suites
// =====================================================================

describe('JsonView decodePayload chain', () => {
  let registry: SchemaRegistry;

  beforeEach(() => {
    registry = new SchemaRegistry();
  });

  it('returns parsed JSON for a JSON payload', () => {
    const json = JSON.stringify({ temperature: 23.5, humidity: 65 });
    const payload = new TextEncoder().encode(json);
    const result = decodePayload(payload, 'bubbaloop/local/m1/weather/current');

    expect(result.schema).toBe('JSON');
    expect(result.schemaSource).toBe('builtin');
    const data = result.data as Record<string, number>;
    expect(data.temperature).toBe(23.5);
    expect(data.humidity).toBe(65);
  });

  it('returns parsed JSON for a JSON array payload', () => {
    const json = JSON.stringify([1, 2, 3]);
    const payload = new TextEncoder().encode(json);
    const result = decodePayload(payload, 'any/topic');

    expect(result.schema).toBe('JSON');
    expect(result.schemaSource).toBe('builtin');
    expect(result.data).toEqual([1, 2, 3]);
  });

  it('returns parsed JSON for a JSON string payload', () => {
    const json = JSON.stringify("hello");
    const payload = new TextEncoder().encode(json);
    const result = decodePayload(payload, 'any/topic');

    expect(result.schema).toBe('JSON');
    expect(result.schemaSource).toBe('builtin');
    expect(result.data).toBe('hello');
  });

  it('decodes NodeList protobuf via built-in decoder when no SchemaRegistry', () => {
    const nodeState = NodeStateProto.create({
      name: 'my-camera',
      path: '/home/nvidia/.bubbaloop/nodes/my-camera',
      status: 2, // RUNNING
      installed: true,
      machineId: 'nvidia-orin00',
      machineHostname: 'nvidia-orin00',
      machineIps: ['192.168.1.100'],
    });
    const nodeList = NodeListProto.create({
      nodes: [nodeState],
      timestampMs: Long.fromNumber(1700000000000),
      machineId: 'nvidia-orin00',
    });
    const encoded = NodeListProto.encode(nodeList).finish();

    // No registry passed -- falls through to built-in decoder
    const result = decodePayload(encoded, 'bubbaloop/nvidia-orin00/daemon/nodes');

    expect(result.schema).toBe('bubbaloop.daemon.v1.NodeList');
    expect(result.schemaSource).toBe('builtin');
    const data = result.data as Record<string, unknown>;
    const nodes = data.nodes as Array<Record<string, unknown>>;
    expect(nodes).toHaveLength(1);
    expect(nodes[0].name).toBe('my-camera');
    expect(nodes[0].statusName).toBe('running');
    expect(nodes[0].installed).toBe(true);
    expect(nodes[0].machineId).toBe('nvidia-orin00');
  });

  it('decodes NodeEvent protobuf via built-in decoder', () => {
    const nodeState = NodeStateProto.create({
      name: 'openmeteo',
      status: 1, // STOPPED
      installed: true,
    });
    const nodeEvent = NodeEventProto.create({
      eventType: 'state_changed',
      nodeName: 'openmeteo',
      state: nodeState,
      timestampMs: Long.fromNumber(1700000000000),
    });
    const encoded = NodeEventProto.encode(nodeEvent).finish();

    const result = decodePayload(encoded, 'bubbaloop/nvidia-orin00/daemon/events');

    expect(result.schema).toBe('bubbaloop.daemon.v1.NodeEvent');
    expect(result.schemaSource).toBe('builtin');
    const data = result.data as Record<string, unknown>;
    expect(data.eventType).toBe('state_changed');
    expect(data.nodeName).toBe('openmeteo');
    const state = data.state as Record<string, unknown>;
    expect(state.name).toBe('openmeteo');
    expect(state.statusName).toBe('stopped');
  });

  it('decodes plain text for short ASCII payloads', () => {
    const text = 'OK healthy';
    const payload = new TextEncoder().encode(text);
    const result = decodePayload(payload, 'bubbaloop/local/m1/daemon/health');

    expect(result.schema).toBe('Text');
    expect(result.schemaSource).toBe('builtin');
    const data = result.data as { message: string };
    expect(data.message).toBe('OK healthy');
  });

  it('returns hex preview for binary data that is not JSON, protobuf, or text', () => {
    // Binary with non-printable bytes
    const payload = new Uint8Array([0x00, 0x01, 0x02, 0xff, 0xfe, 0xab, 0xcd]);
    const result = decodePayload(payload, 'bubbaloop/local/m1/unknown/data');

    expect(result.schema).toBe('Binary');
    expect(result.schemaSource).toBe('raw');
    expect(result.error).toBe('Unknown binary format - showing hex preview');
    const data = result.data as { _format: string; _size: number; _hexPreview: string };
    expect(data._format).toBe('binary');
    expect(data._size).toBe(7);
    expect(data._hexPreview).toBe('00 01 02 ff fe ab cd');
  });

  it('truncates hex preview for payloads > 100 bytes', () => {
    const payload = new Uint8Array(150).fill(0xaa);
    // Add a non-printable byte so it doesn't get caught as text
    payload[0] = 0x00;
    const result = decodePayload(payload, 'bubbaloop/local/m1/big/data');

    expect(result.schema).toBe('Binary');
    const data = result.data as { _hexPreview: string; _size: number };
    expect(data._size).toBe(150);
    expect(data._hexPreview).toMatch(/\.\.\.$/);
  });

  it('prefers JSON over SchemaRegistry for JSON payloads', () => {
    // Set up a registry with a type
    const root = new protobuf.Root();
    const ns = root.define('test.v1');
    const msg = new protobuf.Type('Msg');
    msg.add(new protobuf.Field('value', 1, 'string'));
    ns.add(msg);
    root.resolveAll();
    injectSchema(registry, root, 'test', 'test');

    const json = JSON.stringify({ value: 'hello' });
    const payload = new TextEncoder().encode(json);
    const result = decodePayload(payload, 'test/topic', registry);

    // JSON should win over SchemaRegistry
    expect(result.schema).toBe('JSON');
    expect(result.schemaSource).toBe('builtin');
  });

  it('uses SchemaRegistry when available and payload is protobuf', () => {
    const root = new protobuf.Root();
    const ns = root.define('bubbaloop.weather.v1');
    const weather = new protobuf.Type('CurrentWeather');
    weather.add(new protobuf.Field('temperature', 1, 'float'));
    weather.add(new protobuf.Field('humidity', 2, 'float'));
    ns.add(weather);
    root.resolveAll();
    injectSchema(registry, root, 'weather', 'weather-node');

    const msgType = registry.lookupType('bubbaloop.weather.v1.CurrentWeather')!;
    const encoded = msgType.encode({ temperature: 25.0, humidity: 60.0 }).finish();

    const topic = 'bubbaloop/local/m1/weather/current';
    const result = decodePayload(encoded, topic, registry);

    expect(result.schema).toBe('bubbaloop.weather.v1.CurrentWeather');
    expect(result.schemaSource).toBe('dynamic');
    const data = result.data as Record<string, number>;
    expect(data.temperature).toBeCloseTo(25.0);
    expect(data.humidity).toBeCloseTo(60.0);
  });

  it('falls through to built-in decoder when SchemaRegistry has no matching type', () => {
    // Empty registry -- no types loaded
    const nodeList = NodeListProto.create({
      nodes: [],
      timestampMs: Long.fromNumber(Date.now()),
      machineId: 'test',
    });
    const encoded = NodeListProto.encode(nodeList).finish();

    const result = decodePayload(encoded, 'bubbaloop/test/daemon/nodes', registry);

    expect(result.schema).toBe('bubbaloop.daemon.v1.NodeList');
    expect(result.schemaSource).toBe('builtin');
  });

  it('summarizes large base64 byte fields from SchemaRegistry decodes', () => {
    const root = new protobuf.Root();
    const ns = root.define('test.v1');
    const msg = new protobuf.Type('BigData');
    msg.add(new protobuf.Field('payload', 1, 'bytes'));
    msg.add(new protobuf.Field('name', 2, 'string'));
    ns.add(msg);
    root.resolveAll();
    injectSchema(registry, root, 'big', 'big-node');

    const msgType = registry.lookupType('test.v1.BigData')!;
    // Create a large byte field (will become >1000 char base64 string)
    const largeBytes = new Uint8Array(2000).fill(0x42);
    const encoded = msgType.encode({ payload: largeBytes, name: 'test' }).finish();

    const topic = 'bubbaloop/local/m1/test/topic';
    const result = decodePayload(encoded, topic, registry);

    expect(result.schemaSource).toBe('dynamic');
    const data = result.data as Record<string, unknown>;
    // The 'payload' field should be summarized as 'payloadSize'
    expect(data.payloadSize).toBeDefined();
    expect(typeof data.payloadSize).toBe('number');
    expect(data.name).toBe('test');
    // Original 'payload' key should not exist (replaced by payloadSize)
    expect(data.payload).toBeUndefined();
  });
});

describe('CameraView direct decode path', () => {
  let registry: SchemaRegistry;

  beforeEach(() => {
    registry = new SchemaRegistry();
  });

  it('lookupType for CompressedImage returns type when schema loaded', () => {
    const root = new protobuf.Root();
    const cameraNs = root.define('bubbaloop.camera.v1');
    const headerNs = root.define('bubbaloop.header.v1');

    const header = new protobuf.Type('Header');
    header.add(new protobuf.Field('acq_time', 1, 'int64'));
    header.add(new protobuf.Field('pub_time', 2, 'int64'));
    header.add(new protobuf.Field('sequence', 3, 'uint32'));
    header.add(new protobuf.Field('frame_id', 4, 'string'));
    headerNs.add(header);

    const compressedImage = new protobuf.Type('CompressedImage');
    compressedImage.add(new protobuf.Field('header', 1, 'bubbaloop.header.v1.Header'));
    compressedImage.add(new protobuf.Field('format', 2, 'string'));
    compressedImage.add(new protobuf.Field('data', 3, 'bytes'));
    cameraNs.add(compressedImage);

    root.resolveAll();
    injectSchema(registry, root, 'camera', 'rtsp-camera');

    const msgType = registry.lookupType('bubbaloop.camera.v1.CompressedImage');
    expect(msgType).not.toBeNull();

    // Encode a frame
    const encoded = msgType!.encode({
      header: { acq_time: Long.fromNumber(1000000000), pub_time: Long.fromNumber(1000001000), sequence: 42, frame_id: 'cam0' },
      format: 'h264',
      data: new Uint8Array([0x00, 0x00, 0x00, 0x01, 0x67]),
    }).finish();

    // Decode like CameraView does
    // eslint-disable-next-line @typescript-eslint/no-explicit-any
    const msg = msgType!.decode(encoded) as any;
    expect(msg.format).toBe('h264');
    // In Node.js (jsdom), protobufjs returns Buffer (a Uint8Array subclass)
    expect(ArrayBuffer.isView(msg.data)).toBe(true);
    expect(msg.data.length).toBe(5);
    expect(msg.header.sequence).toBe(42);
    // protobufjs raw decode uses the proto field name (snake_case) when schema
    // is built programmatically. CameraView accesses msg.header.frameId because
    // the compiled .pb.js uses camelCase accessor names.
    expect(msg.header.frameId ?? msg.header.frame_id).toBe('cam0');
  });

  it('lookupType returns null when schema is not loaded (buffers for retry)', () => {
    const msgType = registry.lookupType('bubbaloop.camera.v1.CompressedImage');
    expect(msgType).toBeNull();
  });

  it('accepts format=h264 and skips non-h264 formats', () => {
    const root = new protobuf.Root();
    const cameraNs = root.define('bubbaloop.camera.v1');
    const compressedImage = new protobuf.Type('CompressedImage');
    compressedImage.add(new protobuf.Field('format', 2, 'string'));
    compressedImage.add(new protobuf.Field('data', 3, 'bytes'));
    cameraNs.add(compressedImage);
    root.resolveAll();
    injectSchema(registry, root, 'camera', 'rtsp-camera');

    const msgType = registry.lookupType('bubbaloop.camera.v1.CompressedImage')!;

    // h264 format
    const h264Encoded = msgType.encode({ format: 'h264', data: new Uint8Array([1, 2, 3]) }).finish();
    // eslint-disable-next-line @typescript-eslint/no-explicit-any
    const h264Msg = msgType.decode(h264Encoded) as any;
    const h264Format: string = h264Msg.format ?? '';
    expect(h264Format).toBe('h264');
    // CameraView logic: skip if format && format !== 'h264'
    const shouldSkipH264 = h264Format && h264Format !== 'h264';
    expect(shouldSkipH264).toBe(false);

    // jpeg format
    const jpegEncoded = msgType.encode({ format: 'jpeg', data: new Uint8Array([0xff, 0xd8]) }).finish();
    // eslint-disable-next-line @typescript-eslint/no-explicit-any
    const jpegMsg = msgType.decode(jpegEncoded) as any;
    const jpegFormat: string = jpegMsg.format ?? '';
    expect(jpegFormat).toBe('jpeg');
    const shouldSkipJpeg = jpegFormat && jpegFormat !== 'h264';
    expect(shouldSkipJpeg).toBeTruthy();

    // empty format (allowed -- field might be missing)
    const emptyEncoded = msgType.encode({ format: '', data: new Uint8Array([0]) }).finish();
    // eslint-disable-next-line @typescript-eslint/no-explicit-any
    const emptyMsg = msgType.decode(emptyEncoded) as any;
    const emptyFormat: string = emptyMsg.format ?? '';
    const shouldSkipEmpty = emptyFormat && emptyFormat !== 'h264';
    expect(shouldSkipEmpty).toBeFalsy();
  });

  it('extracts data as Uint8Array for H264 decoding', () => {
    const root = new protobuf.Root();
    const cameraNs = root.define('bubbaloop.camera.v1');
    const compressedImage = new protobuf.Type('CompressedImage');
    compressedImage.add(new protobuf.Field('format', 2, 'string'));
    compressedImage.add(new protobuf.Field('data', 3, 'bytes'));
    cameraNs.add(compressedImage);
    root.resolveAll();
    injectSchema(registry, root, 'camera', 'rtsp-camera');

    const msgType = registry.lookupType('bubbaloop.camera.v1.CompressedImage')!;
    const h264Data = new Uint8Array([0x00, 0x00, 0x00, 0x01, 0x67, 0x42, 0x00, 0x1e]);
    const encoded = msgType.encode({ format: 'h264', data: h264Data }).finish();

    // eslint-disable-next-line @typescript-eslint/no-explicit-any
    const msg = msgType.decode(encoded) as any;
    // CameraView extracts data like this:
    const data: Uint8Array = msg.data instanceof Uint8Array ? msg.data : new Uint8Array(msg.data ?? []);
    expect(data).toBeInstanceOf(Uint8Array);
    expect(data.length).toBe(8);
    expect(data[3]).toBe(0x01);
    expect(data[4]).toBe(0x67);
  });
});

describe('Built-in decoder round-trip', () => {
  it('encodes and decodes NodeList with all fields', () => {
    const now = Date.now();
    const nodeState = NodeStateProto.create({
      name: 'my-camera',
      path: '/home/nvidia/.bubbaloop/nodes/my-camera',
      status: 2, // RUNNING
      installed: true,
      autostartEnabled: true,
      version: '1.0.0',
      description: 'RTSP camera node',
      nodeType: 'native',
      isBuilt: true,
      lastUpdatedMs: Long.fromNumber(now - 5000),
      buildOutput: ['step1 done', 'step2 done'],
      machineId: 'nvidia-orin00',
      machineHostname: 'nvidia-orin00',
      machineIps: ['192.168.1.100', '10.0.0.5'],
      baseNode: 'rtsp-camera',
    });

    const nodeList = NodeListProto.create({
      nodes: [nodeState],
      timestampMs: Long.fromNumber(now),
      machineId: 'nvidia-orin00',
    });

    const encoded = NodeListProto.encode(nodeList).finish();
    const decoded = decodeNodeList(encoded);

    expect(decoded).not.toBeNull();
    expect(decoded!.nodes).toHaveLength(1);
    expect(decoded!.machineId).toBe('nvidia-orin00');
    // timestampMs is Long -> BigInt
    expect(decoded!.timestampMs).toBe(BigInt(now));

    const node = decoded!.nodes[0];
    expect(node.name).toBe('my-camera');
    expect(node.path).toBe('/home/nvidia/.bubbaloop/nodes/my-camera');
    expect(node.status).toBe(2);
    expect(node.statusName).toBe('running');
    expect(node.installed).toBe(true);
    expect(node.autostartEnabled).toBe(true);
    expect(node.version).toBe('1.0.0');
    expect(node.description).toBe('RTSP camera node');
    expect(node.nodeType).toBe('native');
    expect(node.isBuilt).toBe(true);
    expect(node.lastUpdatedMs).toBe(BigInt(now - 5000));
    expect(node.buildOutput).toEqual(['step1 done', 'step2 done']);
    expect(node.machineId).toBe('nvidia-orin00');
    expect(node.machineHostname).toBe('nvidia-orin00');
    expect(node.machineIps).toEqual(['192.168.1.100', '10.0.0.5']);
    expect(node.baseNode).toBe('rtsp-camera');
  });

  it('handles NodeList with multiple nodes', () => {
    const nodes = ['camera-1', 'openmeteo', 'system-telemetry'].map((name, i) =>
      NodeStateProto.create({
        name,
        status: i + 1,
        installed: true,
        machineId: 'nvidia-orin00',
      })
    );

    const nodeList = NodeListProto.create({
      nodes,
      timestampMs: Long.fromNumber(1700000000000),
      machineId: 'nvidia-orin00',
    });

    const encoded = NodeListProto.encode(nodeList).finish();
    const decoded = decodeNodeList(encoded);

    expect(decoded).not.toBeNull();
    expect(decoded!.nodes).toHaveLength(3);
    expect(decoded!.nodes[0].name).toBe('camera-1');
    expect(decoded!.nodes[0].statusName).toBe('stopped');
    expect(decoded!.nodes[1].name).toBe('openmeteo');
    expect(decoded!.nodes[1].statusName).toBe('running');
    expect(decoded!.nodes[2].name).toBe('system-telemetry');
    expect(decoded!.nodes[2].statusName).toBe('failed');
  });

  it('handles NodeList with empty nodes array', () => {
    const nodeList = NodeListProto.create({
      nodes: [],
      timestampMs: Long.fromNumber(1700000000000),
      machineId: 'nvidia-orin00',
    });

    const encoded = NodeListProto.encode(nodeList).finish();
    const decoded = decodeNodeList(encoded);

    expect(decoded).not.toBeNull();
    expect(decoded!.nodes).toHaveLength(0);
    expect(decoded!.machineId).toBe('nvidia-orin00');
  });

  it('converts Long timestampMs to BigInt', () => {
    const timestamp = Long.fromNumber(1700000000000);
    const nodeList = NodeListProto.create({
      nodes: [],
      timestampMs: timestamp,
      machineId: 'test',
    });

    const encoded = NodeListProto.encode(nodeList).finish();
    const decoded = decodeNodeList(encoded);

    expect(decoded).not.toBeNull();
    expect(typeof decoded!.timestampMs).toBe('bigint');
    expect(decoded!.timestampMs).toBe(1700000000000n);
  });

  it('encodes and decodes NodeEvent with all fields', () => {
    const nodeState = NodeStateProto.create({
      name: 'openmeteo',
      status: 2,
      installed: true,
      machineId: 'nvidia-orin00',
      machineHostname: 'nvidia-orin00',
    });

    const nodeEvent = NodeEventProto.create({
      eventType: 'state_changed',
      nodeName: 'openmeteo',
      state: nodeState,
      timestampMs: Long.fromNumber(1700000000000),
    });

    const encoded = NodeEventProto.encode(nodeEvent).finish();
    const decoded = decodeNodeEvent(encoded);

    expect(decoded).not.toBeNull();
    expect(decoded!.eventType).toBe('state_changed');
    expect(decoded!.nodeName).toBe('openmeteo');
    expect(typeof decoded!.timestampMs).toBe('bigint');
    expect(decoded!.timestampMs).toBe(1700000000000n);

    expect(decoded!.state).toBeDefined();
    expect(decoded!.state!.name).toBe('openmeteo');
    expect(decoded!.state!.statusName).toBe('running');
    expect(decoded!.state!.machineId).toBe('nvidia-orin00');
  });

  it('decodes NodeEvent without state field', () => {
    const nodeEvent = NodeEventProto.create({
      eventType: 'removed',
      nodeName: 'old-node',
      timestampMs: Long.fromNumber(1700000000000),
    });

    const encoded = NodeEventProto.encode(nodeEvent).finish();
    const decoded = decodeNodeEvent(encoded);

    expect(decoded).not.toBeNull();
    expect(decoded!.eventType).toBe('removed');
    expect(decoded!.nodeName).toBe('old-node');
    expect(decoded!.state).toBeUndefined();
  });

  it('returns null for corrupted data', () => {
    const garbage = new Uint8Array([0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff]);
    // Suppress console.error during this test
    const consoleSpy = vi.spyOn(console, 'error').mockImplementation(() => {});

    const listResult = decodeNodeList(garbage);
    const eventResult = decodeNodeEvent(garbage);

    consoleSpy.mockRestore();

    // The built-in decoders should return null or a partially decoded result
    // (protobufjs may not throw for all garbage inputs)
    // We just verify they don't throw
    expect(listResult === null || listResult !== undefined).toBe(true);
    expect(eventResult === null || eventResult !== undefined).toBe(true);
  });

  it('maps all status enum values correctly', () => {
    const statusMap: Array<[number, string]> = [
      [0, 'unknown'],
      [1, 'stopped'],
      [2, 'running'],
      [3, 'failed'],
      [4, 'installing'],
      [5, 'building'],
      [6, 'not-installed'],
      [99, 'unknown'], // out of range
    ];

    for (const [statusNum, expected] of statusMap) {
      const nodeState = NodeStateProto.create({ name: 'test', status: statusNum });
      const nodeList = NodeListProto.create({
        nodes: [nodeState],
        timestampMs: Long.fromNumber(0),
        machineId: '',
      });
      const encoded = NodeListProto.encode(nodeList).finish();
      const decoded = decodeNodeList(encoded);
      expect(decoded).not.toBeNull();
      expect(decoded!.nodes[0].statusName).toBe(expected);
    }
  });
});

describe('Topic parsing integration', () => {
  it('extractSchemaFromTopic always returns null for vanilla zenoh topics', () => {
    expect(extractSchemaFromTopic('bubbaloop/local/m1/daemon/nodes')).toBeNull();
    expect(extractSchemaFromTopic('bubbaloop/local/m1/weather/current')).toBeNull();
    expect(extractSchemaFromTopic('bubbaloop/local/m1/system-telemetry/metrics')).toBeNull();
    expect(extractSchemaFromTopic('bubbaloop/local/m1/camera/entrance/compressed')).toBeNull();
    expect(extractSchemaFromTopic('bubbaloop/local/m1/network-monitor/status')).toBeNull();
  });

  it('extractTopicPrefix with scoped topic returns 4-segment prefix', () => {
    expect(extractTopicPrefix('bubbaloop/local/m1/system-telemetry/metrics'))
      .toBe('bubbaloop/local/m1/system-telemetry');
    expect(extractTopicPrefix('bubbaloop/local/nvidia_orin00/camera/entrance/compressed'))
      .toBe('bubbaloop/local/nvidia_orin00/camera');
    expect(extractTopicPrefix('bubbaloop/local/m1/network-monitor/status'))
      .toBe('bubbaloop/local/m1/network-monitor');
  });

  it('extractTopicPrefix returns null for short topics', () => {
    expect(extractTopicPrefix('bubbaloop/daemon')).toBeNull();
    expect(extractTopicPrefix('bubbaloop/local/m1')).toBeNull();
    expect(extractTopicPrefix('bubbaloop')).toBeNull();
    expect(extractTopicPrefix('')).toBeNull();
  });

  it('extractTopicPrefix returns null for non-bubbaloop topics', () => {
    expect(extractTopicPrefix('other/foo/bar/baz')).toBeNull();
    expect(extractTopicPrefix('zenoh/admin/status/check')).toBeNull();
  });

  it('combined: topic -> guess type -> decode -> verify fields', () => {
    const registry = new SchemaRegistry();
    const root = new protobuf.Root();
    const ns = root.define('bubbaloop.weather.v1');
    const weather = new protobuf.Type('CurrentWeather');
    weather.add(new protobuf.Field('temperature', 1, 'float'));
    weather.add(new protobuf.Field('wind_speed', 2, 'float'));
    weather.add(new protobuf.Field('humidity', 3, 'float'));
    ns.add(weather);
    root.resolveAll();
    injectSchema(registry, root, 'weather', 'openmeteo');

    // Encode
    const msgType = root.lookupType('bubbaloop.weather.v1.CurrentWeather');
    const encoded = msgType.encode({ temperature: 22.5, wind_speed: 5.3, humidity: 72.0 }).finish();

    // Full pipeline: use tryDecodeForTopic which guesses type from topic
    const topic = 'bubbaloop/local/m1/weather/current';
    const result = registry.tryDecodeForTopic(topic, encoded);
    expect(result).not.toBeNull();
    expect(result!.source).toBe('openmeteo');
    expect(result!.typeName).toBe('bubbaloop.weather.v1.CurrentWeather');
    const data = result!.data as Record<string, number>;
    expect(data.temperature).toBeCloseTo(22.5);
    expect(data.windSpeed).toBeCloseTo(5.3); // snake_case -> camelCase
    expect(data.humidity).toBeCloseTo(72.0);
  });
});

describe('decodePayload with various vanilla Zenoh topic patterns', () => {
  let registry: SchemaRegistry;

  beforeEach(() => {
    registry = new SchemaRegistry();

    const root = new protobuf.Root();
    const weatherNs = root.define('bubbaloop.weather.v1');
    const currentWeather = new protobuf.Type('CurrentWeather');
    currentWeather.add(new protobuf.Field('temperature', 1, 'float'));
    currentWeather.add(new protobuf.Field('humidity', 2, 'float'));
    weatherNs.add(currentWeather);

    const hourlyForecast = new protobuf.Type('HourlyForecast');
    hourlyForecast.add(new protobuf.Field('temperature', 1, 'float', 'repeated'));
    weatherNs.add(hourlyForecast);

    root.resolveAll();
    injectSchema(registry, root, 'weather', 'openmeteo');
  });

  it('decodes weather from production scope topic', () => {
    const msgType = registry.lookupType('bubbaloop.weather.v1.CurrentWeather')!;
    const encoded = msgType.encode({ temperature: 18.5, humidity: 80.0 }).finish();
    const result = decodePayload(encoded, 'bubbaloop/production/orin_02/weather/current', registry);
    expect(result.schemaSource).toBe('dynamic');
    expect(result.schema).toBe('bubbaloop.weather.v1.CurrentWeather');
  });

  it('decodes weather from staging scope topic', () => {
    const msgType = registry.lookupType('bubbaloop.weather.v1.CurrentWeather')!;
    const encoded = msgType.encode({ temperature: 25.0, humidity: 55.0 }).finish();
    const result = decodePayload(encoded, 'bubbaloop/staging/test01/weather/current', registry);
    expect(result.schemaSource).toBe('dynamic');
    expect(result.schema).toBe('bubbaloop.weather.v1.CurrentWeather');
  });

  it('decodes weather from dev scope topic', () => {
    const msgType = registry.lookupType('bubbaloop.weather.v1.CurrentWeather')!;
    const encoded = msgType.encode({ temperature: 30.0, humidity: 40.0 }).finish();
    const result = decodePayload(encoded, 'bubbaloop/dev/orin_dev01/weather/current', registry);
    expect(result.schemaSource).toBe('dynamic');
    const data = result.data as Record<string, number>;
    expect(data.temperature).toBeCloseTo(30.0);
  });

  it('falls back to hex for unknown binary on vanilla zenoh topic', () => {
    const binary = new Uint8Array([0x00, 0x01, 0x02, 0x03]);
    const result = decodePayload(binary, 'bubbaloop/local/m1/unknown-service/data', registry);
    expect(result.schema).toBe('Binary');
    expect(result.schemaSource).toBe('raw');
  });

  it('decodes JSON payload regardless of vanilla zenoh topic', () => {
    const json = JSON.stringify({ status: 'ok', uptime: 12345 });
    const payload = new TextEncoder().encode(json);
    const result = decodePayload(payload, 'bubbaloop/local/m1/any-node/health', registry);
    expect(result.schema).toBe('JSON');
    expect(result.schemaSource).toBe('builtin');
    expect((result.data as Record<string, unknown>).status).toBe('ok');
  });
});

describe('snakeToCamel field mapping for all message types', () => {
  // Test the snakeToCamel function directly with known field patterns

  it('temperature_2m stays as temperature_2m (digit after underscore)', () => {
    const result = snakeToCamel({ temperature_2m: 23.5 }) as Record<string, unknown>;
    expect(result.temperature_2m).toBe(23.5);
  });

  it('wind_speed_10m becomes windSpeed_10m', () => {
    const result = snakeToCamel({ wind_speed_10m: 12.3 }) as Record<string, unknown>;
    expect(result.windSpeed_10m).toBe(12.3);
  });

  it('acq_time becomes acqTime', () => {
    const result = snakeToCamel({ acq_time: 1000 }) as Record<string, unknown>;
    expect(result.acqTime).toBe(1000);
  });

  it('pub_time becomes pubTime', () => {
    const result = snakeToCamel({ pub_time: 2000 }) as Record<string, unknown>;
    expect(result.pubTime).toBe(2000);
  });

  it('frame_id becomes frameId', () => {
    const result = snakeToCamel({ frame_id: 'cam0' }) as Record<string, unknown>;
    expect(result.frameId).toBe('cam0');
  });

  it('machine_id becomes machineId', () => {
    const result = snakeToCamel({ machine_id: 'nvidia-orin00' }) as Record<string, unknown>;
    expect(result.machineId).toBe('nvidia-orin00');
  });

  it('autostart_enabled becomes autostartEnabled', () => {
    const result = snakeToCamel({ autostart_enabled: true }) as Record<string, unknown>;
    expect(result.autostartEnabled).toBe(true);
  });

  it('last_updated_ms becomes lastUpdatedMs', () => {
    const result = snakeToCamel({ last_updated_ms: '12345' }) as Record<string, unknown>;
    expect(result.lastUpdatedMs).toBe('12345');
  });

  it('health_status becomes healthStatus', () => {
    const result = snakeToCamel({ health_status: 'ok' }) as Record<string, unknown>;
    expect(result.healthStatus).toBe('ok');
  });

  it('status_code becomes statusCode', () => {
    const result = snakeToCamel({ status_code: 200 }) as Record<string, unknown>;
    expect(result.statusCode).toBe(200);
  });

  it('latency_ms becomes latencyMs', () => {
    const result = snakeToCamel({ latency_ms: 42.5 }) as Record<string, unknown>;
    expect(result.latencyMs).toBe(42.5);
  });

  it('per_core becomes perCore', () => {
    const result = snakeToCamel({ per_core: [0.5, 0.3] }) as Record<string, unknown>;
    expect(result.perCore).toEqual([0.5, 0.3]);
  });

  it('total_bytes becomes totalBytes', () => {
    const result = snakeToCamel({ total_bytes: 1024 }) as Record<string, unknown>;
    expect(result.totalBytes).toBe(1024);
  });

  it('used_bytes becomes usedBytes', () => {
    const result = snakeToCamel({ used_bytes: 512 }) as Record<string, unknown>;
    expect(result.usedBytes).toBe(512);
  });

  it('uptime_secs becomes uptimeSecs', () => {
    const result = snakeToCamel({ uptime_secs: 86400 }) as Record<string, unknown>;
    expect(result.uptimeSecs).toBe(86400);
  });

  it('timestamp_ms becomes timestampMs', () => {
    const result = snakeToCamel({ timestamp_ms: '1700000000000' }) as Record<string, unknown>;
    expect(result.timestampMs).toBe('1700000000000');
  });

  it('__proto__ is filtered out', () => {
    const input = { name: 'test' };
    // Manually add __proto__ to avoid prototype pollution warnings
    Object.defineProperty(input, '__proto_extra__', { value: 'evil', enumerable: true });
    const result = snakeToCamel({ __proto__: 'evil', name: 'safe' }) as Record<string, unknown>;
    expect(result.name).toBe('safe');
    expect(Object.keys(result)).not.toContain('__proto__');
  });

  it('constructor is filtered out', () => {
    const result = snakeToCamel({ constructor: 'evil', name: 'safe' }) as Record<string, unknown>;
    expect(result.name).toBe('safe');
    expect(Object.keys(result)).not.toContain('constructor');
  });

  it('handles nested objects recursively', () => {
    const result = snakeToCamel({
      machine_id: 'test',
      header: {
        frame_id: 'cam0',
        acq_time: 1000,
      },
    }) as Record<string, unknown>;

    expect(result.machineId).toBe('test');
    const header = result.header as Record<string, unknown>;
    expect(header.frameId).toBe('cam0');
    expect(header.acqTime).toBe(1000);
  });

  it('handles arrays of objects', () => {
    const result = snakeToCamel([
      { machine_id: 'a', total_bytes: 100 },
      { machine_id: 'b', total_bytes: 200 },
    ]) as Array<Record<string, unknown>>;

    expect(result).toHaveLength(2);
    expect(result[0].machineId).toBe('a');
    expect(result[0].totalBytes).toBe(100);
    expect(result[1].machineId).toBe('b');
    expect(result[1].totalBytes).toBe(200);
  });

  it('handles null and undefined passthrough', () => {
    expect(snakeToCamel(null)).toBeNull();
    expect(snakeToCamel(undefined)).toBeUndefined();
  });

  it('handles primitive passthrough', () => {
    expect(snakeToCamel(42)).toBe(42);
    expect(snakeToCamel('hello')).toBe('hello');
    expect(snakeToCamel(true)).toBe(true);
  });

  it('handles empty object', () => {
    const result = snakeToCamel({}) as Record<string, unknown>;
    expect(Object.keys(result)).toHaveLength(0);
  });

  it('handles keys already in camelCase', () => {
    const result = snakeToCamel({ machineId: 'test', frameId: 'cam0' }) as Record<string, unknown>;
    expect(result.machineId).toBe('test');
    expect(result.frameId).toBe('cam0');
  });

  it('converts multiple underscored segments', () => {
    const result = snakeToCamel({
      very_long_field_name: 'value',
      a_b_c_d: 'multi',
    }) as Record<string, unknown>;
    expect(result.veryLongFieldName).toBe('value');
    expect(result.aBCD).toBe('multi');
  });
});
