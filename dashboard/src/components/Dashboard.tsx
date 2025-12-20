import { useState, useCallback, useEffect } from 'react';
import { Session } from '@eclipse-zenoh/zenoh-ts';
import {
  DndContext,
  closestCenter,
  KeyboardSensor,
  PointerSensor,
  useSensor,
  useSensors,
  DragEndEvent,
} from '@dnd-kit/core';
import {
  arrayMove,
  SortableContext,
  sortableKeyboardCoordinates,
  rectSortingStrategy,
} from '@dnd-kit/sortable';
import { SortableCameraCard } from './SortableCameraCard';
import {
  CameraConfig,
  loadCameras,
  saveCameras,
  loadCameraOrder,
  saveCameraOrder,
  generateCameraId,
} from '../lib/storage';

interface DashboardProps {
  session: Session;
  cameras: Array<{ name: string; topic: string }>;
  availableTopics?: string[];
}

export function Dashboard({ session, cameras: initialCameras, availableTopics = [] }: DashboardProps) {
  // Initialize cameras from localStorage or props
  const [cameras, setCameras] = useState<CameraConfig[]>(() => {
    const stored = loadCameras();
    if (stored && stored.length > 0) {
      return stored;
    }
    // Convert initial cameras to CameraConfig format
    return initialCameras.map((c, i) => ({
      id: `cam-${i}`,
      name: c.name,
      topic: c.topic,
    }));
  });

  // Camera order for drag-and-drop
  const [cameraOrder, setCameraOrder] = useState<string[]>(() => {
    const stored = loadCameraOrder();
    if (stored) {
      // Filter out any IDs that don't exist anymore
      const validIds = new Set(cameras.map((c) => c.id));
      const filteredOrder = stored.filter((id) => validIds.has(id));
      // Add any new cameras that weren't in the order
      const existingIds = new Set(filteredOrder);
      for (const cam of cameras) {
        if (!existingIds.has(cam.id)) {
          filteredOrder.push(cam.id);
        }
      }
      return filteredOrder;
    }
    return cameras.map((c) => c.id);
  });

  // Maximized camera ID
  const [maximizedId, setMaximizedId] = useState<string | null>(null);

  // Persist cameras when they change
  useEffect(() => {
    saveCameras(cameras);
  }, [cameras]);

  // Persist order when it changes
  useEffect(() => {
    saveCameraOrder(cameraOrder);
  }, [cameraOrder]);

  // DnD sensors
  const sensors = useSensors(
    useSensor(PointerSensor, {
      activationConstraint: {
        distance: 8,
      },
    }),
    useSensor(KeyboardSensor, {
      coordinateGetter: sortableKeyboardCoordinates,
    })
  );

  // Handle drag end
  const handleDragEnd = useCallback((event: DragEndEvent) => {
    const { active, over } = event;
    if (over && active.id !== over.id) {
      setCameraOrder((items) => {
        const oldIndex = items.indexOf(String(active.id));
        const newIndex = items.indexOf(String(over.id));
        return arrayMove(items, oldIndex, newIndex);
      });
    }
  }, []);

  // Update camera
  const updateCamera = useCallback((id: string, updates: Partial<CameraConfig>) => {
    setCameras((prev) =>
      prev.map((cam) => (cam.id === id ? { ...cam, ...updates } : cam))
    );
  }, []);

  // Remove camera
  const removeCamera = useCallback((id: string) => {
    setCameras((prev) => prev.filter((cam) => cam.id !== id));
    setCameraOrder((prev) => prev.filter((camId) => camId !== id));
    if (maximizedId === id) {
      setMaximizedId(null);
    }
  }, [maximizedId]);

  // Add camera
  const addCamera = useCallback(() => {
    const newId = generateCameraId();
    const newCamera: CameraConfig = {
      id: newId,
      name: `Camera ${cameras.length + 1}`,
      topic: '',
    };
    setCameras((prev) => [...prev, newCamera]);
    setCameraOrder((prev) => [...prev, newId]);
  }, [cameras.length]);

  // Toggle maximize
  const toggleMaximize = useCallback((id: string) => {
    setMaximizedId((prev) => (prev === id ? null : id));
  }, []);

  // Get ordered cameras
  const orderedCameras = cameraOrder
    .map((id) => cameras.find((c) => c.id === id))
    .filter((c): c is CameraConfig => c !== undefined);

  // Calculate grid columns based on number of cameras and maximize state
  const getGridCols = () => {
    if (maximizedId) return 1;
    const count = orderedCameras.length;
    if (count <= 1) return 1;
    if (count <= 4) return 2;
    return 3;
  };

  return (
    <div className="dashboard">
      <div className="dashboard-header">
        <span className="dashboard-title">Cameras ({cameras.length})</span>
        <button className="add-camera-btn" onClick={addCamera}>
          <svg width="14" height="14" viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="2">
            <path d="M12 5v14M5 12h14" />
          </svg>
          Add Camera
        </button>
      </div>

      {orderedCameras.length > 0 ? (
        <DndContext
          sensors={sensors}
          collisionDetection={closestCenter}
          onDragEnd={handleDragEnd}
        >
          <SortableContext items={cameraOrder} strategy={rectSortingStrategy}>
            <div
              className="camera-grid"
              style={{
                gridTemplateColumns: `repeat(${getGridCols()}, 1fr)`,
              }}
            >
              {orderedCameras.map((camera) => (
                <SortableCameraCard
                  key={camera.id}
                  id={camera.id}
                  session={session}
                  cameraName={camera.name}
                  topic={camera.topic}
                  isMaximized={maximizedId === camera.id}
                  onMaximize={() => toggleMaximize(camera.id)}
                  onTopicChange={(topic) => updateCamera(camera.id, { topic })}
                  onNameChange={(name) => updateCamera(camera.id, { name })}
                  onRemove={() => removeCamera(camera.id)}
                  availableTopics={availableTopics}
                />
              ))}
            </div>
          </SortableContext>
        </DndContext>
      ) : (
        <div className="no-cameras">
          <div className="no-cameras-content">
            <span className="no-cameras-icon">📹</span>
            <h3>No cameras configured</h3>
            <p>Click "Add Camera" to start adding camera streams</p>
            <button className="add-camera-btn large" onClick={addCamera}>
              <svg width="16" height="16" viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="2">
                <path d="M12 5v14M5 12h14" />
              </svg>
              Add Camera
            </button>
          </div>
        </div>
      )}

      <style>{`
        .dashboard {
          flex: 1;
          padding: 24px;
          overflow: auto;
        }

        .dashboard-header {
          display: flex;
          justify-content: space-between;
          align-items: center;
          margin-bottom: 20px;
          max-width: 1800px;
          margin-left: auto;
          margin-right: auto;
        }

        .dashboard-title {
          font-size: 14px;
          font-weight: 500;
          color: var(--text-secondary);
        }

        .add-camera-btn {
          display: flex;
          align-items: center;
          gap: 6px;
          padding: 8px 16px;
          background: var(--bg-tertiary);
          border: 1px solid var(--border-color);
          border-radius: 8px;
          color: var(--text-secondary);
          font-size: 13px;
          font-weight: 500;
          cursor: pointer;
          transition: all 0.15s;
        }

        .add-camera-btn:hover {
          background: var(--bg-card);
          border-color: var(--accent-primary);
          color: var(--accent-primary);
        }

        .add-camera-btn.large {
          padding: 12px 24px;
          font-size: 14px;
        }

        .camera-grid {
          display: grid;
          gap: 24px;
          max-width: 1800px;
          margin: 0 auto;
          overflow: hidden;
        }

        .camera-grid > * {
          min-width: 0;
        }

        .no-cameras {
          display: flex;
          align-items: center;
          justify-content: center;
          min-height: 400px;
        }

        .no-cameras-content {
          text-align: center;
          color: var(--text-muted);
        }

        .no-cameras-icon {
          font-size: 48px;
          display: block;
          margin-bottom: 16px;
        }

        .no-cameras-content h3 {
          font-size: 18px;
          font-weight: 500;
          color: var(--text-secondary);
          margin-bottom: 8px;
        }

        .no-cameras-content p {
          font-size: 14px;
          margin-bottom: 20px;
        }
      `}</style>
    </div>
  );
}
