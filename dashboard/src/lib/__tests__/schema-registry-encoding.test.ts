/**
 * Tests for SchemaRegistry.decodeWithEncoding() — encoding-first decode path.
 *
 * Tests each encoding type:
 * - APPLICATION_JSON (5): decode with JSON.parse, no schema needed
 * - TEXT_JSON (6): same as APPLICATION_JSON
 * - APPLICATION_PROTOBUF (13) with schema suffix + cached type: decode with SchemaRegistry
 * - APPLICATION_PROTOBUF (13) without schema suffix: falls through to sniff
 * - ZENOH_BYTES (0): sniff fallback
 * - ZENOH_STRING (1): sniff fallback
 * - Unknown / other: sniff fallback
 */

import { describe, it, expect, beforeEach, vi, type Mock } from 'vitest';
import { SchemaRegistry } from '../schema-registry';
import { EncodingPredefined } from '../zenoh';
import type { EncodingInfo } from '../zenoh';
import type { Session } from '@eclipse-zenoh/zenoh-ts';

// Minimal Session mock — decodeWithEncoding only calls discoverSchemaForTopic which uses session.get
function makeMockSession(): Session {
  return {
    get: vi.fn().mockResolvedValue(null),
  } as unknown as Session;
}

// Encode a string as UTF-8 bytes
function strBytes(s: string): Uint8Array {
  return new TextEncoder().encode(s);
}

// Build a minimal EncodingInfo
function enc(id: EncodingPredefined, schema?: string): EncodingInfo {
  return { id, schema };
}

describe('SchemaRegistry.decodeWithEncoding', () => {
  let registry: SchemaRegistry;
  let session: Session;

  beforeEach(() => {
    registry = new SchemaRegistry();
    session = makeMockSession();
  });

  // ---------------------------------------------------------------------------
  // APPLICATION_JSON
  // ---------------------------------------------------------------------------
  describe('APPLICATION_JSON (id=5)', () => {
    it('decodes valid JSON payload and returns typeName=json', async () => {
      const payload = strBytes(JSON.stringify({ temperature: 22.5, humidity: 60 }));
      const result = await registry.decodeWithEncoding(
        payload,
        enc(EncodingPredefined.APPLICATION_JSON),
        'bubbaloop/local/m1/weather/current',
        session,
      );
      expect(result).not.toBeNull();
      expect(result!.typeName).toBe('json');
      expect(result!.source).toBe('encoding');
      expect((result!.data as Record<string, unknown>).temperature).toBe(22.5);
    });

    it('returns null for malformed JSON with APPLICATION_JSON encoding', async () => {
      const payload = strBytes('not valid json {{{');
      const result = await registry.decodeWithEncoding(
        payload,
        enc(EncodingPredefined.APPLICATION_JSON),
        'bubbaloop/local/m1/weather/current',
        session,
      );
      expect(result).toBeNull();
    });

    it('handles empty JSON object', async () => {
      const payload = strBytes('{}');
      const result = await registry.decodeWithEncoding(
        payload,
        enc(EncodingPredefined.APPLICATION_JSON),
        'test/topic',
        session,
      );
      expect(result).not.toBeNull();
      expect(result!.typeName).toBe('json');
    });
  });

  // ---------------------------------------------------------------------------
  // TEXT_JSON
  // ---------------------------------------------------------------------------
  describe('TEXT_JSON (id=6)', () => {
    it('decodes valid JSON payload the same as APPLICATION_JSON', async () => {
      const payload = strBytes(JSON.stringify({ key: 'value' }));
      const result = await registry.decodeWithEncoding(
        payload,
        enc(EncodingPredefined.TEXT_JSON),
        'test/topic',
        session,
      );
      expect(result).not.toBeNull();
      expect(result!.typeName).toBe('json');
      expect(result!.source).toBe('encoding');
    });
  });

  // ---------------------------------------------------------------------------
  // TEXT_JSON5
  // ---------------------------------------------------------------------------
  describe('TEXT_JSON5 (id=11)', () => {
    it('decodes JSON5-encoded payload via JSON.parse for basic JSON', async () => {
      const payload = strBytes('{"a":1}');
      const result = await registry.decodeWithEncoding(
        payload,
        enc(EncodingPredefined.TEXT_JSON5),
        'test/topic',
        session,
      );
      expect(result).not.toBeNull();
      expect(result!.typeName).toBe('json');
    });
  });

  // ---------------------------------------------------------------------------
  // APPLICATION_PROTOBUF with schema suffix — type cached in registry
  // ---------------------------------------------------------------------------
  describe('APPLICATION_PROTOBUF (id=13) with schema suffix and cached type', () => {
    it('decodes protobuf payload when type is in cache', async () => {
      // Spy on registry.decode to return a fake result
      const fakeResult = {
        data: { name: 'test-node', status: 2 },
        typeName: 'bubbaloop.daemon.v1.NodeState',
        source: 'node:bubbaloop/local/m1/system-telemetry',
      };
      vi.spyOn(registry, 'decode').mockReturnValueOnce(fakeResult);

      const payload = new Uint8Array([0x0a, 0x09, 0x74, 0x65, 0x73, 0x74]); // fake proto bytes
      const result = await registry.decodeWithEncoding(
        payload,
        enc(EncodingPredefined.APPLICATION_PROTOBUF, 'bubbaloop.daemon.v1.NodeState'),
        'bubbaloop/local/m1/system-telemetry/metrics',
        session,
      );
      expect(result).not.toBeNull();
      expect(result!.typeName).toBe('bubbaloop.daemon.v1.NodeState');
      expect(registry.decode).toHaveBeenCalledWith('bubbaloop.daemon.v1.NodeState', payload);
    });

    it('triggers on-demand schema fetch when type is not cached and retries', async () => {
      // First decode call returns null (not cached), second returns a result (after fetch)
      const fakeResult = {
        data: { temperature: 20 },
        typeName: 'bubbaloop.weather.v1.CurrentWeather',
        source: 'node:bubbaloop/local/m1/weather',
      };
      const decodeMock = vi.spyOn(registry, 'decode')
        .mockReturnValueOnce(null)       // first attempt — not in cache
        .mockReturnValueOnce(fakeResult); // retry after fetch

      // discoverSchemaForTopic should be called with the topic
      const discoverSpy = vi.spyOn(registry, 'discoverSchemaForTopic').mockResolvedValueOnce(true);

      const payload = new Uint8Array([0x0a, 0x02]);
      const result = await registry.decodeWithEncoding(
        payload,
        enc(EncodingPredefined.APPLICATION_PROTOBUF, 'bubbaloop.weather.v1.CurrentWeather'),
        'bubbaloop/local/m1/weather/current',
        session,
      );

      expect(discoverSpy).toHaveBeenCalledWith(session, 'bubbaloop/local/m1/weather/current');
      expect(decodeMock).toHaveBeenCalledTimes(2);
      expect(result).not.toBeNull();
      expect(result!.typeName).toBe('bubbaloop.weather.v1.CurrentWeather');
    });

    it('returns null when type is not found even after schema fetch', async () => {
      vi.spyOn(registry, 'decode').mockReturnValue(null);
      vi.spyOn(registry, 'discoverSchemaForTopic').mockResolvedValueOnce(false);

      const result = await registry.decodeWithEncoding(
        new Uint8Array([0x0a]),
        enc(EncodingPredefined.APPLICATION_PROTOBUF, 'bubbaloop.unknown.Type'),
        'bubbaloop/local/m1/unknown/topic',
        session,
      );
      expect(result).toBeNull();
    });
  });

  // ---------------------------------------------------------------------------
  // APPLICATION_PROTOBUF without schema suffix — falls through to sniff
  // ---------------------------------------------------------------------------
  describe('APPLICATION_PROTOBUF (id=13) without schema suffix', () => {
    it('falls through to tryDecodeForTopic when no schema suffix', async () => {
      const fakeResult = {
        data: { val: 1 },
        typeName: 'bubbaloop.some.v1.Type',
        source: 'node:bubbaloop/local/m1/some-node',
      };
      const sniffSpy = vi.spyOn(registry, 'tryDecodeForTopic').mockReturnValueOnce(fakeResult);

      const result = await registry.decodeWithEncoding(
        new Uint8Array([0x08, 0x01]),
        enc(EncodingPredefined.APPLICATION_PROTOBUF, undefined), // no schema suffix
        'bubbaloop/local/m1/some-node/data',
        session,
      );
      expect(sniffSpy).toHaveBeenCalledWith('bubbaloop/local/m1/some-node/data', expect.any(Uint8Array));
      expect(result).not.toBeNull();
      expect(result!.typeName).toBe('bubbaloop.some.v1.Type');
    });
  });

  // ---------------------------------------------------------------------------
  // ZENOH_BYTES (0) — no encoding signal, use sniff fallback
  // ---------------------------------------------------------------------------
  describe('ZENOH_BYTES (id=0) — no encoding signal', () => {
    it('uses sniff fallback for plain JSON bytes', async () => {
      // tryDecodeForTopic will try JSON.parse via sniff chain
      const payload = strBytes(JSON.stringify({ a: 1 }));
      vi.spyOn(registry, 'tryDecodeForTopic').mockReturnValueOnce({
        data: { a: 1 },
        typeName: 'json',
        source: 'sniff',
      });

      const result = await registry.decodeWithEncoding(
        payload,
        enc(EncodingPredefined.ZENOH_BYTES),
        'bubbaloop/local/m1/some/topic',
        session,
      );
      expect(result).not.toBeNull();
      expect(registry.tryDecodeForTopic).toHaveBeenCalled();
    });

    it('returns null when sniff fallback finds nothing', async () => {
      vi.spyOn(registry, 'tryDecodeForTopic').mockReturnValueOnce(null);

      const result = await registry.decodeWithEncoding(
        new Uint8Array([0xde, 0xad, 0xbe, 0xef]),
        enc(EncodingPredefined.ZENOH_BYTES),
        'bubbaloop/local/m1/unknown/binary',
        session,
      );
      expect(result).toBeNull();
    });
  });

  // ---------------------------------------------------------------------------
  // ZENOH_STRING (1) — no encoding signal, use sniff fallback
  // ---------------------------------------------------------------------------
  describe('ZENOH_STRING (id=1) — treated as no encoding signal', () => {
    it('uses sniff fallback', async () => {
      const sniffSpy = vi.spyOn(registry, 'tryDecodeForTopic').mockReturnValueOnce(null);

      await registry.decodeWithEncoding(
        strBytes('hello'),
        enc(EncodingPredefined.ZENOH_STRING),
        'test/topic',
        session,
      );
      expect(sniffSpy).toHaveBeenCalled();
    });
  });

  // ---------------------------------------------------------------------------
  // APPLICATION_OCTET_STREAM (3) — falls through to sniff
  // ---------------------------------------------------------------------------
  describe('APPLICATION_OCTET_STREAM (id=3) — sniff fallback', () => {
    it('routes to tryDecodeForTopic', async () => {
      const sniffSpy = vi.spyOn(registry, 'tryDecodeForTopic').mockReturnValueOnce(null);

      await registry.decodeWithEncoding(
        new Uint8Array([0xff, 0xd8]),
        enc(EncodingPredefined.APPLICATION_OCTET_STREAM),
        'bubbaloop/local/m1/camera/raw',
        session,
      );
      expect(sniffSpy).toHaveBeenCalledWith('bubbaloop/local/m1/camera/raw', expect.any(Uint8Array));
    });
  });
});

// ---------------------------------------------------------------------------
// Tests for getEncodingInfo and hasExplicitEncoding helpers in zenoh.ts
// ---------------------------------------------------------------------------
import { getEncodingInfo, hasExplicitEncoding } from '../zenoh';
import { Encoding } from '@eclipse-zenoh/zenoh-ts';

describe('getEncodingInfo', () => {
  it('returns ZENOH_BYTES when sample.encoding() throws', () => {
    const badSample = {
      encoding: () => { throw new Error('no encoding'); },
    } as unknown as import('@eclipse-zenoh/zenoh-ts').Sample;
    const info = getEncodingInfo(badSample);
    expect(info.id).toBe(EncodingPredefined.ZENOH_BYTES);
    expect(info.schema).toBeUndefined();
  });

  it('extracts APPLICATION_JSON encoding', () => {
    const mockEncoding = Encoding.APPLICATION_JSON;
    const sample = {
      encoding: () => mockEncoding,
    } as unknown as import('@eclipse-zenoh/zenoh-ts').Sample;
    const info = getEncodingInfo(sample);
    expect(info.id).toBe(EncodingPredefined.APPLICATION_JSON);
  });

  it('extracts APPLICATION_PROTOBUF with schema suffix', () => {
    const schema = 'bubbaloop.camera.v1.CompressedImage';
    const mockEncoding = Encoding.APPLICATION_PROTOBUF.withSchema(schema);
    const sample = {
      encoding: () => mockEncoding,
    } as unknown as import('@eclipse-zenoh/zenoh-ts').Sample;
    const info = getEncodingInfo(sample);
    expect(info.id).toBe(EncodingPredefined.APPLICATION_PROTOBUF);
    expect(info.schema).toBe(schema);
  });
});

describe('hasExplicitEncoding', () => {
  it('returns false for ZENOH_BYTES (0)', () => {
    expect(hasExplicitEncoding({ id: EncodingPredefined.ZENOH_BYTES })).toBe(false);
  });

  it('returns false for ZENOH_STRING (1)', () => {
    expect(hasExplicitEncoding({ id: EncodingPredefined.ZENOH_STRING })).toBe(false);
  });

  it('returns true for APPLICATION_JSON (5)', () => {
    expect(hasExplicitEncoding({ id: EncodingPredefined.APPLICATION_JSON })).toBe(true);
  });

  it('returns true for APPLICATION_PROTOBUF (13)', () => {
    expect(hasExplicitEncoding({ id: EncodingPredefined.APPLICATION_PROTOBUF })).toBe(true);
  });

  it('returns true for APPLICATION_OCTET_STREAM (3)', () => {
    expect(hasExplicitEncoding({ id: EncodingPredefined.APPLICATION_OCTET_STREAM })).toBe(true);
  });
});
