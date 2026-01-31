import { createContext, useContext, useEffect, useRef, useMemo, ReactNode } from 'react';
import { Session } from '@eclipse-zenoh/zenoh-ts';
import {
  ZenohSubscriptionManager,
  TopicStats,
  MonitoredTopicStatsWithMeta,
  SampleCallback,
  EndpointConfig,
  SubscribeOptions,
} from '../lib/subscription-manager';

interface ZenohSubscriptionContextValue {
  manager: ZenohSubscriptionManager;
  /** Get the current session (useful for publishing) */
  getSession: () => Session | null;
  subscribe: (topic: string, callback: SampleCallback, endpointId?: string, options?: SubscribeOptions) => string;
  unsubscribe: (topic: string, listenerId: string, endpointId?: string, options?: SubscribeOptions) => void;
  getTopicStats: (topic: string, endpointId?: string) => TopicStats | null;
  getAllStats: () => Map<string, TopicStats>;
  getAllMonitoredStats: () => Map<string, MonitoredTopicStatsWithMeta>;
  getActiveSubscriptions: (endpointId?: string) => string[];
  getDiscoveredTopics: (endpointId?: string) => string[];
  addRemoteEndpoint: (config: EndpointConfig) => void;
  removeEndpoint: (endpointId: string) => void;
  startMonitoring: (endpointId?: string) => Promise<void>;
  stopMonitoring: (endpointId?: string) => Promise<void>;
  isMonitoringEnabled: (endpointId?: string) => boolean;
}

const ZenohSubscriptionContext = createContext<ZenohSubscriptionContextValue | null>(null);

interface ZenohSubscriptionProviderProps {
  session: Session | null;
  children: ReactNode;
}

/**
 * Provider that manages centralized Zenoh subscriptions.
 * Wrap your app/dashboard with this to enable subscription deduplication.
 */
export function ZenohSubscriptionProvider({
  session,
  children,
}: ZenohSubscriptionProviderProps) {
  const managerRef = useRef<ZenohSubscriptionManager | null>(null);
  const sessionRef = useRef<Session | null>(null);

  // Create manager once (lazy initialization)
  if (!managerRef.current) {
    managerRef.current = new ZenohSubscriptionManager();
    // Expose for debugging
    if (typeof window !== 'undefined') {
      (window as unknown as { __zenohSubManager?: ZenohSubscriptionManager }).__zenohSubManager = managerRef.current;
    }
  }

  // Keep session ref updated
  sessionRef.current = session;

  // Update manager session when it changes
  useEffect(() => {
    managerRef.current?.setSession(session);
  }, [session]);

  // Cleanup on unmount
  useEffect(() => {
    return () => {
      managerRef.current?.destroy();
    };
  }, []);

  // Memoize the context value so functions have stable references
  // This prevents unnecessary re-subscriptions in useZenohSubscription
  const value = useMemo<ZenohSubscriptionContextValue>(() => {
    const manager = managerRef.current!;
    return {
      manager,
      getSession: () => sessionRef.current,
      subscribe: (topic, callback, endpointId, options) => manager.subscribe(topic, callback, endpointId, options),
      unsubscribe: (topic, listenerId, endpointId, options) => manager.unsubscribe(topic, listenerId, endpointId, options),
      getTopicStats: (topic, endpointId) => manager.getTopicStats(topic, endpointId),
      getAllStats: () => manager.getAllStats(),
      getAllMonitoredStats: () => manager.getAllMonitoredStats(),
      getActiveSubscriptions: (endpointId) => manager.getActiveSubscriptions(endpointId),
      getDiscoveredTopics: (endpointId) => manager.getDiscoveredTopics(endpointId),
      addRemoteEndpoint: (config) => manager.addRemoteEndpoint(config),
      removeEndpoint: (endpointId) => manager.removeEndpoint(endpointId),
      startMonitoring: (endpointId) => manager.startMonitoring(endpointId),
      stopMonitoring: (endpointId) => manager.stopMonitoring(endpointId),
      isMonitoringEnabled: (endpointId) => manager.isMonitoringEnabled(endpointId),
    };
  }, []); // Empty deps - manager is created once via ref

  return (
    <ZenohSubscriptionContext.Provider value={value}>
      {children}
    </ZenohSubscriptionContext.Provider>
  );
}

/**
 * Hook to access the subscription context.
 * Must be used within a ZenohSubscriptionProvider.
 */
export function useZenohSubscriptionContext(): ZenohSubscriptionContextValue {
  const context = useContext(ZenohSubscriptionContext);
  if (!context) {
    throw new Error(
      'useZenohSubscriptionContext must be used within a ZenohSubscriptionProvider'
    );
  }
  return context;
}
