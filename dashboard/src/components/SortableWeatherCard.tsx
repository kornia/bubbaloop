import React from 'react';
import { useSortable } from '@dnd-kit/sortable';
import { CSS } from '@dnd-kit/utilities';
import { WeatherViewPanel } from './WeatherView';

interface SortableWeatherCardProps {
  id: string;
  panelName: string; // Kept for compatibility, not used by WeatherViewPanel
  topic: string;     // Kept for compatibility, not used by WeatherViewPanel
  isHidden?: boolean;
  onRemove: () => void;
}

export const SortableWeatherCard = React.memo(function SortableWeatherCard({
  id,
  isHidden = false,
  onRemove,
}: SortableWeatherCardProps) {
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
      <WeatherViewPanel
        onRemove={onRemove}
        dragHandleProps={{ ...attributes, ...listeners }}
      />
    </div>
  );
});
