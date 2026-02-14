import { describe, it, expect } from 'vitest';
import { renderHook, act } from '@testing-library/react';
import { ReactNode } from 'react';
import { FleetProvider, useFleetContext, MachineInfo, FleetNodeInfo } from '../FleetContext';

function wrapper({ children }: { children: ReactNode }) {
  return <FleetProvider>{children}</FleetProvider>;
}

describe('FleetContext', () => {
  describe('with FleetProvider', () => {
    it('initializes with empty arrays for machines and nodes', () => {
      const { result } = renderHook(() => useFleetContext(), { wrapper });

      expect(result.current.machines).toEqual([]);
      expect(result.current.nodes).toEqual([]);
    });

    it('selectedMachineId defaults to null', () => {
      const { result } = renderHook(() => useFleetContext(), { wrapper });

      expect(result.current.selectedMachineId).toBeNull();
    });

    it('reportMachines updates machines array', () => {
      const { result } = renderHook(() => useFleetContext(), { wrapper });

      const machines: MachineInfo[] = [
        {
          machineId: 'machine-1',
          hostname: 'host-1',
          nodeCount: 3,
          runningCount: 2,
          isOnline: true,
          ips: ['192.168.1.1'],
        },
      ];

      act(() => {
        result.current.reportMachines(machines);
      });

      expect(result.current.machines).toEqual(machines);
      expect(result.current.machines).toHaveLength(1);
      expect(result.current.machines[0].machineId).toBe('machine-1');
    });

    it('reportNodes updates nodes array', () => {
      const { result } = renderHook(() => useFleetContext(), { wrapper });

      const nodes: FleetNodeInfo[] = [
        {
          name: 'camera-node',
          status: 'running',
          machineId: 'machine-1',
          hostname: 'host-1',
          ips: ['192.168.1.1'],
          nodeType: 'camera',
          version: '1.0.0',
          baseNode: 'camera-base',
        },
      ];

      act(() => {
        result.current.reportNodes(nodes);
      });

      expect(result.current.nodes).toEqual(nodes);
      expect(result.current.nodes).toHaveLength(1);
      expect(result.current.nodes[0].name).toBe('camera-node');
    });

    it('setSelectedMachineId updates selection', () => {
      const { result } = renderHook(() => useFleetContext(), { wrapper });

      act(() => {
        result.current.setSelectedMachineId('machine-1');
      });

      expect(result.current.selectedMachineId).toBe('machine-1');
    });

    it('setSelectedMachineId can be set back to null', () => {
      const { result } = renderHook(() => useFleetContext(), { wrapper });

      act(() => {
        result.current.setSelectedMachineId('machine-1');
      });
      expect(result.current.selectedMachineId).toBe('machine-1');

      act(() => {
        result.current.setSelectedMachineId(null);
      });
      expect(result.current.selectedMachineId).toBeNull();
    });

    it('multiple calls to reportMachines replaces previous machines', () => {
      const { result } = renderHook(() => useFleetContext(), { wrapper });

      const firstBatch: MachineInfo[] = [
        {
          machineId: 'machine-1',
          hostname: 'host-1',
          nodeCount: 1,
          runningCount: 1,
          isOnline: true,
          ips: ['10.0.0.1'],
        },
        {
          machineId: 'machine-2',
          hostname: 'host-2',
          nodeCount: 2,
          runningCount: 0,
          isOnline: false,
          ips: ['10.0.0.2'],
        },
      ];

      act(() => {
        result.current.reportMachines(firstBatch);
      });
      expect(result.current.machines).toHaveLength(2);

      const secondBatch: MachineInfo[] = [
        {
          machineId: 'machine-3',
          hostname: 'host-3',
          nodeCount: 5,
          runningCount: 5,
          isOnline: true,
          ips: ['10.0.0.3'],
        },
      ];

      act(() => {
        result.current.reportMachines(secondBatch);
      });

      expect(result.current.machines).toHaveLength(1);
      expect(result.current.machines[0].machineId).toBe('machine-3');
    });

    it('multiple calls to reportNodes replaces previous nodes', () => {
      const { result } = renderHook(() => useFleetContext(), { wrapper });

      const firstBatch: FleetNodeInfo[] = [
        {
          name: 'node-a',
          status: 'running',
          machineId: 'm1',
          hostname: 'h1',
          ips: [],
          nodeType: 'camera',
          version: '1.0',
          baseNode: 'base',
        },
      ];

      act(() => {
        result.current.reportNodes(firstBatch);
      });
      expect(result.current.nodes).toHaveLength(1);

      const secondBatch: FleetNodeInfo[] = [
        {
          name: 'node-b',
          status: 'stopped',
          machineId: 'm2',
          hostname: 'h2',
          ips: [],
          nodeType: 'weather',
          version: '2.0',
          baseNode: 'base2',
        },
        {
          name: 'node-c',
          status: 'running',
          machineId: 'm2',
          hostname: 'h2',
          ips: [],
          nodeType: 'telemetry',
          version: '1.0',
          baseNode: 'base3',
        },
      ];

      act(() => {
        result.current.reportNodes(secondBatch);
      });

      expect(result.current.nodes).toHaveLength(2);
      expect(result.current.nodes[0].name).toBe('node-b');
      expect(result.current.nodes[1].name).toBe('node-c');
    });
  });

  describe('without FleetProvider', () => {
    it('useFleetContext works outside provider and returns defaults', () => {
      const { result } = renderHook(() => useFleetContext());

      expect(result.current.machines).toEqual([]);
      expect(result.current.nodes).toEqual([]);
      expect(result.current.selectedMachineId).toBeNull();
      // Default no-op functions should not throw
      expect(() => result.current.reportMachines([])).not.toThrow();
      expect(() => result.current.reportNodes([])).not.toThrow();
      expect(() => result.current.setSelectedMachineId('test')).not.toThrow();
    });
  });
});
