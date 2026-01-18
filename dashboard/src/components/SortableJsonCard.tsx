import React from 'react';
import { useSortable } from '@dnd-kit/sortable';
import { CSS } from '@dnd-kit/utilities';
import { Session } from '@eclipse-zenoh/zenoh-ts';
import { JsonViewPanel } from './JsonView';

interface SortableJsonCardProps {
  id: string;
  session: Session;
  panelName: string;
  topic: string;
  isMaximized: boolean;
  onMaximize: () => void;
  onTopicChange: (topic: string) => void;
  onNameChange: (name: string) => void;
  onRemove: () => void;
  availableTopics: string[];
}

export function SortableJsonCard({
  id,
  session,
  panelName,
  topic,
  isMaximized,
  onMaximize,
  onTopicChange,
  onNameChange,
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
    gridColumn: isMaximized ? '1 / -1' : undefined,
    minWidth: 0,
    overflow: 'hidden',
  };

  return (
    <div ref={setNodeRef} style={style}>
      <JsonViewPanel
        session={session}
        panelName={panelName}
        topic={topic}
        isMaximized={isMaximized}
        onMaximize={onMaximize}
        onTopicChange={onTopicChange}
        onNameChange={onNameChange}
        onRemove={onRemove}
        availableTopics={availableTopics}
        dragHandleProps={{ ...attributes, ...listeners }}
      />
    </div>
  );
}
