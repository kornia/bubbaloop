import { vi, describe, it, expect, beforeEach } from 'vitest';
import React from 'react';
import { render, screen } from '@testing-library/react';

vi.mock('../../contexts/FleetContext', () => ({
  useFleetContext: vi.fn(() => ({
    machines: [],
    reportMachines: vi.fn(),
    nodes: [],
    reportNodes: vi.fn(),
    selectedMachineId: null,
    setSelectedMachineId: vi.fn(),
  })),
  FleetProvider: ({ children }: { children: React.ReactNode }) => children,
}));

import { MeshView } from '../MeshView';

describe('MeshView', () => {
  beforeEach(() => {
    vi.clearAllMocks();
  });

  it('renders SVG container', () => {
    const { container } = render(<MeshView />);
    const svg = container.querySelector('svg.mesh-svg');
    expect(svg).toBeInTheDocument();
  });

  it('renders mesh-view container div', () => {
    const { container } = render(<MeshView />);
    const meshViewDiv = container.querySelector('.mesh-view');
    expect(meshViewDiv).toBeInTheDocument();
  });

  it('shows zoom controls', () => {
    render(<MeshView />);
    expect(screen.getByTitle('Zoom In')).toBeInTheDocument();
    expect(screen.getByTitle('Zoom Out')).toBeInTheDocument();
    expect(screen.getByTitle('Fit to View')).toBeInTheDocument();
  });

  it('renders edges and nodes group layers in SVG', () => {
    const { container } = render(<MeshView />);
    const edgesGroup = container.querySelector('g.edges');
    const nodesGroup = container.querySelector('g.nodes');
    expect(edgesGroup).toBeInTheDocument();
    expect(nodesGroup).toBeInTheDocument();
  });

  it('renders with custom zenohEndpoint prop', () => {
    const { container } = render(
      <MeshView zenohEndpoint="ws://localhost:7447" connectionStatus="connected" />
    );
    const svg = container.querySelector('svg.mesh-svg');
    expect(svg).toBeInTheDocument();
  });
});
