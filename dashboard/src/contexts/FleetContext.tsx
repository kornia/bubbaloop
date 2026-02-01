import { createContext, useContext, useState, useCallback, type ReactNode } from 'react';

export interface MachineInfo {
  machineId: string;
  hostname: string;
  nodeCount: number;
  runningCount: number;
  isOnline: boolean;
  ips: string[];
}

interface FleetContextValue {
  machines: MachineInfo[];
  reportMachines: (machines: MachineInfo[]) => void;
  selectedMachineId: string | null; // null = all machines
  setSelectedMachineId: (id: string | null) => void;
}

// Default value with no-ops so NodesViewPanel works even without FleetProvider
const FleetContext = createContext<FleetContextValue>({
  machines: [],
  reportMachines: () => {},
  selectedMachineId: null,
  setSelectedMachineId: () => {},
});

export function FleetProvider({ children }: { children: ReactNode }) {
  const [machines, setMachines] = useState<MachineInfo[]>([]);
  const [selectedMachineId, setSelectedMachineId] = useState<string | null>(null);

  const reportMachines = useCallback((newMachines: MachineInfo[]) => {
    setMachines(newMachines);
  }, []);

  return (
    <FleetContext.Provider value={{ machines, reportMachines, selectedMachineId, setSelectedMachineId }}>
      {children}
    </FleetContext.Provider>
  );
}

export function useFleetContext() {
  return useContext(FleetContext);
}
