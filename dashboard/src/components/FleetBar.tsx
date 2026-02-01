import { useFleetContext } from '../contexts/FleetContext';

export function FleetBar() {
  const { machines, selectedMachineId, setSelectedMachineId } = useFleetContext();

  // Only render when machines are discovered
  if (machines.length === 0) return null;

  const totalNodes = machines.reduce((sum, m) => sum + m.nodeCount, 0);
  const totalRunning = machines.reduce((sum, m) => sum + m.runningCount, 0);

  return (
    <div className="fleet-bar">
      <div className="fleet-bar-inner">
        <span className="fleet-label">Fleet</span>
        <div className="fleet-chips">
          {/* All Machines chip */}
          <button
            className={`fleet-chip ${selectedMachineId === null ? 'active' : ''}`}
            onClick={() => setSelectedMachineId(null)}
          >
            <span className="chip-dot chip-dot-all" />
            <span className="chip-hostname">All</span>
            <span className="chip-badge">{totalRunning}/{totalNodes}</span>
          </button>

          {/* Per-machine chips */}
          {machines.map(m => (
            <button
              key={m.machineId}
              className={`fleet-chip ${selectedMachineId === m.machineId ? 'active' : ''}`}
              onClick={() => setSelectedMachineId(m.machineId)}
            >
              <span className={`chip-dot ${m.isOnline ? 'online' : 'offline'}`} />
              <span className="chip-hostname">{m.hostname}</span>
              {m.ips.length > 0 && (
                <span className="chip-ip">{m.ips[0]}</span>
              )}
              <span className="chip-badge">{m.runningCount}/{m.nodeCount}</span>
            </button>
          ))}
        </div>
      </div>

      <style>{`
        .fleet-bar {
          padding: 8px 24px;
          background: var(--bg-secondary);
          border-bottom: 1px solid var(--border-color);
        }

        .fleet-bar-inner {
          display: flex;
          align-items: center;
          gap: 12px;
          max-width: 1800px;
          margin: 0 auto;
        }

        .fleet-label {
          font-size: 11px;
          font-weight: 600;
          color: var(--text-muted);
          text-transform: uppercase;
          letter-spacing: 0.5px;
          flex-shrink: 0;
        }

        .fleet-chips {
          display: flex;
          gap: 8px;
          overflow-x: auto;
          flex-wrap: wrap;
          -webkit-overflow-scrolling: touch;
          scrollbar-width: none;
        }

        .fleet-chips::-webkit-scrollbar {
          display: none;
        }

        .fleet-chip {
          display: flex;
          align-items: center;
          gap: 6px;
          padding: 5px 12px;
          background: var(--bg-tertiary);
          border: 1px solid var(--border-color);
          border-radius: 16px;
          color: var(--text-secondary);
          font-size: 12px;
          font-weight: 500;
          cursor: pointer;
          transition: all 0.15s;
          white-space: nowrap;
          flex-shrink: 0;
          font-family: inherit;
        }

        .fleet-chip:hover {
          background: var(--bg-card);
          color: var(--text-primary);
          border-color: var(--text-muted);
        }

        .fleet-chip.active {
          background: rgba(61, 90, 254, 0.12);
          border-color: var(--accent-primary);
          color: var(--text-primary);
        }

        .chip-dot {
          width: 6px;
          height: 6px;
          border-radius: 50%;
          flex-shrink: 0;
        }

        .chip-dot.online {
          background: var(--success);
        }

        .chip-dot.offline {
          background: var(--error);
        }

        .chip-dot-all {
          background: var(--accent-secondary);
        }

        .chip-hostname {
          max-width: 120px;
          overflow: hidden;
          text-overflow: ellipsis;
        }

        .chip-ip {
          font-size: 10px;
          font-family: 'JetBrains Mono', monospace;
          color: var(--text-muted);
        }

        .chip-badge {
          font-size: 10px;
          font-family: 'JetBrains Mono', monospace;
          color: var(--text-muted);
          padding: 1px 6px;
          background: var(--bg-primary);
          border-radius: 8px;
        }

        .fleet-chip.active .chip-badge {
          background: rgba(61, 90, 254, 0.2);
          color: var(--accent-secondary);
        }

        @media (max-width: 768px) {
          .fleet-bar {
            padding: 6px 16px;
          }

          .fleet-label {
            display: none;
          }

          .fleet-chips {
            flex-wrap: nowrap;
          }

          .fleet-chip {
            padding: 6px 10px;
            font-size: 11px;
          }
        }

        @media (max-width: 480px) {
          .fleet-bar {
            padding: 6px 12px;
          }
        }
      `}</style>
    </div>
  );
}
