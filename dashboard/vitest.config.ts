import { defineConfig } from 'vitest/config';
import react from '@vitejs/plugin-react';
import path from 'path';

export default defineConfig({
  plugins: [react()],
  test: {
    environment: 'jsdom',
    setupFiles: ['./src/test-setup.ts'],
    globals: true,
    alias: {
      '@eclipse-zenoh/zenoh-ts': path.resolve(__dirname, 'src/test-utils/mocks/zenoh-ts-mock.ts'),
    },
  },
});
