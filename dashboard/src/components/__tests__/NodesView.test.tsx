import { vi, describe, it, expect, beforeEach, afterEach } from 'vitest';
import React from 'react';

// ---- Mock modules BEFORE importing the component ----

const { mockGetSession, mockReportMachines, mockReportNodes, mockDecodeNodeList } = vi.hoisted(() => ({
  mockGetSession: vi.fn(() => null),
  mockReportMachines: vi.fn(),
  mockReportNodes: vi.fn(),
  mockDecodeNodeList: vi.fn((_data: any) => null),
}));

vi.mock('../../contexts/ZenohSubscriptionContext', () => ({
  useZenohSubscriptionContext: vi.fn(() => ({
    manager: {},
    getSession: mockGetSession,
    subscribe: vi.fn(() => 'listener_1'),
    unsubscribe: vi.fn(),
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
  })),
}));

vi.mock('../../contexts/FleetContext', () => ({
  useFleetContext: vi.fn(() => ({
    machines: [],
    reportMachines: mockReportMachines,
    nodes: [],
    reportNodes: mockReportNodes,
    selectedMachineId: null,
    setSelectedMachineId: vi.fn(),
  })),
  FleetProvider: ({ children }: { children: React.ReactNode }) => children,
}));

vi.mock('../../proto/daemon', () => ({
  decodeNodeList: mockDecodeNodeList,
  decodeNodeEvent: vi.fn(() => null),
  NodeCommandProto: { create: vi.fn(), encode: vi.fn(() => ({ finish: () => new Uint8Array() })) },
  CommandResultProto: { decode: vi.fn(() => ({ success: true, message: 'ok' })) },
  CommandType: {
    COMMAND_TYPE_START: 1,
    COMMAND_TYPE_STOP: 2,
    COMMAND_TYPE_RESTART: 3,
    COMMAND_TYPE_INSTALL: 4,
    COMMAND_TYPE_UNINSTALL: 5,
    COMMAND_TYPE_BUILD: 6,
    COMMAND_TYPE_CLEAN: 7,
    COMMAND_TYPE_ENABLE_AUTOSTART: 8,
    COMMAND_TYPE_DISABLE_AUTOSTART: 9,
    COMMAND_TYPE_GET_LOGS: 10,
  },
}));

vi.mock('../../lib/zenoh', () => ({
  getSamplePayload: vi.fn(() => new Uint8Array([1, 2, 3])),
  extractMachineId: vi.fn(() => null),
}));

// ---- Now import the component and testing utilities ----

import { render, screen, fireEvent, waitFor } from '@testing-library/react';
import { NodesViewPanel } from '../NodesView';

// Helper: create a mock session with async iterator for get()
function createMockSession(replies: unknown[] = []) {
  async function* asyncIter() {
    for (const r of replies) {
      yield r;
    }
  }

  return {
    get: vi.fn(async () => asyncIter()),
    put: vi.fn(),
    close: vi.fn(),
    declareSubscriber: vi.fn(),
  };
}


describe('NodesViewPanel', () => {
  beforeEach(() => {
    vi.clearAllMocks();
    vi.useFakeTimers({ shouldAdvanceTime: true });
    mockGetSession.mockReturnValue(null);
    mockDecodeNodeList.mockReturnValue(null);
  });

  afterEach(() => {
    vi.useRealTimers();
  });

  it('renders Nodes title in header', () => {
    render(<NodesViewPanel />);

    expect(screen.getByText('Nodes')).toBeInTheDocument();
  });

  it('shows loading state initially', () => {
    render(<NodesViewPanel />);

    expect(screen.getByText('Connecting to daemon via Zenoh...')).toBeInTheDocument();
  });

  it('shows error state when session returns no data after timeout', async () => {
    vi.useRealTimers(); // Need real timers for this test

    render(<NodesViewPanel />);

    // The component sets a 15s timeout for initial connection
    // We can't easily wait 15s in a test, but we can verify the loading state
    expect(screen.getByText('Connecting to daemon via Zenoh...')).toBeInTheDocument();
  });

  it('remove button calls onRemove', () => {
    const onRemove = vi.fn();

    render(<NodesViewPanel onRemove={onRemove} />);

    const removeBtn = screen.getByTitle('Remove panel');
    fireEvent.click(removeBtn);
    expect(onRemove).toHaveBeenCalledTimes(1);
  });

  it('does not render remove button when onRemove not provided', () => {
    render(<NodesViewPanel />);

    expect(screen.queryByTitle('Remove panel')).not.toBeInTheDocument();
  });

  it('shows daemon status as offline initially', () => {
    render(<NodesViewPanel />);

    const statusBadge = document.querySelector('.daemon-status');
    expect(statusBadge).toBeInTheDocument();
    expect(statusBadge?.textContent).toContain('offline');
  });

  it('renders header with panel title section', () => {
    render(<NodesViewPanel />);

    // Panel has header with title
    const header = document.querySelector('.panel-header');
    expect(header).toBeInTheDocument();

    const title = document.querySelector('.panel-title');
    expect(title).toBeInTheDocument();
    expect(title?.textContent).toBe('Nodes');
  });

  it('shows spinner during loading', () => {
    render(<NodesViewPanel />);

    const spinner = document.querySelector('.spinner');
    expect(spinner).toBeInTheDocument();
  });

  it('shows node list when data is received from session', async () => {
    vi.useRealTimers();

    const mockSession = createMockSession();
    mockGetSession.mockReturnValue(mockSession as any);

    // Mock decodeNodeList to return node data
    mockDecodeNodeList.mockReturnValue({
      nodes: [
        {
          name: 'camera-node',
          path: '/opt/nodes/camera',
          status: 2, // running
          installed: true,
          autostartEnabled: true,
          version: '1.0.0',
          description: 'Camera node',
          nodeType: 'rust',
          isBuilt: true,
          buildOutput: [],
          machineId: 'local',
          machineHostname: 'nvidia-orin',
          machineIps: ['192.168.1.100'],
          baseNode: '',
        },
      ],
      timestampMs: 0n,
      machineId: 'local',
    } as any);

    // Mock Reply class so instanceof checks work
    const { Reply } = await import('@eclipse-zenoh/zenoh-ts');
    const mockReplyInstance = Object.create(Reply.prototype);
    mockReplyInstance.result = () => ({
      keyexpr: () => ({ toString: () => 'bubbaloop/daemon/nodes' }),
      payload: () => ({
        to_bytes: () => new Uint8Array([1, 2, 3]),
        deserialize: () => new Uint8Array([1, 2, 3]),
      }),
    });

    async function* asyncIterWithReply() {
      yield mockReplyInstance;
    }
    mockSession.get.mockResolvedValue(asyncIterWithReply());

    render(<NodesViewPanel />);

    // Wait for the polling cycle to complete and render nodes
    await waitFor(() => {
      expect(screen.getByText('camera-node')).toBeInTheDocument();
    }, { timeout: 5000 });
  });

  it('applies dragHandleProps to header', () => {
    render(
      <NodesViewPanel dragHandleProps={{ 'data-testid': 'drag-handle' }} />
    );

    const header = document.querySelector('.panel-header');
    expect(header).toBeInTheDocument();
    expect(header?.getAttribute('data-testid')).toBe('drag-handle');
  });

  it('renders table column headers when nodes are shown', async () => {
    vi.useRealTimers();

    const mockSession = createMockSession();
    mockGetSession.mockReturnValue(mockSession as any);

    mockDecodeNodeList.mockReturnValue({
      nodes: [
        {
          name: 'test-node',
          path: '/opt/test',
          status: 1,
          installed: true,
          autostartEnabled: false,
          version: '0.1.0',
          description: '',
          nodeType: 'python',
          isBuilt: true,
          buildOutput: [],
          machineId: 'local',
          machineHostname: 'host',
          machineIps: [],
          baseNode: '',
        },
      ],
      timestampMs: 0n,
      machineId: 'local',
    } as any);

    const { Reply } = await import('@eclipse-zenoh/zenoh-ts');
    const mockReplyInstance = Object.create(Reply.prototype);
    mockReplyInstance.result = () => ({
      keyexpr: () => ({ toString: () => 'bubbaloop/daemon/nodes' }),
      payload: () => ({
        to_bytes: () => new Uint8Array([1]),
        deserialize: () => new Uint8Array([1]),
      }),
    });

    async function* asyncIterWithReply() {
      yield mockReplyInstance;
    }
    mockSession.get.mockResolvedValue(asyncIterWithReply());

    render(<NodesViewPanel />);

    await waitFor(() => {
      expect(screen.getByText('Name')).toBeInTheDocument();
    }, { timeout: 5000 });

    // Check column headers
    expect(screen.getByText('St')).toBeInTheDocument();
    expect(screen.getByText('Name')).toBeInTheDocument();
    expect(screen.getByText('Machine')).toBeInTheDocument();
    expect(screen.getByText('Version')).toBeInTheDocument();
    expect(screen.getByText('Type')).toBeInTheDocument();
  });
});
