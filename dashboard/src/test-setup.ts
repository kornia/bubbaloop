import '@testing-library/jest-dom';
import 'vitest-canvas-mock';
import { installWebCodecsMocks } from './test-utils/mocks/webcodecs';

// Install WebCodecs mocks (VideoDecoder, VideoFrame, EncodedVideoChunk)
installWebCodecsMocks();

// Mock ResizeObserver (used by MeshView and other components)
class MockResizeObserver {
  callback: ResizeObserverCallback;
  constructor(callback: ResizeObserverCallback) {
    this.callback = callback;
  }
  observe(): void {}
  unobserve(): void {}
  disconnect(): void {}
}

globalThis.ResizeObserver = MockResizeObserver as unknown as typeof ResizeObserver;
