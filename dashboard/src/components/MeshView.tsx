import { useState, useEffect, useRef, useCallback, useMemo } from 'react';
import { useFleetContext, type MachineInfo, type FleetNodeInfo } from '../contexts/FleetContext';

// ---------------------------------------------------------------------------
// Types
// ---------------------------------------------------------------------------

interface SimNode {
  id: string;
  type: 'hub' | 'machine' | 'service' | 'client';
  x: number;
  y: number;
  vx: number;
  vy: number;
  fx: number | null; // fixed x (while dragging)
  fy: number | null;
  radius: number;
  data: HubData | MachineInfo | FleetNodeInfo | DashboardData;
}

interface HubData {
  label: string;
}

interface DashboardData {
  label: string;
  host: string;        // window.location.host
  endpoint: string;    // Zenoh WebSocket endpoint
  status: string;      // 'connected' | 'connecting' | 'disconnected' | 'error'
}

interface SimEdge {
  source: string;
  target: string;
  type: 'mesh' | 'service' | 'client';
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
const DASHBOARD_RADIUS = 22;

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
      const d = Math.max(distance(a, b), 1);
      const minDist = a.radius + b.radius + 60;
      if (d < minDist) {
        // Smooth quadratic falloff capped to prevent jitter on overlap
        const overlap = (minDist - d) / minDist; // 0..1
        const force = Math.min(strength * overlap * overlap, 1.5);
        const dx = ((a.x - b.x) / d) * force;
        const dy = ((a.y - b.y) / d) * force;
        if (a.fx === null) { a.vx += dx; a.vy += dy; }
        if (b.fx === null) { b.vx -= dx; b.vy -= dy; }
      }
    }
  }
}

// Very subtle ambient motion so the graph feels alive even when settled
function applyBreathing(nodes: SimNode[], time: number) {
  for (let i = 0; i < nodes.length; i++) {
    const n = nodes[i];
    if (n.fx !== null || n.type === 'hub') continue;
    const phase = i * 2.39996; // golden angle for unique phase
    const amp = n.type === 'service' ? 0.012 : 0.006;
    n.vx += Math.sin(time * 0.0004 + phase) * amp;
    n.vy += Math.cos(time * 0.0003 + phase * 1.3) * amp;
  }
}

function applySpringForce(_nodes: SimNode[], edges: SimEdge[], nodeMap: Map<string, SimNode>) {
  for (const edge of edges) {
    const source = nodeMap.get(edge.source);
    const target = nodeMap.get(edge.target);
    if (!source || !target) continue;

    const idealLen = edge.type === 'mesh' ? 200 : edge.type === 'client' ? 180 : 90;
    const stiffness = edge.type === 'mesh' ? 0.003 : edge.type === 'client' ? 0.003 : 0.006;

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
    // Clamp velocity for smooth motion
    const maxV = 3;
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

function buildGraph(machines: MachineInfo[], fleetNodes: FleetNodeInfo[], cx: number, cy: number, dashboardData: DashboardData): { nodes: SimNode[]; edges: SimEdge[] } {
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

  // Dashboard client node -- positioned opposite to machines
  const dashAngle = Math.PI * 0.75; // bottom-left
  const dashR = 200;
  simNodes.push({
    id: '__dashboard__',
    type: 'client',
    x: cx + Math.cos(dashAngle) * dashR,
    y: cy + Math.sin(dashAngle) * dashR,
    vx: 0, vy: 0,
    fx: null, fy: null,
    radius: DASHBOARD_RADIUS,
    data: dashboardData,
  });
  edges.push({ source: '__hub__', target: '__dashboard__', type: 'client' });

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

export function MeshView({ availableTopics = [], zenohEndpoint = '', connectionStatus = 'connected' }: { availableTopics?: string[]; zenohEndpoint?: string; connectionStatus?: string }) {
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

  // Selected machine (ref mirrors state for use in render loop without recreating it)
  const [selectedMachineId, setSelectedMachineId] = useState<string | null>(null);
  const selectedMachineIdRef = useRef<string | null>(null);
  selectedMachineIdRef.current = selectedMachineId;

  // Detail side panel
  const [detailNode, setDetailNode] = useState<SimNode | null>(null);
  const detailNodeRef = useRef<SimNode | null>(null);
  detailNodeRef.current = detailNode;

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
    const dashboardData: DashboardData = {
      label: 'Dashboard',
      host: window.location.host,
      endpoint: zenohEndpoint,
      status: connectionStatus,
    };
    const { nodes, edges } = buildGraph(machines, fleetNodes, cx, cy, dashboardData);

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
    setViewBox({ x: dims.w * 0.15, y: dims.h * 0.15, w: dims.w * 0.7, h: dims.h * 0.7 });
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
    // Update dashboard status
    const dashNode = map.get('__dashboard__');
    if (dashNode) {
      (dashNode.data as DashboardData).status = connectionStatus;
    }
  }, [machines, fleetNodes, connectionStatus]);

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
        const isClient = edge.type === 'client';
        const selId = selectedMachineIdRef.current;
        const isSelected = selId && (
          edge.target === `machine-${selId}` ||
          edge.source === `machine-${selId}` ||
          tgt.id.startsWith(`service-${selId}-`) ||
          src.id.startsWith(`service-${selId}-`)
        );
        const isDashSelected = edge.target === '__dashboard__' && detailNodeRef.current?.id === '__dashboard__';

        if (isClient) {
          line.setAttribute('stroke', isDashSelected ? 'rgba(156,39,176,0.7)' : 'rgba(156,39,176,0.3)');
          line.setAttribute('stroke-width', '1.5');
          line.setAttribute('stroke-dasharray', '4 3');
        } else {
          line.setAttribute('stroke', isMesh
            ? (isSelected ? 'rgba(61,90,254,0.7)' : 'rgba(61,90,254,0.25)')
            : (isSelected ? 'rgba(0,229,255,0.6)' : 'rgba(0,229,255,0.15)'));
          line.setAttribute('stroke-width', isMesh ? '2' : '1');
          line.setAttribute('stroke-dasharray', isMesh ? '6 4' : '3 3');
        }
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
          } else if (node.type === 'client') {
            buildDashboardNode(g);
          } else {
            buildServiceNode(g);
          }
        }

        // Update transform
        g.setAttribute('transform', `translate(${node.x}, ${node.y})`);

        // Update data-dependent attributes
        if (node.type === 'machine') {
          updateMachineNode(g, node.data as MachineInfo, selectedMachineIdRef.current === (node.data as MachineInfo).machineId);
        } else if (node.type === 'service') {
          updateServiceNode(g, node.data as FleetNodeInfo,
            selectedMachineIdRef.current === (node.data as FleetNodeInfo).machineId);
        } else if (node.type === 'client') {
          updateDashboardNode(g, node.data as DashboardData);
        }

        existingMap.delete(node.id);
      }

      // Remove stale
      for (const stale of existingMap.values()) {
        nodeGroup.removeChild(stale);
      }
    }
  }, []);

  // Animation loop
  useEffect(() => {
    let running = true;
    let frameCount = 0;
    const cx = dims.w / 2;
    const cy = dims.h / 2;

    const tick = (time: number) => {
      if (!running) return;

      frameCount++;
      const nodes = simNodesRef.current;
      const edges = simEdgesRef.current;

      if (nodes.length > 0) {
        if (!isSettledRef.current) {
          // Full physics while unsettled
          applyGravity(nodes, cx, cy, 0.0015);
          applyRepulsion(nodes, 2.0);
          applySpringForce(nodes, edges, nodeMapRef.current);
          applyDamping(nodes, 0.92);
          applyBreathing(nodes, time);
          const kinetic = updatePositions(nodes);
          if (kinetic < 0.01 && !dragNodeIdRef.current) {
            isSettledRef.current = true;
          }
          render();
        } else {
          // Ambient breathing - render every 3rd frame to save CPU
          applyBreathing(nodes, time);
          applyDamping(nodes, 0.85);
          updatePositions(nodes);
          if (frameCount % 3 === 0) render();
        }
      }

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

      // Select machine on click (dashboard/client does not set selectedMachineId)
      if (hit.type === 'machine') {
        const machineData = hit.data as MachineInfo;
        setSelectedMachineId(prev => prev === machineData.machineId ? null : machineData.machineId);
      } else if (hit.type === 'service') {
        const serviceData = hit.data as FleetNodeInfo;
        setSelectedMachineId(prev => prev === serviceData.machineId ? null : serviceData.machineId);
      }

      // Open detail side panel
      setDetailNode(prev => prev?.id === hit.id ? null : hit);
    } else if (!hit) {
      // Start panning
      isPanningRef.current = true;
      panStartRef.current = { x: e.clientX, y: e.clientY, vbx: viewBoxRef.current.x, vby: viewBoxRef.current.y };
      svg.setPointerCapture(e.pointerId);
      e.preventDefault();
      setSelectedMachineId(null);
      setDetailNode(null);
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
  const handleWheel = useCallback((e: WheelEvent) => {
    e.preventDefault();
    const svg = svgRef.current;
    if (!svg) return;

    const rect = svg.getBoundingClientRect();
    const vb = viewBoxRef.current;

    const mx = vb.x + ((e.clientX - rect.left) / rect.width) * vb.w;
    const my = vb.y + ((e.clientY - rect.top) / rect.height) * vb.h;

    const factor = e.deltaY > 0 ? 1.08 : 0.93;
    const newW = Math.max(400, Math.min(6000, vb.w * factor));
    const newH = Math.max(300, Math.min(4500, vb.h * factor));

    const newX = mx - ((mx - vb.x) / vb.w) * newW;
    const newY = my - ((my - vb.y) / vb.h) * newH;

    setViewBox({ x: newX, y: newY, w: newW, h: newH });
  }, []);

  useEffect(() => {
    const svg = svgRef.current;
    if (!svg) return;
    svg.addEventListener('wheel', handleWheel, { passive: false });
    return () => svg.removeEventListener('wheel', handleWheel);
  }, [handleWheel]);

  // Zoom control functions
  const zoomIn = useCallback(() => {
    setViewBox(prev => {
      const factor = 0.8;
      const newW = Math.max(400, prev.w * factor);
      const newH = Math.max(300, prev.h * factor);
      const cx = prev.x + prev.w / 2;
      const cy = prev.y + prev.h / 2;
      return { x: cx - newW / 2, y: cy - newH / 2, w: newW, h: newH };
    });
  }, []);

  const zoomOut = useCallback(() => {
    setViewBox(prev => {
      const factor = 1.25;
      const newW = Math.min(6000, prev.w * factor);
      const newH = Math.min(4500, prev.h * factor);
      const cx = prev.x + prev.w / 2;
      const cy = prev.y + prev.h / 2;
      return { x: cx - newW / 2, y: cy - newH / 2, w: newW, h: newH };
    });
  }, []);

  const fitToView = useCallback(() => {
    setViewBox({ x: dims.w * 0.15, y: dims.h * 0.15, w: dims.w * 0.7, h: dims.h * 0.7 });
  }, [dims.w, dims.h]);

  return (
    <div className="mesh-view" ref={containerRef}>
          <svg
            ref={svgRef}
            className="mesh-svg"
            viewBox={`${viewBox.x} ${viewBox.y} ${viewBox.w} ${viewBox.h}`}
            onPointerDown={handlePointerDown}
            onPointerMove={handlePointerMove}
            onPointerUp={handlePointerUp}
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
              <radialGradient id="meshDashGradient" cx="40%" cy="40%">
                <stop offset="0%" stopColor="#ce93d8" />
                <stop offset="50%" stopColor="#9c27b0" />
                <stop offset="100%" stopColor="#7b1fa2" />
              </radialGradient>
              <radialGradient id="meshDashGlow" cx="50%" cy="50%">
                <stop offset="0%" stopColor="rgba(156,39,176,0.4)" />
                <stop offset="100%" stopColor="rgba(156,39,176,0)" />
              </radialGradient>
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

          {/* Zoom controls */}
          <div className="mesh-zoom-controls">
            <button className="mesh-zoom-btn" onClick={zoomIn} title="Zoom In" aria-label="Zoom In">
              <svg width="16" height="16" viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="2" strokeLinecap="round" strokeLinejoin="round">
                <circle cx="11" cy="11" r="7" />
                <line x1="21" y1="21" x2="16.65" y2="16.65" />
                <line x1="11" y1="8" x2="11" y2="14" />
                <line x1="8" y1="11" x2="14" y2="11" />
              </svg>
            </button>
            <button className="mesh-zoom-btn" onClick={zoomOut} title="Zoom Out" aria-label="Zoom Out">
              <svg width="16" height="16" viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="2" strokeLinecap="round" strokeLinejoin="round">
                <circle cx="11" cy="11" r="7" />
                <line x1="21" y1="21" x2="16.65" y2="16.65" />
                <line x1="8" y1="11" x2="14" y2="11" />
              </svg>
            </button>
            <button className="mesh-zoom-btn" onClick={fitToView} title="Fit to View" aria-label="Fit to View">
              <svg width="16" height="16" viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="2" strokeLinecap="round" strokeLinejoin="round">
                <path d="M15 3h6v6" />
                <path d="M9 21H3v-6" />
                <path d="M21 3l-7 7" />
                <path d="M3 21l7-7" />
              </svg>
            </button>
          </div>

          {/* Detail side panel */}
          <DetailSidePanel
            detailNode={detailNode}
            machines={machines}
            fleetNodes={fleetNodes}
            availableTopics={availableTopics}
            onClose={() => setDetailNode(null)}
          />

          {/* "You are here" badge for dashboard node */}
          {detailNode?.type === 'client' && (
            <div className="mesh-you-are-here">You are here</div>
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

        /* Zoom controls */
        .mesh-zoom-controls {
          position: absolute;
          bottom: 16px;
          right: 16px;
          display: flex;
          flex-direction: column;
          background: var(--bg-card);
          border: 1px solid var(--border-color);
          border-radius: 12px;
          overflow: hidden;
          box-shadow: 0 4px 16px rgba(0,0,0,0.3);
          z-index: 100;
        }

        .mesh-zoom-btn {
          display: flex;
          align-items: center;
          justify-content: center;
          width: 32px;
          height: 32px;
          border: none;
          background: transparent;
          color: var(--text-secondary);
          cursor: pointer;
          transition: background 0.15s, color 0.15s;
          padding: 0;
        }

        .mesh-zoom-btn:hover {
          background: var(--bg-tertiary);
          color: var(--text-primary);
        }

        .mesh-zoom-btn:active {
          background: rgba(61,90,254,0.15);
          color: var(--accent-primary);
        }

        .mesh-zoom-btn + .mesh-zoom-btn {
          border-top: 1px solid var(--border-color);
        }

        /* Detail side panel */
        .mesh-detail-panel {
          position: absolute;
          top: 0;
          right: 0;
          bottom: 0;
          width: 320px;
          background: var(--bg-card);
          border-left: 1px solid var(--border-color);
          z-index: 150;
          display: flex;
          flex-direction: column;
          overflow: hidden;
          transform: translateX(0);
          transition: transform 0.25s cubic-bezier(0.4, 0, 0.2, 1);
          box-shadow: -4px 0 24px rgba(0,0,0,0.3);
        }

        .mesh-detail-panel.mesh-detail-panel--hidden {
          transform: translateX(100%);
          pointer-events: none;
        }

        .mesh-detail-header {
          display: flex;
          align-items: center;
          justify-content: space-between;
          padding: 16px;
          border-bottom: 1px solid var(--border-color);
          flex-shrink: 0;
        }

        .mesh-detail-title {
          font-size: 15px;
          font-weight: 600;
          color: var(--text-primary);
          display: flex;
          align-items: center;
          gap: 8px;
          min-width: 0;
        }

        .mesh-detail-title span {
          overflow: hidden;
          text-overflow: ellipsis;
          white-space: nowrap;
        }

        .mesh-detail-close {
          display: flex;
          align-items: center;
          justify-content: center;
          width: 28px;
          height: 28px;
          border: none;
          background: transparent;
          color: var(--text-muted);
          cursor: pointer;
          border-radius: 6px;
          flex-shrink: 0;
          transition: background 0.15s, color 0.15s;
        }

        .mesh-detail-close:hover {
          background: var(--bg-tertiary);
          color: var(--text-primary);
        }

        .mesh-detail-body {
          flex: 1;
          overflow-y: auto;
          padding: 16px;
        }

        .mesh-detail-section {
          margin-bottom: 16px;
        }

        .mesh-detail-section-title {
          font-size: 10px;
          font-weight: 700;
          color: var(--text-muted);
          text-transform: uppercase;
          letter-spacing: 0.8px;
          margin-bottom: 8px;
        }

        .mesh-detail-row {
          display: flex;
          justify-content: space-between;
          align-items: center;
          gap: 12px;
          font-size: 12px;
          line-height: 1.8;
        }

        .mesh-detail-label {
          color: var(--text-muted);
          flex-shrink: 0;
        }

        .mesh-detail-value {
          color: var(--text-secondary);
          font-family: 'JetBrains Mono', monospace;
          font-size: 11px;
          text-align: right;
          overflow: hidden;
          text-overflow: ellipsis;
          white-space: nowrap;
        }

        .mesh-detail-status-dot {
          display: inline-block;
          width: 7px;
          height: 7px;
          border-radius: 50%;
          flex-shrink: 0;
        }

        .mesh-detail-node-list {
          list-style: none;
          margin: 0;
          padding: 0;
        }

        .mesh-detail-node-item {
          display: flex;
          align-items: center;
          gap: 8px;
          font-size: 12px;
          color: var(--text-secondary);
          padding: 4px 0;
        }

        .mesh-detail-topic {
          font-family: 'JetBrains Mono', monospace;
          font-size: 11px;
          color: var(--text-secondary);
          padding: 3px 0;
          overflow: hidden;
          text-overflow: ellipsis;
          white-space: nowrap;
        }

        .mesh-detail-empty {
          font-size: 11px;
          color: var(--text-muted);
          font-style: italic;
        }

        /* You are here badge */
        .mesh-you-are-here {
          position: absolute;
          bottom: 16px;
          left: 16px;
          background: rgba(156,39,176,0.15);
          border: 1px solid rgba(156,39,176,0.3);
          color: #ce93d8;
          font-size: 11px;
          font-weight: 600;
          padding: 4px 12px;
          border-radius: 20px;
          z-index: 100;
          letter-spacing: 0.3px;
        }

        /* Mobile */
        @media (max-width: 768px) {
          .mesh-tooltip {
            display: none;
          }

          .mesh-detail-panel {
            width: 100%;
            border-left: none;
            border-top: 1px solid var(--border-color);
            top: 40%;
            border-radius: 16px 16px 0 0;
          }

          .mesh-detail-panel.mesh-detail-panel--hidden {
            transform: translateY(100%);
          }

          .mesh-zoom-controls {
            bottom: 12px;
            right: 12px;
          }
        }

        @media (min-width: 769px) and (max-width: 1024px) {
          .mesh-detail-panel {
            width: 280px;
          }
        }
      `}</style>
    </div>
  );
}

// ---------------------------------------------------------------------------
// SVG node builders (imperative for performance in the render loop)
// ---------------------------------------------------------------------------

const SVG_NS = 'http://www.w3.org/2000/svg';

function svgEl<K extends keyof SVGElementTagNameMap>(
  parent: SVGElement,
  tag: K,
  attrs: Record<string, string>,
  text?: string,
): SVGElementTagNameMap[K] {
  const el = document.createElementNS(SVG_NS, tag);
  for (const [k, v] of Object.entries(attrs)) {
    el.setAttribute(k, v);
  }
  if (text !== undefined) el.textContent = text;
  parent.appendChild(el);
  return el;
}

function buildHubNode(g: SVGGElement) {
  svgEl(g, 'circle', { r: String(HUB_RADIUS + 20), fill: 'url(#meshHubGlow)' });
  svgEl(g, 'circle', { r: String(HUB_RADIUS + 4), fill: 'none', stroke: 'rgba(61,90,254,0.3)', 'stroke-width': '1', class: 'hub-pulse-ring' });
  svgEl(g, 'circle', { r: String(HUB_RADIUS), fill: 'url(#meshHubGradient)', filter: 'url(#meshGlow)' });
  svgEl(g, 'circle', { r: String(HUB_RADIUS - 8), fill: 'none', stroke: 'rgba(255,255,255,0.15)', 'stroke-width': '1' });
  svgEl(g, 'text', { 'text-anchor': 'middle', dy: '0.35em', fill: '#fff', 'font-size': '11', 'font-weight': '700', 'font-family': "'Outfit', sans-serif", 'letter-spacing': '0.5' }, 'ZENOH');
  svgEl(g, 'text', { 'text-anchor': 'middle', dy: '0', y: '16', fill: 'rgba(255,255,255,0.6)', 'font-size': '8', 'font-family': "'Outfit', sans-serif", 'letter-spacing': '1' }, 'MESH');
  svgEl(g, 'style', {}, `
    .hub-pulse-ring {
      animation: hubPulse 2.5s ease-in-out infinite;
      transform-origin: center;
    }
    @keyframes hubPulse {
      0%, 100% { r: ${HUB_RADIUS + 4}; opacity: 0.4; }
      50% { r: ${HUB_RADIUS + 14}; opacity: 0; }
    }
  `);
}

function buildMachineNode(g: SVGGElement) {
  const w = 130;
  const h = 56;

  svgEl(g, 'rect', { x: String(-w / 2 - 3), y: String(-h / 2 - 3), width: String(w + 6), height: String(h + 6), rx: '14', fill: 'none', stroke: 'rgba(0,200,83,0.2)', 'stroke-width': '1', class: 'machine-glow', filter: 'url(#meshGlowSmall)' });
  svgEl(g, 'rect', { x: String(-w / 2), y: String(-h / 2), width: String(w), height: String(h), rx: '12', fill: 'var(--bg-card)', stroke: 'var(--border-color)', 'stroke-width': '1', class: 'machine-rect' });
  svgEl(g, 'circle', { cx: String(-w / 2 + 14), cy: String(-h / 2 + 14), r: '3', fill: '#00c853', class: 'machine-status-dot' });
  svgEl(g, 'text', { x: '0', y: String(-6), 'text-anchor': 'middle', fill: 'var(--text-primary)', 'font-size': '12', 'font-weight': '600', 'font-family': "'Outfit', sans-serif", class: 'machine-hostname' }, '');
  svgEl(g, 'text', { x: '0', y: String(10), 'text-anchor': 'middle', fill: 'var(--text-muted)', 'font-size': '9', 'font-family': "'JetBrains Mono', monospace", class: 'machine-info' }, '');
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
  svgEl(g, 'circle', { r: String(SERVICE_RADIUS + 3), fill: 'none', stroke: 'rgba(0,200,83,0.2)', 'stroke-width': '1', class: 'service-ring' });
  svgEl(g, 'circle', { r: String(SERVICE_RADIUS), fill: '#00c853', class: 'service-circle', filter: 'url(#meshGlowSmall)' });
  svgEl(g, 'text', { y: String(SERVICE_RADIUS + 14), 'text-anchor': 'middle', fill: 'var(--text-muted)', 'font-size': '9', 'font-family': "'Outfit', sans-serif", class: 'service-label' }, '');
}

function updateServiceNode(g: SVGGElement, data: FleetNodeInfo, isSelected: boolean) {
  const color = STATUS_COLORS[data.status] || STATUS_COLORS.unknown;

  const circle = g.querySelector('.service-circle') as SVGCircleElement | null;
  if (circle) {
    circle.setAttribute('fill', color);
  }

  const ring = g.querySelector('.service-ring') as SVGCircleElement | null;
  if (ring) {
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

function buildDashboardNode(g: SVGGElement) {
  svgEl(g, 'circle', { r: String(DASHBOARD_RADIUS + 12), fill: 'url(#meshDashGlow)', class: 'dashboard-glow' });
  svgEl(g, 'circle', { r: String(DASHBOARD_RADIUS + 3), fill: 'none', stroke: 'rgba(156,39,176,0.3)', 'stroke-width': '1', class: 'dash-pulse-ring' });
  svgEl(g, 'circle', { r: String(DASHBOARD_RADIUS), fill: 'url(#meshDashGradient)', filter: 'url(#meshGlowSmall)', class: 'dashboard-circle' });
  svgEl(g, 'circle', { r: String(DASHBOARD_RADIUS), fill: 'none', stroke: 'rgba(156,39,176,0.4)', 'stroke-width': '1.5', class: 'dashboard-border' });

  // Monitor icon
  const icon = svgEl(g, 'g', { transform: 'translate(-8, -8) scale(0.7)' });
  svgEl(icon, 'rect', { x: '2', y: '1', width: '20', height: '14', rx: '2', fill: 'none', stroke: '#fff', 'stroke-width': '2' });
  svgEl(icon, 'line', { x1: '12', y1: '15', x2: '12', y2: '19', stroke: '#fff', 'stroke-width': '2' });
  svgEl(icon, 'line', { x1: '8', y1: '19', x2: '16', y2: '19', stroke: '#fff', 'stroke-width': '2', 'stroke-linecap': 'round' });

  svgEl(g, 'text', { y: String(DASHBOARD_RADIUS + 14), 'text-anchor': 'middle', fill: 'var(--text-secondary)', 'font-size': '10', 'font-weight': '600', 'font-family': "'Outfit', sans-serif", class: 'dashboard-label' }, 'Dashboard');
  svgEl(g, 'text', { y: String(DASHBOARD_RADIUS + 26), 'text-anchor': 'middle', fill: 'var(--text-muted)', 'font-size': '8', 'font-family': "'JetBrains Mono', monospace", class: 'dashboard-host' }, '');
  svgEl(g, 'style', {}, `
    .dash-pulse-ring {
      animation: dashPulse 3s ease-in-out infinite;
      transform-origin: center;
    }
    @keyframes dashPulse {
      0%, 100% { r: ${DASHBOARD_RADIUS + 3}; opacity: 0.4; }
      50% { r: ${DASHBOARD_RADIUS + 10}; opacity: 0; }
    }
  `);
}

const DASHBOARD_STATUS_COLORS: Record<string, string> = {
  connected: 'rgba(156,39,176,0.6)',
  connecting: 'rgba(255,214,0,0.5)',
};
const DASHBOARD_STATUS_DEFAULT = 'rgba(255,23,68,0.4)';

function updateDashboardNode(g: SVGGElement, data: DashboardData) {
  const host = g.querySelector('.dashboard-host') as SVGTextElement | null;
  if (host && host.textContent !== data.host) {
    host.textContent = data.host;
  }

  const border = g.querySelector('.dashboard-border') as SVGCircleElement | null;
  if (border) {
    border.setAttribute('stroke', DASHBOARD_STATUS_COLORS[data.status] || DASHBOARD_STATUS_DEFAULT);
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
  } else if (node.type === 'client') {
    const data = node.data as DashboardData;
    content = (
      <>
        <div className="mesh-tooltip-title">
          <span className="mesh-tooltip-dot" style={{ backgroundColor: data.status === 'connected' ? '#9c27b0' : '#ff1744' }} />
          Dashboard
        </div>
        <div className="mesh-tooltip-row">
          <span className="mesh-tooltip-label">Host</span>
          <span className="mesh-tooltip-value">{data.host}</span>
        </div>
        <div className="mesh-tooltip-row">
          <span className="mesh-tooltip-label">Endpoint</span>
          <span className="mesh-tooltip-value">{data.endpoint}</span>
        </div>
        <div className="mesh-tooltip-row">
          <span className="mesh-tooltip-label">Status</span>
          <span className="mesh-tooltip-value">{data.status}</span>
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

  // Clamp tooltip to viewport bounds
  const tooltipW = 280; // max-width from CSS
  const tooltipH = 120; // estimated height
  const left = Math.min(x + 16, window.innerWidth - tooltipW - 8);
  const top = Math.max(8, Math.min(y - 10, window.innerHeight - tooltipH - 8));

  return (
    <div
      className="mesh-tooltip"
      style={{ left, top }}
    >
      {content}
    </div>
  );
}

// ---------------------------------------------------------------------------
// Detail side panel subcomponent
// ---------------------------------------------------------------------------

// Inline helpers for DetailSidePanel to avoid repeated JSX patterns

function DetailRow({ label, value }: { label: string; value: React.ReactNode }) {
  return (
    <div className="mesh-detail-row">
      <span className="mesh-detail-label">{label}</span>
      <span className="mesh-detail-value">{value}</span>
    </div>
  );
}

function StatusRow({ label, color, text }: { label: string; color: string; text: string }) {
  return (
    <div className="mesh-detail-row">
      <span className="mesh-detail-label">{label}</span>
      <span className="mesh-detail-value" style={{ display: 'flex', alignItems: 'center', gap: 6 }}>
        <span className="mesh-detail-status-dot" style={{ backgroundColor: color }} />
        {text}
      </span>
    </div>
  );
}

function getDetailTitle(detailNode: SimNode | null): { title: string; dotColor: string } {
  if (!detailNode) return { title: '', dotColor: '' };

  switch (detailNode.type) {
    case 'machine': {
      const d = detailNode.data as MachineInfo;
      return { title: d.hostname, dotColor: d.isOnline ? '#00c853' : '#ff1744' };
    }
    case 'service': {
      const d = detailNode.data as FleetNodeInfo;
      return { title: d.name, dotColor: STATUS_COLORS[d.status] || STATUS_COLORS.unknown };
    }
    case 'client':
      return {
        title: 'Dashboard',
        dotColor: (detailNode.data as DashboardData).status === 'connected' ? '#9c27b0' : '#ff1744',
      };
    default:
      return { title: '', dotColor: '' };
  }
}

function DetailSidePanel({
  detailNode,
  machines,
  fleetNodes,
  availableTopics,
  onClose,
}: {
  detailNode: SimNode | null;
  machines: MachineInfo[];
  fleetNodes: FleetNodeInfo[];
  availableTopics: string[];
  onClose: () => void;
}) {
  const isOpen = detailNode !== null;

  let body: React.ReactNode = null;

  if (detailNode?.type === 'machine') {
    const data = detailNode.data as MachineInfo;
    const childNodes = fleetNodes.filter(n => n.machineId === data.machineId);
    body = (
      <>
        <div className="mesh-detail-section">
          <div className="mesh-detail-section-title">Status</div>
          <StatusRow label="State" color={data.isOnline ? '#00c853' : '#ff1744'} text={data.isOnline ? 'Online' : 'Offline'} />
          <DetailRow label="Machine ID" value={data.machineId} />
        </div>

        <div className="mesh-detail-section">
          <div className="mesh-detail-section-title">IP Addresses</div>
          {data.ips.length > 0 ? data.ips.map((ip, i) => (
            <div key={i} className="mesh-detail-topic">{ip}</div>
          )) : (
            <div className="mesh-detail-empty">No IPs reported</div>
          )}
        </div>

        <div className="mesh-detail-section">
          <div className="mesh-detail-section-title">Nodes ({data.runningCount}/{data.nodeCount} running)</div>
          {childNodes.length > 0 ? (
            <ul className="mesh-detail-node-list">
              {childNodes.map(cn => (
                <li key={cn.name} className="mesh-detail-node-item">
                  <span className="mesh-detail-status-dot" style={{ backgroundColor: STATUS_COLORS[cn.status] || STATUS_COLORS.unknown }} />
                  {cn.name}
                </li>
              ))}
            </ul>
          ) : (
            <div className="mesh-detail-empty">No service nodes</div>
          )}
        </div>
      </>
    );
  } else if (detailNode?.type === 'service') {
    const data = detailNode.data as FleetNodeInfo;
    const parentMachine = machines.find(m => m.machineId === data.machineId);
    const color = STATUS_COLORS[data.status] || STATUS_COLORS.unknown;
    body = (
      <>
        <div className="mesh-detail-section">
          <div className="mesh-detail-section-title">Status</div>
          <StatusRow label="State" color={color} text={data.status} />
          <DetailRow label="Type" value={data.nodeType || 'unknown'} />
          {data.version && <DetailRow label="Version" value={data.version} />}
        </div>

        <div className="mesh-detail-section">
          <div className="mesh-detail-section-title">Machine</div>
          <DetailRow label="Hostname" value={parentMachine?.hostname || data.hostname} />
          {(parentMachine?.ips || data.ips || []).map((ip, i) => (
            <DetailRow key={i} label={i === 0 ? 'IP' : ''} value={ip} />
          ))}
        </div>
      </>
    );
  } else if (detailNode?.type === 'client') {
    const data = detailNode.data as DashboardData;

    body = (
      <>
        <div className="mesh-detail-section">
          <div className="mesh-detail-section-title">Connection</div>
          <StatusRow label="Status" color={data.status === 'connected' ? '#9c27b0' : '#ff1744'} text={data.status} />
          <DetailRow label="Host" value={data.host} />
          <DetailRow label="Endpoint" value={data.endpoint} />
        </div>
        <div className="mesh-detail-section">
          <div className="mesh-detail-section-title">Discovery</div>
          <DetailRow label="Topics" value={`${availableTopics.length} discovered`} />
          <DetailRow label="Machines" value={`${machines.length} connected`} />
        </div>
        <div className="mesh-detail-section">
          <div className="mesh-detail-section-title">Browser</div>
          <DetailRow label="Protocol" value={window.location.protocol === 'https:' ? 'HTTPS (WSS)' : 'HTTP (WS)'} />
        </div>
      </>
    );
  }

  const { title, dotColor } = getDetailTitle(detailNode);

  return (
    <div className={`mesh-detail-panel ${isOpen ? '' : 'mesh-detail-panel--hidden'}`}>
      <div className="mesh-detail-header">
        <div className="mesh-detail-title">
          {dotColor && <span className="mesh-detail-status-dot" style={{ backgroundColor: dotColor }} />}
          <span>{title}</span>
        </div>
        <button className="mesh-detail-close" onClick={onClose} aria-label="Close detail panel">
          <svg width="14" height="14" viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="2" strokeLinecap="round" strokeLinejoin="round">
            <line x1="18" y1="6" x2="6" y2="18" />
            <line x1="6" y1="6" x2="18" y2="18" />
          </svg>
        </button>
      </div>
      <div className="mesh-detail-body">
        {body}
      </div>
    </div>
  );
}
