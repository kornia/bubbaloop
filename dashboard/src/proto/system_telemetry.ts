// Re-export compiled protobuf types and provide helper functions
import { bubbaloop } from './messages.pb.js';
import Long from 'long';

// Re-export the proto types
export const SystemMetricsProto = bubbaloop.system_telemetry.v1.SystemMetrics;
export const CpuMetricsProto = bubbaloop.system_telemetry.v1.CpuMetrics;
export const MemoryMetricsProto = bubbaloop.system_telemetry.v1.MemoryMetrics;
export const DiskMetricsProto = bubbaloop.system_telemetry.v1.DiskMetrics;
export const NetworkMetricsProto = bubbaloop.system_telemetry.v1.NetworkMetrics;
export const LoadMetricsProto = bubbaloop.system_telemetry.v1.LoadMetrics;

// TypeScript interfaces for convenience
export interface Header {
  acqTime: bigint;
  pubTime: bigint;
  sequence: number;
  frameId: string;
  machineId: string;
  scope: string;
}

export interface CpuMetrics {
  usagePercent: number;
  count: number;
  perCore: number[];
}

export interface MemoryMetrics {
  totalBytes: bigint;
  usedBytes: bigint;
  availableBytes: bigint;
  usagePercent: number;
}

export interface DiskMetrics {
  totalBytes: bigint;
  usedBytes: bigint;
  availableBytes: bigint;
  usagePercent: number;
}

export interface NetworkMetrics {
  bytesSent: bigint;
  bytesRecv: bigint;
}

export interface LoadMetrics {
  oneMin: number;
  fiveMin: number;
  fifteenMin: number;
}

export interface SystemMetrics {
  header?: Header;
  cpu?: CpuMetrics;
  memory?: MemoryMetrics;
  disk?: DiskMetrics;
  network?: NetworkMetrics;
  load?: LoadMetrics;
  uptimeSecs: bigint;
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

// Decode CpuMetrics
function decodeCpuMetrics(cpu: unknown): CpuMetrics | undefined {
  if (!cpu || typeof cpu !== 'object') return undefined;
  const c = cpu as Record<string, unknown>;
  return {
    usagePercent: (c.usagePercent as number) ?? 0,
    count: (c.count as number) ?? 0,
    perCore: (c.perCore as number[]) ?? [],
  };
}

// Decode MemoryMetrics
function decodeMemoryMetrics(memory: unknown): MemoryMetrics | undefined {
  if (!memory || typeof memory !== 'object') return undefined;
  const m = memory as Record<string, unknown>;
  return {
    totalBytes: toLongBigInt(m.totalBytes as Long | number),
    usedBytes: toLongBigInt(m.usedBytes as Long | number),
    availableBytes: toLongBigInt(m.availableBytes as Long | number),
    usagePercent: (m.usagePercent as number) ?? 0,
  };
}

// Decode DiskMetrics
function decodeDiskMetrics(disk: unknown): DiskMetrics | undefined {
  if (!disk || typeof disk !== 'object') return undefined;
  const d = disk as Record<string, unknown>;
  return {
    totalBytes: toLongBigInt(d.totalBytes as Long | number),
    usedBytes: toLongBigInt(d.usedBytes as Long | number),
    availableBytes: toLongBigInt(d.availableBytes as Long | number),
    usagePercent: (d.usagePercent as number) ?? 0,
  };
}

// Decode NetworkMetrics
function decodeNetworkMetrics(network: unknown): NetworkMetrics | undefined {
  if (!network || typeof network !== 'object') return undefined;
  const n = network as Record<string, unknown>;
  return {
    bytesSent: toLongBigInt(n.bytesSent as Long | number),
    bytesRecv: toLongBigInt(n.bytesRecv as Long | number),
  };
}

// Decode LoadMetrics
function decodeLoadMetrics(load: unknown): LoadMetrics | undefined {
  if (!load || typeof load !== 'object') return undefined;
  const l = load as Record<string, unknown>;
  return {
    oneMin: (l.oneMin as number) ?? 0,
    fiveMin: (l.fiveMin as number) ?? 0,
    fifteenMin: (l.fifteenMin as number) ?? 0,
  };
}

// Decode SystemMetrics from Uint8Array
export function decodeSystemMetrics(data: Uint8Array): SystemMetrics | null {
  try {
    const message = SystemMetricsProto.decode(data);
    return {
      header: decodeHeader(message.header),
      cpu: decodeCpuMetrics(message.cpu),
      memory: decodeMemoryMetrics(message.memory),
      disk: decodeDiskMetrics(message.disk),
      network: decodeNetworkMetrics(message.network),
      load: decodeLoadMetrics(message.load),
      uptimeSecs: toLongBigInt(message.uptimeSecs as Long | number),
    };
  } catch (error) {
    console.error('[Proto] Failed to decode SystemMetrics:', error);
    return null;
  }
}
