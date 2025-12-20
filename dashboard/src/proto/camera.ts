// Re-export compiled protobuf types and provide helper functions
import { bubbaloop } from './camera.pb.js';
import Long from 'long';

// Re-export the proto types
export const CompressedImageProto = bubbaloop.camera.v1.CompressedImage;
export const HeaderProto = bubbaloop.camera.v1.Header;

// TypeScript interfaces for convenience
export interface Header {
  acqTime: bigint;
  pubTime: bigint;
  sequence: number;
  frameId: string;
}

export interface CompressedImage {
  header?: Header;
  format: string;
  data: Uint8Array;
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

// Decode CompressedImage from Uint8Array
export function decodeCompressedImage(data: Uint8Array): CompressedImage {
  try {
    const message = CompressedImageProto.decode(data);

    const header: Header | undefined = message.header ? {
      acqTime: toLongBigInt(message.header.acqTime as Long | number),
      pubTime: toLongBigInt(message.header.pubTime as Long | number),
      sequence: message.header.sequence ?? 0,
      frameId: message.header.frameId ?? '',
    } : undefined;

    return {
      header,
      format: message.format ?? '',
      data: message.data ?? new Uint8Array(0),
    };
  } catch (error) {
    console.error('[Proto] Failed to decode CompressedImage:', error);
    return {
      format: '',
      data: new Uint8Array(0),
    };
  }
}
