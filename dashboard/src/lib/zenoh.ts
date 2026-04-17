import { Session, Config, Subscriber, Sample, Encoding } from '@eclipse-zenoh/zenoh-ts';
import { decode as cborDecode } from 'cbor-x';

const DANGEROUS_KEYS = new Set(['__proto__', 'constructor', 'prototype']);

/**
 * Recursively convert snake_case keys to camelCase.
 * Nodes publish snake_case; dashboard normalizes.
 * Only matches `_[a-z]` so fields like `temperature_2m` stay intact.
 * Drops `__proto__`/`constructor`/`prototype` keys to avoid prototype pollution.
 */
export function snakeToCamel(obj: unknown): unknown {
  if (obj === null || obj === undefined || typeof obj !== 'object') return obj;
  if (Array.isArray(obj)) return obj.map(snakeToCamel);
  const result: Record<string, unknown> = {};
  for (const [key, value] of Object.entries(obj as Record<string, unknown>)) {
    if (DANGEROUS_KEYS.has(key)) continue;
    const camelKey = key.replace(/_([a-z])/g, (_, c) => c.toUpperCase());
    if (DANGEROUS_KEYS.has(camelKey)) continue;
    result[camelKey] = snakeToCamel(value);
  }
  return result;
}

/**
 * Numeric encoding IDs matching Zenoh's predefined encodings.
 * Defined locally because zenoh-ts does not export EncodingPredefined from its public API.
 */
export enum EncodingPredefined {
  ZENOH_BYTES = 0,
  ZENOH_STRING = 1,
  ZENOH_SERIALIZED = 2,
  APPLICATION_OCTET_STREAM = 3,
  TEXT_PLAIN = 4,
  APPLICATION_JSON = 5,
  TEXT_JSON = 6,
  APPLICATION_CDR = 7,
  APPLICATION_CBOR = 8,
  APPLICATION_YAML = 9,
  TEXT_YAML = 10,
  TEXT_JSON5 = 11,
  APPLICATION_PYTHON_SERIALIZED_OBJECT = 12,
  APPLICATION_PROTOBUF = 13,
  APPLICATION_JAVA_SERIALIZED_OBJECT = 14,
}
import { useEffect, useRef, useState, useCallback } from 'react';

export interface ZenohConfig {
  endpoint: string; // e.g., 'ws://127.0.0.1:10001'
}

export type ConnectionStatus = 'disconnected' | 'connecting' | 'connected' | 'error';

export interface UseZenohSessionResult {
  session: Session | null;
  status: ConnectionStatus;
  error: Error | null;
  reconnect: () => void;
}

/**
 * React hook for managing a Zenoh session
 */
export function useZenohSession(config: ZenohConfig): UseZenohSessionResult {
  const [session, setSession] = useState<Session | null>(null);
  const [status, setStatus] = useState<ConnectionStatus>('disconnected');
  const [error, setError] = useState<Error | null>(null);
  const sessionRef = useRef<Session | null>(null);

  const connect = useCallback(async () => {
    if (sessionRef.current) {
      try {
        await sessionRef.current.close();
      } catch {
        // Ignore close errors
      }
      sessionRef.current = null;
    }

    setStatus('connecting');
    setError(null);

    try {
      const zenohConfig = new Config(config.endpoint);
      const newSession = await Session.open(zenohConfig);
      sessionRef.current = newSession;
      setSession(newSession);
      setStatus('connected');
    } catch (e) {
      const err = e instanceof Error ? e : new Error(String(e));
      setError(err);
      setStatus('error');
      console.error('[Zenoh] Connection failed:', err);
    }
  }, [config.endpoint]);

  useEffect(() => {
    connect();

    return () => {
      if (sessionRef.current) {
        sessionRef.current.close().catch(console.error);
        sessionRef.current = null;
      }
    };
  }, [connect]);

  return {
    session,
    status,
    error,
    reconnect: connect,
  };
}

export interface UseZenohSubscriberResult {
  messageCount: number;
  fps: number;        // Smoothed FPS (moving average)
  instantFps: number; // Raw FPS for last second
}

// Callback type for sample handler
type SampleCallback = (sample: Sample) => void;

// Number of samples for moving average FPS calculation
const FPS_WINDOW_SIZE = 15;

/**
 * React hook for subscribing to a Zenoh topic
 */
export function useZenohSubscriber(
  session: Session | null,
  topic: string,
  onSample?: SampleCallback
): UseZenohSubscriberResult {
  const [messageCount, setMessageCount] = useState(0);
  const [fps, setFps] = useState(0);
  const [instantFps, setInstantFps] = useState(0);
  const subscriberRef = useRef<Subscriber | null>(null);
  const messageCountRef = useRef(0);
  const fpsIntervalRef = useRef<number | null>(null);
  const fpsHistoryRef = useRef<number[]>([]);
  const onSampleRef = useRef(onSample);

  // Keep callback ref updated
  useEffect(() => {
    onSampleRef.current = onSample;
  }, [onSample]);

  useEffect(() => {
    if (!session || !topic) return;

    let mounted = true;

    const setupSubscriber = async () => {
      try {
        // Clean up previous subscriber
        if (subscriberRef.current) {
          await subscriberRef.current.undeclare();
          subscriberRef.current = null;
        }

        // Use callback-based handler
        const subscriber = await session.declareSubscriber(topic, {
          handler: (sample: Sample) => {
            if (!mounted) return;

            messageCountRef.current++;
            setMessageCount(messageCountRef.current);
            onSampleRef.current?.(sample);
          },
        });

        subscriberRef.current = subscriber;
        console.log(`[Zenoh] Subscribed to ${topic}`);
      } catch (e) {
        console.error(`[Zenoh] Failed to subscribe to ${topic}:`, e);
      }
    };

    setupSubscriber();

    // FPS counter with moving average
    let lastCount = 0;
    fpsIntervalRef.current = window.setInterval(() => {
      const currentCount = messageCountRef.current;
      const currentFps = currentCount - lastCount;
      lastCount = currentCount;

      // Update instant FPS
      setInstantFps(currentFps);

      // Update moving average
      const history = fpsHistoryRef.current;
      history.push(currentFps);
      if (history.length > FPS_WINDOW_SIZE) {
        history.shift();
      }

      // Calculate smoothed average
      const avgFps = Math.round(history.reduce((a, b) => a + b, 0) / history.length);
      setFps(avgFps);
    }, 1000);

    return () => {
      mounted = false;
      if (fpsIntervalRef.current) {
        clearInterval(fpsIntervalRef.current);
      }
      if (subscriberRef.current) {
        subscriberRef.current.undeclare().catch(console.error);
        subscriberRef.current = null;
      }
    };
  }, [session, topic]);

  return {
    messageCount,
    fps,
    instantFps,
  };
}

/**
 * Encoding information extracted from a Zenoh sample.
 * id=0 (ZENOH_BYTES) and id=1 (ZENOH_STRING) mean "no encoding signal" — use sniff fallback.
 */
export interface EncodingInfo {
  /** EncodingPredefined numeric id */
  id: EncodingPredefined;
  /** Optional schema suffix (e.g. "bubbaloop.camera.v1.CompressedImage") */
  schema?: string;
}

export { Encoding };

/**
 * Extract encoding information from a Zenoh sample.
 * Returns the encoding id and optional schema suffix.
 * ZENOH_BYTES (0) and ZENOH_STRING (1) are treated as "no encoding signal".
 */
export function getEncodingInfo(sample: Sample): EncodingInfo {
  try {
    const encoding: Encoding = sample.encoding();
    const [id, schema] = encoding.toIdSchema();
    return { id: id as number as EncodingPredefined, schema };
  } catch {
    // If encoding() throws for any reason, treat as no signal
    return { id: EncodingPredefined.ZENOH_BYTES };
  }
}

/**
 * Returns true when the encoding id carries a meaningful format signal.
 * ZENOH_BYTES (0) and ZENOH_STRING (1) are the "no encoding" defaults.
 */
export function hasExplicitEncoding(info: EncodingInfo): boolean {
  return info.id !== EncodingPredefined.ZENOH_BYTES && info.id !== EncodingPredefined.ZENOH_STRING;
}

/**
 * Extract payload bytes from a Zenoh sample
 * In zenoh-ts, sample.payload() is a method that returns ZBytes,
 * and ZBytes.toBytes() returns the underlying Uint8Array
 */
export function getSamplePayload(sample: Sample): Uint8Array {
  // sample.payload() is a METHOD that returns ZBytes
  const zbytes = sample.payload();

  // ZBytes has a toBytes() method that returns Uint8Array
  if (zbytes && typeof zbytes.toBytes === 'function') {
    return zbytes.toBytes();
  }

  // Fallback: if zbytes is already a Uint8Array somehow
  if (zbytes instanceof Uint8Array) {
    return zbytes;
  }

  console.warn('[Zenoh] Failed to extract payload from sample');
  return new Uint8Array(0);
}

/**
 * Try to decode a JSON-encoded Zenoh payload with snakeToCamel key conversion.
 * Returns the decoded object if the sample has explicit JSON encoding and parses
 * successfully, or null otherwise (caller should fall through to other paths).
 */
/**
 * Try to decode a CBOR-encoded Zenoh payload with snakeToCamel key conversion.
 * Returns the decoded object if the sample has APPLICATION_CBOR encoding and decodes
 * successfully, or null otherwise (caller should fall through to other paths).
 */
/**
 * Detect the SDK provenance envelope `{header, body}` and return the inner
 * body. Returns the original value unchanged for non-enveloped payloads.
 *
 * Defensive: requires exactly the two keys `header` and `body` (otherwise a
 * user payload that happens to define a `header` field would be misread as
 * an envelope).
 */
export function unwrapCborEnvelope(value: unknown): unknown {
  if (
    value !== null &&
    typeof value === 'object' &&
    !Array.isArray(value)
  ) {
    const obj = value as Record<string, unknown>;
    const keys = Object.keys(obj);
    if (
      keys.length === 2 &&
      keys.includes('header') &&
      keys.includes('body') &&
      obj.header !== null &&
      typeof obj.header === 'object'
    ) {
      return obj.body;
    }
  }
  return value;
}

export function tryDecodeCborPayload(payload: Uint8Array, encodingInfo: EncodingInfo): unknown | null {
  if (!hasExplicitEncoding(encodingInfo)) return null;
  if (encodingInfo.id !== EncodingPredefined.APPLICATION_CBOR) return null;
  try {
    const decoded = cborDecode(payload);
    // SDK CBOR payloads are wrapped in `{header, body}` for provenance.
    // Specialized views (camera, telemetry, weather, network) want the body.
    return snakeToCamel(unwrapCborEnvelope(decoded));
  } catch {
    return null;
  }
}

/**
 * Unwrap a JSON `{header, body}` provenance envelope. Same shape as the
 * CBOR envelope. Returns the inner body if the payload matches, otherwise
 * returns the original value unchanged. Shares its detection heuristic
 * with `unwrapCborEnvelope` so both edges look identical to views.
 */
export function unwrapJsonEnvelope(value: unknown): unknown {
  return unwrapCborEnvelope(value);
}

/** Like `unwrapJsonEnvelope` but also reports whether an unwrap happened. */
export function unwrapJsonEnvelopeWithFlag(value: unknown): { body: unknown; wasEnveloped: boolean } {
  const unwrapped = unwrapCborEnvelope(value);
  return { body: unwrapped, wasEnveloped: unwrapped !== value };
}

export function tryDecodeJsonPayload(payload: Uint8Array, encodingInfo: EncodingInfo): unknown | null {
  if (!hasExplicitEncoding(encodingInfo)) return null;
  if (encodingInfo.id !== EncodingPredefined.APPLICATION_JSON &&
      encodingInfo.id !== EncodingPredefined.TEXT_JSON &&
      encodingInfo.id !== EncodingPredefined.TEXT_JSON5) return null;
  try {
    const text = new TextDecoder().decode(payload);
    return snakeToCamel(unwrapJsonEnvelope(JSON.parse(text)));
  } catch {
    return null;
  }
}

/** A discovered topic entry */
export interface DiscoveredTopic {
  /** Human-readable name (bubbaloop/ prefix stripped) */
  display: string;
  /** Raw Zenoh key expression (for subscribing) */
  raw: string;
}

export interface UseTopicDiscoveryResult {
  /** Deduplicated discovered topics with display names and raw keys */
  topics: DiscoveredTopic[];
  isDiscovering: boolean;
  refresh: () => void;
}
/**
 * Normalize a raw Zenoh key expression to a human-readable form.
 *
 * Strips the leading "bubbaloop/global/" prefix so the display shows the
 * machine-id and resource only.
 *
 * Examples:
 *   "bubbaloop/global/nvidia_orin00/camera/entrance/compressed" -> "nvidia_orin00/camera/entrance/compressed"
 *   "bubbaloop/global/nvidia_orin00/system-telemetry/health"    -> "nvidia_orin00/system-telemetry/health"
 */
export function normalizeKeyExpr(keyExpr: string): { display: string; raw: string } {
  const parts = keyExpr.split('/');

  // New format: "bubbaloop/global/{machine_id}/..." → strip "bubbaloop/global/"
  if (parts[0] === 'bubbaloop' && parts[1] === 'global' && parts.length >= 3) {
    return { display: parts.slice(2).join('/'), raw: keyExpr };
  }

  // Fallback: strip only the "bubbaloop/" prefix
  if (parts[0] === 'bubbaloop' && parts.length >= 2) {
    return { display: parts.slice(1).join('/'), raw: keyExpr };
  }

  return { display: keyExpr, raw: keyExpr };
}

/**
 * Extract machine ID from a Zenoh key expression.
 *
 * New format: "bubbaloop/global/{machine_id}/..." -> machine_id (parts[2])
 * Local SHM: "bubbaloop/local/{machine_id}/..." -> null (not network-visible)
 *
 * Returns null for local or unrecognized paths.
 */
export function extractMachineId(keyExpr: string): string | null {
  const parts = keyExpr.split('/');

  if (parts[0] === 'bubbaloop') {
    // Local SHM topics — not network-visible
    if (parts[1] === 'local') {
      return null;
    }

    // New global format: "bubbaloop/global/{machine_id}/..."
    if (parts[1] === 'global' && parts.length >= 3) {
      return parts[2];
    }
  }

  return null;
}

export function useZenohTopicDiscovery(
  session: Session | null,
  pattern: string = '**'
): UseTopicDiscoveryResult {
  const [topics, setTopics] = useState<DiscoveredTopic[]>([]);
  const [isDiscovering, setIsDiscovering] = useState(false);
  // Map from raw key expression -> display name (deduplicate by raw key)
  const topicMapRef = useRef<Map<string, string>>(new Map());
  const subscriberRef = useRef<Subscriber | null>(null);

  const discover = useCallback(async () => {
    if (!session) return;

    setIsDiscovering(true);
    topicMapRef.current.clear();
    setTopics([]);

    try {
      // Clean up previous subscriber
      if (subscriberRef.current) {
        await subscriberRef.current.undeclare();
        subscriberRef.current = null;
      }

      // Subscribe to pattern to discover topics
      const subscriber = await session.declareSubscriber(pattern, {
        handler: (sample: Sample) => {
          const keyExpr = sample.keyexpr().toString();
          const { display, raw } = normalizeKeyExpr(keyExpr);

          if (!topicMapRef.current.has(raw)) {
            topicMapRef.current.set(raw, display);
            const sorted = Array.from(topicMapRef.current.entries())
              .sort(([, a], [, b]) => a.localeCompare(b))
              .map(([r, d]) => ({ display: d, raw: r }));
            setTopics(sorted);
          }
        },
      });

      subscriberRef.current = subscriber;
      console.log(`[Zenoh] Topic discovery started with pattern: ${pattern}`);

      // Stop discovering after a short period but keep subscriber for new topics
      setTimeout(() => {
        setIsDiscovering(false);
      }, 3000);
    } catch (e) {
      console.error('[Zenoh] Topic discovery failed:', e);
      setIsDiscovering(false);
    }
  }, [session, pattern]);

  // Start discovery when session is available
  useEffect(() => {
    if (session) {
      discover();
    }

    return () => {
      if (subscriberRef.current) {
        subscriberRef.current.undeclare().catch(console.error);
        subscriberRef.current = null;
      }
    };
  }, [session, discover]);

  return {
    topics,
    isDiscovering,
    refresh: discover,
  };
}
