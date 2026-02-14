import { describe, it, expect, vi, beforeEach, afterEach } from 'vitest';
import { renderHook, act, waitFor } from '@testing-library/react';
import { ReactNode } from 'react';
import {
  NodeDiscoveryProvider,
  useNodeDiscovery,
  type NodeManifest,
  type DiscoveredNode,
} from '../NodeDiscoveryContext';
import { Reply, Sample, ReplyError } from '@eclipse-zenoh/zenoh-ts';

// ============================================================================
// Mocks
// ============================================================================

// Hoist mock functions so they can be referenced in vi.mock()
const { mockReportMachines, mockReportNodes, mockDecodeNodeList, mockGetSamplePayload } =
  vi.hoisted(() => ({
    mockReportMachines: vi.fn(),
    mockReportNodes: vi.fn(),
    mockDecodeNodeList: vi.fn(),
    mockGetSamplePayload: vi.fn(),
  }));

// Mock FleetContext
vi.mock('../FleetContext', () => ({
  useFleetContext: vi.fn(() => ({
    reportMachines: mockReportMachines,
    reportNodes: mockReportNodes,
    machines: [],
    nodes: [],
    selectedMachineId: null,
    setSelectedMachineId: vi.fn(),
  })),
}));

// Mock proto/daemon
vi.mock('../../proto/daemon', () => ({
  decodeNodeList: mockDecodeNodeList,
}));

// Mock lib/zenoh
vi.mock('../../lib/zenoh', () => ({
  getSamplePayload: mockGetSamplePayload,
}));

// Mock typed-duration
vi.mock('typed-duration', () => ({
  Duration: {
    milliseconds: {
      of: (ms: number) => ms,
    },
  },
}));

// ============================================================================
// Test Helpers
// ============================================================================

function createMockSession(responses: Map<string, Reply[]> = new Map()): any {
  return {
    get: vi.fn(async (keyexpr: string) => {
      const replies = responses.get(keyexpr) || [];
      return (async function* () {
        for (const reply of replies) yield reply;
      })();
    }),
    close: vi.fn(),
    liveliness: vi.fn().mockReturnValue({
      declare_subscriber: vi.fn().mockResolvedValue({ undeclare: vi.fn() }),
    }),
  };
}

function createMockSample(keyexpr: string, payload: Uint8Array): Sample {
  return new Sample(keyexpr, payload);
}

function createMockReply(sample: Sample): Reply {
  return new Reply(sample);
}

function createMockReplyError(): Reply {
  return new Reply(undefined, new ReplyError('Mock error'));
}

function createNodeListPayload(nodes: any[], machineId = 'test-machine'): Uint8Array {
  return new Uint8Array([1, 2, 3]); // Mock bytes
}

function createManifestPayload(manifest: NodeManifest): Uint8Array {
  return new TextEncoder().encode(JSON.stringify(manifest));
}

function makeWrapper(session: any) {
  return function Wrapper({ children }: { children: ReactNode }) {
    return <NodeDiscoveryProvider session={session}>{children}</NodeDiscoveryProvider>;
  };
}

// ============================================================================
// Tests
// ============================================================================

describe('NodeDiscoveryContext', () => {
  beforeEach(() => {
    vi.clearAllMocks();
    vi.useFakeTimers();
  });

  afterEach(() => {
    vi.useRealTimers();
  });

  describe('useNodeDiscovery without provider', () => {
    it('returns default context values', () => {
      const { result } = renderHook(() => useNodeDiscovery());

      expect(result.current.nodes).toEqual([]);
      expect(result.current.loading).toBe(true);
      expect(result.current.error).toBeNull();
      expect(result.current.daemonConnected).toBe(false);
      expect(result.current.manifestDiscoveryActive).toBe(false);
      expect(result.current.refresh).toBeInstanceOf(Function);
    });
  });

  describe('NodeDiscoveryProvider', () => {
    describe('initialization', () => {
      it('starts with loading=true and empty nodes', () => {
        const session = createMockSession();
        const wrapper = makeWrapper(session);

        const { result } = renderHook(() => useNodeDiscovery(), { wrapper });

        expect(result.current.loading).toBe(true);
        expect(result.current.nodes).toEqual([]);
      });

      it('shows daemonConnected=false initially', () => {
        const session = createMockSession();
        const wrapper = makeWrapper(session);

        const { result } = renderHook(() => useNodeDiscovery(), { wrapper });

        expect(result.current.daemonConnected).toBe(false);
      });

      it('has manifestDiscoveryActive=false initially', () => {
        const session = createMockSession();
        const wrapper = makeWrapper(session);

        const { result } = renderHook(() => useNodeDiscovery(), { wrapper });

        expect(result.current.manifestDiscoveryActive).toBe(false);
      });
    });

    describe('null session', () => {
      it('resets all state when session is null', async () => {
        const wrapper = makeWrapper(null);

        const { result } = renderHook(() => useNodeDiscovery(), { wrapper });

        await act(async () => {
          await vi.advanceTimersByTimeAsync(100);
        });

        expect(result.current.nodes).toEqual([]);
        expect(result.current.loading).toBe(true);
        expect(result.current.error).toBeNull();
        expect(result.current.daemonConnected).toBe(false);
      });

      it('sets loading=true with null session', () => {
        const wrapper = makeWrapper(null);

        const { result } = renderHook(() => useNodeDiscovery(), { wrapper });

        expect(result.current.loading).toBe(true);
      });
    });

    describe('daemon polling', () => {
      it('queries bubbaloop/daemon/nodes when session available', async () => {
        const mockGet = vi.fn(async () => (async function* () {})());
        const session = { ...createMockSession(), get: mockGet };
        const wrapper = makeWrapper(session);

        renderHook(() => useNodeDiscovery(), { wrapper });

        await act(async () => {
          await vi.advanceTimersByTimeAsync(100);
        });

        expect(mockGet).toHaveBeenCalledWith('bubbaloop/daemon/nodes', { timeout: 5000 });
      });

      it('sets daemonConnected=true after receiving daemon data', async () => {
        const nodeListData = createNodeListPayload([
          {
            name: 'camera-node',
            path: '/path/to/camera',
            status: 2, // running
            installed: true,
            autostartEnabled: true,
            version: '1.0.0',
            description: 'Camera node',
            nodeType: 'camera',
            isBuilt: true,
            buildOutput: [],
            machineId: 'machine-1',
            machineHostname: 'host-1',
            machineIps: ['192.168.1.1'],
            baseNode: 'camera-base',
          },
        ]);

        mockDecodeNodeList.mockReturnValue({
          nodes: [
            {
              name: 'camera-node',
              path: '/path/to/camera',
              status: 2,
              installed: true,
              autostartEnabled: true,
              version: '1.0.0',
              description: 'Camera node',
              nodeType: 'camera',
              isBuilt: true,
              buildOutput: [],
              machineId: 'machine-1',
              machineHostname: 'host-1',
              machineIps: ['192.168.1.1'],
              baseNode: 'camera-base',
            },
          ],
          timestampMs: BigInt(Date.now()),
          machineId: 'machine-1',
        });

        mockGetSamplePayload.mockReturnValue(nodeListData);

        const sample = createMockSample('bubbaloop/daemon/nodes', nodeListData);
        const reply = createMockReply(sample);
        const responses = new Map([['bubbaloop/daemon/nodes', [reply]]]);
        const session = createMockSession(responses);
        const wrapper = makeWrapper(session);

        const { result } = renderHook(() => useNodeDiscovery(), { wrapper });

        await act(async () => {
          await vi.advanceTimersByTimeAsync(100);
        });

        expect(result.current.daemonConnected).toBe(true);
      });

      it('maps protobuf status numbers to string status', async () => {
        const nodeListData = createNodeListPayload([]);

        mockDecodeNodeList.mockReturnValue({
          nodes: [
            {
              name: 'stopped-node',
              path: '/path/stopped',
              status: 1,
              installed: true,
              autostartEnabled: false,
              version: '1.0.0',
              description: 'Stopped',
              nodeType: 'test',
              isBuilt: true,
              buildOutput: [],
              machineId: 'machine-1',
              machineHostname: 'host-1',
              machineIps: [],
              baseNode: '',
            },
            {
              name: 'running-node',
              path: '/path/running',
              status: 2,
              installed: true,
              autostartEnabled: true,
              version: '1.0.0',
              description: 'Running',
              nodeType: 'test',
              isBuilt: true,
              buildOutput: [],
              machineId: 'machine-1',
              machineHostname: 'host-1',
              machineIps: [],
              baseNode: '',
            },
            {
              name: 'failed-node',
              path: '/path/failed',
              status: 3,
              installed: true,
              autostartEnabled: false,
              version: '1.0.0',
              description: 'Failed',
              nodeType: 'test',
              isBuilt: true,
              buildOutput: [],
              machineId: 'machine-1',
              machineHostname: 'host-1',
              machineIps: [],
              baseNode: '',
            },
          ],
          timestampMs: BigInt(Date.now()),
          machineId: 'machine-1',
        });

        mockGetSamplePayload.mockReturnValue(nodeListData);

        const sample = createMockSample('bubbaloop/daemon/nodes', nodeListData);
        const reply = createMockReply(sample);
        const responses = new Map([['bubbaloop/daemon/nodes', [reply]]]);
        const session = createMockSession(responses);
        const wrapper = makeWrapper(session);

        const { result } = renderHook(() => useNodeDiscovery(), { wrapper });

        await act(async () => {
          await vi.advanceTimersByTimeAsync(100);
        });

        expect(result.current.nodes).toHaveLength(3);
        expect(result.current.nodes[0].status).toBe('stopped');
        expect(result.current.nodes[1].status).toBe('running');
        expect(result.current.nodes[2].status).toBe('failed');
      });

      it('handles empty daemon response gracefully', async () => {
        mockDecodeNodeList.mockReturnValue({
          nodes: [],
          timestampMs: BigInt(Date.now()),
          machineId: 'machine-1',
        });

        const nodeListData = createNodeListPayload([]);
        mockGetSamplePayload.mockReturnValue(nodeListData);

        const sample = createMockSample('bubbaloop/daemon/nodes', nodeListData);
        const reply = createMockReply(sample);
        const responses = new Map([['bubbaloop/daemon/nodes', [reply]]]);
        const session = createMockSession(responses);
        const wrapper = makeWrapper(session);

        const { result } = renderHook(() => useNodeDiscovery(), { wrapper });

        await act(async () => {
          await vi.advanceTimersByTimeAsync(100);
        });

        // Should not crash, but won't set daemonConnected since no nodes
        expect(result.current.nodes).toEqual([]);
      });

      it('handles daemon poll error without crashing', async () => {
        const consoleWarnSpy = vi.spyOn(console, 'warn').mockImplementation(() => {});

        mockDecodeNodeList.mockReturnValue(null);

        const nodeListData = createNodeListPayload([]);
        mockGetSamplePayload.mockReturnValue(nodeListData);

        const errorReply = createMockReplyError();
        const responses = new Map([['bubbaloop/daemon/nodes', [errorReply]]]);
        const session = createMockSession(responses);
        const wrapper = makeWrapper(session);

        const { result } = renderHook(() => useNodeDiscovery(), { wrapper });

        await act(async () => {
          await vi.advanceTimersByTimeAsync(100);
        });

        // Should not crash
        expect(result.current.nodes).toEqual([]);

        consoleWarnSpy.mockRestore();
      });
    });

    describe('manifest discovery', () => {
      it('queries bubbaloop/**/manifest for node discovery', async () => {
        const mockGet = vi.fn(async () => (async function* () {})());
        const session = { ...createMockSession(), get: mockGet };
        const wrapper = makeWrapper(session);

        renderHook(() => useNodeDiscovery(), { wrapper });

        // Wait for manifest discovery to start (2s delay + 100ms buffer)
        await act(async () => {
          await vi.advanceTimersByTimeAsync(2100);
        });

        expect(mockGet).toHaveBeenCalledWith('bubbaloop/**/manifest', { timeout: 5000 });
      });

      it('parses manifest JSON from zenoh replies', async () => {
        const manifest: NodeManifest = {
          name: 'camera-node',
          version: '1.0.0',
          language: 'rust',
          description: 'Camera node',
          machine_id: 'machine-1',
          scope: 'machine',
          capabilities: ['vision'],
          requires_hardware: ['camera'],
          publishes: [
            {
              topic_suffix: 'frames',
              full_topic: 'bubbaloop/camera-node/frames',
              rate_hz: 30,
            },
          ],
          subscribes: [],
          schema_key: 'bubbaloop/camera-node/schema',
          health_key: 'bubbaloop/camera-node/health',
          config_key: 'bubbaloop/camera-node/config',
          security: {
            acl_prefix: 'bubbaloop/camera',
            data_classification: 'public',
          },
          time: {
            clock_source: 'system',
            timestamp_field: 'timestamp',
            timestamp_unit: 'ms',
          },
        };

        const manifestData = createManifestPayload(manifest);
        const sample = createMockSample('bubbaloop/camera-node/manifest', manifestData);
        const reply = createMockReply(sample);

        mockGetSamplePayload.mockReturnValue(manifestData);

        const responses = new Map([['bubbaloop/**/manifest', [reply]]]);
        const session = createMockSession(responses);
        const wrapper = makeWrapper(session);

        const { result } = renderHook(() => useNodeDiscovery(), { wrapper });

        await act(async () => {
          await vi.advanceTimersByTimeAsync(2100);
        });

        // Should have manifest-only node
        expect(result.current.nodes).toHaveLength(1);
        expect(result.current.nodes[0].name).toBe('camera-node');
        expect(result.current.nodes[0].manifest).toEqual(manifest);
      });

      it('handles invalid manifest JSON gracefully', async () => {
        const consoleWarnSpy = vi.spyOn(console, 'warn').mockImplementation(() => {});

        const invalidData = new TextEncoder().encode('not valid json {');
        const sample = createMockSample('bubbaloop/invalid/manifest', invalidData);
        const reply = createMockReply(sample);

        mockGetSamplePayload.mockReturnValue(invalidData);

        const responses = new Map([['bubbaloop/**/manifest', [reply]]]);
        const session = createMockSession(responses);
        const wrapper = makeWrapper(session);

        const { result } = renderHook(() => useNodeDiscovery(), { wrapper });

        await act(async () => {
          await vi.advanceTimersByTimeAsync(2100);
        });

        // Should not crash
        expect(result.current.nodes).toEqual([]);

        consoleWarnSpy.mockRestore();
      });
    });

    describe('node merging', () => {
      it('marks daemon-only nodes as discoveredVia=daemon', async () => {
        const nodeListData = createNodeListPayload([]);

        mockDecodeNodeList.mockReturnValue({
          nodes: [
            {
              name: 'daemon-only',
              path: '/path/daemon',
              status: 2,
              installed: true,
              autostartEnabled: true,
              version: '1.0.0',
              description: 'Daemon only',
              nodeType: 'test',
              isBuilt: true,
              buildOutput: [],
              machineId: 'machine-1',
              machineHostname: 'host-1',
              machineIps: [],
              baseNode: '',
            },
          ],
          timestampMs: BigInt(Date.now()),
          machineId: 'machine-1',
        });

        mockGetSamplePayload.mockReturnValue(nodeListData);

        const sample = createMockSample('bubbaloop/daemon/nodes', nodeListData);
        const reply = createMockReply(sample);
        const responses = new Map([['bubbaloop/daemon/nodes', [reply]]]);
        const session = createMockSession(responses);
        const wrapper = makeWrapper(session);

        const { result } = renderHook(() => useNodeDiscovery(), { wrapper });

        await act(async () => {
          await vi.advanceTimersByTimeAsync(100);
        });

        expect(result.current.nodes).toHaveLength(1);
        expect(result.current.nodes[0].discoveredVia).toBe('daemon');
      });

      it('marks manifest-only nodes as discoveredVia=manifest with status=unknown', async () => {
        const manifest: NodeManifest = {
          name: 'manifest-only',
          version: '1.0.0',
          language: 'python',
          description: 'Manifest only',
          machine_id: 'machine-1',
          scope: 'machine',
          capabilities: [],
          requires_hardware: [],
          publishes: [],
          subscribes: [],
          schema_key: '',
          health_key: '',
          config_key: '',
          security: { acl_prefix: '', data_classification: '' },
          time: { clock_source: '', timestamp_field: '', timestamp_unit: '' },
        };

        const manifestData = createManifestPayload(manifest);
        const sample = createMockSample('bubbaloop/manifest-only/manifest', manifestData);
        const reply = createMockReply(sample);

        mockGetSamplePayload.mockReturnValue(manifestData);

        const responses = new Map([['bubbaloop/**/manifest', [reply]]]);
        const session = createMockSession(responses);
        const wrapper = makeWrapper(session);

        const { result } = renderHook(() => useNodeDiscovery(), { wrapper });

        await act(async () => {
          await vi.advanceTimersByTimeAsync(2100);
        });

        expect(result.current.nodes).toHaveLength(1);
        expect(result.current.nodes[0].discoveredVia).toBe('manifest');
        expect(result.current.nodes[0].status).toBe('unknown');
      });

      it('marks nodes found in both as discoveredVia=both', async () => {
        const nodeName = 'combined-node';
        const machineId = 'machine-1';

        // Daemon response
        const nodeListData = createNodeListPayload([]);
        mockDecodeNodeList.mockReturnValue({
          nodes: [
            {
              name: nodeName,
              path: '/path/combined',
              status: 2,
              installed: true,
              autostartEnabled: true,
              version: '1.0.0',
              description: 'Combined',
              nodeType: 'test',
              isBuilt: true,
              buildOutput: [],
              machineId,
              machineHostname: 'host-1',
              machineIps: [],
              baseNode: '',
            },
          ],
          timestampMs: BigInt(Date.now()),
          machineId,
        });
        mockGetSamplePayload.mockReturnValueOnce(nodeListData);

        // Manifest response
        const manifest: NodeManifest = {
          name: nodeName,
          version: '1.0.0',
          language: 'rust',
          description: 'Combined manifest',
          machine_id: machineId,
          scope: 'machine',
          capabilities: [],
          requires_hardware: [],
          publishes: [],
          subscribes: [],
          schema_key: '',
          health_key: '',
          config_key: '',
          security: { acl_prefix: '', data_classification: '' },
          time: { clock_source: '', timestamp_field: '', timestamp_unit: '' },
        };
        const manifestData = createManifestPayload(manifest);
        mockGetSamplePayload.mockReturnValueOnce(manifestData);

        const daemonSample = createMockSample('bubbaloop/daemon/nodes', nodeListData);
        const daemonReply = createMockReply(daemonSample);

        const manifestSample = createMockSample(
          `bubbaloop/${nodeName}/manifest`,
          manifestData
        );
        const manifestReply = createMockReply(manifestSample);

        const responses = new Map([
          ['bubbaloop/daemon/nodes', [daemonReply]],
          ['bubbaloop/**/manifest', [manifestReply]],
        ]);

        const session = createMockSession(responses);
        const wrapper = makeWrapper(session);

        const { result } = renderHook(() => useNodeDiscovery(), { wrapper });

        // Let daemon poll run
        await act(async () => {
          await vi.advanceTimersByTimeAsync(100);
        });

        // Let manifest discovery run
        await act(async () => {
          await vi.advanceTimersByTimeAsync(2100);
        });

        expect(result.current.nodes).toHaveLength(1);
        expect(result.current.nodes[0].discoveredVia).toBe('both');
        expect(result.current.nodes[0].manifest).toEqual(manifest);
      });
    });

    describe('fleet reporting', () => {
      it('calls reportMachines with machine groups', async () => {
        const nodeListData = createNodeListPayload([]);

        mockDecodeNodeList.mockReturnValue({
          nodes: [
            {
              name: 'node-1',
              path: '/path/1',
              status: 2,
              installed: true,
              autostartEnabled: true,
              version: '1.0.0',
              description: 'Node 1',
              nodeType: 'test',
              isBuilt: true,
              buildOutput: [],
              machineId: 'machine-1',
              machineHostname: 'host-1',
              machineIps: ['192.168.1.1'],
              baseNode: '',
            },
            {
              name: 'node-2',
              path: '/path/2',
              status: 1,
              installed: true,
              autostartEnabled: false,
              version: '1.0.0',
              description: 'Node 2',
              nodeType: 'test',
              isBuilt: true,
              buildOutput: [],
              machineId: 'machine-1',
              machineHostname: 'host-1',
              machineIps: ['192.168.1.1'],
              baseNode: '',
            },
          ],
          timestampMs: BigInt(Date.now()),
          machineId: 'machine-1',
        });

        mockGetSamplePayload.mockReturnValue(nodeListData);

        const sample = createMockSample('bubbaloop/daemon/nodes', nodeListData);
        const reply = createMockReply(sample);
        const responses = new Map([['bubbaloop/daemon/nodes', [reply]]]);
        const session = createMockSession(responses);
        const wrapper = makeWrapper(session);

        renderHook(() => useNodeDiscovery(), { wrapper });

        await act(async () => {
          await vi.advanceTimersByTimeAsync(100);
        });

        // Give React time to flush effects (not timer-based)
        await act(async () => {
          await Promise.resolve();
        });

        expect(mockReportMachines).toHaveBeenCalled();

        const call = mockReportMachines.mock.calls[mockReportMachines.mock.calls.length - 1];
        const machines = call[0];

        expect(machines).toHaveLength(1);
        expect(machines[0]).toMatchObject({
          machineId: 'machine-1',
          hostname: 'host-1',
          nodeCount: 2,
          runningCount: 1,
          isOnline: true,
          ips: ['192.168.1.1'],
        });
      });

      it('calls reportNodes with all discovered nodes', async () => {
        const nodeListData = createNodeListPayload([]);

        mockDecodeNodeList.mockReturnValue({
          nodes: [
            {
              name: 'test-node',
              path: '/path/test',
              status: 2,
              installed: true,
              autostartEnabled: true,
              version: '1.0.0',
              description: 'Test',
              nodeType: 'test',
              isBuilt: true,
              buildOutput: [],
              machineId: 'machine-1',
              machineHostname: 'host-1',
              machineIps: ['192.168.1.1'],
              baseNode: 'test-base',
            },
          ],
          timestampMs: BigInt(Date.now()),
          machineId: 'machine-1',
        });

        mockGetSamplePayload.mockReturnValue(nodeListData);

        const sample = createMockSample('bubbaloop/daemon/nodes', nodeListData);
        const reply = createMockReply(sample);
        const responses = new Map([['bubbaloop/daemon/nodes', [reply]]]);
        const session = createMockSession(responses);
        const wrapper = makeWrapper(session);

        renderHook(() => useNodeDiscovery(), { wrapper });

        await act(async () => {
          await vi.advanceTimersByTimeAsync(100);
        });

        // Give React time to flush effects (not timer-based)
        await act(async () => {
          await Promise.resolve();
        });

        expect(mockReportNodes).toHaveBeenCalled();

        const call = mockReportNodes.mock.calls[mockReportNodes.mock.calls.length - 1];
        const nodes = call[0];

        expect(nodes).toHaveLength(1);
        expect(nodes[0]).toMatchObject({
          name: 'test-node',
          status: 'running',
          machineId: 'machine-1',
          hostname: 'host-1',
          ips: ['192.168.1.1'],
          nodeType: 'test',
          version: '1.0.0',
          baseNode: 'test-base',
        });
      });
    });

    describe('refresh', () => {
      it('refresh function is callable', async () => {
        const session = createMockSession();
        const wrapper = makeWrapper(session);

        const { result } = renderHook(() => useNodeDiscovery(), { wrapper });

        await act(async () => {
          result.current.refresh();
        });

        expect(result.current.refresh).toBeInstanceOf(Function);
      });

      it('refresh triggers new polls', async () => {
        const mockGet = vi.fn(async () => (async function* () {})());
        const session = { ...createMockSession(), get: mockGet };
        const wrapper = makeWrapper(session);

        const { result } = renderHook(() => useNodeDiscovery(), { wrapper });

        await act(async () => {
          await vi.advanceTimersByTimeAsync(100);
        });

        const callCountBefore = mockGet.mock.calls.length;

        await act(async () => {
          result.current.refresh();
          await vi.advanceTimersByTimeAsync(100);
        });

        const callCountAfter = mockGet.mock.calls.length;

        // Refresh should trigger new queries
        expect(callCountAfter).toBeGreaterThan(callCountBefore);
      });
    });
  });
});
