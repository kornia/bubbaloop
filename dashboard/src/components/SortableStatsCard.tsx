import React from 'react';
import { useSortable } from '@dnd-kit/sortable';
import { CSS } from '@dnd-kit/utilities';
import { Session } from '@eclipse-zenoh/zenoh-ts';
import { StatsViewPanel } from './StatsView';

interface SortableStatsCardProps {
  id: string;
  session: Session;
  panelName: string; // Kept for compatibility with Dashboard
  isMaximized: boolean;
  onMaximize: () => void;
  onRemove: () => void;
}

export function SortableStatsCard({
  id,
  session,
  isMaximized,
  onMaximize,
  onRemove,
}: SortableStatsCardProps) {
  const {
    attributes,
    listeners,
    setNodeRef,
    transform,
    transition,
    isDragging,
  } = useSortable({ id });

  const style: React.CSSProperties = {
    transform: CSS.Transform.toString(transform),
    transition,
    opacity: isDragging ? 0.5 : 1,
    gridColumn: isMaximized ? '1 / -1' : undefined,
    minWidth: 0,
    overflow: 'hidden',
  };

  return (
    <div ref={setNodeRef} style={style}>
      <StatsViewPanel
        session={session}
        isMaximized={isMaximized}
        onMaximize={onMaximize}
        onRemove={onRemove}
        dragHandleProps={{ ...attributes, ...listeners }}
      />
    </div>
  );
}
