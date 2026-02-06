/**
 * Dynamic Protobuf Schema Registry
 *
 * Fetches FileDescriptorSet from Zenoh queryables (daemon and individual nodes),
 * then decodes arbitrary protobuf messages at runtime using protobufjs.
 */

import * as protobuf from 'protobufjs';
import { Session, Reply, ReplyError, Sample } from '@eclipse-zenoh/zenoh-ts';
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
  /** Set of node prefixes we've already queried for schemas */
  private queriedPrefixes = new Set<string>();
  /** Whether core schemas have been fetched */
  private coreLoaded = false;

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
  async fetchCoreSchemas(session: Session): Promise<boolean> {
    try {
      const receiver = await session.get('*/daemon/api/schemas', {
        timeout: Duration.milliseconds.of(5000),
      });

      let loaded = false;
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

      this.coreLoaded = loaded;
      return loaded;
    } catch (e) {
      console.error('[SchemaRegistry] Failed to fetch core schemas:', e);
      return false;
    }
  }

  /**
   * Fetch schemas from a specific node's schema queryable.
   *
   * @param session - Active Zenoh session
   * @param prefix - The node topic prefix (e.g., "my-node")
   * @returns true if schemas were fetched successfully
   */
  async fetchNodeSchema(session: Session, prefix: string): Promise<boolean> {
    if (this.queriedPrefixes.has(prefix)) return false;
    this.queriedPrefixes.add(prefix);

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

      return loaded;
    } catch {
      // Not all nodes have schema queryables — this is expected
      return false;
    }
  }

  /**
   * Try to discover schemas for a new topic prefix.
   * Called when the dashboard encounters a topic it hasn't seen before.
   *
   * @param session - Active Zenoh session
   * @param topic - The full topic string
   */
  async discoverSchemaForTopic(session: Session, topic: string): Promise<void> {
    const prefix = extractTopicPrefix(topic);
    if (prefix && !this.queriedPrefixes.has(prefix)) {
      await this.fetchNodeSchema(session, prefix);
    }
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
            data: obj as Record<string, unknown>,
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
   * Get all known type names across all loaded schemas.
   */
  getTypeNames(): string[] {
    const types: string[] = [];
    for (const [, schema] of this.schemas) {
      collectTypeNames(schema.root, '', types);
    }
    return [...new Set(types)];
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
    this.queriedPrefixes.clear();
    this.coreLoaded = false;
  }
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
 * Extract a topic prefix that might correspond to a node name.
 * For standard topics like "my-node/output", returns "my-node".
 * For ros-z topics like "0/my-node%output/Type/Hash", attempts to extract the node portion.
 */
function extractTopicPrefix(topic: string): string | null {
  const parts = topic.split('/');

  // Skip ros-z domain ID if present (first part is a number)
  const startIdx = parts[0]?.match(/^\d+$/) ? 1 : 0;

  if (parts.length > startIdx + 1) {
    const candidate = parts[startIdx];
    // For ros-z encoded topics, decode % separators
    if (candidate.includes('%')) {
      const segments = candidate.split('%');
      return segments.join('/');
    }
    return candidate;
  }

  return null;
}
