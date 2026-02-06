// Re-export compiled protobuf types and provide helper functions
import { bubbaloop } from './messages.pb.js';
import Long from 'long';

// Re-export the proto types
export const NetworkStatusProto = bubbaloop.network_monitor.v1.NetworkStatus;
export const HealthCheckProto = bubbaloop.network_monitor.v1.HealthCheck;
export const CheckType = bubbaloop.network_monitor.v1.CheckType;
export const CheckStatus = bubbaloop.network_monitor.v1.CheckStatus;
export const SummaryProto = bubbaloop.network_monitor.v1.Summary;

// TypeScript interfaces for convenience
export interface Header {
  acqTime: bigint;
  pubTime: bigint;
  sequence: number;
  frameId: string;
  machineId: string;
  scope: string;
}

export interface HealthCheck {
  name: string;
  type: number;
  typeName: string;
  target: string;
  status: number;
  statusName: string;
  latencyMs: number;
  statusCode: number;
  resolved: string;
  error: string;
}

export interface Summary {
  total: number;
  healthy: number;
  unhealthy: number;
}

export interface NetworkStatus {
  header?: Header;
  checks: HealthCheck[];
  summary?: Summary;
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

// Map CheckType enum to string
function checkTypeToString(type: number): string {
  switch (type) {
    case 0: return 'HTTP';
    case 1: return 'DNS';
    case 2: return 'PING';
    default: return 'UNKNOWN';
  }
}

// Map CheckStatus enum to string
function checkStatusToString(status: number): string {
  switch (status) {
    case 0: return 'OK';
    case 1: return 'FAILED';
    case 2: return 'TIMEOUT';
    default: return 'UNKNOWN';
  }
}

// Decode Header
function decodeHeader(header: unknown): Header | undefined {
  if (!header || typeof header !== 'object') return undefined;
  const h = header as Record<string, unknown>;
  return {
    acqTime: toLongBigInt(h.acqTime as Long | number),
    pubTime: toLongBigInt(h.pubTime as Long | number),
    sequence: (h.sequence as number) ?? 0,
    frameId: (h.frameId as string) ?? '',
    machineId: (h.machineId as string) ?? '',
    scope: (h.scope as string) ?? '',
  };
}

// Decode HealthCheck
function decodeHealthCheck(check: unknown): HealthCheck | null {
  if (!check || typeof check !== 'object') return null;
  const c = check as Record<string, unknown>;
  const type = (c.type as number) ?? 0;
  const status = (c.status as number) ?? 0;
  return {
    name: (c.name as string) ?? '',
    type,
    typeName: checkTypeToString(type),
    target: (c.target as string) ?? '',
    status,
    statusName: checkStatusToString(status),
    latencyMs: (c.latencyMs as number) ?? 0,
    statusCode: (c.statusCode as number) ?? 0,
    resolved: (c.resolved as string) ?? '',
    error: (c.error as string) ?? '',
  };
}

// Decode Summary
function decodeSummary(summary: unknown): Summary | undefined {
  if (!summary || typeof summary !== 'object') return undefined;
  const s = summary as Record<string, unknown>;
  return {
    total: (s.total as number) ?? 0,
    healthy: (s.healthy as number) ?? 0,
    unhealthy: (s.unhealthy as number) ?? 0,
  };
}

// Decode NetworkStatus from Uint8Array
export function decodeNetworkStatus(data: Uint8Array): NetworkStatus | null {
  try {
    const message = NetworkStatusProto.decode(data);
    const checks: HealthCheck[] = [];
    for (const c of message.checks ?? []) {
      const check = decodeHealthCheck(c);
      if (check) checks.push(check);
    }
    return {
      header: decodeHeader(message.header),
      checks,
      summary: decodeSummary(message.summary),
    };
  } catch (error) {
    console.error('[Proto] Failed to decode NetworkStatus:', error);
    return null;
  }
}
