/**
 * React context for the dynamic protobuf SchemaRegistry.
 *
 * Wraps the SchemaRegistry class, auto-fetches core schemas on session connect,
 * and exposes a hook for components to decode messages dynamically.
 */

import { createContext, useContext, useEffect, useRef, useState, useCallback, useMemo, type ReactNode } from 'react';
import { Session } from '@eclipse-zenoh/zenoh-ts';
import { SchemaRegistry, DecodeResult } from '../lib/schema-registry';
import { useFleetContext } from './FleetContext';

interface SchemaRegistryContextValue {
  /** The underlying SchemaRegistry instance */
  registry: SchemaRegistry;
  /** Whether core schemas are currently being loaded */
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
}

const SchemaRegistryContext = createContext<SchemaRegistryContextValue | null>(null);

interface SchemaRegistryProviderProps {
  session: Session | null;
  children: ReactNode;
}

/**
 * Provider that manages the SchemaRegistry lifecycle.
 * Auto-fetches core schemas when a Zenoh session becomes available.
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
      // Fetch core schemas and discover node schemas in parallel
      const [coreSuccess, nodeCount] = await Promise.all([
        registryRef.current.fetchCoreSchemas(currentSession, machineIds),
        registryRef.current.discoverAllNodeSchemas(currentSession, machineIds),
      ]);
      if (!coreSuccess) {
        setError('No schemas returned from daemon');
      }
      if (coreSuccess || nodeCount > 0) {
        setSchemaVersion(v => v + 1);
      }
      if (nodeCount > 0) {
        console.log(`[SchemaRegistry] Discovered ${nodeCount} node schema(s)`);
      }
    } catch (e) {
      setError(e instanceof Error ? e.message : 'Failed to fetch schemas');
    } finally {
      setLoading(false);
    }
  }, [machineIdKey]);

  // Auto-fetch when session connects
  useEffect(() => {
    if (session) {
      fetchSchemas();
    } else {
      registryRef.current.clear();
      setError(null);
      setSchemaVersion(0);
    }
  }, [session, fetchSchemas]);

  // Periodic re-discovery: 10s interval, backs off to 30s after 3 consecutive empty cycles
  useEffect(() => {
    if (!session) return;

    let emptyConsecutive = 0;
    let timer: ReturnType<typeof setTimeout>;

    const scheduleNext = () => {
      const interval = emptyConsecutive >= 3 ? 30_000 : 10_000;
      timer = setTimeout(runDiscovery, interval);
    };

    const runDiscovery = async () => {
      const currentSession = sessionRef.current;
      if (!currentSession) {
        scheduleNext();
        return;
      }

      try {
        const machineIds = machineIdKey ? machineIdKey.split(',').filter(Boolean) : undefined;
        const count = await registryRef.current.discoverAllNodeSchemas(currentSession, machineIds);
        if (count > 0) {
          console.log(`[SchemaRegistry] Re-discovery found ${count} new schema(s)`);
          setSchemaVersion(v => v + 1);
          emptyConsecutive = 0;
        } else {
          emptyConsecutive++;
        }
      } catch {
        // Ignore — will retry on next cycle
      }
      scheduleNext();
    };

    // Start first re-discovery after initial 5s
    timer = setTimeout(runDiscovery, 5_000);

    return () => clearTimeout(timer);
  }, [session, machineIdKey]);

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

  const value = useMemo<SchemaRegistryContextValue>(() => ({
    registry: registryRef.current,
    loading,
    error,
    refresh: fetchSchemas,
    decode,
    discoverForTopic,
    schemaVersion,
  }), [loading, error, fetchSchemas, decode, discoverForTopic, schemaVersion]);

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
