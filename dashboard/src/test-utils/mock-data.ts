/**
 * Factory functions for creating mock test data.
 *
 * Provides reusable fixtures for MachineInfo, FleetNodeInfo,
 * and mock payloads for various protobuf message types.
 */

import type { MachineInfo, FleetNodeInfo } from '../contexts/FleetContext';

export function createMachineInfo(overrides: Partial<MachineInfo> = {}): MachineInfo {
  return {
    machineId: 'nvidia-orin00',
    hostname: 'nvidia-orin00',
    nodeCount: 5,
    runningCount: 3,
    isOnline: true,
    ips: ['192.168.1.100'],
    ...overrides,
  };
}

export function createFleetNodeInfo(overrides: Partial<FleetNodeInfo> = {}): FleetNodeInfo {
  return {
    name: 'test-camera',
    status: 'running',
    machineId: 'nvidia-orin00',
    hostname: 'nvidia-orin00',
    ips: ['192.168.1.100'],
    nodeType: 'camera',
    version: '1.0.0',
    baseNode: 'rtsp-camera',
    ...overrides,
  };
}

/** Creates a mock JSON payload as Uint8Array */
export function createJsonPayload(data: unknown): Uint8Array {
  return new TextEncoder().encode(JSON.stringify(data));
}

/** Creates a mock text payload as Uint8Array */
export function createTextPayload(text: string): Uint8Array {
  return new TextEncoder().encode(text);
}

/** Creates random binary payload */
export function createBinaryPayload(length: number): Uint8Array {
  const arr = new Uint8Array(length);
  for (let i = 0; i < length; i++) {
    arr[i] = Math.floor(Math.random() * 256);
  }
  return arr;
}

/** Available topic fixture for testing topic dropdowns */
export const MOCK_AVAILABLE_TOPICS = [
  { display: 'bubbaloop/local/m1/camera/entrance/compressed', raw: '0/bubbaloop%local%m1%camera%entrance%compressed/bubbaloop.camera.v1.CompressedImage/RIHS01_abc' },
  { display: 'bubbaloop/local/m1/camera/parking/compressed', raw: '0/bubbaloop%local%m1%camera%parking%compressed/bubbaloop.camera.v1.CompressedImage/RIHS01_def' },
  { display: 'bubbaloop/local/m1/weather/current', raw: '0/bubbaloop%local%m1%weather%current/bubbaloop.weather.v1.CurrentWeather/RIHS01_ghi' },
  { display: 'bubbaloop/local/m1/daemon/nodes', raw: 'bubbaloop/local/m1/daemon/nodes' },
  { display: 'bubbaloop/local/m1/daemon/events', raw: 'bubbaloop/local/m1/daemon/events' },
  { display: 'bubbaloop/local/m1/network-monitor/status', raw: '0/bubbaloop%local%m1%network-monitor%status/bubbaloop.network_monitor.v1.NetworkStatus/RIHS01_jkl' },
  { display: 'bubbaloop/local/m1/system-telemetry/metrics', raw: '0/bubbaloop%local%m1%system-telemetry%metrics/bubbaloop.system_telemetry.v1.SystemMetrics/RIHS01_mno' },
];

/** Default camera fixtures */
export const MOCK_CAMERAS = [
  { name: 'entrance', topic: '0/bubbaloop%local%m1%camera%entrance%compressed/**' },
  { name: 'parking', topic: '0/bubbaloop%local%m1%camera%parking%compressed/**' },
];

/** Two-machine fleet fixture */
export function createMultiMachineFleet(): MachineInfo[] {
  return [
    createMachineInfo({ machineId: 'nvidia-orin00', hostname: 'nvidia-orin00', ips: ['192.168.1.100'] }),
    createMachineInfo({ machineId: 'jetson-nano01', hostname: 'jetson-nano01', ips: ['192.168.1.101'], nodeCount: 3, runningCount: 2 }),
  ];
}
