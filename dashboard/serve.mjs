#!/usr/bin/env node
/**
 * Simple production server for the Bubbaloop dashboard.
 * Serves static files from dist/ and proxies /zenoh WebSocket to the Zenoh bridge.
 * Usage: node serve.mjs [--port 8080] [--bridge-port 10001]
 */
import http from 'node:http';
import fs from 'node:fs';
import path from 'node:path';
import net from 'node:net';
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

const server = http.createServer((req, res) => {
  res.setHeader('Access-Control-Allow-Origin', '*');
  const url = new URL(req.url, `http://${req.headers.host}`);
  const filePath = path.join(DIST, url.pathname === '/' ? 'index.html' : url.pathname);
  serveFile(res, filePath);
});

// WebSocket proxy: upgrade /zenoh to the Zenoh bridge
server.on('upgrade', (req, clientSocket, head) => {
  if (!req.url.startsWith('/zenoh')) {
    clientSocket.destroy();
    return;
  }

  const bridgeSocket = net.createConnection(BRIDGE_PORT, BRIDGE_HOST, () => {
    const upgradeReq =
      `${req.method} / HTTP/${req.httpVersion}\r\n` +
      Object.entries(req.headers)
        .filter(([k]) => k !== 'host')
        .map(([k, v]) => `${k}: ${v}`)
        .join('\r\n') +
      `\r\nHost: ${BRIDGE_HOST}:${BRIDGE_PORT}\r\n\r\n`;

    bridgeSocket.write(upgradeReq);
    if (head.length > 0) bridgeSocket.write(head);

    bridgeSocket.pipe(clientSocket);
    clientSocket.pipe(bridgeSocket);
  });

  bridgeSocket.on('error', () => clientSocket.destroy());
  clientSocket.on('error', () => bridgeSocket.destroy());
});

server.listen(PORT, '0.0.0.0', () => {
  console.log(`\n  Bubbaloop Dashboard Server\n`);
  console.log(`  Local:   http://localhost:${PORT}/`);
  for (const addrs of Object.values(os.networkInterfaces())) {
    for (const addr of addrs) {
      if (addr.family === 'IPv4' && !addr.internal) {
        console.log(`  Network: http://${addr.address}:${PORT}/`);
      }
    }
  }
  console.log(`  Bridge:  ws://${BRIDGE_HOST}:${BRIDGE_PORT} (proxied at /zenoh)\n`);
});
