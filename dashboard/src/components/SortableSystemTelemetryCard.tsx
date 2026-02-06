import React from 'react';
import { useSortable } from '@dnd-kit/sortable';
import { CSS } from '@dnd-kit/utilities';
import { SystemTelemetryViewPanel } from './SystemTelemetryView';

interface SortableSystemTelemetryCardProps {
  id: string;
  panelName: string;
  isHidden?: boolean;
  onRemove: () => void;
}

export function SortableSystemTelemetryCard({
  id,
  isHidden = false,
  onRemove,
}: SortableSystemTelemetryCardProps) {
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
      <SystemTelemetryViewPanel
        onRemove={onRemove}
        dragHandleProps={{ ...attributes, ...listeners }}
      />
    </div>
  );
}
