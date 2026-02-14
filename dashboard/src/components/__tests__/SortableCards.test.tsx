import { vi, describe, it, expect, beforeEach } from 'vitest';
import React from 'react';
import { render } from '@testing-library/react';

// Use vi.hoisted so the mock fn is available when vi.mock factories run (they are hoisted)
const { mockUseSortable } = vi.hoisted(() => ({
  mockUseSortable: vi.fn(() => ({
    attributes: { 'data-testid': 'sortable' } as Record<string, string>,
    listeners: {},
    setNodeRef: vi.fn(),
    transform: null,
    transition: null,
    isDragging: false,
  })),
}));

vi.mock('@dnd-kit/sortable', () => ({
  useSortable: mockUseSortable,
}));

vi.mock('@dnd-kit/utilities', () => ({
  CSS: {
    Transform: {
      toString: (transform: unknown) => (transform ? 'translate(10px, 20px)' : undefined),
    },
  },
}));

// Mock ALL inner panel components to avoid their hook dependencies
vi.mock('../CameraView', () => ({
  CameraView: (props: Record<string, unknown>) =>
    React.createElement('div', { 'data-testid': 'camera-view', 'data-topic': props.topic }),
}));

vi.mock('../JsonView', () => ({
  JsonViewPanel: (props: Record<string, unknown>) =>
    React.createElement('div', { 'data-testid': 'json-view-panel', 'data-topic': props.topic }),
  RawDataViewPanel: (props: Record<string, unknown>) =>
    React.createElement('div', { 'data-testid': 'raw-data-view-panel', 'data-topic': props.topic }),
}));

vi.mock('../WeatherView', () => ({
  WeatherViewPanel: (props: Record<string, unknown>) =>
    React.createElement('div', { 'data-testid': 'weather-view-panel', 'data-on-remove': props.onRemove ? 'yes' : 'no' }),
}));

vi.mock('../StatsView', () => ({
  StatsViewPanel: (props: Record<string, unknown>) =>
    React.createElement('div', { 'data-testid': 'stats-view-panel', 'data-on-remove': props.onRemove ? 'yes' : 'no' }),
}));

vi.mock('../NodesView', () => ({
  NodesViewPanel: (props: Record<string, unknown>) =>
    React.createElement('div', { 'data-testid': 'nodes-view-panel', 'data-on-remove': props.onRemove ? 'yes' : 'no' }),
}));

vi.mock('../SystemTelemetryView', () => ({
  SystemTelemetryViewPanel: (props: Record<string, unknown>) =>
    React.createElement('div', { 'data-testid': 'system-telemetry-view-panel', 'data-on-remove': props.onRemove ? 'yes' : 'no' }),
}));

vi.mock('../NetworkMonitorView', () => ({
  NetworkMonitorViewPanel: (props: Record<string, unknown>) =>
    React.createElement('div', { 'data-testid': 'network-monitor-view-panel', 'data-on-remove': props.onRemove ? 'yes' : 'no' }),
}));

import { SortableCameraCard } from '../SortableCameraCard';
import { SortableJsonCard } from '../SortableJsonCard';
import { SortableWeatherCard } from '../SortableWeatherCard';
import { SortableStatsCard } from '../SortableStatsCard';
import { SortableNodesCard } from '../SortableNodesCard';
import { SortableSystemTelemetryCard } from '../SortableSystemTelemetryCard';
import { SortableNetworkMonitorCard } from '../SortableNetworkMonitorCard';

describe('SortableCameraCard', () => {
  beforeEach(() => {
    vi.clearAllMocks();
    mockUseSortable.mockReturnValue({
      attributes: { 'data-testid': 'sortable' } as Record<string, string>,
      listeners: {},
      setNodeRef: vi.fn(),
      transform: null,
      transition: null,
      isDragging: false,
    });
  });

  it('renders without crashing', () => {
    const { container } = render(
      <SortableCameraCard
        id="cam-1"
        cameraName="entrance"
        topic="test/topic"
        isMaximized={false}
        onMaximize={vi.fn()}
        onTopicChange={vi.fn()}
        onRemove={vi.fn()}
        availableTopics={[]}
      />
    );
    expect(container.querySelector('[data-testid="camera-view"]')).toBeInTheDocument();
  });

  it('passes props to inner component', () => {
    const { container } = render(
      <SortableCameraCard
        id="cam-1"
        cameraName="entrance"
        topic="my/camera/topic"
        isMaximized={false}
        onMaximize={vi.fn()}
        onTopicChange={vi.fn()}
        onRemove={vi.fn()}
        availableTopics={[]}
      />
    );
    const inner = container.querySelector('[data-testid="camera-view"]');
    expect(inner).toHaveAttribute('data-topic', 'my/camera/topic');
  });

  it('applies display:none when isHidden=true', () => {
    const { container } = render(
      <SortableCameraCard
        id="cam-1"
        cameraName="entrance"
        topic="test/topic"
        isMaximized={false}
        isHidden={true}
        onMaximize={vi.fn()}
        onTopicChange={vi.fn()}
        onRemove={vi.fn()}
        availableTopics={[]}
      />
    );
    const wrapper = container.firstElementChild as HTMLElement;
    expect(wrapper.style.display).toBe('none');
  });

  it('sets opacity:0.5 when isDragging', () => {
    mockUseSortable.mockReturnValue({
      attributes: {} as Record<string, string>,
      listeners: {},
      setNodeRef: vi.fn(),
      transform: null,
      transition: null,
      isDragging: true,
    });
    const { container } = render(
      <SortableCameraCard
        id="cam-1"
        cameraName="entrance"
        topic="test/topic"
        isMaximized={false}
        onMaximize={vi.fn()}
        onTopicChange={vi.fn()}
        onRemove={vi.fn()}
        availableTopics={[]}
      />
    );
    const wrapper = container.firstElementChild as HTMLElement;
    expect(wrapper.style.opacity).toBe('0.5');
  });
});

describe('SortableJsonCard', () => {
  beforeEach(() => {
    vi.clearAllMocks();
    mockUseSortable.mockReturnValue({
      attributes: {} as Record<string, string>,
      listeners: {},
      setNodeRef: vi.fn(),
      transform: null,
      transition: null,
      isDragging: false,
    });
  });

  it('renders without crashing', () => {
    const { container } = render(
      <SortableJsonCard
        id="json-1"
        panelName="JSON Panel"
        topic="test/json"
        onTopicChange={vi.fn()}
        onRemove={vi.fn()}
        availableTopics={[]}
      />
    );
    expect(container.querySelector('[data-testid="json-view-panel"]')).toBeInTheDocument();
  });

  it('passes topic to inner component', () => {
    const { container } = render(
      <SortableJsonCard
        id="json-1"
        panelName="JSON Panel"
        topic="my/json/topic"
        onTopicChange={vi.fn()}
        onRemove={vi.fn()}
        availableTopics={[]}
      />
    );
    const inner = container.querySelector('[data-testid="json-view-panel"]');
    expect(inner).toHaveAttribute('data-topic', 'my/json/topic');
  });

  it('applies display:none when isHidden=true', () => {
    const { container } = render(
      <SortableJsonCard
        id="json-1"
        panelName="JSON Panel"
        topic="test/json"
        isHidden={true}
        onTopicChange={vi.fn()}
        onRemove={vi.fn()}
        availableTopics={[]}
      />
    );
    const wrapper = container.firstElementChild as HTMLElement;
    expect(wrapper.style.display).toBe('none');
  });

  it('sets opacity:0.5 when isDragging', () => {
    mockUseSortable.mockReturnValue({
      attributes: {} as Record<string, string>,
      listeners: {},
      setNodeRef: vi.fn(),
      transform: null,
      transition: null,
      isDragging: true,
    });
    const { container } = render(
      <SortableJsonCard
        id="json-1"
        panelName="JSON Panel"
        topic="test/json"
        onTopicChange={vi.fn()}
        onRemove={vi.fn()}
        availableTopics={[]}
      />
    );
    const wrapper = container.firstElementChild as HTMLElement;
    expect(wrapper.style.opacity).toBe('0.5');
  });
});

describe('SortableWeatherCard', () => {
  beforeEach(() => {
    vi.clearAllMocks();
    mockUseSortable.mockReturnValue({
      attributes: {} as Record<string, string>,
      listeners: {},
      setNodeRef: vi.fn(),
      transform: null,
      transition: null,
      isDragging: false,
    });
  });

  it('renders without crashing', () => {
    const { container } = render(
      <SortableWeatherCard
        id="weather-1"
        panelName="Weather"
        topic="weather/current"
        onRemove={vi.fn()}
      />
    );
    expect(container.querySelector('[data-testid="weather-view-panel"]')).toBeInTheDocument();
  });

  it('passes onRemove to inner component', () => {
    const { container } = render(
      <SortableWeatherCard
        id="weather-1"
        panelName="Weather"
        topic="weather/current"
        onRemove={vi.fn()}
      />
    );
    const inner = container.querySelector('[data-testid="weather-view-panel"]');
    expect(inner).toHaveAttribute('data-on-remove', 'yes');
  });

  it('applies display:none when isHidden=true', () => {
    const { container } = render(
      <SortableWeatherCard
        id="weather-1"
        panelName="Weather"
        topic="weather/current"
        isHidden={true}
        onRemove={vi.fn()}
      />
    );
    const wrapper = container.firstElementChild as HTMLElement;
    expect(wrapper.style.display).toBe('none');
  });

  it('sets opacity:0.5 when isDragging', () => {
    mockUseSortable.mockReturnValue({
      attributes: {} as Record<string, string>,
      listeners: {},
      setNodeRef: vi.fn(),
      transform: null,
      transition: null,
      isDragging: true,
    });
    const { container } = render(
      <SortableWeatherCard
        id="weather-1"
        panelName="Weather"
        topic="weather/current"
        onRemove={vi.fn()}
      />
    );
    const wrapper = container.firstElementChild as HTMLElement;
    expect(wrapper.style.opacity).toBe('0.5');
  });
});

describe('SortableStatsCard', () => {
  beforeEach(() => {
    vi.clearAllMocks();
    mockUseSortable.mockReturnValue({
      attributes: {} as Record<string, string>,
      listeners: {},
      setNodeRef: vi.fn(),
      transform: null,
      transition: null,
      isDragging: false,
    });
  });

  it('renders without crashing', () => {
    const { container } = render(
      <SortableStatsCard
        id="stats-1"
        panelName="Stats"
        onRemove={vi.fn()}
      />
    );
    expect(container.querySelector('[data-testid="stats-view-panel"]')).toBeInTheDocument();
  });

  it('passes onRemove to inner component', () => {
    const { container } = render(
      <SortableStatsCard
        id="stats-1"
        panelName="Stats"
        onRemove={vi.fn()}
      />
    );
    const inner = container.querySelector('[data-testid="stats-view-panel"]');
    expect(inner).toHaveAttribute('data-on-remove', 'yes');
  });

  it('applies display:none when isHidden=true', () => {
    const { container } = render(
      <SortableStatsCard
        id="stats-1"
        panelName="Stats"
        isHidden={true}
        onRemove={vi.fn()}
      />
    );
    const wrapper = container.firstElementChild as HTMLElement;
    expect(wrapper.style.display).toBe('none');
  });

  it('sets opacity:0.5 when isDragging', () => {
    mockUseSortable.mockReturnValue({
      attributes: {} as Record<string, string>,
      listeners: {},
      setNodeRef: vi.fn(),
      transform: null,
      transition: null,
      isDragging: true,
    });
    const { container } = render(
      <SortableStatsCard
        id="stats-1"
        panelName="Stats"
        onRemove={vi.fn()}
      />
    );
    const wrapper = container.firstElementChild as HTMLElement;
    expect(wrapper.style.opacity).toBe('0.5');
  });
});

describe('SortableNodesCard', () => {
  beforeEach(() => {
    vi.clearAllMocks();
    mockUseSortable.mockReturnValue({
      attributes: {} as Record<string, string>,
      listeners: {},
      setNodeRef: vi.fn(),
      transform: null,
      transition: null,
      isDragging: false,
    });
  });

  it('renders without crashing', () => {
    const { container } = render(
      <SortableNodesCard
        id="nodes-1"
        panelName="Nodes"
        onRemove={vi.fn()}
      />
    );
    expect(container.querySelector('[data-testid="nodes-view-panel"]')).toBeInTheDocument();
  });

  it('passes onRemove to inner component', () => {
    const { container } = render(
      <SortableNodesCard
        id="nodes-1"
        panelName="Nodes"
        onRemove={vi.fn()}
      />
    );
    const inner = container.querySelector('[data-testid="nodes-view-panel"]');
    expect(inner).toHaveAttribute('data-on-remove', 'yes');
  });

  it('applies display:none when isHidden=true', () => {
    const { container } = render(
      <SortableNodesCard
        id="nodes-1"
        panelName="Nodes"
        isHidden={true}
        onRemove={vi.fn()}
      />
    );
    const wrapper = container.firstElementChild as HTMLElement;
    expect(wrapper.style.display).toBe('none');
  });

  it('sets opacity:0.5 when isDragging', () => {
    mockUseSortable.mockReturnValue({
      attributes: {} as Record<string, string>,
      listeners: {},
      setNodeRef: vi.fn(),
      transform: null,
      transition: null,
      isDragging: true,
    });
    const { container } = render(
      <SortableNodesCard
        id="nodes-1"
        panelName="Nodes"
        onRemove={vi.fn()}
      />
    );
    const wrapper = container.firstElementChild as HTMLElement;
    expect(wrapper.style.opacity).toBe('0.5');
  });
});

describe('SortableSystemTelemetryCard', () => {
  beforeEach(() => {
    vi.clearAllMocks();
    mockUseSortable.mockReturnValue({
      attributes: {} as Record<string, string>,
      listeners: {},
      setNodeRef: vi.fn(),
      transform: null,
      transition: null,
      isDragging: false,
    });
  });

  it('renders without crashing', () => {
    const { container } = render(
      <SortableSystemTelemetryCard
        id="telemetry-1"
        panelName="System Telemetry"
        onRemove={vi.fn()}
      />
    );
    expect(container.querySelector('[data-testid="system-telemetry-view-panel"]')).toBeInTheDocument();
  });

  it('passes onRemove to inner component', () => {
    const { container } = render(
      <SortableSystemTelemetryCard
        id="telemetry-1"
        panelName="System Telemetry"
        onRemove={vi.fn()}
      />
    );
    const inner = container.querySelector('[data-testid="system-telemetry-view-panel"]');
    expect(inner).toHaveAttribute('data-on-remove', 'yes');
  });

  it('applies display:none when isHidden=true', () => {
    const { container } = render(
      <SortableSystemTelemetryCard
        id="telemetry-1"
        panelName="System Telemetry"
        isHidden={true}
        onRemove={vi.fn()}
      />
    );
    const wrapper = container.firstElementChild as HTMLElement;
    expect(wrapper.style.display).toBe('none');
  });

  it('sets opacity:0.5 when isDragging', () => {
    mockUseSortable.mockReturnValue({
      attributes: {} as Record<string, string>,
      listeners: {},
      setNodeRef: vi.fn(),
      transform: null,
      transition: null,
      isDragging: true,
    });
    const { container } = render(
      <SortableSystemTelemetryCard
        id="telemetry-1"
        panelName="System Telemetry"
        onRemove={vi.fn()}
      />
    );
    const wrapper = container.firstElementChild as HTMLElement;
    expect(wrapper.style.opacity).toBe('0.5');
  });
});

describe('SortableNetworkMonitorCard', () => {
  beforeEach(() => {
    vi.clearAllMocks();
    mockUseSortable.mockReturnValue({
      attributes: {} as Record<string, string>,
      listeners: {},
      setNodeRef: vi.fn(),
      transform: null,
      transition: null,
      isDragging: false,
    });
  });

  it('renders without crashing', () => {
    const { container } = render(
      <SortableNetworkMonitorCard
        id="network-1"
        panelName="Network Monitor"
        onRemove={vi.fn()}
      />
    );
    expect(container.querySelector('[data-testid="network-monitor-view-panel"]')).toBeInTheDocument();
  });

  it('passes onRemove to inner component', () => {
    const { container } = render(
      <SortableNetworkMonitorCard
        id="network-1"
        panelName="Network Monitor"
        onRemove={vi.fn()}
      />
    );
    const inner = container.querySelector('[data-testid="network-monitor-view-panel"]');
    expect(inner).toHaveAttribute('data-on-remove', 'yes');
  });

  it('applies display:none when isHidden=true', () => {
    const { container } = render(
      <SortableNetworkMonitorCard
        id="network-1"
        panelName="Network Monitor"
        isHidden={true}
        onRemove={vi.fn()}
      />
    );
    const wrapper = container.firstElementChild as HTMLElement;
    expect(wrapper.style.display).toBe('none');
  });

  it('sets opacity:0.5 when isDragging', () => {
    mockUseSortable.mockReturnValue({
      attributes: {} as Record<string, string>,
      listeners: {},
      setNodeRef: vi.fn(),
      transform: null,
      transition: null,
      isDragging: true,
    });
    const { container } = render(
      <SortableNetworkMonitorCard
        id="network-1"
        panelName="Network Monitor"
        onRemove={vi.fn()}
      />
    );
    const wrapper = container.firstElementChild as HTMLElement;
    expect(wrapper.style.opacity).toBe('0.5');
  });
});
