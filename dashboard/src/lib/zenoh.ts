import { Session, Config, Subscriber, Sample } from '@eclipse-zenoh/zenoh-ts';
import { useEffect, useRef, useState, useCallback } from 'react';

export interface ZenohConfig {
  endpoint: string; // e.g., 'ws://127.0.0.1:10000'
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

export interface UseTopicDiscoveryResult {
  topics: string[];
  isDiscovering: boolean;
  refresh: () => void;
}

/**
 * React hook for discovering available Zenoh topics
 * Subscribes to a broad pattern and collects unique key expressions
 */
export function useZenohTopicDiscovery(
  session: Session | null,
  pattern: string = '**'
): UseTopicDiscoveryResult {
  const [topics, setTopics] = useState<string[]>([]);
  const [isDiscovering, setIsDiscovering] = useState(false);
  const topicsSetRef = useRef<Set<string>>(new Set());
  const subscriberRef = useRef<Subscriber | null>(null);

  const discover = useCallback(async () => {
    if (!session) return;

    setIsDiscovering(true);
    topicsSetRef.current.clear();
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
          // Filter for compressed image topics
          if (keyExpr.includes('compressed') && !topicsSetRef.current.has(keyExpr)) {
            topicsSetRef.current.add(keyExpr);
            setTopics(Array.from(topicsSetRef.current).sort());
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
