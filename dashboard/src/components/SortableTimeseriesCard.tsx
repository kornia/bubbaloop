import React from 'react';
import { useSortable } from '@dnd-kit/sortable';
import { CSS } from '@dnd-kit/utilities';
import { TimeseriesView } from './TimeseriesView';

interface SortableTimeseriesCardProps {
  id: string;
  panelName: string;
  topic: string;
  windowSecs?: number;
  isHidden?: boolean;
  onTopicChange: (topic: string) => void;
  onRemove: () => void;
  availableTopics: Array<{ display: string; raw: string }>;
}

export const SortableTimeseriesCard = React.memo(function SortableTimeseriesCard({
  id,
  panelName,
  topic,
  windowSecs,
  isHidden = false,
  onTopicChange,
  onRemove,
  availableTopics,
}: SortableTimeseriesCardProps) {
  const { attributes, listeners, setNodeRef, transform, transition, isDragging } = useSortable({ id });

  const style: React.CSSProperties = {
    transform: CSS.Transform.toString(transform),
    transition,
    opacity: isDragging ? 0.5 : 1,
    minWidth: 0,
    overflow: 'hidden',
    display: isHidden ? 'none' : undefined,
    height: 220,
  };

  return (
    <div ref={setNodeRef} style={style}>
      <TimeseriesView
        panelName={panelName}
        topic={topic}
        windowSecs={windowSecs}
        isHidden={isHidden}
        onTopicChange={onTopicChange}
        onRemove={onRemove}
        availableTopics={availableTopics}
        dragHandleProps={{ ...attributes, ...listeners }}
      />
    </div>
  );
});
