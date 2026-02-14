import { vi, describe, it, expect, beforeEach } from 'vitest';
import { render, screen } from '@testing-library/react';
import { MachineBadge } from '../MachineBadge';
import type { MachineInfo } from '../../contexts/FleetContext';

function createMachine(overrides: Partial<MachineInfo> = {}): MachineInfo {
  return {
    machineId: 'orin00',
    hostname: 'nvidia-orin00',
    nodeCount: 5,
    runningCount: 3,
    isOnline: true,
    ips: ['192.168.1.100'],
    ...overrides,
  };
}

describe('MachineBadge', () => {
  beforeEach(() => {
    vi.clearAllMocks();
  });

  it('returns null for empty machines array', () => {
    const { container } = render(<MachineBadge machines={[]} />);
    expect(container.innerHTML).toBe('');
  });

  it('shows hostname for single machine', () => {
    const machines = [createMachine({ hostname: 'nvidia-orin00' })];
    render(<MachineBadge machines={machines} />);
    expect(screen.getByText('nvidia-orin00')).toBeInTheDocument();
  });

  it('shows "N machines" for multiple machines', () => {
    const machines = [
      createMachine({ machineId: 'orin00', hostname: 'nvidia-orin00', ips: ['192.168.1.100'] }),
      createMachine({ machineId: 'nano01', hostname: 'jetson-nano01', ips: ['192.168.1.101'] }),
      createMachine({ machineId: 'nano02', hostname: 'jetson-nano02', ips: ['192.168.1.102'] }),
    ];
    render(<MachineBadge machines={machines} />);
    expect(screen.getByText('3 machines')).toBeInTheDocument();
  });

  it('shows title with all machine hostnames and IPs', () => {
    const machines = [
      createMachine({ machineId: 'orin00', hostname: 'nvidia-orin00', ips: ['192.168.1.100'] }),
      createMachine({ machineId: 'nano01', hostname: 'jetson-nano01', ips: ['192.168.1.101'] }),
    ];
    render(<MachineBadge machines={machines} />);
    const badge = screen.getByText('2 machines');
    expect(badge).toHaveAttribute('title', 'nvidia-orin00 (192.168.1.100), jetson-nano01 (192.168.1.101)');
  });

  it('shows title with empty IP when machine has no IPs', () => {
    const machines = [createMachine({ hostname: 'nvidia-orin00', ips: [] })];
    render(<MachineBadge machines={machines} />);
    const badge = screen.getByText('nvidia-orin00');
    expect(badge).toHaveAttribute('title', 'nvidia-orin00 ()');
  });
});
