import { vi, describe, it, expect, beforeEach } from 'vitest';
import React from 'react';
import { render, screen, fireEvent } from '@testing-library/react';

const mockSetSelectedMachineId = vi.fn();

vi.mock('../../contexts/FleetContext', () => ({
  useFleetContext: vi.fn(() => ({
    machines: [],
    reportMachines: vi.fn(),
    nodes: [],
    reportNodes: vi.fn(),
    selectedMachineId: null,
    setSelectedMachineId: mockSetSelectedMachineId,
  })),
  FleetProvider: ({ children }: { children: React.ReactNode }) => children,
}));

import { FleetBar } from '../FleetBar';
import { useFleetContext } from '../../contexts/FleetContext';

const mockUseFleetContext = useFleetContext as ReturnType<typeof vi.fn>;

describe('FleetBar', () => {
  beforeEach(() => {
    vi.clearAllMocks();
    mockUseFleetContext.mockReturnValue({
      machines: [],
      reportMachines: vi.fn(),
      nodes: [],
      reportNodes: vi.fn(),
      selectedMachineId: null,
      setSelectedMachineId: mockSetSelectedMachineId,
    });
  });

  it('returns null when machines array is empty', () => {
    const { container } = render(<FleetBar />);
    expect(container.innerHTML).toBe('');
  });

  it('renders "Fleet" label when machines exist', () => {
    mockUseFleetContext.mockReturnValue({
      machines: [
        { machineId: 'orin00', hostname: 'nvidia-orin00', nodeCount: 5, runningCount: 3, isOnline: true, ips: ['192.168.1.100'] },
      ],
      selectedMachineId: null,
      setSelectedMachineId: mockSetSelectedMachineId,
    });
    render(<FleetBar />);
    expect(screen.getByText('Fleet')).toBeInTheDocument();
  });

  it('renders "All" chip that is active by default', () => {
    mockUseFleetContext.mockReturnValue({
      machines: [
        { machineId: 'orin00', hostname: 'nvidia-orin00', nodeCount: 5, runningCount: 3, isOnline: true, ips: ['192.168.1.100'] },
      ],
      selectedMachineId: null,
      setSelectedMachineId: mockSetSelectedMachineId,
    });
    render(<FleetBar />);
    const allChip = screen.getByText('All').closest('button');
    expect(allChip).toBeInTheDocument();
    expect(allChip?.className).toContain('active');
  });

  it('shows running/total node counts in All chip', () => {
    mockUseFleetContext.mockReturnValue({
      machines: [
        { machineId: 'orin00', hostname: 'nvidia-orin00', nodeCount: 5, runningCount: 3, isOnline: true, ips: ['192.168.1.100'] },
        { machineId: 'nano01', hostname: 'jetson-nano01', nodeCount: 3, runningCount: 2, isOnline: true, ips: ['192.168.1.101'] },
      ],
      selectedMachineId: null,
      setSelectedMachineId: mockSetSelectedMachineId,
    });
    render(<FleetBar />);
    // Total: 5+3=8, running: 3+2=5 => "5/8"
    expect(screen.getByText('5/8')).toBeInTheDocument();
  });

  it('renders per-machine chips with hostname', () => {
    mockUseFleetContext.mockReturnValue({
      machines: [
        { machineId: 'orin00', hostname: 'nvidia-orin00', nodeCount: 5, runningCount: 3, isOnline: true, ips: ['192.168.1.100'] },
        { machineId: 'nano01', hostname: 'jetson-nano01', nodeCount: 3, runningCount: 2, isOnline: true, ips: ['192.168.1.101'] },
      ],
      selectedMachineId: null,
      setSelectedMachineId: mockSetSelectedMachineId,
    });
    render(<FleetBar />);
    expect(screen.getByText('nvidia-orin00')).toBeInTheDocument();
    expect(screen.getByText('jetson-nano01')).toBeInTheDocument();
  });

  it('shows online/offline dot', () => {
    mockUseFleetContext.mockReturnValue({
      machines: [
        { machineId: 'orin00', hostname: 'nvidia-orin00', nodeCount: 5, runningCount: 3, isOnline: true, ips: [] },
        { machineId: 'nano01', hostname: 'jetson-nano01', nodeCount: 3, runningCount: 0, isOnline: false, ips: [] },
      ],
      selectedMachineId: null,
      setSelectedMachineId: mockSetSelectedMachineId,
    });
    const { container } = render(<FleetBar />);
    const onlineDots = container.querySelectorAll('.chip-dot.online');
    const offlineDots = container.querySelectorAll('.chip-dot.offline');
    expect(onlineDots.length).toBe(1);
    expect(offlineDots.length).toBe(1);
  });

  it('click on machine chip calls setSelectedMachineId', () => {
    mockUseFleetContext.mockReturnValue({
      machines: [
        { machineId: 'orin00', hostname: 'nvidia-orin00', nodeCount: 5, runningCount: 3, isOnline: true, ips: ['192.168.1.100'] },
      ],
      selectedMachineId: null,
      setSelectedMachineId: mockSetSelectedMachineId,
    });
    render(<FleetBar />);
    const machineChip = screen.getByText('nvidia-orin00').closest('button')!;
    fireEvent.click(machineChip);
    expect(mockSetSelectedMachineId).toHaveBeenCalledWith('orin00');
  });

  it('click on "All" chip sets selectedMachineId to null', () => {
    mockUseFleetContext.mockReturnValue({
      machines: [
        { machineId: 'orin00', hostname: 'nvidia-orin00', nodeCount: 5, runningCount: 3, isOnline: true, ips: ['192.168.1.100'] },
      ],
      selectedMachineId: 'orin00',
      setSelectedMachineId: mockSetSelectedMachineId,
    });
    render(<FleetBar />);
    const allChip = screen.getByText('All').closest('button')!;
    fireEvent.click(allChip);
    expect(mockSetSelectedMachineId).toHaveBeenCalledWith(null);
  });

  it('shows IP address in machine chip when available', () => {
    mockUseFleetContext.mockReturnValue({
      machines: [
        { machineId: 'orin00', hostname: 'nvidia-orin00', nodeCount: 5, runningCount: 3, isOnline: true, ips: ['192.168.1.100'] },
      ],
      selectedMachineId: null,
      setSelectedMachineId: mockSetSelectedMachineId,
    });
    render(<FleetBar />);
    expect(screen.getByText('192.168.1.100')).toBeInTheDocument();
  });
});
