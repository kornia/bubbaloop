/**
 * React context for the dynamic protobuf SchemaRegistry.
 *
 * Wraps the SchemaRegistry class, auto-discovers node schemas on session connect,
 * and exposes hooks for components to decode messages dynamically.
 *
 * Phase 3 changes:
 * - Removed periodic 10s/30s re-discovery polling loop
 * - Removed fetchCoreSchemas() call (daemon now sends JSON, not protobuf)
 * - Added decodeWithEncoding() to context for encoding-first decode path
 * - On-demand schema fetch triggered by first sample with unknown protobuf type
 */

import { createContext, useContext, useEffect, useRef, useState, useCallback, useMemo, type ReactNode } from 'react';
import { Session } from '@eclipse-zenoh/zenoh-ts';
import { SchemaRegistry, DecodeResult } from '../lib/schema-registry';
import { EncodingInfo } from '../lib/zenoh';
import { useFleetContext } from './FleetContext';

interface SchemaRegistryContextValue {
  /** The underlying SchemaRegistry instance */
  registry: SchemaRegistry;
  /** Whether schemas are currently being loaded */
  loading: boolean;
  /** Error from the last fetch attempt, if any */
  error: string | null;
  /** Re-fetch all schemas */
  refresh: () => void;
  /** Decode a protobuf message by type name */
  decode: (typeName: string, data: Uint8Array) => DecodeResult | null;
  /** Trigger schema discovery for a topic */
  discoverForTopic: (topic: string) => void;
  /** Increments when new schemas are discovered — consumers can re-trigger decoding */
  schemaVersion: number;
  /**
   * Primary encoding-first decode method.
   * Reads encoding metadata and routes to the correct decoder.
   * Falls back to sniff chain for legacy nodes without encoding.
   */
  decodeWithEncoding: (
    payload: Uint8Array,
    encoding: EncodingInfo,
    topic: string,
  ) => Promise<DecodeResult | null>;
}

const SchemaRegistryContext = createContext<SchemaRegistryContextValue | null>(null);

interface SchemaRegistryProviderProps {
  session: Session | null;
  children: ReactNode;
}

/**
 * Provider that manages the SchemaRegistry lifecycle.
 * Discovers node schemas on session connect via wildcard query.
 * No polling — schemas are fetched on-demand when encoding signals a new protobuf type.
 */
export function SchemaRegistryProvider({ session, children }: SchemaRegistryProviderProps) {
  const registryRef = useRef<SchemaRegistry>(new SchemaRegistry());
  const sessionRef = useRef<Session | null>(null);
  const { machines } = useFleetContext();
  const [loading, setLoading] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const [schemaVersion, setSchemaVersion] = useState(0);

  // Stable machine ID list — only changes when actual IDs change, not on every status update
  const machineIdKey = useMemo(() => machines.map(m => m.machineId).sort().join(','), [machines]);

  // Keep session ref updated
  sessionRef.current = session;

  const fetchSchemas = useCallback(async () => {
    const currentSession = sessionRef.current;
    if (!currentSession) return;

    setLoading(true);
    setError(null);

    try {
      const machineIds = machineIdKey ? machineIdKey.split(',').filter(Boolean) : undefined;
      console.log(`[SchemaRegistry] fetchSchemas: machineIds=${JSON.stringify(machineIds)}`);
      // Fetch core schemas from daemon (backward compat — daemon may still serve them)
      // and discover all node schemas via wildcard query at startup.
      const [coreLoaded, nodeCount] = await Promise.all([
        registryRef.current.fetchCoreSchemas(currentSession, machineIds),
        registryRef.current.discoverAllNodeSchemas(currentSession, machineIds),
      ]);
      console.log(`[SchemaRegistry] fetchSchemas result: core=${coreLoaded} nodes=${nodeCount}`);
      if (coreLoaded || nodeCount > 0) {
        setSchemaVersion(v => v + 1);
      }
      if (!coreLoaded && nodeCount === 0) {
        setError('No schemas returned from daemon');
      }
    } catch (e) {
      setError(e instanceof Error ? e.message : 'Failed to fetch schemas');
    } finally {
      setLoading(false);
    }
  }, [machineIdKey]);

  // Auto-fetch when session connects; clear on disconnect
  useEffect(() => {
    if (session) {
      fetchSchemas();
    } else {
      registryRef.current.clear();
      setError(null);
      setSchemaVersion(0);
    }
  }, [session, fetchSchemas]);

  const decode = useCallback((typeName: string, data: Uint8Array): DecodeResult | null => {
    return registryRef.current.decode(typeName, data);
  }, []);

  const discoverForTopic = useCallback((topic: string) => {
    const currentSession = sessionRef.current;
    if (currentSession) {
      registryRef.current.discoverSchemaForTopic(currentSession, topic).then(found => {
        if (found) {
          setSchemaVersion(v => v + 1);
        }
      });
    }
  }, []);

  const decodeWithEncoding = useCallback(
    async (payload: Uint8Array, encoding: EncodingInfo, topic: string): Promise<DecodeResult | null> => {
      const currentSession = sessionRef.current;
      if (!currentSession) {
        // No session — fall back to synchronous sniff chain
        return registryRef.current.tryDecodeForTopic(topic, payload);
      }
      const result = await registryRef.current.decodeWithEncoding(payload, encoding, topic, currentSession);
      // If a new schema was fetched during decoding, bump schemaVersion
      if (result && result.source !== 'encoding' && registryRef.current.sourceCount > 0) {
        setSchemaVersion(v => v + 1);
      }
      return result;
    },
    [],
  );

  const value = useMemo<SchemaRegistryContextValue>(() => ({
    registry: registryRef.current,
    loading,
    error,
    refresh: fetchSchemas,
    decode,
    discoverForTopic,
    schemaVersion,
    decodeWithEncoding,
  }), [loading, error, fetchSchemas, decode, discoverForTopic, schemaVersion, decodeWithEncoding]);

  return (
    <SchemaRegistryContext.Provider value={value}>
      {children}
    </SchemaRegistryContext.Provider>
  );
}

/**
 * Hook to access the SchemaRegistry context.
 * Must be used within a SchemaRegistryProvider.
 */
export function useSchemaRegistry(): SchemaRegistryContextValue {
  const context = useContext(SchemaRegistryContext);
  if (!context) {
    throw new Error('useSchemaRegistry must be used within a SchemaRegistryProvider');
  }
  return context;
}
