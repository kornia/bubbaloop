import React from 'react';
import { useSortable } from '@dnd-kit/sortable';
import { CSS } from '@dnd-kit/utilities';
import { NetworkMonitorViewPanel } from './NetworkMonitorView';

interface SortableNetworkMonitorCardProps {
  id: string;
  panelName: string;
  isHidden?: boolean;
  onRemove: () => void;
}

export function SortableNetworkMonitorCard({
  id,
  isHidden = false,
  onRemove,
}: SortableNetworkMonitorCardProps) {
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
      <NetworkMonitorViewPanel
        onRemove={onRemove}
        dragHandleProps={{ ...attributes, ...listeners }}
      />
    </div>
  );
}
