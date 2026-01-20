import { Session, Subscriber, Sample } from '@eclipse-zenoh/zenoh-ts';

export type SampleCallback = (sample: Sample) => void;

export interface TopicStats {
  messageCount: number;
  fps: number;
  instantFps: number;
  lastSeen: number;
}

/**
 * Endpoint configuration for multi-endpoint support.
 * Currently only 'local' is implemented, but this prepares for future remote endpoints.
 */
export interface EndpointConfig {
  id: string;
  type: 'local' | 'remote';
  endpoint?: string; // WebSocket URL for remote endpoints
}

interface TopicSubscription {
  subscriber: Subscriber | null;
  listeners: Map<string, SampleCallback>;
  stats: TopicStats;
  timestampBuffer: number[]; // Buffer of message timestamps for Hz calculation
  hzCounter: number; // Messages received in current second
  pending: boolean;
  endpointId: string;
}

interface EndpointState {
  config: EndpointConfig;
  session: Session | null;
  subscriptions: Map<string, TopicSubscription>;
}

const HZ_WINDOW_MS = 2000; // Time window in ms for Hz calculation
const DEFAULT_ENDPOINT_ID = 'local';

/**
 * Centralized Zenoh subscription manager that:
 * 1. Deduplicates subscriptions - multiple listeners share one Zenoh subscriber
 * 2. On-demand subscriptions - only subscribes when listeners exist
 * 3. Auto-cleanup - unsubscribes when last listener is removed
 * 4. Multi-endpoint ready - supports future remote endpoint connections
 */
export class ZenohSubscriptionManager {
  private endpoints: Map<string, EndpointState> = new Map();
  private statsInterval: number | null = null;
  private listenerIdCounter = 0;

  constructor() {
    // Initialize default local endpoint
    this.endpoints.set(DEFAULT_ENDPOINT_ID, {
      config: { id: DEFAULT_ENDPOINT_ID, type: 'local' },
      session: null,
      subscriptions: new Map(),
    });

    this.startStatsInterval();
  }

  /**
   * Set the session for an endpoint. Default is 'local'.
   * Future: Support adding remote endpoints with their own sessions.
   */
  setSession(session: Session | null, endpointId: string = DEFAULT_ENDPOINT_ID): void {
    let endpoint = this.endpoints.get(endpointId);

    if (!endpoint) {
      // Create new endpoint entry
      endpoint = {
        config: { id: endpointId, type: endpointId === DEFAULT_ENDPOINT_ID ? 'local' : 'remote' },
        session: null,
        subscriptions: new Map(),
      };
      this.endpoints.set(endpointId, endpoint);
    }

    // If session changed
    if (endpoint.session !== session) {
      const oldSession = endpoint.session;
      endpoint.session = session;

      // Clean up old Zenoh subscribers (but keep the subscription entries with their listeners!)
      if (oldSession) {
        endpoint.subscriptions.forEach((sub) => {
          if (sub.subscriber) {
            sub.subscriber.undeclare().catch((e) => {
              console.warn('[SubscriptionManager] Error undeclaring subscriber:', e);
            });
            sub.subscriber = null;
            sub.pending = true; // Mark as pending for re-subscription
          }
        });
      }

      // Re-subscribe all topics with listeners for this endpoint
      if (session) {
        const pendingTopics = Array.from(endpoint.subscriptions.entries())
          .filter(([, sub]) => sub.listeners.size > 0 && !sub.subscriber);
        if (pendingTopics.length > 0) {
          console.log(`[SubscriptionManager] Session ready, subscribing to ${pendingTopics.length} pending topic(s)`);
          pendingTopics.forEach(([topic]) => {
            this.createSubscriber(topic, endpointId);
          });
        }
      }
    }
  }

  /**
   * Add a remote endpoint configuration (future use).
   * This prepares the architecture for connecting to remote Zenoh endpoints.
   */
  addRemoteEndpoint(config: EndpointConfig): void {
    if (this.endpoints.has(config.id)) {
      console.warn(`[SubscriptionManager] Endpoint ${config.id} already exists`);
      return;
    }

    this.endpoints.set(config.id, {
      config,
      session: null,
      subscriptions: new Map(),
    });

    console.log(`[SubscriptionManager] Added remote endpoint: ${config.id}`);
  }

  /**
   * Remove an endpoint and clean up all its subscriptions.
   */
  removeEndpoint(endpointId: string): void {
    if (endpointId === DEFAULT_ENDPOINT_ID) {
      console.warn('[SubscriptionManager] Cannot remove default local endpoint');
      return;
    }

    this.cleanupEndpointSubscriptions(endpointId);
    this.endpoints.delete(endpointId);
    console.log(`[SubscriptionManager] Removed endpoint: ${endpointId}`);
  }

  /**
   * Subscribe to a topic on a specific endpoint. Returns a listener ID for unsubscribing.
   * Subscription is created on-demand only when the first listener registers.
   */
  subscribe(
    topic: string,
    callback: SampleCallback,
    endpointId: string = DEFAULT_ENDPOINT_ID
  ): string {
    const listenerId = `listener_${++this.listenerIdCounter}`;
    const endpoint = this.endpoints.get(endpointId);

    if (!endpoint) {
      console.error(`[SubscriptionManager] Unknown endpoint: ${endpointId}`);
      return listenerId;
    }

    let subscription = endpoint.subscriptions.get(topic);

    if (!subscription) {
      // First listener for this topic - create subscription entry (lazy, no Zenoh sub yet)
      subscription = {
        subscriber: null,
        listeners: new Map(),
        stats: { messageCount: 0, fps: 0, instantFps: 0, lastSeen: 0 },
        timestampBuffer: [],
        hzCounter: 0,
        pending: true,
        endpointId,
      };
      endpoint.subscriptions.set(topic, subscription);
      console.log(`[SubscriptionManager] Topic registered: ${topic} on ${endpointId}`);
    }

    // Add listener
    subscription.listeners.set(listenerId, callback);

    // Create Zenoh subscriber on-demand if we have a session and don't have one yet
    if (endpoint.session && !subscription.subscriber && subscription.pending) {
      this.createSubscriber(topic, endpointId);
    }

    return listenerId;
  }

  /**
   * Unsubscribe a specific listener from a topic.
   * When the last listener is removed, the Zenoh subscriber is automatically cleaned up.
   */
  unsubscribe(topic: string, listenerId: string, endpointId: string = DEFAULT_ENDPOINT_ID): void {
    const endpoint = this.endpoints.get(endpointId);
    if (!endpoint) return;

    const subscription = endpoint.subscriptions.get(topic);
    if (!subscription) return;

    subscription.listeners.delete(listenerId);

    // Auto-cleanup: If no more listeners, clean up the Zenoh subscriber
    if (subscription.listeners.size === 0) {
      console.log(`[SubscriptionManager] No listeners remaining, cleaning up: ${topic}`);
      this.cleanupSubscription(topic, endpointId);
    }
  }

  /**
   * Get stats for a specific topic on an endpoint.
   */
  getTopicStats(topic: string, endpointId: string = DEFAULT_ENDPOINT_ID): TopicStats | null {
    const endpoint = this.endpoints.get(endpointId);
    return endpoint?.subscriptions.get(topic)?.stats ?? null;
  }

  /**
   * Get stats for all topics across all endpoints.
   * Only returns topics with active listeners.
   */
  getAllStats(): Map<string, TopicStats> {
    const result = new Map<string, TopicStats>();

    this.endpoints.forEach((endpoint) => {
      endpoint.subscriptions.forEach((sub, topic) => {
        // Only include topics with active listeners
        if (sub.listeners.size === 0) {
          return;
        }

        // Include endpoint prefix for non-local endpoints
        const key = endpoint.config.id === DEFAULT_ENDPOINT_ID
          ? topic
          : `[${endpoint.config.id}] ${topic}`;
        result.set(key, { ...sub.stats });
      });
    });

    return result;
  }

  /**
   * Get list of active subscriptions (topics with at least one listener).
   */
  getActiveSubscriptions(endpointId?: string): string[] {
    const topics: string[] = [];

    const processEndpoint = (endpoint: EndpointState) => {
      endpoint.subscriptions.forEach((sub, topic) => {
        if (sub.listeners.size > 0) {
          topics.push(topic);
        }
      });
    };

    if (endpointId) {
      const endpoint = this.endpoints.get(endpointId);
      if (endpoint) processEndpoint(endpoint);
    } else {
      this.endpoints.forEach(processEndpoint);
    }

    return topics;
  }

  /**
   * Check if a topic has any active listeners.
   */
  hasListeners(topic: string, endpointId: string = DEFAULT_ENDPOINT_ID): boolean {
    const endpoint = this.endpoints.get(endpointId);
    const subscription = endpoint?.subscriptions.get(topic);
    return (subscription?.listeners.size ?? 0) > 0;
  }

  /**
   * Get listener count for a topic.
   */
  getListenerCount(topic: string, endpointId: string = DEFAULT_ENDPOINT_ID): number {
    const endpoint = this.endpoints.get(endpointId);
    return endpoint?.subscriptions.get(topic)?.listeners.size ?? 0;
  }

  /**
   * Clean up all resources.
   */
  destroy(): void {
    if (this.statsInterval !== null) {
      clearInterval(this.statsInterval);
      this.statsInterval = null;
    }

    this.endpoints.forEach((_, endpointId) => {
      this.cleanupEndpointSubscriptions(endpointId);
    });
  }

  private async createSubscriber(topic: string, endpointId: string): Promise<void> {
    const endpoint = this.endpoints.get(endpointId);
    if (!endpoint) return;

    const subscription = endpoint.subscriptions.get(topic);
    if (!subscription || !endpoint.session) return;

    subscription.pending = false;

    try {
      const subscriber = await endpoint.session.declareSubscriber(topic, {
        handler: (sample: Sample) => {
          this.handleSample(topic, sample, endpointId);
        },
      });

      subscription.subscriber = subscriber;
      console.log(`[SubscriptionManager] Subscribed to ${topic} on ${endpointId} (${subscription.listeners.size} listeners)`);
    } catch (e) {
      console.error(`[SubscriptionManager] Failed to subscribe to ${topic}:`, e);
      subscription.pending = true; // Allow retry
    }
  }

  private handleSample(topic: string, sample: Sample, endpointId: string): void {
    const endpoint = this.endpoints.get(endpointId);
    if (!endpoint) return;

    const subscription = endpoint.subscriptions.get(topic);
    if (!subscription) return;

    const now = performance.now();

    // Update message count
    subscription.stats.messageCount++;
    subscription.stats.lastSeen = Date.now();

    // Increment Hz counter (reset every second by stats interval)
    subscription.hzCounter++;

    // Dispatch to all listeners
    subscription.listeners.forEach((callback) => {
      try {
        callback(sample);
      } catch (e) {
        console.error(`[SubscriptionManager] Listener error for ${topic}:`, e);
      }
    });
  }

  private async cleanupSubscription(topic: string, endpointId: string): Promise<void> {
    const endpoint = this.endpoints.get(endpointId);
    if (!endpoint) return;

    const subscription = endpoint.subscriptions.get(topic);
    if (!subscription) return;

    if (subscription.subscriber) {
      try {
        await subscription.subscriber.undeclare();
        console.log(`[SubscriptionManager] Unsubscribed from ${topic} on ${endpointId}`);
      } catch (e) {
        console.error(`[SubscriptionManager] Failed to undeclare ${topic}:`, e);
      }
    }

    endpoint.subscriptions.delete(topic);
  }

  private async cleanupEndpointSubscriptions(endpointId: string): Promise<void> {
    const endpoint = this.endpoints.get(endpointId);
    if (!endpoint) return;

    const topics = Array.from(endpoint.subscriptions.keys());
    for (const topic of topics) {
      const subscription = endpoint.subscriptions.get(topic);
      if (subscription?.subscriber) {
        try {
          await subscription.subscriber.undeclare();
        } catch (e) {
          // Ignore errors during cleanup
        }
      }
    }
    endpoint.subscriptions.clear();
  }

  private startStatsInterval(): void {
    // Every second: calculate Hz from counter and reset
    this.statsInterval = window.setInterval(() => {
      const now = Date.now();
      this.endpoints.forEach((endpoint) => {
        const toDelete: string[] = [];

        endpoint.subscriptions.forEach((subscription, topic) => {
          // Clean up subscriptions with no listeners
          if (subscription.listeners.size === 0) {
            toDelete.push(topic);
            return;
          }

          // Calculate Hz from messages received in last second
          subscription.stats.fps = subscription.hzCounter;
          subscription.stats.instantFps = subscription.hzCounter;

          // Debug log
          if (subscription.hzCounter > 0) {
            console.log(`[Hz] ${topic}: ${subscription.hzCounter} msgs/sec`);
          }

          // Reset counter for next second
          subscription.hzCounter = 0;

          // If no message in last 2 seconds, ensure Hz is 0
          if (now - subscription.stats.lastSeen > 2000) {
            subscription.stats.fps = 0;
            subscription.stats.instantFps = 0;
          }
        });

        // Delete orphaned subscriptions
        toDelete.forEach((topic) => {
          const sub = endpoint.subscriptions.get(topic);
          if (sub?.subscriber) {
            sub.subscriber.undeclare().catch(() => {});
          }
          endpoint.subscriptions.delete(topic);
        });
      });
    }, 1000);
  }
}

// Singleton instance for global access (optional, Context is preferred)
let globalManager: ZenohSubscriptionManager | null = null;

export function getGlobalSubscriptionManager(): ZenohSubscriptionManager {
  if (!globalManager) {
    globalManager = new ZenohSubscriptionManager();
  }
  return globalManager;
}
