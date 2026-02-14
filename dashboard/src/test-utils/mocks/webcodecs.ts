/**
 * Mock WebCodecs API for testing.
 *
 * jsdom doesn't implement WebCodecs (VideoDecoder, VideoFrame, EncodedVideoChunk).
 * These mocks simulate the browser APIs for testing CameraView and H264Decoder.
 */

import { vi } from 'vitest';

export class MockVideoFrame {
  readonly displayWidth: number;
  readonly displayHeight: number;
  readonly timestamp: number;
  closed = false;

  constructor(init: { displayWidth?: number; displayHeight?: number; timestamp?: number } = {}) {
    this.displayWidth = init.displayWidth ?? 1920;
    this.displayHeight = init.displayHeight ?? 1080;
    this.timestamp = init.timestamp ?? 0;
  }

  close(): void {
    this.closed = true;
  }
}

export class MockEncodedVideoChunk {
  readonly type: 'key' | 'delta';
  readonly timestamp: number;
  readonly data: ArrayBuffer;

  constructor(init: { type: 'key' | 'delta'; timestamp: number; data: ArrayBuffer }) {
    this.type = init.type;
    this.timestamp = init.timestamp;
    this.data = init.data;
  }
}

type VideoDecoderOutput = (frame: MockVideoFrame) => void;
type VideoDecoderError = (error: Error) => void;

export class MockVideoDecoder {
  static _instances: MockVideoDecoder[] = [];
  state: 'unconfigured' | 'configured' | 'closed' = 'unconfigured';
  private outputCallback: VideoDecoderOutput;
  private errorCallback: VideoDecoderError;
  decodedChunks: MockEncodedVideoChunk[] = [];
  configuredCodec: string | null = null;

  constructor(init: { output: VideoDecoderOutput; error: VideoDecoderError }) {
    this.outputCallback = init.output;
    this.errorCallback = init.error;
    MockVideoDecoder._instances.push(this);
  }

  static async isConfigSupported(config: { codec: string }): Promise<{ supported: boolean }> {
    return { supported: config.codec.startsWith('avc1') };
  }

  configure(config: { codec: string }): void {
    this.configuredCodec = config.codec;
    this.state = 'configured';
  }

  decode(chunk: MockEncodedVideoChunk): void {
    if (this.state !== 'configured') {
      this.errorCallback(new Error('Decoder not configured'));
      return;
    }
    this.decodedChunks.push(chunk);
    // Simulate async frame output
    queueMicrotask(() => {
      const frame = new MockVideoFrame({ timestamp: chunk.timestamp });
      this.outputCallback(frame);
    });
  }

  async flush(): Promise<void> {
    // no-op
  }

  close(): void {
    this.state = 'closed';
  }

  reset(): void {
    this.state = 'unconfigured';
    this.decodedChunks = [];
  }
}

/** Install WebCodecs mocks on globalThis */
export function installWebCodecsMocks(): void {
  MockVideoDecoder._instances = [];
  const g = globalThis as Record<string, unknown>;
  g.VideoDecoder = MockVideoDecoder;
  g.VideoFrame = MockVideoFrame;
  g.EncodedVideoChunk = MockEncodedVideoChunk;
}

/** Create mock canvas context that tracks drawImage calls */
export function createMockCanvasContext() {
  return {
    drawImage: vi.fn(),
    fillRect: vi.fn(),
    clearRect: vi.fn(),
    getImageData: vi.fn(() => ({ data: new Uint8ClampedArray(4) })),
    putImageData: vi.fn(),
    scale: vi.fn(),
    translate: vi.fn(),
    save: vi.fn(),
    restore: vi.fn(),
    beginPath: vi.fn(),
    closePath: vi.fn(),
    moveTo: vi.fn(),
    lineTo: vi.fn(),
    arc: vi.fn(),
    fill: vi.fn(),
    stroke: vi.fn(),
    measureText: vi.fn(() => ({ width: 0 })),
    fillText: vi.fn(),
    strokeText: vi.fn(),
    setTransform: vi.fn(),
    canvas: { width: 1920, height: 1080 },
  };
}
