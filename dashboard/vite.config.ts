import { defineConfig, createLogger } from 'vite';
import react from '@vitejs/plugin-react';
import wasm from 'vite-plugin-wasm';
import topLevelAwait from 'vite-plugin-top-level-await';
import basicSsl from '@vitejs/plugin-basic-ssl';

// Custom logger that filters out noisy proxy errors
const logger = createLogger();
const originalError = logger.error.bind(logger);
logger.error = (msg, options) => {
  // Filter out WebSocket proxy errors (bridge not running, connection closed, etc.)
  if (
    msg.includes('ws proxy error') &&
    (msg.includes('ECONNREFUSED') ||
     msg.includes('ECONNRESET') ||
     msg.includes('EPIPE') ||
     msg.includes('socket hang up'))
  ) {
    return; // Silently ignore - frontend shows connection status
  }
  originalError(msg, options);
};

export default defineConfig({
  customLogger: logger,
  plugins: [
    wasm(),
    topLevelAwait(),
    react(),
    basicSsl(), // Self-signed cert for HTTPS
  ],
  server: {
    port: 5173,
    host: true,
    // Proxy Zenoh WebSocket through Vite - no separate WSS port needed
    proxy: {
      '/zenoh': {
        target: 'ws://127.0.0.1:10000',
        ws: true,
        rewriteWsOrigin: true,
      },
    },
  },
  optimizeDeps: {
    // Force pre-bundle these CommonJS dependencies
    include: ['channel-ts', '@eclipse-zenoh/zenoh-ts'],
    esbuildOptions: {
      // Handle CommonJS modules
      mainFields: ['module', 'main'],
    },
  },
  build: {
    target: 'esnext',
    commonjsOptions: {
      // Transform CommonJS to ESM
      transformMixedEsModules: true,
    },
  },
});
