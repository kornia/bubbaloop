import { describe, it, expect, vi } from 'vitest';
import { normalizeTopicPattern, ZenohSubscriptionManager } from '../subscription-manager';

describe('normalizeTopicPattern', () => {
  it('returns empty string for empty input', () => {
    expect(normalizeTopicPattern('')).toBe('');
  });

  it('strips trailing /** wildcard', () => {
    expect(normalizeTopicPattern('camera/terrace/**')).toBe('camera/terrace');
  });

  it('strips trailing /* wildcard', () => {
    expect(normalizeTopicPattern('camera/terrace/*')).toBe('camera/terrace');
  });

  it('preserves plain topics without wildcards', () => {
    expect(normalizeTopicPattern('weather/current')).toBe('weather/current');
  });

  it('preserves vanilla zenoh topic without wildcards', () => {
    expect(normalizeTopicPattern('bubbaloop/local/m1/camera/entrance/compressed')).toBe('bubbaloop/local/m1/camera/entrance/compressed');
  });

  it('preserves topic with multiple segments', () => {
    const topic = 'bubbaloop/scope/machine/camera/terrace/data';
    expect(normalizeTopicPattern(topic)).toBe('bubbaloop/scope/machine/camera/terrace/data');
  });

  it('strips trailing wildcard from vanilla zenoh topic', () => {
    const topic = 'bubbaloop/local/m1/camera/**';
    expect(normalizeTopicPattern(topic)).toBe('bubbaloop/local/m1/camera');
  });

  it('handles wildcard-only pattern', () => {
    expect(normalizeTopicPattern('**')).toBe('**');
    expect(normalizeTopicPattern('*')).toBe('*');
  });

  it('handles single segment topics', () => {
    expect(normalizeTopicPattern('test')).toBe('test');
  });

  it('preserves daemon topic without wildcards', () => {
    expect(normalizeTopicPattern('bubbaloop/daemon/nodes')).toBe('bubbaloop/daemon/nodes');
  });

  it('preserves topic when no wildcard suffix', () => {
    const topic = 'camera/terrace/data';
    expect(normalizeTopicPattern(topic)).toBe('camera/terrace/data');
  });

  it('strips wildcard from weather subscription pattern', () => {
    expect(normalizeTopicPattern('**/weather/current/**')).toBe('**/weather/current');
  });

  it('preserves scoped multi-machine topic', () => {
    const topic = 'bubbaloop/local/nvidia_orin00/system-telemetry/metrics';
    expect(normalizeTopicPattern(topic)).toBe('bubbaloop/local/nvidia_orin00/system-telemetry/metrics');
  });

  it('strips wildcard from network-monitor topic', () => {
    const topic = '**/network-monitor/status/**';
    expect(normalizeTopicPattern(topic)).toBe('**/network-monitor/status');
  });

  it('handles topic with only trailing slash', () => {
    expect(normalizeTopicPattern('camera/terrace/')).toBe('camera/terrace/');
  });

  it('does not strip wildcards in the middle of topic', () => {
    expect(normalizeTopicPattern('bubbaloop/**/camera/compressed')).toBe('bubbaloop/**/camera/compressed');
  });

  it('strips trailing /** from full vanilla zenoh path', () => {
    expect(normalizeTopicPattern('bubbaloop/local/nvidia_orin00/weather/**')).toBe('bubbaloop/local/nvidia_orin00/weather');
  });

  it('strips trailing /* from short path', () => {
    expect(normalizeTopicPattern('bubbaloop/daemon/*')).toBe('bubbaloop/daemon');
  });

  it('handles production scope topics', () => {
    const topic = 'bubbaloop/production/orin_02/camera/entrance/compressed';
    expect(normalizeTopicPattern(topic)).toBe(topic);
  });

  it('handles staging scope with wildcard', () => {
    expect(normalizeTopicPattern('bubbaloop/staging/test01/**')).toBe('bubbaloop/staging/test01');
  });

  it('preserves topic with embedded double star', () => {
    expect(normalizeTopicPattern('bubbaloop/**/schema')).toBe('bubbaloop/**/schema');
  });
});

describe('ZenohSubscriptionManager', () => {
  describe('multi-endpoint management', () => {
    it('initializes with a default local endpoint', () => {
      const manager = new ZenohSubscriptionManager();
      // Default endpoint exists — getActiveSubscriptions returns empty
      expect(manager.getActiveSubscriptions('local')).toEqual([]);
    });

    it('adds a remote endpoint', () => {
      const manager = new ZenohSubscriptionManager();
      manager.addRemoteEndpoint({ id: 'remote-1', type: 'remote', endpoint: 'ws://192.168.1.100:10001' });
      expect(manager.getActiveSubscriptions('remote-1')).toEqual([]);
    });

    it('warns when adding duplicate endpoint', () => {
      const manager = new ZenohSubscriptionManager();
      const warnSpy = vi.spyOn(console, 'warn').mockImplementation(() => {});
      manager.addRemoteEndpoint({ id: 'remote-1', type: 'remote' });
      manager.addRemoteEndpoint({ id: 'remote-1', type: 'remote' });
      expect(warnSpy).toHaveBeenCalledWith(expect.stringContaining('already exists'));
      warnSpy.mockRestore();
    });

    it('removes a remote endpoint', () => {
      const manager = new ZenohSubscriptionManager();
      manager.addRemoteEndpoint({ id: 'remote-1', type: 'remote' });
      manager.removeEndpoint('remote-1');
      // After removal, getActiveSubscriptions for that endpoint returns empty
      expect(manager.getActiveSubscriptions('remote-1')).toEqual([]);
    });

    it('cannot remove default local endpoint', () => {
      const manager = new ZenohSubscriptionManager();
      const warnSpy = vi.spyOn(console, 'warn').mockImplementation(() => {});
      manager.removeEndpoint('local');
      expect(warnSpy).toHaveBeenCalledWith(expect.stringContaining('Cannot remove default'));
      // Local endpoint should still exist
      expect(manager.getActiveSubscriptions('local')).toEqual([]);
      warnSpy.mockRestore();
    });
  });

  describe('listener lifecycle', () => {
    it('subscribe returns a listener ID', () => {
      const manager = new ZenohSubscriptionManager();
      const callback = vi.fn();
      const id = manager.subscribe('camera/terrace', callback);
      expect(id).toMatch(/^listener_\d+$/);
    });

    it('tracks listeners for a topic', () => {
      const manager = new ZenohSubscriptionManager();
      const cb1 = vi.fn();
      const cb2 = vi.fn();
      manager.subscribe('camera/terrace', cb1);
      manager.subscribe('camera/terrace', cb2);
      expect(manager.getListenerCount('camera/terrace')).toBe(2);
      expect(manager.hasListeners('camera/terrace')).toBe(true);
    });

    it('unsubscribe removes a listener', () => {
      const manager = new ZenohSubscriptionManager();
      const cb = vi.fn();
      const id = manager.subscribe('camera/terrace', cb);
      manager.unsubscribe('camera/terrace', id);
      expect(manager.getListenerCount('camera/terrace')).toBe(0);
      expect(manager.hasListeners('camera/terrace')).toBe(false);
    });

    it('deduplicates subscriptions by normalized topic', () => {
      const manager = new ZenohSubscriptionManager();
      const cb1 = vi.fn();
      const cb2 = vi.fn();
      // These should normalize to the same base topic
      manager.subscribe('camera/terrace', cb1);
      manager.subscribe('camera/terrace/**', cb2);
      // Both listeners should be on the same normalized topic
      expect(manager.getListenerCount('camera/terrace')).toBe(2);
    });

    it('auto-cleanup: no listeners means no active subscription', () => {
      const manager = new ZenohSubscriptionManager();
      const cb = vi.fn();
      const id = manager.subscribe('weather/current', cb);
      expect(manager.getActiveSubscriptions()).toEqual(['weather/current']);
      manager.unsubscribe('weather/current', id);
      // After removing last listener, topic should not be active
      expect(manager.getActiveSubscriptions()).toEqual([]);
    });
  });

  describe('stats aggregation', () => {
    it('getAllStats returns empty map with no listeners', () => {
      const manager = new ZenohSubscriptionManager();
      expect(manager.getAllStats().size).toBe(0);
    });

    it('getAllStats includes topics with active listeners', () => {
      const manager = new ZenohSubscriptionManager();
      manager.subscribe('camera/terrace', vi.fn());
      const stats = manager.getAllStats();
      expect(stats.has('camera/terrace')).toBe(true);
      expect(stats.get('camera/terrace')!.messageCount).toBe(0);
    });

    it('getAllStats excludes topics after last listener removed', () => {
      const manager = new ZenohSubscriptionManager();
      const id = manager.subscribe('camera/terrace', vi.fn());
      expect(manager.getAllStats().has('camera/terrace')).toBe(true);
      manager.unsubscribe('camera/terrace', id);
      expect(manager.getAllStats().has('camera/terrace')).toBe(false);
    });

    it('getAllStats prefixes non-local endpoint topics', () => {
      const manager = new ZenohSubscriptionManager();
      manager.addRemoteEndpoint({ id: 'remote-1', type: 'remote' });
      manager.subscribe('camera/terrace', vi.fn(), 'remote-1');
      const stats = manager.getAllStats();
      expect(stats.has('[remote-1] camera/terrace')).toBe(true);
    });
  });

  describe('getActiveSubscriptions', () => {
    it('returns active topics for specific endpoint', () => {
      const manager = new ZenohSubscriptionManager();
      manager.subscribe('camera/terrace', vi.fn());
      manager.subscribe('weather/current', vi.fn());
      const active = manager.getActiveSubscriptions('local');
      expect(active).toContain('camera/terrace');
      expect(active).toContain('weather/current');
    });

    it('returns active topics across all endpoints when no endpointId given', () => {
      const manager = new ZenohSubscriptionManager();
      manager.addRemoteEndpoint({ id: 'remote-1', type: 'remote' });
      manager.subscribe('camera/terrace', vi.fn(), 'local');
      manager.subscribe('weather/data', vi.fn(), 'remote-1');
      const active = manager.getActiveSubscriptions();
      expect(active).toContain('camera/terrace');
      expect(active).toContain('weather/data');
    });
  });

  describe('getDiscoveredTopics', () => {
    it('returns empty when no topics discovered', () => {
      const manager = new ZenohSubscriptionManager();
      expect(manager.getDiscoveredTopics()).toEqual([]);
    });

    it('returns discovered topics for specific endpoint', () => {
      const manager = new ZenohSubscriptionManager();
      expect(manager.getDiscoveredTopics('local')).toEqual([]);
    });

    it('returns empty for non-existent endpoint', () => {
      const manager = new ZenohSubscriptionManager();
      expect(manager.getDiscoveredTopics('nonexistent')).toEqual([]);
    });
  });

  describe('hasListeners and getListenerCount', () => {
    it('hasListeners returns false for unsubscribed topic', () => {
      const manager = new ZenohSubscriptionManager();
      expect(manager.hasListeners('camera/terrace')).toBe(false);
    });

    it('getListenerCount returns 0 for unsubscribed topic', () => {
      const manager = new ZenohSubscriptionManager();
      expect(manager.getListenerCount('camera/terrace')).toBe(0);
    });

    it('counts listeners correctly with multiple subscribes/unsubscribes', () => {
      const manager = new ZenohSubscriptionManager();
      const id1 = manager.subscribe('camera/terrace', vi.fn());
      const id2 = manager.subscribe('camera/terrace', vi.fn());
      const id3 = manager.subscribe('camera/terrace', vi.fn());
      expect(manager.getListenerCount('camera/terrace')).toBe(3);
      expect(manager.hasListeners('camera/terrace')).toBe(true);

      manager.unsubscribe('camera/terrace', id2);
      expect(manager.getListenerCount('camera/terrace')).toBe(2);
      expect(manager.hasListeners('camera/terrace')).toBe(true);

      manager.unsubscribe('camera/terrace', id1);
      manager.unsubscribe('camera/terrace', id3);
      expect(manager.getListenerCount('camera/terrace')).toBe(0);
      expect(manager.hasListeners('camera/terrace')).toBe(false);
    });

    it('handles endpoint-scoped listener counts', () => {
      const manager = new ZenohSubscriptionManager();
      manager.addRemoteEndpoint({ id: 'remote-1', type: 'remote' });
      manager.subscribe('camera/terrace', vi.fn(), 'local');
      manager.subscribe('camera/terrace', vi.fn(), 'remote-1');
      expect(manager.getListenerCount('camera/terrace', 'local')).toBe(1);
      expect(manager.getListenerCount('camera/terrace', 'remote-1')).toBe(1);
    });
  });

  describe('getDebugInfo', () => {
    it('returns empty subscriptions for new manager', () => {
      const manager = new ZenohSubscriptionManager();
      const info = manager.getDebugInfo();
      expect(info.subscriptions).toEqual([]);
    });

    it('returns subscription details when topics are subscribed', () => {
      const manager = new ZenohSubscriptionManager();
      manager.subscribe('camera/terrace', vi.fn());
      manager.subscribe('weather/current', vi.fn());
      const info = manager.getDebugInfo();
      expect(info.subscriptions).toHaveLength(2);
      expect(info.subscriptions[0].listeners).toBe(1);
      expect(info.subscriptions[0].hasSubscriber).toBe(false); // No session set
      expect(info.subscriptions[0].messageCount).toBe(0);
    });

    it('tracks listener count per topic', () => {
      const manager = new ZenohSubscriptionManager();
      manager.subscribe('camera/terrace', vi.fn());
      manager.subscribe('camera/terrace', vi.fn());
      manager.subscribe('camera/terrace', vi.fn());
      const info = manager.getDebugInfo();
      expect(info.subscriptions).toHaveLength(1);
      expect(info.subscriptions[0].listeners).toBe(3);
    });
  });

  describe('isMonitoringEnabled', () => {
    it('returns false by default', () => {
      const manager = new ZenohSubscriptionManager();
      expect(manager.isMonitoringEnabled()).toBe(false);
    });

    it('returns false for non-existent endpoint', () => {
      const manager = new ZenohSubscriptionManager();
      expect(manager.isMonitoringEnabled('nonexistent')).toBe(false);
    });
  });

  describe('getTopicStats', () => {
    it('returns null for unsubscribed topic', () => {
      const manager = new ZenohSubscriptionManager();
      expect(manager.getTopicStats('camera/terrace')).toBeNull();
    });

    it('returns initial stats when topic has listener', () => {
      const manager = new ZenohSubscriptionManager();
      manager.subscribe('camera/terrace', vi.fn());
      const stats = manager.getTopicStats('camera/terrace');
      expect(stats).not.toBeNull();
      expect(stats!.messageCount).toBe(0);
      expect(stats!.fps).toBe(0);
      expect(stats!.instantFps).toBe(0);
    });

    it('returns stats for topic with trailing wildcard normalization', () => {
      const manager = new ZenohSubscriptionManager();
      manager.subscribe('camera/terrace', vi.fn());
      // Query with wildcard variant should find same stats
      const stats = manager.getTopicStats('camera/terrace/**');
      expect(stats).not.toBeNull();
      expect(stats!.messageCount).toBe(0);
    });
  });

  describe('vanilla Zenoh topic patterns', () => {
    it('subscribes to full vanilla zenoh path', () => {
      const manager = new ZenohSubscriptionManager();
      const id = manager.subscribe('bubbaloop/local/nvidia_orin00/camera/entrance/compressed', vi.fn());
      expect(id).toMatch(/^listener_\d+$/);
      expect(manager.hasListeners('bubbaloop/local/nvidia_orin00/camera/entrance/compressed')).toBe(true);
    });

    it('deduplicates vanilla zenoh path with and without wildcard', () => {
      const manager = new ZenohSubscriptionManager();
      manager.subscribe('bubbaloop/local/m1/weather/current', vi.fn());
      manager.subscribe('bubbaloop/local/m1/weather/current/**', vi.fn());
      expect(manager.getListenerCount('bubbaloop/local/m1/weather/current')).toBe(2);
    });

    it('keeps different vanilla zenoh paths separate', () => {
      const manager = new ZenohSubscriptionManager();
      manager.subscribe('bubbaloop/local/m1/weather/current', vi.fn());
      manager.subscribe('bubbaloop/local/m1/weather/hourly', vi.fn());
      expect(manager.getListenerCount('bubbaloop/local/m1/weather/current')).toBe(1);
      expect(manager.getListenerCount('bubbaloop/local/m1/weather/hourly')).toBe(1);
    });
  });
});
