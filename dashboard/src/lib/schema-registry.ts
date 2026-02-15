/**
 * Dynamic Protobuf Schema Registry
 *
 * Fetches FileDescriptorSet from Zenoh queryables (daemon and individual nodes),
 * then decodes arbitrary protobuf messages at runtime using protobufjs.
 */

import * as protobuf from 'protobufjs';
import { Session, Reply, ReplyError, Sample, ConsolidationMode } from '@eclipse-zenoh/zenoh-ts';
import { Duration } from 'typed-duration';
import { getSamplePayload } from './zenoh';

// Import the descriptor extension — adds fromDescriptor/toDescriptor to Root
// and registers google.protobuf.FileDescriptorSet in protobuf.roots
import 'protobufjs/ext/descriptor';

/** Schema source metadata */
export interface SchemaSource {
  /** Where the schema came from: "core" for daemon, node name for individual nodes */
  source: string;
  /** The protobufjs Root containing all types from this source */
  root: protobuf.Root;
}

/** Decode result */
export interface DecodeResult {
  /** Decoded message as a plain object */
  data: Record<string, unknown>;
  /** The fully qualified type name that was used */
  typeName: string;
  /** Where the schema came from */
  source: string;
}

/**
 * Extract schema name from ros-z topic format: <domain_id>/<topic>/<schema>/<hash>
 * e.g., "0/weather%current/bubbaloop.weather.v1.CurrentWeather/RIHS01_..."
 */
export function extractSchemaFromTopic(topic: string): string | null {
  const parts = topic.split('/');
  for (let i = parts.length - 2; i >= 1; i--) {
    const part = parts[i];
    if (part.includes('.') && !part.startsWith('RIHS')) {
      return part;
    }
  }
  return null;
}

/**
 * Load a FileDescriptorSet from raw bytes into a protobufjs Root.
 * Uses the descriptor extension which adds Root.fromDescriptor() at runtime.
 */
function rootFromDescriptorBytes(bytes: Uint8Array): protobuf.Root {
  // The descriptor extension adds fromDescriptor to Root.
  // Access it via the any-cast since TypeScript types don't include it.
  // eslint-disable-next-line @typescript-eslint/no-explicit-any
  const RootAny = protobuf.Root as any;

  // Look up FileDescriptorSet from the descriptor extension's registered root
  const descriptorRoot = protobuf.roots['default'];
  if (!descriptorRoot) {
    throw new Error('protobufjs/ext/descriptor not loaded');
  }

  const FileDescriptorSet = descriptorRoot.lookupType('google.protobuf.FileDescriptorSet');
  const decoded = FileDescriptorSet.decode(bytes);
  const root: protobuf.Root = RootAny.fromDescriptor(decoded);
  root.resolveAll();
  return root;
}

/**
 * SchemaRegistry manages dynamic protobuf schema loading and decoding.
 *
 * It queries Zenoh endpoints for FileDescriptorSet bytes, loads them into
 * protobufjs Root objects, and can then decode any message type at runtime.
 */
export class SchemaRegistry {
  /** Map from source key to loaded schema */
  private schemas = new Map<string, SchemaSource>();
  /** Node prefixes whose schemas loaded successfully — never re-query */
  private succeededPrefixes = new Set<string>();
  /** Node prefixes that failed — prefix -> last attempt timestamp */
  private failedPrefixes = new Map<string, number>();
  /** Cooldown (ms) before retrying a failed prefix */
  private static readonly RETRY_COOLDOWN_MS = 10_000;
  /** Whether core schemas have been fetched */
  private coreLoaded = false;
  /** Cached type names array, invalidated when schemas change */
  private typeNamesCache: string[] | null = null;
  /** Type-to-topic guess cache */
  private topicGuessCache = new Map<string, string | null>();

  /**
   * Load a FileDescriptorSet from raw bytes into a protobufjs Root.
   *
   * @param bytes - FileDescriptorSet binary data
   * @param sourceKey - Key to identify this schema source
   * @param sourceLabel - Human-readable label (e.g., "core" or node name)
   * @returns true if successfully loaded
   */
  loadDescriptorSet(bytes: Uint8Array, sourceKey: string, sourceLabel: string): boolean {
    try {
      const root = rootFromDescriptorBytes(bytes);
      this.schemas.set(sourceKey, { source: sourceLabel, root });
      this.typeNamesCache = null;
      this.topicGuessCache.clear();
      return true;
    } catch (e) {
      console.error(`[SchemaRegistry] Failed to load descriptor from ${sourceKey}:`, e);
      return false;
    }
  }

  /**
   * Fetch core schemas from the daemon's /schemas endpoint.
   *
   * @param session - Active Zenoh session
   * @returns true if schemas were fetched successfully
   */
  async fetchCoreSchemas(session: Session, machineIds?: string[]): Promise<boolean> {
    // Build list of schema endpoints to query.
    // Use machine-scoped paths when available, fall back to legacy path.
    const endpoints: string[] = [];
    if (machineIds && machineIds.length > 0) {
      for (const mid of machineIds) {
        endpoints.push(`bubbaloop/${mid}/daemon/api/schemas`);
      }
    } else {
      endpoints.push('bubbaloop/daemon/api/schemas');
    }

    let loaded = false;
    for (const endpoint of endpoints) {
      try {
        const receiver = await session.get(endpoint, {
          timeout: Duration.milliseconds.of(5000),
        });

        if (receiver) {
          for await (const replyItem of receiver) {
            if (replyItem instanceof Reply) {
              const replyResult = replyItem.result();
              if (replyResult instanceof ReplyError) continue;
              const payload = getSamplePayload(replyResult as Sample);
              if (payload.length > 0) {
                const keyExpr = (replyResult as Sample).keyexpr().toString();
                const sourceKey = `core:${keyExpr}`;
                if (this.loadDescriptorSet(payload, sourceKey, 'core')) {
                  loaded = true;
                }
              }
            }
          }
        }
      } catch (e) {
        console.error(`[SchemaRegistry] Failed to fetch schemas from ${endpoint}:`, e);
      }
    }

    this.coreLoaded = loaded;
    return loaded;
  }

  /**
   * Fetch schemas from a specific node's schema queryable.
   *
   * @param session - Active Zenoh session
   * @param prefix - The node topic prefix (e.g., "my-node")
   * @returns true if schemas were fetched successfully
   */
  async fetchNodeSchema(session: Session, prefix: string): Promise<boolean> {
    // Already loaded successfully — skip
    if (this.succeededPrefixes.has(prefix)) return false;

    // Failed before — only retry after cooldown
    const lastFailed = this.failedPrefixes.get(prefix);
    if (lastFailed !== undefined) {
      if (Date.now() - lastFailed < SchemaRegistry.RETRY_COOLDOWN_MS) return false;
    }

    try {
      const receiver = await session.get(`${prefix}/schema`, {
        timeout: Duration.milliseconds.of(3000),
      });

      let loaded = false;
      if (receiver) {
        for await (const replyItem of receiver) {
          if (replyItem instanceof Reply) {
            const replyResult = replyItem.result();
            if (replyResult instanceof ReplyError) continue;
            const payload = getSamplePayload(replyResult as Sample);
            if (payload.length > 0) {
              if (this.loadDescriptorSet(payload, `node:${prefix}`, prefix)) {
                loaded = true;
              }
            }
          }
        }
      }

      if (loaded) {
        this.succeededPrefixes.add(prefix);
        this.failedPrefixes.delete(prefix);
      } else {
        this.failedPrefixes.set(prefix, Date.now());
      }

      return loaded;
    } catch {
      this.failedPrefixes.set(prefix, Date.now());
      return false;
    }
  }

  /**
   * Proactively discover all node schemas by querying wildcard schema endpoints.
   * Called at startup to ensure all node types are available for decoding.
   *
   * This wildcard query works because the daemon's queryable for node schemas
   * no longer uses `.complete(true)`, allowing multiple nodes to respond to the
   * same wildcard pattern. Each running node with a schema queryable will reply.
   */
  async discoverAllNodeSchemas(session: Session, _machineIds?: string[]): Promise<number> {
    // Always use broad wildcard to avoid machine ID format mismatches
    // (e.g., nvidia_orin00 vs nvidia-orin00)
    const pattern = 'bubbaloop/**/schema';

    let discovered = 0;
    try {
      const receiver = await session.get(pattern, {
        timeout: Duration.milliseconds.of(5000),
        consolidation: ConsolidationMode.NONE,
      });

      if (receiver) {
        let replyIndex = 0;
        for await (const replyItem of receiver) {
          if (replyItem instanceof Reply) {
            const replyResult = replyItem.result();
            if (replyResult instanceof ReplyError) continue;
            const payload = getSamplePayload(replyResult as Sample);
            if (payload.length > 0) {
              const keyExpr = (replyResult as Sample).keyexpr().toString();
              // Skip core daemon schemas (already loaded by fetchCoreSchemas)
              if (keyExpr.includes('/daemon/api/schemas')) continue;
              // When nodes reply with the wildcard pattern (query.key_expr()),
              // all replies share the same key — use index-based source names
              // to avoid dedup skipping subsequent schemas.
              const isWildcard = keyExpr.includes('*');
              const segments = keyExpr.split('/');
              const schemaIdx = segments.lastIndexOf('schema');
              const nodeName = isWildcard
                ? `discovered-${replyIndex}`
                : (schemaIdx > 0 ? segments[schemaIdx - 1] : keyExpr);
              const prefix = isWildcard
                ? `wildcard-${replyIndex}`
                : segments.slice(0, schemaIdx).join('/');
              // Only skip if already succeeded — failed prefixes should be retried
              if (!this.succeededPrefixes.has(prefix) && this.loadDescriptorSet(payload, `node:${prefix}`, nodeName)) {
                this.succeededPrefixes.add(prefix);
                this.failedPrefixes.delete(prefix);
                discovered++;
              }
              replyIndex++;
            }
          }
        }
      }
    } catch (e) {
      console.error(`[SchemaRegistry] Failed to discover node schemas with pattern ${pattern}:`, e);
    }

    return discovered;
  }

  /**
   * Try to discover schemas for a new topic prefix.
   * Called when the dashboard encounters a topic it hasn't seen before.
   *
   * Strategy:
   * 1. Extract topic prefix (first 4 segments) and try direct query to {prefix}/schema.
   *    This works when the 4th segment is the node name (e.g., "bubbaloop/local/m1/system-telemetry/metrics").
   *
   * 2. If that fails, fall back to wildcard discovery (bubbaloop/** /schema).
   *    This is necessary because topic resource names often differ from node names:
   *      - camera/* topics are published by the "rtsp-camera" node
   *      - weather/* topics are published by the "openmeteo" node
   *      - The 4th segment in these cases is the topic resource ("camera", "weather"),
   *        not the node name, so the prefix-based query would fail.
   *
   *    The wildcard query now works correctly since the daemon's schema queryable
   *    no longer uses `.complete(true)`, allowing all nodes to respond.
   *
   * @param session - Active Zenoh session
   * @param topic - The full topic string
   */
  async discoverSchemaForTopic(session: Session, topic: string): Promise<boolean> {
    const prefix = extractTopicPrefix(topic);
    if (prefix && !this.succeededPrefixes.has(prefix)) {
      const found = await this.fetchNodeSchema(session, prefix);
      if (found) return true;
    }
    // Prefix-based query failed — topic path may not match node name
    // (e.g., camera/* topics vs rtsp-camera/schema, weather/* vs openmeteo/schema).
    // Fall back to wildcard discovery which finds all node schemas.
    const count = await this.discoverAllNodeSchemas(session);
    return count > 0;
  }

  /**
   * Decode a protobuf message using a fully qualified type name.
   *
   * @param typeName - Fully qualified type name (e.g., "bubbaloop.weather.v1.CurrentWeather")
   * @param data - Raw protobuf bytes
   * @returns Decoded result or null if type not found
   */
  decode(typeName: string, data: Uint8Array): DecodeResult | null {
    for (const [, schema] of this.schemas) {
      try {
        const messageType = schema.root.lookupType(typeName);
        if (messageType) {
          const message = messageType.decode(data);
          const obj = messageType.toObject(message, {
            longs: String,
            enums: String,
            bytes: String,
            defaults: true,
          });
          return {
            data: snakeToCamel(obj) as Record<string, unknown>,
            typeName,
            source: schema.source,
          };
        }
      } catch {
        // Type not found in this root, try next
        continue;
      }
    }
    return null;
  }

  /**
   * Try to decode a protobuf message by attempting all known message types.
   * Useful when the type name is unknown (e.g., vanilla zenoh topics without ros-z schema hints).
   *
   * @param data - Raw protobuf bytes
   * @returns Decoded result or null if no type matches
   */
  tryDecodeAny(data: Uint8Array): DecodeResult | null {
    for (const [, schema] of this.schemas) {
      const types: string[] = [];
      collectTypeNames(schema.root, '', types);
      for (const typeName of types) {
        try {
          const messageType = schema.root.lookupType(typeName);
          if (!messageType) continue;
          // Skip types with no fields (e.g., empty wrapper messages)
          if (messageType.fieldsArray.length === 0) continue;
          const message = messageType.decode(data);
          const obj = messageType.toObject(message, {
            longs: String,
            enums: String,
            bytes: String,
            defaults: true,
          });
          // Heuristic: a valid decode should have at least one non-default field set
          const hasContent = Object.values(obj).some(v =>
            v !== '' && v !== 0 && v !== '0' && v !== false && v !== null &&
            !(Array.isArray(v) && v.length === 0)
          );
          if (hasContent) {
            return {
              data: snakeToCamel(obj) as Record<string, unknown>,
              typeName,
              source: schema.source,
            };
          }
        } catch {
          continue;
        }
      }
    }
    return null;
  }

  /**
   * Consolidated decode chain for a given topic and payload.
   * Tries: ros-z type hint -> topic-based guess -> brute force.
   *
   * @param topic - The full Zenoh topic string
   * @param data - Raw protobuf bytes
   * @returns Decoded result or null if no type matches
   */
  tryDecodeForTopic(topic: string, data: Uint8Array): DecodeResult | null {
    // 1. Try ros-z type hint embedded in topic
    const schemaHint = extractSchemaFromTopic(topic);
    if (schemaHint) {
      const result = this.decode(schemaHint, data);
      if (result) return result;
    }

    // 2. Infer type from topic path segments
    const guessedType = this.guessTypeForTopic(topic);
    if (guessedType) {
      const result = this.decode(guessedType, data);
      if (result) return result;
    }

    // 3. Brute force: try all known types
    return this.tryDecodeAny(data);
  }

  /**
   * Get all known type names across all loaded schemas.
   */
  getTypeNames(): string[] {
    if (this.typeNamesCache) return this.typeNamesCache;
    const types: string[] = [];
    for (const [, schema] of this.schemas) {
      collectTypeNames(schema.root, '', types);
    }
    this.typeNamesCache = [...new Set(types)];
    return this.typeNamesCache;
  }

  /**
   * Look up a protobufjs Type by fully qualified name.
   * Useful when callers need direct access (e.g., for raw byte fields or encoding).
   */
  lookupType(typeName: string): protobuf.Type | null {
    for (const [, schema] of this.schemas) {
      try {
        const t = schema.root.lookupType(typeName);
        if (t) return t;
      } catch {
        continue;
      }
    }
    return null;
  }

  /**
   * Guess the most likely protobuf type for a vanilla zenoh topic.
   * Infers from the topic path segments by matching against known type names.
   *
   * E.g., "bubbaloop/local/m1/network-monitor/status" → "bubbaloop.network_monitor.v1.NetworkStatus"
   */
  guessTypeForTopic(topic: string): string | null {
    const cached = this.topicGuessCache.get(topic);
    if (cached !== undefined) return cached;

    const allTypes = this.getTypeNames();
    if (allTypes.length === 0) return null;

    // Extract meaningful path segments from the topic
    const parts = topic.split('/');
    // Normalize: remove "bubbaloop" prefix and convert dashes to underscores
    const normalized = parts
      .filter(s => s !== 'bubbaloop')
      .map(s => s.replace(/-/g, '_').toLowerCase());

    // Try to find a type whose package/name matches topic segments
    // Score types by how many segments match
    let bestType: string | null = null;
    let bestScore = 0;

    for (const typeName of allTypes) {
      const typeLower = typeName.toLowerCase();
      let score = 0;
      for (const seg of normalized) {
        if (seg.length >= 3 && typeLower.includes(seg)) {
          score += seg.length; // longer segment matches are better
        }
      }
      if (score > bestScore) {
        bestScore = score;
        bestType = typeName;
      }
    }

    this.topicGuessCache.set(topic, bestType);
    return bestType;
  }

  /** Check if core schemas have been loaded. */
  get isCoreLoaded(): boolean {
    return this.coreLoaded;
  }

  /** Get the number of loaded schema sources. */
  get sourceCount(): number {
    return this.schemas.size;
  }

  /** Clear all loaded schemas and reset state. */
  clear(): void {
    this.schemas.clear();
    this.succeededPrefixes.clear();
    this.failedPrefixes.clear();
    this.coreLoaded = false;
    this.typeNamesCache = null;
    this.topicGuessCache.clear();
  }
}

const DANGEROUS_KEYS = new Set(['__proto__', 'constructor', 'prototype']);

/**
 * Recursively convert snake_case object keys to camelCase.
 * protobufjs toObject() returns proto field names (snake_case),
 * but JavaScript convention is camelCase.
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

/** Recursively collect fully qualified type names from a protobufjs namespace. */
function collectTypeNames(ns: protobuf.NamespaceBase, prefix: string, out: string[]): void {
  for (const nested of ns.nestedArray) {
    const fullName = prefix ? `${prefix}.${nested.name}` : nested.name;
    if (nested instanceof protobuf.Type) {
      out.push(fullName);
    }
    if (nested instanceof protobuf.Namespace || nested instanceof protobuf.Type) {
      collectTypeNames(nested, fullName, out);
    }
  }
}

/**
 * Extract a scoped topic prefix that might correspond to a node's schema endpoint.
 *
 * Attempts to extract the first 4 segments (bubbaloop/{scope}/{machine}/{node-or-resource})
 * from a topic. This works when the 4th segment is the actual node name, but may fail when
 * the 4th segment is a topic resource name instead:
 *   - "bubbaloop/local/m1/camera/my-cam/compressed" → prefix is "bubbaloop/local/m1/camera"
 *     but the schema is actually at "bubbaloop/local/m1/rtsp-camera/schema"
 *   - "bubbaloop/local/m1/weather/current" → prefix is "bubbaloop/local/m1/weather"
 *     but the schema is actually at "bubbaloop/local/m1/openmeteo/schema"
 *
 * This is a best-effort heuristic. Callers should fall back to wildcard discovery when
 * this prefix-based approach fails.
 *
 * For scoped topics like "bubbaloop/local/m1/system-telemetry/metrics", returns
 * "bubbaloop/local/m1/system-telemetry" so we can query "{prefix}/schema".
 * For ros-z topics like "0/bubbaloop%local%m1%node%res/Type/Hash", decodes the
 * %-encoded portion first.
 */
export function extractTopicPrefix(topic: string): string | null {
  let segments: string[];
  const parts = topic.split('/');

  if (parts.length >= 2 && /^\d+$/.test(parts[0])) {
    if (parts[1].includes('%')) {
      // Old ros-z: decode %-encoded topic portion
      segments = parts[1].replace(/%/g, '/').split('/');
    } else {
      // New ros-z: slashes preserved, skip domain ID
      segments = parts.slice(1);
    }
  } else {
    segments = parts;
  }

  // Extract first 4 segments: bubbaloop/{scope}/{machine}/{node-or-resource}
  if (segments.length >= 4 && segments[0] === 'bubbaloop') {
    return segments.slice(0, 4).join('/');
  }

  return null;
}
