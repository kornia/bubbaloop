import React from 'react';
import { useSortable } from '@dnd-kit/sortable';
import { CSS } from '@dnd-kit/utilities';
import { StatsViewPanel } from './StatsView';

interface SortableStatsCardProps {
  id: string;
  panelName: string; // Kept for compatibility with Dashboard
  isHidden?: boolean;
  onRemove: () => void;
}

export const SortableStatsCard = React.memo(function SortableStatsCard({
  id,
  isHidden = false,
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
    minWidth: 0,
    overflow: 'hidden',
    display: isHidden ? 'none' : undefined,
  };

  return (
    <div ref={setNodeRef} style={style}>
      <StatsViewPanel
        onRemove={onRemove}
        dragHandleProps={{ ...attributes, ...listeners }}
      />
    </div>
  );
});
