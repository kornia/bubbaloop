// Re-export compiled protobuf types and provide helper functions for daemon messages
import { bubbaloop } from './messages.pb.js';
import Long from 'long';

// Re-export the proto types
export const NodeStateProto = bubbaloop.daemon.v1.NodeState;
export const NodeListProto = bubbaloop.daemon.v1.NodeList;
export const NodeCommandProto = bubbaloop.daemon.v1.NodeCommand;
export const CommandResultProto = bubbaloop.daemon.v1.CommandResult;
export const NodeEventProto = bubbaloop.daemon.v1.NodeEvent;

// Status enum values
export const NodeStatus = bubbaloop.daemon.v1.NodeStatus;
export const CommandType = bubbaloop.daemon.v1.CommandType;

// TypeScript interfaces
export interface NodeState {
  name: string;
  path: string;
  status: number;
  statusName: string;
  installed: boolean;
  autostartEnabled: boolean;
  version: string;
  description: string;
  nodeType: string;
  isBuilt: boolean;
  lastUpdatedMs: bigint;
  buildOutput: string[];
  machineId: string;
  machineHostname: string;
  machineIps: string[];
  baseNode: string;
}

export interface NodeList {
  nodes: NodeState[];
  timestampMs: bigint;
  machineId: string;
}

export interface NodeEvent {
  eventType: string;
  nodeName: string;
  state?: NodeState;
  timestampMs: bigint;
}

// Convert protobufjs Long to BigInt
function toLongBigInt(value: Long | number | undefined | null): bigint {
  if (value === undefined || value === null) {
    return 0n;
  }
  if (typeof value === 'number') {
    return BigInt(value);
  }
  if (Long.isLong(value)) {
    return BigInt(value.toString());
  }
  return 0n;
}

// Map status enum to string
function statusToString(status: number): string {
  switch (status) {
    case 0: return 'unknown';
    case 1: return 'stopped';
    case 2: return 'running';
    case 3: return 'failed';
    case 4: return 'installing';
    case 5: return 'building';
    case 6: return 'not-installed';
    default: return 'unknown';
  }
}

// Decode NodeState from proto message
function decodeNodeState(msg: unknown): NodeState | null {
  if (!msg || typeof msg !== 'object') return null;
  const m = msg as Record<string, unknown>;
  return {
    name: (m.name as string) ?? '',
    path: (m.path as string) ?? '',
    status: (m.status as number) ?? 0,
    statusName: statusToString((m.status as number) ?? 0),
    installed: (m.installed as boolean) ?? false,
    autostartEnabled: (m.autostartEnabled as boolean) ?? false,
    version: (m.version as string) ?? '',
    description: (m.description as string) ?? '',
    nodeType: (m.nodeType as string) ?? '',
    isBuilt: (m.isBuilt as boolean) ?? false,
    lastUpdatedMs: toLongBigInt(m.lastUpdatedMs as Long | number),
    buildOutput: (m.buildOutput as string[]) ?? [],
    machineId: (m.machineId as string) ?? '',
    machineHostname: (m.machineHostname as string) ?? '',
    machineIps: (m.machineIps as string[]) ?? [],
    baseNode: (m.baseNode as string) ?? '',
  };
}

// Decode NodeList from Uint8Array
export function decodeNodeList(data: Uint8Array): NodeList | null {
  try {
    const message = NodeListProto.decode(data);
    const nodes: NodeState[] = [];
    for (const n of message.nodes ?? []) {
      const state = decodeNodeState(n);
      if (state) nodes.push(state);
    }
    return {
      nodes,
      timestampMs: toLongBigInt(message.timestampMs as Long | number),
      machineId: (message.machineId as string) ?? '',
    };
  } catch (error) {
    console.error('[Proto] Failed to decode NodeList:', error);
    return null;
  }
}

// Decode NodeEvent from Uint8Array
export function decodeNodeEvent(data: Uint8Array): NodeEvent | null {
  try {
    const message = NodeEventProto.decode(data);
    const state = message.state ? decodeNodeState(message.state) : undefined;
    return {
      eventType: message.eventType ?? '',
      nodeName: message.nodeName ?? '',
      state: state ?? undefined,
      timestampMs: toLongBigInt(message.timestampMs as Long | number),
    };
  } catch (error) {
    console.error('[Proto] Failed to decode NodeEvent:', error);
    return null;
  }
}
