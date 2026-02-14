import { useSchemaRegistry } from '../contexts/SchemaRegistryContext';

/**
 * Returns true once schemas have loaded at least once (schemaVersion > 0).
 * Components can use this to gate subscriptions or know when decode is possible.
 */
export function useSchemaReady(): boolean {
  const { schemaVersion } = useSchemaRegistry();
  return schemaVersion > 0;
}
