import React from 'react';
import { useSortable } from '@dnd-kit/sortable';
import { CSS } from '@dnd-kit/utilities';
import { CameraView } from './CameraView';

interface SortableCameraCardProps {
  id: string;
  cameraName: string;
  topic: string;
  isMaximized: boolean;
  isHidden?: boolean;
  onMaximize: () => void;
  onTopicChange: (topic: string) => void;
  onRemove: () => void;
  availableTopics: Array<{ display: string; raw: string }>;
}

export function SortableCameraCard({
  id,
  cameraName,
  topic,
  isMaximized,
  isHidden = false,
  onMaximize,
  onTopicChange,
  onRemove,
  availableTopics,
}: SortableCameraCardProps) {
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
    display: isHidden ? 'none' : undefined,
  };

  return (
    <div ref={setNodeRef} style={style}>
      <CameraView
        cameraName={cameraName}
        topic={topic}
        isMaximized={isMaximized}
        onMaximize={onMaximize}
        onTopicChange={onTopicChange}
        onRemove={onRemove}
        availableTopics={availableTopics}
        dragHandleProps={{ ...attributes, ...listeners }}
      />
    </div>
  );
}
