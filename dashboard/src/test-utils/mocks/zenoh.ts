/**
 * Mock Zenoh types for testing.
 *
 * Provides minimal mocks for Session, Subscriber, Sample, Reply, etc.
 * so component tests can run without a real Zenoh connection.
 */

import { vi } from 'vitest';

/** Minimal KeyExpr mock */
class MockKeyExpr {
  private expr: string;
  constructor(expr: string) {
    this.expr = expr;
  }
  toString(): string {
    return this.expr;
  }
}

/** Minimal Sample mock matching the Zenoh Sample interface */
export class MockSample {
  private _keyexpr: MockKeyExpr;
  private _payload: Uint8Array;

  constructor(keyexpr: string, payload: Uint8Array) {
    this._keyexpr = new MockKeyExpr(keyexpr);
    this._payload = payload;
  }

  keyexpr() {
    return this._keyexpr;
  }

  payload() {
    return {
      to_bytes: () => this._payload,
      deserialize: (s: string) => {
        if (s === 'string') {
          return new TextDecoder().decode(this._payload);
        }
        return this._payload;
      },
    };
  }
}

/** Creates a mock Sample for testing */
export function createMockSample(keyexpr: string, payload: Uint8Array | string): MockSample {
  const data = typeof payload === 'string'
    ? new TextEncoder().encode(payload)
    : payload;
  return new MockSample(keyexpr, data);
}

/** Mock Subscriber that stores callback for triggering in tests */
export class MockSubscriber {
  callback: ((sample: MockSample) => void) | null = null;
  topic: string;
  undeclared = false;

  constructor(topic: string, callback: (sample: MockSample) => void) {
    this.topic = topic;
    this.callback = callback;
  }

  receive(sample: MockSample): void {
    if (this.callback && !this.undeclared) {
      this.callback(sample);
    }
  }

  async undeclare(): Promise<void> {
    this.undeclared = true;
    this.callback = null;
  }
}

/** Mock Reply wrapping a Sample, matches Zenoh Reply interface */
export class MockReply {
  private _sample: MockSample;

  constructor(sample: MockSample) {
    this._sample = sample;
  }

  result() {
    return this._sample;
  }
}

/** Creates a mock Reply for testing query responses */
export function createMockReply(keyexpr: string, payload: Uint8Array): MockReply {
  return new MockReply(new MockSample(keyexpr, payload));
}

/** Async iterator helper for mock GET responses */
async function* asyncIterator<T>(items: T[]): AsyncIterableIterator<T> {
  for (const item of items) {
    yield item;
  }
}

/** Mock Session that tracks subscribers and supports mock GET queries */
export class MockSession {
  subscribers: MockSubscriber[] = [];
  getResponses = new Map<string, MockReply[]>();
  closed = false;

  /** Set up canned responses for a GET query key expression */
  setGetResponse(keyexpr: string, replies: MockReply[]): void {
    this.getResponses.set(keyexpr, replies);
  }

  async declareSubscriber(
    topic: string,
    options: { handler: (sample: MockSample) => void }
  ): Promise<MockSubscriber> {
    const sub = new MockSubscriber(topic, options.handler);
    this.subscribers.push(sub);
    return sub;
  }

  async get(keyexpr: string, _options?: unknown): Promise<AsyncIterableIterator<MockReply>> {
    const replies = this.getResponses.get(keyexpr) ?? [];
    return asyncIterator(replies);
  }

  async close(): Promise<void> {
    this.closed = true;
    for (const sub of this.subscribers) {
      await sub.undeclare();
    }
    this.subscribers = [];
  }

  async put(_keyexpr: string, _payload: unknown): Promise<void> {
    // no-op for tests
  }
}

/** Factory function to create a fresh mock session */
export function createMockZenohSession(): MockSession {
  return new MockSession();
}

/** Create a vi.fn()-based mock of the ZenohSubscriptionContext value */
export function createMockSubscriptionContext() {
  const listeners = new Map<string, (sample: MockSample) => void>();
  let listenerId = 0;

  return {
    manager: {} as unknown,
    getSession: vi.fn(() => null),
    subscribe: vi.fn((_topic: string, callback: (sample: MockSample) => void) => {
      const id = `mock_listener_${++listenerId}`;
      listeners.set(id, callback);
      return id;
    }),
    unsubscribe: vi.fn((_topic: string, id: string) => {
      listeners.delete(id);
    }),
    getTopicStats: vi.fn(() => null),
    getAllStats: vi.fn(() => new Map()),
    getAllMonitoredStats: vi.fn(() => new Map()),
    getActiveSubscriptions: vi.fn(() => []),
    getDiscoveredTopics: vi.fn(() => []),
    addRemoteEndpoint: vi.fn(),
    removeEndpoint: vi.fn(),
    startMonitoring: vi.fn(async () => {}),
    stopMonitoring: vi.fn(async () => {}),
    isMonitoringEnabled: vi.fn(() => false),
    // Test helper: dispatch a sample to all listeners
    _dispatchSample: (sample: MockSample) => {
      listeners.forEach(cb => cb(sample));
    },
    _listeners: listeners,
  };
}
