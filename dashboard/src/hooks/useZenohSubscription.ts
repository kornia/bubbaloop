import { useEffect, useRef, useState } from 'react';
import { Sample } from '@eclipse-zenoh/zenoh-ts';
import { useZenohSubscriptionContext } from '../contexts/ZenohSubscriptionContext';
import { TopicStats } from '../lib/subscription-manager';

export type SampleCallback = (sample: Sample) => void;

export interface UseZenohSubscriptionResult {
  messageCount: number;
  fps: number;
  instantFps: number;
}

export interface UseZenohSubscriptionOptions {
  /** Optional endpoint ID for multi-endpoint support. Defaults to 'local'. */
  endpointId?: string;
}

/**
 * Hook for subscribing to a Zenoh topic using the centralized subscription manager.
 * Multiple components using this hook with the same topic will share a single subscriber.
 * Subscriptions are created on-demand and automatically cleaned up when unused.
 *
 * @param topic - The Zenoh topic pattern to subscribe to
 * @param onSample - Optional callback for processing incoming samples
 * @param options - Optional configuration (endpointId for multi-endpoint support)
 * @returns Stats about the subscription (messageCount, fps, instantFps)
 */
export function useZenohSubscription(
  topic: string,
  onSample?: SampleCallback,
  options?: UseZenohSubscriptionOptions
): UseZenohSubscriptionResult {
  const { subscribe, unsubscribe, getTopicStats } = useZenohSubscriptionContext();
  const [stats, setStats] = useState<TopicStats>({
    messageCount: 0,
    fps: 0,
    instantFps: 0,
    lastSeen: 0,
  });
  const listenerIdRef = useRef<string | null>(null);
  const onSampleRef = useRef(onSample);
  const endpointId = options?.endpointId;

  // Keep callback ref updated without triggering re-subscription
  useEffect(() => {
    onSampleRef.current = onSample;
  }, [onSample]);

  // Subscribe/unsubscribe effect
  useEffect(() => {
    if (!topic) return;

    // Create callback wrapper that uses the ref
    const callback = (sample: Sample) => {
      onSampleRef.current?.(sample);
    };

    // Subscribe and store listener ID (on-demand subscription)
    listenerIdRef.current = subscribe(topic, callback, endpointId);

    // Poll stats periodically
    const statsInterval = setInterval(() => {
      const topicStats = getTopicStats(topic, endpointId);
      if (topicStats) {
        setStats(topicStats);
      }
    }, 1000);

    // Cleanup: unsubscribe when component unmounts or topic changes
    // This triggers auto-cleanup in the manager if this was the last listener
    return () => {
      clearInterval(statsInterval);
      if (listenerIdRef.current) {
        unsubscribe(topic, listenerIdRef.current, endpointId);
        listenerIdRef.current = null;
      }
    };
  }, [topic, endpointId, subscribe, unsubscribe, getTopicStats]);

  return {
    messageCount: stats.messageCount,
    fps: stats.fps,
    instantFps: stats.instantFps,
  };
}

/**
 * Hook for getting stats for all topics.
 * Useful for stats panels that need to display info about all active subscriptions.
 *
 * @param pollInterval - How often to update stats (default: 1000ms)
 * @returns Map of topic to stats
 */
export function useAllTopicStats(pollInterval = 1000): Map<string, TopicStats> {
  const { getAllStats } = useZenohSubscriptionContext();
  const [stats, setStats] = useState<Map<string, TopicStats>>(new Map());

  useEffect(() => {
    // Initial fetch
    setStats(getAllStats());

    // Poll periodically
    const interval = setInterval(() => {
      setStats(new Map(getAllStats()));
    }, pollInterval);

    return () => clearInterval(interval);
  }, [getAllStats, pollInterval]);

  return stats;
}
