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
 * Gracefully close a Zenoh session with timeout
 */
async function closeSessionGracefully(session: Session | null, timeoutMs = 1000): Promise<void> {
  if (!session) return;

  try {
    // Race between close and timeout
    await Promise.race([
      session.close(),
      new Promise((_, reject) =>
        setTimeout(() => reject(new Error('Close timeout')), timeoutMs)
      ),
    ]);
  } catch {
    // Ignore close errors - session may already be closed
  }
}

/**
 * React hook for managing a Zenoh session
 */
export function useZenohSession(config: ZenohConfig): UseZenohSessionResult {
  const [session, setSession] = useState<Session | null>(null);
  const [status, setStatus] = useState<ConnectionStatus>('disconnected');
  const [error, setError] = useState<Error | null>(null);
  const sessionRef = useRef<Session | null>(null);
  const isClosingRef = useRef(false);

  const connect = useCallback(async () => {
    // Prevent reconnect while closing
    if (isClosingRef.current) return;

    if (sessionRef.current) {
      await closeSessionGracefully(sessionRef.current);
      sessionRef.current = null;
      setSession(null);
    }

    setStatus('connecting');
    setError(null);

    try {
      const zenohConfig = new Config(config.endpoint);
      const newSession = await Session.open(zenohConfig);

      // Check if we were closed while connecting
      if (isClosingRef.current) {
        await closeSessionGracefully(newSession);
        return;
      }

      sessionRef.current = newSession;
      setSession(newSession);
      setStatus('connected');
    } catch (e) {
      if (isClosingRef.current) return; // Ignore errors during shutdown

      const err = e instanceof Error ? e : new Error(String(e));
      setError(err);
      setStatus('error');
      console.error('[Zenoh] Connection failed:', err);
    }
  }, [config.endpoint]);

  useEffect(() => {
    isClosingRef.current = false;
    connect();

    // Cleanup on unmount
    const cleanup = () => {
      isClosingRef.current = true;
      if (sessionRef.current) {
        closeSessionGracefully(sessionRef.current);
        sessionRef.current = null;
      }
    };

    // Handle page close/refresh
    const handleBeforeUnload = () => {
      cleanup();
    };

    // Handle visibility change (tab switch, minimize)
    const handleVisibilityChange = () => {
      if (document.visibilityState === 'hidden') {
        // Page is hidden - could pause heavy operations here
      }
    };

    window.addEventListener('beforeunload', handleBeforeUnload);
    document.addEventListener('visibilitychange', handleVisibilityChange);

    return () => {
      window.removeEventListener('beforeunload', handleBeforeUnload);
      document.removeEventListener('visibilitychange', handleVisibilityChange);
      cleanup();
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
 * Gracefully undeclare a subscriber with timeout
 */
async function undeclareSubscriberGracefully(subscriber: Subscriber | null, timeoutMs = 500): Promise<void> {
  if (!subscriber) return;

  try {
    await Promise.race([
      subscriber.undeclare(),
      new Promise((_, reject) =>
        setTimeout(() => reject(new Error('Undeclare timeout')), timeoutMs)
      ),
    ]);
  } catch {
    // Ignore undeclare errors
  }
}

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
    let currentSubscriber: Subscriber | null = null;

    const setupSubscriber = async () => {
      try {
        // Clean up previous subscriber
        if (subscriberRef.current) {
          await undeclareSubscriberGracefully(subscriberRef.current);
          subscriberRef.current = null;
        }

        // Check if still mounted before creating new subscriber
        if (!mounted) return;

        // Use callback-based handler
        const subscriber = await session.declareSubscriber(topic, {
          handler: (sample: Sample) => {
            if (!mounted) return;

            messageCountRef.current++;
            setMessageCount(messageCountRef.current);

            try {
              onSampleRef.current?.(sample);
            } catch (e) {
              // Don't let callback errors break the subscriber
              console.error('[Zenoh] Sample callback error:', e);
            }
          },
        });

        if (!mounted) {
          // Component unmounted during subscribe
          await undeclareSubscriberGracefully(subscriber);
          return;
        }

        currentSubscriber = subscriber;
        subscriberRef.current = subscriber;
        console.log(`[Zenoh] Subscribed to ${topic}`);
      } catch (e) {
        if (!mounted) return; // Ignore errors during cleanup
        console.error(`[Zenoh] Failed to subscribe to ${topic}:`, e);
      }
    };

    setupSubscriber();

    // FPS counter with moving average
    let lastCount = 0;
    fpsIntervalRef.current = window.setInterval(() => {
      if (!mounted) return;

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
        fpsIntervalRef.current = null;
      }

      // Clean up subscriber
      const sub = currentSubscriber || subscriberRef.current;
      if (sub) {
        undeclareSubscriberGracefully(sub);
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
  const mountedRef = useRef(true);

  const discover = useCallback(async () => {
    if (!session || !mountedRef.current) return;

    setIsDiscovering(true);
    topicsSetRef.current.clear();
    setTopics([]);

    try {
      // Clean up previous subscriber
      if (subscriberRef.current) {
        await undeclareSubscriberGracefully(subscriberRef.current);
        subscriberRef.current = null;
      }

      if (!mountedRef.current) return;

      // Subscribe to pattern to discover topics
      const subscriber = await session.declareSubscriber(pattern, {
        handler: (sample: Sample) => {
          if (!mountedRef.current) return;

          try {
            const keyExpr = sample.keyexpr().toString();
            // Filter for compressed image topics
            if (keyExpr.includes('compressed') && !topicsSetRef.current.has(keyExpr)) {
              topicsSetRef.current.add(keyExpr);
              setTopics(Array.from(topicsSetRef.current).sort());
            }
          } catch {
            // Ignore errors extracting key expression
          }
        },
      });

      if (!mountedRef.current) {
        await undeclareSubscriberGracefully(subscriber);
        return;
      }

      subscriberRef.current = subscriber;
      console.log(`[Zenoh] Topic discovery started with pattern: ${pattern}`);

      // Stop discovering after a short period but keep subscriber for new topics
      setTimeout(() => {
        if (mountedRef.current) {
          setIsDiscovering(false);
        }
      }, 3000);
    } catch (e) {
      if (!mountedRef.current) return;
      console.error('[Zenoh] Topic discovery failed:', e);
      setIsDiscovering(false);
    }
  }, [session, pattern]);

  // Start discovery when session is available
  useEffect(() => {
    mountedRef.current = true;

    if (session) {
      discover();
    }

    return () => {
      mountedRef.current = false;

      if (subscriberRef.current) {
        undeclareSubscriberGracefully(subscriberRef.current);
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
