import { vi, describe, it, expect, beforeEach, afterEach } from 'vitest';
import React from 'react';

// ---- Mock modules BEFORE importing the component ----

const { mockGetSession, mockUseNodeDiscovery, mockSelectedMachineId } = vi.hoisted(() => ({
  mockGetSession: vi.fn(() => null),
  mockUseNodeDiscovery: vi.fn(() => ({
    nodes: [] as unknown[],
    loading: true,
    error: null as string | null,
    daemonConnected: false,
    manifestDiscoveryActive: false,
    refresh: vi.fn(),
  })),
  mockSelectedMachineId: { value: null as string | null },
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
    reportMachines: vi.fn(),
    nodes: [],
    reportNodes: vi.fn(),
    selectedMachineId: mockSelectedMachineId.value,
    setSelectedMachineId: vi.fn(),
  })),
  FleetProvider: ({ children }: { children: React.ReactNode }) => children,
}));

vi.mock('../../contexts/NodeDiscoveryContext', () => ({
  useNodeDiscovery: mockUseNodeDiscovery,
  NodeDiscoveryProvider: ({ children }: { children: React.ReactNode }) => children,
}));

vi.mock('../../proto/daemon', () => ({
  decodeNodeList: vi.fn(() => null),
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

import { render, screen, fireEvent } from '@testing-library/react';
import { NodesViewPanel } from '../NodesView';
import type { DiscoveredNode } from '../../contexts/NodeDiscoveryContext';

// Helper: create a discovered node for tests
function makeNode(overrides: Partial<DiscoveredNode> = {}): DiscoveredNode {
  return {
    name: 'test-node',
    machine_id: 'local',
    path: '/opt/nodes/test',
    status: 'running',
    installed: true,
    autostart_enabled: true,
    version: '1.0.0',
    description: 'Test node',
    node_type: 'rust',
    is_built: true,
    build_output: [],
    machine_hostname: 'nvidia-orin',
    machine_ips: ['192.168.1.100'],
    base_node: '',
    discoveredVia: 'daemon',
    stale: false,
    lastSeen: Date.now(),
    ...overrides,
  };
}


describe('NodesViewPanel', () => {
  beforeEach(() => {
    vi.clearAllMocks();
    mockGetSession.mockReturnValue(null);
    mockSelectedMachineId.value = null;
    mockUseNodeDiscovery.mockReturnValue({
      nodes: [],
      loading: true,
      error: null,
      daemonConnected: false,
      manifestDiscoveryActive: false,
      refresh: vi.fn(),
    });
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

    expect(screen.getByText('Discovering nodes via Zenoh...')).toBeInTheDocument();
  });

  it('shows error state when context reports error', () => {
    mockUseNodeDiscovery.mockReturnValue({
      nodes: [],
      loading: false,
      error: 'No data from daemon or nodes - check if bubbaloop-daemon is running',
      daemonConnected: false,
      manifestDiscoveryActive: false,
      refresh: vi.fn(),
    });

    render(<NodesViewPanel />);

    expect(screen.getByText('No data from daemon or nodes - check if bubbaloop-daemon is running')).toBeInTheDocument();
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

  it('shows daemon status as connected when daemonConnected is true', () => {
    mockUseNodeDiscovery.mockReturnValue({
      nodes: [makeNode()],
      loading: false,
      error: null,
      daemonConnected: true,
      manifestDiscoveryActive: false,
      refresh: vi.fn(),
    });

    render(<NodesViewPanel />);

    const statusBadge = document.querySelector('.daemon-status');
    expect(statusBadge).toBeInTheDocument();
    expect(statusBadge?.textContent).toContain('zenoh');
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

  it('shows node list when context provides nodes', () => {
    mockUseNodeDiscovery.mockReturnValue({
      nodes: [
        makeNode({ name: 'camera-node', version: '1.0.0', node_type: 'rust' }),
      ],
      loading: false,
      error: null,
      daemonConnected: true,
      manifestDiscoveryActive: false,
      refresh: vi.fn(),
    });

    render(<NodesViewPanel />);

    expect(screen.getByText('camera-node')).toBeInTheDocument();
  });

  it('applies dragHandleProps to header', () => {
    render(
      <NodesViewPanel dragHandleProps={{ 'data-testid': 'drag-handle' }} />
    );

    const header = document.querySelector('.panel-header');
    expect(header).toBeInTheDocument();
    expect(header?.getAttribute('data-testid')).toBe('drag-handle');
  });

  it('renders table column headers when nodes are shown', () => {
    mockUseNodeDiscovery.mockReturnValue({
      nodes: [makeNode({ name: 'test-node', node_type: 'python' })],
      loading: false,
      error: null,
      daemonConnected: true,
      manifestDiscoveryActive: false,
      refresh: vi.fn(),
    });

    render(<NodesViewPanel />);

    // Check column headers
    expect(screen.getByText('St')).toBeInTheDocument();
    expect(screen.getByText('Name')).toBeInTheDocument();
    expect(screen.getByText('Src')).toBeInTheDocument();
    expect(screen.getByText('Machine')).toBeInTheDocument();
    expect(screen.getByText('Version')).toBeInTheDocument();
    expect(screen.getByText('Type')).toBeInTheDocument();
  });

  it('shows discovery source badge for daemon nodes', () => {
    mockUseNodeDiscovery.mockReturnValue({
      nodes: [makeNode({ name: 'daemon-only-node', discoveredVia: 'daemon' })],
      loading: false,
      error: null,
      daemonConnected: true,
      manifestDiscoveryActive: false,
      refresh: vi.fn(),
    });

    render(<NodesViewPanel />);

    const badge = screen.getByTitle('Discovered via daemon');
    expect(badge).toBeInTheDocument();
    expect(badge.textContent).toBe('D');
  });

  it('shows discovery source badge for manifest-only nodes', () => {
    mockUseNodeDiscovery.mockReturnValue({
      nodes: [makeNode({ name: 'manifest-node', discoveredVia: 'manifest', status: 'unknown' })],
      loading: false,
      error: null,
      daemonConnected: false,
      manifestDiscoveryActive: false,
      refresh: vi.fn(),
    });

    render(<NodesViewPanel />);

    const badge = screen.getByTitle('Discovered via manifest only (daemon offline)');
    expect(badge).toBeInTheDocument();
    expect(badge.textContent).toBe('M');
  });

  it('shows discovery source badge for nodes found via both sources', () => {
    mockUseNodeDiscovery.mockReturnValue({
      nodes: [makeNode({ name: 'both-node', discoveredVia: 'both' })],
      loading: false,
      error: null,
      daemonConnected: true,
      manifestDiscoveryActive: false,
      refresh: vi.fn(),
    });

    render(<NodesViewPanel />);

    const badge = screen.getByTitle('Discovered via daemon + manifest');
    expect(badge).toBeInTheDocument();
    expect(badge.textContent).toBe('B');
  });

  it('shows manifest discovery scanning indicator', () => {
    mockUseNodeDiscovery.mockReturnValue({
      nodes: [],
      loading: true,
      error: null,
      daemonConnected: false,
      manifestDiscoveryActive: true,
      refresh: vi.fn(),
    });

    render(<NodesViewPanel />);

    expect(screen.getByText('scanning...')).toBeInTheDocument();
  });

  it('renders multiple nodes from different sources', () => {
    mockUseNodeDiscovery.mockReturnValue({
      nodes: [
        makeNode({ name: 'camera', discoveredVia: 'both', status: 'running' }),
        makeNode({ name: 'weather', discoveredVia: 'manifest', status: 'unknown' }),
        makeNode({ name: 'telemetry', discoveredVia: 'daemon', status: 'stopped' }),
      ],
      loading: false,
      error: null,
      daemonConnected: true,
      manifestDiscoveryActive: false,
      refresh: vi.fn(),
    });

    render(<NodesViewPanel />);

    expect(screen.getByText('camera')).toBeInTheDocument();
    expect(screen.getByText('weather')).toBeInTheDocument();
    expect(screen.getByText('telemetry')).toBeInTheDocument();
  });

  it('shows "No nodes registered" when context provides empty nodes and no loading', () => {
    mockUseNodeDiscovery.mockReturnValue({
      nodes: [],
      loading: false,
      error: null,
      daemonConnected: true,
      manifestDiscoveryActive: false,
      refresh: vi.fn(),
    });

    render(<NodesViewPanel />);

    expect(screen.getByText('No nodes registered')).toBeInTheDocument();
  });
});
