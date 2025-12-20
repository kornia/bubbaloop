import { defineConfig } from 'vite';
import react from '@vitejs/plugin-react';
import wasm from 'vite-plugin-wasm';
import topLevelAwait from 'vite-plugin-top-level-await';

export default defineConfig({
  plugins: [
    wasm(),
    topLevelAwait(),
    react(),
  ],
  server: {
    port: 5173,
    host: true,
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
