import { createContext, useContext, useState, useCallback, type ReactNode } from 'react';

export interface MachineInfo {
  machineId: string;
  hostname: string;
  nodeCount: number;
  runningCount: number;
  isOnline: boolean;
  ips: string[];
}

export interface FleetNodeInfo {
  name: string;
  status: string;
  machineId: string;
  hostname: string;
  ips: string[];
  nodeType: string;
  version: string;
  baseNode: string;
}

interface FleetContextValue {
  machines: MachineInfo[];
  reportMachines: (machines: MachineInfo[]) => void;
  nodes: FleetNodeInfo[];
  reportNodes: (nodes: FleetNodeInfo[]) => void;
  selectedMachineId: string | null; // null = all machines
  setSelectedMachineId: (id: string | null) => void;
}

// Default value with no-ops so NodesViewPanel works even without FleetProvider
const FleetContext = createContext<FleetContextValue>({
  machines: [],
  reportMachines: () => {},
  nodes: [],
  reportNodes: () => {},
  selectedMachineId: null,
  setSelectedMachineId: () => {},
});

export function FleetProvider({ children }: { children: ReactNode }) {
  const [machines, setMachines] = useState<MachineInfo[]>([]);
  const [nodes, setNodes] = useState<FleetNodeInfo[]>([]);
  const [selectedMachineId, setSelectedMachineId] = useState<string | null>(null);

  const reportMachines = useCallback((newMachines: MachineInfo[]) => {
    setMachines(newMachines);
  }, []);

  const reportNodes = useCallback((newNodes: FleetNodeInfo[]) => {
    setNodes(newNodes);
  }, []);

  return (
    <FleetContext.Provider value={{ machines, reportMachines, nodes, reportNodes, selectedMachineId, setSelectedMachineId }}>
      {children}
    </FleetContext.Provider>
  );
}

export function useFleetContext() {
  return useContext(FleetContext);
}
