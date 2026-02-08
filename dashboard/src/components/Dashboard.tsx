import { useState, useCallback, useEffect } from 'react';
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
import { SortableJsonCard } from './SortableJsonCard';
import { SortableWeatherCard } from './SortableWeatherCard';
import { SortableStatsCard } from './SortableStatsCard';
import { SortableNodesCard } from './SortableNodesCard';
import { SortableSystemTelemetryCard } from './SortableSystemTelemetryCard';
import { SortableNetworkMonitorCard } from './SortableNetworkMonitorCard';
import {
  PanelConfig,
  PanelType,
  loadPanels,
  savePanels,
  loadPanelOrder,
  savePanelOrder,
  generatePanelId,
} from '../lib/storage';

interface DashboardProps {
  cameras: Array<{ name: string; topic: string }>;
  availableTopics?: Array<{ display: string; raw: string }>;
}

export function Dashboard({ cameras: initialCameras, availableTopics = [] }: DashboardProps) {
  // Initialize panels from localStorage or props
  const [panels, setPanels] = useState<PanelConfig[]>(() => {
    const stored = loadPanels();
    if (stored && stored.length > 0) {
      return stored;
    }
    // Convert initial cameras to PanelConfig format
    return initialCameras.map((c, i) => ({
      id: `cam-${i}`,
      name: c.name,
      topic: c.topic,
      type: 'camera' as const,
    }));
  });

  // Panel order for drag-and-drop
  const [panelOrder, setPanelOrder] = useState<string[]>(() => {
    const stored = loadPanelOrder();
    if (stored) {
      // Filter out any IDs that don't exist anymore
      const validIds = new Set(panels.map((p) => p.id));
      const filteredOrder = stored.filter((id) => validIds.has(id));
      // Add any new panels that weren't in the order
      const existingIds = new Set(filteredOrder);
      for (const panel of panels) {
        if (!existingIds.has(panel.id)) {
          filteredOrder.push(panel.id);
        }
      }
      return filteredOrder;
    }
    return panels.map((p) => p.id);
  });

  // Maximized panel ID
  const [maximizedId, setMaximizedId] = useState<string | null>(null);

  // Add panel menu state
  const [showAddMenu, setShowAddMenu] = useState(false);

  // Persist panels when they change
  useEffect(() => {
    savePanels(panels);
  }, [panels]);

  // Persist order when it changes
  useEffect(() => {
    savePanelOrder(panelOrder);
  }, [panelOrder]);

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
      setPanelOrder((items) => {
        const oldIndex = items.indexOf(String(active.id));
        const newIndex = items.indexOf(String(over.id));
        return arrayMove(items, oldIndex, newIndex);
      });
    }
  }, []);

  // Update panel
  const updatePanel = useCallback((id: string, updates: Partial<PanelConfig>) => {
    setPanels((prev) =>
      prev.map((panel) => (panel.id === id ? { ...panel, ...updates } : panel))
    );
  }, []);

  // Remove panel
  const removePanel = useCallback((id: string) => {
    setPanels((prev) => prev.filter((panel) => panel.id !== id));
    setPanelOrder((prev) => prev.filter((panelId) => panelId !== id));
    if (maximizedId === id) {
      setMaximizedId(null);
    }
  }, [maximizedId]);

  // Add panel
  const addPanel = useCallback((type: PanelType) => {
    const newId = generatePanelId(type);
    const count = panels.filter((p) => p.type === type).length + 1;
    let newPanel: PanelConfig;

    switch (type) {
      case 'camera':
        newPanel = {
          id: newId,
          name: `Camera ${count}`,
          topic: '',
          type: 'camera',
        };
        break;
      case 'json':
        newPanel = {
          id: newId,
          name: `JSON ${count}`,
          topic: '',
          type: 'json',
        };
        break;
      case 'rawdata':
        newPanel = {
          id: newId,
          name: `Raw Data ${count}`,
          topic: '',
          type: 'rawdata',
        };
        break;
      case 'weather':
        newPanel = {
          id: newId,
          name: `Weather ${count}`,
          topic: '0/weather%current/**',
          type: 'weather',
        };
        break;
      case 'stats':
        newPanel = {
          id: newId,
          name: `Stats ${count}`,
          topic: '',
          type: 'stats',
        };
        break;
      case 'nodes':
        newPanel = {
          id: newId,
          name: `Nodes ${count}`,
          topic: '',
          type: 'nodes',
        };
        break;
      case 'telemetry':
        newPanel = {
          id: newId,
          name: `Telemetry ${count}`,
          topic: '0/system-telemetry%metrics/**',
          type: 'telemetry',
        };
        break;
      case 'network':
        newPanel = {
          id: newId,
          name: `Network ${count}`,
          topic: '0/network-monitor%status/**',
          type: 'network',
        };
        break;
      default:
        return;
    }

    setPanels((prev) => [...prev, newPanel]);
    setPanelOrder((prev) => [...prev, newId]);
    setShowAddMenu(false);
  }, [panels]);

  // Toggle maximize
  const toggleMaximize = useCallback((id: string) => {
    setMaximizedId((prev) => (prev === id ? null : id));
  }, []);

  // Get ordered panels
  const orderedPanels = panelOrder
    .map((id) => panels.find((p) => p.id === id))
    .filter((p): p is PanelConfig => p !== undefined);

  // Calculate grid columns based on number of panels and maximize state
  const getGridCols = () => {
    if (maximizedId) return 1;
    const count = orderedPanels.length;
    if (count <= 1) return 1;
    if (count <= 4) return 2;
    return 3;
  };

  return (
    <div className="dashboard">
      <div className="dashboard-header">
        <span className="dashboard-title">Panels ({panels.length})</span>
        <div className="add-panel-container">
          <button className="add-panel-btn" onClick={() => setShowAddMenu(!showAddMenu)}>
            <svg width="14" height="14" viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="2">
              <path d="M12 5v14M5 12h14" />
            </svg>
            Add Panel
          </button>
          {showAddMenu && (
            <div className="add-panel-menu">
              <button className="add-panel-option" onClick={() => addPanel('camera')}>
                <svg width="16" height="16" viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="2">
                  <path d="M23 19a2 2 0 01-2 2H3a2 2 0 01-2-2V8a2 2 0 012-2h4l2-3h6l2 3h4a2 2 0 012 2z" />
                  <circle cx="12" cy="13" r="4" />
                </svg>
                Camera Panel
              </button>
              <button className="add-panel-option" onClick={() => addPanel('rawdata')}>
                <svg width="16" height="16" viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="2">
                  <path d="M14 2H6a2 2 0 00-2 2v16a2 2 0 002 2h12a2 2 0 002-2V8z" />
                  <path d="M14 2v6h6M16 13H8M16 17H8M10 9H8" />
                </svg>
                Raw Data Panel
              </button>
              <button className="add-panel-option" onClick={() => addPanel('weather')}>
                <svg width="16" height="16" viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="2">
                  <path d="M12 2v2m0 16v2M4.93 4.93l1.41 1.41M17.66 17.66l1.41 1.41M2 12h2m16 0h2M4.93 19.07l1.41-1.41M17.66 6.34l1.41-1.41" />
                  <circle cx="12" cy="12" r="5" />
                </svg>
                Weather Panel
              </button>
              <button className="add-panel-option" onClick={() => addPanel('stats')}>
                <svg width="16" height="16" viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="2">
                  <path d="M18 20V10M12 20V4M6 20v-6" />
                </svg>
                Stats Panel
              </button>
              <button className="add-panel-option" onClick={() => addPanel('nodes')}>
                <svg width="16" height="16" viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="2">
                  <circle cx="12" cy="12" r="3" />
                  <path d="M12 2v4M12 18v4M4.93 4.93l2.83 2.83M16.24 16.24l2.83 2.83M2 12h4M18 12h4M4.93 19.07l2.83-2.83M16.24 7.76l2.83-2.83" />
                </svg>
                Nodes Panel
              </button>
              <button className="add-panel-option" onClick={() => addPanel('telemetry')}>
                <svg width="16" height="16" viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="2">
                  <rect x="2" y="3" width="20" height="14" rx="2" ry="2" />
                  <path d="M8 21h8M12 17v4" />
                </svg>
                Telemetry Panel
              </button>
              <button className="add-panel-option" onClick={() => addPanel('network')}>
                <svg width="16" height="16" viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="2">
                  <path d="M1 12s4-8 11-8 11 8 11 8-4 8-11 8-11-8-11-8z" />
                  <circle cx="12" cy="12" r="3" />
                </svg>
                Network Panel
              </button>
            </div>
          )}
        </div>
      </div>

      {orderedPanels.length > 0 ? (
        <DndContext
          sensors={sensors}
          collisionDetection={closestCenter}
          onDragEnd={handleDragEnd}
        >
          <SortableContext items={panelOrder} strategy={rectSortingStrategy}>
            <div
              className="panel-grid"
              style={{
                gridTemplateColumns: `repeat(${getGridCols()}, 1fr)`,
              }}
            >
              {orderedPanels.map((panel) => {
                const isHidden = maximizedId !== null && maximizedId !== panel.id;
                switch (panel.type) {
                  case 'camera':
                    return (
                      <SortableCameraCard
                        key={panel.id}
                        id={panel.id}
                        cameraName={panel.name}
                        topic={panel.topic}
                        isMaximized={maximizedId === panel.id}
                        isHidden={isHidden}
                        onMaximize={() => toggleMaximize(panel.id)}
                        onTopicChange={(topic) => updatePanel(panel.id, { topic })}
                        onRemove={() => removePanel(panel.id)}
                        availableTopics={availableTopics}
                      />
                    );
                  case 'json':
                  case 'rawdata':
                    return (
                      <SortableJsonCard
                        key={panel.id}
                        id={panel.id}
                        panelName={panel.name}
                        topic={panel.topic}
                        isHidden={isHidden}
                        onTopicChange={(topic) => updatePanel(panel.id, { topic })}
                        onRemove={() => removePanel(panel.id)}
                        availableTopics={availableTopics}
                      />
                    );
                  case 'weather':
                    return (
                      <SortableWeatherCard
                        key={panel.id}
                        id={panel.id}
                        panelName={panel.name}
                        topic={panel.topic}
                        isHidden={isHidden}
                        onRemove={() => removePanel(panel.id)}
                      />
                    );
                  case 'stats':
                    return (
                      <SortableStatsCard
                        key={panel.id}
                        id={panel.id}
                        panelName={panel.name}
                        isHidden={isHidden}
                        onRemove={() => removePanel(panel.id)}
                      />
                    );
                  case 'nodes':
                    return (
                      <SortableNodesCard
                        key={panel.id}
                        id={panel.id}
                        panelName={panel.name}
                        isHidden={isHidden}
                        onRemove={() => removePanel(panel.id)}
                      />
                    );
                  case 'telemetry':
                    return (
                      <SortableSystemTelemetryCard
                        key={panel.id}
                        id={panel.id}
                        panelName={panel.name}
                        isHidden={isHidden}
                        onRemove={() => removePanel(panel.id)}
                      />
                    );
                  case 'network':
                    return (
                      <SortableNetworkMonitorCard
                        key={panel.id}
                        id={panel.id}
                        panelName={panel.name}
                        isHidden={isHidden}
                        onRemove={() => removePanel(panel.id)}
                      />
                    );
                  default:
                    return null;
                }
              })}
            </div>
          </SortableContext>
        </DndContext>
      ) : (
        <div className="no-panels">
          <div className="no-panels-content">
            <span className="no-panels-icon">📊</span>
            <h3>No panels configured</h3>
            <p>Click "Add Panel" to start adding camera, raw data, or weather panels</p>
            <div className="no-panels-buttons">
              <button className="add-panel-btn large" onClick={() => addPanel('camera')}>
                <svg width="16" height="16" viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="2">
                  <path d="M23 19a2 2 0 01-2 2H3a2 2 0 01-2-2V8a2 2 0 012-2h4l2-3h6l2 3h4a2 2 0 012 2z" />
                  <circle cx="12" cy="13" r="4" />
                </svg>
                Add Camera
              </button>
              <button className="add-panel-btn large" onClick={() => addPanel('rawdata')}>
                <svg width="16" height="16" viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="2">
                  <path d="M14 2H6a2 2 0 00-2 2v16a2 2 0 002 2h12a2 2 0 002-2V8z" />
                  <path d="M14 2v6h6M16 13H8M16 17H8M10 9H8" />
                </svg>
                Add Raw Data
              </button>
              <button className="add-panel-btn large" onClick={() => addPanel('weather')}>
                <svg width="16" height="16" viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="2">
                  <path d="M12 2v2m0 16v2M4.93 4.93l1.41 1.41M17.66 17.66l1.41 1.41M2 12h2m16 0h2M4.93 19.07l1.41-1.41M17.66 6.34l1.41-1.41" />
                  <circle cx="12" cy="12" r="5" />
                </svg>
                Add Weather
              </button>
              <button className="add-panel-btn large" onClick={() => addPanel('stats')}>
                <svg width="16" height="16" viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="2">
                  <path d="M18 20V10M12 20V4M6 20v-6" />
                </svg>
                Add Stats
              </button>
              <button className="add-panel-btn large" onClick={() => addPanel('nodes')}>
                <svg width="16" height="16" viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="2">
                  <circle cx="12" cy="12" r="3" />
                  <path d="M12 2v4M12 18v4M4.93 4.93l2.83 2.83M16.24 16.24l2.83 2.83M2 12h4M18 12h4M4.93 19.07l2.83-2.83M16.24 7.76l2.83-2.83" />
                </svg>
                Add Nodes
              </button>
              <button className="add-panel-btn large" onClick={() => addPanel('telemetry')}>
                <svg width="16" height="16" viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="2">
                  <rect x="2" y="3" width="20" height="14" rx="2" ry="2" />
                  <path d="M8 21h8M12 17v4" />
                </svg>
                Add Telemetry
              </button>
              <button className="add-panel-btn large" onClick={() => addPanel('network')}>
                <svg width="16" height="16" viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="2">
                  <path d="M1 12s4-8 11-8 11 8 11 8-4 8-11 8-11-8-11-8z" />
                  <circle cx="12" cy="12" r="3" />
                </svg>
                Add Network
              </button>
            </div>
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

        .add-panel-container {
          position: relative;
        }

        .add-panel-btn {
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

        .add-panel-btn:hover {
          background: var(--bg-card);
          border-color: var(--accent-primary);
          color: var(--accent-primary);
        }

        .add-panel-btn.large {
          padding: 12px 24px;
          font-size: 14px;
        }

        .add-panel-menu {
          position: absolute;
          top: 100%;
          right: 0;
          margin-top: 4px;
          background: var(--bg-card);
          border: 1px solid var(--border-color);
          border-radius: 8px;
          box-shadow: 0 4px 20px rgba(0, 0, 0, 0.3);
          overflow: hidden;
          z-index: 100;
          min-width: 160px;
        }

        .add-panel-option {
          display: flex;
          align-items: center;
          gap: 10px;
          width: 100%;
          padding: 12px 16px;
          background: transparent;
          border: none;
          color: var(--text-secondary);
          font-size: 13px;
          cursor: pointer;
          transition: all 0.15s;
          text-align: left;
        }

        .add-panel-option:hover {
          background: var(--bg-tertiary);
          color: var(--text-primary);
        }

        .add-panel-option:not(:last-child) {
          border-bottom: 1px solid var(--border-color);
        }

        .panel-grid {
          display: grid;
          gap: 24px;
          max-width: 1800px;
          margin: 0 auto;
          overflow: hidden;
        }

        .panel-grid > * {
          min-width: 0;
        }

        .no-panels {
          display: flex;
          align-items: center;
          justify-content: center;
          min-height: 400px;
        }

        .no-panels-content {
          text-align: center;
          color: var(--text-muted);
        }

        .no-panels-icon {
          font-size: 48px;
          display: block;
          margin-bottom: 16px;
        }

        .no-panels-content h3 {
          font-size: 18px;
          font-weight: 500;
          color: var(--text-secondary);
          margin-bottom: 8px;
        }

        .no-panels-content p {
          font-size: 14px;
          margin-bottom: 20px;
        }

        .no-panels-buttons {
          display: flex;
          gap: 12px;
          justify-content: center;
          flex-wrap: wrap;
        }

        /* Mobile responsive styles */
        @media (max-width: 768px) {
          .dashboard {
            padding: 12px;
          }

          .dashboard-header {
            margin-bottom: 12px;
          }

          .dashboard-title {
            font-size: 12px;
          }

          .add-panel-btn {
            padding: 10px 14px;
            font-size: 12px;
          }

          .add-panel-btn.large {
            padding: 10px 16px;
            font-size: 13px;
          }

          .add-panel-menu {
            min-width: 140px;
          }

          .add-panel-option {
            padding: 14px 16px;
            font-size: 14px;
          }

          .panel-grid {
            gap: 12px;
            grid-template-columns: 1fr !important;
          }

          .no-panels {
            min-height: 300px;
            padding: 20px;
          }

          .no-panels-icon {
            font-size: 36px;
          }

          .no-panels-content h3 {
            font-size: 16px;
          }

          .no-panels-content p {
            font-size: 13px;
          }

          .no-panels-buttons {
            flex-direction: column;
            align-items: stretch;
          }

          .no-panels-buttons .add-panel-btn {
            justify-content: center;
          }
        }

        @media (max-width: 480px) {
          .dashboard {
            padding: 8px;
          }

          .panel-grid {
            gap: 8px;
          }
        }
      `}</style>
    </div>
  );
}
