/**
 * React context for the dynamic protobuf SchemaRegistry.
 *
 * Wraps the SchemaRegistry class, auto-fetches core schemas on session connect,
 * and exposes a hook for components to decode messages dynamically.
 */

import { createContext, useContext, useEffect, useRef, useState, useCallback, type ReactNode } from 'react';
import { Session } from '@eclipse-zenoh/zenoh-ts';
import { SchemaRegistry, DecodeResult } from '../lib/schema-registry';

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
  const [loading, setLoading] = useState(false);
  const [error, setError] = useState<string | null>(null);

  // Keep session ref updated
  sessionRef.current = session;

  const fetchSchemas = useCallback(async () => {
    const currentSession = sessionRef.current;
    if (!currentSession) return;

    setLoading(true);
    setError(null);

    try {
      const success = await registryRef.current.fetchCoreSchemas(currentSession);
      if (!success) {
        setError('No schemas returned from daemon');
      }
    } catch (e) {
      setError(e instanceof Error ? e.message : 'Failed to fetch schemas');
    } finally {
      setLoading(false);
    }
  }, []);

  // Auto-fetch when session connects
  useEffect(() => {
    if (session) {
      fetchSchemas();
    } else {
      registryRef.current.clear();
      setError(null);
    }
  }, [session, fetchSchemas]);

  const decode = useCallback((typeName: string, data: Uint8Array): DecodeResult | null => {
    return registryRef.current.decode(typeName, data);
  }, []);

  const discoverForTopic = useCallback((topic: string) => {
    const currentSession = sessionRef.current;
    if (currentSession) {
      registryRef.current.discoverSchemaForTopic(currentSession, topic);
    }
  }, []);

  const value: SchemaRegistryContextValue = {
    registry: registryRef.current,
    loading,
    error,
    refresh: fetchSchemas,
    decode,
    discoverForTopic,
  };

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
