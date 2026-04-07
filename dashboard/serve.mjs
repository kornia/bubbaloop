#!/usr/bin/env node
/**
 * Simple production server for the Bubbaloop dashboard.
 * Serves static files from dist/ and proxies /zenoh WebSocket to the Zenoh bridge.
 * Usage: node serve.mjs [--port 8080] [--bridge-port 10001]
 */
import http from 'node:http';
import https from 'node:https';
import fs from 'node:fs';
import path from 'node:path';
import os from 'node:os';
import { fileURLToPath } from 'node:url';

const __dirname = path.dirname(fileURLToPath(import.meta.url));
const DIST = path.join(__dirname, 'dist');

const args = process.argv.slice(2);
function getArg(name, def) {
  const i = args.indexOf(name);
  return i >= 0 && args[i + 1] ? args[i + 1] : def;
}
const PORT = parseInt(getArg('--port', '8080'), 10);
const BRIDGE_HOST = '127.0.0.1';
const BRIDGE_PORT = parseInt(getArg('--bridge-port', '10001'), 10);

const MIME = {
  '.html': 'text/html',
  '.js':   'application/javascript',
  '.css':  'text/css',
  '.json': 'application/json',
  '.wasm': 'application/wasm',
  '.svg':  'image/svg+xml',
  '.png':  'image/png',
  '.ico':  'image/x-icon',
};

function serveFile(res, filePath) {
  const ext = path.extname(filePath);
  const mime = MIME[ext] || 'application/octet-stream';

  fs.readFile(filePath, (err, data) => {
    if (err) {
      // SPA fallback: serve index.html for any non-file route
      fs.readFile(path.join(DIST, 'index.html'), (err2, html) => {
        if (err2) {
          res.writeHead(500);
          res.end('Internal error');
          return;
        }
        res.writeHead(200, { 'Content-Type': 'text/html' });
        res.end(html);
      });
      return;
    }
    res.writeHead(200, { 'Content-Type': mime });
    res.end(data);
  });
}

// Use HTTPS if certs exist and --no-tls is not set, otherwise plain HTTP
const certsDir = path.join(__dirname, 'certs');
const certPath = path.join(certsDir, 'cert.pem');
const keyPath = path.join(certsDir, 'key.pem');
const noTls = args.includes('--no-tls');
const useHttps = !noTls && fs.existsSync(certPath) && fs.existsSync(keyPath);

const handler = (req, res) => {
  res.setHeader('Access-Control-Allow-Origin', '*');
  const proto = useHttps ? 'https' : 'http';
  const url = new URL(req.url, `${proto}://${req.headers.host}`);
  const filePath = path.join(DIST, url.pathname === '/' ? 'index.html' : url.pathname);
  serveFile(res, filePath);
};

const server = useHttps
  ? https.createServer({ key: fs.readFileSync(keyPath), cert: fs.readFileSync(certPath) }, handler)
  : http.createServer(handler);

// WebSocket proxy: upgrade /zenoh to the Zenoh bridge
server.on('upgrade', (req, clientSocket, head) => {
  if (!req.url.startsWith('/zenoh')) {
    clientSocket.destroy();
    return;
  }

  // Use http.request to perform a proper HTTP upgrade to the bridge
  const proxyReq = http.request({
    hostname: BRIDGE_HOST,
    port: BRIDGE_PORT,
    path: '/',
    method: req.method,
    headers: {
      ...req.headers,
      host: `${BRIDGE_HOST}:${BRIDGE_PORT}`,
    },
  });

  proxyReq.on('upgrade', (proxyRes, bridgeSocket, bridgeHead) => {
    // Send the 101 response back to the client
    const resHeaders = [`HTTP/1.1 ${proxyRes.statusCode} ${proxyRes.statusMessage}`];
    for (const [k, v] of Object.entries(proxyRes.headers)) {
      resHeaders.push(`${k}: ${v}`);
    }
    clientSocket.write(resHeaders.join('\r\n') + '\r\n\r\n');
    if (bridgeHead.length > 0) clientSocket.write(bridgeHead);

    // Bi-directional pipe
    bridgeSocket.pipe(clientSocket);
    clientSocket.pipe(bridgeSocket);

    bridgeSocket.on('error', () => clientSocket.destroy());
    clientSocket.on('error', () => bridgeSocket.destroy());
  });

  proxyReq.on('error', () => clientSocket.destroy());

  if (head.length > 0) proxyReq.write(head);
  proxyReq.end();
});

server.listen(PORT, '127.0.0.1', () => {
  const proto = useHttps ? 'https' : 'http';
  console.log(`\n  Bubbaloop Dashboard Server ${useHttps ? '(HTTPS)' : '(HTTP)'}\n`);
  console.log(`  Local:   ${proto}://localhost:${PORT}/`);
  for (const addrs of Object.values(os.networkInterfaces())) {
    for (const addr of addrs) {
      if (addr.family === 'IPv4' && !addr.internal) {
        console.log(`  Network: ${proto}://${addr.address}:${PORT}/`);
      }
    }
  }
  console.log(`  Bridge:  ws://${BRIDGE_HOST}:${BRIDGE_PORT} (proxied at /zenoh)\n`);
});
