import { Session, Subscriber, Sample } from '@eclipse-zenoh/zenoh-ts';

export type SampleCallback = (sample: Sample) => void;

/**
 * Normalize a topic to a canonical base pattern (without wildcards).
 * This ensures that full key expressions and wildcard patterns that match
 * the same keys are deduplicated to use the same subscription.
 *
 * Handles two formats:
 * 1. ros-z format: {domain_id}/{topic_encoded}/{type_name}/{type_hash}
 *    Example: "0/camera%terrace%raw_shm/bubbaloop.camera.v1.Image/RIHS01_..."
 *    Normalized: "0/camera%terrace%raw_shm"
 *
 * 2. Raw Zenoh key: {topic/path}/{type_name}/{type_hash}
 *    Example: "camera/terrace/raw_shm/bubbaloop.camera.v1.Image/RIHS01_..."
 *    Normalized: "camera/terrace/raw_shm"
 *
 * This function extracts just the topic part, stripping:
 * - Trailing wildcards (/** or /*)
 * - Type name and hash suffixes
 */
export function normalizeTopicPattern(topic: string): string {
  if (!topic) return topic;

  // Strip trailing wildcards first
  let normalized = topic;
  if (normalized.endsWith('/**')) {
    normalized = normalized.slice(0, -3);
  } else if (normalized.endsWith('/*')) {
    normalized = normalized.slice(0, -2);
  }

  // Don't strip type/hash from intentional wildcard subscription patterns
  // like "**/bubbaloop.weather.v1.CurrentWeather" — these are meant to match
  // by type name across all topics. Only strip from concrete key expressions.
  if (normalized.includes('*')) {
    return normalized;
  }

  const parts = normalized.split('/');

  // Strip type/hash segments from the end.
  // Type name looks like "bubbaloop.camera.v1.Image" (has dots, starts with "bubbaloop")
  // Hash starts with "RIHS"
  let cutIndex = parts.length;
  for (let i = parts.length - 1; i >= 0; i--) {
    const part = parts[i];
    if (part.startsWith('RIHS') || (part.includes('.') && part.startsWith('bubbaloop'))) {
      cutIndex = i;
    } else {
      break;
    }
  }

  if (cutIndex < parts.length) {
    return parts.slice(0, cutIndex).join('/');
  }

  return normalized;
}

/**
 * Convert a normalized base topic to a Zenoh subscription pattern.
 * Adds /** wildcard to match all type/hash variants.
 */
function toSubscriptionPattern(baseTopic: string): string {
  if (!baseTopic) return baseTopic;
  if (baseTopic.endsWith('/**') || baseTopic.endsWith('/*')) {
    return baseTopic;
  }
  return baseTopic + '/**';
}

/**
 * Extract the display name from a topic (for matching across formats).
 * Handles both ros-z format (0/camera%terrace%raw_shm) and raw format (camera/terrace/raw_shm).
 * Returns: camera/terrace/raw_shm
 */
function toDisplayName(topic: string): string {
  if (!topic) return topic;

  const parts = topic.split('/');

  // Check if it's ros-z format (starts with domain ID like "0/")
  if (parts.length >= 2 && /^\d+$/.test(parts[0])) {
    // ros-z format: decode percent encoding in the second part
    return parts[1].replace(/%/g, '/');
  }

  // Raw format: already in display form
  return topic;
}

export interface TopicStats {
  messageCount: number;
  fps: number;
  instantFps: number;
  lastSeen: number;
}

/**
 * Stats for a monitored topic (from wildcard subscription).
 */
interface MonitoredTopicStats {
  messageCount: number;
  lastSeen: number;
  hzBuffer: TimestampRingBuffer;
}

/**
 * Extended topic stats with listener metadata.
 * Used by StatsView to show all topics and indicate which have active listeners.
 */
export interface MonitoredTopicStatsWithMeta extends TopicStats {
  hasActiveListeners: boolean;
  listenerCount: number;
}

// Ring buffer for timestamp-based Hz calculation
const HZ_BUFFER_SIZE = 100; // Store last 100 message timestamps
const HZ_WINDOW_SECONDS = 2; // Calculate Hz over 2 second window

class TimestampRingBuffer {
  private buffer: number[] = [];
  private index = 0;
  private filled = false;

  push(timestamp: number): void {
    this.buffer[this.index] = timestamp;
    this.index = (this.index + 1) % HZ_BUFFER_SIZE;
    if (this.index === 0) this.filled = true;
  }

  // Calculate Hz based on messages within the time window
  getHz(now: number): number {
    const windowStart = now - (HZ_WINDOW_SECONDS * 1000);
    let count = 0;
    const len = this.filled ? HZ_BUFFER_SIZE : this.index;

    for (let i = 0; i < len; i++) {
      if (this.buffer[i] >= windowStart) {
        count++;
      }
    }

    return count / HZ_WINDOW_SECONDS;
  }
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

export interface SubscribeOptions {
  /** When true, subscribe to the exact topic string without appending /** wildcard */
  exactMatch?: boolean;
}

interface TopicSubscription {
  subscriber: Subscriber | null;
  listeners: Map<string, SampleCallback>;
  stats: TopicStats;
  hzBuffer: TimestampRingBuffer; // Ring buffer for Hz calculation
  pending: boolean;
  undeclaring: boolean; // True while undeclare is in progress
  endpointId: string;
  subscriberId: number; // Unique ID for debugging
  exactMatch: boolean; // When true, skip wildcard pattern conversion
}

// Global counter for subscriber IDs (for debugging)
let subscriberIdCounter = 0;

interface EndpointState {
  config: EndpointConfig;
  session: Session | null;
  subscriptions: Map<string, TopicSubscription>;
  discoveredTopics: Set<string>; // Topics seen from incoming messages
  // Monitoring infrastructure (wildcard subscription for all topics)
  monitoredTopics: Map<string, MonitoredTopicStats>;
  monitorSubscriber: Subscriber | null;
  monitoringEnabled: boolean;
}

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
  private listenerIdCounter = 0;

  constructor() {
    // Initialize default local endpoint
    this.endpoints.set(DEFAULT_ENDPOINT_ID, {
      config: { id: DEFAULT_ENDPOINT_ID, type: 'local' },
      session: null,
      subscriptions: new Map(),
      discoveredTopics: new Set(),
      monitoredTopics: new Map(),
      monitorSubscriber: null,
      monitoringEnabled: false,
    });
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
        discoveredTopics: new Set(),
        monitoredTopics: new Map(),
        monitorSubscriber: null,
        monitoringEnabled: false,
      };
      this.endpoints.set(endpointId, endpoint);
    }

    // If session changed
    if (endpoint.session !== session) {
      const oldSession = endpoint.session;
      endpoint.session = session;

      // Clean up old Zenoh subscribers (but keep the subscription entries with their listeners!)
      if (oldSession) {
        // Stop monitoring if it was active
        if (endpoint.monitorSubscriber) {
          endpoint.monitorSubscriber.undeclare()
            .catch((e) => {
              console.warn('[SubscriptionManager] Error undeclaring monitor subscriber:', e);
            });
          endpoint.monitorSubscriber = null;
        }

        endpoint.subscriptions.forEach((sub) => {
          if (sub.subscriber && !sub.undeclaring) {
            sub.undeclaring = true;
            sub.subscriber.undeclare()
              .catch((e) => {
                console.warn('[SubscriptionManager] Error undeclaring subscriber:', e);
              })
              .finally(() => {
                sub.undeclaring = false;
              });
            sub.subscriber = null;
            sub.pending = true; // Mark as pending for re-subscription
          }
        });
      }

      // Re-subscribe all topics with listeners for this endpoint
      if (session) {
        const pendingTopics = Array.from(endpoint.subscriptions.entries())
          .filter(([, sub]) => sub.listeners.size > 0 && !sub.subscriber && !sub.undeclaring);
        if (pendingTopics.length > 0) {
          console.log(`[SubscriptionManager] Session ready, subscribing to ${pendingTopics.length} pending topic(s)`);
          pendingTopics.forEach(([topic]) => {
            this.createSubscriber(topic, endpointId);
          });
        }

        // Auto-start monitoring when session becomes available
        if (endpoint.monitoringEnabled && !endpoint.monitorSubscriber) {
          this.startMonitoring(endpointId);
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
      discoveredTopics: new Set(),
      monitoredTopics: new Map(),
      monitorSubscriber: null,
      monitoringEnabled: false,
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
   * Topics are normalized to canonical wildcard patterns for deduplication.
   */
  subscribe(
    topic: string,
    callback: SampleCallback,
    endpointId: string = DEFAULT_ENDPOINT_ID,
    options?: SubscribeOptions
  ): string {
    const listenerId = `listener_${++this.listenerIdCounter}`;
    const endpoint = this.endpoints.get(endpointId);
    const exactMatch = options?.exactMatch ?? false;

    if (!endpoint) {
      console.error(`[SubscriptionManager] Unknown endpoint: ${endpointId}`);
      return listenerId;
    }

    // Normalize topic to canonical pattern for deduplication
    // Skip normalization for exactMatch to preserve the raw topic
    const normalizedTopic = exactMatch ? topic : normalizeTopicPattern(topic);

    let subscription = endpoint.subscriptions.get(normalizedTopic);
    const isNewSubscription = !subscription;

    console.log(`[SubscriptionManager] subscribe("${topic}") -> normalized: "${normalizedTopic}", exactMatch: ${exactMatch}, exists: ${!isNewSubscription}, existingSubs: [${Array.from(endpoint.subscriptions.keys()).join(', ')}]`);

    if (!subscription) {
      // First listener for this topic - create subscription entry (lazy, no Zenoh sub yet)
      const subId = ++subscriberIdCounter;
      subscription = {
        subscriber: null,
        listeners: new Map(),
        stats: { messageCount: 0, fps: 0, instantFps: 0, lastSeen: 0 },
        hzBuffer: new TimestampRingBuffer(),
        pending: true,
        undeclaring: false,
        endpointId,
        subscriberId: subId,
        exactMatch,
      };
      endpoint.subscriptions.set(normalizedTopic, subscription);
      console.log(`[SubscriptionManager] New subscription #${subId}: ${normalizedTopic} (exactMatch: ${exactMatch})`);
    }

    // Add listener
    subscription.listeners.set(listenerId, callback);
    console.log(`[SubscriptionManager] Added listener ${listenerId} to ${normalizedTopic} (total: ${subscription.listeners.size}, new sub: ${isNewSubscription})`);

    // Create Zenoh subscriber on-demand if we have a session and don't have one yet
    // Skip if an undeclare is in progress - it will be retried when undeclare completes
    if (endpoint.session && !subscription.subscriber && subscription.pending && !subscription.undeclaring) {
      this.createSubscriber(normalizedTopic, endpointId);
    } else if (subscription.undeclaring) {
      console.log(`[SubscriptionManager] Undeclare in progress for ${normalizedTopic}, will retry after completion`);
    }

    return listenerId;
  }

  /**
   * Unsubscribe a specific listener from a topic.
   * When the last listener is removed, the Zenoh subscriber is automatically cleaned up.
   */
  unsubscribe(topic: string, listenerId: string, endpointId: string = DEFAULT_ENDPOINT_ID, options?: SubscribeOptions): void {
    const endpoint = this.endpoints.get(endpointId);
    if (!endpoint) return;

    // Normalize topic to match subscription key (skip for exactMatch)
    const normalizedTopic = options?.exactMatch ? topic : normalizeTopicPattern(topic);

    const subscription = endpoint.subscriptions.get(normalizedTopic);
    if (!subscription) return;

    subscription.listeners.delete(listenerId);
    console.log(`[SubscriptionManager] Removed listener ${listenerId} from ${normalizedTopic} (remaining: ${subscription.listeners.size})`);

    // Auto-cleanup: If no more listeners, clean up the Zenoh subscriber (but keep entry for reuse)
    if (subscription.listeners.size === 0 && subscription.subscriber && !subscription.undeclaring) {
      console.log(`[SubscriptionManager] No listeners remaining, undeclaring subscriber for: ${normalizedTopic}`);
      const sub = subscription.subscriber;
      subscription.subscriber = null;
      subscription.pending = true;
      subscription.undeclaring = true;
      // Undeclare async but don't delete the subscription entry - it can be reused
      sub.undeclare()
        .then(() => {
          console.log(`[SubscriptionManager] Undeclare complete for: ${normalizedTopic}`);
        })
        .catch((e) => {
          console.warn(`[SubscriptionManager] Error undeclaring subscriber for ${normalizedTopic}:`, e);
        })
        .finally(() => {
          subscription.undeclaring = false;
          // If listeners were added while undeclaring, create new subscriber
          if (subscription.listeners.size > 0 && !subscription.subscriber && endpoint.session) {
            console.log(`[SubscriptionManager] Listeners waiting after undeclare, creating subscriber for: ${normalizedTopic}`);
            this.createSubscriber(normalizedTopic, endpointId);
          }
        });
    }
  }

  /**
   * Get stats for a specific topic on an endpoint.
   */
  getTopicStats(topic: string, endpointId: string = DEFAULT_ENDPOINT_ID): TopicStats | null {
    const endpoint = this.endpoints.get(endpointId);
    const normalizedTopic = normalizeTopicPattern(topic);
    return endpoint?.subscriptions.get(normalizedTopic)?.stats ?? null;
  }

  /**
   * Get stats for all topics across all endpoints.
   * Only returns topics with active listeners.
   * Hz is computed on-demand from the ring buffer.
   */
  getAllStats(): Map<string, TopicStats> {
    const result = new Map<string, TopicStats>();
    const now = Date.now();

    this.endpoints.forEach((endpoint) => {
      endpoint.subscriptions.forEach((sub, topic) => {
        // Only include topics with active listeners
        if (sub.listeners.size === 0) {
          return;
        }

        // Compute Hz from ring buffer
        const hz = sub.hzBuffer.getHz(now);

        // Include endpoint prefix for non-local endpoints
        const key = endpoint.config.id === DEFAULT_ENDPOINT_ID
          ? topic
          : `[${endpoint.config.id}] ${topic}`;

        result.set(key, {
          messageCount: sub.stats.messageCount,
          fps: hz,
          instantFps: hz,
          lastSeen: sub.stats.lastSeen,
        });
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
   * Get all topics discovered from incoming messages.
   * This is populated as messages arrive, providing topic discovery without
   * needing a separate `**` wildcard subscription.
   */
  getDiscoveredTopics(endpointId?: string): string[] {
    const topics: string[] = [];

    const processEndpoint = (endpoint: EndpointState) => {
      endpoint.discoveredTopics.forEach((topic) => {
        topics.push(topic);
      });
    };

    if (endpointId) {
      const endpoint = this.endpoints.get(endpointId);
      if (endpoint) processEndpoint(endpoint);
    } else {
      this.endpoints.forEach(processEndpoint);
    }

    return topics.sort();
  }

  /**
   * Debug: Get information about all subscriptions for troubleshooting.
   */
  getDebugInfo(): { subscriptions: Array<{ topic: string; listeners: number; hasSubscriber: boolean; messageCount: number }> } {
    const subs: Array<{ topic: string; listeners: number; hasSubscriber: boolean; messageCount: number }> = [];

    this.endpoints.forEach((endpoint) => {
      endpoint.subscriptions.forEach((sub, topic) => {
        subs.push({
          topic,
          listeners: sub.listeners.size,
          hasSubscriber: sub.subscriber !== null,
          messageCount: sub.stats.messageCount,
        });
      });
    });

    console.log('[SubscriptionManager] Debug Info:', JSON.stringify(subs, null, 2));
    return { subscriptions: subs };
  }

  /**
   * Check if a topic has any active listeners.
   */
  hasListeners(topic: string, endpointId: string = DEFAULT_ENDPOINT_ID): boolean {
    const endpoint = this.endpoints.get(endpointId);
    const normalizedTopic = normalizeTopicPattern(topic);
    const subscription = endpoint?.subscriptions.get(normalizedTopic);
    return (subscription?.listeners.size ?? 0) > 0;
  }

  /**
   * Get listener count for a topic.
   */
  getListenerCount(topic: string, endpointId: string = DEFAULT_ENDPOINT_ID): number {
    const endpoint = this.endpoints.get(endpointId);
    const normalizedTopic = normalizeTopicPattern(topic);
    return endpoint?.subscriptions.get(normalizedTopic)?.listeners.size ?? 0;
  }

  /**
   * Start monitoring all topics with a wildcard subscription.
   * This subscribes to ** to see all message traffic on the network.
   */
  async startMonitoring(endpointId: string = DEFAULT_ENDPOINT_ID): Promise<void> {
    const endpoint = this.endpoints.get(endpointId);
    if (!endpoint) {
      console.error(`[SubscriptionManager] Unknown endpoint for monitoring: ${endpointId}`);
      return;
    }

    endpoint.monitoringEnabled = true;

    // If no session yet, monitoring will start when session is set
    if (!endpoint.session) {
      console.log(`[SubscriptionManager] Monitoring enabled, will start when session is available`);
      return;
    }

    // If already monitoring, do nothing
    if (endpoint.monitorSubscriber) {
      console.log(`[SubscriptionManager] Already monitoring on ${endpointId}`);
      return;
    }

    try {
      const subscriber = await endpoint.session.declareSubscriber('**', {
        handler: (sample) => {
          this.handleMonitorSample(sample, endpointId);
        },
      });

      endpoint.monitorSubscriber = subscriber;
      console.log(`[SubscriptionManager] Started monitoring all topics on ${endpointId}`);
    } catch (e) {
      console.error(`[SubscriptionManager] Failed to start monitoring:`, e);
    }
  }

  /**
   * Stop monitoring all topics.
   */
  async stopMonitoring(endpointId: string = DEFAULT_ENDPOINT_ID): Promise<void> {
    const endpoint = this.endpoints.get(endpointId);
    if (!endpoint) return;

    endpoint.monitoringEnabled = false;

    if (endpoint.monitorSubscriber) {
      try {
        await endpoint.monitorSubscriber.undeclare();
      } catch (e) {
        console.warn(`[SubscriptionManager] Error stopping monitoring:`, e);
      }
      endpoint.monitorSubscriber = null;
      console.log(`[SubscriptionManager] Stopped monitoring on ${endpointId}`);
    }
  }

  /**
   * Check if monitoring is enabled for an endpoint.
   */
  isMonitoringEnabled(endpointId: string = DEFAULT_ENDPOINT_ID): boolean {
    const endpoint = this.endpoints.get(endpointId);
    return endpoint?.monitoringEnabled ?? false;
  }

  /**
   * Handle a sample from the monitor wildcard subscription.
   * Aggregates stats by normalized topic.
   */
  private handleMonitorSample(sample: Sample, endpointId: string): void {
    const endpoint = this.endpoints.get(endpointId);
    if (!endpoint) return;

    const keyExpr = sample.keyexpr().toString();
    const normalizedTopic = normalizeTopicPattern(keyExpr);
    const now = Date.now();

    // Get or create monitored topic stats
    let stats = endpoint.monitoredTopics.get(normalizedTopic);
    if (!stats) {
      stats = {
        messageCount: 0,
        lastSeen: 0,
        hzBuffer: new TimestampRingBuffer(),
      };
      endpoint.monitoredTopics.set(normalizedTopic, stats);
      console.log(`[SubscriptionManager] Monitor discovered topic: ${normalizedTopic}`);
    }

    stats.messageCount++;
    stats.lastSeen = now;
    stats.hzBuffer.push(now);

    // Also record in discovered topics set
    if (!endpoint.discoveredTopics.has(normalizedTopic)) {
      endpoint.discoveredTopics.add(normalizedTopic);
    }
  }

  /**
   * Get stats for all monitored topics with listener metadata.
   * Returns all topics seen by the monitor subscription, along with
   * whether each topic has active component listeners.
   */
  getAllMonitoredStats(): Map<string, MonitoredTopicStatsWithMeta> {
    const result = new Map<string, MonitoredTopicStatsWithMeta>();
    const now = Date.now();

    this.endpoints.forEach((endpoint) => {
      // Iterate over all monitored topics
      endpoint.monitoredTopics.forEach((stats, topic) => {
        // Compute Hz from ring buffer
        const hz = stats.hzBuffer.getHz(now);

        // Check if this topic has active listeners by matching display names
        // (monitor uses raw format, subscriptions use ros-z format)
        const topicDisplayName = toDisplayName(topic);
        let hasActiveListeners = false;
        let listenerCount = 0;

        endpoint.subscriptions.forEach((sub, subTopic) => {
          if (toDisplayName(subTopic) === topicDisplayName && sub.listeners.size > 0) {
            hasActiveListeners = true;
            listenerCount = sub.listeners.size;
          }
        });

        // Include endpoint prefix for non-local endpoints
        const key = endpoint.config.id === DEFAULT_ENDPOINT_ID
          ? topic
          : `[${endpoint.config.id}] ${topic}`;

        result.set(key, {
          messageCount: stats.messageCount,
          fps: hz,
          instantFps: hz,
          lastSeen: stats.lastSeen,
          hasActiveListeners,
          listenerCount,
        });
      });
    });

    return result;
  }

  /**
   * Clean up all resources.
   */
  destroy(): void {
    this.endpoints.forEach((_, endpointId) => {
      this.stopMonitoring(endpointId);
      this.cleanupEndpointSubscriptions(endpointId);
    });
  }

  private async createSubscriber(baseTopic: string, endpointId: string): Promise<void> {
    const endpoint = this.endpoints.get(endpointId);
    if (!endpoint) return;

    const subscription = endpoint.subscriptions.get(baseTopic);
    if (!subscription || !endpoint.session) return;

    // Guard against creating duplicate subscribers (race condition protection)
    if (subscription.subscriber) {
      console.log(`[SubscriptionManager] Subscriber already exists for ${baseTopic}, skipping`);
      return;
    }

    if (!subscription.pending) {
      console.log(`[SubscriptionManager] Subscription not pending for ${baseTopic}, skipping`);
      return;
    }

    subscription.pending = false;

    // Convert base topic to wildcard pattern for Zenoh subscription
    // Skip wildcard conversion for exactMatch subscriptions
    const subscriptionPattern = subscription.exactMatch ? baseTopic : toSubscriptionPattern(baseTopic);

    try {
      const subscriber = await endpoint.session.declareSubscriber(subscriptionPattern, {
        handler: (sample: Sample) => {
          this.handleSample(baseTopic, sample, endpointId);
        },
      });

      // Double-check we don't already have a subscriber (could have been set by another concurrent call)
      if (subscription.subscriber) {
        console.warn(`[SubscriptionManager] Race condition detected, undeclaring duplicate subscriber for ${baseTopic}`);
        await subscriber.undeclare();
        return;
      }

      subscription.subscriber = subscriber;
      console.log(`[SubscriptionManager] Subscribed #${subscription.subscriberId} to ${subscriptionPattern} (base: ${baseTopic}) on ${endpointId} (${subscription.listeners.size} listeners)`);
    } catch (e) {
      console.error(`[SubscriptionManager] Failed to subscribe to ${subscriptionPattern}:`, e);
      subscription.pending = true; // Allow retry
    }
  }

  private handleSample(topic: string, sample: Sample, endpointId: string): void {
    const endpoint = this.endpoints.get(endpointId);
    if (!endpoint) return;

    const subscription = endpoint.subscriptions.get(topic);
    if (!subscription) return;

    const now = Date.now();

    // Update stats
    subscription.stats.messageCount++;
    subscription.stats.lastSeen = now;

    // Push timestamp to ring buffer for Hz calculation
    subscription.hzBuffer.push(now);

    // Record discovered topic from the actual key expression
    const keyExpr = sample.keyexpr().toString();
    const normalizedKey = normalizeTopicPattern(keyExpr);
    if (!endpoint.discoveredTopics.has(normalizedKey)) {
      endpoint.discoveredTopics.add(normalizedKey);
      console.log(`[SubscriptionManager] Discovered new topic: ${normalizedKey} (from ${keyExpr})`);
    }

    // Debug: log messages to track subscription behavior
    if (subscription.stats.messageCount <= 5 || subscription.stats.messageCount % 100 === 1) {
      console.log(`[SubscriptionManager] Sub #${subscription.subscriberId} Msg #${subscription.stats.messageCount} for "${topic}" from key "${keyExpr}" (${subscription.listeners.size} listeners)`);
    }

    // Dispatch to all listeners
    subscription.listeners.forEach((callback) => {
      try {
        callback(sample);
      } catch (e) {
        console.error(`[SubscriptionManager] Listener error for ${topic}:`, e);
      }
    });
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
}

// Singleton instance for global access (optional, Context is preferred)
let globalManager: ZenohSubscriptionManager | null = null;

export function getGlobalSubscriptionManager(): ZenohSubscriptionManager {
  if (!globalManager) {
    globalManager = new ZenohSubscriptionManager();
  }
  return globalManager;
}

