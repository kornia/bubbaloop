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
