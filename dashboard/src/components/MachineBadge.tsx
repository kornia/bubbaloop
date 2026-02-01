import { type MachineInfo } from '../contexts/FleetContext';

interface MachineBadgeProps {
  machines: MachineInfo[];
}

export function MachineBadge({ machines }: MachineBadgeProps) {
  if (machines.length === 0) return null;

  const title = machines
    .map(m => `${m.hostname} (${m.ips[0] || ''})`)
    .join(', ');

  const label = machines.length === 1
    ? machines[0].hostname
    : `${machines.length} machines`;

  return (
    <span className="panel-machine-badge" title={title}>
      {label}
    </span>
  );
}
