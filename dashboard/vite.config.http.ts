import { defineConfig, mergeConfig } from 'vite';
import baseConfig from './vite.config';

// HTTP-only config for LAN/mobile access (no self-signed cert issues)
export default mergeConfig(baseConfig, defineConfig({
  plugins: [],
  server: {
    port: 5174,
    https: false,
  },
}));
