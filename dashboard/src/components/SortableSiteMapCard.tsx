import React from 'react';
import { useSortable } from '@dnd-kit/sortable';
import { CSS } from '@dnd-kit/utilities';
import { SiteMapView } from './SiteMapView';

interface SortableSiteMapCardProps {
  id: string;
  panelName: string;
  topic: string;
  isHidden?: boolean;
  onRemove: () => void;
}

export const SortableSiteMapCard = React.memo(function SortableSiteMapCard({
  id,
  topic,
  isHidden = false,
  onRemove,
}: SortableSiteMapCardProps) {
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
      <SiteMapView
        topic={topic}
        onRemove={onRemove}
        dragHandleProps={{ ...attributes, ...listeners }}
      />
    </div>
  );
});
