import { defineConfig, type Plugin } from 'vite';
import react from '@vitejs/plugin-react';
import wasm from 'vite-plugin-wasm';
import topLevelAwait from 'vite-plugin-top-level-await';
import basicSsl from '@vitejs/plugin-basic-ssl';

// Helper to check if error is a normal socket close (shared across handlers)
const isSocketCloseError = (err: Error & { code?: string }) => {
  const msg = err.message || '';
  const code = err.code || '';
  return (
    msg.includes('ended by the other party') ||
    msg.includes('socket hang up') ||
    msg.includes('write after end') ||
    msg.includes('ECONNRESET') ||
    msg.includes('ECONNREFUSED') ||
    msg.includes('EPIPE') ||
    msg.includes('read ECONNRESET') ||
    code === 'ECONNRESET' ||
    code === 'ECONNREFUSED' ||
    code === 'EPIPE' ||
    code === 'ERR_STREAM_WRITE_AFTER_END'
  );
};

// Check if output contains socket-related stack traces that should be suppressed
const isSocketStackTrace = (str: string) => {
  return (
    str.includes('TCP.onStreamRead') ||
    str.includes('Socket.emit') ||
    str.includes('addChunk') ||
    str.includes('readableAddChunkPushByteMode') ||
    str.includes('Readable.push') ||
    (str.includes('at ') && str.includes('node:internal/streams'))
  );
};

// Intercept stderr to filter socket-related noise (runs at module load time)
const originalStderrWrite = process.stderr.write.bind(process.stderr);
process.stderr.write = ((
  chunk: string | Uint8Array,
  encodingOrCallback?: BufferEncoding | ((err?: Error | null) => void),
  callback?: (err?: Error | null) => void
): boolean => {
  const str = typeof chunk === 'string' ? chunk : chunk.toString();
  if (isSocketStackTrace(str)) {
    // Suppress socket-related stack traces
    if (typeof encodingOrCallback === 'function') {
      encodingOrCallback();
    } else if (callback) {
      callback();
    }
    return true;
  }
  return originalStderrWrite(chunk, encodingOrCallback as BufferEncoding, callback);
}) as typeof process.stderr.write;

// Plugin to handle uncaught socket errors gracefully
function socketErrorHandler(): Plugin {
  return {
    name: 'socket-error-handler',
    configureServer() {
      // Handle uncaught exceptions from socket errors
      process.on('uncaughtException', (err: Error & { code?: string }) => {
        if (isSocketCloseError(err)) {
          // Silently ignore socket close errors
          return;
        }
        console.error('[Server] Uncaught exception:', err.message);
      });

      // Handle unhandled promise rejections from socket errors
      process.on('unhandledRejection', (reason: unknown) => {
        const err = reason as Error & { code?: string };
        if (err && isSocketCloseError(err)) {
          // Silently ignore socket close errors
          return;
        }
        console.error('[Server] Unhandled rejection:', reason);
      });
    },
  };
}

export default defineConfig({
  plugins: [
    socketErrorHandler(),
    wasm(),
    topLevelAwait(),
    react(),
    basicSsl(), // Self-signed cert for HTTPS
  ],
  server: {
    port: 5173,
    host: true,
    // Proxy through Vite for cross-machine access
    proxy: {
      // Zenoh WebSocket proxy
      '/zenoh': {
        target: 'ws://127.0.0.1:10001',
        ws: true,
        rewriteWsOrigin: true,
        // Suppress noisy socket errors on connection close
        configure: (proxy) => {
          proxy.on('error', (err: Error & { code?: string }, _req, res) => {
            // Silently ignore socket close errors - these happen when browser/tab closes
            if (isSocketCloseError(err)) return;
            console.error('[Proxy] Error:', err.message);
            // Try to end the response gracefully
            if (res && 'writeHead' in res && !res.headersSent) {
              res.writeHead(502, { 'Content-Type': 'text/plain' });
              res.end('Proxy error');
            }
          });

          // Handle client socket errors (browser side)
          proxy.on('proxyReqWs', (_proxyReq, _req, socket) => {
            socket.on('error', (err: Error & { code?: string }) => {
              if (isSocketCloseError(err)) return;
              console.error('[Proxy WS Client] Error:', err.message);
            });
          });

          // Handle target socket errors (Zenoh side) - this is the key one!
          // eslint-disable-next-line @typescript-eslint/no-explicit-any
          (proxy as any).on('open', (proxySocket: any) => {
            proxySocket.on('error', (err: Error & { code?: string }) => {
              if (isSocketCloseError(err)) return;
              console.error('[Proxy WS Target] Error:', err.message);
            });
          });

          // Handle the WebSocket close event
          // eslint-disable-next-line @typescript-eslint/no-explicit-any
          (proxy as any).on('close', (_res: any, socket: any, _head: any) => {
            // Ensure socket errors during close are handled
            if (socket && !socket.destroyed) {
              socket.on('error', () => {
                // Silently ignore - socket is closing
              });
            }
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
    rollupOptions: {
      onwarn(warning, defaultHandler) {
        // Suppress eval warning from protobufjs (third-party, cannot fix)
        if (warning.code === 'EVAL' && warning.id?.includes('@protobufjs')) return;
        defaultHandler(warning);
      },
      output: {
        manualChunks: {
          'vendor-react': ['react', 'react-dom'],
          'vendor-zenoh': ['@eclipse-zenoh/zenoh-ts', 'channel-ts'],
          'vendor-proto': ['protobufjs/minimal', 'long'],
          'vendor-dnd': ['@dnd-kit/core', '@dnd-kit/sortable'],
          'vendor-json-view': ['react18-json-view'],
        },
      },
    },
  },
});
