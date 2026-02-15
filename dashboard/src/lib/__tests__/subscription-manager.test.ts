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

  it('handles ros-z format: domain/encoded_topic/type/hash', () => {
    const topic = '0/camera%terrace%raw_shm/bubbaloop.camera.v1.Image/RIHS01_abc123';
    expect(normalizeTopicPattern(topic)).toBe('0/camera%terrace%raw_shm');
  });

  it('handles ros-z format without type info', () => {
    expect(normalizeTopicPattern('0/camera%terrace')).toBe('0/camera%terrace');
  });

  it('strips bubbaloop type suffix from raw Zenoh key', () => {
    const topic = 'camera/terrace/raw_shm/bubbaloop.camera.v1.Image/RIHS01_abc';
    expect(normalizeTopicPattern(topic)).toBe('camera/terrace/raw_shm');
  });

  it('preserves plain topics without type/hash suffixes', () => {
    expect(normalizeTopicPattern('weather/current')).toBe('weather/current');
  });

  it('handles RIHS hash-only suffix', () => {
    const topic = '0/test/RIHS01_abc123';
    expect(normalizeTopicPattern(topic)).toBe('0/test');
  });

  it('strips RIHS suffix from end of raw Zenoh key', () => {
    const topic = 'camera/terrace/RIHS01_xyz789';
    expect(normalizeTopicPattern(topic)).toBe('camera/terrace');
  });

  it('handles multiple segments before type/hash', () => {
    const topic = 'bubbaloop/scope/machine/camera/terrace/bubbaloop.camera.v1.Image/RIHS01_abc';
    expect(normalizeTopicPattern(topic)).toBe('bubbaloop/scope/machine/camera/terrace');
  });

  it('handles ros-z with wildcard then strips wildcard', () => {
    const topic = '0/camera%terrace/**';
    expect(normalizeTopicPattern(topic)).toBe('0/camera%terrace');
  });

  it('handles topic with type but no hash', () => {
    const topic = 'camera/terrace/bubbaloop.camera.v1.Image';
    expect(normalizeTopicPattern(topic)).toBe('camera/terrace');
  });

  it('does not strip segments that look like domains but are not', () => {
    const topic = 'bubbaloop/scope/123/test';
    // First segment is not a digit, so not ros-z format
    // No type/hash suffixes either
    expect(normalizeTopicPattern(topic)).toBe('bubbaloop/scope/123/test');
  });

  it('handles wildcard-only pattern', () => {
    expect(normalizeTopicPattern('**')).toBe('**');
    expect(normalizeTopicPattern('*')).toBe('*');
  });

  it('handles single segment topics', () => {
    expect(normalizeTopicPattern('test')).toBe('test');
    expect(normalizeTopicPattern('0')).toBe('0');
  });

  it('handles ros-z with only domain and topic', () => {
    expect(normalizeTopicPattern('0/simple_topic')).toBe('0/simple_topic');
  });

  it('handles type suffix that starts with bubbaloop', () => {
    const topic = 'some/path/bubbaloop.system_telemetry.v1.SystemMetrics';
    expect(normalizeTopicPattern(topic)).toBe('some/path');
  });

  it('preserves topic when last segment does not match type/hash pattern', () => {
    const topic = 'camera/terrace/data';
    expect(normalizeTopicPattern(topic)).toBe('camera/terrace/data');
  });

  describe('ros-z new format (slash-preserved)', () => {
    it('strips type and hash from new ros-z format', () => {
      const topic = '0/bubbaloop/local/m1/camera/terrace/raw_shm/bubbaloop.camera.v1.Image/RIHS01_abc123';
      expect(normalizeTopicPattern(topic)).toBe('0/bubbaloop/local/m1/camera/terrace/raw_shm');
    });

    it('strips type and hash from telemetry new format', () => {
      const topic = '0/bubbaloop/local/nvidia_orin00/system_telemetry/metrics/bubbaloop.system_telemetry.v1.SystemMetrics/RIHS01_xyz';
      expect(normalizeTopicPattern(topic)).toBe('0/bubbaloop/local/nvidia_orin00/system_telemetry/metrics');
    });

    it('preserves new format without type/hash', () => {
      const topic = '0/bubbaloop/local/m1/camera/raw';
      expect(normalizeTopicPattern(topic)).toBe('0/bubbaloop/local/m1/camera/raw');
    });

    it('handles new format with only hash suffix', () => {
      const topic = '0/bubbaloop/local/m1/test/RIHS01_abc123';
      expect(normalizeTopicPattern(topic)).toBe('0/bubbaloop/local/m1/test');
    });

    it('handles new format with trailing wildcard', () => {
      const topic = '0/bubbaloop/local/m1/camera/**';
      expect(normalizeTopicPattern(topic)).toBe('0/bubbaloop/local/m1/camera');
    });
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
});
