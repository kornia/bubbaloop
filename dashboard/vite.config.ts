import { defineConfig } from 'vite';
import react from '@vitejs/plugin-react';
import wasm from 'vite-plugin-wasm';
import topLevelAwait from 'vite-plugin-top-level-await';
import basicSsl from '@vitejs/plugin-basic-ssl';

export default defineConfig({
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
        // Suppress noisy socket errors on connection close
        configure: (proxy) => {
          proxy.on('error', (err) => {
            // Silently ignore "socket ended" errors - these happen on reconnect
            if (err.message?.includes('ended by the other party')) return;
            console.error('[Proxy] Error:', err.message);
          });
        },
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
