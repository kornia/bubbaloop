import React from 'react';
import { useSortable } from '@dnd-kit/sortable';
import { CSS } from '@dnd-kit/utilities';
import { Session } from '@eclipse-zenoh/zenoh-ts';
import { WeatherViewPanel } from './WeatherView';

interface SortableWeatherCardProps {
  id: string;
  session: Session;
  panelName: string;
  topic: string;
  isMaximized: boolean;
  onMaximize: () => void;
  onNameChange: (name: string) => void;
  onRemove: () => void;
}

export function SortableWeatherCard({
  id,
  session,
  panelName,
  topic,
  isMaximized,
  onMaximize,
  onNameChange,
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
    gridColumn: isMaximized ? '1 / -1' : undefined,
    minWidth: 0,
    overflow: 'hidden',
  };

  return (
    <div ref={setNodeRef} style={style}>
      <WeatherViewPanel
        session={session}
        panelName={panelName}
        topic={topic}
        isMaximized={isMaximized}
        onMaximize={onMaximize}
        onNameChange={onNameChange}
        onRemove={onRemove}
        dragHandleProps={{ ...attributes, ...listeners }}
      />
    </div>
  );
}
