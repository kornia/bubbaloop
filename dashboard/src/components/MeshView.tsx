import { useState, useEffect, useRef, useCallback, useMemo } from 'react';
import { useFleetContext, type MachineInfo, type FleetNodeInfo } from '../contexts/FleetContext';

// ---------------------------------------------------------------------------
// Types
// ---------------------------------------------------------------------------

interface SimNode {
  id: string;
  type: 'hub' | 'machine' | 'service';
  x: number;
  y: number;
  vx: number;
  vy: number;
  fx: number | null; // fixed x (while dragging)
  fy: number | null;
  radius: number;
  data: HubData | MachineInfo | FleetNodeInfo;
}

interface HubData {
  label: string;
}

interface SimEdge {
  source: string;
  target: string;
  type: 'mesh' | 'service';
}

interface TooltipInfo {
  x: number;
  y: number;
  node: SimNode;
}

// ---------------------------------------------------------------------------
// Constants
// ---------------------------------------------------------------------------

const HUB_RADIUS = 36;
const MACHINE_RADIUS = 28;
const SERVICE_RADIUS = 10;

const STATUS_COLORS: Record<string, string> = {
  running: '#00c853',
  stopped: '#9090a0',
  failed: '#ff1744',
  building: '#ffd600',
  installing: '#ffd600',
  'not-installed': '#606070',
  unknown: '#606070',
};

// ---------------------------------------------------------------------------
// Force simulation helpers
// ---------------------------------------------------------------------------

function distance(a: SimNode, b: SimNode): number {
  const dx = a.x - b.x;
  const dy = a.y - b.y;
  return Math.sqrt(dx * dx + dy * dy) || 1;
}

function applyGravity(nodes: SimNode[], cx: number, cy: number, strength: number) {
  for (const n of nodes) {
    if (n.fx !== null) continue;
    n.vx += (cx - n.x) * strength;
    n.vy += (cy - n.y) * strength;
  }
}

function applyRepulsion(nodes: SimNode[], strength: number) {
  for (let i = 0; i < nodes.length; i++) {
    for (let j = i + 1; j < nodes.length; j++) {
      const a = nodes[i];
      const b = nodes[j];
      if (a.fx !== null && b.fx !== null) continue;
      const d = distance(a, b);
      const minDist = a.radius + b.radius + 40;
      if (d < minDist) {
        const force = (strength * (minDist - d)) / d;
        const dx = (a.x - b.x) * force;
        const dy = (a.y - b.y) * force;
        if (a.fx === null) { a.vx += dx; a.vy += dy; }
        if (b.fx === null) { b.vx -= dx; b.vy -= dy; }
      }
    }
  }
}

function applySpringForce(_nodes: SimNode[], edges: SimEdge[], nodeMap: Map<string, SimNode>) {
  for (const edge of edges) {
    const source = nodeMap.get(edge.source);
    const target = nodeMap.get(edge.target);
    if (!source || !target) continue;

    const idealLen = edge.type === 'mesh' ? 180 : 80;
    const stiffness = edge.type === 'mesh' ? 0.004 : 0.008;

    const d = distance(source, target);
    const displacement = d - idealLen;
    const force = displacement * stiffness;
    const dx = ((source.x - target.x) / d) * force;
    const dy = ((source.y - target.y) / d) * force;

    if (source.fx === null) { source.vx -= dx; source.vy -= dy; }
    if (target.fx === null) { target.vx += dx; target.vy += dy; }
  }
}

function applyDamping(nodes: SimNode[], factor: number) {
  for (const n of nodes) {
    n.vx *= factor;
    n.vy *= factor;
  }
}

function updatePositions(nodes: SimNode[]): number {
  let totalKinetic = 0;
  for (const n of nodes) {
    if (n.fx !== null) { n.x = n.fx; n.y = n.fy!; n.vx = 0; n.vy = 0; continue; }
    // Clamp velocity
    const maxV = 8;
    n.vx = Math.max(-maxV, Math.min(maxV, n.vx));
    n.vy = Math.max(-maxV, Math.min(maxV, n.vy));
    n.x += n.vx;
    n.y += n.vy;
    totalKinetic += n.vx * n.vx + n.vy * n.vy;
  }
  return totalKinetic;
}

// ---------------------------------------------------------------------------
// Build simulation graph from fleet data
// ---------------------------------------------------------------------------

function buildGraph(machines: MachineInfo[], fleetNodes: FleetNodeInfo[], cx: number, cy: number): { nodes: SimNode[]; edges: SimEdge[] } {
  const simNodes: SimNode[] = [];
  const edges: SimEdge[] = [];

  // Hub
  simNodes.push({
    id: '__hub__',
    type: 'hub',
    x: cx,
    y: cy,
    vx: 0, vy: 0,
    fx: cx, fy: cy,
    radius: HUB_RADIUS,
    data: { label: 'Zenoh Mesh' } as HubData,
  });

  // Machines
  const machineCount = machines.length;
  machines.forEach((m, i) => {
    const angle = (2 * Math.PI * i) / (machineCount || 1) - Math.PI / 2;
    const r = 180;
    simNodes.push({
      id: `machine-${m.machineId}`,
      type: 'machine',
      x: cx + Math.cos(angle) * r,
      y: cy + Math.sin(angle) * r,
      vx: 0, vy: 0,
      fx: null, fy: null,
      radius: MACHINE_RADIUS,
      data: m,
    });
    edges.push({ source: '__hub__', target: `machine-${m.machineId}`, type: 'mesh' });
  });

  // Service nodes
  const nodesByMachine = new Map<string, FleetNodeInfo[]>();
  for (const n of fleetNodes) {
    const mid = n.machineId || 'local';
    if (!nodesByMachine.has(mid)) nodesByMachine.set(mid, []);
    nodesByMachine.get(mid)!.push(n);
  }

  for (const [mid, serviceNodes] of nodesByMachine.entries()) {
    const parentId = `machine-${mid}`;
    const parent = simNodes.find(n => n.id === parentId);
    if (!parent) continue;

    const count = serviceNodes.length;
    serviceNodes.forEach((sn, i) => {
      const angle = (2 * Math.PI * i) / (count || 1) - Math.PI / 2;
      const r = 65;
      simNodes.push({
        id: `service-${mid}-${sn.name}`,
        type: 'service',
        x: parent.x + Math.cos(angle) * r,
        y: parent.y + Math.sin(angle) * r,
        vx: 0, vy: 0,
        fx: null, fy: null,
        radius: SERVICE_RADIUS,
        data: sn,
      });
      edges.push({ source: parentId, target: `service-${mid}-${sn.name}`, type: 'service' });
    });
  }

  return { nodes: simNodes, edges };
}

// ---------------------------------------------------------------------------
// Component
// ---------------------------------------------------------------------------

export function MeshView() {
  const { machines, nodes: fleetNodes } = useFleetContext();
  const svgRef = useRef<SVGSVGElement>(null);
  const containerRef = useRef<HTMLDivElement>(null);

  // Simulation state stored in refs for performance (avoid re-renders during animation)
  const simNodesRef = useRef<SimNode[]>([]);
  const simEdgesRef = useRef<SimEdge[]>([]);
  const nodeMapRef = useRef<Map<string, SimNode>>(new Map());
  const animRef = useRef<number>(0);
  const isSettledRef = useRef(false);

  // Viewport (pan/zoom)
  const [viewBox, setViewBox] = useState({ x: 0, y: 0, w: 1200, h: 800 });
  const viewBoxRef = useRef(viewBox);
  viewBoxRef.current = viewBox;

  // Drag state
  const [dragNodeId, setDragNodeId] = useState<string | null>(null);
  const dragNodeIdRef = useRef<string | null>(null);
  dragNodeIdRef.current = dragNodeId;

  // Pan state
  const isPanningRef = useRef(false);
  const panStartRef = useRef({ x: 0, y: 0, vbx: 0, vby: 0 });

  // Tooltip
  const [tooltip, setTooltip] = useState<TooltipInfo | null>(null);

  // Selected machine
  const [selectedMachineId, setSelectedMachineId] = useState<string | null>(null);

  // Dimensions
  const [dims, setDims] = useState({ w: 1200, h: 800 });
  useEffect(() => {
    const el = containerRef.current;
    if (!el) return;
    const obs = new ResizeObserver(entries => {
      for (const entry of entries) {
        const { width, height } = entry.contentRect;
        if (width > 0 && height > 0) {
          setDims({ w: width, h: height });
        }
      }
    });
    obs.observe(el);
    return () => obs.disconnect();
  }, []);

  // Rebuild graph when machines/nodes change
  const graphKey = useMemo(() => {
    const machineIds = machines.map(m => m.machineId).sort().join(',');
    const nodeIds = fleetNodes.map(n => `${n.machineId}:${n.name}`).sort().join(',');
    return `${machineIds}|${nodeIds}`;
  }, [machines, fleetNodes]);

  useEffect(() => {
    const cx = dims.w / 2;
    const cy = dims.h / 2;
    const { nodes, edges } = buildGraph(machines, fleetNodes, cx, cy);

    // Preserve positions for nodes that already exist
    const oldMap = nodeMapRef.current;
    for (const n of nodes) {
      const old = oldMap.get(n.id);
      if (old) {
        n.x = old.x;
        n.y = old.y;
        n.vx = old.vx;
        n.vy = old.vy;
        if (n.type === 'hub') {
          n.fx = cx;
          n.fy = cy;
          n.x = cx;
          n.y = cy;
        }
      }
    }

    simNodesRef.current = nodes;
    simEdgesRef.current = edges;
    const map = new Map<string, SimNode>();
    for (const n of nodes) map.set(n.id, n);
    nodeMapRef.current = map;
    isSettledRef.current = false;

    // Update viewBox to center
    setViewBox({ x: 0, y: 0, w: dims.w, h: dims.h });
  // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [graphKey, dims.w, dims.h]);

  // Update data on existing nodes without rebuilding (for status changes)
  useEffect(() => {
    const map = nodeMapRef.current;
    for (const m of machines) {
      const node = map.get(`machine-${m.machineId}`);
      if (node) node.data = m;
    }
    for (const fn of fleetNodes) {
      const node = map.get(`service-${fn.machineId}-${fn.name}`);
      if (node) node.data = fn;
    }
  }, [machines, fleetNodes]);

  // Render loop
  const render = useCallback(() => {
    const svg = svgRef.current;
    if (!svg) return;

    const nodes = simNodesRef.current;
    const edges = simEdgesRef.current;

    // Render edges
    const edgeGroup = svg.querySelector('.edges') as SVGGElement | null;
    if (edgeGroup) {
      // Reconcile edge elements
      while (edgeGroup.children.length > edges.length) {
        edgeGroup.removeChild(edgeGroup.lastChild!);
      }
      while (edgeGroup.children.length < edges.length) {
        const line = document.createElementNS('http://www.w3.org/2000/svg', 'line');
        edgeGroup.appendChild(line);
      }
      for (let i = 0; i < edges.length; i++) {
        const edge = edges[i];
        const line = edgeGroup.children[i] as SVGLineElement;
        const src = nodeMapRef.current.get(edge.source);
        const tgt = nodeMapRef.current.get(edge.target);
        if (!src || !tgt) continue;
        line.setAttribute('x1', String(src.x));
        line.setAttribute('y1', String(src.y));
        line.setAttribute('x2', String(tgt.x));
        line.setAttribute('y2', String(tgt.y));

        const isMesh = edge.type === 'mesh';
        const isSelected = selectedMachineId && (
          edge.target === `machine-${selectedMachineId}` ||
          edge.source === `machine-${selectedMachineId}` ||
          tgt.id.startsWith(`service-${selectedMachineId}-`) ||
          src.id.startsWith(`service-${selectedMachineId}-`)
        );

        line.setAttribute('stroke', isMesh
          ? (isSelected ? 'rgba(61,90,254,0.7)' : 'rgba(61,90,254,0.25)')
          : (isSelected ? 'rgba(0,229,255,0.6)' : 'rgba(0,229,255,0.15)'));
        line.setAttribute('stroke-width', isMesh ? '2' : '1');
        line.setAttribute('stroke-dasharray', isMesh ? '6 4' : '3 3');
        line.setAttribute('class', 'sim-edge');
      }
    }

    // Render nodes
    const nodeGroup = svg.querySelector('.nodes') as SVGGElement | null;
    if (nodeGroup) {
      // We use a keyed approach: create elements on first pass, then update
      const existingMap = new Map<string, SVGGElement>();
      for (const child of Array.from(nodeGroup.children)) {
        const id = child.getAttribute('data-id');
        if (id) existingMap.set(id, child as SVGGElement);
      }

      for (const node of nodes) {
        let g = existingMap.get(node.id);
        if (!g) {
          g = document.createElementNS('http://www.w3.org/2000/svg', 'g');
          g.setAttribute('data-id', node.id);
          g.setAttribute('class', `sim-node sim-node-${node.type}`);
          nodeGroup.appendChild(g);
          // Build initial structure
          if (node.type === 'hub') {
            buildHubNode(g);
          } else if (node.type === 'machine') {
            buildMachineNode(g);
          } else {
            buildServiceNode(g);
          }
        }

        // Update transform
        g.setAttribute('transform', `translate(${node.x}, ${node.y})`);

        // Update data-dependent attributes
        if (node.type === 'machine') {
          updateMachineNode(g, node.data as MachineInfo, selectedMachineId === (node.data as MachineInfo).machineId);
        } else if (node.type === 'service') {
          updateServiceNode(g, node.data as FleetNodeInfo,
            selectedMachineId === (node.data as FleetNodeInfo).machineId);
        }

        existingMap.delete(node.id);
      }

      // Remove stale
      for (const stale of existingMap.values()) {
        nodeGroup.removeChild(stale);
      }
    }
  }, [selectedMachineId]);

  // Animation loop
  useEffect(() => {
    let running = true;
    const cx = dims.w / 2;
    const cy = dims.h / 2;

    const tick = () => {
      if (!running) return;

      const nodes = simNodesRef.current;
      const edges = simEdgesRef.current;

      if (nodes.length > 0 && !isSettledRef.current) {
        applyGravity(nodes, cx, cy, 0.002);
        applyRepulsion(nodes, 2.5);
        applySpringForce(nodes, edges, nodeMapRef.current);
        applyDamping(nodes, 0.88);
        const kinetic = updatePositions(nodes);
        if (kinetic < 0.01 && !dragNodeIdRef.current) {
          isSettledRef.current = true;
        }
      }

      render();
      animRef.current = requestAnimationFrame(tick);
    };

    animRef.current = requestAnimationFrame(tick);
    return () => {
      running = false;
      cancelAnimationFrame(animRef.current);
    };
  }, [dims.w, dims.h, render]);

  // Kick simulation when data changes
  useEffect(() => {
    isSettledRef.current = false;
  }, [graphKey]);

  // ---- Interaction: drag ----
  const screenToSvg = useCallback((clientX: number, clientY: number) => {
    const svg = svgRef.current;
    if (!svg) return { x: 0, y: 0 };
    const rect = svg.getBoundingClientRect();
    const vb = viewBoxRef.current;
    return {
      x: vb.x + ((clientX - rect.left) / rect.width) * vb.w,
      y: vb.y + ((clientY - rect.top) / rect.height) * vb.h,
    };
  }, []);

  const handlePointerDown = useCallback((e: React.PointerEvent) => {
    const svg = svgRef.current;
    if (!svg) return;

    const pt = screenToSvg(e.clientX, e.clientY);

    // Check if clicking on a node
    const nodes = simNodesRef.current;
    let hit: SimNode | null = null;
    // Check in reverse order (top-most first)
    for (let i = nodes.length - 1; i >= 0; i--) {
      const n = nodes[i];
      const dx = pt.x - n.x;
      const dy = pt.y - n.y;
      const hitRadius = n.type === 'machine' ? n.radius + 20 : n.radius + 6;
      if (dx * dx + dy * dy < hitRadius * hitRadius) {
        hit = n;
        break;
      }
    }

    if (hit && hit.type !== 'hub') {
      // Start dragging node
      setDragNodeId(hit.id);
      hit.fx = hit.x;
      hit.fy = hit.y;
      isSettledRef.current = false;
      svg.setPointerCapture(e.pointerId);
      e.preventDefault();

      // Select machine on click
      if (hit.type === 'machine') {
        const machineData = hit.data as MachineInfo;
        setSelectedMachineId(prev => prev === machineData.machineId ? null : machineData.machineId);
      } else if (hit.type === 'service') {
        const serviceData = hit.data as FleetNodeInfo;
        setSelectedMachineId(prev => prev === serviceData.machineId ? null : serviceData.machineId);
      }
    } else if (!hit) {
      // Start panning
      isPanningRef.current = true;
      panStartRef.current = { x: e.clientX, y: e.clientY, vbx: viewBoxRef.current.x, vby: viewBoxRef.current.y };
      svg.setPointerCapture(e.pointerId);
      e.preventDefault();
      setSelectedMachineId(null);
    }
  }, [screenToSvg]);

  const handlePointerMove = useCallback((e: React.PointerEvent) => {
    if (dragNodeIdRef.current) {
      const pt = screenToSvg(e.clientX, e.clientY);
      const node = nodeMapRef.current.get(dragNodeIdRef.current);
      if (node) {
        node.fx = pt.x;
        node.fy = pt.y;
        isSettledRef.current = false;
      }
      e.preventDefault();
    } else if (isPanningRef.current) {
      const svg = svgRef.current;
      if (!svg) return;
      const rect = svg.getBoundingClientRect();
      const vb = viewBoxRef.current;
      const scaleX = vb.w / rect.width;
      const scaleY = vb.h / rect.height;
      const dx = (e.clientX - panStartRef.current.x) * scaleX;
      const dy = (e.clientY - panStartRef.current.y) * scaleY;
      setViewBox(prev => ({
        ...prev,
        x: panStartRef.current.vbx - dx,
        y: panStartRef.current.vby - dy,
      }));
      e.preventDefault();
    } else {
      // Hover tooltip
      const pt = screenToSvg(e.clientX, e.clientY);
      const nodes = simNodesRef.current;
      let hit: SimNode | null = null;
      for (let i = nodes.length - 1; i >= 0; i--) {
        const n = nodes[i];
        const dx = pt.x - n.x;
        const dy = pt.y - n.y;
        const hitR = n.type === 'machine' ? n.radius + 20 : n.radius + 6;
        if (dx * dx + dy * dy < hitR * hitR) {
          hit = n;
          break;
        }
      }
      if (hit) {
        setTooltip({ x: e.clientX, y: e.clientY, node: hit });
      } else {
        setTooltip(null);
      }
    }
  }, [screenToSvg]);

  const handlePointerUp = useCallback((e: React.PointerEvent) => {
    if (dragNodeIdRef.current) {
      const node = nodeMapRef.current.get(dragNodeIdRef.current);
      if (node) {
        node.fx = null;
        node.fy = null;
      }
      setDragNodeId(null);
      isSettledRef.current = false;
    }
    if (isPanningRef.current) {
      isPanningRef.current = false;
    }
    const svg = svgRef.current;
    if (svg) svg.releasePointerCapture(e.pointerId);
  }, []);

  // Zoom
  const handleWheel = useCallback((e: React.WheelEvent) => {
    e.preventDefault();
    const svg = svgRef.current;
    if (!svg) return;

    const rect = svg.getBoundingClientRect();
    const vb = viewBoxRef.current;

    // Mouse position in SVG coordinates
    const mx = vb.x + ((e.clientX - rect.left) / rect.width) * vb.w;
    const my = vb.y + ((e.clientY - rect.top) / rect.height) * vb.h;

    const factor = e.deltaY > 0 ? 1.08 : 0.93;
    const newW = Math.max(400, Math.min(6000, vb.w * factor));
    const newH = Math.max(300, Math.min(4500, vb.h * factor));

    // Zoom toward mouse position
    const newX = mx - ((mx - vb.x) / vb.w) * newW;
    const newY = my - ((my - vb.y) / vb.h) * newH;

    setViewBox({ x: newX, y: newY, w: newW, h: newH });
  }, []);

  // Empty state
  const isEmpty = machines.length === 0 && fleetNodes.length === 0;

  return (
    <div className="mesh-view" ref={containerRef}>
      {isEmpty ? (
        <div className="mesh-empty">
          <div className="mesh-empty-inner">
            <div className="mesh-empty-pulse" />
            <div className="mesh-empty-icon">
              <svg width="48" height="48" viewBox="0 0 48 48" fill="none">
                <circle cx="24" cy="24" r="8" stroke="url(#emptyGrad)" strokeWidth="2" />
                <circle cx="10" cy="14" r="3" stroke="var(--text-muted)" strokeWidth="1.5" />
                <circle cx="38" cy="14" r="3" stroke="var(--text-muted)" strokeWidth="1.5" />
                <circle cx="10" cy="34" r="3" stroke="var(--text-muted)" strokeWidth="1.5" />
                <circle cx="38" cy="34" r="3" stroke="var(--text-muted)" strokeWidth="1.5" />
                <line x1="17" y1="19" x2="13" y2="16" stroke="var(--text-muted)" strokeWidth="1" strokeDasharray="2 2" />
                <line x1="31" y1="19" x2="35" y2="16" stroke="var(--text-muted)" strokeWidth="1" strokeDasharray="2 2" />
                <line x1="17" y1="29" x2="13" y2="32" stroke="var(--text-muted)" strokeWidth="1" strokeDasharray="2 2" />
                <line x1="31" y1="29" x2="35" y2="32" stroke="var(--text-muted)" strokeWidth="1" strokeDasharray="2 2" />
                <defs>
                  <linearGradient id="emptyGrad" x1="16" y1="16" x2="32" y2="32">
                    <stop offset="0%" stopColor="#3d5afe" />
                    <stop offset="100%" stopColor="#00e5ff" />
                  </linearGradient>
                </defs>
              </svg>
            </div>
            <p className="mesh-empty-text">Waiting for machines to connect to the mesh...</p>
            <p className="mesh-empty-hint">Nodes will appear here as they join the Zenoh network</p>
          </div>
        </div>
      ) : (
        <>
          <svg
            ref={svgRef}
            className="mesh-svg"
            viewBox={`${viewBox.x} ${viewBox.y} ${viewBox.w} ${viewBox.h}`}
            onPointerDown={handlePointerDown}
            onPointerMove={handlePointerMove}
            onPointerUp={handlePointerUp}
            onWheel={handleWheel}
          >
            <defs>
              <radialGradient id="meshHubGradient" cx="40%" cy="40%">
                <stop offset="0%" stopColor="#5c7cff" />
                <stop offset="50%" stopColor="#3d5afe" />
                <stop offset="100%" stopColor="#00b8d4" />
              </radialGradient>
              <radialGradient id="meshHubGlow" cx="50%" cy="50%">
                <stop offset="0%" stopColor="rgba(61,90,254,0.4)" />
                <stop offset="100%" stopColor="rgba(61,90,254,0)" />
              </radialGradient>
              <filter id="meshGlow" x="-50%" y="-50%" width="200%" height="200%">
                <feGaussianBlur stdDeviation="6" result="blur" />
                <feMerge>
                  <feMergeNode in="blur" />
                  <feMergeNode in="SourceGraphic" />
                </feMerge>
              </filter>
              <filter id="meshGlowSmall" x="-50%" y="-50%" width="200%" height="200%">
                <feGaussianBlur stdDeviation="3" result="blur" />
                <feMerge>
                  <feMergeNode in="blur" />
                  <feMergeNode in="SourceGraphic" />
                </feMerge>
              </filter>
              <linearGradient id="meshAccentGrad" x1="0%" y1="0%" x2="100%" y2="100%">
                <stop offset="0%" stopColor="#3d5afe" />
                <stop offset="100%" stopColor="#00e5ff" />
              </linearGradient>
            </defs>

            {/* Background grid pattern */}
            <defs>
              <pattern id="meshGrid" width="40" height="40" patternUnits="userSpaceOnUse">
                <path d="M 40 0 L 0 0 0 40" fill="none" stroke="rgba(42,42,58,0.3)" strokeWidth="0.5" />
              </pattern>
            </defs>
            <rect x={viewBox.x - 500} y={viewBox.y - 500} width={viewBox.w + 1000} height={viewBox.h + 1000} fill="url(#meshGrid)" />

            {/* Edges layer */}
            <g className="edges" />

            {/* Nodes layer */}
            <g className="nodes" />
          </svg>

          {/* Tooltip overlay */}
          {tooltip && <MeshTooltip tooltip={tooltip} />}
        </>
      )}

      <style>{`
        .mesh-view {
          flex: 1;
          position: relative;
          overflow: hidden;
          background:
            radial-gradient(ellipse at 30% 20%, rgba(61, 90, 254, 0.06) 0%, transparent 60%),
            radial-gradient(ellipse at 70% 80%, rgba(0, 229, 255, 0.04) 0%, transparent 60%),
            var(--bg-primary);
        }

        .mesh-svg {
          width: 100%;
          height: 100%;
          display: block;
          cursor: grab;
          user-select: none;
          touch-action: none;
        }

        .mesh-svg:active {
          cursor: grabbing;
        }

        .sim-edge {
          animation: meshEdgeFlow 1.2s linear infinite;
        }

        @keyframes meshEdgeFlow {
          from { stroke-dashoffset: 10; }
          to { stroke-dashoffset: 0; }
        }

        .sim-node {
          cursor: pointer;
        }

        .sim-node-hub {
          cursor: default;
        }

        /* Empty state */
        .mesh-empty {
          position: absolute;
          inset: 0;
          display: flex;
          align-items: center;
          justify-content: center;
        }

        .mesh-empty-inner {
          text-align: center;
          position: relative;
        }

        .mesh-empty-pulse {
          position: absolute;
          top: 50%;
          left: 50%;
          transform: translate(-50%, -50%);
          width: 120px;
          height: 120px;
          border-radius: 50%;
          background: radial-gradient(circle, rgba(61,90,254,0.08) 0%, transparent 70%);
          animation: meshEmptyPulse 3s ease-in-out infinite;
        }

        @keyframes meshEmptyPulse {
          0%, 100% { transform: translate(-50%, -50%) scale(1); opacity: 0.6; }
          50% { transform: translate(-50%, -50%) scale(1.4); opacity: 0.2; }
        }

        .mesh-empty-icon {
          position: relative;
          margin-bottom: 20px;
          display: inline-block;
        }

        .mesh-empty-text {
          font-size: 15px;
          color: var(--text-secondary);
          margin: 0 0 6px;
        }

        .mesh-empty-hint {
          font-size: 12px;
          color: var(--text-muted);
          margin: 0;
        }

        /* Tooltip */
        .mesh-tooltip {
          position: fixed;
          pointer-events: none;
          z-index: 200;
          background: var(--bg-card);
          border: 1px solid var(--border-color);
          border-radius: 10px;
          padding: 12px 16px;
          box-shadow: 0 8px 32px rgba(0,0,0,0.5);
          min-width: 160px;
          max-width: 280px;
        }

        .mesh-tooltip-title {
          font-size: 13px;
          font-weight: 600;
          color: var(--text-primary);
          margin-bottom: 6px;
          display: flex;
          align-items: center;
          gap: 6px;
        }

        .mesh-tooltip-dot {
          width: 6px;
          height: 6px;
          border-radius: 50%;
          flex-shrink: 0;
        }

        .mesh-tooltip-row {
          display: flex;
          justify-content: space-between;
          gap: 12px;
          font-size: 11px;
          line-height: 1.6;
        }

        .mesh-tooltip-label {
          color: var(--text-muted);
          text-transform: uppercase;
          letter-spacing: 0.3px;
          font-weight: 600;
        }

        .mesh-tooltip-value {
          color: var(--text-secondary);
          font-family: 'JetBrains Mono', monospace;
          text-align: right;
        }

        /* Mobile */
        @media (max-width: 768px) {
          .mesh-tooltip {
            display: none;
          }
        }
      `}</style>
    </div>
  );
}

// ---------------------------------------------------------------------------
// SVG node builders (imperative for performance in the render loop)
// ---------------------------------------------------------------------------

function buildHubNode(g: SVGGElement) {
  const ns = 'http://www.w3.org/2000/svg';

  // Outer glow
  const glowCircle = document.createElementNS(ns, 'circle');
  glowCircle.setAttribute('r', String(HUB_RADIUS + 20));
  glowCircle.setAttribute('fill', 'url(#meshHubGlow)');
  g.appendChild(glowCircle);

  // Pulse ring
  const pulseRing = document.createElementNS(ns, 'circle');
  pulseRing.setAttribute('r', String(HUB_RADIUS + 4));
  pulseRing.setAttribute('fill', 'none');
  pulseRing.setAttribute('stroke', 'rgba(61,90,254,0.3)');
  pulseRing.setAttribute('stroke-width', '1');
  pulseRing.setAttribute('class', 'hub-pulse-ring');
  g.appendChild(pulseRing);

  // Main circle
  const circle = document.createElementNS(ns, 'circle');
  circle.setAttribute('r', String(HUB_RADIUS));
  circle.setAttribute('fill', 'url(#meshHubGradient)');
  circle.setAttribute('filter', 'url(#meshGlow)');
  g.appendChild(circle);

  // Inner highlight
  const inner = document.createElementNS(ns, 'circle');
  inner.setAttribute('r', String(HUB_RADIUS - 8));
  inner.setAttribute('fill', 'none');
  inner.setAttribute('stroke', 'rgba(255,255,255,0.15)');
  inner.setAttribute('stroke-width', '1');
  g.appendChild(inner);

  // Label
  const text = document.createElementNS(ns, 'text');
  text.setAttribute('text-anchor', 'middle');
  text.setAttribute('dy', '0.35em');
  text.setAttribute('fill', '#fff');
  text.setAttribute('font-size', '11');
  text.setAttribute('font-weight', '700');
  text.setAttribute('font-family', "'Outfit', sans-serif");
  text.setAttribute('letter-spacing', '0.5');
  text.textContent = 'ZENOH';
  g.appendChild(text);

  // Sub-label
  const sub = document.createElementNS(ns, 'text');
  sub.setAttribute('text-anchor', 'middle');
  sub.setAttribute('dy', '0');
  sub.setAttribute('y', '16');
  sub.setAttribute('fill', 'rgba(255,255,255,0.6)');
  sub.setAttribute('font-size', '8');
  sub.setAttribute('font-family', "'Outfit', sans-serif");
  sub.setAttribute('letter-spacing', '1');
  sub.textContent = 'MESH';
  g.appendChild(sub);

  // Add CSS animation for pulse ring
  const style = document.createElementNS(ns, 'style');
  style.textContent = `
    .hub-pulse-ring {
      animation: hubPulse 2.5s ease-in-out infinite;
      transform-origin: center;
    }
    @keyframes hubPulse {
      0%, 100% { r: ${HUB_RADIUS + 4}; opacity: 0.4; }
      50% { r: ${HUB_RADIUS + 14}; opacity: 0; }
    }
  `;
  g.appendChild(style);
}

function buildMachineNode(g: SVGGElement) {
  const ns = 'http://www.w3.org/2000/svg';
  const w = 130;
  const h = 56;

  // Glow backdrop
  const glow = document.createElementNS(ns, 'rect');
  glow.setAttribute('x', String(-w / 2 - 3));
  glow.setAttribute('y', String(-h / 2 - 3));
  glow.setAttribute('width', String(w + 6));
  glow.setAttribute('height', String(h + 6));
  glow.setAttribute('rx', '14');
  glow.setAttribute('fill', 'none');
  glow.setAttribute('stroke', 'rgba(0,200,83,0.2)');
  glow.setAttribute('stroke-width', '1');
  glow.setAttribute('class', 'machine-glow');
  glow.setAttribute('filter', 'url(#meshGlowSmall)');
  g.appendChild(glow);

  // Background rect
  const rect = document.createElementNS(ns, 'rect');
  rect.setAttribute('x', String(-w / 2));
  rect.setAttribute('y', String(-h / 2));
  rect.setAttribute('width', String(w));
  rect.setAttribute('height', String(h));
  rect.setAttribute('rx', '12');
  rect.setAttribute('fill', 'var(--bg-card)');
  rect.setAttribute('stroke', 'var(--border-color)');
  rect.setAttribute('stroke-width', '1');
  rect.setAttribute('class', 'machine-rect');
  g.appendChild(rect);

  // Status dot
  const dot = document.createElementNS(ns, 'circle');
  dot.setAttribute('cx', String(-w / 2 + 14));
  dot.setAttribute('cy', String(-h / 2 + 14));
  dot.setAttribute('r', '3');
  dot.setAttribute('fill', '#00c853');
  dot.setAttribute('class', 'machine-status-dot');
  g.appendChild(dot);

  // Hostname
  const hostname = document.createElementNS(ns, 'text');
  hostname.setAttribute('x', '0');
  hostname.setAttribute('y', String(-6));
  hostname.setAttribute('text-anchor', 'middle');
  hostname.setAttribute('fill', 'var(--text-primary)');
  hostname.setAttribute('font-size', '12');
  hostname.setAttribute('font-weight', '600');
  hostname.setAttribute('font-family', "'Outfit', sans-serif");
  hostname.setAttribute('class', 'machine-hostname');
  hostname.textContent = '';
  g.appendChild(hostname);

  // IP + node count
  const info = document.createElementNS(ns, 'text');
  info.setAttribute('x', '0');
  info.setAttribute('y', String(10));
  info.setAttribute('text-anchor', 'middle');
  info.setAttribute('fill', 'var(--text-muted)');
  info.setAttribute('font-size', '9');
  info.setAttribute('font-family', "'JetBrains Mono', monospace");
  info.setAttribute('class', 'machine-info');
  info.textContent = '';
  g.appendChild(info);
}

function updateMachineNode(g: SVGGElement, data: MachineInfo, isSelected: boolean) {
  const hostname = g.querySelector('.machine-hostname') as SVGTextElement | null;
  if (hostname) {
    const label = data.hostname.length > 14 ? data.hostname.slice(0, 13) + '...' : data.hostname;
    if (hostname.textContent !== label) hostname.textContent = label;
  }

  const info = g.querySelector('.machine-info') as SVGTextElement | null;
  if (info) {
    const ip = data.ips[0] || '';
    const label = `${ip}  ${data.runningCount}/${data.nodeCount}`;
    if (info.textContent !== label) info.textContent = label;
  }

  const dot = g.querySelector('.machine-status-dot') as SVGCircleElement | null;
  if (dot) {
    dot.setAttribute('fill', data.isOnline ? '#00c853' : '#ff1744');
  }

  const glowRect = g.querySelector('.machine-glow') as SVGRectElement | null;
  if (glowRect) {
    glowRect.setAttribute('stroke', data.isOnline ? 'rgba(0,200,83,0.25)' : 'rgba(255,23,68,0.25)');
  }

  const rect = g.querySelector('.machine-rect') as SVGRectElement | null;
  if (rect) {
    rect.setAttribute('stroke', isSelected ? 'var(--accent-primary)' : 'var(--border-color)');
    rect.setAttribute('stroke-width', isSelected ? '2' : '1');
  }
}

function buildServiceNode(g: SVGGElement) {
  const ns = 'http://www.w3.org/2000/svg';

  // Outer ring
  const ring = document.createElementNS(ns, 'circle');
  ring.setAttribute('r', String(SERVICE_RADIUS + 3));
  ring.setAttribute('fill', 'none');
  ring.setAttribute('stroke', 'rgba(0,200,83,0.2)');
  ring.setAttribute('stroke-width', '1');
  ring.setAttribute('class', 'service-ring');
  g.appendChild(ring);

  // Main circle
  const circle = document.createElementNS(ns, 'circle');
  circle.setAttribute('r', String(SERVICE_RADIUS));
  circle.setAttribute('fill', '#00c853');
  circle.setAttribute('class', 'service-circle');
  circle.setAttribute('filter', 'url(#meshGlowSmall)');
  g.appendChild(circle);

  // Label
  const text = document.createElementNS(ns, 'text');
  text.setAttribute('y', String(SERVICE_RADIUS + 14));
  text.setAttribute('text-anchor', 'middle');
  text.setAttribute('fill', 'var(--text-muted)');
  text.setAttribute('font-size', '9');
  text.setAttribute('font-family', "'Outfit', sans-serif");
  text.setAttribute('class', 'service-label');
  text.textContent = '';
  g.appendChild(text);
}

function updateServiceNode(g: SVGGElement, data: FleetNodeInfo, isSelected: boolean) {
  const color = STATUS_COLORS[data.status] || STATUS_COLORS.unknown;

  const circle = g.querySelector('.service-circle') as SVGCircleElement | null;
  if (circle) {
    circle.setAttribute('fill', color);
  }

  const ring = g.querySelector('.service-ring') as SVGCircleElement | null;
  if (ring) {
    const alpha = isSelected ? '0.5' : '0.2';
    ring.setAttribute('stroke', color.replace(')', `, ${alpha})`).replace('rgb', 'rgba').replace('#', ''));
    // For hex colors, convert to rgba
    ring.setAttribute('stroke', `${color}${isSelected ? '80' : '33'}`);
    ring.setAttribute('stroke-width', isSelected ? '2' : '1');
  }

  const label = g.querySelector('.service-label') as SVGTextElement | null;
  if (label) {
    const name = data.name.length > 16 ? data.name.slice(0, 15) + '..' : data.name;
    if (label.textContent !== name) label.textContent = name;
    label.setAttribute('fill', isSelected ? 'var(--text-secondary)' : 'var(--text-muted)');
  }
}

// ---------------------------------------------------------------------------
// Tooltip subcomponent
// ---------------------------------------------------------------------------

function MeshTooltip({ tooltip }: { tooltip: TooltipInfo }) {
  const { node, x, y } = tooltip;

  let content: React.ReactNode;

  if (node.type === 'hub') {
    content = (
      <>
        <div className="mesh-tooltip-title">Zenoh Mesh Hub</div>
        <div className="mesh-tooltip-row">
          <span className="mesh-tooltip-label">Role</span>
          <span className="mesh-tooltip-value">Router</span>
        </div>
      </>
    );
  } else if (node.type === 'machine') {
    const data = node.data as MachineInfo;
    content = (
      <>
        <div className="mesh-tooltip-title">
          <span className="mesh-tooltip-dot" style={{ backgroundColor: data.isOnline ? '#00c853' : '#ff1744' }} />
          {data.hostname}
        </div>
        <div className="mesh-tooltip-row">
          <span className="mesh-tooltip-label">Status</span>
          <span className="mesh-tooltip-value">{data.isOnline ? 'Online' : 'Offline'}</span>
        </div>
        {data.ips.length > 0 && (
          <div className="mesh-tooltip-row">
            <span className="mesh-tooltip-label">IP</span>
            <span className="mesh-tooltip-value">{data.ips[0]}</span>
          </div>
        )}
        <div className="mesh-tooltip-row">
          <span className="mesh-tooltip-label">Nodes</span>
          <span className="mesh-tooltip-value">{data.runningCount}/{data.nodeCount} running</span>
        </div>
      </>
    );
  } else {
    const data = node.data as FleetNodeInfo;
    const color = STATUS_COLORS[data.status] || STATUS_COLORS.unknown;
    content = (
      <>
        <div className="mesh-tooltip-title">
          <span className="mesh-tooltip-dot" style={{ backgroundColor: color }} />
          {data.name}
        </div>
        <div className="mesh-tooltip-row">
          <span className="mesh-tooltip-label">Status</span>
          <span className="mesh-tooltip-value">{data.status}</span>
        </div>
        <div className="mesh-tooltip-row">
          <span className="mesh-tooltip-label">Machine</span>
          <span className="mesh-tooltip-value">{data.hostname}</span>
        </div>
        <div className="mesh-tooltip-row">
          <span className="mesh-tooltip-label">Type</span>
          <span className="mesh-tooltip-value">{data.nodeType}</span>
        </div>
        {data.version && (
          <div className="mesh-tooltip-row">
            <span className="mesh-tooltip-label">Version</span>
            <span className="mesh-tooltip-value">{data.version}</span>
          </div>
        )}
      </>
    );
  }

  return (
    <div
      className="mesh-tooltip"
      style={{
        left: x + 16,
        top: y - 10,
      }}
    >
      {content}
    </div>
  );
}
