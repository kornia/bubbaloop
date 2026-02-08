import React from 'react';
import { useSortable } from '@dnd-kit/sortable';
import { CSS } from '@dnd-kit/utilities';
import { JsonViewPanel } from './JsonView';

interface SortableJsonCardProps {
  id: string;
  panelName: string; // Kept for compatibility with Dashboard
  topic: string;
  isHidden?: boolean;
  onTopicChange: (topic: string) => void;
  onRemove: () => void;
  availableTopics: Array<{ display: string; raw: string }>;
}

export function SortableJsonCard({
  id,
  topic,
  isHidden = false,
  onTopicChange,
  onRemove,
  availableTopics,
}: SortableJsonCardProps) {
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
      <JsonViewPanel
        topic={topic}
        onTopicChange={onTopicChange}
        onRemove={onRemove}
        availableTopics={availableTopics}
        dragHandleProps={{ ...attributes, ...listeners }}
      />
    </div>
  );
}
